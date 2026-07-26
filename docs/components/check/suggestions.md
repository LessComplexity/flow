# check — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 one source of truth for shared structure | `is_print_builtin` exists twice: `mapal-lower/src/effects.rs` (`pub(crate)`) and `mapal-check/src/effects.rs` (local copy) — the ADR-0015 print-family membership is shared structure with two definitions | host the predicate once where both can reach it (`mapal-syntax` next to the reserved-name list, or `mapal-ir` next to `Print{newline}`) and import from both | a third print-family builtin (or a rename) cannot drift the two copies; the seam is declared once |
| 2 | §3 consolidation (watchful, not actionable) | `mapal_ir::SourceLoc` / `mapal_syntax::SourceLoc` are bijective-on-fields twins with per-consumer converters (lower's, now check's `to_syntax`) | none now — the split is a deliberate no-deps pin (ir/loc.rs D8); revisit only if a third converter appears | recorded so the third copy triggers the §3 review instead of a fourth converter |
