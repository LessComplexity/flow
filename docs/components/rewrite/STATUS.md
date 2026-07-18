# Component: rewrite

Status: tested
Last updated: 2026-07-18 · Session 13 (`is_canonical`/replay migrated to `flow_ir::loop_plan` (BL7); testgen gains `Update` + multi-loop)
Spec references: category-ir.md §9 (optimization framework; §9.6 verification), §6.1.1 (map fusion). Binding model: DESIGN.md (R1–R6, §1.2 P1–P3, RW1–RW11). Supporting: architecture.md §2.2.6.
Depends on: ir, interp (dev) Depended on by: backend-llvm, backend-cuda, backend-verilog, cli

## What works

**P4 complete (S12).** Plan+replay rewriter over sealed IR: layer 3 (const fold at oracle-exact wrapping/IEEE semantics + integer/bool identities + proj∘pack/index-of-const/phi-select forwarding), layer 4 (trap-conservative DCE incl. fn-level; CSE value numbering), layer 1 (Map∘Map fusion with synthesized inline bodies + map(id) elimination), fixpoint driver (`rewrite`/`rewrite_with`, MAX_ROUNDS 32, §9.6 report). Whole pipeline property-tested vs the oracle: random program × random input, per-pass and full, R1 (`Done` byte-exact; traps ⊥-identified; classes never cross) + validate-clean + determinism + idempotence. testgen (random Core program generator, trap-free mode included) lives in `tests/testgen/` for P5–P7 differential reuse — **S13: emits `Step::Update` (ADR-0021) and multi-loop shapes**. Two sequential canonical loops per fn are rewritable (S12 per-merge exit attribution). **S13: canonicity + loop replay now delegate to `flow_ir::CategoryIr::loop_plan`** (`is_canonical` gates on `loop_plan(...).is_some()`, replay reads the same `LoopPlan`) — the BL7 one-source-of-truth loop CFG shared with interp and backend-llvm, replacing the local route-feeder derivation.

## What does not / known issues

- Non-canonical loop shapes (multi-merge nested SCC, multi-back/exit) ⇒ whole-graph identity with `skipped_non_canonical` (RW8; interp M1 shares the boundary).
- Conservative as-built (all R1-safe, headroom in DESIGN §11): DCE never removes dead `Div/Mod/Index/Call/Map/Fold` cones; CSE skips constants (P1); layer 2 is a data table only (`naturality.rs`, no pass).

## Invariants enforced (and where in code)

P1/P2 plan-consistency (`plan.rs::is_consistent`, debug-asserted at `replay`); P3 fusion divergence guard (`functor_laws.rs::is_loop_free_fn`); token-forward exclusion (`equations.rs::forward`, `graph_rewrites.rs` CSE token skip); R2 validate-after-every-replay (`driver.rs`); R6 canonicity gate (`replay.rs::is_canonical` gating on `flow_ir::CategoryIr::loop_plan(...).is_some()` — S13: per-merge route-feeder attribution now lives in flow-ir, BL7).

## Test coverage (27 in-crate)

Identity anchor (`identity.rs`, 5: 10 in-Core examples byte-equal; two-sequential-loops rewritable; RW8 nested-loop non-canonical + rewrite-is-identity pins; **ADR-0021 `update_graph_round_trips_through_rewrite` — an `Update`-bearing graph survives the optimizer interp-equal**) · property battery (`property.rs`, 8: 4 proptest suites × per-pass/full/determinism/idempotence, 48 cases each default — 16k+ programs in extended runs — the testgen now emits `Step::Update` + two-loop shapes; + 4 adversarial pins: dead trapping Div kept; two-trap permutation; float x+0.0 untouched; divergent loop stays Diverged) · example golden (`golden.rs`, 1: interp-exact + Mermaid + report snapshots, sanity-read: CSE ×6 on sepia, ConstFold+DCE on vector_add/zip_demo) · const-fold micros (`const_fold.rs`, 7: **ADR-0021 L-a `index_of_update_equal_const_in_bounds_folds` + `index_of_update_oob_not_folded` + `index_of_update_non_const_not_folded`**, token-proj-pack no-forward, P1 const/identity-add-into-Return survives ×2, P2 `loop_carried_mul_zero_not_constified`) · per-rule micros (`micro.rs`, 6: P3 loop-bearing body not fused, DCE/CSE exclusions, fusion shapes, token-forward regression).

## Performance notes (rewrite_scale, 2026-07-18)

chain (analysis scan): 1k 720µs · 10k 7.75ms · 100k 87ms. grid_cse (CSE + one replay): 1k 257µs · 10k 2.3ms · 100k 23ms. ~Linear; rebuild-not-mutate is nowhere near hot (ir §17 stopgap holds).

## Open questions (→ ADR candidates)

- RW2 (R1 ⊥-identifies traps, fuel-insensitive) — flagged for Sapir; ADR if contested.
- Headroom (DESIGN §11): precise DCE, constant dedup via replay channel, layer-2 naturality pass, generic-SCC replay, Ret-targeted fold via `output()` re-emission.
