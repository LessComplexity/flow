# Plan — loop→map / loop→fold lifting (the matmul4 gap)

Status: **SHIPPED — 2026-07-24 (S27b, same-day continuation); ratified by Sapir at S27 close: "I want even
loop naive implementations to enjoy the perf boost from this structure
automatically."**
Implemented at `crates/flow-rewrite/src/lift.rs`,
`PassId::LiftLoops` in the default list after `Inline`. Acceptance = the S26/WP3
matmul4 pins INVERT: `cell`'s k-loop fold-lifts → `cell` is loop-free → Inline strips
it → the t-loop map-lifts → `tile_plan` fires — matmul4 tiles, outputs `-275`/`3748`
byte-exact on every engine.
(Original design text below; Sapir S26 context: matmul4 loop form pinned non-tiling —
byte-identical emission — while its cap twin `benches/matmul/matmul4_cap.flow` tiles.)

## Why

`tile_plan` is a graph-shape detector: it recognizes `map { … fold { … } }` sites with
affine reads. The idiomatic loop form of the same math (`examples/matmul4.flow` —
`mut`/`Update`/cross-fn `Call`) never produces that shape, so the whole tile ladder
(rungs 1–3, FMA, cuda-to-come) is invisible to it. Lifting canonical loops to
`map`/`fold` is a **rewrite-level equivalence**, proven per-loop from the SCC facts
`loop_plan` already deduces — after it, the naive loop program IS the cap program and
every downstream rung fires unchanged.

## Categorical model

A canonical loop is the guarded trace (E1/ADR-0016): state object `S`, body
`g : S → S ⊕ P` (advance or exit-with-payload). The lift rules are two theorems of the
form "this trace factors through a Core collection op":

| Rule | Shape recognized | Replacement | Values |
| --- | --- | --- | --- |
| **R-LF** (loop→fold) | `S = (k, acc)`; k: init 0, advance k+1, guard `k < K` (K static); acc: advance `acc' = g(acc, k, inv)` pure; exit payload = acc | `(seed, Iota(K)) -> Fold{g}` with captures = inv (ADR-0027) | fold walks array order 0..K−1 = loop order; the acc chain is morphism-identical ⇒ **byte-exact** |
| **R-LM** (loop→map) | `S = (t, c : [E; n])`; t: init 0, advance t+1, guard `t < T` (T static); c: advance exactly one `Update c[t] <- v` (index object ≡ the counter object, v1), v's cone pure and c-free; **n = T**; exit payload = c | `Iota(T) -> Map{t -> v}` with captures = inv; the c-init edge is dropped | identity index over [0,T) writes every slot ⇒ init values dead; cell t computes the same v(t) cone ⇒ **byte-exact** |

Both are consolidations, not new structure: the loop SCC (merge, back-route, decide/
advance cones — `flow_ir::algo::loop_plan`'s `LoopPlan` fields) already *contains* the
`Iota`/`Fold`/`Map` decomposition; the rule cashes it. No new IR ops; `Iota` (ADR-0029)
and captured `Map`/`Fold` (ADR-0027) are the existing targets.

### Why the chain reaches the tiled form (the interleave)

`matmul4.flow`: `cell`'s k-loop is R-LF (acc chain, k as the item); `matmul`'s t-loop is
R-LM whose v-cone contains a pure `Call cell`. The existing **fixpoint driver** delivers
the full chain with no new sequencing machinery, provided `PassId::Inline` is wired in
(S27 fn-strip WP, prerequisite):

1. R-LF fires in `cell` → `cell` becomes a straight-line fn: `(0.0, iota(4)) -> fold`.
2. `Inline` strips the now-loop-free `cell` into `matmul`'s loop body (a `Fold` node in
   the body — a node, not a nested SCC; the backend nested-loop `Unsupported` cell is
   never touched).
3. R-LM fires on the t-loop (cone = affine ops + the Fold, pure) → `iota(16) -> map`.
4. `tile_plan` sees `map{fold}` + affine triples → the site tiles. Rungs 1–3 + FMA fire.

Order-independence: R-LM accepts pure `Call`s in the v-cone (callee token-free), so the
chain also converges if the map lift fires before the inline. Fixpoint = the union.

### Trap and edge alignment

- Body may trap (Index OOB): interp evaluates map/fold elements ascending — identical
  order to the loop's iterations; parallel backends already own map-site trap ordering
  via the S24 speculate-and-order protocol (R-PAR). No new trap story.
- K ≥ 1 is a **condition** of both rules (amended at implementation, 2026-07-24):
  Core has no empty arrays (`Ty::Array` size ≥ 1; `FnBuilder::iota` count ≥ 1), so a
  zero-trip loop (canonical bound `K = 0`, hence guard `k < 0`) cannot lift to
  `Iota(0)` — it stays a loop
  (dead-code territory for const-fold/DCE, not a lift arm; nobody writes it). R-LM's
  T ≥ 1 already follows from `n = T ∧ n ≥ 1`.
- Guard recognition is the canonical lower-emitted shape only (`(k < K)` cond in
  `decide_order`, one attributed exit): anything else stays a loop.

## Rejections (recorded, stay loops — each a future rung, none v1)

Extra carried state (tuple accs are fold-with-product-acc, v2); non-identity write
index (permutations/strided); non-static bounds (ADR-0023 territory); effects/token in
the SCC; multiple `Update`s per iteration; v-cone reading `c`; `n ≠ T` (partial
coverage — init lives); counter step ≠ +1 or init ≠ 0.

## Placement & tests (for the implementation WP, post-ratification)

- `crates/flow-rewrite/src/lift.rs`, `PassId::LiftLoops`, in the DEFAULT list beside
  `Inline` (prerequisite: the S27 fn-strip wiring WP).
- Gate: R1 property harness (per-pass + full pipeline, determinism, idempotence) over
  testgen + a new liftable-loop testgen Step; differential — lifted output byte-equal
  to oracle at -O0/-O2, any FLOW_PAR.
- **Acceptance = the pin flip:** `examples/matmul4.flow` under default `rewrite()`
  emits tile-nest markers (the S26 "verified non-tiling" pin inverts), and its output
  stays `-275` / `3748` byte-exact on interp + llvm + cuda.

## As built (S27b)

- `analyze_lift(&CategoryIr) -> RewritePlan` consumes `loop_plan` directly and keys
  `RewritePlan::lift` by the loop merge. It does not re-derive SCC or route facts.
- Replay synthesizes captured `MapBody`/`FoldBody` functions with the fused-body
  reconstruction pattern, then replaces the complete SCC with
  `const K -> Iota(K) -> Map/Fold`. Pure loop-invariant scalar derivations are copied
  into the body down to parameter-projection capture boundaries, keeping affine
  fields visible to `tile_plan`; the selected cone root targets Return directly.
- R-LF and R-LM both require `K >= 1`. The pinned `K = 0` shape and every listed
  rejection remain byte-identical loops.
- Whole-SCC replacement is guarded by `covers_loop_body`: every decide/advance
  morphism must be selected cone work or exact loop scaffolding. Additional
  counter-dependent/trapping advance work is rejection-pinned and stays a loop.
- Default-rewritten `matmul4` has zero Calls, zero loop SCCs, and a captured Map whose
  body contains a captured Fold. LLVM selects the packed align-64 tile path and
  prints exactly `-275\n3748\n` at `-O0`/`-O2`, under both the default environment
  and `FLOW_PAR=1`.
- Focused rules/rejections, the per-pass/full R1 property battery, generated lift
  steps in the 1,280-program LLVM differential, the full release workspace, and
  formatting are all gated.
