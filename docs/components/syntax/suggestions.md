# syntax — suggestions (category-theory derived)

> Improvements deduced from DESIGN.md's categorical model by FRAMEWORK rules. Each cites
> its rule and names the concrete change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |

_Suggestion 1 (`LineIndex` owned-`String` copy → borrow `&'a str`) applied Session 09._

## Already-ledgered reductions — cited, not re-proposed

Per the task's firewall on re-litigating settled reductions:

- **`GuardKind` (token) vs `GuardDiscr` (AST).** A §3 consolidation reader would flag these as
  near-twins. DESIGN's Categorical-model preamble already ran the reduction: `GuardDiscr` **is**
  `GuardKind` *plus* the `OutOfCore` morphism (pattern arms `-Some(x)->` are not single lexemes,
  so the token level cannot carry `OutOfCore`). "Extend, don't parallel" — they stay distinct by
  design. No change proposed.
- **Two `SourceLoc`s (`flow-syntax` ↔ `flow-ir`), surface `Ty` vs IR `Ty`.** Explicitly kept
  separate (D8 stored-copy at the crate seam; distinct objects resolved by `flow-lower::tys`).
  Ledgered in DESIGN Bridges table + `docs/architecture/categorical-model.md` §7. Not touched.
- **Stored `Int` values (`GuardKind::Int(u64)`, `ExprKind::Int(u64)`).** Would read as a
  deduce-don't-store candidate (value re-derivable from the span digits), but DESIGN §3 / C14
  already justify carrying them (digits live *inside* the guard lexeme; array lengths and guard
  discriminants need the value; clamping mirrors L0008 with no duplicate diagnostic). Kept.

No other unledgered smells found.
