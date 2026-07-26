# cli — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Each cites its rule and names the concrete
> change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 define each boundary once | Each crate carries its own renderer-free error enum; the CLI would grow one renderer per enum | One declared `Diagnostic` target every crate's errors map into; the CLI renders exactly one type (the soft candidate in [categorical-model.md §7.5](../../architecture/categorical-model.md)) | One renderer, not N; the boundary is declared once |

## Detail
### 1. Single diagnostic contract
Already derived in the Session-06 audit (soft — revisit when `mapal-cli` is built); see
[categorical-model.md §7.5](../../architecture/categorical-model.md) item 2. Explicitly
**not** a merge of `IrError`/`IrViolation` — those stay two (§7.2, oracle independence).
