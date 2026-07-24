# rewrite — implementation map

> The functor DESIGN.md ("Categorical model") → code (ADR-0017). Rows updated WITH
> the model and the code, in the same change (FRAMEWORK §6.3). Last: 2026-07-24 · S27b (`LiftLoops` shipped after Inline; R-LF/R-LM plan+replay; matmul4 reaches `tile_plan`). Previously S27 (Inline default-first; loop-bearing-callee guard; cap 256; body Call stripping).

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `RewritePlan` (alias?/constify?/drop/fuse?/inline?/lift?) | product of 6 plan channels | `crates/flow-rewrite/src/plan.rs:RewritePlan` | built |
| `FusionSpec` | product (f, g) | `plan.rs:FusionSpec` | built |
| `LiftSpec` / `LiftKind` | product (kind, counter, count, captures, body cone/result); sum {Fold(acc, seed), Map} | `plan.rs:{LiftSpec,LiftKind}` | built |
| `RewriteResult` | product (ir, report) — `Debug` only (CategoryIr not Clone) | `src/driver.rs:RewriteResult` | built |
| `RewriteReport` (rounds, applied*, skipped_non_canonical) | product; `applied` = cumulative (PassId × ℕ) in first-fire order | `driver.rs:RewriteReport` | built |
| `PassId` | discrete cat {Inline, LiftLoops, ConstFold, Cse, Dce, MapFusion} | `driver.rs:PassId` | built |
| `ReplayError` | sum (NonCanonicalLoop) | `src/replay.rs:ReplayError` | built |
| `NaturalityLaw` / `NATURALITY_LAWS` | layer-2 rule table, data only, all `Planned` | `src/naturality.rs` | built (no pass) |
| `INLINE_MAX_BODY` | ℕ policy constant (256 — S27, was 64; recorded, perf-tunable, never semantics-bearing) | `src/inline.rs:INLINE_MAX_BODY` | built |

## Morphisms / passes (Trn) → code

| Pass | Signature | Realised at | State |
| --- | --- | --- | --- |
| `analyze_const_fold` | `&CategoryIr → RewritePlan` (layer 3: oracle-exact folds, integer/bool identities, proj∘pack, index-of-const, **index∘update L-a (ADR-0021 §3: const equal in-bounds indices ⇒ alias to the Update value operand)**, phi-select; P1/P2 + token guard in `forward`) | `src/equations.rs:analyze_const_fold` | built |
| `analyze_cse` | `&CategoryIr → RewritePlan` (layer 4: value numbering, BTreeMap string keys, positions not id bits; excludes non-Temporary/token/SCC/internal-pack; **no constant dedup** — P1, headroom) | `src/graph_rewrites.rs:analyze_cse` | built |
| `analyze_dce` | `&CategoryIr → RewritePlan` (layer 4: backward liveness from keep-roots {Returns, SCC objects (RW11), non-pure results (R4), LoopExit targets}; pure-only removal — conservative as-built) | `src/graph_rewrites.rs:analyze_dce` | built |
| `analyze_map_fusion` | `&CategoryIr → RewritePlan` (layer 1: out-degree-1 Map∘Map with loop-free + single-full-writer bodies (P3); map(id) → alias under P1/P2) | `src/functor_laws.rs:analyze_map_fusion` | built |
| `analyze_inline` | `&CategoryIr → RewritePlan` (mark a `Call` site iff callee morphisms ≤ `INLINE_MAX_BODY` ∧ callee ≠ entry ∧ **callee loop-free** (`loop_structure(g).is_empty()` — nested-SCC prevention, S27) ∧ no `Call` cycle; Calls inside Map/Fold body fns ARE planned (graph-wide walk); the Map/Fold morphisms themselves never elaborated. **S27: in the default `rewrite()` list, first**) | `src/inline.rs:analyze_inline` | built |
| `analyze_lift` | `&CategoryIr → RewritePlan` (consume `loop_plan`; R-LF/R-LM exact two-component state, const `K >= 1`, 0/+1 counter, pure/token-free cone, exact exit; map additionally one identity `Update`, c-free value, `n == K`; `covers_loop_body` rejects unselected decide/advance work before whole-SCC replacement; key `lift` by merge) | `src/lift.rs:analyze_lift` | built |
| `replay` | `(&CategoryIr, &RewritePlan) ⇀ CategoryIr` (recipe classification §1.1; loop quartet facts delegated to `flow_ir::CategoryIr::loop_plan`; fused and lifted body synthesis; lift replay mints count object + `Iota`, captured Map/Fold, marks old SCC/routes complete; call substitution via `inline_call`/`inline_return`; fn-level DCE via post-plan reference liveness) | `src/replay.rs:replay` (+ `synthesize_lifted_body`, `reconstruct_lifted_loop`, `emit_lifted_return`) | built |
| `rewrite` / `rewrite_with` | `CategoryIr → RewriteResult` (by-value; default fixpoint `Inline→LiftLoops→ConstFold→Cse→Dce→MapFusion`, `MAX_ROUNDS=32`; non-canonical ⇒ whole-graph identity; validate debug-asserted per replay) | `src/driver.rs:rewrite{,_with}` | built |

## Test / harness → code

| Item | Realised at |
| --- | --- |
| testgen (random Core programs over the public builder; closed+open, default+trap_free; Update, multi-loop, `LiftFold`, `LiftMap` with `K >= 1` — feeds P5–P7) | `tests/testgen/mod.rs` |
| identity anchor (10 examples, byte-equal + validate + lint; two-loop rewritable pin; RW8 nested-loop decline) | `tests/identity.rs` |
| R1 property battery (per-pass + full; determinism; idempotence) + §8.5 adversarial — per-pass list includes `Inline` and `LiftLoops` | `tests/property.rs` |
| example goldens (interp-exact + rewritten Mermaid + report snapshots) | `tests/golden.rs` + `tests/snapshots/` |
| per-rule micros (P1/P2/P3 pins, DCE/CSE exclusions, fusion shapes) | `tests/micro.rs`, `tests/const_fold.rs` |
| inline strip pins (policy guards, substitution shapes, duplication, determinism/idempotence) + raw-vs-inlined proptest suites | `tests/inline.rs` |
| R-LF/R-LM positives, every v1 rejection, zero-trip pin, and matmul4 zero-Call/zero-SCC Map-with-Fold acceptance | `tests/lift.rs`, `tests/inline.rs` |
| LLVM tiled acceptance and 1,280 generated differential lift coverage | `crates/backends/llvm/tests/differential.rs` |
| bench | `benches/rewrite_scale.rs` |

## Notes / divergences

DESIGN as-built deltas are recorded in DESIGN §3.1/§3.2/§4.1/§5/§11 and the shipped plan. S12: conservative DCE, no constant dedup, Debug-only `RewriteResult`, cumulative report, per-merge exit attribution. S27 made Inline default-first. S27b adds LiftLoops immediately after it; fixpoint interleaving is load-bearing for matmul4.
