# lower — implementation map

> The functor DESIGN.md ("Categorical model") → code. Each categorical object/morphism →
> the file:symbol that realises it. Keep in sync WITH the code (FRAMEWORK §6.3):
> a new morphism gets a row here in the same change that adds its code.

Firewall (ADR-0014): **Level B only.** Every object/morphism below is one of the
lowering filter's own Rust types — the compiler modeled as `Dat`/`Trn`. Nothing here
describes a Flow *program* as a category (that lives in `docs/architecture/categorical-model.md`);
`flow_syntax::Program` in and `flow_ir::CategoryIr` out are opaque typed `Trm` payloads.

## Objects (Dat) → code
| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `𝒮` — surface AST | the parse-clean input category | `flow_syntax::Program` (opaque `Trm` in) | built |
| `ℐ` — sealed IR | the codomain category | `flow_ir::CategoryIr` (opaque `Trm` out) | built |
| `Ty` — object-map target | `TyKind → Ty` image (scalars/Tuple/Array/Struct) | `flow_ir::Ty` (external) | built |
| `TypeTable` (Pass A) | `name → Ty::Struct`, `BTreeMap` | `crates/flow-lower/src/tys.rs:TypeTable` | built |
| `Effects` (Pass B) | `name → effectful: bool`, `BTreeMap` | `crates/flow-lower/src/effects.rs:Effects` | built |
| `FnSig` (surface signature, `ir_signature` domain) | param/return tys per fn | `crates/flow-lower/src/typing.rs:FnSig` | built |
| `TypeInfo` (Pass D1) | `lit_ty`, block sigs, guard/loop decisions | `crates/flow-lower/src/typing.rs:TypeInfo` | built |
| `IrBuilder` state (Pass C/D2/D3) | per-fn emission context (wire, token, scope) | `crates/flow-lower/src/emit.rs:Emitter` | built |
| `Binding` (symbol table, §3) | `{obj, ty, mutable, decl_seq}` on a scope stack | `crates/flow-lower/src/emit.rs:Binding` (+ `scope.rs:ScopeStack`) | built |
| `Diagnostic*` (L1xxx sum) | the `⊕ Diagnostic*` codomain of the partial functor | `crates/flow-lower/src/diag.rs:LCode` / `diag.rs:diag` | built |

## Morphisms (Trn / relations) → code
| Morphism | Signature | Realising code | State |
| --- | --- | --- | --- |
| `lower` | `𝒮 ⇀ ℐ` | `crates/flow-lower/src/lib.rs:lower` (orchestration) → `crates/flow-lower/src/emit.rs:lower_program` | built |
| `table` (Pass A) | `𝒮 → TypeTable` | `crates/flow-lower/src/tys.rs:build` | built |
| `resolve_ty` (object map) | `TyKind ⇀ Ty` | `crates/flow-lower/src/tys.rs:resolve_tykind` (shared skeleton) via `tys.rs:resolve_ty` (build-time seam) / `tys.rs:TypeTable::resolve` (post-build seam) | built |
| `effects` (Pass B) | `𝒮 → Effects` (I6 cycle → L1008) | `crates/flow-lower/src/effects.rs:analyze` | built |
| `ir_signature` | `FnSig → Ty × Ty` (§6.2 law) | `crates/flow-lower/src/emit.rs:ir_signature` | built |
| `type` (Pass D1) | `𝒮 → TypeInfo` (+ fn sigs) | `crates/flow-lower/src/typing.rs:analyze_fn` / `typing.rs:build_fn_sigs` | built |
| `emit` (Pass D2/D3, morphism map) | `𝒮 ⇀ CategoryIr` (under construction) | `crates/flow-lower/src/emit.rs:build_bodies` (D2) / `emit.rs:Emitter::emit_fn` (D3) | built |
| `binop` | `Binary → Pair*-then-op` (product formation) | `crates/flow-lower/src/emit.rs:Emitter::binop` | built |
| `zip` (builtin, ADR-0018) | `Tuple[A^n,B^n] → Zip` (proj + re-pair; L1606/L1607/L1608) | `crates/flow-lower/src/emit.rs:Emitter::emit_zip` (routed in `emit_expr_stage`; `lib.rs:is_collection_builtin`) | built |
| `enumerate` (builtin, ADR-0018) | `A^n → Enumerate` (single-source; L1609/L1610) | `crates/flow-lower/src/emit.rs:Emitter::emit_enumerate` (routed in `emit_expr_stage`) | built |
| `guard` | `Guard → Phi` (bool / right-folded value-match) | `crates/flow-lower/src/emit.rs:Emitter::emit_phi_guard` → `emit_bool_guard` / `emit_value_match` | built |
| `loop` | `Loop → inline cycle` (LoopEnter/Back/Exit) | `crates/flow-lower/src/emit.rs:Emitter::emit_loop` (+ `emit_routing_guard`, `emit_exit_arm`) | built |
| `seal` (Pass E) | `Builder ⇀ CategoryIr` (entry=main; fail → L1901) | `crates/flow-lower/src/emit.rs:lower_program` (`b.seal(main_id)` tail) | built |
| `ir_err` | `IrError → Diagnostic` (L12xx / L1901) | `crates/flow-lower/src/emit.rs:ir_err` | built |
| `feeds` (TT/EF/TI → BU) | data availability (degenerate `Loc`) | `crates/flow-lower/src/emit.rs:lower_program` (three passed by ref, no copy; `fn_sigs` threads separately as the `ir_signature` domain at Pass C) | built |
| `rejection` (out-of-Core → Diagnostic*) | partial-morphism sites; L1000 backstop | `lib.rs:lower` (Error-item loop) / `tys.rs` (Dynamic/Error arm) / `emit.rs:Emitter::emit_stage`, `emit_expr_stage` | built |

## Composition rules / invariants → where enforced
| Rule (from DESIGN) | Enforced at | Tested at |
| --- | --- | --- |
| Functoriality / staging law: `lower = seal ∘ emit ∘ type ∘ declare ∘ effects ∘ table` (pass order) | `crates/flow-lower/src/lib.rs:lower` (sequential passes; abort-on-diags before emission) | `tests/golden.rs::golden_pipeline` (and all goldens) |
| §6.2 signature-synthesis law (pin 1): pure `A→B`; effectful token-threaded | `crates/flow-lower/src/emit.rs:ir_signature` | `tests/structural.rs::declared_signatures_match_table` |
| Canonical ret-write (pin 2): producer targets `Dest::Ret`; `output()` only for pre-existing | `crates/flow-lower/src/emit.rs:Emitter::lookahead_dest` / `emit_ret_existing` | `tests/golden.rs::golden_pipeline` |
| Negative-literal fold (pin 3): `Neg(lit)` → one negated `Constant`, no `Neg` morphism | `crates/flow-lower/src/emit.rs:Emitter::emit_expr_dest` (Unary arm) / `int_value` | `tests/structural.rs::abs_phi_and_neg_literal` |
| Value-match right-folded Phi chain (pin 4): arm order preserved, default innermost | `crates/flow-lower/src/emit.rs:Emitter::emit_value_match` | — (no dedicated test; shape from ir/DESIGN §16 golden) |
| Loop exit reads merge-state view (pin 5): exit against snapshot, back edge recomputed | `crates/flow-lower/src/emit.rs:Emitter::emit_exit_arm` (+ `ScopeStack::snapshot`) | `tests/structural.rs::sum_to_n_loop_shape` (55-not-66) |
| Partiality: `dom(lower) = Flow-Core`; rejection = functor undefined | each L-code `Err` arm | `tests/rejection.rs` (one test per L-code) |
| Collection builtins pure (LD26): no token ⇒ fanout/body-legal; emit owns L1606–L1610, builder re-derives | `crates/flow-lower/src/lib.rs:is_collection_builtin` / `emit.rs:Emitter::emit_zip`,`emit_enumerate` | `tests/rejection.rs::l1606_*..l1610_*` / `tests/golden.rs::golden_zip_builtin`,`golden_enumerate_builtin`,`fanout_pure_collection_ops_lower_clean` |
| I1 one-source/one-target: arity reified as product `Object` (Pair-then-op) | `crates/flow-lower/src/emit.rs:Emitter::binop` / `pack` | `tests/proptests.rs` (Ok ⇒ `validate` empty) |
| Call-graph acyclicity (I6): cycle → L1008 before declare | `crates/flow-lower/src/effects.rs:analyze` / `find_cycle` | `tests/rejection.rs::l1008_recursive_call` |
| Determinism (§11): `BTreeMap`/`Vec` throughout; no `HashMap` in emission order | all modules (structural discipline) | `tests/golden.rs` (insta snapshots stable) |
| seal Ok ⇒ `validate(&ir).is_empty()` (§1) | `crates/flow-lower/src/emit.rs:lower_program` (seal tail) | `tests/proptests.rs` / `tests/golden.rs` round-trip |

## Notes / divergences
Per FRAMEWORK §6.6 — code and model differences and their resolution.

- **Derives-from-merge tags + loop-guard checks live in D3, not D1.** DESIGN §7.3 lists
  the derives-from-merge tags (for L1503) as "two more D1 products" computed in the
  typing walk; L1502/L1503/L1504 are described as D1 verdicts. The code computes them in
  Pass **D3** emission: the `derived` set is a field of `crates/flow-lower/src/emit.rs:Emitter`
  (seeded/propagated in `emit_expr_dest`, checked in `emit_loop` / `emit_routing_guard`),
  and `LoopNoState`/`LoopGuardShape`/`NestedLoopShape` are raised there, not in `typing.rs`.
  Resolution: the code is correct-by-construction — `derived` keys on live `ObjectId`s that
  exist only once emission has minted the merge projections, so D1 (which has no ObjectIds)
  cannot hold them. The model's D1 attribution is aspirational prose; recommend amending
  DESIGN §7.3 to say "computed during D3 emission over the same dataflow." Non-blocking.
- **`Emitter.obj_ty` is a model-silent stored index.** `crates/flow-lower/src/emit.rs:Emitter.obj_ty`
  (`ObjectId → Ty`) has no model element. It is a stored copy of a deduced morphism
  (object → ty is recoverable from the built graph) kept because `flow_ir`'s builder does
  not expose object tys for branch decisions. Per FRAMEWORK §5 this is a justified store
  (forward navigation the builder does not provide) and the code says so at the field; no
  action, recorded here for completeness.
- **Stale doc comment in `scope.rs`.** The module doc of `crates/flow-lower/src/scope.rs`
  describes the payload as `Binding { obj, ty, mutable, kind }`; the real `Binding`
  (`emit.rs:29`) has `decl_seq`, not `kind`. Cosmetic — the scope stack is generic
  (`ScopeStack<T>`), so the comment is illustrative, not a type contract.
