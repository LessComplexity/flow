# rewrite — suggestions (category-theory derived)

> Improvements deduced by FRAMEWORK rules. Not applied — a backlog for future work.
> Last: 2026-07-18 · S13 (post-U5).

| # | Rule (§) | Smell found | Proposed change | Payoff |
| --- | --- | --- | --- | --- |
| 1 | §5 deduce-don't-store (precision) | DCE pins every `Div/Mod/Index/Call/Map/Fold` result live even when provably total (const-nonzero divisor, in-bounds const index, transitively loop-free callee) | Implement DESIGN §3.1's refined removability rows | More dead code removed; strictly R1-safe (superset-conservative today) |
| 2 | §3 consolidation | Lower mints one `Constant` per use site; CSE cannot dedup them (P1 forbids keying Constants) | A replay-side skip-constant channel keyed by `(ty, bit pattern)` | Smaller graphs for backends; DESIGN §3.2/§11 records the design |
| 3 | §9.2 naturality (Level A) | The four `Zip`/`Enumerate` laws sit as data (`naturality.rs`), no pass | Layer-2 pass with a cost direction per law | Free rewrites already proven by ADR-0018's NT status |
| 4 | §4.3 composition | Non-canonical loop shapes take the whole-graph identity (RW8) though the builder can express multi-merge SCCs (ir algos.rs nested test) | Generic-SCC replay via nested `LoopHandle`s | Unlocks rewriting the lower-reachable inner-exits-via-`ret` nested loop; coordinate with interp M1 lifting |
| 5 | §2 layer-3 / ADR-0021 §3 | `Update` laws L-b (`index_j ∘ update_i`, i≠j const in-bounds → `index_j` of the base) and L-c (`update_i ∘ update_i` const in-bounds → outer write) are unimplementable: the `RewritePlan` `alias`/`constify`/`drop`/`fuse` channels all rewrite a morphism's *result*, never re-source a *surviving* op's *operand*. Only L-a (index∘update at equal const, which aliases the Index result to an existing object) fits the current plan. | Add a `reoperand : MorphismId → (slot, ObjectId)` plan channel + its replay wiring (re-point a kept op's input feeder), then land L-b/L-c in `equations.rs` | Const-index reads past a non-matching write fold to the base array (L-b); redundant writes to the same slot collapse (L-c) — both trap-conservative like L-a |
