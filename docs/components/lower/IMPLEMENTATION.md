# lower — implementation map

> The functor DESIGN.md ("Categorical model") → code. Each categorical object/morphism →
> the file:symbol that realizes it. Keep in sync WITH the code (FRAMEWORK §6.3):
> a new morphism gets a row here in the same change that adds its code.

Firewall (ADR-0014): **Level B only.** Every object/morphism below is one of the
lowering filter's own Rust types — the compiler modeled as `Dat`/`Trn`. Nothing here
describes a Mapal *program* as a category (that lives in `docs/architecture/categorical-model.md`);
`mapal_syntax::Program` in and `mapal_ir::CategoryIr` out are opaque typed `Trm` payloads.

## Objects (Dat) → code
| Object | Form / shape | Realized at | State |
| --- | --- | --- | --- |
| `𝒮` — surface AST | the parse-clean input category | `mapal_syntax::Program` (opaque `Trm` in) | built |
| `ℐ` — sealed IR | the codomain category | `mapal_ir::CategoryIr` (opaque `Trm` out) | built |
| `Ty` — object-map target | `TyKind → Ty` image (scalars/Tuple/Array/Struct) | `mapal_ir::Ty` (external) | built |
| `TypeTable` (Pass A) | `name → Ty::Struct`, `BTreeMap` | `crates/mapal-lower/src/tys.rs:TypeTable` | built |
| `Effects` (Pass B) | `name → effectful: bool`, `BTreeMap` | `crates/mapal-lower/src/effects.rs:Effects` | built |
| `FnSig` (surface signature, `ir_signature` domain) | param/return tys per fn | `crates/mapal-lower/src/typing.rs:FnSig` | built |
| `TypeInfo` (Pass D1) | `lit_ty`, block sigs, guard/loop decisions | `crates/mapal-lower/src/typing.rs:TypeInfo` | built |
| `IrBuilder` state (Pass C/D2/D3) | per-fn emission context (wire, token, scope) | `crates/mapal-lower/src/emit.rs:Emitter` | built |
| `Binding` (symbol table, §3) | `{obj, ty, mutable, decl_seq}` on a scope stack | `crates/mapal-lower/src/emit.rs:Binding` (+ `scope.rs:ScopeStack`) | built |
| `Diagnostic*` (L1xxx sum) | the `⊕ Diagnostic*` codomain of the partial functor | `crates/mapal-lower/src/diag.rs:LCode` / `diag.rs:diag` | built |

## Morphisms (Trn / relations) → code
| Morphism | Signature | Realizing code | State |
| --- | --- | --- | --- |
| `lower` | `𝒮 ⇀ ℐ` | `crates/mapal-lower/src/lib.rs:lower` (orchestration) → `crates/mapal-lower/src/emit.rs:lower_program` | built |
| `table` (Pass A) | `𝒮 → TypeTable` | `crates/mapal-lower/src/tys.rs:build` | built |
| `resolve_ty` (object map) | `TyKind ⇀ Ty` | `crates/mapal-lower/src/tys.rs:resolve_tykind` (shared skeleton) via `tys.rs:resolve_ty` (build-time seam) / `tys.rs:TypeTable::resolve` (post-build seam) | built |
| `effects` (Pass B) | `𝒮 → Effects` (I6 cycle → L1008) | `crates/mapal-lower/src/effects.rs:analyze` | built |
| `ir_signature` | `FnSig → Ty × Ty` (§6.2 law) | `crates/mapal-lower/src/emit.rs:ir_signature` | built |
| `type` (Pass D1) | `𝒮 → TypeInfo` (+ fn sigs) | `crates/mapal-lower/src/typing.rs:analyze_fn` / `typing.rs:build_fn_sigs` | built |
| `emit` (Pass D2/D3, morphism map) | `𝒮 ⇀ CategoryIr` (under construction) | `crates/mapal-lower/src/emit.rs:build_bodies` (D2) / `emit.rs:Emitter::emit_fn` (D3) | built |
| `binop` | `Binary → Pair*-then-op` (product formation) | `crates/mapal-lower/src/emit.rs:Emitter::binop` | built |
| `zip` (builtin, ADR-0018) | `Tuple[A^n,B^n] → Zip` (proj + re-pair; L1606/L1607/L1608) | `crates/mapal-lower/src/emit.rs:Emitter::emit_zip` (routed in `emit_expr_stage`; `lib.rs:is_collection_builtin`) | built |
| `enumerate` (builtin, ADR-0018) | `A^n → Enumerate` (single-source; L1609/L1610) | `crates/mapal-lower/src/emit.rs:Emitter::emit_enumerate` (routed in `emit_expr_stage`) | built |
| `time` (builtin, effectful; plan-time-builtin) | `IoToken → (IoToken × f64)` — the wire-LESS stage: `() -> time` only; token in, `(token, ms)` out, split by two Projs | `crates/mapal-lower/src/emit.rs:Emitter::emit_time` (routed in `emit.rs:Emitter::emit_expr_stage`; the `()` head seeds `cur = None` in `emit.rs:Emitter::emit_chain`; `lib.rs:is_time_builtin`) | built |
| `iota`/`fill` (builtins, ADR-0029) | `i32 n → Iota`; `(T×i32 n) → Fill` via the internal pair (count = positive literal ≤ i32::MAX; L1612/L1613) | `crates/mapal-lower/src/emit.rs:Emitter::{emit_iota, emit_fill, static_count_arg}` (routed at `ExprKind::Call` in `emit_expr_dest`; typing's Call arm synthesizes `WTy::Array`) | built |
| `guard` | `Guard → Phi` (bool / right-folded value-match) | `crates/mapal-lower/src/emit.rs:Emitter::emit_phi_guard` → `emit_bool_guard` / `emit_value_match` | built |
| `loop` | `Loop → inline cycle` (LoopEnter/Back/Exit) | `crates/mapal-lower/src/emit.rs:Emitter::emit_loop` (+ `emit_routing_guard`, `emit_exit_arm`) | built |
| `seq` (ADR-0019) | `SeqBlock → statement thread` (no IR footprint; enclosing scope; tail = value; L1611 continues-no-tail) | `crates/mapal-lower/src/emit.rs:Emitter::emit_seq_block` (routed in `emit_stage`); D1 `typing.rs:stage` (SeqBlock arm); effects `effects.rs:chain` (SeqBlock arm). Enclosing-scope sub-pass descent (§8.10): `emit.rs:scan_chain` (phi-arm), `emit.rs:collect_assigns_chain` (loop carried-set), `typing.rs:capture_chain` (map/fold capture) each recurse both `Fanout` branches and `SeqBlock` bodies; effectful-B-present return-position tail lowers under `ChainCtx::RetValue` so tail-less seq → L1611 uniformly | built |
| `element update` (ADR-0021; LD27) | `c[i] <- x → Update(cur,i,x)`-then-rebind (indexed `BindStmt`; pure, no token) | `crates/mapal-lower/src/emit.rs:Emitter` — `StmtKind::Bind` `index.is_some()` path (`update()` builder call + `rebind()`, never `bind_new`); carried-set + Phi-arm wiring: `emit.rs:collect_assigns_stmt` / `emit.rs:scan_stmt`; typing `typing.rs:stage` (indexed-bind arm, unify with `array_elem_wty`) + `typing.rs:capture_stmt` (indexed-bind capture branch) | built |
| `seal` (Pass E) | `Builder ⇀ CategoryIr` (entry=main; fail → L1901) | `crates/mapal-lower/src/emit.rs:lower_program` (`b.seal(main_id)` tail) | built |
| `ir_err` | `IrError → Diagnostic` (L12xx / L1901) | `crates/mapal-lower/src/emit.rs:ir_err` | built |
| `feeds` (TT/EF/TI → BU) | data availability (degenerate `Loc`) | `crates/mapal-lower/src/emit.rs:lower_program` (three passed by ref, no copy; `fn_sigs` threads separately as the `ir_signature` domain at Pass C) | built |
| `rejection` (out-of-Core → Diagnostic*) | partial-morphism sites; L1000 backstop | `lib.rs:lower` (Error-item loop) / `tys.rs` (Dynamic/Error arm) / `emit.rs:Emitter::emit_stage`, `emit_expr_stage` | built |

## Composition rules / invariants → where enforced
| Rule (from DESIGN) | Enforced at | Tested at |
| --- | --- | --- |
| Functoriality / staging law: `lower = seal ∘ emit ∘ type ∘ declare ∘ effects ∘ table` (pass order) | `crates/mapal-lower/src/lib.rs:lower` (sequential passes; abort-on-diags before emission) | `tests/golden.rs::golden_pipeline` (and all goldens) |
| §6.2 signature-synthesis law (pin 1): pure `A→B`; effectful token-threaded | `crates/mapal-lower/src/emit.rs:ir_signature` | `tests/structural.rs::declared_signatures_match_table` |
| Canonical ret-write (pin 2): producer targets `Dest::Ret`; `output()` only for pre-existing | `crates/mapal-lower/src/emit.rs:Emitter::lookahead_dest` / `emit_ret_existing` | `tests/golden.rs::golden_pipeline` |
| Negative-literal fold (pin 3): `Neg(lit)` → one negated `Constant`, no `Neg` morphism | `crates/mapal-lower/src/emit.rs:Emitter::emit_expr_dest` (Unary arm) / `int_value` | `tests/structural.rs::abs_phi_and_neg_literal` |
| Value-match right-folded Phi chain (pin 4): arm order preserved, default innermost | `crates/mapal-lower/src/emit.rs:Emitter::emit_value_match` | — (no dedicated test; shape from ir/DESIGN §16 golden) |
| Loop exit reads merge-state view (pin 5): exit against snapshot, back edge recomputed | `crates/mapal-lower/src/emit.rs:Emitter::emit_exit_arm` (+ `ScopeStack::snapshot`) | `tests/structural.rs::sum_to_n_loop_shape` (55-not-66) |
| Partiality: `dom(lower) = Mapal-Core`; rejection = functor undefined | each L-code `Err` arm | `tests/rejection.rs` (one test per L-code) |
| Collection builtins pure (LD26): no token ⇒ fanout/body-legal; emit owns L1606–L1610, builder re-derives | `crates/mapal-lower/src/lib.rs:is_collection_builtin` / `emit.rs:Emitter::emit_zip`,`emit_enumerate` | `tests/rejection.rs::l1606_*..l1610_*` / `tests/golden.rs::golden_zip_builtin`,`golden_enumerate_builtin`,`fanout_pure_collection_ops_lower_clean` |
| `seq` (ADR-0019 §8.10): no IR footprint — token thread orders statements; enclosing scope (bindings escape); tail = value; L1611 continues-no-tail; effectful seq in a `Plain` fanout → L1305 parity (no effect escape); sub-passes gating on unconditional execution descend into fanout **and** seq (phi-arm L1404/L1405/L1408, loop carried-set, map/fold L1108 capture) | `crates/mapal-lower/src/emit.rs:Emitter::emit_seq_block`, `scan_chain`, `collect_assigns_chain`, `typing.rs:capture_chain` | `tests/golden.rs::golden_seq_two_printlns`,`golden_seq_mid_chain`,`golden_seq_return_tail`,`golden_seq_explicit_ret` / `tests/rejection.rs::l1611_seq_continues_no_tail`,`l1611_seq_return_position_no_tail`,`l1611_effectful_seq_return_position_no_tail`,`seq_return_position_valued_effectful_lowers_clean`,`l1404_effectful_seq_in_phi_arm`,`l1404_effectful_fanout_in_phi_arm`,`l1108_capture_in_seq_in_map_body`,`effectful_seq_in_fanout_join_rejected`,`empty_seq_lowers_clean`,`seq_bindings_escape_to_enclosing_scope`,`seq_headless_statements_seed_from_input` / `mapal-interp/tests/acceptance.rs::sum_to_n_seq_wrapped_reassign_value_contract` |
| `time` is an effect site (plan-time-builtin rule 1): it threads the IO token exactly like `print`, so the same four seams classify it — reserved name (L1009), direct effect in the call-graph walk, typing row `→ f64`, banned in map/fold bodies (L1605). `()` is not a value (L1301) and `time` takes no wire (L1302) — no new L-code | `crates/mapal-lower/src/lib.rs:is_time_builtin` (one predicate, LD25's rule) at `lib.rs:is_reserved`, `effects.rs:NameWalk::chain`, `typing.rs:stage` (f64) / `typing.rs:body_effect_span` (L1605) / `typing.rs:capture_chain`, `emit.rs:Emitter::{emit_expr_stage, emit_time, stage_writes_value}`, `emit.rs:Emitter::emit_expr_dest` (`ExprKind::Unit` → L1301) | `tests/structural.rs::time_bracket_types_f64`,`time_reads_thread_the_io_token` / `tests/rejection.rs::l1009_reserved_time`,`l1301_unit_as_value`,`l1302_time_with_a_wire`,`l1605_body_time` |
| I1 one-source/one-target: arity reified as product `Object` (Pair-then-op) | `crates/mapal-lower/src/emit.rs:Emitter::binop` / `pack` | `tests/proptests.rs` (Ok ⇒ `validate` empty) |
| Call-graph acyclicity (I6): cycle → L1008 before declare | `crates/mapal-lower/src/effects.rs:analyze` / `find_cycle` | `tests/rejection.rs::l1008_recursive_call` |
| Determinism (§11): `BTreeMap`/`Vec` throughout; no `HashMap` in emission order | all modules (structural discipline) | `tests/golden.rs` (insta snapshots stable) |
| seal Ok ⇒ `validate(&ir).is_empty()` (§1) | `crates/mapal-lower/src/emit.rs:lower_program` (seal tail) | `tests/proptests.rs` / `tests/golden.rs` round-trip |

## Notes / divergences
Per FRAMEWORK §6.6 — code and model differences and their resolution.

- **Derives-from-merge tags + loop-guard checks live in D3, not D1.** DESIGN §7.3 lists
  the derives-from-merge tags (for L1503) as "two more D1 products" computed in the
  typing walk; L1502/L1503/L1504 are described as D1 verdicts. The code computes them in
  Pass **D3** emission: the `derived` set is a field of `crates/mapal-lower/src/emit.rs:Emitter`
  (seeded/propagated in `emit_expr_dest`, checked in `emit_loop` / `emit_routing_guard`),
  and `LoopNoState`/`LoopGuardShape`/`NestedLoopShape` are raised there, not in `typing.rs`.
  Resolution: the code is correct-by-construction — `derived` keys on live `ObjectId`s that
  exist only once emission has minted the merge projections, so D1 (which has no ObjectIds)
  cannot hold them. The model's D1 attribution is aspirational prose; recommend amending
  DESIGN §7.3 to say "computed during D3 emission over the same dataflow." Non-blocking.
- **`Emitter.obj_ty` is a model-silent stored index.** `crates/mapal-lower/src/emit.rs:Emitter.obj_ty`
  (`ObjectId → Ty`) has no model element. It is a stored copy of a deduced morphism
  (object → ty is recoverable from the built graph) kept because `mapal_ir`'s builder does
  not expose object tys for branch decisions. Per FRAMEWORK §5 this is a justified store
  (forward navigation the builder does not provide) and the code says so at the field; no
  action, recorded here for completeness.
- **All four effect detectors learn `time` (S29 — found in reconcile, fixed in the same
  session).** `time` is an effect by construction (it consumes/produces the IO token —
  plan-time-builtin rule 1), and four seams ask "is this stage an effect?":
  `effects.rs:NameWalk::chain`, `typing.rs:body_effect_span` (L1605),
  `emit.rs:scan_phi_arm` (L1404) and `emit.rs:effect_chain` (the `loop_body_has_effect`
  predicate). The builtin first landed in two of them; the other two still tested
  `is_print_builtin` alone, which lowered a Phi-arm clock read clean (where `print` is
  L1404) and — the real defect — left the token out of a loop's carried set `U`, hoisting
  a loop-body `TimeMs` **out of the cycle**: one timestamp instead of one per iteration,
  with `validate` empty (the ATK-02 failure mode `effect_chain`'s own doc comment names).
  All four now call the shared predicate; pinned by
  `llvm/tests/golden_ll.rs:time_inside_a_loop_stays_inside_the_loop`, which asserts the
  emitted call sits between the loop header and the back edge. The structural fix — one
  `stage_is_effect` helper so a fifth effect builtin cannot miss a seam — stays open as
  suggestions.md #3.
- **Stale doc comment in `scope.rs`.** The module doc of `crates/mapal-lower/src/scope.rs`
  describes the payload as `Binding { obj, ty, mutable, kind }`; the real `Binding`
  (`emit.rs:29`) has `decl_seq`, not `kind`. Cosmetic — the scope stack is generic
  (`ScopeStack<T>`), so the comment is illustrative, not a type contract.
