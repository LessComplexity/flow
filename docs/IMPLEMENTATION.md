# System implementation map

> Whole-system functor [`architecture-map.md`](architecture-map.md) → code, deduced
> from the component IMPLEMENTATION.md files (FRAMEWORK §4.3 — summarize and point,
> never fork). System-level rows only; per-morphism detail lives in the linked maps.
> Keep in sync WITH the code (FRAMEWORK §6.3).

## Components → code root

| Component | Code root | Model | Code map | State |
| --- | --- | --- | --- | --- |
| syntax | `crates/mapal-syntax/` | [DESIGN](components/syntax/DESIGN.md) | [IMPLEMENTATION](components/syntax/IMPLEMENTATION.md) | built |
| ir | `crates/mapal-ir/` | [DESIGN](components/ir/DESIGN.md) | [IMPLEMENTATION](components/ir/IMPLEMENTATION.md) | built |
| lower | `crates/mapal-lower/` | [DESIGN](components/lower/DESIGN.md) | [IMPLEMENTATION](components/lower/IMPLEMENTATION.md) | built |
| check | `crates/mapal-check/` — entry `mapal_check::check(source, &Program, &CategoryIr) -> Vec<Diagnostic>` | [DESIGN](components/check/DESIGN.md) | [IMPLEMENTATION](components/check/IMPLEMENTATION.md) | built |
| interp | `crates/mapal-interp/` | [DESIGN](components/interp/DESIGN.md) | [IMPLEMENTATION](components/interp/IMPLEMENTATION.md) | built |
| rewrite | `crates/mapal-rewrite/` — entry `mapal_rewrite::rewrite(CategoryIr) -> RewriteResult` (by-value; fixpoint of 4 passes) | [DESIGN](components/rewrite/DESIGN.md) | [IMPLEMENTATION](components/rewrite/IMPLEMENTATION.md) | built |
| backend-llvm | `crates/backends/llvm/` — entry `mapal_backend_llvm::emit(&CategoryIr) -> Result<String, EmitError>` (ADR-0020; `src/{ty,module,func,loops}.rs`) | [DESIGN](components/backend-llvm/DESIGN.md) | [IMPLEMENTATION](components/backend-llvm/IMPLEMENTATION.md) | built/tested |
| mapal-rt | `crates/mapal-rt/` — shared runtime seam (ADR-0020; owned by backend-llvm per its DESIGN §1): 7 `mapal_print_*` externs (Rust `Display` = interp render parity) + `mapal_trap` → exit 101; **S24: the parallel scheduler (11 `mapal_par_*` externs — work-stealing pool, rank-seeded static schedule, watermark waits, CAS-min trap flag, registration-time pinning; `MAPAL_PAR` env; std-only)**; **S25: cgroup-quota-aware width (`cpu.max` v2 / cfs v1 parsers; `MAPAL_PAR` override absolute) + `mapal_perf_begin`/`mapal_perf_end` compute-timer externs (pool-warm then `Instant`; `MAPAL_PERF total ms=` stdout grammar)**; **S29: `mapal_time_ms` (the `time` builtin's clock seam, one process-lifetime `Instant` epoch) + the heap arena `mapal_rt_alloc`/`mapal_rt_free_all` (a `Mutex<Vec<(usize, Layout)>>` registry over `std::alloc`) — the llvm backend places entry-block blocks ≥ `HEAP_MIN_BYTES` (256 KB) there, which is what lets matmul2048 run on a 64 MB stack**; `staticlib`+`rlib` | [DESIGN](components/backend-llvm/DESIGN.md) §1 + [plan-parallel-orchestrator](components/backend-llvm/plans/plan-parallel-orchestrator.md) | `crates/mapal-rt/src/lib.rs` | built |
| backend-cuda | `crates/backends/cuda/` — entry `mapal_backend_cuda::emit(&CategoryIr) -> Result<String, EmitError>` (ADR-0020; `src/{ty,module,func,kernel,loops,arena}.rs`) + `emit_with_opts(&CategoryIr, &EmitOpts)` (S20: `perf_timing` — CUDA-event `MAPAL_PERF` instrumentation; `emit` = defaults, byte-identical) | [DESIGN](components/backend-cuda/DESIGN.md) | [IMPLEMENTATION](components/backend-cuda/IMPLEMENTATION.md) | built/tested (M3, S15; arenas + emitter-quality S20) |
| backend-verilog | `crates/backends/verilog/` | [DESIGN](components/backend-verilog/DESIGN.md) | [IMPLEMENTATION](components/backend-verilog/IMPLEMENTATION.md) | stub |
| cli | `crates/mapal-cli/` | [DESIGN](components/cli/DESIGN.md) | [IMPLEMENTATION](components/cli/IMPLEMENTATION.md) | stub |

## Shared objects (one `Dat`, materialised in ≥2 components)

The three cross-component bridges — each an *audited, justified* shape; do not "fix"
them (detail: [categorical-model.md §6–§7](architecture/categorical-model.md)):

| Object / bridge | Signature | Realized at | Stored? |
| --- | --- | --- | --- |
| `SourceLoc` duality (D8) | `mapal_syntax::SourceLoc → mapal_ir::SourceLoc` | seam: `crates/mapal-lower/src/tys.rs:ir_loc` | stored copy at one declared seam (keeps `mapal-ir` zero-dep) |
| type-resolution functor | `mapal_syntax::TyKind ⇀ mapal_ir::Ty` (partial) | `crates/mapal-lower/src/tys.rs:resolve_ty` / `tys.rs:TypeTable::resolve` | deduced (a pass) |
| Diagnostic seam | `Diagnostic ⊕ IrError ⊕ IrViolation ⇀ rendered 𝕊` | per-crate enums; renderer reserved to `mapal-cli` (planned) | deduced at the CLI |
| loop attribution `LoopPlan` (BL7) | `(FuncId, merge ObjectId) ⇀ LoopPlan` — one canonical-loop predicate | exported `mapal_ir::CategoryIr::loop_plan` (`crates/mapal-ir/src/algo.rs`); consumed by `mapal-interp` (`src/loops.rs`), `mapal-backend-llvm` (`src/{lib,func,loops}.rs`), `mapal-backend-cuda` (`src/{lib,func,kernel,loops}.rs`), `mapal-rewrite` | deduced (recomputed, never stored) |
| emission classification `EmissionPlan` (S22, plan-minimal-emission) | `FuncId ⇀ (Obj ⇀ Dissolved \| Inline \| Named)` — the minimal-emission split rule (fanout/boundary/guard-driven) | exported `mapal_ir::CategoryIr::emission_plan` (`algo.rs`); consumed by `mapal-backend-cuda` (`src/{kernel,func}.rs` — DevEmit/FnEmit); llvm assessment = WP-E | deduced (recomputed, never stored) |
| task parallelism `PathPlan` (S24, plan-parallel-orchestrator; **S29 clock rules**) | `FuncId → PathPlan` — the task-DAG (fused/parallel/split tasks, speculate-and-order watermarks) | exported `mapal_ir::CategoryIr::path_plan` (`algo.rs:946`); consumed by `mapal-backend-llvm` (parallel `mapal_main`) + `mapal-rt` (work-stealing scheduler) · **S29 (plan-time-builtin):** a `TimeMs` checkpoint FENCES every task written entirely before it in the SOURCE (`algo.rs` `task_max_loc` vs `morph.loc.start` ⇒ `WaitEntry.threshold = None`) — source order because the graph orders pure work against a clock read not at all; and a clock read's consumer cone stays on the host spine (`host_value`/`host_cone` in `is_host`) because a task cannot read a value the host writes after dispatch (FRAMEWORK §4.5 Law 1). Both mutation-verified (`mapal-ir/tests/algos.rs`) | deduced (recomputed, never stored) |
| guard elision `BoundsProof` (S20c) | `FuncId → BoundsProof` — per-`Index` proven-in-bounds | exported `mapal_ir::CategoryIr::bounds_proof` (`algo.rs:2008`); consumed by `tile_plan` (T4 precondition) + llvm/cuda guard elision | deduced (recomputed, never stored) |
| tile sites `TilePlan` (S25 rung 1; S26 rung 2 TI×TJ; S27 rung 3 packing; S28 fold k-split + window/conv rungs; **S29 KC k-panel rung — built, measured a 3× loss locally, shipped default-OFF**) | `MorphismId ⇀ TileSite` — affine-triple recognition, lane strides {0,1}, guard-free; S28: `TileRead.ksplit? : TileRead → TileKSplit` — fold-body `(k÷div, k%div)` derived axes (XOR raw `k`, `depth % div == 0`) | exported `mapal_ir::CategoryIr::tile_plan` (`algo.rs:510`); consumed by `mapal-backend-llvm` (`func.rs:emit_tiled_map` + `emit_tiled_map_blocked` + `emit_tiled_map_blocked_1d` + `emit_tiled_map_conv`; gates `packing_site`/`window1d_site`/`conv_site`; S29 `emit_tile_packed_kc` behind `EmitOpts::kc_nest`, default off — bit-exact either way, see `docs/performance/matmul/s29.md`); cuda consumption standing (S27 item 6) | deduced (recomputed, never stored) |
| last-use `LastUsePlan` | `FuncId → LastUsePlan` — dead-after per object | exported `mapal_ir::CategoryIr::last_use_plan` (`algo.rs:1747`); consumed by llvm in-place `Update` (`llvm/func.rs:1861`) + cuda back-edge freeing/arena coloring (`cuda/func.rs:393`) | deduced (recomputed, never stored) |

## System entry points

| Entry | Trn triggered | Code | State |
| --- | --- | --- | --- |
| `mapal` CLI | build/run/dump-ir/test | `crates/mapal-cli/src/main.rs:main` | stub (exits 1) |
| `dump_ir` example | file → lex→parse→lower → Mermaid | `crates/mapal-lower/examples/dump_ir.rs` | built |
| `emit` examples | file → lex→parse→lower → `.ll` / `.cu` text (dev tools — the `mapal build` embryo) | `crates/backends/{llvm,cuda}/examples/emit.rs` | built (S16) |
| `run` example / test pipeline | parse→lower→`run` (fueled) | `mapal-interp` tests + example (see [interp map](components/interp/IMPLEMENTATION.md)) | built |
| `cargo test --workspace` | the whole `Alg` under golden/property/differential harnesses | per-crate `tests/` | built (673 green, S15) |

## Divergences (system-level)

None known. Watch item: until `cli` is built the Diagnostic seam has no renderer —
structured errors are only surfaced through tests and examples.
