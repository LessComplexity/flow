# ADR-0012: Labeled blocks are written `:label { … }` and jumped to with `-> :label;`; jumps target enclosing labels only

Date: 2026-06-12 · Status: accepted (decided with Sapir, Session 03) · Amends: ADR-0011

## Context (what forced the decision; spec refs)

ADR-0011 resolved the statement-initial `Ident {` collision (struct literal vs labeled
loop) with a four-token content scan — sound after review hardening, but a scan-shaped
*law* with accepted degenerate edges (`X { }`, flow-free `X { x }` read as struct
literals, DESIGN §17 W13). Reviewing it, Sapir proposed the underlying reframe: a label
is a **mark on a block** that code can jump to; the loop is a *side effect* of a
backwards jump-edge. That intuition is already the IR's own viewpoint — `category-ir.md`
§4.5: there is no special "branch with back-target" morphism; the back-edge is a real
adjacency edge and loops are *discovered* by Tarjan SCC. What the surface lacked was an
unambiguous way to write the mark: un-sigiled `search { … }` collides with struct
literals (`Pixel { … }` heads statements in `examples/sepia.flow`), while a **prefix
sigil** occupies empty grammar: `:` before an identifier is currently illegal in every
position (statement-initial, stage, expression). A suffix form (`search: {`) would
re-collide with type ascription (`x: i32 …` also starts `Ident Colon`). Un-sigiled
*jumps* (`-> search;`) are likewise indistinguishable from flowing into a variable named
`search`; a sigiled jump (`-> :search;`) is self-identifying. Spec exhibits affected:
user-guide §3.5 (nested `outer`/`inner`) and §8.5 (`search`).

## Decision (one paragraph, imperative)

A labeled block is written **`:label { … }`** (prefix `:` sigil, then an identifier, then
the block); a jump to it is written **`-> :label;`** — sigiled on **both** ends, so both
declaration and jump are recognized with one token of lookahead and no name resolution. A
jump may target only a **lexically enclosing** label, with continue-at-head semantics —
labels are marks on block *heads*, not general gotos: this keeps every cycle reducible
with a single header, which is what the E1 trace lowering (`Tr^U`, ADR-0002) and the
Verilog FSM done-protocol require; forward or sibling jumps remain illegal. The Core
keyword form **`loop { … -> loop; … }` is unchanged** (it denotes a self-labeled block;
all six examples keep compiling verbatim). Labeled blocks remain **out of Flow-Core**
(Core+1): the parser recognizes `:label { … }` and `-> :label` precisely and rejects them
with P0110; un-sigiled `Ident {` is now **always a struct literal** grammatically —
ADR-0011's four-token scan is demoted from disambiguation law to a recovery heuristic
that detects loop-shaped braces and says "labels are written `:name { … }` (Core+1)".
Blocks stay syntax, not values (ADR-0009): a labeled block is a *labeled block*, not a
named lambda — this ADR does not reopen closures. Break-to-after-a-loop is explicitly
**not** decided here (today: `-> :outer;` restarts outer; exits are `-> ret;`); the
Core+1 ADR that lifts P0110 must decide or re-defer it.

## Consequences (tradeoffs, implementation impact)

- Statement-initial `Ident {` is unambiguous (struct literal, always): W13's semantic
  fork disappears; the scan survives only to improve one error message.
- The label system is parser-resolvable end to end: `:label {` declares, `-> :label`
  jumps, nothing depends on later name resolution to classify syntax.
- Parser delta is small and net-simplifying: a `Colon Ident LBrace` statement form and a
  `Colon Ident` stage form, both P0110-rejected-but-parsed; the ADR-0011 scan path keeps
  its machinery with a new message. Snapshot churn limited to fixture messages.
- Core+1 reintroduction is now purely "lift P0110": the surface, the enclosing-only rule,
  and the reducibility argument are already fixed.
- Cost: `:` gains a third role (ascription, struct fields, labels) — all three are
  position-distinguished (after-ident vs before-ident), recorded so it isn't re-litigated.
- The §3.5/§8.5 spec exhibits are patched to the sigiled form (ERRATA LC-3) so the corpus
  stays consistent with the decided surface.

## Spec impact (exact files/sections to patch; patched? yes — Session 03)

`docs/spec/user-guide.md` §3.5 nested-loops exhibit (`outer`/`inner` → `:outer`/`:inner`,
jumps `-> :inner;`/`-> :outer;`) and §8.5 binary search (`search {` → `:search {`,
`-> search;` → `-> :search;`), each marked with a blockquote pointing here and at
ERRATA LC-3. The `loop` keyword exhibits are untouched. `docs/spec/ERRATA.md` gains
**LC-3**. ADR-0011's status is amended (scan law superseded in part; its loop-label
scoping and the Core `loop`-only rule stand). patched? yes — Session 03.
