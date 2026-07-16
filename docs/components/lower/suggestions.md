# lower — suggestions (category-theory derived)

> Improvements deduced from DESIGN.md's categorical model by FRAMEWORK rules. Each cites
> its rule and names the concrete change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 one-source-of-truth (watch-item) | `tys.rs:detect_cycles` and `effects.rs:find_cycle` are the same iterative white/gray/black DFS with an identical `Frame { node, refs, cursor }` explicit stack, forked across two files. | A generic `fn dfs_cycles<N: Ord+Clone>(nodes, succ, on_back_edge)` in a small graph util; callers pass the back-edge callback. | One traversal to test and fix instead of two. **Deferred (§5 YAGNI):** only two call sites and their outputs genuinely differ (see Detail) — adopt when a third graph-cycle check appears, not before. |

> The `resolve_ty` / `TypeTable::resolve` consolidation (former suggestion 1) was applied Session 09: the shared `TyKind ⇀ Ty` skeleton is now `tys.rs:resolve_tykind` with the two callers passing only their `resolve_named` seam.

## Detail

**Suggestion 1 — why it is only a watch-item.** The two DFS *do not* compute the same
morphism: `detect_cycles` returns the whole set of cyclic nodes and emits one L1007 per
back edge (continuing the walk); `find_cycle` returns the first cycle path and stops. By
§3 step 5 the differing on-back-edge behavior is a genuine distinction, so consolidation
buys only the shared traversal skeleton, not a full merge. With two call sites (§5 YAGNI:
"an abstraction earns its place when a third call site appears"), the duplication is
tolerable today; flag it so a third cycle check (e.g. a future dependency-graph pass)
triggers the extraction instead of a third copy.
