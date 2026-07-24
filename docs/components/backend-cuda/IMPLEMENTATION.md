# Component: backend-cuda — IMPLEMENTATION (model → code functor)

Written: 2026-07-21 · Session 15 · State: **built + M3 green** (the §6 sweep oracle-equal on an RTX 4090 — 640 nvcc compile-and-runs, zero divergences). Updated 2026-07-21 (emitter-quality wave: suggestions #17/#12/#13/#14 discharged — 129 tests; optimization-marathon W2: suggestions #18 arena v1.0 + #19a kernel-time instrumentation discharged — 144 tests: 111 lib + 13 differential + 4 gate + 16 golden) and 2026-07-22 (last-use wave: suggestions #2 back-edge freeing + the BC5 in-place Update amendment discharged — 151 tests: 113 lib + 13 differential + 4 gate + 21 golden; S20 guard wave: proven-Index guard elision + the `TrapCaps` bounds-proof refinement discharged — 153 tests: 115 lib + 13 differential + 4 gate + 21 golden) and 2026-07-25 (**S29 `time` builtin, cuda half**: `Operation::TimeMs` recorded as a new `Unsupported` cell — 5 cells now; no emitter change, no new cuda tests — **163 green** unchanged: 118 lib + 15 differential + 4 gate + 26 golden).

Maps the DESIGN's model (`DESIGN.md` §"Categorical model (Dat + Trn)" + §1–§6) to the realizing code (FRAMEWORK's "realising code" column; drift here is the earliest signal, §6.3). As-built deltas vs the S14 design text are marked **(S15)** and reconciled in DESIGN §"As-built (S15)"; the W2 rows are marked **(W2)** and reconciled in DESIGN §"As-built (2026-07-21 smart arenas + #19a)"; the last-use rows are marked **(LU)** and reconciled in DESIGN §"As-built (2026-07-22 last-use)"; the S20 rows are marked **(S20)** and reconciled in DESIGN §"As-built (2026-07-22 S20 guard wave)"; the S29 rows are marked **(S29)** and reconciled in DESIGN §5 (cell 5) + the L3 rider.

## Dat objects

| Model object | Realised at | State |
| --- | --- | --- |
| `CuText` (𝕊) | `pub fn emit(&CategoryIr) -> Result<String, EmitError>` — the String IS the `.cu` TU (ADR-0020 §1) — `crates/backends/cuda/src/lib.rs` | built |
| `EmitError` ⊕ | `pub enum EmitError { Unsupported { feature, loc }, Internal(String) }` — renderer-free (C3) — `src/lib.rs` | built |
| `HostScalar` | hoisted C++ locals `o{ord}` (ordinal scheme; constants fold into use sites) — `src/func.rs` `FnEmit::{slots, decls}` | built |
| `DevHandle` | host `T*` variable; `n` + strides from the `Ty`, never in the text — `src/ty.rs:lower_ty` (`Array → T*`); rebind = pointer assignment (`FnEmit::copy_obj`) | built |
| `DevBuf` | `cudaMalloc`'d contiguous AoS — byte text `sizeof(base) * flat` in `src/kernel.rs:buffer_bytes`; allocated at `src/func.rs:malloc_buffer` | built |
| `TrapFlag` | `static unsigned int* d_trap` + `trap_init`/`trap_check_after_launch` in `src/module.rs` PRELUDE — stores the flow-rt kind **+1** (0 quiescent, 1 div_zero, 2 index_oob), host decodes `flow_trap(kind - 1)` | built **(S15 — the S14 bare-kind encoding collided with the quiescent 0)** |
| `FnCtx` | `src/func.rs:FnEmit` (slots, decls, body, ordinal/temp counters, allocation registry, the fn's `arena: Option<ArenaPlan>` **(W2)**, the `perf`/`ev_ord` instrumentation state **(W2)**) | built |
| `KernelCtx` | `src/kernel.rs:DevEmit` (+ the produced-local-array set; per-thread inline mode). **S22 WP-B:** plan-driven — `DevEmit.plan` (`flow_ir::emission_plan`, BL7 pattern) + `DevEmit.exprs` memo + `DevEmit.force_named` (Call targets, product-typed Inline, **+ `Fold` targets — S23 fix: an in-twin fold is a loop, its scalar result has no expression form; caught by the first hardware differential over the S22 emitters**); `store_obj` routes Inline to expressions, `component_expr` dissolves Pair-built wrapper products, `is_atomic_expr` parenthesization. **S23 WP-D:** `assemble_body_arg(pair_ty, parts, varying) -> (pre, inloop, arg)` — the three looping sites (fold kernel, twin map, twin fold) hoist the decl + invariant assigns preloop; `examples/emit_sweep.rs` = the differential's deterministic 320-draw emission sweep, no nvcc needed (the local blind-spot closer) | built |
| `ArenaPlan` **(W2)** | `src/arena.rs:ArenaPlan` — the deduced `arena_plan` query's image: `offsets : ArenaKey → u64` (lookup-only) + `capacity : u64`; `ArenaKey = Obj(ObjectId) \| Cell(MorphismId)` (the 1-cell readback temps have no ObjectId — keyed by site) | built |
| `LastUsePlan` **(LU)** | `flow_ir::LastUsePlan` held per fn on `FnEmit.last_use` (computed at construction, the BL7 deduced-query pattern — never re-derived); the twin's per-fn in-place target set `DevEmit.in_place` is computed from it once in `DevEmit::new` (lookup-only, L2) | built |
| `BoundsProof` **(S20)** | `flow_ir::BoundsProof` held per fn on `TrapCaps.proofs` (computed once in `analyze`, read back by `site` via the site's owner fn) and on `DevEmit.proof` (computed at construction — the BL7 deduced-query pattern, never re-derived per site) | built |
| `EmitOpts` **(W2)** | `pub struct EmitOpts { perf_timing: bool }` + `pub fn emit_with_opts` (`src/lib.rs`) — `emit` ≡ `emit_with_opts(default)`, byte-identical | built |

## Trn morphisms (the emitter)

| Model `Trn` | Realised at | State |
| --- | --- | --- |
| `emit` | `src/lib.rs:emit` — gate → strings → prod structs → qualifiers → F3/F7 cells → device section → kernels → host section → main | built |
| L3 capability gate | `src/lib.rs:emit` (≡ `flow-backend-llvm/src/lib.rs:41–56`, same `flow_ir::loop_plan` predicate, same `"nested loops"` feature string) | built |
| `emit_fn` (host) | `src/func.rs:FnEmit::emit` (+ `fn_signature`; HostDevice single-definition form with the `d_trap` shadow parameter — ridden only when the fn can trap, #14) | built |
| `emit_morphism` (host table) | `src/func.rs:FnEmit::emit_morphism` (+ `emit_arith`/`emit_compare`/`emit_call`/`emit_print`; BC2 unsigned-cast wrapping; Div/Mod zero + MIN/−1 guards, elided for literal non-zero/≠ −1 constant divisors (#13, `kernel::const_int_operand`); BC7 strict Phi, `&`/`\|`; **(S21)** `Widen` = the C cast `(target_ct)(val)`, scalar, trap-free; **(S29)** `TimeMs` = the `Unsupported` cell below, the table's one rejecting arm) | built |
| `emit_morphism` (inline table) | `src/kernel.rs:DevEmit::emit_morphism` (+ per-thread `emit_map`/`emit_zip`/`emit_enumerate`/`emit_fold`/`emit_index`/`emit_update`; **(S21)** `emit_iota`/`emit_fill` local for-loops + the `Widen` C-cast arm; guards `*trap = kind+1; return`; #13 elision on the Div/Mod arm; #14 trap args/checks only for trap-capable callees; **(S20)** a `bounds_proof`-proven `emit_index` emits the extension temp + the plain per-thread read — no §3 bounds guard; **(S29)** `TimeMs` joins `Print` in the E2 `unreachable!` arm — tokens never reach the device) | built |
| `emit_kernel` | `src/kernel.rs:emit_kernel` + `Kernel::{map, zip, enumerate, update, index, fold}_kernel`; site names `k{f_ord}_{site_ord}` from `collect_sites`; **#17 dedup** `emit_kernel_set` (each unique kernel text emitted once, first site's name survives; `KernelSet.names` maps every site's launch to the survivor); trap parameter ridden only for trap-capable sites (#14); **(S20)** `index_kernel` drops the §3 bounds guard for a trap-free (proven) site — the proven/unproven split rides #17's shape key, launch counts unchanged | built |
| `iota`/`fill` sites **(S21, ADR-0029 stage 2)** | `src/func.rs:FnEmit::{iota_site, fill_site}` (bulk-site family; arena-member output via `alloc_buffer`; no trap plumbing) + `src/kernel.rs:Kernel::{iota_kernel, fill_kernel}` — the count as a `long long n` launch arg (NOT baked) so equal-shape kernels dedup across different counts under #17; elem ctype derived from the target ty; Fill's value by launch arg, erased-element degenerate graceful | built |
| #12 dead-host-twin skip | `src/kernel.rs:host_reachable` (Call-edge fixpoint from the entry) + `dead_host_twin` — a Twin fn with no host caller: no host prototype/definition (`src/lib.rs`), no sites in `emit_kernel_set` | built |
| #14 trap-capability pre-pass | `src/kernel.rs:TrapCaps::analyze` (syntactic: integer Div/Mod ∨ Index ∨ Update reachable through Call + Map/Fold-body edges, callerward fixpoint; float Div/Mod never counts; **(S20)** an `Index` counts only when the fn's cached `BoundsProof` does NOT clear it — `Update` stays conservative) — `get` (fn) / `site` (kernel; **(S20)** an `Index` site reads its owner fn's cached proof) gate the trap parameter, call-site argument, post-call check, and post-launch readback | built |
| `emit_loop` | host quartet `src/loops.rs:emit_loop`; per-thread quartet `src/kernel.rs:DevEmit::emit_loop`; both under the same `loop_plan` gate, guard-first (ADR-0016); **(LU)** the host quartet's back edge is preceded by `FnEmit::emit_back_edge_frees` where the last-use plan proves the outgoing state dead past the swap | built |
| walk driver-ownership skip | `src/func.rs:FnEmit::walk` + `src/kernel.rs:DevEmit::emit` — `loop_plan` decide∪advance ∪ SCC incidence (llvm `func.rs:280–285` verbatim) | built |
| BC8 qualifier analysis | `src/kernel.rs:Qualifiers::analyze` (`FnQual::{HostOnly, HostDevice, Twin}`; token ⇒ host-only; Twin-ness (bulk ∨ array-return) propagates **callerward** through calls — **(S15)**; array return forces Twin only when it lowers) | built |
| allocation registry | `src/func.rs` `register_alloc` / `remove_alloc` / `emit_frees` + `escape_lvalues` — epilogue frees value-guarded against **every** array-typed component of the return value **(S15)**; **(W2)** zone members are unregistered, the zone release rides the range-test veto (`escaped0`) | built |
| in-place Update **(LU)** | `src/func.rs:FnEmit::update_site` + `in_place_update` (rule 4's plan predicate `dead_after(src, position)` + the consumer half: `merge_family_dead` same-field alias positions, `owned_loop_init` borrowed/extra-used init veto, `fresh_owned_buffer` provenance) — no fresh buffer, the source handle as `out`; twin form `src/kernel.rs:DevEmit::emit_update` (per-thread copy skipped, store into the source array, target slot aliases it, no produced local declared); in-placed sites keep their arena offset reserved-but-unused (recorded v1 simplification) | built |
| back-edge freeing **(LU)** | `src/func.rs:FnEmit::emit_back_edge_frees` (suggestions #2) — at the back edge, frees each array component of the merge's outgoing handle whose producer is a registered allocation, gated on `dead_after(merge, position(LoopBack))` + a fn-owned init component, under the pointer-value init guard `if (merge.fE != init.fE)`; called from `src/loops.rs:emit_loop` before the swap | built |
| `arena_plan` **(W2)** | `src/arena.rs:arena_plan` — one FnScope zone per fn over the non-loop-cone buffer sites (`collect_sites`-mirrored conditions + `literal_pair_counts` shapes + the readback-temp sites), topo-order assignment (L2), 256 B offsets, `capacity = Σ align256(abi bytes)`, `ARENA_MAX_BYTES` = 4 GiB guard ⇒ `EmitError::Unsupported` (rule 4); cone membership = the walk's driver-owned set | built |
| `abi_sizeof` **(W2)** | `src/kernel.rs:abi_sizeof`/`abi_alignof`/`align_up`/`buffer_bytes_of` — C-layout-exact sizes (component alignment + tail padding; residual-1 = the bare component) where `nominal_sizeof` undercounts; per-struct `static_assert(sizeof(FlowProd_*) == N)` emitted by `src/module.rs:emit_prod_structs` (plan §8's box-leg drift guard) | built |
| emission swap **(W2)** | `src/func.rs:FnEmit::alloc_buffer` (the single choke point — zone member ⇒ `{slot} = ({ct}*)(arena0 + {OFF}ULL);` at the old malloc point, cone site ⇒ per-buffer `malloc_buffer` unchanged); fn-entry `cudaMalloc(arena0)`; zone release in `emit_frees` | built |
| #19a instrumentation **(W2)** | `src/func.rs:FnEmit::launch_and_check` (+ `ev_ord` from `collect_sites` order) — `cudaEventRecord(start)` → launch → `Record(stop)`+`Synchronize`+`ElapsedTime` → `printf("FLOW_PERF launch=…")`, the stop BEFORE the trap check; fn-entry `cudaEventCreate` pairs, fn-end `FLOW_PERF total ms=` + `cudaEventDestroy`; `examples/emit.rs --perf` | built |
| literal upload (BC11) | `src/func.rs:FnEmit::emit_literal` (all-const ⇒ `static const` + one H→D memcpy; computed ⇒ plain local; nested ⇒ per-element D→D) | built |

## Trm — the §2 transfer inventory

| # | Crossing | Realised at | State |
| --- | --- | --- | --- |
| 1 | H→D literal upload | `src/func.rs:FnEmit::emit_literal` (`cudaMemcpyHostToDevice`, at the construction site, per execution) | built |
| 2 | H→D launch args | kernel parameter lists (`src/kernel.rs:emit_kernel`) ↔ host launches (`src/func.rs:launch_and_check`) positionally; the trailing `d_trap` rides only trap-capable sites (#14) | built |
| 3 | H→D trap zeroing | `trap_init` (PRELUDE) — `cudaMemset` 4 B, once per process, first in `main` | built |
| 4 | D→H trap flag | `trap_check_after_launch` after **every launch that can trap** (the memcpy is the sync point) — #14: provably trap-free launches skip the readback; capable launches keep the every-launch convention; **(S20)** a `bounds_proof`-proven `Index` launch is trap-free exactly — no flag argument, no readback | built |
| 5 | D→H `Index` result | `src/func.rs:index_site` — 1-cell buffer + `cudaMemcpyDeviceToHost` (`"cudaMemcpy(index)"`) | built |
| 6 | D→H `Fold` acc | `src/func.rs:fold_site` (`"cudaMemcpy(fold)"`); array acc stays on device (result buffer) | built |
| 7 | D→H error status | `cu_check` on every `cudaMalloc`/`cudaMemcpy` return + `cudaGetLastError()` per launch → stderr + exit 102 | built |
| 8 | (no crossing) body-local literal | `src/kernel.rs:DevEmit::emit_literal` — per-thread local initializer | built |
| — | whole-array D→H | **does not exist** (L5's theorem — review-verified by exhaustive grep) | theorem |

## Composition rules → enforcement

| Rule | Enforced/pinned at | State |
| --- | --- | --- |
| L1 oracle parity (R1) | `tests/differential.rs` — 10 examples + 320 closed testgen (256+64, ≥256 non-diverged), raw + rewritten, exit-101 traps, exit-102 = infra (never a data point); **green on the 4090 (S15): 640 compile-and-runs, 0 divergences** | green |
| L2 determinism | ordinal/counter naming only; `emit_twice_byte_equal` pins (10 examples + error paths); HashMap/HashSet never iterated into output | green |
| L3 one shared ceiling | `flow_ir::loop_plan` consumed (never re-derived); `tests/gate.rs::nested_loop_is_unsupported` (hand-built multi-merge graph). **(S29)** op-level totality now has one exception that is NOT a shared ceiling: the `TimeMs` cell below (llvm emits it) | green |
| L4 erasure (`Unit\|IoToken\|Str`) | `src/ty.rs:lower_ty` → `None`; residual rule `residual_arity`/`erased_index`; Str only as host globals (`src/module.rs:collect_str_globals`) | green |
| L5 residency discipline | no whole-array D→H path exists; scalars host, arrays device, handles host | green |

## Recorded `Unsupported` cells (DESIGN §5)

| Cell | Realised at | Pin |
| --- | --- | --- |
| non-canonical loops (multi-merge SCC) | `src/lib.rs:emit` gate | `tests/gate.rs` |
| arrays embedded in products on device **(S15)** | `src/kernel.rs:check_device_product_arrays` (+ `src/ty.rs:{residual_contains_array, tree_contains_product_with_array}`; transient operand aggregates + input parameter excluded — provably per-thread) | `kernel::tests::*_is_unsupported` (×4) + `product_array_cell_does_not_fire_on_supported_shapes` |
| per-thread local array over 16384 B **(S15)** | `src/kernel.rs:MAX_LOCAL_ARRAY_BYTES` + `check_local_array_budget` (twin produced locals) + `check_fold_acc_budgets` (launch-form fold accs) | `kernel::tests::{twin_local_array, fold_kernel_acc}_at_budget…` (×2) |
| fn arena over 4 GiB **(W2)** | `src/arena.rs:ARENA_MAX_BYTES` + `arena_plan`'s capacity guard (rule 4, the F7 precedent — compile-time, never a device query) | `arena::tests::{over_capacity_is_unsupported, at_capacity_emits}` |
| the `time` builtin **(S29)** | `src/func.rs:FnEmit::emit_morphism` — the `Operation::TimeMs` arm returns `EmitError::Unsupported { feature: "the `time` builtin (no CUDA clock seam)", loc }`. The **only** reachable site: the op is token-bearing ⇒ BC8 host-only ⇒ the token-free twins can't contain one (`src/kernel.rs:DevEmit::emit_morphism` pairs it with `Print` in the E2 `unreachable!`) | **none** — no local pin (the S29 wave was llvm-side; a hand-built-graph pin in the `*_is_unsupported` style is the missing row) |

## Bridges (model → code)

| Bridge | Realised at | State |
| --- | --- | --- |
| IR intake (`&CategoryIr`, raw or rewritten) | `src/lib.rs:emit` param; differential runs both | built |
| `flow-rt` extern C (7 prints + `flow_trap`, host-only) | `src/module.rs` PRELUDE `extern "C"` block (exact signatures; `flow_trap` `[[noreturn]]` **(S15)**); linked `libflow_rt.a` + `-lpthread -ldl -lm` (verified as-built, S15) | built |
| nvcc toolchain (remote) | `tests/differential.rs` discovery `$NVCC → $CUDA_HOME/bin/nvcc → which nvcc`; absent ⇒ skip-with-reason (HANDOFF §5.5) | built |
| testgen (`#[path]` include of `flow-rewrite/tests/testgen`) | `tests/differential.rs` (closed-mode only, `TestRunner::deterministic()`) | built |
| `flow_ir::loop_plan` (BL7) | consumed in `src/lib.rs`, `src/func.rs`, `src/kernel.rs`, `src/loops.rs` — never re-derived | built |
| `flow_ir::last_use_plan` (BL7) **(LU)** | consumed in `src/func.rs` (`FnEmit.last_use` → `in_place_update`, `emit_back_edge_frees`) and `src/kernel.rs` (`DevEmit::new`'s in-place set) — the single source of dead/escape/carried facts, never re-derived | built |
| `flow_ir::bounds_proof` (BL7) **(S20)** | consumed in `src/kernel.rs` (`TrapCaps::{analyze, site}` — the Index capability rule; `DevEmit::emit_index` — the in-twin guard elision; `Kernel::index_kernel` — the launch-form guard, via site capability) — the single source of provably-in-bounds facts, never re-derived | built |

## Notes / divergences

- Doc-vs-code deltas the S15 review caught are all reconciled into DESIGN §"As-built (S15)" (trap encoding, the escape guard, the two new cells, BC8 split-emission base case, caller-frees honesty). None outstanding.
- Known recorded-not-fixed: call-result buffers consumed locally are never freed by the caller (bounded leak, reclaimed at process teardown — DESIGN §2's honest amendment); per-iteration loop buffers where the last-use plan can't prove (borrowed/escape shapes) reclaim at teardown (the O(k·n) note — the provable classes are in-placed or back-edge-freed since 2026-07-22 **(LU)**); open-entry array inputs get `nullptr` (emission-totality only, llvm's zeroinitializer rule).
- **(S20)** scope honesty: the one-kernel matmul's map kernel stays trap-capable under the landed `bounds_proof` — the inner fold's `a[i*64+k]`/`b[k*64+j]` reads index by ADR-0027 *captured* values, which the analysis does not range (its Map/Fold body quantification covers the element param only), and the map body's own integer `t / 64` / `t % 64` hold it capable under #14's conservative Div/Mod rule regardless. What trims today is the corpus's constant readbacks. Capture-range flow in the analysis (a flow-ir row, suggestions 14c) would light the map-kernel path with zero further backend change.
