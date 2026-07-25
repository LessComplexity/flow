# Plan — S32: scheduling is deduced at compile time from the path plan

**Status:** written pre-build, S32. Origin: Sapir, S31 close — *"we need to know that the
threading is optimized at each step of the application… we can do conv2d on 1024, then matmul
on 8192, ofc we will want more threads on the matmul than on the conv2d — and we can know it
from the execution graph properties at those points"*, and *"graph analysis gives parallelism
at each point, using this parallelism graph maybe we can schedule smarter than a simple thread
pool"*. Recorded as a founding goal of the language, not a contingent optimization: **the
compiler decides the schedule, per backend and per architecture, from what it already knows
about the graph.**

Supersedes the framing in `docs/next-session.md` §"S31 focus", which asks for *the* thread
count — one number per program. That question is malformed; see §1.

Related: ADR-0032 D4 (backend config), ADR-0033 (second-consumer obligation), ADR-0034
(candidate — constants are searched), `plan-s31-target-profiles.md` (the machine facts),
`plan-s31-deduced-blocking.md` (the same geometry-vs-constants split one level down).

## 1. Why — four measurements, and the malformed question

**(a) The optimum width spans the whole range, within one process.** Measured this session,
min-of-5..15, compute-only, M4 Pro (10 P + 4 E):

| kernel | arithmetic intensity | best width | at default 14 |
| --- | --- | ---: | ---: |
| `x*3+1` map, 8 M | ~0.25 ops/byte | **2** | 1.26× *slower than 1 thread* |
| conv2d 1024 | FMA:load 1.20 | **4** | 1.75× off its own best |
| matmul 512 | 1 MB working set | 4 (see (c)) | flat |
| fir 1 M | FMA:load 4.00 | **8** | — |
| matmul 1024 | FMA:load 8.00 | **8** | 1.10× off |
| matmul 4096 | FMA:load 8.00 | **14** | best |

A single process containing conv2d@1024 and matmul@8192 has two right answers. **"What is the
thread count" has no answer; "what is the width of *this dispatch*" does.**

**(b) The pool is not slow — it is under-specified.** Two failure modes tested and refuted:
fixed per-dispatch cost is ~0 (an 8192-element, 2-slice dispatch is timing noise at every
width, so the `notify_all` herd costs nothing measurable), and per-slice steal contention is
not dominant (16× coarser slices moved the 14-thread regression by 20% / 7%). Compute-dense
work scales cleanly — 4.01× at 4 threads on matmul512, 5.6× at 8 on matmul1024.

**(c) The actual defect is one line, and it disables load balancing entirely.**
`flow-rt:slice_ranges` computes

```rust
slices = ceil(n / GRAIN).min(threads)
```

so a dispatch is cut into **exactly one equal-sized piece per worker**. The work-stealing
deques therefore have nothing to steal — stealing is dead code in the common case — and any
core-speed asymmetry (10 P + 4 E, or thermal variance) makes the whole dispatch wait on the
slowest piece. Over-decomposing 8× measures the cost of that:

| kernel | 4t | 8t | 14t |
| --- | ---: | ---: | ---: |
| matmul512, one slice per worker | 0.775 | 0.784 | 0.778 |
| matmul512, 8× over-decomposed | 0.831 | 0.532 | **0.463** |
| matmul1024, one slice per worker | 5.506 | 3.194 | **3.546** |
| matmul1024, 8× over-decomposed | 4.971 | 2.992 | 4.639 |
| conv2d 1024, one slice per worker | **0.236** | 0.327 | 0.428 |
| conv2d 1024, 8× over-decomposed | 0.268 | 0.371 | 0.512 |

matmul512's "plateau" was never a hardware ceiling: **1.68× at 14 threads from a one-line
slicing change.** But the same change costs matmul1024 31% at 14t (more slices, worse panel
locality) and conv2d ~13-20% everywhere (per-slice cost exceeds the balance it buys).

**(d) Therefore the conclusion that orders this plan.** Width and grain are **two independent
knobs**, today collapsed into one expression and one global constant, and **no single setting
wins**: over-decomposition is +68% for one kernel and −20% for another. Making the pool
"better" is impossible; making it *capable* — able to be told a width and a grain per dispatch
— is necessary, and something must then decide those per dispatch. That decider is the
compiler, because the inputs are graph facts it already holds.

## 2. Categorical model

The schedule is a **deduced morphism**, not stored configuration — the §5 "deduce, don't
store" rule applied to placement. Its inputs split exactly where everything in this project
splits: **geometry from the graph, constants from the profile.**

| Item | Kind | Model |
| --- | --- | --- |
| `work_per_element : Task → ℕ` | `Trn`, **deduced**, **flow-ir** | the body's op count, weighted by op class. A graph fact — a property of the program, not of any machine — and the ONE thing flow-ir must learn here. Absent today: `Task.rank` weights are element counts (`algo.rs:1207`), and the crate has no op counting at all |
| `bytes_per_element : Task → ℕ` | `Trn`, deduced, emitter | from the recorded `TileRead` strides and `elem` width; already derivable, uncomputed |
| `intensity : Task → ℚ` | `Trn`, deduced | `work_per_element / bytes_per_element` — the quantity §1(a) shows the optimum tracking |
| `footprint : Task → ℕ` | `Trn`, deduced | live bytes per dispatch, from the site's array extents. Distinguishes matmul 1024 (wants 8) from matmul 4096 (wants 14) at *equal* intensity |
| `Dispatch` | `Dat` | `{ task, width : ℕ, grain : ℕ, lanes : 𝒫(Loc) }` — the reified scheduling decision. `width` and `grain` are **independent**, which is the structural fix of §1(d) |
| `schedule : (PathPlan × TargetProfile) → Task ⇀ Dispatch` | `Trn`, **deduced**, emitter-local | the compile-time scheduler. Total on `Split` tasks, undefined on `Seq`/pinned ones (they are host-spine by construction) |
| `levels : PathPlan → (𝒫(Task))*` | `Trn`, deduced | the DAG's antichains — the sets of tasks that may run **concurrently**. This is the object rung 3 needs and `path_plan.deps` already determines |
| `Loc` — worker lane | `Loc` | a pool lane, with its core class (P or E) from the profile. Lanes are physical sites; that a lane is "slow" is a machine fact |

**Composition rules.**

1. **Value-invariance (ADR-0032 D1).** No field of a `Dispatch` may change an output bit.
   Width, grain and lane assignment are performance tailors; the differential suite stays a
   valid gate under every schedule, and is run under a non-default one to prove it.
2. **R-PAR is preserved.** Output is byte-equal to the oracle at any width, grain, or lane
   set — the S24 speculate-and-order trap protocol is untouched. A schedule may not reorder
   trap observation.
3. **Width and grain are independent.** `grain` is *not* derived from `width`; a dispatch may
   be 4 lanes × 32 pieces. Collapsing them is the defect being removed.
4. **flow-ir learns `work_per_element` and nothing else.** Core counts, P/E ratios, cache
   sizes and lane classes are machine facts and stay in `TargetProfile` (the ADR-0032
   contract). The scheduler itself is emitter-local, consuming both.
5. **The default profile emits byte-identical text** where the deduced schedule equals today's
   (`width = threads`, `grain = width`) — the rule-1 discipline every S31 rung used.
6. **A schedule is static.** It is computed at emission and baked into the emitted dispatch
   calls. No runtime search, no adaptive feedback: a build is reproducible, and the same
   program on the same profile emits the same schedule (ADR-0034 D3's spirit).

## 3. The rungs — all three committed

### R0 — the pool gains the two knobs (necessary, not sufficient)

`flow_par_task` carries `width` and `grain` per task; `slice_ranges` stops collapsing them.
Stealing becomes live: with `grain > width` there is finally something to steal, which is what
lets an E-core take fewer pieces without anyone deciding it should.

- `slices = clamp(grain, 1, n)`, `lanes = the width lanes chosen for this dispatch`;
- enqueue confined to the dispatch's lane set, so a narrow dispatch genuinely uses few cores
  rather than merely creating few pieces;
- **defaults reproduce today exactly** (`width = threads`, `grain = width`), so R0 alone is a
  no-op on every measured number — the safety property, checked not asserted.

### R1 — per-dispatch width and grain, deduced

`schedule` computes `(width, grain)` per `Split` task from `intensity`, `footprint` and `n`
(graph) against cores, P/E split and cache sizes (profile). §1(a) and §1(c) are the fixture:
the deduction must independently reproduce **2 / 4 / 8 / 8 / 14** for the six measured kernels
and must pick over-decomposition for matmul512 while refusing it for conv2d.

This is where `work_per_element` enters flow-ir — the single graph fact the whole S31/S32 line
has been missing, and the one thing here that is legal to put there.

### R2 — DAG co-scheduling (committed, designed up front)

Rung 1 answers *how wide is this dispatch*. Rung 2 answers *what else runs at the same time*.
Two independent tasks each wanting 4 lanes on a 14-lane machine should run **concurrently on
8**, not sequentially on 4 with 10 lanes idle. `path_plan.deps` already determines the
antichains; nothing new is needed from the graph.

- `levels` partitions ready tasks into concurrent sets;
- within a level, apportion lanes by each task's deduced width, breaking ties by `rank` (the
  recorded critical-path weight — the longest chain should not be the one that waits);
- when `Σ width > lanes`, prefer the critical path and narrow the rest rather than serialize;
- when `Σ width < lanes`, widen the critical path into the slack.

**This rung is not gated on discovering a program that needs it.** The benchmark programs in
§4 are deliverables of this plan, written *for* it — a language whose thesis is that the
compiler sees the whole graph must be designed for wide graphs before one shows up, or the
scheduler will be shaped wrong.

## 4. Benchmark programs — deliverables, written for the rungs

None of these exist; every current bench is a single-site pipeline, which is why the DAG rung
has never been exercised.

| program | shape | what it must show |
| --- | --- | --- |
| `mixed_widths.flow` | conv2d@1024 **then** matmul@8192 in one program | R1: two dispatches in one process, deduced 4 and 14. A global width is wrong for at least one of them — this is Sapir's own example and the plan's headline case |
| `wide_small.flow` | 8 independent small maps, no deps | R2: co-scheduled onto one wave. Serialized, this is 8× a dispatch that cannot fill the machine |
| `wide_big.flow` | 4 independent matmuls, each wanting ~4 lanes | R2 under contention: `Σ width = 16 > 14` — the apportionment rule, not just the packing |
| `deep_narrow.flow` | a long dependent chain of small maps | the negative control: no co-scheduling is possible, so R2 must cost nothing |
| `critical_path.flow` | one long task beside several short ones | R2's tie-break: the long task must get the lanes, which `rank` already knows |

Measured small **and** big per Sapir's directive, at 1t/4t/8t/14t and against the deduced
schedule, with cpp/rust baselines where a baseline is meaningful.

## 5. Tests and gates

- **Byte-identity** where the deduced schedule equals today's (rule 5), over the existing
  golden set.
- **Value-invariance under a non-default schedule** — the differential suite run with a forced
  narrow width and a forced coarse grain, output byte-equal to the oracle at `-O0`/`-O2`. This
  is the R-PAR claim and it gets a test, as `differential_zen3_profile_is_value_invariant`
  does for the profile.
- **The deduction reproduces the measured optima** — a unit test over the six kernels of
  §1(a), the way `profile::tests::generic_reproduces_the_six_literals` pins the tile factors.
- **`work_per_element` oracle tests** in flow-ir, against hand-counted bodies.
- **R0 is a no-op at its defaults** — every measured number unmoved before R1 sets the knobs.

## 6. ADR-0033 D2 — the three-line record

- **Record fields consumed:** `PathPlan.tasks[*].{kind, deps, rank}`, `TaskKind::Split{site, n}`,
  and via the site, `TileSite.{rows, c, k, elem}` + `TileRead` strides. **Added:**
  `work_per_element` — a genuine graph fact, the only flow-ir change in this plan.
- **CUDA realization against the record:** the same deduction with different constants — width
  becomes block/grid geometry, grain becomes items per thread, lanes become SMs; `levels`
  becomes stream assignment, which is the direct GPU analogue of R2 and the reason to design
  R2 generically rather than for one pool. Named, unexecuted: `path_plan` still has one
  consumer.
- **Machine facts the record does not carry:** core count, P/E split and their throughput
  ratio, cache sizes, lane→core-class mapping. None exist in `TargetProfile` today — S31
  recorded this collision and deferred it; **this plan is where that debt comes due.**

## 7. Open questions for Sapir

1. **Where does the runtime half of the profile live?** The thread-count inputs (core count,
   P/E split, throughput ratio) are facts about a *runtime* placement, and `TargetProfile` is
   an emitter table. Either it grows a runtime half, or flow-rt gets its own profile and the
   emitter passes a schedule it cannot fully validate. Recorded as a collision in
   plan-s31-target-profiles; it must be settled before R1.
2. **Does `work_per_element` weight op classes?** A divide is not an add. Unweighted is
   simpler and probably sufficient for a width decision; weighted is more honest and invites
   the "what are the weights" question that ADR-0034 answers with a search.
3. **Is the E-core answer exclusion or uneven slicing?** R0's over-decomposition lets stealing
   balance them automatically, which may be enough. If not, the profile must expose lane
   classes and the scheduler must slice unevenly — a bigger commitment.
