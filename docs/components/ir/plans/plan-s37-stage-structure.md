# plan-s37 — `elem_plan`: what `out[i]` is, as a graph fact

Status: **PLANNED** — not started. Component: `ir` (the query) · consumers: `backend-llvm`, `backend-cuda`.
Supersedes: `docs/components/rewrite/plans/plan-s37-stage-composition.md` (same session, weaker design —
four match arms normalising into "Map over an Iota", as a destructive rewrite).
Also supersedes the framing of S36c §3(b) and `next-session.md` §2's second bullet.
Related: `plan-s37-scan-recurrence.md` (the carry-bearing sibling), ADR-0018 (`Zip`/`Enumerate`),
ADR-0027 (capture scoping), ADR-0029 (`Iota`/`Fill`), ADR-0032 (backend genericity), S34's trap lesson.

---

## 1. The idea in one page

Sapir, opening the design:

> "In a graph we have object a, a fanout creates object b, a fanout creates object c — we can compose
> the fanout from a→b and b→c into a single fanout, skipping the b object."

and sharpening it:

> "zip is the same logic as iota. It shouldn't be restricted to a single node type, it should be a
> generic notation of a graph 'fanout' with a structure. e.g. iota does the +1 structure, zip does the
> map-combine-to-tuple structure, map does the function-on-each-element structure."

**The verdict: right, with three precisions.** Write the element laws out with their targets and the
consolidation proves itself:

| stage | `out[i] =` | arrays read at `i` | carries a body? |
| --- | --- | ---: | --- |
| `Iota` | `i` | 0 | no |
| `Fill` | `x` (a capture) | 0 | no |
| `Zip` | `(a[i], b[i])` | 2 | no |
| `Enumerate` | `(i, a[i])` | 1 | no |
| `Map{f}` | `f(a[i])` | 1 | **yes** (`FuncId`) |

Same target object, same shape of law. FRAMEWORK §3's criterion for "one object with extra structure"
is met — so it is one notion, and the composition is one law, not five match arms.

The thing being removed has a name in the framework: **an intermediate array is a stored copy of a
deduced morphism** (§5, deduce-don't-store). It is paid for twice — an N-element store pass to build
it plus its frame footprint, then an N-element load pass in the consumer — and worse, it is an opaque
memory object, so `-Rpass-analysis=loop-vectorize` reports `cannot identify array bounds` and the
consuming loop stays scalar (S36c §3(b)).

### 1.1 Precision one — iota is a closed form, not a recurrence

"The +1 structure" has two readings and only one of them is true here:

- `out[i] = out[i-1] + 1` — a **recurrence**. Sequential unless a closed form is proven.
- `out[i] = i` — a **closed form**. Any element computable at any index, independently.

**Iota is the closed form.** `algo.rs:2471` already records it as `Rng::Int(0, n-1)` in the range
lattice. The literal `+1` visible in `emit_iota` (`func.rs:6704`) is the shared loop-counter
increment every emitter writes, not a data dependence.

This matters twice. It is exactly the property that makes substitution legal — the consumer at index
`i` can compute the producer's element without any other element existing. And it keeps `Iota` out of
`plan-s37-scan-recurrence`'s `combine?`/`unit?` splittability machinery: a recurrence reading would
owe a monoid proof that the closed form gets for free. That plan's R5 ("do not smuggle it in") cuts
in both directions.

### 1.2 Precision two — `Map` is not a peer of `iota` and `zip`

`Zip`, `Enumerate`, `Iota`, `Fill` are **bodyless** — bare unit variants in `graph.rs:114-129`, no
fields at all. The op tag *is* the entire formula; zero degrees of freedom. `Map { body: FuncId,
captures: u32 }` and `Fold { body: FuncId, captures: u32 }` carry an arbitrary body, and in this
repo's own benchmarks those bodies read:

- **transpose**: `a[(t % 1024) * 1024 + t / 1024]` — a computed, permuted address
- **gather**: `x[j]` where `j` is loaded data — data-dependent
- **conv2d**: two arrays, nine times each, at addresses depending on the outer *and* inner index

So "map does the function-on-each-element structure" is true as a *type* and empty as a *fact* — the
function is opaque. **The producer family is the four bodyless ops.** `Map` joins unconditionally as
a **consumer** (the producer's law is substituted *into* its body; the body never needs classifying)
and as a **producer** only behind a classifiability gate, which ships as its own later step.

### 1.3 Precision three — the mapping is a fact, not a firing

Per Sapir's own layering (§2): "we can compose a→b→c skipping b" is a **legality fact** mapal-ir
records unconditionally. *Whether* to skip `b` is per-target and belongs to the backend.

### 1.4 What the repo's own comments already say

Three findings from `graph.rs`, none of which needed inventing:

- **`Zip` is documented as "the canonical iso `Aⁿ × Bⁿ ≅ (A×B)ⁿ` (ADR-0018)."** An isomorphism.
  saxpy materialises 8 MB of `[1048576 × {float,float}]` to realise an iso. That is the entire
  justification for the `Zip` case, written in the ADR before this plan existed.
- **`Enumerate : [A;n] → [(i32,A);n]` is therefore `Zip ∘ Iota`** — it needs no constructor of its
  own. One fewer member of the family, by the same consolidation.
- **`Fill`'s count is already "type-carried by the target `[T; n]` (deduced, not stored — no count
  operand)."** Deduce-don't-store, already applied inside the op. Precedent, not innovation.

---

## 2. The layering — three questions, three owners

Sapir's directive, and the architecture this plan obeys:

| # | question | owner | why there |
| --- | --- | --- | --- |
| 1 | **Is it legal?** trap ordering, divergence, partiality, does the composed body denote the same morphism | **mapal-ir** | machine-independent value semantics; this is what the byte-equality oracle protects |
| 2 | **Keep the intermediate in memory, or recompute?** | **backend** | opposite answers per target — GPU is bandwidth-bound with cheap registers so recompute wins; CPU weighs body cycles against an L2 round trip; FPGA's "store" is BRAM and "recompute" is silicon area |
| 3 | **Does combining blow the cache / SMEM / occupancy / register budget?** | **backend** | same reason |

So **two tables in two places** (§5, §6), never one. And a corollary that is an instruction: the
mapal-ir query carries **no cost model** — no op counts, no read counts. The backend already has the
graph and can count its own operations.

**The precedent is shipped, twice.** `tile_plan` (`algo.rs:564`) records only geometry and proves
bit-exact interleavability; `TargetProfile` (`backends/llvm/src/profile.rs`) holds the machine
constants and derives `tile_i = vec_regs/(2×acc_vecs_per_row)`, the KC gate, TJ per width. The
developer wrote matmul as a plain `map + fold` that reads like an ordinary loop nest, and neither half
knows the other's business. Likewise `last_use_plan` + the emitter-local `elided_updates`
(`func.rs:1237`). This plan is the third instance of one pattern, not a new architecture.

### 2.1 Query, not rewrite

`elem_plan` is a **non-destructive deduced query** — a peer of the six existing ones — not a rewrite
pass. It records "this is what `out[i]` is" and **leaves the graph intact**. Two hard reasons:

1. Questions 2 and 3 only have an owner if the intermediate object still *exists* for the backend to
   decide about. A rewrite doing `plan.drop.insert(mid)` has already made the backend's decision.
2. **This repo already has a case where materialising is the optimisation**: S27 rung-3 packing
   deliberately builds a j-tile-major 64-aligned panel. A law that always elides forbids it.

The existing `analyze_map_fusion` rewrite stays as it is — it is the only mechanism that collapses
`Split` tasks and shrinks the graph for *all* backends. Re-expressing it as a query consumer is
measurement-gated future work, not owed here.

---

## 3. The representation

In `crates/mapal-ir/src/algo.rs`, exported from `lib.rs`'s `pub use algo::{…}` block beside
`TilePlan`/`BoundsProof`/`LastUsePlan`:

```rust
pub enum ElemSrc {
    Index,                                          // out[i] = i            (Iota)
    Broadcast { source: ObjectId, slot: u32 },      // out[i] = x            (Fill)
    Load { source: ObjectId, slot: Option<u32> },   // THE CUT — materialise and load
    Pair(Box<ElemSrc>, Box<ElemSrc>),               // structural pairing    (Zip, Enumerate)
    // step 4 only:
    // Apply { body: FuncId, source: ObjectId, captures: u32, inner: Box<ElemSrc> },
}

pub struct ElemPlan { /* SecondaryMap<ObjectId, ElemSrc> */ }
impl ElemPlan { pub fn src(&self, arr: ObjectId) -> Option<&ElemSrc>; }

impl CategoryIr { pub fn elem_plan(&self, f: FuncId) -> ElemPlan; }
```

`Load`'s `(source, slot)` is deliberately the exact argument pair `array_operand_ptr` and
`load_component` already take, so no emitter needs new pointer plumbing. `Zip = Pair(Load, Load)`;
`Enumerate = Pair(Index, Load)` — no constructor of its own, per §1.4.

Same per-function pure-query shape as the six that exist: `emission_plan` (`algo.rs:396`), `tile_plan`
(`:564`), `path_plan` (`:1080`), `loop_plan` (`:1844`), `last_use_plan` (`:2038`), `bounds_proof`
(`:2299`).

**ADR-0032 on its first rung:** a generic graph fact expressed as a mapal-ir query. `ElemSrc` says
what `out[i]` *is* — never what it costs, never anything about a machine. No `Operation` variant, no
`RewritePlan` channel, no change to `mapal-lower`, `mapal-interp`, or `mapal-syntax`.

### 3.1 The law

`elem : ObjectId → ElemSrc` is the unique homomorphism from the graph's producer DAG into the free
`ElemSrc` term algebra:

```
elem(m.target) = shape(m.op) applied to elem of m's array operands

  Iota ↦ Index      Fill ↦ Broadcast      Zip ↦ Pair(·,·)      Enumerate ↦ Pair(Index, ·)
  everything else ↦ Load          (the cut)
```

**Stage composition is the recursion continuing past a `Load`.** One substitution law:

> `Consumer[mid[i]] ∘ Producer{L}  =  Consumer[L(i)]`

Legal iff `L` is total (trap-free), terminating (loop-free), and effect-free (token-free) — automatic
by construction for the four bodyless producers (`graph.rs:116/120/124/128`) — and `mid` has exactly
one producer and sits outside every loop SCC.

Profitability is never consulted. The graph is never mutated. The substitution is *applied* per read
site by the backend, and only at elementwise-at-loop-index reads — **never** at an `Operation::Index`
with a computed address, because that would delete an OOB trap.

Termination: structural recursion over an acyclic producer DAG with a depth-16 cap, the precedent
already used by `tile_iota_size` (`algo.rs:998`) and `element_range` (`algo.rs:2453`). No rewrite
fixpoint exists, so the driver's monotone-measure obligation never arises.

---

## 4. Where it actually bites — read from the sources, not projected

Timed windows only (`() -> time -> t0` … `t1`), from `benches/shapes/*.mapal`:

| shape | timed stage | intermediate in-window | moved by |
| --- | --- | --- | --- |
| transpose_1024 | `ib -> map {…}` | **`Iota` array** (`ib`) | step 2 |
| saxpy_1048576 | `(x,y0) -> zip -> map {…}` | **`Zip` AoS**, 8 MB | step 2 |
| gather_1048576 | `idx -> map { j -> x[j] }` | `Map`-produced `idx` | **step 4** only |
| fir_1048576 | `ts -> map { fold over kr }` | `Iota` (`ts`, `kr`) | buffer only (step 3) |
| conv2d_1024 | `ts -> map { fold over kr }` | `Iota` (`ts`, `kr`) | buffer only (step 3) |
| reduce_1048576 | `(0.0, x) -> fold {…}` | none | — |

Two corrections to the inherited framing, both read out of the code:

**(a) saxpy's timed window contains no `iota`.** Its intermediate is the `Zip` result. S36c §3(b)
attributed its 3.1× probe to the iota law and quoted saxpy 1t; the *pattern* is real but the instance
in saxpy's bracket is `Zip`. S36c §4 half-caught this without following it into the attribution.

**(b) The tiled sites already skip the in-kernel element load.** `emit_tiled_map` (`func.rs:2687`)
reads operands from `site.a.slot`/`site.b.slot` — the captures — and derives `i`/`j` from loop
counters. fir, conv2d and matmul never load the iota buffer. What they gain is buffer elision and
frame footprint, both outside the timed bracket.

`0.3043 → 0.0972` and "saxpy 1t 0.0972 vs C++ 0.0945 = parity" are **probe numbers** (S36c §3), not
compiler measurements. They are pre-registered as predictions here, not carried as results.

---

## 5. Table A — legality (owner: mapal-ir)

| producer | consumer | verdict | reason |
| --- | --- | --- | --- |
| `Iota` (`Index`) | elementwise-at-`i` read, untiled | **legal** | closed form, trap/token-free (`graph.rs:124`); the substituted value is the same trunc'd counter `emit_iota` stores (`func.rs:6698`) — bit-identical |
| `Iota` (`Index`) | **tiled** map site | **legal** | same; element-inline is a no-op there (§4b) — only buffer elision is live |
| `Fill` (`Broadcast`) | elementwise-at-`i` read | **legal** | `i`-independent capture; `emit_fill` already hoists the load above the loop (`func.rs:6718`), so the register holds identical bits |
| `Zip` (`Pair(Load,Load)`) | elementwise-at-`i` read | **legal** | pure structural pairing, no body, no arithmetic (`graph.rs:114-116`); store/reload of `f32`/`f64` is bit-preserving and there is no reassociation to reorder |
| `Enumerate` (`Pair(Index,Load)`) | elementwise-at-`i` read | **legal** | index half is the loop counter (`emit_enumerate`, `func.rs:6655`); ADR-0018's `n ≤ i32::MAX` is a validate fact on the surviving graph |
| any bodyless producer | body reading `mid` at a **computed** index | **ILLEGAL** | `Index` OOB = trap (ADR-0013). Substituting a total law for a trapping load turns `Trapped(IndexOob)` into `Done(…)` — S34's failure class one level up |
| any bodyless producer | whole-array consumer: `Output`, `Return`, `Call` arg, `Phi` operand, `Update` source | **ILLEGAL** | no per-element read to substitute into; for `Output`/`Return` the buffer *is* the observable bytes |
| classifiable `Map` (`Apply`) | elementwise-at-`i` read | **legal** (step 4) | body total, terminating, effect-free, called on bit-identical inputs in the same per-`i` order. Capture identity **not** required — nothing is spliced, it is two calls in one loop |
| unclassifiable `Map` (loop, unproven `Index`, unsafe `Div`/`Mod`, `Call`, `Print`/`TimeMs`) | anything | **ILLEGAL** | recompute could delete or reorder a trap (S34: `Trapped(DivZero) → Done(0)`), turn `Trapped` into `Diverged`, or reorder effects |
| `Fold` (its accumulator result) | anything | **ILLEGAL** | end of a sequentially data-dependent chain; no `out[i]` law without a splittability proof, and that proof is `plan-s37-scan-recurrence`'s territory |
| `Update` (scatter) | anything | **ILLEGAL** | `out[j] = (j==idx) ? val : arr[j]` with a runtime write index — not a function of `i`, and may-trap. Keeps its own emitter-local elision via `last_use_plan` |
| `Call`-produced array | anything | **ILLEGAL** | opaque; totality underivable, already outside `is_pure` (`graph_rewrites.rs:86`). The `Inline` pass may strip the `Call` first, after which the query sees whatever it exposed |
| object in a loop SCC, or with multiple in-edges | anything | **ILLEGAL** | no well-defined element law — a loop-carried array differs per iteration; a multi-producer object has no unique law |
| any legal producer | `mid` with **multiple** elementwise consumers | **legal** | unconditional on fanout count — pure laws recompute value-identically at every reader. This is Sapir's actual fanout case and it composes. Whether to *use* it per site is Table B |

---

## 6. Table B — profitability (owner: each backend; mapal-ir is never consulted)

| target | policy |
| --- | --- |
| **CPU / LLVM** | Inline bodyless laws unconditionally — at most 2 loads + 1 trunc, never worse than the load they replace. `Apply`: inline iff the body's op count (counted off the graph by the *backend*) beats an L2 round trip **and** reads-per-element is ~1 — refuse fir-shaped windowed reuse where `r ≫ n` (fir's `w` is read 67,108,864× for 64 elements). **Keep materialising** when a tiled site consumes via capture slots — S27 rung-3 packing proves materialising is sometimes the optimisation |
| **GPU / CUDA** | Bandwidth-bound, registers cheap — recompute wins for bodyless and most `Apply` laws. The refusal axis is question 3 (registers / occupancy / SMEM), decided in `kernel.rs` against its own budgets (the F7 `MAX_LOCAL_ARRAY_BYTES` precedent, `kernel.rs:780`) |
| **FPGA / verilog** | "Store" is BRAM, "recompute" is silicon area. No policy owed until that backend emits these ops at all — zero occurrences today |

Every row is a **per-target answer to the same legal fact**. That divergence is precisely why the
table cannot live in mapal-ir.

---

## 7. Preconditions, each with the failure it prevents

1. **Producer recognition is an exact op-tag set** — `Operation::{Iota, Fill, Zip, Enumerate}` — never
   a "has no `FuncId`" shape test. *Prevents:* admitting a future bodyless-looking op, or a `Call`
   whose totality is unproven. Trap-freedom is a documented guarantee of those four tags specifically.
2. **`ir.in_edges(arr).len() == 1`** at every recursion step. *Prevents:* substituting the wrong
   definition for a multiply-produced object.
3. **`arr` outside every loop SCC**, checked at every step. *Prevents:* treating a loop-carried array
   as a per-`i` function — recompute would observe a different iteration than the load did.
4. **Depth cap 16** (`algo.rs:998`/`:2453` precedent). *Prevents:* unbounded query cost. A cut is
   always sound: `Load` = materialise = the status quo.
5. **The query is read-only** — no `RewritePlan` channel, no `plan.drop`, no `IrBuilder`. *Prevents:*
   pre-empting the backend's decision, forbidding deliberate materialisation, and — structurally —
   the de-tiling cliff of §8, since no object is ever deleted.
6. **No cost / op-count / read-count field anywhere in `ElemSrc` or `ElemPlan`.** *Prevents:* mapal-ir
   learning machine facts (ADR-0032).
7. **The backend substitutes only at elementwise-at-loop-index read sites** — the six `emit_*`
   skeleton loads — never at an `Operation::Index` with a computed address. *Prevents:* deleting an
   OOB trap. *Recorded headroom:* computed-index substitution is legal with a synthesised `j < n`
   bound; that is a separate law.
8. **Buffer elision requires** `ObjectKind::Temporary`, not `Output`/`Return`-reachable, and every
   consumer either inlines the law or provably never loads the buffer. *Prevents:* dereferencing an
   elided buffer; changing observable bytes.
9. **The `Iota`/`Fill` count `Constant` and its `IotaCountMismatch` tie (`validate.rs:192-229`) survive
   untouched** — automatic under query-not-rewrite, asserted so a future rewrite cannot regress it.
   *Prevents:* losing the static-`n` witness the Scan plan's tree-shape proofs rely on.
10. **The step-4 `Apply` gate, all three conjuncts:** `tile_trap_free(body, &bounds_proof(body), None)`
    (`algo.rs:1038`) **and** `loop_structure(body).is_empty()` **and** no `Print`/`TimeMs` in
    `func(body).morphisms`. The third is not redundant — `tile_trap_free`'s `_ => true` arm would admit
    them, and its existing callers are covered by token threading while this one is not.
    *Prevents:* trap reorder, divergence-class change, effect reorder.
11. **`is_pure` (`graph_rewrites.rs:86`) is NOT widened.** `Iota`/`Fill` stay outside it; `elem_plan`
    is a separate predicate family sharing no list. *Prevents:* the crate-wide DCE behaviour change
    S34 warns deletes observable behaviour.
12. **No `_ =>` wildcard** added to any `Operation` match in `mapal-interp` or either backend.
    *Prevents:* a future variant landing as a silent missing arm instead of a compile error.
13. **Every step ships only when the 1,280-run byte-equality differential is green** at `-O0` and
    `-O2` at every `MAPAL_PAR` width; steps 1–3 additionally require fir/conv2d/matmul emitted-IR
    hashes **unchanged** (they take the tiled early return at `func.rs:6415` and must never reach the
    new code — the negative control).

---

## 8. The accepted counterexample, and why it becomes a requirement

An adversarial verifier refuted "eliding the intermediate is an optimisation", and the refutation is
accepted rather than argued down.

`tile_iota_size` matches a **literal op tag** — `algo.rs:1016`:

```rust
(self.morphisms.get(*definer)?.op == Operation::Iota).then_some(*size)
```

`tile_site` calls it **twice per site**: once for the outer mapped array (`algo.rs:594`), once for the
inner fold's trip count (`algo.rs:623`). Lose either and `tile_site` returns `None`; `func.rs` does
`tile_plan.and_then(|plan| plan.sites.get(m))` with **silent** fallthrough to `emit_map`. The measured
cost of that fall, from this repo's own ablation in `performance/matmul/s25.md:46-48`: matmul f32 1024
**238.3 → 947.6 ms (4.0×)**, f64 3.5×, attn 4.6×. Byte-equal, no diagnostic, 4× slower.

But the *reason* it breaks is that recognition keys on an op tag rather than a structure — which is
exactly the fragility this design removes. So it is a requirement, not an objection:

**`tile_iota_size`'s final check moves from `op == Operation::Iota` to `elem_plan`'s
`ElemSrc::Index` fact, in the same commit as the query.** The `Pair`/`Proj`/capture walk at
`algo.rs:998-1009` and `tile_site`'s two call sites are untouched.

Note that query-not-rewrite already makes the cliff structurally impossible at the IR level — no
object is ever deleted, so the witness always survives. The migration additionally makes the
recognizer robust *in structure terms*, which is the durable half.

---

## 9. Steps

Each independently shippable, gated, measured. Do not batch.

**Step 1 — `elem_plan` + the `tile_site` migration + the pin (one commit).**
Add `ElemSrc`/`ElemPlan`/`CategoryIr::elem_plan` (bodyless producer family; cut everywhere else;
preconditions 1–6) and export from `lib.rs`. Same commit: re-point `algo.rs:1016` to the `ElemSrc`
fact. **Before** the change, land the pin — a test recording `tile_plan(main).sites.len()` for all 7
bench shapes plus matmul and attn.
*Done when:* pinned sites-count green (zero sites lost — this is the §8 gate and it must exist first);
unit tests green (`iota → Index`, `fill → Broadcast`, `zip → Pair(Load,Load)`,
`enumerate → Pair(Index,Load)`, map-produced → cut, loop-SCC → cut, `Parameter` → cut); 1,280-run
differential byte-identical; fir/conv2d/matmul `.ll` hashes unchanged. **Moves no benchmark by
design** — the measurement it installs is the pin.

**Step 2 — LLVM untiled `emit_map` consumes the fact (buffers still materialise).**
Add `emit_elem` (four arms) and an `elem: ElemPlan` field beside `tile_plan`; in `emit_map`, replace
the GEP+load at `func.rs:6477-6482` with `emit_elem` when the fact is non-`Load`. The tiled early
return (`func.rs:6415`) precedes the new code, so fir/conv2d/matmul are untouched by construction.
No frame, arena, or path change — nothing disappears, only the reload is skipped.
*Pre-registered predictions, written before the run:* saxpy's timed loop vectorizes
(`-Rpass=loop-vectorize` flips off `cannot identify array bounds`) and its wall clock drops; transpose
loses the 4 MB `ib` load stream; **gather does not move** (its `idx` producer is a `Map` — cut until
step 4).
*Done when:* `-Rpass=loop-vectorize` shows saxpy and transpose timed loops vectorized; wall clock
(min **and** median, 1t) recorded on the pinned i9 with the window-boundary caveat named; differential
green; fir/conv2d/matmul hashes unchanged.

**Step 3 — LLVM buffer elision + remaining skeleton loads + ADR amendments.**
Extend consumption to `emit_fold`/`emit_zip`/`emit_enumerate` element loads. Then the backend-owned
elision under precondition 8: skip the producer's store loop and its `build_frame_layout`
(`func.rs:1281`) field. Amend **ADR-0029** and **ADR-0018**'s realization clauses to "materialisation
conditional on a loading consumer" **in this commit** — the elision is what falsifies them.
*Done when:* the emitted saxpy module contains no `[1048576 x {float,float}]` and no zip store loop;
frame-size deltas recorded for saxpy/transpose/fir/conv2d (a static number, no benchmark needed);
saxpy re-measured for the store-pass recovery; differential green; both ADR amendments in the commit.

**Step 4 — `Apply`: `Map` as producer, behind the classifiability gate. Ships alone.**
Add `ElemSrc::Apply` and the recursion arm gated by precondition 10. `emit_elem` gains an `Apply` arm:
recurse for `inner`, then the existing `body_call_arg` + call verbatim. Add the proptest: generate a
producer body containing a trapping op (`Div` by maybe-zero, unproven `Index`), assert `elem_plan`
cuts at it. **This is the concentrated correctness risk.**
*Done when:* gather's timed window loses the `idx` load stream and `-Rpass` reports it vectorized;
wall clock recorded; the trapping-body proptest green; its **own** full 1,280-run differential run in
isolation from any other change.

**Step 5 — CUDA: DEFERRED, and not a mirror.**

Sapir's scoping, recorded so nobody treats this as a port of steps 2–3:

> "CUDA is missing a lot of the optimizations we did in LLVM for vectorization and cache management.
> In CUDA this needs to translate into smem/MMA to use tensor cores and shared memory correctly and
> efficiently. This should come after we already manage to do all the graph shenanigans and
> optimizations on the LLVM at least."

The LLVM side got register blocking (S26), packing and panel residence (S27/S27b), vector
accumulators (S30), `TargetProfile` (S31) and deduced scheduling (S32) — six sessions of cache and
register work that CUDA never received. Consuming `elem_plan` there without that foundation buys the
smallest part of the available win: **the GPU's version of "stop materialising the intermediate" is
staging it in shared memory and feeding tensor cores through MMA**, and that is its own multi-session
track with its own plan, not an arm on this one.

So: **`elem_plan` is available to the CUDA backend from step 1 and is deliberately not consumed
there yet.** Steps 2–4 are LLVM-only by design, not by scheduling accident. There is no correctness
fork — a backend that ignores the query is correct by construction (absence of a fact means load),
which is the property that makes deferring safe.

*Blocked on:* the LLVM track landing complete (steps 1–4 measured), then a CUDA plan whose subject is
smem staging and MMA, with `elem_plan` as one input among several. Table B's GPU row stands as the
recorded policy for when that happens.

**Step 6 — Docs.**
Supersede the draft plan; cross-link both S37 plans with the fibration paragraph (§10); write the ADR
recording the two-tables-two-owners layering, `elem_plan`'s legality-and-geometry-only contract, and
the coexistence rule for `analyze_map_fusion`.

---

## 10. Relationship to `plan-s37-scan-recurrence`

**They compose; neither subsumes the other.** `ElemSrc` is precisely `Rec`'s carry-absent fibre — the
Scan plan's own consolidation table already places `Map` at `window=1, jump=1, no carry at all`, and
every `ElemSrc` law is a specialisation of that row.

`elem_plan` answers *"what is `out[i]` as a closed function of `i`"* and deliberately has **no carry
constructor**, so it can never smuggle `Fold` in — the Scan plan's R5, now enforced by the type rather
than by prose. `Scan` answers *"can a carried recurrence be split"* via the genuinely partial
`combine?`/`unit?` pair, which no stage is ever forced to declare vacuously.

They meet at three points: `Fold`-as-consumer (elem facts for the folded array feed any future Scan
lowering unchanged); `Iota` settled as closed form so it never routes through `combine?` recognition,
closing a duplication where both plans answered "is `out[i] = i` parallel" by unconnected routes; and
the cross-plan hazard — elision destroying the static-`n` witness Scan's tree-shape proof needs —
dissolves under query-not-rewrite, since the graph and the count `Constant` survive.

When `Rec` lands, the Consolidation Principle says `ElemSrc` becomes `Rec`'s `carry = None` projection:
one object, two fibres. Building that fibration before `Rec` exists is speculative (YAGNI). Step 6
adds the cross-reference instead, so the two tracks stop being uncoordinated.

---

## 11. Risks

- **Concentrated correctness risk — step 4's `Apply` gate.** Trap/effect reorder if classifiability
  drifts (the S34 class). *Mitigated:* ships alone with its own differential and a proptest generating
  trapping producer bodies; the gate reuses `tile_trap_free`, the predicate the tiled path already
  trusts, plus the two explicit additions its `_ => true` arm would miss.
- **Tile-recognition regression** — §8. *Mitigated* structurally (nothing is deleted) and by process
  (the pin lands before the migration commit; a lost site is a blocker, not a note).
- **One-backend fork** — LLVM consumes the fact and CUDA deliberately does not (step 5). This is a
  *chosen* asymmetry, not drift: a backend that ignores the query is correct by construction, since
  absence of a fact means load. What it costs is that the two backends' performance stories diverge
  further for a while, on top of the six sessions of cache/register work CUDA already lacks. Do not
  "fix" it by porting steps 2–3 — that buys the smallest part of the GPU win and spends the slot that
  smem staging and MMA should have.
- **Residual empty `Split` task** — the producer morphism survives, so after buffer elision
  `path_plan` still schedules a now-empty task: one wasted dispatch+sync per elided producer. Accepted
  cost of query-not-rewrite. An emitter-local "skip launching a task whose body emitted nothing" is
  legal ADR-0032 rung-2 headroom if measurement ever shows it matters.
- **Measurement distortion** — inlining moves producer arithmetic *into* the timed window on
  transpose and gather while removing the load stream, so the number is not like-for-like and must be
  reported with the boundary caveat. Precedent: the conv2d "per-core gap" was exactly such a window
  artifact.
- **Multi-consumer `Apply` recompute** — a naive backend policy could double work on fanout.
  *Mitigated* by Table B's CPU default: materialise once when there is more than one consumer and the
  body is non-trivial. Bodyless laws are always cheaper than the load and are immune.
- **Depth-16 cap** — deep chains cut early: a missed optimisation, never a wrong answer. Raise only
  with a shape in hand.
- **Scope-creep pressure** — "fully generic" pulls toward widening `is_pure` (DCE blast radius) or
  admitting `Fold` as a producer (the Scan plan's territory). Both are refused *by type*: `ElemSrc`
  has no carry constructor and `elem_plan` shares no list with `is_pure`. Named so the implementing
  agent does not helpfully unify them.

## 12. Opportunistic cleanup (no design dependency)

Five independently re-enumerated "bulk op" match lists — `algo.rs:1181`, `algo.rs:1221`,
`cuda/kernel.rs:143`, `:564`, `:1779`, `cuda/arena.rs:124` — collapse into one `is_bulk_op` predicate.

And a note for anyone who later builds a graph-level rewrite for bodyless producers (e.g. to collapse
`Split` tasks): follow the `LiftSpec` synthesis-recipe precedent, **never** `FusionSpec`'s
verbatim-splice — there is no `FuncId` to splice — and `live_functions`/`push_refs` needs a bodyless
branch.
