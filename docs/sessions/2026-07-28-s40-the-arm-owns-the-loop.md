# 2026-07-28 — S40: the arm owns the loop, and the review that found seven more

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017).
Driven by Sapir. Continues `2026-07-28-s39-guards-gate-the-flow.md` (same day).

## 0. Continuation brief

Current state: **plan-s40 SHIPPED and the gate is GREEN — 1006 passed, 0 failed, fmt clean.**
S39's §4a P0 is closed: gating is stable across `LiftLoops`. A 17-agent adversarial review of the
first build confirmed seven further defects — one proven by execution as an interpreter panic on a
valid graph — and all seven were fixed in this same session, including a pre-existing S39-class
DCE instability the 1024-case hammer then reproduced independently. Zero surface emissions moved
(A/B re-proven after every fix round). Work is **uncommitted on `main` @ `8b40442`**, S39 + S40
together — Sapir's call on committing.
Next step: S41 per `docs/next-session.md` — the verdict-stability invariant test (is DCE really
the only pass that can flip a gate?), then the NVPTX leg.
Resume command/check: `cargo test --workspace --release` (green), `git status --short`.

## 1. The defect, and the fix's shape

S39 §4a: `guard_plan`'s v1 refusal skipped any site whose arm work touched a loop SCC. Refusal
keyed the site's *semantics* on graph shape; `LiftLoops` removes the SCC while preserving meaning,
so the same site was strict raw and gated rewritten — `eval ∘ rewrite = eval` broken, three seeds
pinned.

The replacement (plan-s40 §2–§3): **loops join an arm as UNITS.**

- A unit is exactly the region the flat walk hands the driver: SCC-incident morphisms + machinery
  + the `loop_plan` cones.
- **Machinery is never per-morphism ownable.** Per-morphism consumer closure provably cannot
  complete a cycle (no member can be first), so the only machinery it reaches alone is the exit
  boundary — and gating that fragment starves the driver. This is the precise explanation of
  S39's failed "delete the refusal" attempt (`route object built before read`).
- A canonical unit joins iff every consumer outside it of every member's target is already owned,
  and is represented in `own` by its `LoopEnter` handle alone. Internals stay out — subtraction,
  re-close, and every consumer's gated skip-set never see them. The handle carries the unit's
  transitive `can_trap`; a loop is `heavy` by definition.
- A non-canonical unit (no `loop_plan`) never joins; it also cannot lift (`R-LF`/`R-LM` consume
  `loop_plan`), so refused-raw ⇒ refused-rewritten — no stability hole.
- Two topologies, one mechanism: (a) loop-inside-arm gates through the handle — interp fires
  `run_loop` from the Phi, LLVM emits the loop CFG inside the branch; (b) arm-inside-loop-body
  gates through ordinary closure once machinery is barred — the LLVM loop emitter's cones gained
  the gated-skip interp's driver already had (an asymmetry invisible while (b) sites could not
  exist).
- `path_plan` folds a gated loop into its Phi's sequential Seq component instead of minting a
  launch-dispatched loop task; a gated loop's bulk members are never `Split`.
- ConstFold refuses to fold a Phi whose arm owns a `LoopEnter`, transitively through nested
  sites; the fixpoint driver refolds after `LiftLoops` turns the arm into droppable `Map`/`Fold`.

**CUDA skipped (Sapir, at plan gate): "cuda can be skipped it will be translated to nvptx later
anyways."** The host emitter keeps strict semantics for any gated site touching loop machinery or
in-SCC work (a ctx-build site filter). Goldens unmoved; surface Mapal cannot build the shapes
(L1406); the CUDA differential does not run.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Fix CUDA host emitter too | **skipped (Sapir)** | replaced by NVPTX; strict-fallback filter keeps local emission valid at zero dispatch work |
| Unit granularity = flat-walk driver region | kept | any smaller fragment starves the driver; any larger is not what consumers skip |
| Handle-only representation in `own` | kept | subtraction/re-close/skip-sets stay untouched; internals belong to the driver |
| Delete `Option` from `guard_arm`'s return | kept | nothing refuses at site level any more |
| Fix review find [5] by pinning dead readers in DCE's plan | **rejected — measured** | replay materializes only READ objects; the pinned product vanished in the rebuild (probe: addpair kept in plan, absent in output) |
| Fix [5] by making dead `Temporary` sinks ownable in `guard_arm` | **rejected — caught by invariants** | a dead cone can read BOTH arms' values; gating it into one arm reads the other's un-fired value. The ownership suite's disjointness assert caught it ("arms share …") before any eval did |
| Fix [5] by DCE pinning tainted dead SINKS in the verdict cone | **kept** | pinning the sink keeps the whole cone through replay; the taint filter (observable-when-executed ops only) keeps pure dead cones droppable — the untainted version moved `sepia`, the un-narrowed version moved `matmul4_loop`, both caught by the A/B sweep |
| Pin [5] red as S41's opener | **overtaken** | the 1024-case hammer reproduced the class through `open_default` once fix [3]/[6] kept dead Phis alive — it blocked the gate, so it was fixed now |

## 3. The adversarial review round

17 agents (4 Opus dimension reviewers → 13 findings → 13 Opus refuters; 6 refuted, 7 confirmed;
~1.6M tokens). One finding was **confirmed by execution**: the verifier built the reviewer's graph
against the working tree from an out-of-repo probe crate and produced
`internal error: read before write of object` on a valid graph, both input polarities, with the
8b40442 baseline evaluating the same graph to a well-defined `Trapped(DivZero)`.

The seven, all fixed and pinned in `mapal-rewrite/tests/guard_loops.rs` (details plan-s40 §6b):

1. **Re-close could not unwind a joined unit** — the handle's test inspected `out_edges(merge)`
   only (all members, vacuously covered), so the subtraction cascade could drop the unit's payload
   consumers while the loop stayed gated → un-gated survivors read what the gated loop never
   wrote. Handle now re-tested with the JOIN predicate; dropped when the unit's outputs leak.
2. **Sink members joined vacuously** — `all()` over an empty consumer list; a loop writing the
   function's Return gated, and the untaken side panicked (`non-Unit return is always written`).
   Observable sinks now refuse the join.
3. **In-body arm work double-owned** — loop-invariant exclusive work of an in-body guard is not a
   unit member, so the enclosing arm owned it too. The subtraction now sees nested Phis through
   owned handles.
4. **DCE's dead-Phi pin keyed on `can_trap` alone** — a heavy-but-trap-free gated site (arm owning
   a non-terminating trap-free loop) lost its Phi while RW11 kept the loop: Done raw, Diverged
   rewritten. Pin now keys on `gated()`.
5. **ConstFold's drop was unconditional, its alias conditional** — `forward` refuses an SCC/token
   winner; dropping the losing arm + triple anyway made replay panic ("feeder is not mapped").
   The drop now mirrors the alias conditions exactly.
6. = 4 (same defect, second statement).
7. **[5] DCE flipped strict→gated by deleting a dead sibling reader** — pre-existing S39-class,
   no loop required: a trap-capable value with a dead second consumer is strict; drop the reader
   and the site gates, suppressing a trap (`Trapped(DivZero)` raw → `Done(42)` rewritten). Fixed
   via the verdict-cone tainted dead-sink pin (decision table above records the two rejected
   designs). The class question for the other five passes is S41's — the hammer exercises all six
   with `PhiTrapArm` and only DCE fell.

## 4. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| 3 pinned LiftLoops seeds (`closed_default`, `open_default`, `trap_free`) | pass | S39 §4a closed |
| `PROPTEST_CASES=1024 … --test property` | green, ~6.5 s | no fresh instability; the run that convicted DCE mid-session now passes with its seed retained |
| `mapal-ir/tests/algos.rs` (+3: unit join, shared loop stays unconditional, in-body arm) | 79/79 | the rule's three cases; the in-body test caught the self-enclosing-unit hole on first run |
| `mapal-interp/tests/guards.rs` (+3, builder-built) | 11/11 | untaken loop does not run; taken loop still traps; in-body untaken arm never fires |
| `mapal-rewrite/tests/guard_loops.rs` (new, 6) | 6/6 | all seven review findings as regressions |
| `guard_ownership.rs` + unit-atomicity asserts | green (139+ sites) | S39 invariants intact (edge-last, disjointness, closure); handles carry whole units |
| LLVM differential (`--test differential`) | 37/37, 419.6 s | backends agree with the oracle across the 1,280-run sweep |
| CUDA suite | 163 green | strict-fallback filter; goldens unmoved |
| **A/B emission vs `8b40442`**, 3 faces × all benches + examples | **103 identical, 1 differs (`calc` raw), 0 new emit failures** | S40 moved zero surface emissions — re-proven after the review fixes; two intermediate designs moved `sepia`/`matmul4_loop` and were caught here |
| **`cargo test --workspace --release`** | **1006 passed, 0 failed** | the whole gate |
| `cargo fmt --check` | clean | — |

Perf: nothing owed — emission byte-identical everywhere reachable from surface (measurement rules
9/10). No timing runs taken.

## 5. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `8b40442` | **uncommitted** — S39 + S40 together | `git status --short` | Sapir's call |
| worktree | `…/scratchpad/s40/pre` @ `8b40442` | A/B PRE tree — removed at close | `git worktree list` | done |
| worktrees | three stale (`d3ca82c`, `6168863`, `1daddaa`) | S33/S38 debt, not mine, left alone | `git worktree list` | `git worktree prune` after removing dirs |
| scratch | `…/scratchpad/s40/`, `…/probe/`, `…/reclose-check/` | A/B binaries + probe crates (mine + the review verifiers') — session-scoped, will vanish | `ls` | none needed |
| untracked | `oainotes.md` | S39's external review notes | `head oainotes.md` | Sapir's call |

Re-run the A/B: the S39 log §6 procedure verbatim (build `--example emit -p mapal-backend-llvm`
in both trees, copy each binary out before building the other — the CUDA crate's example collides).

## 6. Open items

| P | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | Verdict-stability across the OTHER five passes | next-session §1 | invariant test: per pass, `guard_plan` verdicts pre/post over the testgen corpus, no flip either direction | test in tree, green, or a convicted pass fixed |
| P0 | GPU leg via NVPTX | S38 §6, ADR-0033 | write the plan | matmul site on the 4090 through PTX, bit-exact |
| P1 | Per-task enable predicates in `mapal-rt` | `ponytail:` in `path_plan` | unchanged | guarded big map dispatches parallel |
| P1 | ADR "guards gate the flow" (+ the unit rule) | plan-s39 §9 Q3, plan-s40 | write it; amend ADR-0026 Q8 | ADR merged |
| P1 | Beat OpenBLAS at ONE thread | S33 | unchanged | 1t parity on the i9 |
| P1 | Inlining stamps spliced morphisms with call-site position | plan-s38 §6.1 | unchanged | counterexample passes |
| P2 | testgen topology (b): `PhiTrapArm` inside a loop body | plan-s40 §6a.3 | add the Step | (b) sites in the differential census |
| P2 | Oracle clones captured arrays per fold step | S37 | plan first | differential < 60 s |

## 7. Architecture / model changes

`guard_plan` §13 rule 4 (DESIGN.md): loops join an arm as units; `LoopUnit` (private) with
members/enters/canonical/can_trap; `guard_arm` loses its `Option`. `GuardArm.heavy` gains
`LoopEnter`. DCE gains the verdict-cone tainted dead-sink pin; its dead-Phi pin keys on `gated()`.
ConstFold's losing-arm drop mirrors `forward`'s refusal and skips loop-armed Phis. No new
`Operation`, no `Ty` change, no `validate()` change. Coherence: unchanged from S39 — the gate is a
placement fact; the unit is one `Trn` whose realization is the driver.

## 8. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/ir/plans/plan-s40-the-arm-owns-the-loop.md` | written; §6a found-while-building; §6b the review round |
| `components/ir/DESIGN.md` | §13 `guard_plan` rule 4 (units); deduced-query table row |
| `components/ir/IMPLEMENTATION.md` | functor row + test rows updated |
| `components/ir/STATUS.md` | S40 header + review round |
| `components/rewrite/STATUS.md` | S40 header (the three rewrite-side fixes) |
| `docs/STATUS.md` | S40 roll-up |
| `docs/next-session.md` | rewritten for S41 |
| this log | new |

## 9. Files changed

Code: `mapal-ir/src/algo.rs` (guard_plan units + path_plan gated-loop folding) ·
`mapal-interp/src/eval.rs` (flat-walk gated LoopEnter + Phi driver dispatch) ·
`backends/llvm/src/func.rs` (two walks + Phi branch dispatch; `gated` pub(crate)) +
`loops.rs` (cone gated-skip) · `backends/cuda/src/func.rs` (strict site filter) ·
`mapal-rewrite/src/equations.rs` (loop-armed fold refusal; alias-mirrored drop) +
`graph_rewrites.rs` (gated() pin; verdict-cone tainted dead-sink pin).
Tests: `mapal-ir/tests/algos.rs` (+3) · `mapal-interp/tests/guards.rs` (+3) ·
`mapal-rewrite/tests/guard_loops.rs` (new, 6) · `guard_ownership.rs` (unit-atomicity) ·
`property.proptest-regressions` (+1 DCE seed, auto-pinned).
Docs: as §8.

## 10. Method notes earned

1. **Adversarial review after "all green" pays.** The full gate was 1000/0 and every differential
   passed when the review round found seven real defects, one an interpreter panic on a valid
   graph. Independence found what the suites' shapes could not.
2. **Replay materializes only read objects.** Pinning a dead product in a rewrite plan does not
   survive the rebuild — pin the SINK and the cone follows backward.
3. **Dead-sink ownership is a cross-arm trap.** A dead cone can read both arms' values; the
   disjointness invariant caught the design before any eval did. Invariant suites earn their keep
   on new designs, not just regressions.
4. **Run the A/B sweep after every fix round, not once.** Two semantically-sound designs moved
   surface emissions (`sepia`, `matmul4_loop`); the sweep was the only check that saw it.
5. **A quarantined red test does not stay quarantined.** The [5] pin was meant to ship red as
   S41's opener; the hammer routed the same class through `open_default` within the hour, so it
   was fixed now. If a defect class is live, the property suite will find its own path to it.
