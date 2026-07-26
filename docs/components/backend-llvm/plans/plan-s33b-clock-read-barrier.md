# plan-s33b — a clock read must be a barrier in both directions

Status: **OPEN. One approach built, measured, and REVERTED** — see §4. Component: `flow-rt`
(backend-llvm's runtime seam) **plus the emitter**, which is the finding.

## 1. The defect

`() -> time` cannot time a parallel task, because **workers do not stop at checkpoints.**

```llvm
flow_par_launch
flow_par_wait(%h, @ckpt0_entries, i32 5)   ; the generation tasks
%t0 = call double @flow_time_ms()          ; t0
flow_par_wait(%h, @ckpt1_entries, i32 7)   ; adds task6, task7 — the kernel
%t2 = call double @flow_time_ms()          ; t1
```

S29 fenced the clock read against everything written *before* it, so `t0` is correctly after the
generation tasks. Nothing stops the tasks written *after* it from starting before it. The moment
`ckpt0` is satisfied the DAG unlocks task6 → task7 and the workers begin, while the host is still
between the wait and its next instruction. When the kernel finishes early, `t1 - t0` collapses.

Measured: **3–4% of threaded runs**, `FLOW_PAR=1` 0/100 (no workers, so nothing can run ahead).
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
already orders everything else. Emitter-side, `flow_par_task` gains a pinned scalar node for the
read (pinned tasks already exist and already run on the host spine, so the machinery is there).

Cost: one extra DAG node per clock read, and a genuine loss of overlap across the read — which is
*correct*, because that overlap is precisely what makes the measurement wrong.

## 4. REVERTED: the runtime-only dispatch ceiling. Do not retry it.

Built in full, measured, reverted. `RunState` gained `ceiling: i64` + `held: Vec<usize>`;
`schedule()` held any task above the ceiling; `flow_par_wait` raised it to the max task index in
its entry set; `flow_par_finish` released it.

**Two independent reasons it fails, both measured:**

1. **It does not fix the race.** With the ceiling starting unbounded and clamped by the first
   wait, the failure rate was **5/100 vs 4/100 unfixed** — unchanged. The window between
   `flow_par_launch` and the first `flow_par_wait` is wide enough to lose in: fir's generation
   tasks finish in ~20 µs across 14 lanes, which is ample time to unlock and enqueue the kernel
   before the host reaches its first wait.
2. **Closing that window breaks the launch contract.** Starting the ceiling at "hold everything"
   fixes reason 1 but fails two existing tests, and they are right to fail:
   `watermark_wait_can_finish_before_task_completion` calls `flow_par_launch` and then requires
   task 0 to be *running* before any `flow_par_wait` exists;
   `wait_helps_while_the_background_worker_is_busy` likewise. **Launch must dispatch immediately.**

The root reason is structural, and it is the useful part of this failure: **at launch the runtime
does not know where the first checkpoint is.** Only the emitter does. So no runtime-only ceiling
can be both correct and non-breaking — the ordering information has to come from the emitter, and
once the emitter is supplying it, an edge in the DAG is strictly simpler than a second mechanism
that shadows the DAG (§3).

## 5. Acceptance

| # | Check | Done when |
| --- | --- | --- |
| 1 | The race is gone | fir 65 536, `FLOW_PAR=14`, 100 runs: **0/100** readings under 0.01 ms (baseline 3–4/100) |
| 2 | Min becomes usable again for par | min and median within noise of each other on every par cell |
| 3 | No value change | gate green; outputs byte-identical; `FLOW_PAR=1` unaffected |
| 4 | Launch contract intact | `watermark_wait_can_finish_before_task_completion` and `wait_helps_while_the_background_worker_is_busy` pass **unmodified** — they are the guard rail this plan's first attempt tripped |
| 5 | Cost named | measure the overlap lost across a clock read; a benchmark-only construct must not slow the shipping path |
| 6 | S32 re-confirmed | re-run the scheduling verdict (1.41–1.43×) under a now-trustworthy min |

## 6. Not in scope

The `par` numbers stay medians until check 1 passes. Nothing about this changes the 1t figures,
which were never affected (`FLOW_PAR=1` is 0/100 by construction).
