# plan-s37 — telling LLVM what `build_frame_layout` already proved

> **SUPERSEDED BY MEASUREMENT, before any code was written. Do not implement.**
>
> The premise — that `%Frame` blocks vectorization of the streaming shapes — is **false for the
> emitted code**. Three checks, in order:
>
> 1. **A hand-written control.** Three arrays as three fields of one struct, one `ptr` parameter,
>    a saxpy loop: it **vectorizes with no metadata at all** (width 4). LLVM computes non-overlap
>    from the *constant field offsets*; it never needs a runtime check between two pieces of one
>    object whose offsets it knows.
> 2. **The whole corpus.** Every task in 7 shapes was extracted and compiled alone — 61 tasks.
>    Exactly **one** reports `unsafe dependent memory operations`, and it is `saxpy__task7`, the
>    `Zip` task, whose output array **nothing reads** since the sibling plan's step 2 (task8's GEP
>    to that field is dead). The other four failures are unrelated: `cannot identify array bounds`
>    (transpose's permuted read), `call instruction cannot be vectorized` (gather), and two tiled
>    kernels with `instruction return type`.
> 3. **saxpy's timed loop already vectorizes.** `task8` — the loop between `t0` and `t1` — is in
>    the vectorized set. There was nothing for this plan to unblock.
>
> S36c §3(a)'s 2.3× was measured on a synthetic probe, never against emitted output. The probe
> reproduced a *pattern* the compiler does not actually emit — the same way its 3.1× iota probe did
> not transfer to transpose.
>
> **What the evidence points at instead:** the sibling plan's **step 3b (buffer elision)**. It
> deletes `saxpy__task7` outright — 8 MB of stores nothing reads, *inside the timed window* — which
> is strictly better than teaching LLVM to vectorize dead work. Second: captured map bodies
> round-trip their argument through one reused `alloca` per element (`body_call_arg`), which
> serializes gather's loop and costs two stores plus a load per element on every capture-carrying
> map.
>
> Kept, not deleted: the §2 disjointness derivation and the §3.1 `ptr_resident`/`packed` exclusions
> are correct and would be needed if a *live* loop ever does hit this. Re-open only with a named
> shape whose timed loop reports the aliasing message.

Status: **SUPERSEDED — not implemented.** Component: `backend-llvm` (emitter-local, zero mapal-ir change).
Sibling: `../../ir/plans/plan-s37-stage-structure.md` (steps 1–4, landed) — that one removed the
*layout* obstacle to vectorizing the streaming shapes; this one removes the *aliasing* obstacle, and
the measurements say this is the one that unblocks the other.
Source of the finding: S36c §3(a). ADR-0032 rung 2: emitter-local cashing, no mapal-ir change.

---

## 1. What is wrong

The parallel orchestrator (S24) turns the graph into tasks with a uniform signature:

```llvm
define internal void @task5(i64 %lo, i64 %hi, ptr %frame)
```

Tasks are separate functions, so the values they share cannot be locals. They all live in one struct,
`%Frame`, built by `build_frame_layout` (`func.rs:1383`) and reached by field index:

```llvm
%o3 = getelementptr %Frame, ptr %frame, i32 0, i32 1   ; x
%o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2   ; y0
%o6 = getelementptr %Frame, ptr %frame, i32 0, i32 4   ; out
```

**Data flows between tasks by address; order is declared separately** through `mapal_par_dep`. That
split is sound — but it means every array a task touches is a piece of *one object*.

LLVM's loop-access analysis wants to emit a runtime "do these ranges overlap?" guard before taking a
vector path, and it can only do that between *distinguishable* objects. Given three pointers all
derived from `%frame`, it cannot, and reports `loop not vectorized: unsafe dependent memory
operations`. A loop that is two unit-stride loads and one unit-stride store, with no loop-carried
dependence, stays scalar.

**Two things to be precise about, because they change what the fix is.**

1. Nothing asserts a dependence. LLVM simply cannot *prove* independence, and a compiler that cannot
   prove a thing assumes the conservative case.
2. **This is not about tasks racing each other.** The question is intra-loop: can iteration `i+1`'s
   load read what iteration `i`'s store wrote? `@task5` is compiled as ordinary sequential code and
   the vectorizer has no idea a pool exists. So the loss is baked in at compile time and **paid at
   `MAPAL_PAR=1`** — the object code does not depend on thread count. That is why the streaming
   shapes lose at one thread, and why the check below is a 1t measurement.

## 2. What the backend already proved and never said

`build_frame_layout` walks `owned_objects()` and gives **every non-elided object its own field
index** (`func.rs:1385-1409`). Two objects with different field indices therefore occupy disjoint
storage *by construction* — not by analysis, by how the layout is built.

The single exception is deliberate and already recorded: an in-place `Update` whose target aliases
its source. `update_in_place_source` marks it (`func.rs:1376-1380`), `elided_updates` drops the
target's own field, and `update_aliases` maps target → source; `build_frame_layout` then resolves
the chain so both objects land on **one** field index (`func.rs:1449-1455`).

So the disjointness fact reduces to something trivially checkable:

> **Two accesses whose base is a different `%Frame` field index cannot alias.**
> Aliasing objects already share an index, by construction.

And grep says the emitter never tells LLVM: zero `!alias.scope` / `!noalias` in `crates/`, apart from
the by-ref *parameter* attribute at `func.rs:1408`.

## 3. The change

Emit alias metadata per access, not per type — LLVM has no way to hear "field 1 and field 2 are
disjoint" as a property of `%Frame`.

- One `!alias.scope` domain per emitted function, one **scope per frame field index** actually used.
- Every load/store based at field `K` carries `!alias.scope !{K}` and `!noalias !{all other used
  fields}`.
- Precedent in the same file: clean by-ref array *parameters* already carry
  `noalias nocapture readonly` (`func.rs:1408`). This is that move applied to fields.

### 3.1 Excluded — and these are correctness exclusions, not conservatism

Two field classes must get **no** metadata. Both were found by reading the layout code, and both
would be wrong rather than merely imprecise:

- **`ptr_resident` fields** (`func.rs:1365`). A by-ref array parameter's frame field holds a `ptr`;
  the array itself lives in the *caller's* storage. Two distinct such fields can point at the same
  caller array, so field-index distinctness proves nothing about the pointees. Annotate the field
  slot if you like; never the memory reached *through* it.
- **`packed` fields** (`func.rs:196`, the S27 rung-3 j-tile-major panels). Same shape — `ptr`-typed
  members whose pointee is arena memory, not frame storage.

The rule that follows: **annotate only accesses whose base is a direct `getelementptr %Frame` on a
field with inline array storage.** Anything reached through a loaded pointer gets nothing. That is
derivable from `slot_type`, which already decides which fields are inline and which are `ptr`.

## 4. Why this one needs a plan when the others did not

Every earlier step in this s37 track was value-preserving by construction: a wrong `elem_plan` fact
produces a wrong *answer*, and the 1,280-run byte-equality differential catches wrong answers by
construction.

**`!noalias` is a promise, not a claim the compiler checks.** If two annotated accesses do alias,
`-O2` is licensed to produce anything, and the differential may pass anyway — undefined behaviour is
not obliged to fail loudly, or to fail on the inputs the suite happens to run.

So the discipline is inverted here: the metadata must be **derived** from `build_frame_layout` and
`update_aliases`, never asserted from a belief about a program. Where the emitter cannot derive
disjointness, it emits nothing — which costs performance and nothing else.

## 5. Preconditions

1. **Scopes are keyed on the resolved field index**, after `update_aliases` chain resolution — the
   same index `build_frame_layout` assigns. *Prevents:* claiming two aliasing objects are disjoint,
   the one case the layout deliberately creates.
2. **Only inline-array fields are annotated**; `ptr_resident` and `packed` fields are excluded (§3.1).
   *Prevents:* asserting disjointness of two pointees that may be the same caller array.
3. **No metadata on the `%Frame` base pointer itself** — only on the loads/stores derived from a
   field GEP. *Prevents:* an assertion about the whole struct that constrains unrelated accesses.
4. **A scope is emitted only for fields actually accessed in that function**, so an unused field
   cannot contribute a `noalias` claim to accesses that never touch it.
5. **The differential is necessary but not sufficient here** (§4). Ship with a disassembly check that
   the intended loop actually vectorized, and treat an unexplained value change anywhere as a
   blocker rather than a puzzle.

## 6. Steps

**Step 1 — emit the metadata.** Scope domain per function; scope per used inline-array field;
`!alias.scope`/`!noalias` on frame-field loads and stores.
*Done when:* saxpy's remaining `unsafe dependent memory operations` is gone and its timed loop
appears in `-Rpass=loop-vectorize`; 1,280-run differential green at `-O0`/`-O2` at every
`MAPAL_PAR` width; tile-site pin unchanged; golden snapshots reviewed one at a time.

**Step 2 — measure, interleaved.** min **and** median, alternating runs in one session, 1t and par,
across the whole ladder plus matmul as the negative control.
*Pre-registered predictions, written before the run:* saxpy 1t improves (S36c's probe said 2.3×, but
that was a probe measured before steps 1–4 — the incremental on top of the banked 1.41× is unknown
and must not be assumed multiplicative). transpose and gather do **not** improve: their bodies read
at a permuted and a data-dependent index respectively, so vectorization was never available and
step 2 of the sibling plan already demonstrated the iota probe does not transfer to them. matmul,
fir and conv2d do not move: tiled kernels read through capture slots, not frame fields.
*Done when:* the numbers exist with the boundary caveats stated, and every prediction above is
marked confirmed or refuted.

**Step 3 — reconcile.** `docs/performance/shape-ladder-v2.md` and the README's shape rows if any
published cell moves; record the result in `backend-llvm/STATUS.md` either way.

## 7. Risks

- **UB, not a wrong answer** (§4). The distinguishing risk of this change; §5 is the mitigation.
- **The win may be smaller than the probe.** S36c measured 2.3× on a hand-written probe *before* the
  layout work landed. Steps 1–4 have since removed some of the same loop's work. Pre-register, then
  measure; do not republish the probe figure.
- **Frame layout is about to move again.** Step 3b of the sibling plan (buffer elision) removes dead
  fields, which renumbers indices. Scopes are derived per emission, so this is a re-measure rather
  than a correctness issue — but the two must not be measured as one change.
- **CUDA is untouched.** The frame is an LLVM-backend construct; the GPU path has its own arena.
  Nothing here transfers, and per Sapir's scoping the CUDA track is smem/MMA, not this.
