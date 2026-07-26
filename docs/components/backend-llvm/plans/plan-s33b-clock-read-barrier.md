# plan-s33b — a clock read must be a barrier in both directions

Status: **SHIPPED S36 (2026-07-27), §3 as written.** The fix landed in `mapal-ir`'s `path_plan`,
not in the runtime or the emitter: `crates/mapal-rt/` has a zero-line diff and the emitter needed
no new concept — the pinned-task machinery built for trap-capable calls already carried it. One
earlier approach was built, measured and REVERTED; §4 records why, and it stays do-not-retry.
Results and the revised acceptance are in §7.

## 1. The defect

`() -> time` cannot time a parallel task, because **workers do not stop at checkpoints.**

```llvm
mapal_par_launch
mapal_par_wait(%h, @ckpt0_entries, i32 5)   ; the generation tasks
%t0 = call double @mapal_time_ms()          ; t0
mapal_par_wait(%h, @ckpt1_entries, i32 7)   ; adds task6, task7 — the kernel
%t2 = call double @mapal_time_ms()          ; t1
```

S29 fenced the clock read against everything written *before* it, so `t0` is correctly after the
generation tasks. Nothing stops the tasks written *after* it from starting before it. The moment
`ckpt0` is satisfied the DAG unlocks task6 → task7 and the workers begin, while the host is still
between the wait and its next instruction. When the kernel finishes early, `t1 - t0` collapses.

Measured: **3–4% of threaded runs**, `MAPAL_PAR=1` 0/100 (no workers, so nothing can run ahead).
One live case self-timed a 1024² matmul at **0.0001 ms**. Pre-existing — PRE 3/100, POST 3/100
against a `HEAD~1` runtime, so unrelated to plan-s33's `reside`.

Consequence recorded in `docs/performance/matmul/s33.md` §5: **min is the wrong statistic for any
threaded cell** (this race makes *fast* outliers, so min is maximally vulnerable and worsens with
N), and every `par` minimum in S28–S32 is suspect.

## 2. Categorical model (FRAMEWORK §2 + §4)

A clock read is a `Trn` — `now : IoToken → (IoToken, ℝ)` — that the emitter places on the **host
spine** rather than in the task DAG. That placement is the defect, stated exactly.

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `now` | `IoToken → IoToken × ℝ` | Total | read the clock |
| `before` | `now → Task*` | Total | tasks that must be Done when it fires (S29: source order) |
| `after?` | `now → Task*` | **MISSING** | tasks that must not have STARTED when it fires |

`before` exists; `after?` does not. **§4.5 Law 1 in its ordering form:** a transformation placed at
a location may not observe state that a transmission has not yet delivered *or has already
overwritten*. `t1 - t0` is meant to observe the interval containing task7's execution; today it
observes an interval that task7 may have already left.

The fix is to make `after` total. Two ways, and the model says which is principled:

- **`now` becomes a DAG node.** Then `after` is just `dependents(now)` — ordinary edges, no new
  concept, and the existing dependency machinery enforces it. This is the §3 Consolidation move:
  a clock read is not a new kind of thing needing its own fencing rules, it is a `Trn` with a
  degenerate payload, and it should be placed the same way every other `Trn` is.
- **A dispatch ceiling in the runtime.** Holds work above the current checkpoint. This is a
  *parallel* mechanism to the dependency graph that encodes the same ordering — exactly the
  redundant-morphism smell §3 warns about. It is also what was tried, and it does not work (§4).

## 3. The shape of the real fix

`path_plan` already computes clock-read fences. Make the clock read a task with edges **both
ways**: `before` as today, plus an edge from it to every task written after it in source order.
Then task7 cannot be dispatched until the clock-read task has completed, by the same rule that
already orders everything else. Emitter-side, `mapal_par_task` gains a pinned scalar node for the
read (pinned tasks already exist and already run on the host spine, so the machinery is there).

Cost: one extra DAG node per clock read, and a genuine loss of overlap across the read — which is
*correct*, because that overlap is precisely what makes the measurement wrong.

## 4. REVERTED: the runtime-only dispatch ceiling. Do not retry it.

Built in full, measured, reverted. `RunState` gained `ceiling: i64` + `held: Vec<usize>`;
`schedule()` held any task above the ceiling; `mapal_par_wait` raised it to the max task index in
its entry set; `mapal_par_finish` released it.

**Two independent reasons it fails, both measured:**

1. **It does not fix the race.** With the ceiling starting unbounded and clamped by the first
   wait, the failure rate was **5/100 vs 4/100 unfixed** — unchanged. The window between
   `mapal_par_launch` and the first `mapal_par_wait` is wide enough to lose in: fir's generation
   tasks finish in ~20 µs across 14 lanes, which is ample time to unlock and enqueue the kernel
   before the host reaches its first wait.
2. **Closing that window breaks the launch contract.** Starting the ceiling at "hold everything"
   fixes reason 1 but fails two existing tests, and they are right to fail:
   `watermark_wait_can_finish_before_task_completion` calls `mapal_par_launch` and then requires
   task 0 to be *running* before any `mapal_par_wait` exists;
   `wait_helps_while_the_background_worker_is_busy` likewise. **Launch must dispatch immediately.**

The root reason is structural, and it is the useful part of this failure: **at launch the runtime
does not know where the first checkpoint is.** Only the emitter does. So no runtime-only ceiling
can be both correct and non-breaking — the ordering information has to come from the emitter, and
once the emitter is supplying it, an edge in the DAG is strictly simpler than a second mechanism
that shadows the DAG (§3).

## 5. Acceptance

| # | Check | Done when |
| --- | --- | --- |
| 1 | The race is gone | fir 65 536, `MAPAL_PAR=14`, 100 runs: **0/100** readings under 0.01 ms (baseline 3–4/100) |
| 2 | Min becomes usable again for par | min and median within noise of each other on every par cell |
| 3 | No value change | gate green; outputs byte-identical; `MAPAL_PAR=1` unaffected |
| 4 | Launch contract intact | `watermark_wait_can_finish_before_task_completion` and `wait_helps_while_the_background_worker_is_busy` pass **unmodified** — they are the guard rail this plan's first attempt tripped |
| 5 | Cost named | measure the overlap lost across a clock read; a benchmark-only construct must not slow the shipping path |
| 6 | S32 re-confirmed | re-run the scheduling verdict (1.41–1.43×) under a now-trustworthy min |

## 6. Not in scope

The `par` numbers stay medians until check 1 passes. Nothing about this changes the 1t figures,
which were never affected (`MAPAL_PAR=1` is 0/100 by construction).

## 7. As built (S36) — results, and what acceptance 2 actually says

**Where it landed.** `CategoryIr::path_plan` (`crates/mapal-ir/src/algo.rs`). A `TimeMs` morphism
stops being host-only (`is_clock` lifts it out of `is_host`/`is_scalar`), becomes its own
single-morphism `Seq` task with `pinned = true`, and gets edges both ways off `Morphism.loc.start`:
tasks with `task_max_loc < start` become its `deps`, tasks with `task_min_loc > start` gain a dep
**on** it. The two sets are disjoint and a read is in neither of its own, so no clock edge can close
a cycle. A `TimeMs` still on the spine — inside an effectful loop region — keeps its `Checkpoint`;
one that became a task drops it, because the pinned sequence already emits the `mapal_par_check` at
that topo position and a second wait list would state the same ordering twice (§3).

**Measured (M4 Pro, `benches/shapes/fir_65536.mapal`, `MAPAL_PAR=14`, 100 runs each):**

| | readings < 0.01 ms | min | median | min/median |
| --- | ---: | ---: | ---: | ---: |
| before | **6/100** | 0.000125 | 0.0666 | 0.22 |
| after | **0/100** | 0.0423 | 0.0745 | 0.57 |

| # | Check | Verdict |
| --- | --- | --- |
| 1 | The race is gone | **MET** — 0/100 against a 6/100 baseline re-measured on the same HEAD |
| 2 | Min usable for par | **MET as amended below** |
| 3 | No value change | **MET** — fir prints byte-identical (`2169`/`1405`); gate green at 972 passed, 0 failed; `shapes_ab.sh` verification "baselines byte-equal; Mapal FMA rel-error ≤ 1e-4" |
| 4 | Launch contract intact | **MET** — `crates/mapal-rt/` untouched; `watermark_wait_can_finish_before_task_completion` and `wait_helps_while_the_background_worker_is_busy` pass unmodified |
| 5 | Cost named | **MET** — total wall time of the fir binary is unchanged (median 2.73 → 2.62 ms, min 2.44 → 2.47, n=25). The self-timed interval RISES ~12% because work that used to start before `t0` is now inside the bracket: the fix does not slow the program, it stops the measurement from excluding work it was supposed to contain |
| 6 | S32 re-confirmed | **open** — unblocked by 1–5, but it needs the pre-S32 A/B leg rebuilt, which is its own campaign |

## 8. Cross-machine validation (S36b) — the acceptance re-checked on a second machine

`benches/results-s36/` — seven shapes, each emitted by the compiler at `35fb681` (pre) and
`896fb3c` (post) from a worktree so both binaries exist at once; n = 100 per cell; three machine
configurations. **8,400 timed runs.**

The sharp test is not the 0.01 ms counter, which is calibrated to fir and missed a **1494×**
reading on matmul (par min 0.0209 ms against a 31.22 ms 1t median — above the threshold, so the
counter recorded zero). A cell is *impossible* when `1t_median / par_min` exceeds the machine's
thread count:

| Configuration | impossible, PRE | impossible, POST | sub-0.01 ms, PRE | sub-0.01 ms, POST |
| --- | ---: | ---: | ---: | ---: |
| M4 Pro, unpinned (14) | 3 / 7 | **0 / 7** | 13 | **0** |
| i9-14900F, unpinned (32T) | 5 / 7 | **0 / 7** | 12 | **0** |
| i9-14900F, pinned (8P/16T) | 3 / 7 | **0 / 7** | 9 | **0** |

Post-fix maxima: Mac 8.6× (matmul), pinned box 11.6× (gather, 16 hardware threads), unpinned box
24.4× (transpose — inside the 32-thread bound but above 24 physical cores, and an artifact of the
same governor ramp: against that cell's 1t *min* rather than its ramp-inflated median it is 3.8×).
Two limits worth stating: `reduce` and `saxpy` never exhibited the defect in any pre log, so their
post-fix result is a control rather than a repair; and n=100 with zero hits bounds the residual rate
near 3%/run at 95%, it does not zero it. The `MAPAL_PAR=1`
control is unmoved on the pinned box and the Mac (pinned: conv2d 0.0496 → 0.0503, matmul 17.3485 →
17.3874; the unpinned box swings up to 34% on a leg that cannot race, which is the ramp)
and all seven shapes print byte-identical output pre and post. Acceptance 1 and 3 therefore hold on
a second architecture, and acceptance 2's amendment below is confirmed there: the residual par
spread tracks kernel size and pool dispatch, and both ends of it are physically reachable.

**Acceptance 2, amended by measurement.** "Min and median within noise on every par cell" is not
achievable and was never about this race: the residual gap tracks kernel size, because what is left
is pool wake-up jitter. fir 65 536 (~0.07 ms of kernel) sits at min/median 0.57; fir 1 048 576
(~0.39 ms) at **0.84**, and neither has a single sub-0.01 reading in 60–100 runs. The distribution
is unimodal and continuous from the low tail up — a race leaves a cluster three orders of magnitude
below the body, which is what 6/100 looked like. So the honest rule is: **min is trustworthy again
where the kernel is large enough to dominate pool wake-up, and the tiny cells stay on medians for a
reason that has nothing to do with the clock.**
