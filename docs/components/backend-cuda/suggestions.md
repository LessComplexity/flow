# backend-cuda — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Each cites its rule and names the concrete
> change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §4.4 / §7.4 strategy 2-category | Three backend crates will realise one contract `CategoryIr → TargetText`; no shared contract is declared yet | Fix a shared `Backend` trait + `TargetText` type by ADR **before** the first backend is written (the firm candidate in [categorical-model.md §7.5](../../architecture/categorical-model.md)) | Adding a target = adjoining an object; never edits the core |

## Detail
### 1. Backend strategy 2-category
Already derived and adversarially verified in the Session-06 audit — see
[categorical-model.md §7.5](../../architecture/categorical-model.md) item 1. Owned by a
future backend ADR; recorded here so the component increment starts from it.
