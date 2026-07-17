# Component: rewrite

Status: tested
Last updated: 2026-07-18 · Session 12
Spec references: category-ir.md §9 (optimization framework; §9.6 verification), §6.1.1 (map fusion). Binding model: DESIGN.md (R1–R6, §1.2 P1–P3, RW1–RW11). Supporting: architecture.md §2.2.6.
Depends on: ir, interp (dev) Depended on by: backend-llvm, backend-cuda, backend-verilog, cli

## What works

**P4 complete (S12).** Plan+replay rewriter over sealed IR: layer 3 (const fold at oracle-exact wrapping/IEEE semantics + integer/bool identities + proj∘pack/index-of-const/phi-select forwarding), layer 4 (trap-conservative DCE incl. fn-level; CSE value numbering), layer 1 (Map∘Map fusion with synthesized inline bodies + map(id) elimination), fixpoint driver (`rewrite`/`rewrite_with`, MAX_ROUNDS 32, §9.6 report). Whole pipeline property-tested vs the oracle: random program × random input, per-pass and full, R1 (`Done` byte-exact; traps ⊥-identified; classes never cross) + validate-clean + determinism + idempotence. testgen (random Core program generator, trap-free mode included) lives in `tests/testgen/` for P5–P7 differential reuse. Two sequential canonical loops per fn are rewritable (S12 per-merge exit attribution).

## What does not / known issues

- Non-canonical loop shapes (multi-merge nested SCC, multi-back/exit) ⇒ whole-graph identity with `skipped_non_canonical` (RW8; interp M1 shares the boundary).
- Conservative as-built (all R1-safe, headroom in DESIGN §11): DCE never removes dead `Div/Mod/Index/Call/Map/Fold` cones; CSE skips constants (P1); layer 2 is a data table only (`naturality.rs`, no pass).

## Invariants enforced (and where in code)

P1/P2 plan-consistency (`plan.rs::is_consistent`, debug-asserted at `replay`); P3 fusion divergence guard (`functor_laws.rs::is_loop_free_fn`); token-forward exclusion (`equations.rs::forward`, `graph_rewrites.rs` CSE token skip); R2 validate-after-every-replay (`driver.rs`); R6 canonicity gate (`replay.rs::is_canonical`, route-feeder attribution).

## Test coverage (23 in-crate)

Identity anchor (10 in-Core examples byte-equal, 4 tests incl. two-loop + RW8 pins) · property battery (4 proptest suites × per-pass/full/determinism/idempotence, 48 cases each default — 16k+ programs in extended runs) + 4 adversarial pins (dead trapping Div kept; two-trap permutation; float x+0.0 untouched; divergent loop stays Diverged) · example goldens (interp-exact + Mermaid + report snapshots, sanity-read: CSE ×6 on sepia, ConstFold+DCE on vector_add/zip_demo) · per-rule micros incl. P1 (`2+3 -> ret` unchanged), P2 (loop-state `x*0` not constified), P3 (loop-bearing body not fused), token-forward regression.

## Performance notes (rewrite_scale, 2026-07-18)

chain (analysis scan): 1k 720µs · 10k 7.75ms · 100k 87ms. grid_cse (CSE + one replay): 1k 257µs · 10k 2.3ms · 100k 23ms. ~Linear; rebuild-not-mutate is nowhere near hot (ir §17 stopgap holds).

## Open questions (→ ADR candidates)

- RW2 (R1 ⊥-identifies traps, fuel-insensitive) — flagged for Sapir; ADR if contested.
- Headroom (DESIGN §11): precise DCE, constant dedup via replay channel, layer-2 naturality pass, generic-SCC replay, Ret-targeted fold via `output()` re-emission.
