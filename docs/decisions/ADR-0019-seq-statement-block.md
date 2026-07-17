# ADR-0019: `seq` is a statement block, not a fanout kind

Date: 2026-07-16 · Status: accepted (proposed by orchestrator, ratified by Sapir,
Session 11; answers OQ-C1 by construction)

## Context (what forced the decision; spec refs)

`seq { … }` is realized today as `StageKind::Fanout { kind: FanoutKind::Seq }` — the
*same* parse-tree node as a parallel fanout, differing only in a field. That shape has
produced a trail of defects and debt:

1. **The S10 walk trap.** Any tree walk that cares about parallelism must key on the
   `kind` field, not the node kind (Session 10's headline design-review catch; pinned
   as a standing gotcha in `next-session.md`). Traps that must be re-warned every
   session are architecture bugs, not documentation gaps.
2. **The spec's own canonical example does not parse.** user-guide §5.2 writes
   `-> { "Step 1" -> log };` inside `seq` — an anonymous block stage, P0115-rejected
   (out of Core). Verified this session: the §5.2 example fails at parse. In-Core
   `seq` today accepts only bare chains.
3. **Silent statement drops.** `parse_fanout_block` parses a full block body, then
   `filter_map`s away every non-`Chain` statement (parser.rs:1940) with **no
   diagnostic** — a `x <- e` rebind or `loop { }` inside `seq` (or any fanout block)
   vanishes silently.
4. **`kind` is semantically dead.** `emit_fanout` binds `_kind` (emit.rs:2011) —
   lower gives seq no distinct semantics. Effect ordering already falls out of
   source-order token threading (ADR-0013's linear world token); seq-as-fanout also
   *packs branch tails into a join*, which in body-return position produces a
   baffling L1201 instead of anything a user intended.
5. **OQ-C1.** "Is `seq { print }` inside a fanout branch illegal?" needed a
   conservative pin (CK5) precisely because seq was a fanout variant — the spec never
   drew the nested case because the composite reading was obscured by the shared node.

Zero `.flow` examples use `seq`; check's tests exercise only bare-chain bodies. Blast
radius is minimal. `seq` is already a keyword (`KwSeq`), so `seq {` never collides
with the `Ident {` struct-literal rule (ADR-0011/0012 gate untouched); the block
production (`parse_block_body`) already exists for fn bodies and guard-arm blocks.

## Decision (one paragraph, imperative)

Make `seq { … }` a **statement block in stage position**: the body is the ordinary
block production (statements — chains, `x <- e` rebinds, `loop`s — plus an optional
tail chain; guard arms stay illegal in it — as-built S11: a clean guard token is a
stray guard → P0004, spaced/pattern arm forms draw P0005/P0106, plus P0006 when
mixed with statements), parsed to a **new
`StageKind::SeqBlock(Block)`** node; `FanoutKind` shrinks to `Plain | Void`.
Semantics, pinned: (a) headless chain statements and the tail seed from the seq
input — today's bare-chain branch form therefore still parses and means the same;
(b) the block lowers **in the enclosing scope** (bindings escape, exactly as fanout
branches do today — the `fanout.flow` idiom); (c) the seq's value is its **tail
chain's value**; a seq whose chain continues (or that sits in return position) with
no tail is an error (no more silent pack-of-tails); (d) `seq` has **no IR
footprint** — its ordering guarantee *is* the token thread that statement-order
lowering produces (ADR-0013 unchanged; interp unchanged; pure statements inside
carry no observable order, so rewrites may still parallelize them); (e) `seq`
remains the E2 legal effect site, and a `seq` block inside a `Plain` fanout branch
is simply a composite morphism, effectful iff its body is — ADR-0003 rejects the
effectful case verbatim, so **CK5 becomes a theorem and OQ-C1 is closed** (a pure
seq block in a branch is trivially legal, and pointless). Independently, replace the
silent non-chain-statement drop in the remaining (`Plain`/`Void`) fanout blocks with
a new diagnostic **P0117**. P0115 anonymous blocks stay out of Core: `seq` is
Flow-Core's keyword-marked block stage, and that is exactly its job.

## Consequences (tradeoffs, implementation impact)

- `flow-syntax`: `StageKind::SeqBlock(Block)`; the `KwSeq` parser arm calls the block
  production instead of `parse_fanout_block`; P0117 for dropped statements in
  `Plain`/`Void` fanout blocks; goldens/tests for statement-form seq, rebind/loop
  inside seq, arm-mixing P0006, empty `seq { }`, old bare-chain compat.
- `flow-lower`: a `SeqBlock` emit path — statements via the existing statement
  lowering, tail via `HeadlessSeed(input)`, enclosing scope, no-tail-but-continues
  error (reuse `FanoutNoValue` if the message parameterizes cleanly, else next free
  L-code L1611); `emit_fanout` drops its dead `_kind` parameter. Mostly reuse plus
  deletion.
- `flow-check`: the effects walk discriminates on node kind — a `Fanout` node opens
  the illegal-effect context unconditionally (`Void` is unreachable behind the
  parse-clean precondition), `SeqBlock` recurses with sticky context. C-check-4
  simplifies; CK5's rationale upgrades from pin to theorem; OQ-C1 struck.
- `flow-ir` / `flow-interp`: **untouched** — no IR delta by pin (d).
- Users write `data -> seq { "step 1" -> println; -> save -> ret; }` — imperative
  statements, no double-wrap. The §5.2 *intent* becomes expressible in Core.
- Composite value semantics change: seq no longer joins branch tails into a tuple.
  Nothing in the corpus, examples, or tests relies on the pack.

## Spec impact (exact files/sections to patch; patched? yes/no)

Level A patched via erratum, the ADR-0012/LC-3 precedent: **LC-5** — user-guide §5.2
prose ("wrap the fanout in `seq`" → block wording) + the canonical example rewritten
in statement form. §8.6 (channels, full-language) untouched. §5.4 decision table
unchanged. `HANDOFF.md` §4.1 fanout line amended (`seq { … }` block for ordering).
**Patched (Session 11, plan WP4):** LC-5 recorded in `docs/spec/ERRATA.md`;
user-guide §5.2 prose + canonical example rewritten in statement form with an
inline marker; `HANDOFF.md` §4.1 amended. As-built deltas from this ADR's
forecasts are recorded in the plan's "As-built" section:
`docs/components/syntax/plans/plan-seq-block.md`.
