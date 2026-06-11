# Component: syntax

Status: building
Last updated: 2026-06-11 · Session 02
Spec references: user-guide.md §3 (Syntax reference) + ADR-0005 (a flow is a statement; E4) + ADR-0010 (guard-arrow lexing). Supporting: architecture.md §2.2.1–§2.2.2 (lexer + recursive-descent parser), ADR-0008 (error recovery + structured diagnostics, binding), ADR-0009 (map/fold postfix block), ADR-0006 (`type` keyword, `category` reserved).
Depends on: (none) Depended on by: lower, cli

## What works

- **Lexer complete** (`lex(source) -> LexOutput`): full v0.2-surface token set, byte-offset spans (`SourceLoc`) + `LineIndex`, structured diagnostics L0001–L0008 with recovery (total function — never panics, never bails).
- Guard arrows lex as single tokens per ADR-0010 (adjacency + `{`/`;`/`}`-or-start context gate); all six `examples/*.flow` lex with zero diagnostics.
- `category` reserved-and-rejected at the lexer with L0004 + SuggestedFix → `type` (ADR-0006); `executor`/`pub`/`use`/`void` lex as reserved keywords for parser-side Core rejection.
- Out-of-Core v0.2 surface (`@`, `?`, `::`, `...`, `channel<i32>`, generics) tokenizes cleanly (no Error tokens) so the parser can reject with precision.

## What does not / known issues

- **No parser yet** — next increment (recursive-descent, statement-level flow chains per ADR-0005; design notes pre-collected in DESIGN.md §12).
- Documented lexical warts W1–W9 (DESIGN.md §6): notably `-7->x;` statement-initial tight negation lexes as a guard arm (ADR-0010 accepted tradeoff; parser will hint), `i<-1` is `i <- 1` by maximal munch.
- Char literals are not lexed (`'` → L0001) — known C8 soft-spot, corpus-empty (DESIGN.md §10).

## Invariants enforced (and where in code)

- I1 lex is total, always advances, ends with Eof — scanner structure (`lexer.rs` run loop) + proptest over arbitrary strings.
- I2 spans strictly ascending, in-bounds, char-boundary aligned — debug_asserts in `push_token` + proptest.
- I3 every byte is exactly one token span or trivia region — `tests/support/mod.rs::assert_gap_is_trivia`, wired into every fixture suite and proptest; negative regression `tests/coverage.rs`.
- I4 every Error token overlaps ≥1 diagnostic — per-L-code unit tests + proptest.
- I5 no rendering in this crate (`Diagnostic` is Debug-only, no Display) — ADR-0008(b); grep-verified.

## Test coverage (golden / property / differential / skipped+why)

74 tests, all green (`cargo test -p flow-syntax`): 58 unit (DESIGN §5 worked-consequences table, §6 wart ledger W1–W9, munch boundaries, strings, CRLF, LineIndex multi-byte) · 6 golden token-stream snapshots (one per example; zero diagnostics asserted) · 2 golden diagnostics (all L-codes incl. guard-discriminant overflow) · 2 full-surface C8 fixtures · 3 coverage-invariant tests · 3 proptest properties (4096 cases: totality, span invariants, determinism). Snapshots were verified token-by-token against sources by independent review (reference re-tokenization), not merely accepted.

## Performance notes (numbers + bench name + date; regressions flagged)

None yet. Criterion bench deliberately deferred to the parser increment — the component (lexer+parser) is not yet functional as a unit and a lexer-only microbench has no consumer (DESIGN.md §9; HANDOFF §7.2 step 6).

## Open questions (→ ADR candidates)

- String escape set (`\\ \" \n \t`) and single-line-string rule are design decisions where the spec is silent (DESIGN.md §11) — revisit only if an example needs more.
- Core+1 pattern guards (`-Some(x)->`) cannot be single lexemes; resolution parked for the Core+1 coproducts ADR (ADR-0010 records the expected shape).
- Parser: `Ident {` disambiguation (struct literal vs loop label) and whether Core restricts loop labels to `loop` — decide in the parser design (DESIGN.md §12).
