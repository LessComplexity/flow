# System implementation map

> Whole-system functor [`architecture-map.md`](architecture-map.md) → code, deduced
> from the component IMPLEMENTATION.md files (FRAMEWORK §4.3 — summarise and point,
> never fork). System-level rows only; per-morphism detail lives in the linked maps.
> Keep in sync WITH the code (FRAMEWORK §6.3).

## Components → code root

| Component | Code root | Model | Code map | State |
| --- | --- | --- | --- | --- |
| syntax | `crates/flow-syntax/` | [DESIGN](components/syntax/DESIGN.md) | [IMPLEMENTATION](components/syntax/IMPLEMENTATION.md) | built |
| ir | `crates/flow-ir/` | [DESIGN](components/ir/DESIGN.md) | [IMPLEMENTATION](components/ir/IMPLEMENTATION.md) | built |
| lower | `crates/flow-lower/` | [DESIGN](components/lower/DESIGN.md) | [IMPLEMENTATION](components/lower/IMPLEMENTATION.md) | built |
| check | `crates/flow-check/` — entry `flow_check::check(source, &Program, &CategoryIr) -> Vec<Diagnostic>` | [DESIGN](components/check/DESIGN.md) | [IMPLEMENTATION](components/check/IMPLEMENTATION.md) | built |
| interp | `crates/flow-interp/` | [DESIGN](components/interp/DESIGN.md) | [IMPLEMENTATION](components/interp/IMPLEMENTATION.md) | built |
| rewrite | `crates/flow-rewrite/` — entry `flow_rewrite::rewrite(CategoryIr) -> RewriteResult` (by-value; fixpoint of 4 passes) | [DESIGN](components/rewrite/DESIGN.md) | [IMPLEMENTATION](components/rewrite/IMPLEMENTATION.md) | built |
| backend-llvm | `crates/flow-backend-llvm/` | [DESIGN](components/backend-llvm/DESIGN.md) | [IMPLEMENTATION](components/backend-llvm/IMPLEMENTATION.md) | stub |
| backend-cuda | `crates/flow-backend-cuda/` | [DESIGN](components/backend-cuda/DESIGN.md) | [IMPLEMENTATION](components/backend-cuda/IMPLEMENTATION.md) | stub |
| backend-verilog | `crates/flow-backend-verilog/` | [DESIGN](components/backend-verilog/DESIGN.md) | [IMPLEMENTATION](components/backend-verilog/IMPLEMENTATION.md) | stub |
| cli | `crates/flow-cli/` | [DESIGN](components/cli/DESIGN.md) | [IMPLEMENTATION](components/cli/IMPLEMENTATION.md) | stub |

## Shared objects (one `Dat`, materialised in ≥2 components)

The three cross-component bridges — each an *audited, justified* shape; do not "fix"
them (detail: [categorical-model.md §6–§7](architecture/categorical-model.md)):

| Object / bridge | Signature | Realised at | Stored? |
| --- | --- | --- | --- |
| `SourceLoc` duality (D8) | `flow_syntax::SourceLoc → flow_ir::SourceLoc` | seam: `crates/flow-lower/src/tys.rs:ir_loc` | stored copy at one declared seam (keeps `flow-ir` zero-dep) |
| type-resolution functor | `flow_syntax::TyKind ⇀ flow_ir::Ty` (partial) | `crates/flow-lower/src/tys.rs:resolve_ty` / `tys.rs:TypeTable::resolve` | deduced (a pass) |
| Diagnostic seam | `Diagnostic ⊕ IrError ⊕ IrViolation ⇀ rendered 𝕊` | per-crate enums; renderer reserved to `flow-cli` (planned) | deduced at the CLI |

## System entry points

| Entry | Trn triggered | Code | State |
| --- | --- | --- | --- |
| `flow` CLI | build/run/dump-ir/test | `crates/flow-cli/src/main.rs:main` | stub (exits 1) |
| `dump_ir` example | file → lex→parse→lower → Mermaid | `crates/flow-lower/examples/dump_ir.rs` | built |
| `run` example / test pipeline | parse→lower→`run` (fueled) | `flow-interp` tests + example (see [interp map](components/interp/IMPLEMENTATION.md)) | built |
| `cargo test --workspace` | the whole `Alg` under golden/property/differential harnesses | per-crate `tests/` | built (511 green, S12) |

## Divergences (system-level)

None known. Watch item: until `cli` is built the Diagnostic seam has no renderer —
structured errors are only surfaced through tests and examples.
