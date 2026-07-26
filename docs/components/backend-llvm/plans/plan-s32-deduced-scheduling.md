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
`mapal-rt:slice_ranges` computes

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
| `work_per_element : Task → ℕ` | `Trn`, **deduced**, **mapal-ir** | the body's op count, weighted by op class. A graph fact — a property of the program, not of any machine — and the ONE thing mapal-ir must learn here. Absent today: `Task.rank` weights are element counts (`algo.rs:1207`), and the crate has no op counting at all |
| `bytes_per_element : Task → ℕ` | `Trn`, deduced, emitter | from the recorded `TileRead` strides and `elem` width; already derivable, uncomputed |
| `intensity : Task → ℚ` | `Trn`, deduced | `work_per_element / bytes_per_element` — the quantity §1(a) shows the optimum tracking |
| `footprint : Task → ℕ` | `Trn`, deduced | live bytes per dispatch, from the site's array extents. Distinguishes matmul 1024 (wants 8) from matmul 4096 (wants 14) at *equal* intensity |
| `Dispatch` | `Dat` | `{ task, width : ℕ, grain : ℕ, lanes : 𝒫(Loc) }` — the reified scheduling decision. `width` and `grain` are **independent**, which is the structural fix of §1(d) |
| `schedule : (PathPlan × TargetProfile) → Task ⇀ Dispatch` | `Trn`, **deduced**, emitter-local | the compile-time scheduler. Total on `Split` tasks, undefined on `Seq`/pinned ones (they are host-spine by construction) |
| `levels : PathPlan → (𝒫(Task))*` | `Trn`, deduced | the DAG's antichains — the sets of tasks that may run **concurrently**. This is the object step 3 needs and `path_plan.deps` already determines |
| `RegionPlan : Task ⇀ Granularity` | `Dat`, **deduced**, emitted as data | the whole granularity nest for one region — `{tile_i, tile_j, kc, nc, slice_elems, width, lane_pref}` — computed at emission and read by the pool. The levels travel together because they constrain each other (§2.5) |
| `halo : (TileRead, ℕ) → ℕ` | `Trn`, deduced | the reuse a slice boundary re-pays, from `reuse::distinct_runs` one level up. The floor under `slice_elems` (§2.6) |
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
4. **mapal-ir learns `work_per_element` and nothing else.** Core counts, P/E ratios, cache
   sizes and lane classes are machine facts and stay in `TargetProfile` (the ADR-0032
   contract). The scheduler itself is emitter-local, consuming both.
5. **The default profile emits byte-identical text** where the deduced schedule equals today's
   (`width = threads`, `grain = width`) — the rule-1 discipline every S31 rung used.
6. **A schedule is static.** It is computed at emission and baked into the emitted dispatch
   calls. No runtime search, no adaptive feedback: a build is reproducible, and the same
   program on the same profile emits the same schedule (ADR-0034 D3's spirit).

## 2.5 The granularity nest — one record per region, sizes at compile time

Sapir, S32: *"we must know from the DAG to notate per graph area (subgraph) the parameters for
good parallelism — from tile, panels and now even slices — so the backend can structure it in a
way the runtime can do the optimal assignment."* That is the unifying statement this plan is
really about, and it reorganises the three rungs into one artifact.

**There are three nested granularities. Only two are derived.**

| level | sized so that… | derived from | today |
| --- | --- | --- | --- |
| tile (`TI`×`TJ`) | the accumulator block fits the **register file** | `vec_regs`, `vec_bytes` | ✅ S31 (`TargetProfile::tile_i/tile_j`) |
| panel (`KC`×`NC`) | the packed window stays **L2-resident** | `l2_bytes` | ✅ S31 (`TargetProfile::tile_kc/nc`) |
| **slice** | a worker's working set stays hot for the slice's duration | **nothing** | ❌ `GRAIN = 4096`, one number for every kernel on every machine |
| **width** | the dispatch is worth spreading | **nothing** | ❌ `configured_threads()`, one number per process |

`GRAIN` is the same hand-set literal `TILE_J` and `TILE_KC` were, and ADR-0034 already names it
— *"the same disease at the runtime placement"*. S31 skipped it under rule 4 because it lives
in mapal-rt rather than the emitter. **That placement split is the actual obstacle**: the
emitter knows the graph's reuse structure and cannot reach the pool; the pool knows the
machine's runtime state and knows nothing about the graph. So neither can size a slice, and a
constant sits in the gap.

**The fix is one deduced record per region, emitted as data.**

```
RegionPlan : Task ⇀ Granularity
Granularity = { tile_i, tile_j, kc, nc, slice_elems, width, lane_pref }
```

computed at emission from (graph facts × `TargetProfile`), baked into the emitted dispatch,
and read by the pool. The whole nest travels together because the levels are not independent —
a slice that is smaller than the panel it re-packs is incoherent, and a width that exceeds the
slice count is idle lanes by construction.

**The principle that decides what goes where:**

> **Compile time decides the SIZES. Runtime decides the ASSIGNMENT.**

The compiler cannot know which core a slice will land on, whether that core is a P or an E,
what else is resident, or how the machine is loaded — so it must not try to assign. The
runtime cannot know the reuse structure, the halo cost of a boundary, or the working-set
footprint — so it must not try to size. Each decides exactly what it can see, and the
`RegionPlan` is the interface between them. That also states step 1's job precisely: the pool
stops *inventing* sizes and starts *receiving* them.

## 2.6 Grain is bounded on both sides, and both bounds are computable

The slice is not a free parameter sitting between two vague preferences. It has a floor and a
ceiling, and this plan claims both are deducible today.

**Ceiling — load balance.** `slices ≥ width × oversubscription`, or the deques have nothing to
steal (§1(c)). This is the truck problem and it is pure counting.

**Floor — the halo.** A slice boundary re-pays whatever reuse crossed it. For a read that
slides across rows, adjacent output rows share all but `q` of their tap-rows, so **every extra
boundary costs `(k/div − 1)·q` re-read rows**. That is `distinct_runs`' arithmetic
(`crates/backends/llvm/src/reuse.rs`, shipped in S31 for register blocking) evaluated one level
up: the same query prices a block of rows and a slice of rows.

The consequence is a **falsifiable prediction, and it already has two data points**:

| read classification | halo per boundary | predicted response to finer slicing | measured |
| --- | --- | --- | --- |
| `Invariant` (matmul `b`, `ci == 0`) | **0** | free — slice as fine as balance wants | matmul512 **+68%** at 14t under 8× oversubscription |
| `Sliding{q}` (conv2d `b`, `ci == cq`) | `(k/div − 1)·q` rows | costly — every boundary re-reads the window overlap | conv2d **−13…20%** under the same change |

**One recorded fact — `i_reuse` — predicts both signs.** The implementation must reproduce
that: a site whose sliding read has zero halo may be sliced to the balance ceiling, and a site
with halo must trade halo against imbalance rather than take a global constant. matmul1024's
31% regression at 14t under oversubscription is the third case and the harder one — its `b` is
`Invariant`, so the halo term does not explain it; the panel-residence term does (fewer rows
per slice amortising the same pack), which is why `slice_elems` and `kc`/`nc` must be decided
**together in one record** rather than by two independent rules.

## 2.7 Region plans compose — plans are built recursively on the graph

Sapir, S32: *"this is composable — combining regions is combining plans into a bigger plan,
allowing us to create plans recursively on the graph."* This is the FRAMEWORK §4.3 rule
(a composite's views are **deduced** from its parts, never re-described) applied to scheduling,
and it changes the shape of the work: **co-scheduling stops being a third mechanism and becomes
the composition operator.**

A region is any subgraph of the execution DAG. A leaf region is one `Split` task. Two regions
combine in exactly two ways, because the DAG offers exactly two relationships:

| operator | when | how the plans combine |
| --- | --- | --- |
| `A ▸ B` (sequential) | `B` depends on `A` | the machine is handed over whole: each keeps its own width and grain. The composite's cost is additive; its width is the max, not the sum |
| `A ∥ B` (concurrent) | neither depends on the other — an antichain of `levels` | lanes are **apportioned**: `width(A ∥ B) = width(A) + width(B)` when that fits, otherwise share by `rank` (the recorded critical-path weight) and narrow the rest. Grain is unchanged — it is a locality property of each region, not of the pair |

Both operators are associative, and the empty region is the unit, so **regions and their plans
form a monoid** and a plan for the whole program is built by folding the DAG. That is what
makes "recursively on the graph" precise rather than a metaphor.

Two consequences worth stating, because they are what the structure buys:

1. **The scheduler is one fold, not three passes.** Deduce leaves (§2.6), then fold up the DAG
   with `▸` and `∥`. Step 3 adds no new deduction — it adds `∥`.
2. **Grain is invariant under composition; width is not.** Grain answers "how big a piece keeps
   this region's working set hot", which no sibling can change. Width answers "how much of the
   machine does this region get", which every sibling changes. That asymmetry is why the two
   knobs had to be separated (§1(d)) before composition could mean anything — and it is the
   test: composing must never rewrite a grain.

The same structure is what carries to other backends: on CUDA `∥` is stream assignment and `▸`
is a stream dependency, over the identical record.

## 3. The steps — all three committed

### Step 1 — the pool stops inventing sizes and starts receiving them

`mapal_par_task` carries the region's `Granularity` — at minimum `width` and `slice_elems` —
and `slice_ranges` stops collapsing them. This is the §2.5 principle made concrete: the pool
no longer derives sizes from `GRAIN` and `configured_threads()`, it applies the ones the
compiler computed, and keeps only what it alone can see — which lane, when, and who steals.
Stealing becomes live: with `grain > width` there is finally something to steal, which is what
lets an E-core take fewer pieces without anyone deciding it should.

- `slices = clamp(grain, 1, n)`, `lanes = the width lanes chosen for this dispatch`;
- enqueue confined to the dispatch's lane set, so a narrow dispatch genuinely uses few cores
  rather than merely creating few pieces;
- **defaults reproduce today exactly** (`width = threads`, `grain = width`), so R0 alone is a
  no-op on every measured number — the safety property, checked not asserted.

### Step 2 — the sizes are deduced per region

`schedule` computes `(width, grain)` per `Split` task from `intensity`, `footprint` and `n`
(graph) against cores, P/E split and cache sizes (profile). §1(a) and §1(c) are the fixture:
the deduction must independently reproduce **2 / 4 / 8 / 8 / 14** for the six measured kernels
and must pick over-decomposition for matmul512 while refusing it for conv2d.

This is where `work_per_element` enters mapal-ir — the single graph fact the whole S31/S32 line
has been missing, and the one thing here that is legal to put there.

### Step 3 — region plans compose (this is the DAG rung, and it is the same operator)

Step 2 answers *how wide is this dispatch*. Step 3 answers *what else runs at the same time* — and
by §2.7 that is not a new mechanism, it is `RegionPlan` composition.
Two independent tasks each wanting 4 lanes on a 14-lane machine should run **concurrently on
8**, not sequentially on 4 with 10 lanes idle. `path_plan.deps` already determines the
antichains; nothing new is needed from the graph.

- `levels` partitions ready tasks into concurrent sets;
- within a level, apportion lanes by each task's deduced width, breaking ties by `rank` (the
  recorded critical-path weight — the longest chain should not be the one that waits);
- when `Σ width > lanes`, prefer the critical path and narrow the rest rather than serialize;
- when `Σ width < lanes`, widen the critical path into the slack.

**This step is not gated on discovering a program that needs it.** The benchmark programs in
§4 are deliverables of this plan, written *for* it — a language whose thesis is that the
compiler sees the whole graph must be designed for wide graphs before one shows up, or the
scheduler will be shaped wrong.

## 4. Benchmark programs — deliverables, written for the rungs

None of these exist; every current bench is a single-site pipeline, which is why the DAG rung
has never been exercised.

| program | shape | what it must show |
| --- | --- | --- |
| `mixed_widths.mapal` | conv2d@1024 **then** matmul@8192 in one program | R1: two dispatches in one process, deduced 4 and 14. A global width is wrong for at least one of them — this is Sapir's own example and the plan's headline case |
| `wide_small.mapal` | 8 independent small maps, no deps | R2: co-scheduled onto one wave. Serialized, this is 8× a dispatch that cannot fill the machine |
| `wide_big.mapal` | 4 independent matmuls, each wanting ~4 lanes | R2 under contention: `Σ width = 16 > 14` — the apportionment rule, not just the packing |
| `deep_narrow.mapal` | a long dependent chain of small maps | the negative control: no co-scheduling is possible, so R2 must cost nothing |
| `critical_path.mapal` | one long task beside several short ones | R2's tie-break: the long task must get the lanes, which `rank` already knows |

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
- **Composition law tests** (§2.7): plans for two regions composed must equal the plan deduced
  for their union, on both the sequential and the concurrent operator.
- **`work_per_element` oracle tests** in mapal-ir, against hand-counted bodies.
- **Step 1 is a no-op at its defaults** — every measured number unmoved before R1 sets the knobs.

## 6. ADR-0033 D2 — the three-line record

- **Record fields consumed:** `PathPlan.tasks[*].{kind, deps, rank}`, `TaskKind::Split{site, n}`,
  and via the site, `TileSite.{rows, c, k, elem}` + `TileRead` strides. **Added:**
  `work_per_element` — a genuine graph fact, the only mapal-ir change in this plan.
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
   an emitter table. Either it grows a runtime half, or mapal-rt gets its own profile and the
   emitter passes a schedule it cannot fully validate. Recorded as a collision in
   plan-s31-target-profiles; it must be settled before step 2.
2. **Does `work_per_element` weight op classes?** A divide is not an add. Unweighted is
   simpler and probably sufficient for a width decision; weighted is more honest and invites
   the "what are the weights" question that ADR-0034 answers with a search.
3. **Is the E-core answer exclusion or uneven slicing?** Step 1's over-decomposition lets stealing
   balance them automatically, which may be enough. If not, the profile must expose lane
   classes and the scheduler must slice unevenly — a bigger commitment.
