# plan-s40: the arm owns the loop

Status: **IN BUILD** — S39's §4a P0. Continues `plan-s39-guards-are-conditional.md`;
session log `sessions/2026-07-28-s39-guards-gate-the-flow.md` §4a.

**Amendment (Sapir, at plan gate): CUDA is skipped** — "cuda can be skipped it will be translated to
nvptx later anyways." The CUDA host emitter keeps STRICT semantics for any gated site touching loop
machinery or in-SCC work (a site filter at ctx build, `backends/cuda/src/func.rs`), so its emission
stays valid and its goldens stay put with zero dispatch work. Surface Mapal cannot build the shapes
(L1406), so no compiled program diverges; the divergence exists only for testgen IR on a differential
that does not run. NVPTX inherits gating from scratch.

## 1. The defect

`LiftLoops: Trapped(IndexOob) !≈ Done(I32(1))` — `eval ∘ rewrite = eval` broken. Three seeds pinned
(`property.proptest-regressions`): `closed_default`, `open_default`, `trap_free`, all
`PhiTrapArm` + a loop step. Gate: 992 passed, 2 failed.

`guard_plan`'s `guard_arm` **refuses** a site when any owned candidate is incident to a loop SCC or
is `LoopEnter`/`LoopBack`/`LoopExit` (`algo.rs:3317`, "v1 refusal"). `LiftLoops` removes the SCC, so
the same site is refused raw and gated rewritten. The gated side is correct; the raw side must gate.

Blast radius is IR-only: L1406 rejects `-> loop` in a Phi arm and `lower` never puts loop machinery
in an arm. `testgen` builds it directly.

## 2. The model — why refusal is the wrong mechanism, categorically

The gate is a **placement fact**: which `TrnLoc`s fire, decided by the flow condition (plan-s39 §3).
The v1 refusal makes gate-ability a function of the graph's *shape* (SCC presence). Rewriting
preserves meaning while changing shape — `LiftLoops` sends a loop SCC to a `Map`/`Fold` realizing
the same morphism — so any shape-conditioned semantics fails to commute with rewrite. This is not a
coding slip in the refusal; refusal-by-shape is unsound as a mechanism (S39 §4a said this; this plan
cashes it).

**The loop unit.** The flat walk already partitions a function into "mine" and "the driver's":

```
unit(SCC) = machinery(LoopEnter/LoopBack/LoopExit)
          ∪ loop_plan cones (decide_order ∪ advance_order)
          ∪ every morphism incident to an SCC object
```

(`eval.rs:100-146`, `func.rs:2090-2121` — both consumers compute exactly this set and skip it.)
That set is one `Trn` at the arm's altitude — the driver is its realization — with inputs (init cone
value, captures) and outputs (the exit objects). FRAMEWORK §4.1: a transformation is an object you
place; the unit is placed *as a whole* under the gate or not at all.

**Why per-morphism closure cannot own a loop.** The backward worklist admits a candidate only when
all consumers of its target are already owned. In a cycle no member can be first — the closure
provably never completes an SCC, which is why the only SCC-incident morphisms that ever get owned
are exit-boundary ones (`LoopExit`), and why S39's "just delete the refusal" attempt died with
`route object built before read`: fragments of loop machinery got gated, the driver then skipped
them, and the route object was never built. Ownership must be decided at **unit granularity**.

## 3. The rule

1. **Consumer closure lifts to the quotient.** In `guard_arm`'s walk, a candidate that lies in a
   unit stands for the unit. Unit `U` joins iff for every `m ∈ U`, every consumer of `target(m)`
   outside `U` is already owned. On join, all of `U`'s morphisms enter `owned_set`, and the walk
   continues into `U`'s external inputs (in-edges of unit sources produced outside `U`).
2. **The `LoopEnter` is the unit's handle in `own`.** Unit internals stay OUT of every own-list.
   Consequences, each load-bearing:
   - the nested-site subtraction, the re-close fixpoint, and ConstFold's transitive drop machinery
     never see unit internals — no new fragmentation mode;
   - the `gated` skip-set (own-lists, all four consumers) contains the `LoopEnter` but not the
     cones, so a driver actually invoked runs its cones exactly as today (`run_loop`'s gated-skip
     keeps meaning "nested arm work", unchanged);
   - `own` stays topo-ordered for free — the handle sits at the `LoopEnter`'s topo position, after
     the init cone, before the exit consumers.
3. **Flags are computed over the whole unit.** `can_trap` |= any unit morphism
   `path_trap_capable` (this is what the seeds' `Index` needs). `heavy` |= true — a loop is heavy by
   definition (unbounded trip count); `LoopEnter` joins the heavy op list.
4. **A non-canonical unit never joins** (an SCC whose `loop_plan` is `None` — multi-merge,
   non-builder shapes). It runs unconditionally, which is always safe, and it cannot destabilize:
   `R-LF`/`R-LM` consume `loop_plan` facts, so a shape the driver can't run is a shape the rewriter
   can't lift — refused raw ⇒ refused rewritten. The v1 site-refusal is deleted, not relocated.

## 4. The four consumers + ConstFold

| Consumer | Change |
| --- | --- |
| `mapal-interp/src/eval.rs` | flat walk's `LoopEnter` arm checks `gated` first (skip — the Phi fires it); Phi arm firing dispatches: `LoopEnter` → `run_loop(ctx, target, budget)`, else `eval_morphism` as today. `eval_morphism` itself never sees machinery (not in own-lists). |
| `backends/llvm/src/func.rs` | `walk`/`walk_filtered` `LoopEnter` arms check `gated`; Phi branch emission dispatches `LoopEnter` → `loops::emit_loop`. |
| `backends/cuda/src/func.rs` | same two changes, host emitter (mirror). |
| `path_plan` (`algo.rs:1569`) | a gated loop must NOT mint its own launch-dispatched `Seq` task — its morphisms join the Phi's scalar component, exactly like gated bulk ops (`ponytail:` marker stands: per-task enable predicates stay future). |
| ConstFold (`equations.rs:97-111`) | **refuse to fold** a Phi whose losing arm's own-list contains a `LoopEnter`. Dropping a whole loop unit is replay surgery this defect does not need; the fixpoint driver refolds after `LiftLoops` turns the arm into droppable `Map`/`Fold`. `ponytail:` marker with that upgrade path. |

## 5. Build order

1. `guard_plan`: unit map (per SCC: morphisms, canonical?), quotient closure in `guard_arm`, flags,
   delete the site refusal. Unit tests in `mapal-ir/tests/algos.rs`.
2. interp (the seeds' oracle) — the three pinned seeds are the acceptance here.
3. ConstFold refusal + `PROPTEST_CASES=1024`.
4. LLVM walk + Phi dispatch; `path_plan` Seq-folding; 1,280-run differential.
5. CUDA host mirror; goldens (expect zero movement — no surface program is affected).
6. `guard_ownership.rs`: add unit-atomicity invariant (no own-list contains a proper fragment of a
   unit; a `LoopEnter` in an own-list implies its whole unit's flags reached the arm).

## 6. Tests

| Check | Proves |
| --- | --- |
| 3 pinned seeds | the defect, both polarities |
| `PROPTEST_CASES=1024 cargo test -p mapal-rewrite --test property` | no fresh instability (this is what found §4a) |
| `algos.rs` new: arm-owned loop; loop shared with outside stays unconditional; non-canonical never joins | the rule's three cases |
| `guards.rs` new: hand-built gated-loop arm, taken and untaken | driver fires iff arm chosen; untaken loop's trap/effect never fires |
| 1,280-run LLVM differential | backends agree with the oracle on testgen shapes |
| A/B emission, benches + examples, three faces | zero surface movement, byte-identical |
| full gate + fmt | 994 passed, 0 failed |

## 6a. Found while building

1. **A site inside a loop can vacuously "own" its enclosing unit.** From inside the unit, every
   member's external consumers are trivially satisfied (the whole world is the unit), so the
   quotient join test passed for an in-body arm and handed the arm its own enclosing loop. Caught by
   the new `guard_plan_gates_arm_inside_loop_body` unit test on first run. Rule: a site whose
   boundary edge is itself a unit member never joins that unit — the driver already runs the site.
2. **Two topologies, one mechanism.** (a) loop-inside-arm gates through the unit handle; (b)
   arm-inside-loop-body gates through ordinary per-morphism closure once machinery is barred from
   per-morphism ownership — the S39 "delete the refusal" failure was machinery ownership alone, and
   closure self-protects everything else (anything a non-gated cone morphism reads stays ungated,
   because its consumers are not all owned). (b) needed one extra line: the LLVM loop emitter's
   cones now skip gated morphisms, which interp's driver already did (S39 wired interp, not LLVM —
   the asymmetry was invisible while (b) sites could not exist).
3. **testgen builds only topology (a)** (`PhiTrapArm` picks pool values; Phis are top-level).
   Topology (b) coverage is the two hand-built tests (`algos.rs`, `guards.rs`); no differential
   coverage. Recorded as a gap, acceptable because (b) is reachable from surface Mapal only via
   loop-body guards, which the A/B emission check sweeps.

## 6b. The adversarial review round — seven confirmed findings, all fixed

A 17-agent review workflow (4 dimension reviewers, 13 findings, every finding independently
attacked; 6 refuted) confirmed seven defects in the first build. One was proven by execution — the
verifier built the repro against the working tree and produced an interpreter PANIC on a valid
graph. All seven are fixed and pinned in `mapal-rewrite/tests/guard_loops.rs` (6 tests):

1. **[0] Re-close could not unwind a joined unit.** The handle's own re-close test inspected
   `out_edges(merge)` — all members, vacuously covered — so when the nested-site subtraction
   cascade dropped the unit's payload consumers, the handle survived, the loop stayed gated, and
   the un-gated survivors read an object the gated loop never wrote (`read before write` panic,
   both polarities). Fix: the handle is re-tested with the JOIN predicate against the surviving
   list, and dropped — loop unconditional, always safe — when the unit's outputs leak.
2. **[1] Sink members joined vacuously.** `all()` over an empty consumer list is true, so a unit
   member writing the function's Return joined and gated observable work (`non-Unit return is
   always written` panic on the untaken side). Fix: sink targets refuse the join, mirroring the
   per-morphism rule.
3. **[2] In-body arm work double-owned.** An in-body guard's loop-INVARIANT exclusive work (fed by
   constants) is not a unit member, so the enclosing arm owned it too — same `Div` in two
   own-lists. Fix: the nested-site subtraction treats a Phi inside an owned unit as gated through
   the handle and strips its work from the enclosing arm.
4. **[3]/[6] DCE's dead-Phi pin keyed on `can_trap` alone.** A heavy-but-trap-free gated site
   (an arm owning a non-terminating trap-free loop) lost its Phi to DCE while RW11 kept the loop:
   Done raw, Diverged rewritten. Fix: pin on `gated()`.
5. **[4] ConstFold's drop was unconditional, its alias conditional.** `forward` refuses an SCC or
   token winner; dropping the losing arm + triple anyway made replay rebuild a boundary edge
   against a dropped feeder ("feeder is not mapped" panic). Fix: drop only when the alias provably
   fires (same refusal conditions, mirrored).
6. **[5] DCE flipped strict→gated by deleting a dead sibling reader** — PRE-EXISTING S39-class,
   no loop needed: a trap-capable value with a dead second consumer is strict; drop the reader and
   the site gates, suppressing the trap (`Trapped(DivZero)` raw, `Done` rewritten). The 1024-case
   hammer independently reproduced it through `open_default` once fix 4 kept dead Phis alive.
   **Fix path mattered:** (a) pinning the dead *product* in DCE's plan does not survive replay
   (replay materializes only read objects); (b) making dead `Temporary` sinks *ownable* in
   `guard_arm` is unsound — a dead cone can read BOTH arms' values, and gating it into one arm
   reads the other's un-fired value (caught by the ownership invariants' disjointness assert);
   (c) shipped: DCE pins every dead `Temporary` SINK that is forward-reachable from the verdict
   cone (backward cone of Phi triples + touched units' member targets) AND tainted (its cone
   contains an observable-when-executed morphism — partial op, call, body, effect, machinery).
   Pinning the sink keeps the whole cone through replay; the taint filter keeps pure dead cones
   droppable, so surface emissions do not move (`sepia` regressed under the untainted version and
   came back byte-identical under the tainted one). The class question for OTHER
   consumer-set-changing passes is recorded for S41 — the 1024-case hammer exercises all six
   passes with `PhiTrapArm` and only DCE fell.

Invariant updates earned: none — `guard_ownership.rs` kept all S39 invariants (edge-last,
disjointness, closure) and gained the unit-atomicity asserts; the sink-ownable experiment that
would have relaxed them was reverted before shipping.

## 7. Not doing

- Per-task enable predicates in `mapal-rt` (unchanged P1).
- Gating *fragments* of a loop body per-arm — the unit is atomic by design.
- Gating non-canonical loops.
- `Ty::Sum` (ADR-0026 unchanged).

## 8. Done when

Three seeds pass, 1024-case property run green, full workspace gate green, emission A/B
byte-identical outside testgen-only shapes, docs reconciled (DESIGN §13 `guard_plan` entry gains the
unit rule; IMPLEMENTATION functor row updated).
