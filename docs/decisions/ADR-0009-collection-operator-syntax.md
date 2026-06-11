# ADR-0009: Collection operators take a postfix inline block; their input tuple corresponds positionally to the block's parameters

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

`user-guide.md` §10.4 ("Common patterns") shows `fold` written as

```flow
array -> fold(0, { acc, item -> acc + item })
```

This is the only place in the entire corpus where an inline block sits inside call
parentheses, occupying an argument position. It is wrong on three counts. (a) It contradicts
HANDOFF §4.1, which scopes `map`/`fold` as collection operators whose body is an inline block
that **is not a first-class value** — here the block is passed as the second argument of a
call, i.e. treated exactly as a value. (b) It is inconsistent with the rest of the surface:
`map` already uses the postfix-block form (`array -> map { item -> ... }`, §10.4 and the sepia
example), and multi-argument primitives use the tuple-input call convention (`(v, lo, hi) ->
clamp`), so `fold` here matches neither. (c) It would force the parser and the lowering to
carry a special case for a block in argument position that nothing else in Flow-Core needs.
The defect was found by Sapir while reviewing `examples/sepia.flow` in Session 01, whose `fold`
statement had inherited the §10.4 form.

## Decision (one paragraph, imperative)

Adopt the collection-operator law: data arrives through the wire (the `->` edge), and the
inline block is **postfix operator syntax, never an argument**; the operator's input tuple
corresponds **positionally** to the block's parameters. Concretely the two canonical
Flow-Core forms are `map`, written `array -> map { item -> ... }` (the input array
corresponds to the block parameter `item`, applied element-wise), and `fold`, written
`(init, array) -> fold { acc, item -> ... }` (input `init` corresponds to block parameter
`acc`; input `array` corresponds to `item`, applied element-wise). The `(init, array)` input
order is chosen exactly so that it lines up positionally with the block parameters
`(acc, item)`. Any future collection operator (`filter` etc., Core+1) must obey the same law:
inputs flow in through the wire as a tuple, the block is a postfix suffix, and the input tuple
maps positionally onto the block parameters.

## Consequences (tradeoffs, implementation impact)

- The parser treats `map`/`fold` as operators that take a **block suffix** — the same
  postfix-block production for both — rather than as calls that can take a block argument.
  There is no "block in argument position" case anywhere in the grammar.
- The positional-correspondence rule is a single teachable invariant (input tuple ↔ block
  parameters, left to right) that already covers `map` and `fold` and extends unchanged to any
  later collection operator, so the language surface stays uniform as Core+1 grows.
- Lowering gets **zero special cases**: `(init, array)` lowers to `Pair(init, array)` followed
  by the `fold` primitive, exactly the Pair-then-primitive rule of `category-ir.md` §4; `map`
  lowers as the single-input primitive it already was. The earlier `fold(init, block)` form
  would have required a bespoke "block as second operand" path; that path no longer exists.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/user-guide.md` §10.4 — the `fold` line is changed from
`array -> fold(0, { acc, item -> acc + item })` to
`(0, array) -> fold { acc, item -> acc + item }`, with an adjacent blockquote pointing here.
The `filter` line is non-normative full-language material and is left untouched. patched? yes
— Session 01. `examples/sepia.flow` — the `fold` statement is rewritten to the postfix-block
form. `docs/spec/ERRATA.md` — recorded as later correction **LC-2** (this entry does change
spec text, unlike LC-1).
