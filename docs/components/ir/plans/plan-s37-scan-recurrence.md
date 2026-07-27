# plan-s37 — the recurrence primitive: one `Scan`, three parallel classes

Status: **PLAN, not started.** Component: `mapal-ir` (the op and the classification), with
consequences in `path_plan`, the LLVM emitter and `mapal-rt`. Raised by Sapir:

> "Assuming fold f, accumulator a, array a₀…aₙ … so f is a scan on a, a₀…aₙ. Maybe we can
> approach this more generally: a scan function with init value (acc), window (slice, default 1
> like fold), jump (default 1 like fold). This sounds more parallelizable."

## 0. The intuition first

Three loops, and the only thing that matters is which of them can be cut in half and handed to two
people.

```
FOLD          acc = f(acc, x[i])           each step needs the PREVIOUS answer
WINDOW/FIR    y[i] = g(x[i..i+k])          each step needs only the INPUT
SCAN          acc = f(acc, x[i]); y[i]=acc both: carries, and writes every step
```

Cut the middle one anywhere you like — the halves never speak to each other. That is why `fir` and
`conv2d` already parallelise and already beat C++ by 8×: **no step depends on another step's
output.**

Cut the first one and you have a problem, because the second half needs the first half's answer to
start. Unless — and this is the whole trick — you can start it with the *wrong* value and fix it
afterwards. If `f` is `+`, thread B can sum its own half from zero, thread A can sum its half from
the real start, and adding the two totals at the end gives the same answer. That works because
`(a+b)+(c+d) = ((a+b)+c)+d`. It does **not** work for `f = λ(acc,x). acc*2 + x`, where starting from
the wrong place scales everything by the wrong power of two.

So the question "can this loop be split" is really the question "**is there an operator that lets me
join two independently-computed halves**". That operator is the missing thing, and finding it — not
proving a property of `f` — is what the compiler has to do.

The reason to unify all three under one op is that today the compiler learns this fact **three
separate times, in three separate places, and gets it wrong twice**: `Map` is split because it
obviously can be, `Fold` is sequential because nobody looked, and `fir`'s window only parallelises
because it is written as a map-of-fold and the *outer* map is what gets sliced. One primitive that
carries `window`, `jump` and `carry` says all of it in the signature.

## 1. Where we are (the measurement that raised this)

`benches/results-s36/LANGUAGES.md`, pinned i9, reduce 2²⁰ f32:

| leg | ms | what it computes |
| --- | ---: | --- |
| Mapal 1t | **0.3668** | strict left fold |
| C++ 1t | 0.3821 | strict left fold |
| Rust 1t | 0.3821 | strict left fold |
| Mapal par | 1.9799 | **the same left fold, on one lane, paying pool cost** |
| Rust mt | 0.1410 | 16 chunk-sums, then a sum of chunks — **a different function** |
| NumPy | 0.1519 | pairwise summation — **a different function** |

At one thread, where all three compute the same thing, **Mapal is the fastest**. The "14× behind"
cell is a semantics gap wearing a performance gap's clothes. `Operation::Fold` becomes
`TaskKind::Seq { morphisms: vec![m] }` (`crates/mapal-ir/src/algo.rs:1221`) — one lane, always —
while `Map` in the arm directly above becomes `TaskKind::Split`. The emitted kernel is a single
`vaddss` dependency chain at exactly 2.00 cycles/element: FADD latency, nothing else.

## 2. Categorical model (FRAMEWORK §2)

**Why a category buys something here.** A parallel reduction is not "a fold that got faster" — it is
a **monoid homomorphism**, and once it is written that way the legality condition stops being a
judgement call and becomes a diagram that either commutes or does not.

### 2.1 The objects

| Object | Meaning |
| --- | --- |
| `Elem` | the element type of the scanned array |
| `Acc` | the accumulator type — **not** necessarily `Elem` |
| `Window` | `Elem*` of fixed length `w` (the free monoid, truncated) |
| `Rec` | a recurrence: the thing this plan makes first-class |

### 2.2 The morphism table

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `init` | `Rec → Acc` | Total | the seed (`fold`'s accumulator argument) |
| `step` | `Rec → (Acc × Window → Acc)` | Total | the body |
| `window` | `Rec → ℕ` | Total | elements visible per step (default 1) |
| `jump` | `Rec → ℕ` | Total | index advance per step (default 1) |
| `emit?` | `Rec → 𝔹` | Total | keep every intermediate (`scan`) or only the last (`fold`) |
| `combine?` | `Rec → (Acc × Acc → Acc)` | **Partial** | the join operator — *present iff the recurrence is splittable* |
| `unit?` | `Rec → Acc` | **Partial** | `combine`'s identity — required with `combine` |

`combine?` is the entire plan. Everything else is bookkeeping around it.

### 2.3 The three shapes, as fibres of one object

| Shape | `window` | `jump` | `emit` | `combine?` | today |
| --- | ---: | ---: | --- | --- | --- |
| `Map` | 1 | 1 | all | *no carry at all* | `TaskKind::Split` ✅ |
| `Fold` | 1 | 1 | last | **absent** | `TaskKind::Seq` ❌ |
| `Scan` (prefix) | 1 | 1 | all | absent | does not exist |
| `Window`/FIR | k | 1 | all | *no carry at all* | map-of-fold, tiled ✅ |
| `Strided`/pool | k | k | all | *no carry at all* | not expressible |

Read the table as the §3 Consolidation move: `Fold` is not a different kind of thing from `Map`,
it is **the same object with a carried accumulator**, and `Window` is the same object with `w > 1`.
Three ops become one object with partial morphisms, exactly as §3 prescribes.

### 2.4 The law that makes splitting legal

Write `⊕` for `combine`, `e` for `unit`, `g` for the per-element contribution. A recurrence is
**splittable** iff `step` factors as

```
step(acc, x) = acc ⊕ g(x)          with  (Acc, ⊕, e)  a monoid
```

i.e. `⊕` associative and `e ⊕ a = a ⊕ e = a`. Then for any partition of the input,

```
foldl step init xs  =  init ⊕ (⊕ over chunk₁) ⊕ (⊕ over chunk₂) ⊕ …
```

and **any** bracketing gives the same answer. That is the whole theorem. Sapir's condition
`f(f(a,b), f(c,d)) = f(f(f(a,b),c),d)` is this law specialised to `Acc = Elem` and `g = id` — right,
and the general form is needed because a fold's accumulator usually is *not* the element type
(`Σ f32` into `f64`, min-and-argmin, a histogram bucket array).

**Composition rules the implementation must preserve:**

1. `combine?(r) ≠ ∅ ⟹ step(a, x) = a ⊕ g(x)` for the recognised `g` — checked structurally, not
   assumed.
2. `combine?(r) ≠ ∅ ⟹ unit?(r) ≠ ∅`. A chunk that gets zero elements must have an answer.
3. **The tree shape is a function of the static `n` alone** — never of `pool.threads`,
   never of `MAPAL_SLICE`. See §5; this is the one that bites.
4. `window > 1 ∧ carry ⟹` the windows a chunk boundary straddles belong to exactly one chunk
   (the FIR halo rule, already solved for the tiled window rung).
5. `emit = all ∧ combine? ≠ ∅ ⟹` the two-pass form (up-sweep of chunk totals, then down-sweep with
   each chunk's exclusive prefix as its seed). Blelloch, but written as: *run each chunk from `e`,
   then fix it up*.

### 2.5 What `combine` is for a body the compiler can actually see

| body shape | `⊕` | `e` | deducible? |
| --- | --- | --- | --- |
| `acc + x` (ℤ) | `+` | `0` | **yes, structurally** |
| `acc * x` (ℤ) | `*` | `1` | **yes** |
| `min/max(acc, x)` | same | `±∞` | **yes** |
| `acc and/or/xor x` | same | identity | **yes** |
| `acc + x` (ℝ) | `+` | `0` | **not without permission** — §4 |
| `acc * 2 + x` (Horner) | matrix/affine compose | identity affine | possible, out of scope |
| `f(acc, x)` opaque call | — | — | **no** — stays sequential, and says so |

The recogniser is a match on the body's morphism graph, not a theorem prover: one arithmetic
morphism from a closed set, one operand the carried accumulator, no traps, no effects. If it does
not match, `combine? = ∅` and the recurrence is sequential — which is a *correct answer*, not a
failure.

## 3. Why this is not "make Fold splittable"

`TaskKind` has two variants and both are 1-to-1 on outputs. `Split`'s soundness argument is
literally that *slices write disjoint element ranges*
(`plan-parallel-orchestrator.md` §2 rule 2), and `path_plan` asserts one producer per object
(`algo.rs:1347`, `debug_assert!(old.is_none_or(|id| id == task_id))`). Every slice of a split fold
writes **the same scalar**. So this is not a wider `Split`; it is a third task shape:

| needs | where | why |
| --- | --- | --- |
| per-lane partial storage | frame layout (`func.rs:build_frame_layout`) | each chunk needs its own `Acc` cell |
| a merge morphism | the plan | `⊕` over the partials, in a **pinned order** |
| a completion hook | `mapal-rt` `complete_slice` | today it only decrements `slices_left` and unlocks dependents; nothing runs at "all slices done" |

That third shape is the same machinery a windowed scan needs, which is the argument for building it
once, under one op, rather than as a fold patch.

## 4. Floats: a permission, not a deduction

Reassociating `+` on `f32` changes the answer. Three facts, all measured or already decided:

- **It does not cost accuracy — it buys it.** Measured against `math.fsum` over 2²⁰ f32:
  left fold rel-err `1.46e-5` vs 16-chunk tree `8.96e-7` (**16× better**); with a `+1e4` offset,
  `1.36e-2` vs `3.15e-4` (**43× better**). The tree is closer to the true sum, not further.
- **What it costs is reproducibility** — the answer stops being a function of the source alone,
  which is what R-PAR/L1 and ADR-0032's "provably the same function, plug-play backends" promise.
  That is a much better reason than "precision", and the docs currently imply the weaker one.
- **The decision is already written down and unimplemented.** ADR-0032 D1 defines the lattice
  `exact | contract | tf32-class` and states outright that *reassociation permission rides the same
  lattice, `exact` forbids it*. `crates/mapal-ir/src/ty.rs` has no contract dimension, so **today a
  program has no way to grant it.** ADR-0028 ("tree reduction for exact-op folds") is accepted, its
  operator set is integer/bool only (D1), it pins float folds sequential by construction (D4), and
  `grep` finds no recogniser — prose only.

So the ladder is: **integers split by deduction today; floats split only when the type carries the
permission.** That is Sapir's own standing rule — *type system = precision contracts; backend config
= performance tailors* — landing exactly where it was always going to land.

## 5. The trap: deducing the slice COUNT breaks determinism

The hand-written baselines have this bug and it is worth naming, because "deduce what we did by
hand" would inherit it. `ladder2_baseline.rs:run_reduce` splits into `thread_width()` chunks — **its
f32 answer is a function of how many cores the machine has.** Ours would be worse: `slice_ranges`
(`mapal-rt/src/lib.rs:209`) derives the slice count from `pool.threads`, and `MAPAL_SLICE` can
override it, and work stealing lets any lane help.

The fix is composition rule 3, and it is already the house rule: **compile time decides the sizes,
runtime decides the assignment.** Pin the chunk count from the static `n` at emission. The runtime
then chooses only *who* runs each chunk — which is scheduling, and scheduling may vary freely
because the tree shape does not. A 4-core box and a 64-core box return bit-identical answers.

## 6. Staging

| # | Step | Gate |
| --- | --- | --- |
| 1 | `combine?`/`unit?` recognition in mapal-ir over the exact-op set (ADR-0028 D1), integers only | the query returns `Some(⊕)` for `Σ i32` and `∅` for `Σ f32` and for an opaque call |
| 2 | The third `TaskKind` + per-lane partials + the merge hook | an i32 reduce splits; oracle-equal; the canonical tree is stable under `MAPAL_PAR` ∈ {1,2,4,16} and under `MAPAL_SLICE` |
| 3 | `Ty` gains the ADR-0032 contract dimension; `contract` grants float reassociation | an f32 reduce under `exact` stays sequential; under `contract` splits and is oracle-equal *to the tree oracle*, and the interpreter learns the same tree |
| 4 | Generalise to `window`/`jump`, unify the FIR rung under it | `fir` emission byte-identical before and after the unification (this is a refactor, not a speedup) |
| 5 | `emit = all` — prefix scan, two-pass | a running-sum shape lands on the ladder |

Steps 1–2 are worth doing on their own: **an i32 reduce is fully deducible today and needs no new
type machinery at all.**

## 7. What this does NOT fix, and must not be sold as fixing

The reduce cell was never the real gap on the non-compute shapes. Two measured findings from the
same investigation dwarf it, and both are self-inflicted memory-layout facts rather than missing
algebra:

- **The `%Frame` struct destroys LLVM's alias analysis.** The emitter *proves* disjointness in
  `build_frame_layout` and then emits every object as a field of one struct, so LAA cannot separate
  them: `loop not vectorized: unsafe dependent memory operations`. Emitting the fact the backend
  already has is worth **2.3× on saxpy 1t** (0.5253 → 0.2262 ms, identical output).
- **`iota` is materialised as an array instead of being an index law**, so every map over an iota
  loads its own index out of memory. Worth **3.1×** (0.3043 → 0.0972 ms).

Together they put saxpy 1t at **0.0972 vs C++ 0.0945** — parity — with *zero* mapal-ir changes and
nothing to do with this plan. Sequence accordingly: those two first, this second.

## 8. The same type change unlocks FMA — and FMA should become an op, not a flag

Raised by Sapir: *"by default compilers do the fma and we don't, why? Maybe this should be the
default? Or should/can it be deduced at mapal ir?"*

**It cannot be deduced, for the same reason reassociation cannot.** Fusing `(a*b)+c` changes the
value; nothing in the dataflow graph says whether the program wants one rounding or two. It is a
permission on the same ADR-0032 D1 lattice — `exact | contract | tf32-class` — and it is
unrepresentable today for the same reason: `Ty` carries no contract dimension. **One type change
answers both questions.**

**The premise deserves checking, and it is only half true.** C and C++ contract by default; **Rust
does not** — `rustc -O -C target-cpu=native` produces zero FMA instructions on our own matmul
baselines, against two per C++ leg (measured on the objects). NumPy's FMA comes from hand-written
BLAS kernels, not a compiler default. Mapal's `exact` default puts it where Rust is, which is a
defensible place, not an oversight.

**Why the default cannot simply be flipped today.** Contraction is currently a *flag*:
`EmitOpts::contract` attaches a `contract` fast-math flag to the emitted `fmul`/`fadd` and **LLVM
then decides which pairs actually fuse**. There is no `Fma` in `graph.rs` and no `mul_add` in
`mapal-interp`. So the oracle cannot predict the fusion set, and the differential suite's
byte-for-byte `assert_eq!` against the interpreter — 1,280 compile-and-runs per push — would have
to weaken to a tolerance. That is trading the project's strongest correctness property for 1.6×.

**The fix is the same move this plan makes for `combine`: put the decision in the graph.**

| step | change | why it matters |
| --- | --- | --- |
| 1 | `Operation::Fma` in Core; `lower` emits it for `a*b+c` **when the contract permits** | the fusion set becomes a graph fact, not an LLVM outcome |
| 2 | `mapal-interp` evaluates it with `f32::mul_add` | IEEE-guaranteed single rounding ⟹ bit-identical to the hardware instruction, so byte-equality with the oracle **survives** |
| 3 | `Ty` gains the ADR-0032 D1 contract dimension | `exact` forbids the lowering; `contract` permits it — per type, not per compiler invocation |
| 4 | default becomes `contract` | naive code gets the fast build; `exact` stays reachable for anyone who needs unfused left-to-right semantics |

Step 2 is the load-bearing one and it is why "make it an op" beats "flip the flag": a permission
expressed as an op is testable against the oracle, a permission expressed as a backend flag is not.

## 9. Open questions for Sapir

1. **Does `Scan` replace `Fold` in Core, or join it?** Consolidation (§3) says replace — `Fold` is
   `Scan` with `window=1, jump=1, emit=last`. That is a Core op change with an ADR and a migration.
2. **Is `combine` ever written by the programmer**, or only ever deduced? A `fold` with an explicit
   `combine` clause is honest and needs no recogniser, but it puts the associativity burden on the
   user with no checker.
3. **Does the interpreter oracle learn the tree?** If a split fold is oracle-equal only to a
   *tree* oracle, then `mapal-interp` has to compute the same canonical tree — otherwise R-PAR
   compares two different functions and every differential test on a float reduce goes red.
