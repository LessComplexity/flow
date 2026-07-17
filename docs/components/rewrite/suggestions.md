# rewrite — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Not applied — a backlog for future work.
> Last: 2026-07-18 · S12 (post-P4).

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 deduce-don't-store (precision) | DCE pins every `Div/Mod/Index/Call/Map/Fold` result live even when provably total (const-nonzero divisor, in-bounds const index, transitively loop-free callee) | Implement DESIGN §3.1's refined removability rows | More dead code removed; strictly R1-safe (superset-conservative today) |
| 2 | §3 consolidation | Lower mints one `Constant` per use site; CSE cannot dedup them (P1 forbids keying Constants) | A replay-side skip-constant channel keyed by `(ty, bit pattern)` | Smaller graphs for backends; DESIGN §3.2/§11 records the design |
| 3 | §9.2 naturality (Level A) | The four `Zip`/`Enumerate` laws sit as data (`naturality.rs`), no pass | Layer-2 pass with a cost direction per law | Free rewrites already proven by ADR-0018's NT status |
| 4 | §4.3 composition | Non-canonical loop shapes take the whole-graph identity (RW8) though the builder can express multi-merge SCCs (ir algos.rs nested test) | Generic-SCC replay via nested `LoopHandle`s | Unlocks rewriting the lower-reachable inner-exits-via-`ret` nested loop; coordinate with interp M1 lifting |
