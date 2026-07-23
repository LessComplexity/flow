# backend-llvm — implementation map

> The functor DESIGN.md ("Categorical model") → code (ADR-0020). Rows updated WITH
> the model and the code, in the same change (FRAMEWORK §6.3). Last: 2026-07-22 ·
> wave 4 — proven-`Index` guard elision + the `FnAttrs` proof refinement (S20
> `bounds_proof` consumer). Previously wave 3 — last-use `Update` memcpy elision
> (suggestion #2, plan-last-use §2 rule 4); 2026-07-21 · wave 2 — truthful fn
> attributes (suggestion #7) + by-reference array call args (suggestion #8, BL5
> amendment). `flow-rt` has no own component folder — its symbols live here
> (DESIGN §1 ownership).

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `TargetText` | `𝕊` — the emitted `.ll` translation unit (the `String` **is** the artifact) | `src/lib.rs:emit` (the `Ok(String)` arm) | built |
| `EmitError` | sum ⊕ — `Unsupported { feature, loc }` (the ✋ cell) ⊕ `Internal(String)`; renderer-free (C3) | `src/lib.rs:EmitError` | built |
| Module skeleton | product assembled inline: `RT_DECLS` externs + `Str` globals + fn bodies + `@main` wrapper | `src/lib.rs:emit` (via `module::{RT_DECLS, collect_str_globals, emit_str_globals, emit_main_wrapper}`) | built |
| `StrGlobal` | product `{ name, bytes }` — one private `unnamed_addr constant` per `Str` object | `src/module.rs:StrGlobal` (+ `collect_str_globals`/`emit_str_globals`/`escape_bytes`) | built |
| `FnCtx` = `FnEmit` | per-fn state: `slots : ObjectId ⇀ 𝕊` (partial — erased objects have none), `allocas`/`body` buffers, `next` ordinal counter, borrowed `fnames`/`strings`/`attrs`; `byref : (ObjectId × u32 × 𝕊) option` (the fn's by-ref input: object, prefix k — capture count for a Map/Fold body, `u32::MAX` = every top-level Array for a Named fn — by-ref struct text) and `ptr_resident : ObjectId ⇒ ()` (by-ref-array Projs whose slot is an `alloca ptr`, plus the input itself when a Named fn's whole input is one bare Array); `lup : LastUsePlan` (the fn's `last_use_plan` — dead/escape/carried facts, never re-derived) and `elided_updates : ObjectId ⇒ ()` (Update targets sharing their dead source's slot — no alloca of their own); `bp : BoundsProof` (the fn's `bounds_proof` — the provably-in-bounds `Index` set backing the guard elision, computed once in `FnEmit::new`) | `src/func.rs:FnEmit` | built |
| `FnAttrs` | product of two `FuncId ⇒ bool` maps — the clean set (no integer `Div`/`Mod`, no trap-capable `Index`/`Update` — a `bounds_proof`-proven `Index` cannot fire, so it does not count — no `Print`/token, transitively-clean callees) and the loopy set (`LoopEnter` or a call/body cycle, transitively callerward) | `src/func.rs:FnAttrs` | built |

## Morphisms / passes (Trn) → code

| Pass | Signature | Realised at | State |
| --- | --- | --- | --- |
| `emit` | `&CategoryIr → Result<String, EmitError>` (ADR-0020 §1; L3 capability gate on `loop_plan(...).is_some()` per merge, then skeleton + per-fn walk; deterministic fn names `flow_main`/`fn{ord}`; `FnAttrs::analyze` pre-pass) | `src/lib.rs:emit` | built |
| `FnAttrs::analyze` | `&CategoryIr → FnAttrs` — the two fixpoints (clean callerward over `Call` + `Map`/`Fold` body edges; loopy likewise, seeded with `LoopEnter` fns and self-reaching call cycles — the `TrapCaps` spirit, suggestions #7); the clean scan holds the fn's `bounds_proof` so a proven `Index` is skipped as not trap-capable (S20 wave 4; `Update` stays counted) | `src/func.rs:FnAttrs::analyze` | built |
| `emit_fn` = `FnEmit::emit` | `(CategoryIr × FuncId) → 𝕊` — entry-block allocas (one per materialized object), `%arg` prologue store, the topo walk, the `ret`/`ret void` epilogue; `define internal <sig>` + truthful attributes (clean ⇒ `readonly nounwind` + `willreturn` when loop-free; a clean bare-`ptr` by-ref param ⇒ `noalias nocapture readonly`) | `src/func.rs:FnEmit::emit` (`+ walk`) | built |
| `emit_morphism` | per `topo_order` step — the §2 op table dispatch over per-object slots (the piecewise functor application) | `src/func.rs:FnEmit::emit_morphism` (+ helpers `emit_arith`/`emit_compare`/`emit_call`/`emit_print`/`emit_index`/`emit_update`/`emit_map`/`emit_fold`/`emit_zip`/`emit_enumerate`/`emit_iota`/`emit_fill`) | built |
| `emit_loop` | per canonical quartet — ADR-0016 guard-first CFG (`entry`/`header`/`advance`/`exit`/`after`) over the `LoopPlan` decide/advance cones | `src/loops.rs:emit_loop` | built |

## Supporting maps (Trn) → code

| Item | Signature / role | Realised at |
| --- | --- | --- |
| `lower_ty` | `Ty → Option<𝕊>` — LLVM type text; `None` for erased (`Unit`/`IoToken`/`Str`, empty-residual product) | `src/ty.rs:lower_ty` |
| `lower_body_input_ty` | `(Ty × u32) → Option<𝕊>` — a Map/Fold body input with the first-k Array components lowered to `ptr` (by-ref captures, suggestion #6); residual arity and the `erased_index` remap are unchanged, `k = 0` is `lower_ty` | `src/ty.rs:lower_body_input_ty` (callers `FnEmit::{emit, emit_map, emit_fold}`; k via `FnEmit::body_captures`) |
| `lower_named_input_ty` | `Ty → Option<𝕊>` — a Named fn's input with EVERY top-level Array (the input itself, or direct product components) lowered to `ptr` (by-ref call args, suggestion #8 — the capture lowering at `k = u32::MAX`; nested products-in-products stay by value); scalar-only inputs lower identically to `lower_ty` | `src/ty.rs:lower_named_input_ty` (callers `FnEmit::{emit, emit_call}`, `module::emit_main_wrapper`) |
| erasure remap | `residual_arity`/`erased_index` — component→surviving-index remap, derived on demand from the ty (deduce-don't-store, L4) | `src/ty.rs:{residual_arity, erased_index, residual_tys, component_tys}` |
| slot/operand helpers | `load_whole` (deep-copies through the pointer for a ptr-resident object; assembles the by-value whole of the by-ref input product for escaping uses)/`load_component`/`component_ptr` (GEPs the by-ref struct text for the fn input)/`store_obj`/`scratch`/`field_store` — the alloca-slot template (BL1) | `src/func.rs:FnEmit::*` |
| by-ref array operands | `array_operand_ptr` — an `Index`/`Update`/`Map`/`Fold`/`Zip`/`Enumerate`/call-arg array operand's base address: the forwarded `load ptr` when the `Pair` feeder (or bare source) is ptr-resident, a `load ptr` from the field when the operand is a by-ref Array component of the fn input itself, **(S21 WP3b)** the feeder's own slot when the feeder is an Array (no staging roundtrip), the ordinary slot/component address otherwise; `body_call_arg` — the body call's `(c₁…cₖ, rest…)` scratch, Array-capture fields storing addresses (also `emit_call`'s whole-argument template, every component a "capture") | `src/func.rs:FnEmit::{array_operand_ptr, body_call_arg}` |
| aggregate-move discipline **(S21 WP3b)** | an array value NEVER moves as a first-class SSA aggregate: `pointer_only_array_component` (every out-edge of a Pair-built product reads component k as an address — per-op slot rules for Index/Update/Zip/Map/Fold/Call) ⇒ the staged field and its alloca type become `ptr` (`lower_slot_ty`) holding the source alloca's address; value-owed moves (Pair/Proj/Output/Phi/loop init→merge/back-route/exit-payload, escaping Proj targets) go through `emit_memcpy` (the null-GEP sizeof idiom; `dst == src` identity skipped — preserves the #2 in-place elision); array Phi = `select` over the two arm POINTERS + one memcpy | `src/func.rs:FnEmit::{pointer_only_array_component, lower_slot_ty, field_ptr, emit_memcpy, copy_obj, copy_component}` |
| last-use elision | `update_in_place_source` — rule 4's legality for one `Update` morphism (plan-last-use §2, suggestion #2): the source array object when the plan proves it `dead_after` the update (uses ranked decide < `LoopExit` < advance < `LoopBack`, ¬escapes, ¬carried) and it is not ptr-resident (borrowed caller memory — the explicit veto behind the plan's rule 2); `emit_update` then skips the `llvm.memcpy` and inserts the source's slot as the target's (the element store lands in place), the `elided_updates` pre-pass having minted no alloca for it. `None` ⇒ the fresh-alloca copy, byte-identical to before | `src/func.rs:FnEmit::{update_in_place_source, emit_update}` |
| Named call | `emit_call` — top-level Array (components of the) argument by reference per `lower_named_input_ty` (single surviving array ⇒ the bare address; product ⇒ the scratch template; forwarded ptr-resident feeders), scalar-only arguments by value byte-identical to before | `src/func.rs:FnEmit::emit_call` |
| trap emit | `trap_if(cond, kind)` — per-site inline trap block calling `flow_trap`+`unreachable` (as-built S13: not one shared `trap_bb`) | `src/func.rs:FnEmit::trap_if` |
| index guard | `load_index`/`guard_index` — type-directed extension (u8 `zext`, signed `sext`) + range trap; `emit_index(m, …)` calls `guard_index` only when `bp.proven(m)` is false (S20 wave 4 — a proven `Index`'s trap is dead, so the elision emits just the extension + GEP + load; unproven is byte-identical, and `emit_update` guards unconditionally) | `src/func.rs:FnEmit::{load_index, guard_index, emit_index}` |
| loop-driver hooks | `copy_obj`/`copy_component`/`load_route_component` — init→merge, next→merge, exit-payload→exit, guard load | `src/func.rs:FnEmit::*` |
| `@main` wrapper | closed-entry wrapper: `void` call, or scalar return printed through flow-rt (BL8); the (open-entry) call argument's type is the entry fn's by-ref signature (`lower_named_input_ty`, suggestion #8 — array components `ptr`, nulled by `zeroinitializer`); `print_call` places `zeroext` **after** the type (as-built S13) | `src/module.rs:{emit_main_wrapper, print_call}` |

## flow-rt runtime symbols (DESIGN §1; own crate, rows owned here)

| Symbol | Signature | Realised at |
| --- | --- | --- |
| `flow_print_{i32,i64,u8,bool,f32,f64}` | `extern "C" fn(v, newline)` — `Display` (== interp `render`) + flush; declared `i8/i1 zeroext` on the emitter side | `crates/flow-rt/src/lib.rs` (`print_fn!` macro → `emit`) |
| `flow_print_str` | `unsafe extern "C" fn(*const u8, usize, newline)` — `from_raw_parts` a `Str` global (never data) | `crates/flow-rt/src/lib.rs:flow_print_str` |
| `flow_trap` | `extern "C" fn(u32) -> !` — `0=div_zero`/`1=index_oob`; stderr message, `exit(101)`; declared `noreturn` on the emitter side (suggestion #7) | `crates/flow-rt/src/lib.rs:flow_trap` |

## Test / harness → code

| Item | Realised at |
| --- | --- |
| golden `.ll` snapshots (10 examples + micro arith/update/two-loops + the last-use elision pins — `update_inplace_carried_loop` (the matmul4-class loop form emits NO `llvm.memcpy` call, suggestion #2) and `update_memcpy_kept_when_not_dead` (escape-via-`Pair`-field and by-ref ptr-resident sources keep the copy) + ADR-0027 capture map/fold/one-kernel-matmul — the matmul golden pins the by-ref `ptr` capture shape, suggestion #6, and the Named fn's `{ ptr, ptr, ptr }` by-ref call shape, suggestion #8; micro_update pins the bare-`ptr` array input; fir pins the by-ref Named signature + forwarded-pointer Index + the S20 elision pins — `proven_index_guard_elision` (the matmul-cell-class body: ZERO `icmp slt`/`sge` for the proven affine index, the guard present for the unproven sibling) and `proven_index_fn_clean_attrs` (the proof-clean cell body `readonly nounwind willreturn`; the bare-`ptr` param `noalias nocapture readonly`); determinism; nested-loop `Unsupported` pin; exit-only-payload-once pin) | `tests/golden_ll.rs` (+ `tests/snapshots/`) |
| compile-and-run differential (examples raw+rewritten; two sequential loops; traps exit-101; u8 ABI; ≥256-case closed testgen sweep raw+rewritten; matmul loop-driven Update; captures; computed exit payloads; by-ref call-arg escaping uses) | `tests/differential.rs` (testgen via `#[path]` include of `flow-rewrite/tests/testgen`) |
| sepia-at-N perf baseline (`-O0`/`-O2` vs interp; ignored-by-default) | `tests/perf_baseline.rs` |
| flow-rt render-parity table (`4080.0`, `5.375`, `-0.0`, `NaN`, `inf`, u8 255, i64 extremes) | `crates/flow-rt/src/lib.rs` (`#[cfg(test)] render_parity`) |
| **parallel orchestrator (S24)** — plan gate + acyclic check + body-site map | `src/lib.rs:emit` (`path_plan_is_acyclic`, `parallel_body_sites`, `mark_body_closure`) |
| parallel `flow_main` (frame layout/GEPs, task registration, checkpoint/pinned injection, finish) | `src/func.rs:FnEmit::{emit_parallel,build_frame_layout,materialize_frame_slots,slot}`; `HostEmit`/`CheckpointEmit`/`PinnedEmit`/`FrameLayout` |
| task fns (Split range loops, Seq chains/folds/pure loops) + host/task walk filter | `src/func.rs:FnEmit::{emit_task,walk_filtered,bulk_bounds}`; `GuardFlavor` |
| speculate-and-order guards (record + dummy-zero continue; watermarks; site topo) | `src/func.rs:FnEmit::{record_trap,emit_watermark,task_site,emit_task_div}` + task arms in `emit_index`/`emit_update`/`emit_arith` |
| checkpoint injection (earliest task-reading host glue; pre-`LoopEnter` hoist for effectful loops — S24 review find) | `src/func.rs:checkpoint_injection`, `HostEmit::pre_loop`, `walk_filtered` LoopEnter arm |
| packed wait-entry constants `(task<<32)\|threshold` | `src/func.rs:wait_global` |
| scheduler runtime (pool, rank-seeded deques + stealing, help-first waits, trap flag CAS-min, watermarks, pinning, `FLOW_PAR`, `GRAIN=4096`) | `crates/flow-rt/src/lib.rs` (`flow_par_begin/task/pin/dep/launch/wait/check/trap/watermark/run_pinned/finish`, `Pool`, `Run`) |
| the deduced task DAG itself (paths, deps, ranks, transitive trap sites, thresholds, pinning, effectful-loop exclusion) | `crates/flow-ir/src/algo.rs:CategoryIr::path_plan` (+ `fn_trap_capabilities`, `WaitEntry`) |
| R-PAR live pins (big-N split parity; trap stdout-prefix order; env matrix 1/8/unset; run-twice) | `tests/differential.rs:differential_parallel_{bign,trap_order,env_matrix,run_twice}` |
| parallel structural pins (frame/task/ckpt shapes; speculating fold body; watermark; pre-loop wait order) | `tests/golden_ll.rs:{golden_parallel_matmul_cap,parallel_scalar_guard_publishes_watermark,parallel_effectful_loop_waits_before_entry}` |
| **tile emission (S25)** — the `TILE_J=16` k-outer/lane-inner register micro-kernel at recognized sites (both flavors via `bulk_bounds`; per-row range clipping; per-cell op/operand/k-order exact) | `src/func.rs:FnEmit::emit_tiled_map` (dispatch in `emit_map` on `tile_plan.sites`; plan computed in `FnEmit::new` when `EmitOpts::tiling`) |
| emission options + compute timer (S25) | `src/lib.rs:{EmitOpts,emit_with_opts}` (tiling default-on; `emit` = defaults, byte-identical); `src/module.rs:PERF_DECLS`; `flow_perf_begin/end` brackets in `FnEmit::emit`/`emit_parallel_flow_main` (perf drops clean attrs); example flags `--perf`/`--no-tile` |
| tile pins (nest shape 2-D/1-D; untiled fallback shape; tiled-vs-oracle + tiled-vs-untiled runtime at -O0/-O2 incl. FIR) | `tests/golden_ll.rs:{tile_nest_shape,tile_nest_shape_1d,untiled_map_shape}` · `tests/differential.rs:{differential_tiled_matmul,differential_tiled_fir}` |

## Notes / divergences

DESIGN as-built deltas (all marked `(as-built S13)` in DESIGN §1/§2/§4): per-site inline
trap blocks instead of one shared `trap_bb`; `guard_index` emits the two-sided signed
compare on the zext'd i64 for every index type (semantically equal to the uge-only u8
text); call-site `zeroext` after the type; perf top-N 65536 with the 262144 escape hatch.

BL5 amendment (2026-07-21, suggestion #8): the internal ABI is no longer
"aggregates by value" for arrays — top-level Array (components of) a Named fn's input
travel as `ptr` (observably read-only parameters), products/scalars still by value;
nested products-in-products stay by value (recorded limitation). `internal` linkage and
the `@main` wrapper are unchanged.
