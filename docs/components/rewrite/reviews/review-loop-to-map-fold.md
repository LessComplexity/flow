# Review — loop→Map/Fold lifting (S27b)

Date: 2026-07-24 · Subject: ratified
`plans/plan-loop-to-map-fold.md` and its implementation · Method: condition-by-
condition code review, focused oracle tests, property battery, and end-to-end LLVM
acceptance · Verdict: **ready; no unresolved blocker or v1-condition conflict**.

## Review lenses

- **Rule fidelity:** traced every R-LF/R-LM condition from the ratified plan to
  `analyze_lift`, including `K >= 1`, exact two-component products, 0/+1 counter,
  attributed exit payload, token exclusion, identity Update, c-free cone, and n=T.
- **Single source of loop truth:** verified analysis and replay both consume
  `CategoryIr::loop_plan`; the pass does not re-derive SCCs, routes, or phase order.
- **Replay/canonical shape:** checked old-SCC retirement, count-object minting before
  Iota, capture order, seed mapping, body reconstruction, Return targeting, and
  function liveness.
- **Observable semantics:** compared interp before/after for both rules and every
  rejection; ran LiftLoops alone and the full fixpoint under R1/R2, determinism, and
  idempotence.
- **Downstream acceptance:** checked the final matmul4 graph and the emitted LLVM
  tile marker, exact stdout, optimization levels, and parallel environment.

## Findings and dispositions

| id | severity | finding | disposition |
| --- | --- | --- | --- |
| L1 | major | Capturing a precomputed affine temporary hid its derivation from `tile_plan`, so the semantically-correct lift did not reach tiled matmul4. | Fixed: clone safe pure invariant derivations to parameter-projection boundaries; captures remain ordered boundary objects. |
| L2 | major | Synthesized body roots initially wrote `Temporary -> Output`; `tile_plan` requires the lower-canonical final primitive/Fold to target Return. | Fixed: `emit_lifted_return` directs the selected primitive or product root to Return. |
| L3 | minor | The random lift step could capture an arbitrary dead temporary and expose a pre-existing DCE loop-liveness limitation unrelated to this plan. | Kept scope: generated invariants are fresh constants; arbitrary captured values remain covered by focused lift tests and matmul4. No DCE widening. |
| L4 | minor | Missing feeder diagnostics used `SecondaryMap` indexing and obscured reduction failures. | Fixed: replay now reports the unmapped original/resolved feeder ids. |
| L5 | major | A loop satisfying the selected value cone could still contain additional advance-phase work; replacing the SCC would silently retire that unselected work. | Fixed conservatively: `covers_loop_body` requires every decide/advance morphism to be selected body work or exact loop scaffolding. A counter-dependent trapping extra-work rejection pins the boundary. |

## §4.5 completion checklist

- [x] `src/lift.rs`; `PassId::LiftLoops`; default order immediately after Inline.
- [x] `analyze_lift(&CategoryIr) -> RewritePlan`; lift channel keyed by loop merge.
- [x] R-LF: exact state, counter, guard, `K >= 1`, pure acc cone, no token, acc exit.
- [x] R-LM: one identity-index Update, c-free value cone, n=T, c exit, init dropped.
- [x] Rejections pinned: dynamic/zero bound, extra state, token/effect, update count,
      index identity, n≠T, step, init, c-read, and unselected phase work.
- [x] Complete decide/advance coverage: no morphism outside the selected cone and
      exact loop scaffolding can be retired by replay.
- [x] Replay synthesizes captured bodies and replaces the complete SCC.
- [x] R1/R2, determinism, idempotence, and second-round empty planning.
- [x] Testgen emits both lift shapes with `K >= 1`; 1,280 differential observes lifts.
- [x] matmul4: 0 Calls, 0 loop SCCs, Map-with-Fold; tiled align-64 LLVM marker.
- [x] Exact `-275\n3748\n` at O0/O2, default and `FLOW_PAR=1`.
- [x] No changes made to `crates/backends/llvm/src`.
- [x] Release rewrite, LLVM, workspace, and formatting gates green.

## Verdict

The implementation matches the ratified v1 boundary. The performance blockers were
shape-preservation defects in synthesis, not new rule conditions; the final coverage
audit also closes a conservative soundness hole without widening either rule. All are
fixed and regression-pinned. All rejected forms remain loops, including `K = 0`.
