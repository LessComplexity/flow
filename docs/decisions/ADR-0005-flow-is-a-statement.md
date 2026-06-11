# ADR-0005: `a -> b + c -> d` parses as `a -> (b + c) -> d`; a flow is a statement, not a value (E4)

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

`user-guide.md` §3.6 gives an operator-precedence table that places `->`/`<-` (level 8)
*looser* than `+` (level 4). The same section's worked example then claimed
`a -> b + c -> d` parses as `(a -> b) + (c -> d)`. That parse is impossible under the table —
it would require `->` to bind tighter than `+`, the opposite of what the table says — and it
is incoherent at a deeper level: it treats `a -> b` as a value that can be a left operand of
`+`, presupposing that a flow produces a value usable in an arithmetic expression. The defect
is both a contradiction inside one section and a hint at an unresolved grammar question: is a
flow an expression or a statement? Both must be settled before the parser is written (this is
the first thing P1 builds), so the resolution is recorded here and the parser is built to it.

## Decision (one paragraph, imperative)

Parse `a -> b + c -> d` as `a -> (b + c) -> d`, consistent with the §3.6 precedence table
(`->` looser than `+`), and correct the example text accordingly with an explanatory line.
Additionally settle the grammar question at the parser level: **a flow is a statement, not a
value-producing expression** — `->`/`<-` chains are parsed at statement level, so a flow may
never appear as an operand of an arithmetic, comparison, or boolean operator. Implement this
as the statement-level parse rule in the recursive-descent parser: expression parsing handles
operators through level 7 (`||`), and the flow operators are handled by a separate
statement-level production that consumes a chain of `->`/`<-` arrows over expression operands.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: flows cannot be nested inside expressions; you cannot write `(a -> f) + 1`. This
  is intentional and removes a whole class of ambiguous and meaningless parses.
- Implementation: the parser has a clean two-tier grammar — an expression parser (precedence
  levels 1–7) and a statement-level flow-chain parser (levels 8–10: `->`/`<-`, then `?`, then
  `;`). Each arrow's operands are full expressions; chains do not produce expression values.
- This makes the E4 example, the precedence table, and the parser mutually consistent and
  golden-testable from the first parse-tree snapshot in P1.
- Pipelines with operator shorthand (`data * 2 -> + 5 -> ret;`) remain expressible because the
  shorthand operands are expression fragments attached to arrows, not nested flows.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/user-guide.md` §3.6 — corrected example (`a -> b + c -> d ≡ a -> (b + c) -> d`)
plus the added line "A flow is a statement, not a value-producing expression; `->`/`<-`
chains are parsed at statement level." Marked
`> **Erratum E4 applied — see docs/spec/ERRATA.md and ADR-0005.**`. patched? yes — Session 01.
