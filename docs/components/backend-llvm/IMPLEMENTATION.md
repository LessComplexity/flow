# backend-llvm — implementation map

> The functor DESIGN.md ("Categorical model") → code (ADR-0020). Rows updated WITH
> the model and the code, in the same change (FRAMEWORK §6.3). Last: 2026-07-18 · S13.
> `flow-rt` has no own component folder — its symbols live here (DESIGN §1 ownership).

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `TargetText` | `𝕊` — the emitted `.ll` translation unit (the `String` **is** the artifact) | `src/lib.rs:emit` (the `Ok(String)` arm) | built |
| `EmitError` | sum ⊕ — `Unsupported { feature, loc }` (the ✋ cell) ⊕ `Internal(String)`; renderer-free (C3) | `src/lib.rs:EmitError` | built |
| Module skeleton | product assembled inline: `RT_DECLS` externs + `Str` globals + fn bodies + `@main` wrapper | `src/lib.rs:emit` (via `module::{RT_DECLS, collect_str_globals, emit_str_globals, emit_main_wrapper}`) | built |
| `StrGlobal` | product `{ name, bytes }` — one private `unnamed_addr constant` per `Str` object | `src/module.rs:StrGlobal` (+ `collect_str_globals`/`emit_str_globals`/`escape_bytes`) | built |
| `FnCtx` = `FnEmit` | per-fn state: `slots : ObjectId ⇀ 𝕊` (partial — erased objects have none), `allocas`/`body` buffers, `next` ordinal counter, borrowed `fnames`/`strings` | `src/func.rs:FnEmit` | built |

## Morphisms / passes (Trn) → code

| Pass | Signature | Realised at | State |
| --- | --- | --- | --- |
| `emit` | `&CategoryIr → Result<String, EmitError>` (ADR-0020 §1; L3 capability gate on `loop_plan(...).is_some()` per merge, then skeleton + per-fn walk; deterministic fn names `flow_main`/`fn{ord}`) | `src/lib.rs:emit` | built |
| `emit_fn` = `FnEmit::emit` | `(CategoryIr × FuncId) → 𝕊` — entry-block allocas (one per materialized object), `%arg` prologue store, the topo walk, the `ret`/`ret void` epilogue; `define internal <sig>` | `src/func.rs:FnEmit::emit` (`+ walk`) | built |
| `emit_morphism` | per `topo_order` step — the §2 op table dispatch over per-object slots (the piecewise functor application) | `src/func.rs:FnEmit::emit_morphism` (+ helpers `emit_arith`/`emit_compare`/`emit_call`/`emit_print`/`emit_index`/`emit_update`/`emit_map`/`emit_fold`/`emit_zip`/`emit_enumerate`) | built |
| `emit_loop` | per canonical quartet — ADR-0016 guard-first CFG (`entry`/`header`/`advance`/`exit`/`after`) over the `LoopPlan` decide/advance cones | `src/loops.rs:emit_loop` | built |

## Supporting maps (Trn) → code

| Item | Signature / role | Realised at |
| --- | --- | --- |
| `lower_ty` | `Ty → Option<𝕊>` — LLVM type text; `None` for erased (`Unit`/`IoToken`/`Str`, empty-residual product) | `src/ty.rs:lower_ty` |
| erasure remap | `residual_arity`/`erased_index` — component→surviving-index remap, derived on demand from the ty (deduce-don't-store, L4) | `src/ty.rs:{residual_arity, erased_index, residual_tys, component_tys}` |
| slot/operand helpers | `load_whole`/`load_component`/`component_ptr`/`store_obj`/`scratch`/`field_store` — the alloca-slot template (BL1) | `src/func.rs:FnEmit::*` |
| trap emit | `trap_if(cond, kind)` — per-site inline trap block calling `flow_trap`+`unreachable` (as-built S13: not one shared `trap_bb`) | `src/func.rs:FnEmit::trap_if` |
| index guard | `load_index`/`guard_index` — type-directed extension (u8 `zext`, signed `sext`) + range trap | `src/func.rs:FnEmit::{load_index, guard_index}` |
| loop-driver hooks | `copy_obj`/`copy_component`/`load_route_component` — init→merge, next→merge, exit-payload→exit, guard load | `src/func.rs:FnEmit::*` |
| `@main` wrapper | closed-entry wrapper: `void` call, or scalar return printed through flow-rt (BL8); `print_call` places `zeroext` **after** the type (as-built S13) | `src/module.rs:{emit_main_wrapper, print_call}` |

## flow-rt runtime symbols (DESIGN §1; own crate, rows owned here)

| Symbol | Signature | Realised at |
| --- | --- | --- |
| `flow_print_{i32,i64,u8,bool,f32,f64}` | `extern "C" fn(v, newline)` — `Display` (== interp `render`) + flush; declared `i8/i1 zeroext` on the emitter side | `crates/flow-rt/src/lib.rs` (`print_fn!` macro → `emit`) |
| `flow_print_str` | `unsafe extern "C" fn(*const u8, usize, newline)` — `from_raw_parts` a `Str` global (never data) | `crates/flow-rt/src/lib.rs:flow_print_str` |
| `flow_trap` | `extern "C" fn(u32) -> !` — `0=div_zero`/`1=index_oob`; stderr message, `exit(101)` | `crates/flow-rt/src/lib.rs:flow_trap` |

## Test / harness → code

| Item | Realised at |
| --- | --- |
| golden `.ll` snapshots (10 examples + micro arith/update/two-loops; determinism; nested-loop `Unsupported` pin; exit-only-payload-once pin) | `tests/golden_ll.rs` (+ `tests/snapshots/`) |
| compile-and-run differential (examples raw+rewritten; two sequential loops; traps exit-101; u8 ABI; ≥256-case closed testgen sweep raw+rewritten; matmul loop-driven Update) | `tests/differential.rs` (testgen via `#[path]` include of `flow-rewrite/tests/testgen`) |
| sepia-at-N perf baseline (`-O0`/`-O2` vs interp; ignored-by-default) | `tests/perf_baseline.rs` |
| flow-rt render-parity table (`4080.0`, `5.375`, `-0.0`, `NaN`, `inf`, u8 255, i64 extremes) | `crates/flow-rt/src/lib.rs` (`#[cfg(test)] render_parity`) |

## Notes / divergences

DESIGN as-built deltas (all marked `(as-built S13)` in DESIGN §1/§2/§4): per-site inline
trap blocks instead of one shared `trap_bb`; `guard_index` emits the two-sided signed
compare on the zext'd i64 for every index type (semantically equal to the uge-only u8
text); call-site `zeroext` after the type; perf top-N 65536 with the 262144 escape hatch.
