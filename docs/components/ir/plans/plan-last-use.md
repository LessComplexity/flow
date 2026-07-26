# Plan: last-use analysis in mapal-ir — one query, three consumers

Status: **shipped (query), 2026-07-22 wave** — sequencing step 1 (`mapal-ir`: the query + tests) landed; the backend consumer rows (steps 2-3) and the §6.1 testgen brute-force row land with their backend owners (testgen is unreachable from mapal-ir's tests — it imports mapal_interp, downstream of mapal-ir — so agreement coverage rides the consumers' differentials). · deviations: (i) **rule 2's "any path … (through Pair fields and Phi arms)" is realized as escape-by-retention-only** — the walk traverses backward ONLY value-retaining/-aliasing morphisms: `Pair`, `Phi`, `Output`, `LoopExit`, `LoopEnter` (the init aliases the merge's first instance), `Proj`, `Call` (the borrowed boundary may return an alias), and array-typed `Index` (the sub-buffer alias); computational edges (`Add`/`Sub`/…, `Update`/`Map`/`Fold`/`Zip`/`Enumerate`, comparisons, `Neg`/`Not`, `Print`) consume and produce anew — no escape flows through them (a chain's intermediates would otherwise all "escape" through the final `Add`, and the rule would be vacuous); (ii) **rule 2's escape is further refined for carried state** — the merge, its `Proj` views, and the back-route state cone do NOT escape through their own loop's `LoopExit` (the per-iteration release valve; the escaping final instance is protected via the exit OBJECT, which is not exempt) — without this exemption the merge escapes and rule 3's matmul4 note ("`c`'s only uses are the Update and the swap ⟹ legal") is uninstantiable; (iii) **rule 1 is realized as a total per-loop permutation** — each canonical loop's morphisms are re-ranked decide < `LoopExit` < advance < `LoopBack` within the topo slots they already occupy; the plan text named only the `LoopBack` rank, but the `LoopExit` rank between the cones is load-bearing (it's what makes the exit-route retention pin distinguish the legal matmul4 shape from an illegal decide-cone write-after-pack into the same buffer); (iv) **`death` is also `None` for use-less objects** (no use position exists; `dead_after` still answers `true`); (v) nested canonical loops with overlapping body slots skip the inner loop's re-ranking (rule 6 degradation); (vi) the back-route carried cone enters through the route's STATE field (slot 0) only — the cond at slot 1 is consumed, never carried. · Written: 2026-07-22 · Session 20 (optimization marathon), feeding the W3 wave.
Source: backend-cuda suggestions #2 (back-edge freeing) + #18a (arena v1.1 coloring), backend-llvm suggestion #2 (Update memcpy elision), Futhark uniqueness/last-use (PLDI'17 §3) + Mojo ASAP-destruction — Mapal's sealed DAG makes last-use a trivial deduced query; no type-system machinery needed.
Scope: a new deduced query in `mapal-ir` (BL7 pattern, alongside `loop_plan`) + its three backend consumers. Language semantics untouched: every consumer is representation-only (values pure; interp reads values, not pointers).

## 1. Why

Three recorded debts share one missing fact — *"is this buffer dead here?"*:

1. **CUDA loop-carried O(k·n) residency** (loops.rs:37-38): every iteration's buffers leak to fn exit.
2. **Update full-copy** (BC5 / llvm func.rs:576-579): `c[i] <- x` copies the whole source even when the source is never read again — the canonical loop-carried array case (matmul4: `c' = update(c,…); back edge c := c'` — in-place makes the swap an identity and deletes both the copy and the per-iteration malloc).
3. **Arena v1.1 (#18a)**: merging buffers with disjoint live intervals (Futhark coloring) needs live intervals.

All three are one analysis, and it belongs in mapal-ir next to `loop_plan` — computed once, every backend inherits it, property-testable in one place.

## 2. The categorical model

```
last_use : CategoryIr × FuncId → LastUsePlan          (deduced, total, deterministic — L2)
```

| Object | Meaning |
| --- | --- |
| `LastUsePlan` | per-object death positions + escape/carried classification for one fn |
| `Use` | a consumption site of an object's value (out-edge morphism, or a loop-special role) |

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `last_use` | `IR × FuncId → LastUsePlan` | Deduced | the whole plan — pure function of the sealed graph |
| `death` | `ObjectId →? TopoIdx` | Partial | the greatest topo position of any use; ⊥ for objects that escape the fn |
| `escapes` | `ObjectId → 𝔹` | Total | reachable into `Output`/`Return` (incl. through Pair fields), or a `Parameter` (borrowed), or a capture source owned by an outer body |
| `carried_by` | `ObjectId →? LoopMerge` | Partial | the value crosses a `LoopBack` into `merge` — it lives "into the next iteration" |
| `dead_after` | `ObjectId × TopoIdx → 𝔹` | Deduced | all uses ≤ idx ∧ ¬escapes ∧ ¬carried — the consumer-facing predicate |

Composition rules (invariants the consumers rely on):

1. **Oracle order is topo order.** Uses are indexed by `topo_order(f)`; a use in the decide cone precedes any in the advance cone of the same iteration (ADR-0016 orders them so). `LoopBack` ranks past every body morphism of its loop.
2. **Escape is conservative.** Any path from `o` to an `Output`/`Return` object (through `Pair` fields and `Phi` arms), any `Parameter`, any object owned by an outer fn read as a capture ⟹ `escapes(o) = true` and `death(o) = ⊥`. Never freed, never written in place. (This is the S15 `escape_lvalues` analysis lifted to the IR — the backends' pointer-value guard stays as the second line of defense.)
3. **Carried is two-iteration liveness.** `carried_by(o) = m` means: alive from its definition until the *next* iteration's back-edge swap. In-place writes to a carried buffer are legal iff every non-`LoopBack` use of `o` sits strictly before the writing morphism within the iteration (matmul4: `c`'s only uses are the Update and the swap ⟹ legal).
4. **In-place Update legality** (the consumer rule, stated once here): `Update(s,…)` may write in place iff `dead_after(s, idx(Update))` under rule 1 — i.e. no use of `s` at or after the update, `¬escapes(s)`, and (loop case) rule 3's ordering. Borrowed/init handles fail rule 2 and are never written.
5. **Determinism (L2).** Same graph ⇒ same plan; iteration order follows `topo_order`.
6. **Totality + cheap.** `O(V+E)`, recursion-free (J1), total on any sealed fn — non-canonical loops just make carried analysis partial (consumers fall back to today's behavior on `None`).

## 3. Consumers (the three rows, one per backend + the arena follow-up)

| Consumer | Uses | Change | Pays |
| --- | --- | --- | --- |
| backend-cuda in-place Update | rule 4 | `update_site`/twin Update: when legal, write into the source handle (no fresh buffer, no full copy — BC5 amended with an explicit as-built note) | kills the matmul4-class per-iteration malloc+copy |
| backend-cuda back-edge freeing (suggestion #2) | rules 2-3 | at the back edge, free the merge's outgoing handle iff it is a registered allocation ∧ ¬escapes ∧ not the borrowed init (pointer-value guard extended) | the remaining carried shapes (map-in-loop etc.) |
| backend-llvm Update elision (suggestion #2) | rule 4 | `emit_update`: skip `llvm.memcpy`, reuse the source alloca when dead | the N⁴-copy wall in loop forms |
| arena v1.1 (#18a, recorded, later) | `death` intervals | interference coloring merges disjoint buffers; loop-cone zones | capacity = max-clique not sum |

Trap semantics unchanged everywhere (the bounds guard fires identically — in-place changes *where* the value lands, never whether a guard fires). f64/IEEE untouched (same values, same order).

## 4. What does NOT change

`Ty`, `Operation`, sealing, validate; the oracle; R1; the escape guard's pointer-value epilogue (it remains as the emitted-text-level guard); arena v1.0 zones (in-place *shrinks* the cone-site malloc count — the v1.1 note in the arena plan already anticipates this).

## 5. Perf contract

- **Structural (CI):** matmul4-class loop form: the cone `update_site` emits no `cudaMalloc` and no full-copy kernel for the carried update (in-place = element write kernel into the same handle, or fused per the emitter's shape); llvm loop-form `.ll` contains no `llvm.memcpy` for the carried update. Structural pins in `golden_cu.rs`/`golden_ll.rs` + unit tests of the query (chain graph, diamond, carried pair, escape-via-pair-field, borrowed-init, two-sequential-loops).
- **Measured (box sweep):** loop-form matmul wall + MAPAL_PERF launch counts/kinds; llvm loop-form already at the 0.01 s floor locally — the sweep keeps it there (regression watch).

## 6. Sequencing

1. `mapal-ir`: the query + property tests (determinism, totality, agreement with a brute-force reference over testgen graphs).
2. backend-llvm elision (smallest consumer; differential covers it at -O0/-O2).
3. backend-cuda in-place Update, then back-edge freeing (each with structural gates; remote differential re-run on the box).
4. Record arena v1.1 (#18a) as the follow-up that consumes `death` intervals — not built in this wave.

## 7. Risks

- **Mis-classified escape ⟹ use-after-free/in-place corruption.** Mitigated by rule 2's conservatism + the pointer-value epilogue guard (a wrong in-place target still compares unequal to escapes) + the R1 differential over testgen (the decisive gate — 1280 compile-and-runs at two opt levels on llvm, 640 on cuda).
- **Nested loops:** carried analysis is per-merge; an inner loop's carried object feeding an outer merge resolves by composing rules 1-3 per loop; non-canonical shapes degrade to `None` (status quo).
- **Phi arms:** a value selected by `Phi` lives if either arm's use is live — the use-walk treats `Phi` as a use of both arms (conservative, correct).
