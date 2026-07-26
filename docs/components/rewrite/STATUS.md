# Component: rewrite

Status: tested
Last updated: 2026-07-26 · S34 — **the P0 is fixed: `map(id) → id` deleted traps.** `is_identity_body` judged a body by its Return writer alone, so a body that returns its parameter *and* computes a dead trapping `Div` (the shape ConstFold's `proj∘pack` forwarding produces) read as the identity and the whole `Map` was aliased away — `Trapped(DivZero)` became `Done(0)`. The guard now quantifies over the body's entire morphism set through the crate's one purity predicate (`graph_rewrites::is_pure`, now `pub(crate)` — DCE's "may this dead cone go?" and the identity law's "does this body denote `id`?" are the same question). All four failing property entry points were this one bug; the pinned proptest seed is **retained** and now passes. 70 in-crate tests (+2: the hand-written adversarial case and its positive control, both negative-controlled). Detail: `plans/plan-s34-identity-map-trap.md`. Previous: 2026-07-25 · S29 — `replay.rs:emit_op` gained a `TimeMs` arm so a clock-bearing program can be replayed at all (`fb.time_ms` off the bare token). **No pass plans it**: the existing purity/token predicates exclude it from CSE, forwarding, DCE and lifting with no `TimeMs` special case, exactly as intended for an effectful op. 68 in-crate tests, unchanged — nothing here generates a clock read yet (see known issues). Previous: 2026-07-24 · S27b **loop→Map/Fold lifting SHIPPED** (`plan-loop-to-map-fold.md`, ratified option 2): `PassId::LiftLoops` runs immediately after default-first Inline. `analyze_lift` consumes `flow_ir::loop_plan` and records `LiftSpec` by merge; replay replaces the SCC with a minted count + `Iota(K)` + synthesized captured Map/Fold body. R-LF and R-LM require `K >= 1`; zero-trip loops stay loops. Every rejection is pinned. The matmul4 chain now completes across fixpoint rounds: cell fold-lifts → Inline strips it → outer loop map-lifts → `tile_plan` fires; the final graph has 0 Calls, 0 loop SCCs, and Map-with-Fold, with exact `-275\n3748\n` LLVM output at O0/O2 and default/`FLOW_PAR=1`. Testgen supplies both lift shapes to the 1,280-run differential; 68 in-crate tests green. Previous S27: fn-strip wired default-first, loop-bearing-callee guard, cap 256, body Call stripping.
Spec references: category-ir.md §9 (optimization framework; §9.6 verification), §6.1.1 (map fusion). Binding model: DESIGN.md (R1–R6, §1.2 P1–P3, RW1–RW11). Supporting: architecture.md §2.2.6; docs/components/backend-cuda/plans/plan-region-emission.md §Move 1.
Depends on: ir, interp (dev) Depended on by: backend-llvm, backend-cuda, backend-verilog, cli

## What works

**P4 complete (S12).** Plan+replay rewriter over sealed IR: layer 3 (const fold at oracle-exact wrapping/IEEE semantics + integer/bool identities + proj∘pack/index-of-const/phi-select forwarding), layer 4 (trap-conservative DCE incl. fn-level; CSE value numbering), layer 1 (Map∘Map fusion with synthesized inline bodies + map(id) elimination), fixpoint driver (`rewrite`/`rewrite_with`, MAX_ROUNDS 32, §9.6 report). Whole pipeline property-tested vs the oracle: random program × random input, per-pass and full, R1 (`Done` byte-exact; traps ⊥-identified; classes never cross) + validate-clean + determinism + idempotence. testgen (random Core program generator, trap-free mode included) lives in `tests/testgen/` for P5–P7 differential reuse — **S13: emits `Step::Update` (ADR-0021) and multi-loop shapes**. Two sequential canonical loops per fn are rewritable (S12 per-merge exit attribution). **S13: canonicity + loop replay now delegate to `flow_ir::CategoryIr::loop_plan`** (`is_canonical` gates on `loop_plan(...).is_some()`, replay reads the same `LoopPlan`) — the BL7 one-source-of-truth loop CFG shared with interp and backend-llvm, replacing the local route-feeder derivation.

**Region-emission Move 1 (S20): the `inline` strip pass.** A fifth plan channel `inline : MorphismId → ()` marks strippable `Call` edges; replay substitutes the callee's body into the caller (`input ↦ call source`, `output ↦ call target`, fresh ids in builder emission order — L2 byte-identical), recursing into the callee's own planned calls and redirecting Return writers (`RetDest`: loop exits, slot-wise tuple returns, direct-to-Return sites all covered); fully stripped callees fall out via the existing fn-level reference liveness. Policy (plan §Move 1; S27 revision): inline a site iff callee morphism count ≤ `INLINE_MAX_BODY` (**256**, recorded, perf-tunable) ∧ callee ≠ entry ∧ **callee has no loop** (nested-SCC prevention — loop-bearing callees await the loop→fold lift) ∧ no `Call` cycle (the builder rejects recursion at seal — `IrError::RecursiveCall`). The Map/Fold *morphisms* are never elaborated (parameterized fanout subgraphs), but Calls **inside** their body fns are planned and stripped like any site. **S27: in the default `rewrite()` list, first**; the region pipeline's separate pre-pass invocation is unchanged. R1-pinned: determinism, idempotence (inline ∘ inline = inline), oracle equality over testgen (directed suites + the property battery's per-pass row).

**S27b guarded-trace lifting.** A sixth plan channel
`lift : ObjectId(loop merge) → LiftSpec` recognizes only R-LF/R-LM. The pass consumes
`LoopPlan::{merge,init,back_route,exits,exit_route,scc_objects,decide_order,
advance_order,product_targets}` rather than recomputing topology. Synthesized bodies
reuse replay's fused-body pattern and ADR-0027 captures. Pure invariant derivations
are copied only to the parameter-projection capture boundary, preserving affine
structure for `tile_plan`; the cone root targets Return directly.

## What does not / known issues

- Non-canonical loop shapes (multi-merge nested SCC, multi-back/exit) ⇒ whole-graph identity with `skipped_non_canonical` (RW8; interp M1 shares the boundary).
- `TimeMs` replay (S29) is **untested in-crate**: testgen emits no clock read and the S29 `time` fixtures in backend-llvm emit without `rewrite`, so the new arm runs only under `emit --rewrite` on a timed program. One `identity.rs` byte-identical case closes it when a driver actually rewrites bench sources.
- Conservative as-built (all R1-safe, headroom in DESIGN §11): DCE never removes dead `Div/Mod/Index/Call/Map/Fold` cones; CSE skips constants (P1); layer 2 is a data table only (`naturality.rs`, no pass).
- **S34 headroom:** `map(id) → id` now refuses a body containing `Widen`, `Iota` or `Fill`. All three are total and would be admissible; they are simply outside `is_pure`, which is deliberately DCE's allow-list. Widening it is a separate change with its own pins — the identity law is not the place to relax a purity notion two passes share.

## Invariants enforced (and where in code)

P1/P2 plan-consistency (`plan.rs::is_consistent`, debug-asserted at `replay`); P3 fusion divergence guard (`functor_laws.rs::is_loop_free_fn`); token-forward exclusion (`equations.rs::forward`, `graph_rewrites.rs` CSE token skip); R2 validate-after-every-replay (`driver.rs`); R6 canonicity gate (`replay.rs::is_canonical` gating on `flow_ir::CategoryIr::loop_plan(...).is_some()` — S13: per-merge route-feeder attribution now lives in flow-ir, BL7); inline policy guards — cap / non-entry / Call-cycle (`inline.rs::analyze_inline`), L2 fresh-id determinism (replay builder emission order; `replay.rs::inline_call`).

## Test coverage (70 in-crate)

Identity anchor (`identity.rs`, 5) · property battery (`property.rs`, 11: four
proptest suites × six per-pass/full/determinism/idempotence rows **plus the
pass-composition prefix bisect (S33)**, the pinned CI counterexample seed, and
adversarial pins — including S34's `identity_map_body_with_dead_trap_stays_trapped`
and its positive control `pure_identity_map_is_still_eliminated`, so the guard
cannot be quietly widened into deleting traps again) · example golden (`golden.rs`, 1) · const-fold
micros (`const_fold.rs`, 7) · per-rule micros (`micro.rs`, 6) · inline strip
(`inline.rs`, 15, including the inverted matmul4 pin) · lift rules (`lift.rs`, 3:
captured fold, captured identity-Update map, and named per-rejection pins) · capture,
widen, fixpoint, and replay-unit suites (22).

## Performance notes (rewrite_scale, 2026-07-18)

chain (analysis scan): 1k 720µs · 10k 7.75ms · 100k 87ms. grid_cse (CSE + one replay): 1k 257µs · 10k 2.3ms · 100k 23ms. ~Linear; rebuild-not-mutate is nowhere near hot (ir §17 stopgap holds).

## Open questions (→ ADR candidates)

- RW2 (R1 ⊥-identifies traps, fuel-insensitive) — flagged for Sapir; ADR if contested.
- Headroom (DESIGN §11): precise DCE, constant dedup via replay channel, layer-2 naturality pass, generic-SCC replay, Ret-targeted fold via `output()` re-emission.
