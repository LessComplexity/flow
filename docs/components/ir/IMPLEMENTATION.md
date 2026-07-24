# ir — implementation map

> The functor DESIGN.md ("Categorical model") → code. Each categorical object/morphism →
> the file:symbol that realises it. Keep in sync WITH the code (FRAMEWORK §6.3):
> a new morphism gets a row here in the same change that adds its code.

Scope: **Level B only** (ADR-0014) — these rows map the *compiler's own* Rust types (`Dat`)
and passes (`Alg`). They are not arrows of Flow-Cat; a Flow program is never described as a
category here. `Loc`/`Trm` are degenerate for this crate (DESIGN §0 scoping truth), so the
model is `Dat` + `Alg` and no placement/transmission rows exist.

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `CategoryIr` | product (sealed graph) | `crates/flow-ir/src/graph.rs:CategoryIr` | built |
| `Object` | product (graph node) | `crates/flow-ir/src/graph.rs:Object` | built |
| `ObjectKind` | discrete cat `{Parameter,Temporary,Constant,Return,LoopMerge}` | `crates/flow-ir/src/graph.rs:ObjectKind` | built |
| `Morphism` | product (dataflow edge; 1 source, 1 target = I1) | `crates/flow-ir/src/graph.rs:Morphism` | built |
| `Operation` | discrete cat + `FuncId` payload on `Call`/`Map`/`Fold` | `crates/flow-ir/src/graph.rs:Operation` | built |
| `FuncDef` | product (one input, one output, `MorphismId*`) | `crates/flow-ir/src/graph.rs:FuncDef` | built |
| `FuncKind` | discrete cat `{Named,MapBody,FoldBody}` | `crates/flow-ir/src/graph.rs:FuncKind` | built |
| `Ty` | sum ⊕ + recursive product (`Tuple`/`Struct`/`Array`); depth ≤64 (I10) | `crates/flow-ir/src/ty.rs:Ty` | built |
| `Value` | sum ⊕ (literal of a `Constant`); `Value::ty` total | `crates/flow-ir/src/ty.rs:Value` | built |
| `SourceLoc` | product `[start,end)` byte span (I11) — a datum, **not** a `Loc` | `crates/flow-ir/src/loc.rs:SourceLoc` | built |
| `ObjectId` / `MorphismId` / `FuncId` | identity atoms (slotmap keys; insertion-ordered ⇒ I12/D2) | `crates/flow-ir/src/graph.rs:ObjectId` (`new_key_type!`) | built |

## Morphisms (Trn / relations) → code

| Morphism | Signature | Realising code | State |
| --- | --- | --- | --- |
| `Object.ty` | `Object → Ty` (total) | `crates/flow-ir/src/graph.rs:Object` (`ty` field) | built |
| `Object.kind` | `Object → ObjectKind` (total) | `crates/flow-ir/src/graph.rs:Object` (`kind` field) | built |
| `Object.value?` | `Object → Value` (partial; `Some ⇔ kind==Constant`, I7) | `crates/flow-ir/src/graph.rs:Object` (`value` field); set only by `crates/flow-ir/src/builder.rs:constant` | built |
| `Object.name?` | `Object → 𝕊` (partial, D4) | `crates/flow-ir/src/graph.rs:Object` (`name` field) | built |
| `Object.loc` | `Object → SourceLoc` (total, I11) | `crates/flow-ir/src/graph.rs:Object` (`loc` field) | built |
| `Morphism.source` / `Morphism.target` | `Morphism → Object` (total; I1 type-level) | `crates/flow-ir/src/graph.rs:Morphism` (`source`/`target` fields) | built |
| `Morphism.op` | `Morphism → Operation` (total; constrains src/tgt per §5.1, I2) | `crates/flow-ir/src/graph.rs:Morphism` (`op` field) | built |
| `Value.ty` | `Value → Ty` (total; underwrites I7) | `crates/flow-ir/src/ty.rs:Value::ty` | built |
| `FuncDef.input` / `FuncDef.output` | `FuncDef → Object` (total) | `crates/flow-ir/src/graph.rs:FuncDef` (`input`/`output` fields) | built |
| `FuncDef.morphisms` | `FuncDef → MorphismId*` (total; insertion = construction order, D5) | `crates/flow-ir/src/graph.rs:FuncDef` (`morphisms` field) | built |
| `CategoryIr.owner` | `ObjectId → FuncId` (total; `try_owner` = partial form validate uses, I6) | `crates/flow-ir/src/graph.rs:CategoryIr::owner` / `CategoryIr::try_owner` | built |
| `CategoryIr.entry` | `CategoryIr → FuncId` (total; always `Named`) | `crates/flow-ir/src/graph.rs:CategoryIr::entry` | built |
| `CategoryIr.in_edges` / `out_edges` | `ObjectId → MorphismId*` (total; `in_edges` shape decides I3) | `crates/flow-ir/src/graph.rs:CategoryIr::in_edges` / `out_edges` | built |
| **§5.1 Operation typing** (the edge-tagging morphism quiver) | per-op `source ty → target ty` (I2) | builder per call: `crates/flow-ir/src/builder.rs:binop` / `unop` / `proj` / `pack` / `index` / `update` / `zip` / `enumerate` / `iota` / `fill` / `phi` / `call` / `map` / `fold` / `print_with` / `time_ms`; validator re-derives: `crates/flow-ir/src/validate.rs:edge_type_ok` | built |
| `Update` op (Trn; ADR-0021) | `(Array{T,n},I,T) → Array{T,n}` — 3-tuple source, slot `i` replaced, OOB traps like `Index` | builder `crates/flow-ir/src/builder.rs:update`; typed in `edge_type_ok`; consumed by lower's `c[i] <- x` (lower §8), interp `eval.rs:update`, rewrite testgen | built |
| `TimeMs` op (Trn ⊸; plan-time-builtin §"Categorical model") | `IoToken → (IoToken × f64)` — Core's second effect and its first clock read. Source is the **bare** token (no internal pair — unlike `Print` there is no value operand); target is a fresh `(IoToken, f64)` the caller `Proj`s apart (slot 0 = rebound token, slot 1 = monotonic ms). Token-threaded, so never reordered/folded/CSE'd/DCE'd — no new invariant machinery, it rides `Print`'s (DESIGN §8) | builder `crates/flow-ir/src/builder.rs:FnBuilder::time_ms`; typed in `crates/flow-ir/src/validate.rs:edge_type_ok`; label `crates/flow-ir/src/mermaid.rs:edge_label`; consumed by lower's `time` stage (lower `emit.rs:FnEmit::emit_time`), interp `eval.rs` `Operation::TimeMs` arm, backend-llvm `func.rs:FnEmit::emit_time_ms`; refused by backend-cuda (`EmitError::Unsupported` — no device clock seam) | built |
| `IrBuilder::declare` (Trn) | `(FuncKind,𝕊,Ty,Ty,SourceLoc) → FuncId ⊕ IrError` (mints Parameter+Return, I9) | `crates/flow-ir/src/builder.rs:IrBuilder::declare` | built |
| `FnBuilder` primitives (Trn; object/morphism constructors) | `(args…,Dest,SourceLoc) → ObjectId ⊕ IrError`; enforce §5.1 + I9 per call | `crates/flow-ir/src/builder.rs:FnBuilder` (`constant`,`proj`,`pack`,`pack_struct`,`pack_array`,`unop`,`binop`,`phi`,`index`,`update`,`zip`,`enumerate`,`iota`,`fill`,`call`,`map`,`fold`,`print`,`println`,`time_ms`,`output`,`begin_loop`,`loop_back`,`loop_exit`,`end_loop`,`finish`) | built |
| `IrBuilder::seal` (Trn) | `(IrBuilder,FuncId) ⇀ CategoryIr ⊕ IrError`; runs global checks; `seal Ok ⇒ validate empty` | `crates/flow-ir/src/builder.rs:IrBuilder::seal` → `crates/flow-ir/src/builder.rs:check_sealed` | built |
| `validate` (independent oracle) | `&CategoryIr → IrViolation*`; empty ⇔ well-formed | `crates/flow-ir/src/validate.rs:validate` | built |
| `topo_order` (deduced) | `CategoryIr × FuncId → MorphismId*` (Kahn; LoopBack non-gating) | `crates/flow-ir/src/algo.rs:CategoryIr::topo_order` | built |
| `sccs` (deduced) | `CategoryIr × FuncId → Vec<ObjectId>*` (iterative Tarjan) | `crates/flow-ir/src/algo.rs:CategoryIr::sccs` | built |
| `loop_structure` (deduced) | `CategoryIr × FuncId → LoopScc*` (backend predicate) | `crates/flow-ir/src/algo.rs:CategoryIr::loop_structure` | built |
| `loop_plan` / `LoopPlan` (deduced; BL7) | `CategoryIr × FuncId × ObjectId → LoopPlan?` — per-merge canonical loop CFG (init/carried/decide/advance feeders + exit attribution), `None` for non-canonical; the one source of truth consumed by interp `run_loop`, rewrite `is_canonical`/replay, backend-llvm | `crates/flow-ir/src/algo.rs:CategoryIr::loop_plan` / `algo.rs:LoopPlan` (re-exported `lib.rs`) | built |
| `last_use` / `LastUsePlan` (deduced; BL7 — plan-last-use §2) | `CategoryIr × FuncId → LastUsePlan` — per-object death positions (greatest topo use position, `Pair`/`Phi` retention pins), escape classification (rule 2, conservative; own-loop-`LoopExit` exemption for carried state), `carried_by` (rule 3, back-route state cone), `dead_after` (rule 4's in-place-`Update` predicate); rule 1 ranking = per-canonical-loop permutation to decide < `LoopExit` < advance < `LoopBack`; total + deterministic, non-canonical degrades (rule 6) | `crates/flow-ir/src/algo.rs:CategoryIr::last_use_plan` / `algo.rs:LastUsePlan` (accessors `position`/`death`/`escapes`/`carried_by`/`dead_after`) | built |
| `bounds_proof` / `BoundsProof` (deduced; BL7 — the S20 kernel-gap analysis) | `CategoryIr × FuncId → BoundsProof` — one `topo_order` interval pass: unsigned ranges from `Constant`s, `Iota` elements, enumerate `.0` indices, literal-ramp arrays, Map/Fold body quantification; an `Index` proven iff the index range ⊂ the array's static size; unknown/wrapping/negative/loop-carried ⇒ not proven | `crates/flow-ir/src/algo.rs:CategoryIr::bounds_proof` / `algo.rs:BoundsProof::proven` (+ `algo.rs:Rng`, `width_max`, `arith_range`, `element_range`, `proj_range`, `pair_slot_source`) | built |
| `tile_plan` / `TilePlan` / `TileSite` / `TileRead` / `TileKSplit` (deduced; BL7 — plan-tile-emission §model, S25; fold k-split S28 — plan-s28-shapes-ladder §model, work item A1) | `CategoryIr × FuncId → TilePlan` — map{fold} sites whose cell chains interleave bit-exactly: 2-D (`t/C`,`t%C`) + 1-D lane modes; reads as affine triples `base + ci·i + ck·k + clane·lane` via a checked recursive walker; S28 adds the partial morphism `TileRead.ksplit? : TileRead → TileKSplit` (§3 consolidation — the same `TileRead` object, one more morphism, NOT a new site type): a fold-body `Div`/`Mod` pair on the fold's counted element (slot `fold_captures + 1`) with one shared literal `div` and `depth % div == 0` binds the derived `kq`/`kr` axes as walker identity leaves — the map-body `tile_split` move one level down, address `… + cq·(k÷div) + cr·(k%div)`; pair bound but both derived coefficients 0 ⇒ `ksplit: None` (pre-S28 records bit-identical); legality = one `clane=0` + one `clane=1` read, all `Index` `bounds_proof`-proven, `Constant` seed, trap-free bodies (`Call`/nested Map/Fold refused — the R1 skip-hole fix), k-split rules (composition table below); partial — unmatched sites absent | `crates/flow-ir/src/algo.rs:CategoryIr::tile_plan` / `algo.rs:{TilePlan,TileSite,TileRead,TileKSplit}` (+ `tile_site`, `tile_fold_shape`, `tile_affine`, `tile_split`, `tile_trap_free`, `tile_iota_size`) | built |
| `emission_plan` / `EmissionPlan` (deduced; S22 plan-minimal-emission WP-A) | `CategoryIr × FuncId → EmissionPlan` — per owned non-constant, non-token object: `Dissolved` (Pair-BUILT product, non-boundary, only Proj/Pair-fed-primitive consumers — Proj-produced tuples never dissolve, the R-NODUP count-drop fix) \| `Named` (boundary: fn output/`Output`, call-arg + bulk-op operand products, loop endpoints + `loop_plan` cones, arrays, guarded producer — non-const-safe int Div/Mod divisor or `bounds_proof`-unproven Index/Update — or effective fanout > 1) \| `Inline` (effective count 1, pure, guard-free); counts taken through dissolved products (transparent redistribution) | `crates/flow-ir/src/algo.rs:CategoryIr::emission_plan` / `algo.rs:EmissionPlan::class` / `algo.rs:EmissionClass` (+ `emission_guarded`, `is_pair_primitive`, `safe_integer_divisor`) | built |
| `path_plan` / `PathPlan` (deduced; BL7 — S24 plan-parallel-orchestrator; S29 clock rules) | `CategoryIr × FuncId → PathPlan` — the execution graph's task DAG (`Task{kind: Split{site,n}\|Seq{morphisms}, deps, rank, trap_min, pinned}` in first-topo-occurrence order) + host-spine `Checkpoint{topo, wait: [WaitEntry{task, threshold}]}` at every token op and at exit (`Some(w)` = watermark ≥ `w`, `None` = completion). Token-bearing morphisms and effectful-loop regions stay on the spine; transitive `fn_trap_capabilities` attributes body/callee traps at the referencing site's topo. **S29 (plan-time-builtin rules 4/5):** `TimeMs` is a checkpoint like `Print`, plus (a) **the fence** — it forces `threshold: None` on every task whose morphisms all start before the read in the SOURCE (`Morphism.loc.start`, not topo — the graph orders pure work against a clock read not at all), and (b) **the host cone** — the forward consumer cone of a clock read is host, since `TimeMs` is the first spine op producing a VALUE and tasks are dispatched before the host writes it (FRAMEWORK §4.5 Law 1: reading data not present at the location — the observed symptom was a NEGATIVE elapsed) | `crates/flow-ir/src/algo.rs:CategoryIr::path_plan` / `algo.rs:{PathPlan,Task,TaskKind,Checkpoint,WaitEntry,TaskId}` (+ `fn_trap_capabilities`; S29 internals `host_value`/`host_cone` in `is_host`, and `task_max_loc` in the checkpoint loop) | built |
| `to_mermaid` / `lint_mermaid` | `&CategoryIr → 𝕊` / `&str → 𝕊*` | `crates/flow-ir/src/mermaid.rs:CategoryIr::to_mermaid` / `crates/flow-ir/src/mermaid.rs:lint_mermaid` | built |

## Composition rules / invariants → where enforced

The §9 invariant ledger. Each row: the model rule, the builder + independent-validator symbols that
enforce its graph-shape clause, and a test that pins it. (DESIGN §11: validate certifies the
graph-shape clause; API-provenance clauses are builder-only.)

| Rule (from DESIGN) | Enforced at | Tested at |
| --- | --- | --- |
| I1 — every morphism has one source, one target | type level: `crates/flow-ir/src/graph.rs:Morphism` | (type-level; exercised throughout) |
| I2 — every morphism satisfies the §5.1 typing table | `crates/flow-ir/src/builder.rs:binop`/`unop`/… (per call); `crates/flow-ir/src/validate.rs:edge_type_ok` | `tests/builder_rejections.rs::binop_eq_bool_ok_but_lt_bool_rejects`; `src/builder/tests.rs::type_mismatch_on_mixed_numeric`; **§5.1 golden oracle** (validate side, op-by-op): `src/validate.rs::typing_table_golden::edge_type_ok_matches_design_5_1` |
| ADR-0018 — `Zip`/`Enumerate` ops + enumerate `n ≤ i32::MAX` bound (`EnumerateIndexOverflow`) | `crates/flow-ir/src/builder.rs:zip`/`enumerate`; `crates/flow-ir/src/validate.rs:edge_type_ok` (typing) + `check_edges` (bound twin) | `src/builder/tests.rs::zip_builds_and_validates`/`enumerate_builds_and_validates`/`zip_result_feeds_map`/`zip_size_mismatch_rejects`/`zip_non_array_rejects`/`enumerate_non_array_rejects`/`enumerate_oversize_rejects`; `src/validate.rs::enumerate_bound_twin::oversize_enumerate_edge_flagged`; golden rows in `typing_table_golden` |
| ADR-0029 — `Iota`/`Fill` ops + static-n rule (`NonStaticCount` builder-side, `IotaCountMismatch` validate twin) | `crates/flow-ir/src/builder.rs:iota`/`fill`/`static_count`; `crates/flow-ir/src/validate.rs:edge_type_ok` (typing) + `check_edges` (static-n twin); mermaid labels `src/mermaid.rs` | `tests/builder_rejections.rs` (5 static-count rejections); `src/validate.rs::iota_count_twin::{drifted_iota_count_flagged,drifted_fill_count_flagged}`; oracle contracts `flow-interp/tests/iota_fill.rs` (3); llvm compile-run parity `flow-backend-llvm/tests/differential.rs::differential_iota_fill` |
| I3 — one-definition rule (in-edge shapes) | `crates/flow-ir/src/builder.rs` (atomic mint); `crates/flow-ir/src/validate.rs:check_in_edge_shapes` / `product_pair_shape_ok` | `src/builder/tests.rs::empty_product_pack`; `tests/builder_rejections.rs::pack_singleton_rejects` |
| I-RET — Return in-edge shapes (writers/slots, no mixing) | `crates/flow-ir/src/builder.rs:check_i_ret`; `crates/flow-ir/src/validate.rs:check_returns` | `src/builder/tests.rs::ret_mixed_writers`/`ret_slot_conflict`/`ret_slot_missing`; `tests/builder_rejections.rs::ret_slot_on_scalar_rejects` |
| I4 — token linearity (+ loop-fork exception, no token in Phi, token-free bodies) | `crates/flow-ir/src/builder.rs:check_token_linearity` (+ `is_loop_fork`/`cone_reaches`); `crates/flow-ir/src/validate.rs:check_tokens` (+ `is_loop_fork`/`cone_classify`) | `src/builder/tests.rs::token_in_phi`/`token_in_map_body`; `tests/builder_rejections.rs::phi_with_token_branch_rejects`/`token_double_consumption_is_unconstructible` |
| I4b — token sink (no dropped tokens; live tail = Return) | `crates/flow-ir/src/builder.rs:check_token_sinks`; `crates/flow-ir/src/validate.rs:check_tokens` | `tests/builder_rejections.rs::token_chain_ends_at_return`/`dangling_final_token_rejects`; `src/builder/tests.rs::token_dropped_at_seal` |
| I4 loop — token-in ⇒ token-out (`TokenNotEscaping`) | `crates/flow-ir/src/builder.rs:check_loops`; `crates/flow-ir/src/validate.rs:check_loops` | `src/builder/tests.rs::token_not_escaping`; `tests/golden_mermaid.rs::golden_h_print_in_loop` |
| I5 — LoopMerge SCC placement (per-edge back/enter/exit) | `crates/flow-ir/src/builder.rs:check_loops` (SCC) + `LoopHandle` local counts; `crates/flow-ir/src/validate.rs:check_loops` | `src/builder/tests.rs::loop_back_outside_scc`; `tests/builder_rejections.rs::second_loopback_from_const_with_real_cond_rejects` |
| I6 — ref graph acyclic; Call→Named; body-kind match; no cross-fn edges | `crates/flow-ir/src/builder.rs:check_reference_acyclic`; `crates/flow-ir/src/validate.rs:check_references` / `check_ownership` | `src/builder/tests.rs::recursive_call`/`call_to_body`/`body_kind_mismatch`/`wrong_builder_cross_function` |
| I7 — `kind==Constant ⇔ value.is_some()`, `value.ty()==ty` | `crates/flow-ir/src/builder.rs:constant` (sole writer); `crates/flow-ir/src/validate.rs:check_objects` | `src/builder/tests.rs::value_ty_mismatch_is_unreachable_constant_is_sound` |
| I8 — no identity morphisms; `Output` only Return-targeted | `crates/flow-ir/src/builder.rs:output` (op not exposed raw); `crates/flow-ir/src/validate.rs:check_edges` | (graph-shape clause in `check_edges`; provenance is builder-only per §11) |
| I9 — Core types only, incl. synthesized tys | `crates/flow-ir/src/builder.rs:intake_ty`; `crates/flow-ir/src/validate.rs:ty_is_core` | `src/builder/tests.rs::non_core_type_bad_int_width`/`non_core_singleton_tuple_ty`/`non_core_zero_field_struct_ty` |
| I9s — `Str` only as Constant ty or the `print()`-internal pair | `crates/flow-ir/src/builder.rs:check_str` (+ `ty::ty_contains_str`); `crates/flow-ir/src/validate.rs:check_str` | `src/builder/tests.rs::str_outside_print`; `tests/builder_rejections.rs::pack_of_str_constant_rejects`/`str_bearing_declared_ty_rejected_at_seal` |
| I10 — `Ty` depth ≤64; no unguarded recursion | `crates/flow-ir/src/ty.rs:ty_depth_ok` (via `builder.rs:intake_ty`) | `src/builder/tests.rs::ty_too_deep`; `src/ty.rs::tests::depth_guard_accepts_at_limit_rejects_past` |
| I11 — every Object/Morphism carries a `SourceLoc` | type level: `crates/flow-ir/src/graph.rs:Object`/`Morphism` (non-optional field) | (type-level) |
| I12 — deterministic iteration (no HashMap; insertion-ordered Vecs) | `crates/flow-ir/src/graph.rs:CategoryIr` (SlotMap/SecondaryMap storage) | `tests/proptest_builder.rs::determinism_same_program_same_output` |
| Struct-name coherence (`StructNameConflict`) | `crates/flow-ir/src/builder.rs:check_struct_name_conflict`; `crates/flow-ir/src/validate.rs:check_struct_names` | `src/builder/tests.rs::struct_name_conflict`; `tests/builder_rejections.rs::two_pixel_decls_with_different_fields_reject_at_seal` |
| Entry is `Named` (`NoEntry`/`EntryNotNamed`) | `crates/flow-ir/src/builder.rs:seal` | `src/builder/tests.rs::no_entry`/`entry_not_named` |
| Headline: `seal Ok ⇒ validate empty` (FRAMEWORK §7.2 dual realization) | `crates/flow-ir/src/builder.rs:check_sealed` ∥ `crates/flow-ir/src/validate.rs:validate` (no shared code) | `tests/proptest_builder.rs::interleaved_calls_seal_implies_valid` |
| topo header-first / LoopBack non-gating (§13) | `crates/flow-ir/src/algo.rs:CategoryIr::topo_order` | `tests/algos.rs::topo_places_header_first_body_before_exit_before_consumer`; `tests/algos.rs::loopback_edges_are_exactly_the_cycle_breakers` |
| loop regions = non-trivial SCCs (D3) | `crates/flow-ir/src/algo.rs:CategoryIr::sccs` / `loop_structure` | `tests/algos.rs::loop_has_exactly_one_nontrivial_scc_with_the_merge`; `loop_structure_single_loop_is_accept_shape`; `nested_loops_multi_merge_one_scc_reject_shape` |
| tile k-split rules (S28; plan-s28-shapes-ladder §"Composition rules"): (1) `ksplit.is_some() ⇒ ck == 0` on that read — affine in raw `k` XOR in `(k÷div, k%div)`, never mixed; (2) `depth % div == 0` (rectangular window) else the pair stays unbound and the site refuses via the walker's `_ => None`; (3) a ksplit site ⇒ the conv emission branch or the untiled fallback, never the affine tile path (cross-component — backend-llvm's dispatch) | (1) `crates/flow-ir/src/algo.rs:tile_fold_shape` (`index_parts` closure); (2) `algo.rs:tile_fold_shape` (pair binding gate); (3) `crates/backends/llvm/src/func.rs:conv_site` + the site-dispatch filter | `tests/algos.rs::tile_conv2d_site_recognized`; `tile_refuses_conv2d_mixed_raw_and_derived_k` (rule 1); `tile_refuses_conv2d_non_rectangular_window` + `tile_refuses_conv2d_divisor_mismatch` (rule 2) |
| `TimeMs` typing (S29; DESIGN §5.1 row + §8): `IoToken → (IoToken, f64)`, bare-token source, `f64` pinned | `crates/flow-ir/src/builder.rs:FnBuilder::time_ms` (source-ty check ⇒ `IrError::TypeMismatch{expected: IoToken}`); `crates/flow-ir/src/validate.rs:edge_type_ok` (independent twin). I4/I4b/I5 need no clause: the token-bearing `(IoToken, f64)` target rides `ty::ty_contains_token` unchanged | `src/builder/tests.rs::time_ms_builds_and_validates` (op shape + `validate` clean) / `time_ms_rejects_non_token_source`; **§5.1 golden oracle** gains 5 `TimeMs` rows (1 legal + 4 rejected: non-token source, non-pair target, wrong slot 0, wrong slot 1) in `src/validate.rs::typing_table_golden::edge_type_ok_matches_design_5_1` |
| `time` scheduling rules (S29; plan-time-builtin composition rules 4/5 — DESIGN §13 `path_plan`): (4) a clock read fences the tasks written entirely above it in the SOURCE; (5) a clock value never leaves the host spine (FRAMEWORK §4.5 Law 1) | `crates/flow-ir/src/algo.rs:CategoryIr::path_plan` — (4) `task_max_loc` vs `morph.loc.start`, forcing `WaitEntry.threshold = None`; (5) the `host_value`/`host_cone` sweep folded into `is_host` | `tests/algos.rs::path_time_ms_fences_only_the_tasks_entirely_before_the_read` (t0 fences generation but not the kernel it opens; t1 fences both; neither fences the post-bracket readout) and `path_time_ms_consumer_cone_stays_on_the_host_spine` (no task holds a morphism touching a clock value) over the shared `time_bracket_fixture` (ascending per-line spans); `path_plan_is_deterministic` extended to the same fixture. Both mutation-verified |
| Mermaid lint (one arrow style; quoted labels; D9) | `crates/flow-ir/src/mermaid.rs:lint_mermaid` | `tests/golden_mermaid.rs::golden_a_add` … `golden_i_phi_chain` (all linted) |

## Notes / divergences

Per FRAMEWORK §6.6 (the model is a specification, not a transcript) — where code carries structure
the categorical model does not name, and the resolution:

- **Deliberate dual realization is not drift.** The `check_*` helpers in `builder.rs`
  (`check_str`, `check_token_linearity`, `check_struct_name_conflict`, `check_reference_acyclic`,
  `check_i_ret`, `check_token_sinks`, `check_loops`, plus `is_loop_fork`/`cone_reaches`) are
  intentionally *parallel, non-shared* to their `validate.rs` twins (`check_str`, `check_tokens`,
  `check_struct_names`, `check_references`, `check_returns`, `check_loops`, `is_loop_fork`/
  `cone_classify`). This is the FRAMEWORK §7.2 / DESIGN §11 "validate twice, honestly" shape — it
  makes `seal Ok ⇒ validate empty` load-bearing rather than a tautology. **Do not consolidate them**
  (a merge would collapse the property). One contract, two realizations, by design.
- **`in_edges` / `out_edges` are stored adjacency deductions.** `in_edges(o) = { m : target(m)=o }`
  is computable by scanning `morphisms`; the SecondaryMaps store it for O(1) navigation, which every
  pass (`sccs`, `topo_order`, `validate`) hot-paths. Consistency mechanism: the single writer
  `builder.rs:add_edge` maintains both, and the graph is append-only-then-sealed, so no post-seal
  drift is possible. This is the §5-sanctioned stored-deduced-morphism; the model already lists them
  as total morphisms rather than deduced (DESIGN morphism table), so no divergence — recorded for the
  future reader who might otherwise "deduce them away."
- **§5.1-notation helpers have no standalone model row.** `ty.rs:Ty::is_numeric`/`is_integer`/
  `is_printable` realise the `N`/`I`/`P` notation of the §5.1 table; `product_arity`/
  `product_arity_u32`/`component_ty` realise its arity/slot mechanics. `product_arity_u32` also carries
  an implementation refinement beyond the model (reviewer F4/SND-3): arity is computed as `u64` and a
  product whose arity exceeds `u32::MAX` is rejected as not slot-addressable, because `Pair{arity:u32}`
  cannot name such a slot. These are the machinery *of* the I2/I-RET rows, not separate morphisms.
- **Internal algorithm helpers are unnamed in the model** and need no row: `algo.rs:func_objects`/
  `has_self_loop`/`object_seq`/`escape_reach`/`TileAffine` (the tile walker's checked coefficient
  accumulator — `add`/`scale` per axis, overflow ⇒ refuse), `validate.rs:reachable`/`build_scc_membership`/`loopback_state_source`/
  `two_tuple`, `builder.rs:mint_object`/`add_edge`/`dest_target`/`emit_to_dest`/`array_size_of`. They
  realise the deduced-morphism and constructor rows above.
- **`TimeMs` has a Mermaid label but no golden snapshot.** `mermaid.rs:edge_label` renders it `"TimeMs"`; the §16 golden set is the fixed (a)–(i) graph catalogue and none of them read a clock, so the label is covered only by the lint rules (quoting/one-arrow-style), not by a snapshot. Deliberate — the dump is exercised end-to-end by the lower/llvm `time` tests; add a golden only if a `time` graph shape ever needs pinning.
- **DESIGN §5's `Operation` listing lags the enum by four items (pre-S29 debt, flagged not fixed).** `graph.rs:Operation` has 34 variants; §5/§5.1 now carry `TimeMs` (added this session, since the model changed) but still omit `Iota`/`Fill` (ADR-0029 stage 1, S20), `Widen` (ADR-0029 amendment, S21) and the ADR-0027 `captures` field on `Map`/`Fold`. The code is the truth for those; the rows in this file are complete. Recorded so a reader does not take §5 as exhaustive.
- **`loc.rs:SourceLoc` utility methods** (`new`/`empty_at`/`len`/`is_empty`) are datum conveniences,
  not `Dat` morphisms — consistent with D8 ("`SourceLoc` is a datum, not a `Loc`").
- **Out of increment 1 (planned, no code yet):** JSON serialization (§5.3), any mutation/rewrite API
  (P4), bifunctor-image tagging (§9.5). Deliberately absent per DESIGN §0 "Out"; no rows written.
