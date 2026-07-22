# Component: syntax

Status: tested
Last updated: 2026-07-22 · **S22 ADR-0031**: the ADR-0029 call-expression carve REMOVED — P0108 rejects every call expression uniformly again (`iota(...)`/`fill(...)` get the arrow-form teaching message: "write `n -> iota` / `(x, n) -> fill`"); `ExprKind::Call` remains as the rejection-recovery node only. S20 (carve, superseded) · S13 (ADR-0021 element-update bind `c[i] <- x`; P0013–P0015)
Spec references: user-guide.md §3 (Syntax reference) + ADR-0005 (a flow is a statement; E4) + ADR-0009 (map/fold postfix block) + ADR-0010 (guard-arrow lexing) + ADR-0011 (Core loops are `loop` only) as amended by ADR-0012 (labeled blocks `:label { … }` / jumps `-> :label;`; `Ident {` is always a struct literal) + ADR-0019 (`seq { … }` is a statement block `StageKind::SeqBlock`, not a fanout kind; P0117). Supporting: architecture.md §2.2.1–§2.2.2 (lexer + recursive-descent parser, minimal parse tree), ADR-0008 (error recovery + structured diagnostics, binding), ADR-0006 (`type` keyword, `category` reserved).
Depends on: (none) Depended on by: lower, cli

## What works

- **P1 frontend complete: lexer + parser.** `lex(source) -> LexOutput` and `parse(source) -> ParseOutput` (total, pure, error-recovering; merged span-sorted diagnostics).
- **Lexer** (Session 02): full v0.2-surface token set, spans + `LineIndex`, L0001–L0008 with recovery, ADR-0010 single-lexeme guard arrows.
- **Parser** (Session 03): two-tier grammar per ADR-0005 (expressions 1–7; `->`/`<-` chains at statement level), thin spanned parse tree (DESIGN §15, `Expr::Hole` for operator shorthand), guard blocks/fanout/`map`/`fold` postfix blocks (ADR-0009), loop statements with `-> loop;` back-edges, struct/array/tuple literals, typed `mut` bindings both directions.
- **`seq` statement block** (ADR-0019, Session 11): `seq { … }` is `StageKind::SeqBlock(Block)` — the ordinary block production (statements + optional tail), no longer a `FanoutKind::Seq` fanout. Rebinds/loops inside `seq` are first-class (no silent drop); the old bare-chain branch form still parses; guard arms in `seq` are stray guards (P0004).
- **Element-update bind `c[i] <- x`** (ADR-0021, Session 13): `BindStmt` gained `index: Option<Expr>` — the optional `[' expr ']` after the name (`parse_bind_stmt`). `Some` marks the indexed (element-write) form; `mut`/type-annotation/nested-`[…]` on it draw P0013/P0014/P0015 with recovery. The desugar to `Update` is lower's job, not syntax's.
- **Diagnostics:** P0001–P0015 (syntax/recovery incl. ADR-0010's two guard hints with machine-applicable fixes; **P0013–P0015** guard the ADR-0021 element-update bind `c[i] <- x` — `mut` on an indexed target / a type annotation on one / a nested `c[i][j]` target, all recovered) + P0101–P0117 (out-of-Core rejections, each naming construct + horizon; **P0117** is a structural reject — a non-chain statement dropped from a fanout `void` block, ADR-0019). All eight `examples/*.flow` parse with **zero** diagnostics; `full_surface.flow` yields exactly {P0101×2, P0102×2, P0103×2, P0107, P0109, P0111, P0112, P0113} and zero L-codes — the C8 story end-to-end.
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
- J4 acceptance: eight examples parse with zero diagnostics (`tests/golden_trees.rs`; zip_demo + vector_add added with ADR-0018's zip form).
- J5 presentation-free: no `Display` in crate (C3); render helpers live in `tests/support/`.
- J6 lex-diagnostic preservation: `parse(s).diagnostics ⊇ lex(s).diagnostics` — unit + proptest.

## Test coverage (golden / property / differential / skipped+why)

200 tests, all green (`cargo test -p flow-syntax`): 159 lib (101 parser units — precedence/§3.6 verbatim, W10–W24 ledger, F-matrix payloads, ADR-0011 scan, classification, the ADR-0019 `seq` statement-block suite [stmt-form/headless-compat/rebind+loop-kept/tail/empty/optional-`;`/guard-arm-P0004/fanout-P0117 + each-drop-P0117/unterminated-tail-branch-kept/seq-arm-mixed-P0006 regressions], the ADR-0021 element-update-bind suite [`index_bind_top_level`/`index_bind_in_loop_body`/`index_bind_computed_index` accept + `index_bind_mut_rejected_p0013`/`index_bind_type_annotation_rejected_p0014`/`index_bind_nested_rejected_p0015`], **the ADR-0031 uniform-P0108 test `iota_fill_calls_are_p0108_rejected_with_teaching_text`** — every call expression rejects, iota/fill with the arrow-form teaching text, and the arrow forms (`4 -> iota`, `(1.0, 4) -> fill`) parse clean, every P-code, design-review regression pins; 58 lexer) · 9 golden token streams · 9 golden parse trees (zero diags asserted; +zip_demo +vector_add, ADR-0018) · 2+3+6 diagnostics/error/out-of-Core fixtures · 3 coverage · 3+3 proptests (lexer/parser, 2048 cases each). Golden trees were verified by **independent re-derivation** (one reviewer per example, node-by-node against source + grammar); implementation passed 2 adversarial reviews + a fix round (stack-overflow totality fix, P0007 climber path).

## Performance notes (numbers + bench name + date; regressions flagged)

`benches/lex_parse.rs` (criterion 0.5.1), 2026-06-12, Apple-silicon dev machine: parse abs 1.24 µs · pipeline 1.07 µs · fanout 1.51 µs · sum_to_n 1.79 µs · fir 3.03 µs · sepia 7.45 µs · ~100× sepia synthetic 740 µs (≈100× single sepia ⇒ linear, no superlinear blowup from the ADR-0011 scan on real shapes). No baseline regressions to flag (first recording).

## Open questions (→ ADR candidates)

- ADR-0012 (decided with Sapir, Session 03): labeled blocks `:label { … }`, jumps `-> :label;`, enclosing-targets-only; Core+1 lifts P0110. Break-to-after-a-loop deliberately undecided — the Core+1 ADR must decide or re-defer.
- P0115 scope reading: anonymous block stages (user-guide §8.3 fanout form, §5.2 `seq` branches) rejected as out-of-Core — HANDOFF §4.1 silence read as default-reject; flip = lift P0115.
- W15 unary-minus/`!` precedence: §3.6 table omits unary; bound tighter than `*`, looser than postfix (standard). Spec gap, no ADR.
- W23: `?` parsed as expression postfix (per exhibits) not §3.6 rank 9 — final call belongs to the Core+1 error-handling ADR.
- Lexer items unchanged (string escape set; Core+1 pattern guards).
