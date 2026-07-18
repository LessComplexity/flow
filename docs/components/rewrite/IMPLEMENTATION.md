# rewrite — implementation map

> The functor DESIGN.md ("Categorical model") → code (ADR-0017). Rows updated WITH
> the model and the code, in the same change (FRAMEWORK §6.3). Last: 2026-07-18 · S12.

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `RewritePlan` (alias?/constify?/drop/fuse?) | product of 4 partial maps | `crates/flow-rewrite/src/plan.rs:RewritePlan` | built |
| `FusionSpec` | product (f, g) | `plan.rs:FusionSpec` | built |
| `RewriteResult` | product (ir, report) — `Debug` only (CategoryIr not Clone) | `src/driver.rs:RewriteResult` | built |
| `RewriteReport` (rounds, applied*, skipped_non_canonical) | product; `applied` = cumulative (PassId × ℕ) in first-fire order | `driver.rs:RewriteReport` | built |
| `PassId` | discrete cat {ConstFold, Cse, Dce, MapFusion} | `driver.rs:PassId` | built |
| `ReplayError` | sum (NonCanonicalLoop) | `src/replay.rs:ReplayError` | built |
| `NaturalityLaw` / `NATURALITY_LAWS` | layer-2 rule table, data only, all `Planned` | `src/naturality.rs` | built (no pass) |

## Morphisms / passes (Trn) → code

| Pass | Signature | Realised at | State |
| --- | --- | --- | --- |
| `analyze_const_fold` | `&CategoryIr → RewritePlan` (layer 3: oracle-exact folds, integer/bool identities, proj∘pack, index-of-const, **index∘update L-a (ADR-0021 §3: const equal in-bounds indices ⇒ alias to the Update value operand)**, phi-select; P1/P2 + token guard in `forward`) | `src/equations.rs:analyze_const_fold` | built |
| `analyze_cse` | `&CategoryIr → RewritePlan` (layer 4: value numbering, BTreeMap string keys, positions not id bits; excludes non-Temporary/token/SCC/internal-pack; **no constant dedup** — P1, headroom) | `src/graph_rewrites.rs:analyze_cse` | built |
| `analyze_dce` | `&CategoryIr → RewritePlan` (layer 4: backward liveness from keep-roots {Returns, SCC objects (RW11), non-pure results (R4), LoopExit targets}; pure-only removal — conservative as-built) | `src/graph_rewrites.rs:analyze_dce` | built |
| `analyze_map_fusion` | `&CategoryIr → RewritePlan` (layer 1: out-degree-1 Map∘Map with loop-free + single-full-writer bodies (P3); map(id) → alias under P1/P2) | `src/functor_laws.rs:analyze_map_fusion` | built |
| `replay` | `(&CategoryIr, &RewritePlan) ⇀ CategoryIr` (recipe classification §1.1; loop quartet — canonicity + per-merge layout **delegated to `flow_ir::CategoryIr::loop_plan`** (BL7, S13: `is_canonical` gates on `loop_plan(...).is_some()`, replay reads the same `LoopPlan`); fused-body synthesis via verbatim inline; fn-level DCE via post-plan reference liveness) | `src/replay.rs:replay` (+ `is_canonical` calling `ir.loop_plan`, `synthesize_fused`, `inline_body`) | built |
| `rewrite` / `rewrite_with` | `CategoryIr → RewriteResult` (by-value; fixpoint ConstFold→Cse→Dce→MapFusion, `MAX_ROUNDS=32`; non-canonical ⇒ whole-graph identity; validate debug-asserted per replay) | `src/driver.rs:rewrite{,_with}` | built |

## Test / harness → code

| Item | Realised at |
| --- | --- |
| testgen (random Core programs over the public builder; closed+open, default+trap_free; `Step::Update` (ADR-0021) + multi-loop shapes — feeds P5–P7) | `tests/testgen/mod.rs` |
| identity anchor (10 examples, byte-equal + validate + lint; two-loop rewritable pin; RW8 nested-loop decline) | `tests/identity.rs` |
| R1 property battery (per-pass + full; determinism; idempotence) + §8.5 adversarial | `tests/property.rs` |
| example goldens (interp-exact + rewritten Mermaid + report snapshots) | `tests/golden.rs` + `tests/snapshots/` |
| per-rule micros (P1/P2/P3 pins, DCE/CSE exclusions, fusion shapes) | `tests/micro.rs`, `tests/const_fold.rs` |
| bench | `benches/rewrite_scale.rs` |

## Notes / divergences

DESIGN as-built deltas are recorded in DESIGN §3.1/§3.2/§5/§11 + plan-rewrite.md As-built (S12): conservative DCE, no constant dedup, Debug-only `RewriteResult`, cumulative report, per-merge exit attribution.
