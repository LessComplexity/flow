# lower — suggestions (category-theory derived)

> Improvements deduced from DESIGN.md's categorical model by FRAMEWORK rules. Each cites
> its rule and names the concrete change. Not applied — a backlog for future work.

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 one-source-of-truth (watch-item) | `tys.rs:detect_cycles` and `effects.rs:find_cycle` are the same iterative white/gray/black DFS with an identical `Frame { node, refs, cursor }` explicit stack, forked across two files. | A generic `fn dfs_cycles<N: Ord+Clone>(nodes, succ, on_back_edge)` in a small graph util; callers pass the back-edge callback. | One traversal to test and fix instead of two. **Deferred (§5 YAGNI):** only two call sites and their outputs genuinely differ (see Detail) — adopt when a third graph-cycle check appears, not before. |
| 2 | §5 one-source-of-truth (found S11, ADR-0019 review) | The same condition — a return-position stage with no value in an **effectful** fn body-tail — draws different codes by stage kind: `seq` gets the uniform L1611 (via the S11 `ChainCtx::RetValue`), a value-less `Fanout` still falls through to the generic L1306, because `emit_fanout`'s `continues` predicate (emit.rs:2030) excludes `RetValue`. Pre-existing behavior, deliberately left by the WP2 fixer (out of ADR-0019 scope). | Add `ChainCtx::RetValue` to `emit_fanout`'s `continues` match, promoting the fanout no-value case to its own precise code (or parameterized L1305), mirroring seq. | One return-position no-value seam; users get "fanout produces no value" instead of the generic "incomplete return". Small diff; needs one negative test per the L1611 pattern. |

| 3 | §5 one-source-of-truth (found S29, plan-time-builtin reconcile) | "Is this stage an effect?" is asked at **four** independent sites — `effects.rs:NameWalk::chain` (Pass B), `typing.rs:body_effect_span` (L1605), `emit.rs:scan_phi_arm` (L1404) and `emit.rs:effect_chain` (`loop_body_has_effect`, the loop's token-carrying decision). `time` was added to the first two only, so the last two tested `is_print_builtin` alone (**the immediate half was applied in S29** — all four now test both; the structural half below is what remains). This is exactly the drift LD25 created `is_print_builtin` to stop, and it has now fired twice (`println` in S-earlier, `time` in S29). | Immediate: add `\|\| crate::is_time_builtin(text)` to both `emit.rs` sites (+ one L1404 rejection test and one loop-shape test: the loop must carry the token). Structural: hoist the shared predicate into one `fn stage_is_effect(source, stage, fn_sigs) -> bool` in `lib.rs` (or `effects.rs`) and have all four call it — the fourth call site is the one §5 asks for. | **Immediate half APPLIED S29** — it closed a **validate-clean miscompile**: `() -> time` in a loop body was hoisted out of the cycle (token absent from `U`), so the clock read once instead of per iteration; and a Phi arm could hold a clock read where `print` is rejected. Pinned by `time_inside_a_loop_stays_inside_the_loop`. Then no fifth effect builtin can miss a site. |

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
