# check — implementation map

> The functor DESIGN.md ("Categorical model") → code. Every model object/morphism maps
> to the `file:symbol` that realises it (FRAMEWORK §6.3 — rows updated WITH the code).
> Last reconciled: 2026-07-25 · S29 (`time` joins the effectful-builtin set; 30 tests).

## Objects (Dat) → code

| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `Src` | `&str` (borrowed source text) | `mapal-check/src/lib.rs:check` param | built |
| `Program` | `&mapal_syntax::Program` (borrowed tree) | `mapal-check/src/lib.rs:check` param | built |
| `CategoryIr` | `&mapal_ir::CategoryIr` (borrowed sealed graph) | `mapal-check/src/lib.rs:check` param | built |
| `Diagnostic*` | `Vec<mapal_syntax::Diagnostic>` (free monoid, append-only) | `mapal-check/src/lib.rs:check` return | built |
| `TCode` | `enum TCode { MultipleReturnWriters, EffectInFanout }` | `mapal-check/src/diag.rs:TCode` | built |
| `EffectSig` (deduced) | `BTreeMap<&str, bool>` transient, rebuilt per call | `mapal-check/src/effects.rs:check` (step 1) | built |
| `WriterSet` (deduced) | iterator over non-`Pair` in-edges, never materialised | `mapal-check/src/exclusivity.rs:check` | built |
| `FanoutBranch` | tree region: `StageKind::Fanout` branch chains (any kind — opens context unconditionally) | `mapal-check/src/effects.rs:Walk::chain` (Fanout + SeqBlock arms) | built |
| scope (locals) | `Scope { frames: Vec<BTreeSet<String>> }` | `mapal-check/src/effects.rs:Scope` | built |

## Morphisms (Trn / relations) → code

| Morphism | Signature | Realised at | State |
| --- | --- | --- | --- |
| `check` | `Src × Program × CategoryIr → Diagnostic*` | `mapal-check/src/lib.rs:check` | built |
| `exclusivity` | `CategoryIr → Diagnostic*` | `mapal-check/src/exclusivity.rs:check` | built |
| `effects` | `Src × Program × CategoryIr → Diagnostic*` | `mapal-check/src/effects.rs:check` | built |
| `writers` (deduced) | `Return object → 𝒫(MorphismId)` | `mapal-check/src/exclusivity.rs:check` (in-edge filter) | built |
| `effectful?` (deduced) | `FuncDef → 𝔹` | `mapal-check/src/effects.rs:check` (`ty_contains_token` on input/output tys — reused from mapal-ir, not re-rolled) | built |
| `fanouts` | `Program → FanoutBranch*` | `mapal-check/src/effects.rs:Walk` (node-kind walk; `Fanout` opens context unconditionally, `SeqBlock` recurses sticky) | built |
| effectful builtin? (deduced) | `𝕊 → 𝔹` — the builtin effect sites: `print`/`println` (ADR-0015) ∪ `time` (S29) | `mapal-check/src/effects.rs:is_print_builtin` · `mapal-check/src/effects.rs:is_time_builtin` (both consulted by `effects.rs:classify_stage_call`) | built |
| `name_text` | `Name × Src → 𝕊` | `mapal-check/src/effects.rs:name_text` | built |
| `code` | `Diagnostic → TCode` | `mapal-check/src/diag.rs:TCode::code` | built |
| `boundary` (C-check-1) | `debug_assert!(validate(ir).is_empty())` | `mapal-check/src/lib.rs:check` entry | built |

## Tests → plan rows

| DESIGN §7 row | Realised at | Count |
| --- | --- | --- |
| §7.1 acceptance (9 examples + determinism + cross-pass order) | `mapal-check/tests/acceptance.rs` | 12 |
| §7.2/§7.4 exclusivity (incl. token-bearing loop) | `mapal-check/tests/exclusivity.rs` | 6 |
| §7.3 effects (incl. node-kind seq discrimination, CK5 theorem, pure-seq-in-branch clean, L1105-shadow documentation, T0201 span pins, **S29 `time_in_plain_branch_is_t0201`**) | `mapal-check/tests/effects.rs` | 12 |

## Notes / divergences

- `mapal_ir::SourceLoc` → `mapal_syntax::SourceLoc` conversion is a 2-field local helper
  (`exclusivity.rs:to_syntax`) — the types are field-identical but deliberately distinct
  (ir/loc.rs D8, no-deps discipline).
- `is_print_builtin` is a local 2-line copy of lower's `pub(crate)` predicate
  (ADR-0015 family) — consolidation candidate, see suggestions.md #1. **S29 added a second
  such copy**, `effects.rs:is_time_builtin` alongside `mapal-lower/src/lib.rs:is_time_builtin`:
  same shape, same reason, so suggestion #1's payoff now covers two predicates (the
  builtin-effect-site set), not one. Not re-filed — it is the same smell.
- E3: **no code by design** (DESIGN §5 vacuity proof; CK6). T03xx unallocated.
- No typing pass by design (DESIGN §0; supersedes lower §12's old re-walk wording).
