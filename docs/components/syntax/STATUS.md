# Component: syntax

Status: tested
Last updated: 2026-06-12 · Session 03
Spec references: user-guide.md §3 (Syntax reference) + ADR-0005 (a flow is a statement; E4) + ADR-0009 (map/fold postfix block) + ADR-0010 (guard-arrow lexing) + ADR-0011 (Core loops are `loop` only) as amended by ADR-0012 (labeled blocks `:label { … }` / jumps `-> :label;`; `Ident {` is always a struct literal). Supporting: architecture.md §2.2.1–§2.2.2 (lexer + recursive-descent parser, minimal parse tree), ADR-0008 (error recovery + structured diagnostics, binding), ADR-0006 (`type` keyword, `category` reserved).
Depends on: (none) Depended on by: lower, cli

## What works

- **P1 frontend complete: lexer + parser.** `lex(source) -> LexOutput` and `parse(source) -> ParseOutput` (total, pure, error-recovering; merged span-sorted diagnostics).
- **Lexer** (Session 02): full v0.2-surface token set, spans + `LineIndex`, L0001–L0008 with recovery, ADR-0010 single-lexeme guard arrows.
- **Parser** (Session 03): two-tier grammar per ADR-0005 (expressions 1–7; `->`/`<-` chains at statement level), thin spanned parse tree (DESIGN §15, `Expr::Hole` for operator shorthand), guard blocks/fanout/`seq`/`map`/`fold` postfix blocks (ADR-0009), loop statements with `-> loop;` back-edges, struct/array/tuple literals, typed `mut` bindings both directions.
- **Diagnostics:** P0001–P0012 (syntax/recovery incl. ADR-0010's two guard hints with machine-applicable fixes) + P0101–P0116 (out-of-Core rejections, each naming construct + horizon). All six `examples/*.flow` parse with **zero** diagnostics; `full_surface.flow` yields exactly {P0101×2, P0102×2, P0103×2, P0107, P0109, P0111, P0112, P0113} and zero L-codes — the C8 story end-to-end.
- Recovery: panic-mode with sync sets, diagnostic cooldown, shared depth guard (128) over expressions/blocks/**types**, progress lemma — `parse` is total on adversarial input (≈490K-case proptest run during review).

## What does not / known issues

- Parser-level imprecision, deliberate (DESIGN §16/§17): expression-position generics surface as P0007/P0001 not P0103 (W16); `filter` recovery renders as `map` in the tree (P0114 diagnostic carries the name); stage-position P0006 statements are reported then dropped from the arm-only Guard node.
- Semantic/scope checks deferred to flow-check by design (C10): scalar-type validity, string-as-data, mut/arity/exhaustiveness, `print` placement, recursion, named-param partial application (W21).
- Char literals still unlexed (`'` → L0001) — corpus-empty soft spot (DESIGN §10).

## Invariants enforced (and where in code)

- Lexer I1–I5 — unchanged (see Session 02 entry; `lexer.rs`, proptests).
- J1 parse total: depth guard (P0011, limit 128, shared incl. type recursion) + progress lemma `debug_assert`s — `parser.rs`; proptests over arbitrary strings/unicode/flow-soup.
- J2 span sanity (child ⊆ parent, in-bounds): `debug_assert`s in node constructors + recursive walkers in unit & property tests (incl. Bind.ty, MapFold params, arm/discr spans).
- J3 zero diagnostics ⟹ no Error nodes / rejected-kept forms: walkers in goldens + proptests.
- J4 acceptance: six examples parse with zero diagnostics (`tests/golden_trees.rs`).
- J5 presentation-free: no `Display` in crate (C3); render helpers live in `tests/support/`.
- J6 lex-diagnostic preservation: `parse(s).diagnostics ⊇ lex(s).diagnostics` — unit + proptest.

## Test coverage (golden / property / differential / skipped+why)

174 tests, all green (`cargo test -p flow-syntax`): 140 lib (104 parser units — precedence/§3.6 verbatim, W10–W24 ledger, F-matrix payloads, ADR-0011 scan, classification, every P-code, design-review regression pins; 36 lexer) · 6 golden token streams · 6 golden parse trees (zero diags asserted) · 2+3+6 diagnostics/error/out-of-Core fixtures · 3 coverage · 3+3 proptests (lexer/parser, 2048 cases each). Golden trees were verified by **independent re-derivation** (one reviewer per example, node-by-node against source + grammar); implementation passed 2 adversarial reviews + a fix round (stack-overflow totality fix, P0007 climber path).

## Performance notes (numbers + bench name + date; regressions flagged)

`benches/lex_parse.rs` (criterion 0.5.1), 2026-06-12, Apple-silicon dev machine: parse abs 1.24 µs · pipeline 1.07 µs · fanout 1.51 µs · sum_to_n 1.79 µs · fir 3.03 µs · sepia 7.45 µs · ~100× sepia synthetic 740 µs (≈100× single sepia ⇒ linear, no superlinear blowup from the ADR-0011 scan on real shapes). No baseline regressions to flag (first recording).

## Open questions (→ ADR candidates)

- ADR-0012 (decided with Sapir, Session 03): labeled blocks `:label { … }`, jumps `-> :label;`, enclosing-targets-only; Core+1 lifts P0110. Break-to-after-a-loop deliberately undecided — the Core+1 ADR must decide or re-defer.
- P0115 scope reading: anonymous block stages (user-guide §8.3 fanout form, §5.2 `seq` branches) rejected as out-of-Core — HANDOFF §4.1 silence read as default-reject; flip = lift P0115.
- W15 unary-minus/`!` precedence: §3.6 table omits unary; bound tighter than `*`, looser than postfix (standard). Spec gap, no ADR.
- W23: `?` parsed as expression postfix (per exhibits) not §3.6 rank 9 — final call belongs to the Core+1 error-handling ADR.
- Lexer items unchanged (string escape set; Core+1 pattern guards).
