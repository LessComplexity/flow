# ADR-0006: Rename the surface keyword `category` to `type` (E5)

Date: 2026-06-11 · Status: accepted (pending Sapir veto at bootstrap review)

## Context (what forced the decision; spec refs)

Flow's surface language uses the keyword `category` to declare a type — i.e. an object in
Flow-Cat. The formal semantics simultaneously uses "category" for the entire
category-theoretic structure (objects + morphisms + composition). One word names two
different things, and the collision actively confuses the spec's own exposition: it is
flagged in `category-ir.md` Appendix A (which has to disambiguate by capitalization and
context — "lowercase category / Flow-Cat" vs. "capitalized Category / Flow type") and in
`CHANGES.md` §8, and Appendix A explicitly anticipates "a future revision may rename the
surface keyword to `type`." Right now zero compiler code exists, so this is the last free
moment to make the rename: every later moment costs a migration. The IR's internal naming is
already neutral (`Ty`), so the change is confined to the surface.

## Decision (one paragraph, imperative)

Rename the surface keyword `category` to `type` now, while no code exists. Declare named
product types with `type Point { x: f32, y: f32 }`. Reserve the old keyword `category` and
reject it with a helpful diagnostic that points at `type`, so existing-in-the-wild examples
fail loudly rather than silently. Perform the rename across `user-guide.md`,
`getting-started.md`, and all `examples/` during bootstrap unless Sapir vetoes; the
historical `flow-language-design.docx` is deferred (referenced, not edited). The IR keeps its
already-neutral `Ty` naming. This decision is accepted but explicitly subject to Sapir's veto
at the bootstrap review (flagged in `docs/next-session.md`); if vetoed, the rename and the
reserved-word rejection are both reverted.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: the v0.1-era docx and any external material using `category` become stale; the
  reserved-and-rejected keyword mitigates this by giving a precise migration message.
- Implementation: the lexer/parser recognize `type` as the type-declaration keyword and
  `category` as a reserved-rejected keyword from day one (P1); no special handling is needed
  in the IR since `Ty` was already the internal name.
- The rename touches user-facing docs and every example, so it must land in bootstrap before
  those files are treated as the acceptance surface — doing it later would invalidate golden
  snapshots.
- Because the decision is veto-pending, the next-session handoff must surface it as an open
  question for Sapir so the bootstrap review can confirm or reverse it cheaply.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/user-guide.md`, `docs/spec/getting-started.md`, and `examples/` — surface keyword
`category` → `type` (product-type declarations), old keyword reserved-and-rejected.
`flow-language-design.docx` deferred (historical, not edited). Affected sections marked
`> **Erratum E5 applied — see docs/spec/ERRATA.md and ADR-0006.**`. patched? yes — Session 01.
