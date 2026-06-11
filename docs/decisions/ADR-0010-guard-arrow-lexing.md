# ADR-0010: Guard arrows are single lexemes, recognized by adjacency plus a statement-initial context gate

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

Guard arms are written `-true->`, `-false->`, `-_->`, and `-0->`/`-42->` (user-guide §3.4;
`architecture.md` §2.2.1 lists "Guards: `-true->`, `-false->`, `-pattern->`" at the **lexer**
level). The spec exhibits these lexemes but is silent on two questions the P1 lexer cannot
dodge. **(1) Adjacency.** Inside a guard block, `-7-> x;` (an arm with discriminant 7 whose
payload flows to `x`) and `-7 -> x;` (the value −7 flowing into `x`) consist of the *same*
token sequence if guards are assembled from `Minus Int Arrow` — only whitespace
distinguishes them. Whitespace is therefore semantically load-bearing, and a token stream
that erases it cannot be parsed correctly: the guard arrow must be one token, produced by
the lexer. **(2) Context.** An unconditional single-token rule mis-lexes ordinary
arithmetic: `a-1->c` (tightly written `(a-1) -> c`) would become `a` `Guard(Int 1)` `c`.
Every guard arm the v0.2 corpus exhibits is statement-initial — preceded by `{`, `;`, or
`}` (verified across user-guide §3.4/§3.5 incl. value-match, nested-loop, and
binary-search forms, and all six `examples/*.flow`) — while expression-position `-` never
is. Both questions were settled during the Session 02 lexer design (DESIGN.md §5) and are
recorded here because they fix user-visible surface behavior, the same caliber of decision
as ADR-0009.

## Decision (one paragraph, imperative)

Lex a guard arrow as the **single lexeme** `-D->` with **zero interior whitespace**, where
the discriminant `D ∈ { true, false, _, [0-9]+ }` (the Flow-Core guard set; ADR-0001).
Attempt this production **only when** the most recently emitted token is `{`, `;`, or `}`
(or at start of input); otherwise, and whenever the pattern fails to match exactly, lex the
leading `-` ordinarily (`->` if `>` follows, else `-`). Accept the resulting wart:
statement-initial *tight* negation `-7->x;` lexes as a guard arm — write `-7 -> x;` (as
the spec's own style always does); the parser must report stray `Guard` tokens outside
guard blocks with an "add a space" hint, and report `- true ->`/`-true ->` inside guard
blocks with a "guard arrows are written without interior spaces" hint. An over-`u64`
discriminant clamps to `u64::MAX` with the same diagnostic as integer literals (lexing is
total). Core+1 pattern guards (`-Some(x)->`, `-[head, ...tail]->`) contain nested structure
and **cannot** be single lexemes; they stay outside this rule and will be resolved in the
Core+1 coproducts ADR (expected: parser-level composition with span-adjacency checks, with
the Core discriminant set staying single-token).

## Consequences (tradeoffs, implementation impact)

- Lexing is deterministic and whitespace-honest: the `-7-> x` / `-7 -> x` distinction
  survives into the token stream, which a `Minus Int Arrow` decomposition cannot achieve.
- The lexer carries one piece of state (kind of last emitted token) for the gate — the
  standard contextual-lexing hack (cf. regex-vs-division in JS), confined to one production.
- Warts, accepted: `-7->x;` at statement start is a guard arm (all three gate contexts,
  incl. after `}`); `i<-1` is `i <- 1` by maximal munch (same family, no special case).
  Both get targeted parser hints; neither occurs in spec-style code, which spaces operators.
- Because `>` is never a discriminant-start character, the rule can never capture a plain
  `->`; fanout arms (`-> square;`) are safe even where the gate passes.
- The six `examples/*.flow` and every guard form the user-guide exhibits lex correctly with
  zero diagnostics under this rule (verified by three-way independent design review,
  Session 02; pinned by golden token-stream tests in `flow-syntax`).

## Spec impact (exact files/sections to patch; patched? n/a)

None — the spec exhibits the lexemes but never states adjacency or context rules, so no
v0.2 text is corrected (nothing for ERRATA). The binding record is this ADR; the
implementation contract lives in `docs/components/syntax/DESIGN.md` §5/§6 and is enforced
by the golden/unit tests named there. Flagged to Sapir in `docs/next-session.md` (Session
02) with the W1 wart called out; revisable by a superseding ADR if vetoed. patched? n/a.
