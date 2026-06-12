# ADR-0011: Flow-Core loop labels are the keyword `loop` only; statement-initial `Ident {` is resolved by semicolon-scan

Date: 2026-06-11 · Status: accepted · **amended by ADR-0012 (2026-06-12):** decision (2)'s
four-token scan is superseded as a disambiguation law — statement-initial `Ident {` is now
always a struct literal, labeled blocks are written `:label { … }` (sigiled), and the scan
survives only as a recovery heuristic for the "did you mean `:name { … }`" hint. Decision
(1) — Core loops are the `loop` keyword only, custom labels Core+1 (P0110) — stands.

## Context (what forced the decision; spec refs)

Two statement forms collide on the prefix `Ident {`. A **struct literal** can head a flow
statement (`Pixel { r: nr, g: ng, b: nb }` as a map-block tail in `examples/sepia.flow`;
`RGB { r, g, b } -> ret;` in user-guide §8.3), and a **custom-labeled loop** is also
`Ident {` (`search { … }` in user-guide §8.5; `outer { … inner { … } … }` in §3.5). The
spec exhibits both and is silent on how a parser tells them apart; recursive descent needs
a deterministic rule with bounded lookahead (HANDOFF §5 item 2). Separately, HANDOFF §4.1
scopes Core loops as "labeled `loop { … -> loop; … -> ret; }`" — every in-Core exhibit
(all six examples, user-guide §3.5 while-form, §4.4) uses exactly the label `loop`; custom
labels appear only in exhibits that are out of Core for other reasons too (slices,
`Option`, `usize`). A custom-label jump (`-> search;`) is lexically indistinguishable from
flowing into a variable named `search`, so admitting custom labels would push label/name
resolution into the parser. Both questions were pre-collected in DESIGN.md §12 and must be
settled before the parser is written (P1).

## Decision (one paragraph, imperative)

**(1)** Flow-Core's only loop introducer is the keyword `loop`; the back-edge target
`-> loop;` binds to the **innermost enclosing loop**. Custom loop labels (`search { … }`)
and cross-loop jumps are **out of Flow-Core** — the parser recognizes the labeled-loop
form precisely and rejects it with dedicated diagnostic P0110 (out-of-Core class), keeping
the parse tree as a loop for recovery; reintroduction is a Core+1 concern (it lifts P0110
without grammar change). **(2)** Disambiguate statement-initial `Ident {` by a bounded
token scan to the matching `}` (brace-depth counting): if any of **`Semi`, `Arrow`,
`BackArrow`, or `Guard`** occurs before the matching close (at any depth), the form is a
labeled loop (→ P0110); otherwise it is a struct literal heading a flow statement. The
scan is sound in the struct direction because a struct literal's field initializers are
*expressions*, and Flow expressions contain no `;`, no block-expressions, and — by
ADR-0005, a flow is a statement — no `->`/`<-`/guard tokens. In the loop direction it is
sound for every loop that *does* anything: a loop body containing none of those four
tokens has no statement terminators and no flows (e.g. `outer { x }`), is operationally
empty, and reads as a struct literal — an accepted degenerate (DESIGN §17 ledger). The
scan applies **only at statement-initial position** and only to a plain `Ident`;
keyword-introduced blocks (`loop`, `seq`, `map`, `fold`, `void`) are dispatched by their
keyword token before the scan is considered. In every other expression position `Ident {`
is unambiguously a struct literal (loops are statements, ADR-0005).

## Consequences (tradeoffs, implementation impact)

- Deterministic, backtracking-free parsing: one O(block-length) lookahead scan per
  statement-initial `Ident {`, worst-case quadratic only on pathological nesting of
  exactly this form — irrelevant at Flow-Core scale, noted in DESIGN §14.
- `-> loop;` needs no label resolution in the parser or tree: the keyword token is the
  back-edge marker; lower binds it to the innermost `loop` (and flow-check rejects a
  `-> loop;` with no enclosing loop).
- Degenerate edges: `X { }` (empty braces) and a flow-free labeled body (`X { x }`)
  classify as struct literals — acceptable (the latter is an operationally empty loop);
  recorded in the DESIGN §17 ledger. A custom-labeled loop with any real body (it must
  contain a flow or a `;` to do anything, e.g. the user-guide §8.5 `search` loop) is
  detected and gets P0110.
- Nested `loop { loop { … } }` remains grammatically legal; with only the `loop` label,
  inner code cannot target the outer loop — exactly the expressivity HANDOFF §4.1 grants
  Core. The user-guide §3.5/§8.5 custom-label exhibits become *precise* rejections rather
  than confusing parse errors (C8).
- The `Ident block`/`Ident {`-scan machinery is reused verbatim when Core+1 admits labels.

## Spec impact (exact files/sections to patch; patched? n/a)

None — the spec exhibits both forms but states no disambiguation or label-scoping rule, so
no v0.2 text is corrected (nothing for ERRATA; the §3.5/§8.5 exhibits stay valid
full-language material). The binding record is this ADR; the implementation contract is
DESIGN.md §14/§17 and the P0110 row of §16, enforced by the parser unit tests and the
out-of-Core golden fixture. Flagged to Sapir in `docs/next-session.md` (Session 03);
revisable by a superseding ADR. patched? n/a.
