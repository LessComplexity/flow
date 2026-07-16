# check — implementation map

> The functor DESIGN.md ("Categorical model") → code. Every model object/morphism maps
> to the `file:symbol` that realises it (FRAMEWORK §6.3 — rows updated WITH the code).
> Last reconciled: 2026-07-16 · Session 10 (crate built, 25 tests).

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `Src` | `&str` (borrowed source text) | `flow-check/src/lib.rs:check` param | built |
| `Program` | `&flow_syntax::Program` (borrowed tree) | `flow-check/src/lib.rs:check` param | built |
| `CategoryIr` | `&flow_ir::CategoryIr` (borrowed sealed graph) | `flow-check/src/lib.rs:check` param | built |
| `Diagnostic*` | `Vec<flow_syntax::Diagnostic>` (free monoid, append-only) | `flow-check/src/lib.rs:check` return | built |
| `TCode` | `enum TCode { MultipleReturnWriters, EffectInFanout }` | `flow-check/src/diag.rs:TCode` | built |
| `EffectSig` (deduced) | `BTreeMap<&str, bool>` transient, rebuilt per call | `flow-check/src/effects.rs:check` (step 1) | built |
| `WriterSet` (deduced) | iterator over non-`Pair` in-edges, never materialised | `flow-check/src/exclusivity.rs:check` | built |
| `FanoutBranch` | tree region: `StageKind::Fanout{kind: Plain}` branch chains | `flow-check/src/effects.rs:Walk::chain` (Fanout arm) | built |
| scope (locals) | `Scope { frames: Vec<BTreeSet<String>> }` | `flow-check/src/effects.rs:Scope` | built |

## Morphisms (Trn / relations) → code

| Morphism | Signature | Realised at | State |
| --- | --- | --- | --- |
| `check` | `Src × Program × CategoryIr → Diagnostic*` | `flow-check/src/lib.rs:check` | built |
| `exclusivity` | `CategoryIr → Diagnostic*` | `flow-check/src/exclusivity.rs:check` | built |
| `effects` | `Src × Program × CategoryIr → Diagnostic*` | `flow-check/src/effects.rs:check` | built |
| `writers` (deduced) | `Return object → 𝒫(MorphismId)` | `flow-check/src/exclusivity.rs:check` (in-edge filter) | built |
| `effectful?` (deduced) | `FuncDef → 𝔹` | `flow-check/src/effects.rs:check` (`ty_contains_token` on input/output tys — reused from flow-ir, not re-rolled) | built |
| `fanouts` | `Program → FanoutBranch*` | `flow-check/src/effects.rs:Walk` (context-sensitive walk; `branch_ctx = in_fanout \|\| Plain`) | built |
| `name_text` | `Name × Src → 𝕊` | `flow-check/src/effects.rs:name_text` | built |
| `code` | `Diagnostic → TCode` | `flow-check/src/diag.rs:TCode::code` | built |
| `boundary` (C-check-1) | `debug_assert!(validate(ir).is_empty())` | `flow-check/src/lib.rs:check` entry | built |

## Tests → plan rows

| DESIGN §7 row | Realised at | Count |
| --- | --- | --- |
| §7.1 acceptance (9 examples + determinism + cross-pass order) | `flow-check/tests/acceptance.rs` | 11 |
| §7.2/§7.4 exclusivity (incl. token-bearing loop) | `flow-check/tests/exclusivity.rs` | 6 |
| §7.3 effects (incl. FanoutKind-Seq trap, CK5 pin, L1105-shadow documentation, T0201 span pins) | `flow-check/tests/effects.rs` | 8 |

## Notes / divergences

- `flow_ir::SourceLoc` → `flow_syntax::SourceLoc` conversion is a 2-field local helper
  (`exclusivity.rs:to_syntax`) — the types are field-identical but deliberately distinct
  (ir/loc.rs D8, no-deps discipline).
- `is_print_builtin` is a local 2-line copy of lower's `pub(crate)` predicate
  (ADR-0015 family) — consolidation candidate, see suggestions.md #1.
- E3: **no code by design** (DESIGN §5 vacuity proof; CK6). T03xx unallocated.
- No typing pass by design (DESIGN §0; supersedes lower §12's old re-walk wording).
