# Plan — `map(id) → id` must read the whole body, not just its Return writer

Status: **SHIPPED — S34, 2026-07-26.** Closes the project's top P0 (README "What is next" #4,
`docs/STATUS.md` Blockers): the rewriter deleted a trap that must fire. Found by CI on its
first run; pinned as a proptest regression seed
(`crates/mapal-rewrite/tests/property.proptest-regressions`), **retained and now passing**.
Implemented at `crates/mapal-rewrite/src/functor_laws.rs:is_identity_body` +
`graph_rewrites.rs:is_pure` (`pub(crate)`). Gate: `cargo test --workspace --release` green,
LLVM differential 36/36 in 462 s (ran, did not skip); `cargo fmt` clean.

## The counterexample, and what it proves

Re-shrunk with `PROPTEST_MAX_SHRINK_ITERS=200000` (S33 hit the 192-iteration default and
recorded the case as non-minimal). The minimal program is three steps and one map body:

```
map body 0 = [ Bin{op:3 → Div, a:0, b:0},          // elem / elem — traps at elem 0
               PackProj{a:0, b:12, snd:false} ]     // proj₀(pack(elem, y)) = elem
main       = [ ConstI32(0), Iota, MapArr{arr:0, body:0} ]   // iota ⇒ elements 0…N−1
```

Bisect verdict (the pass-composition walk added S33): prefix
`Inline → LiftLoops → ConstFold → Cse → Dce` **preserves** `Trapped(DivZero)`; adding
**`MapFusion`** yields `Done(Scalar(I32(0)))`.

So MapFusion is the deleter, and the earlier passes are its *enabler*: they collapse
`proj₀ ∘ pack` to the parameter, which turns the body's Return writer into
`Output(param)` — while the dead `Div` stays, correctly, because DCE keeps impure dead cones
(R4). `analyze_map_fusion`'s `map(id) → id` arm then reads that one writer, declares the body
the identity, and aliases the entire `Map` away — taking the trap with it.

**It is the `map(id)` arm, not fusion.** Fusion (`map g ∘ map f → map (g ∘ f)`) inlines both
bodies verbatim into the synthesized `h`, so trapping ops in `f` survive.

## Categorical model

The law being applied is the `List` functor's identity law: `List(id_A) = id_{List A}`
(category-ir §6.1.1, FRAMEWORK §1 functor). Its precondition is an equality of **morphisms**:
the body must *be* `id_A`.

In this IR a body is a graph, and its denotation is not only its Return value. The oracle
(`mapal-interp`, the specification) evaluates every op in a function's graph, which is exactly
why DCE pins impure dead cones live (`graph_rewrites.rs::analyze_dce`, R4). So for a body `f`
with Return writer `Output(param)` and a residual op set `R`:

| | `f` denotes | `map(id) → id` legal? |
| --- | --- | --- |
| every `r ∈ R` pure and total | `id_A` | yes — the law applies |
| some `r ∈ R` partial or effectful | `id_A ∘ r`, i.e. `A ⇀ A` | **no** — dropping it strengthens a partial morphism into a total one |

The defect is a precondition read at the wrong scope: `is_identity_body`
(`functor_laws.rs:149`) checks the Return writer — the *value* half — and never asks what else
the body computes. The `?`-marked (partial) morphism is invisible to it.

The same reading gives the fix's shape: the guard must quantify over the body's **whole**
morphism set, and the purity notion is the one DCE already uses for the same reason
(`is_pure`) — one predicate, one source of truth, not a second list that can drift.

## The change

1. `graph_rewrites::is_pure` → `pub(crate)` (it stops being DCE-private; it is now *the*
   crate's "pure and total" predicate, cited by both users).
2. `functor_laws::is_identity_body` additionally requires every morphism owned by the body to
   be `Output`, `Pair{..}` (structural — a product build, no observable of its own), or
   `is_pure`. Anything else — `Div`, `Mod`, `Index`, `Update`, `Call`, `Map`, `Fold`, `Print`,
   and every loop-quartet edge — refuses the rewrite.

Deliberately conservative: `Widen`, `Iota` and `Fill` are total and would be admissible, but
they are absent from `is_pure` and adding them is a separate, testable change. Refusing a
legal rewrite costs an optimisation; permitting an illegal one costs the guarantee the project
rests on.

## Acceptance — as built

- [x] `cargo test -q -p mapal-rewrite --release --test property` — **11 green** (9 + 2 new) with
      the pinned seed retained, 0.23 s. The seed was not touched.
- [x] `identity_map_body_with_dead_trap_stays_trapped` — the hand-written form of the
      counterexample, alongside `dead_trapping_div_stays_trapped`.
- [x] `pure_identity_map_is_still_eliminated` — the positive control: a genuinely pure identity
      body still forwards, asserted through `report.applied` containing `MapFusion`.
- [x] **Negative control run.** Guard reverted ⇒ 5 failures: the new test *and* all four
      property entry points. That is the proof the four were one bug, and that the two new pins
      can actually fail.
- [x] `cargo test --workspace --release` green; `cargo fmt --all --check` clean.
- [x] Docs reconciled in the same change: `docs/components/rewrite/IMPLEMENTATION.md`
      (`analyze_map_fusion` row + a divergence note), `docs/components/rewrite/STATUS.md`
      (header, known-issue headroom, 68 → 70), `docs/STATUS.md` (header, Blockers,
      component row), README Status rows + "What is next" #4.

## Cost, recorded

`map(id) → id` now refuses any body containing `Widen`, `Iota` or `Fill`. All three are total,
so all three are legal to forward; they are outside `is_pure` because that list is DCE's
"always removable" set. Widening it is a separate change with its own pins — and the identity
law is the wrong place to relax a predicate two passes share. No bench or golden moved
(`cargo test --workspace` includes the 1,280-run differential and every `.ll` golden), so the
refusal costs nothing measurable today.

## What this does not touch

The second P0 (`mapal_par_wait` lets workers run ahead of the clock —
`components/backend-llvm/plans/plan-s33b-clock-read-barrier.md`) is unrelated: a measurement
race in the runtime, not a rewrite-law defect.
