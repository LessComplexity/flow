# interp — implementation map

> The functor DESIGN.md ("Categorical model") → code. Each categorical object/morphism →
> the file:symbol that realizes it. Keep in sync WITH the code (FRAMEWORK §6.3):
> a new morphism gets a row here in the same change that adds its code.

Firewall (ADR-0014): **Level B only** — these are the interpreter's own Rust types and its
one `eval` pass. Mapal programs are *not* modeled as categories here.

## Objects (Dat) → code
| Object | Form / shape | Realized at | State |
| --- | --- | --- | --- |
| `RValue` | the runtime value domain (`Dat`) | `crates/mapal-interp/src/value.rs:RValue` | built |
| `Scalar(mapal_ir::Value)` | scalar payload (i32/i64/u8/f32/f64/bool/str) | `crates/mapal-interp/src/value.rs:RValue` (variant `Scalar`) | built |
| `Tuple(RValue*)` | anonymous product, arity ≥ 2 | `crates/mapal-interp/src/value.rs:RValue` (variant `Tuple`) | built |
| `Struct(name, fields)` | named product, declared field order | `crates/mapal-interp/src/value.rs:RValue` (variant `Struct`) | built |
| `Array(RValue*)` | fixed-size, free-monoid-shaped | `crates/mapal-interp/src/value.rs:RValue` (variant `Array`) | built |
| `Token(𝕊)` | the world token / output log (§5) | `crates/mapal-interp/src/value.rs:RValue` (variant `Token`) | built |
| `Unit` | the Unit witness (mirrors `Ty::Unit`) | `crates/mapal-interp/src/value.rs:RValue` (variant `Unit`) | built |
| `Outcome` | the public run result sum | `crates/mapal-interp/src/value.rs:Outcome` | built |
| `Done(RValue)` | halted with a value | `crates/mapal-interp/src/value.rs:Outcome` (variant `Done`) | built |
| `Diverged` | budget exhausted (E1) | `crates/mapal-interp/src/value.rs:Outcome` (variant `Diverged`) | built |
| `Trapped(TrapKind)` | a defined runtime trap | `crates/mapal-interp/src/value.rs:Outcome` (variant `Trapped`) | built |
| `TrapKind` | `{DivZero, IndexOob}` (ADR-0013) | `crates/mapal-interp/src/value.rs:TrapKind` | built |
| `Abort` | internal error monad `Diverged \| Trapped` threaded by `?` (§1) | `crates/mapal-interp/src/value.rs:Abort` | built |
| `RunResult` | `{ outcome, output }` (§7) | `crates/mapal-interp/src/lib.rs:RunResult` | built |

## Morphisms (Trn / relations) → code
| Morphism | Signature | Realizing code | State |
| --- | --- | --- | --- |
| `scalar?` | `RValue → mapal_ir::Value` | `crates/mapal-interp/src/eval.rs:scalar` | built |
| `tuple?` | `RValue → RValue*` | `crates/mapal-interp/src/eval.rs:component` | built |
| `struct?` | `RValue → (𝕊 × RValue)*` | `crates/mapal-interp/src/eval.rs:component` | built |
| `array?` | `RValue → RValue*` | `crates/mapal-interp/src/eval.rs:component` | built |
| `token?` | `RValue → 𝕊` | `crates/mapal-interp/src/eval.rs:print_op` · `crates/mapal-interp/src/lib.rs:run` | built |
| `done?` | `Outcome → RValue` | `crates/mapal-interp/src/lib.rs:run` (match `Outcome::Done`) | built |
| `trapped?` | `Outcome → TrapKind` | `crates/mapal-interp/src/value.rs:Abort` (into_outcome; `Outcome::Trapped`) | built |
| `eval` | `(CategoryIr × FuncId × RValue × Budget) → Outcome` — **the one `Trn`** | `crates/mapal-interp/src/eval.rs:eval_fn` | built |
| `render` | `mapal_ir::Value → 𝕊` | `crates/mapal-interp/src/value.rs:render` | built |
| `eval_morphism` (§3 per-op dispatch) | `Morphism → env′` | `crates/mapal-interp/src/eval.rs:eval_morphism` | built |
| `Add/Sub/Mul/Div/Mod` (§3) | `(N,N) → N` | `crates/mapal-interp/src/eval.rs:arith` | built |
| `Eq/Neq/Lt/Gt/Le/Ge` (§3) | `(A,A) → Bool` | `crates/mapal-interp/src/eval.rs:compare` (`num_lt`/`num_le`) | built |
| `And/Or` (§3) | `(Bool,Bool) → Bool` | `crates/mapal-interp/src/eval.rs:logic` | built |
| `Neg` (§3) | `N → N` (IEEE fneg for floats) | `crates/mapal-interp/src/eval.rs:neg` | built |
| `Index` (§3) | `(Array, I) → T` (OOB ⇒ trap) | `crates/mapal-interp/src/eval.rs:index` | built |
| `Zip` (§3; ADR-0018) | `([A;n],[B;n]) → [(A,B);n]` elementwise pair | `crates/mapal-interp/src/eval.rs:eval_morphism` (`Zip` arm) · `as_array` | built |
| `Enumerate` (§3; ADR-0018) | `[A;n] → [(i32,A);n]` (index pinned `i32`) | `crates/mapal-interp/src/eval.rs:eval_morphism` (`Enumerate` arm) | built |
| `Iota`/`Fill` (ADR-0029, stage 1) | `n → [i32;n]` of `0..n-1` (count = `Constant` source); `(x,n) → [x;n]` (count = internal pair's slot-1 `Constant`) | `crates/mapal-interp/src/eval.rs:eval_morphism` (`Iota`/`Fill` arms) | built (oracle contracts `tests/iota_fill.rs`) |
| `Update` (§3; ADR-0021) | `(Array{T,n}, I, T) → Array{T,n}` (slot `i` replaced; OOB ⇒ `Trapped(IndexOob)`; pure) | `crates/mapal-interp/src/eval.rs:eval_morphism` (`Update` arm) · `eval.rs:update` | built |
| `Print{newline}` (§3/§5) | `(IoToken, P) → IoToken` | `crates/mapal-interp/src/eval.rs:print_op` | built |
| `TimeMs` (§3/§5; plan-time-builtin) | `IoToken → (IoToken, f64)` — token through slot 0, ms against the process epoch in slot 1 | `crates/mapal-interp/src/eval.rs:eval_morphism` (`TimeMs` arm) · `crates/mapal-interp/src/eval.rs:time_epoch` | built |
| product assembly (§2) | `arity × Pair{slot} → Tuple/Struct/Array` | `crates/mapal-interp/src/eval.rs:stage_pair` · `finalize_product` | built |
| `run_loop` (§4 driver) | `LoopMerge → env(exit objects)` | `crates/mapal-interp/src/loops.rs:run_loop` | built |
| loop-layout derivation (§4; BL7) | `merge → LoopPlan` (decide/advance split) — **delegated to mapal-ir** (S13: the local `derive_plan`/`LoopPlan` were deleted, one source of truth) | `mapal_ir::CategoryIr::loop_plan` (called in `crates/mapal-interp/src/loops.rs:run_loop`) | built |
| in-SCC set (§2) | `FuncId → 𝒫(ObjectId)` | `crates/mapal-interp/src/eval.rs:build_in_scc` | built |
| `run` (entry protocol §7) | `(CategoryIr × Budget) → RunResult` | `crates/mapal-interp/src/lib.rs:run` | built |
| `eval_call` (§8) | `(CategoryIr × FuncId × RValue × Budget) → Outcome` | `crates/mapal-interp/src/lib.rs:eval_call` | built |
| into_outcome (public lift §1/§7) | `Abort → Outcome` | `crates/mapal-interp/src/value.rs:Abort` (`into_outcome`) | built |
| IR intake (bridge) | `mapal_ir::CategoryIr → (borrowed &)` | `crates/mapal-interp/src/eval.rs:eval_fn` (`ir: &CategoryIr`) | built |
| value seed (bridge, deduced) | `mapal_ir::Value → RValue::Scalar` | `crates/mapal-interp/src/eval.rs:eval_fn` (Constant seed) · `crates/mapal-interp/src/lib.rs:run` | built |

## Composition rules / invariants → where enforced
| Rule (from DESIGN) | Enforced at | Tested at |
| --- | --- | --- |
| **C-interp-1** totality/E1 — `eval` total into `Outcome`, no panic/hang | `crates/mapal-interp/src/eval.rs:EvalCtx::spend` · `crates/mapal-interp/src/lib.rs:eval_call` (lift) | `tests/divergence.rs::constant_true_loop_diverges_and_returns` · `tests/divergence.rs::sum_to_n_budget_boundary` |
| **C-interp-2** oracle equivalence (`eval∘r ≅ eval`) | *not in interp* — discharged by `rewrite`/`backend` (P4+) | — (external) |
| **C-interp-3** effect order = dataflow / E2 | `crates/mapal-interp/src/eval.rs:print_op` (token threads through `env`); topo walk in `eval_fn` | `tests/determinism.rs::deterministic_*` · `tests/acceptance.rs::golden_fanout` |
| Budget (IN1) — decrement once per morphism, `0 ⇒ Diverged` | `crates/mapal-interp/src/eval.rs:EvalCtx::spend` (called in `eval_morphism`) | `tests/divergence.rs::sum_to_n_budget_boundary` |
| Traps (ADR-0013) — int ÷/% 0 ⇒ DivZero; OOB Index ⇒ IndexOob; float IEEE | `crates/mapal-interp/src/eval.rs:arith` · `crates/mapal-interp/src/eval.rs:index` | `tests/traps.rs::integer_div_by_zero_traps` · `tests/traps.rs::index_at_n_traps` · `tests/traps.rs::index_negative_traps` · `tests/traps.rs::float_div_by_zero_is_done_not_trap` |
| The 55 contract / D7 — exit reads the exit-iteration merge state (guard-first, ADR-0016) | `crates/mapal-interp/src/loops.rs:run_loop` (decide→guard→advance) · `mapal_ir::loop_plan` (decide/advance split) | `tests/acceptance.rs::sum_to_n_value_contract` · `tests/acceptance.rs::golden_sum_to_n` |
| Guard-first (ADR-0016) — advance set not evaluated on the exit step | `mapal_ir::CategoryIr::loop_plan` (`plan.decide_order`/`plan.advance_order`) · `crates/mapal-interp/src/loops.rs:run_loop` | `tests/acceptance.rs::golden_fir` · `tests/acceptance.rs::golden_countdown` |
| One definition per object / product seal (ir I3) — `env[o]` written once, product `arity` times then sealed | `crates/mapal-interp/src/eval.rs:stage_pair` (finalize on last slot) | `tests/acceptance.rs::golden_sepia` · `tests/acceptance.rs::sepia_input_ty_is_named_product` |
| Determinism (E2 / §10) — no `HashMap`; `SecondaryMap`+`Vec` only | `crates/mapal-interp/src/eval.rs:EvalCtx` (env/staging are `SecondaryMap`) | `tests/determinism.rs::deterministic_*` |
| Entry protocol (§7) — IoToken ⇒ seed `Token("")`; Unit ⇒ `RValue::Unit`, output "" | `crates/mapal-interp/src/lib.rs:run` | `tests/acceptance.rs::golden_*` (all six) |
| Newline (ADR-0015) — `print` raw, `println` appends `\n` | `crates/mapal-interp/src/eval.rs:print_op` (`if newline`) | `tests/acceptance.rs::golden_pipeline` · `tests/acceptance.rs::golden_fanout` |
| Clock (§5, S29) — one process-lifetime epoch ⇒ two reads finite and non-decreasing; the bracketed work still computes | `crates/mapal-interp/src/eval.rs:time_epoch` (`OnceLock<Instant>`, same clock as mapal-rt's `mapal_time_ms`) | `tests/acceptance.rs::time_brackets_are_monotone_and_finite` |
| Float render (IN4) — Rust shortest round-trip Display | `crates/mapal-interp/src/value.rs:render` | `tests/acceptance.rs::golden_fir` · `tests/acceptance.rs::golden_sepia` |
| Out-of-M1 loop shapes error, not miscompute (§4) | `mapal_ir::CategoryIr::loop_plan` returns `None` for non-canonical shapes; `crates/mapal-interp/src/loops.rs:run_loop` unwraps (M1-guaranteed) | — (no fixture; `loop_plan` encodes the single merge / one LoopBack / one LoopExit predicate, lower OQ7 never generates a non-canonical one) |
| Zip/Enumerate denotation (ADR-0018) — elementwise pairs; `(i as i32, x)`; total/pure, legal under fanout | `crates/mapal-interp/src/eval.rs:eval_morphism` (`Zip`/`Enumerate` arms) | `tests/zip_enumerate.rs::zip_add_value_contract` (c[0]=100, c[15]=115) · `::enumerate_indices_contract` · `::enumerate_under_fanout` |

## Notes / divergences
Resolution per FRAMEWORK §6.6 (the model is a specification, not a transcript).

- **`EvalCtx` bundles `env` + `staging` + `ir` + `f`.** DESIGN §2 names only `env`
  (`SecondaryMap<ObjectId, RValue>`); the per-object staging buffers of the §2 product
  assembly and the borrowed `ir`/`f` are folded into one activation struct
  (`crates/mapal-interp/src/eval.rs:EvalCtx`). Implementation grouping, no model object added
  — **consistent** with the model.
- **`LoopPlan` is a derived record with no named model object.** DESIGN §4 says the driver
  "derives them once per merge" but names no object. **S13:** the memo moved to mapal-ir as
  `mapal_ir::LoopPlan` / `CategoryIr::loop_plan` (ir §13, BL7 — one source of truth for the
  loop CFG shared by interp, rewrite, backend-llvm); the interp-local `derive_plan`/`LoopPlan`
  were deleted. **Consistent** — still the deduced §4 layout, not new state, now shared.
- **In-SCC representation diverges from DESIGN §2.** §2 prescribes folding `loop_structure`
  into `SecondaryMap<ObjectId, ObjectId>` (object → its merge). The code instead keeps
  membership-only (`SecondaryMap<ObjectId, ()>` in `build_in_scc`) and re-derives the merge via
  `mapal_ir::loop_plan`. Behaviorally equivalent for M1's single-merge SCCs; the object→merge fold is
  the more efficient shape the model intends. Recorded as a **deduce-don't-store seam**, see
  `suggestions.md` S1. Resolution: code is correct, model shape is the target — no invariant
  violated.
- **The clock epoch is process state, not run state (S29).** Every other value the evaluator reads lives in `EvalCtx`; `eval.rs:time_epoch` is a `OnceLock<Instant>` static, i.e. one `DataLoc` outside the `run` boundary — deliberate, and the same choice mapal-rt's `mapal_time_ms` makes, so interp and the LLVM backend measure against the same kind of epoch. Consequence, recorded once: two `run`s in one process share it, so a clock reading is comparable only *within* a run (§5 contracts only monotonicity + finiteness) and `tests/determinism.rs`' byte-identical rule does not extend to a `time`-bearing program. **Consistent** with DESIGN §5 / C-interp-3 as amended.
- **`num_le` uses native `<=`, not `!num_lt`.** Deliberate (IEEE NaN ordering: `Le(NaN,x)`
  must be false); documented in `crates/mapal-interp/src/eval.rs:compare`. **Consistent** with
  DESIGN §3 "IEEE ordering for floats."
- **`Abort` ↔ `Outcome` is not duplication.** `Result<RValue, Abort>` is exactly
  `Done(RValue) ⊕ Abort` with `Ok` carrying the `Done` payload; the split buys straight-line
  `?`-propagation (DESIGN §1). Bridged once by `Abort::into_outcome`. Justified, not a smell —
  see `suggestions.md` Detail.
