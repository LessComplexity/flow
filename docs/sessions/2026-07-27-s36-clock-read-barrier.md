# 2026-07-27 — S36: a clock read is a DAG node

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-26-s35-shape-ladder-and-the-ast-question.md`. Repository:
`github.com/LessComplexity/mapal`.

Driven by Sapir: `start` → chose the P0 (`mapal_par_wait` clock race) → approved realization **A**
(the node lives in `path_plan`, not in the emitter) → *"Is this all about the time function? What is
the issue now? Explain simply, briefly, intuitively"*.

## 0. Continuation brief

Current state: **the S33 P0 is closed. Gate green — 972 passed, 0 failed.** A clock read is now a
pinned task in the execution DAG with edges both ways, so a threaded kernel can no longer start —
let alone finish — before the `time` read that opens its bracket. fir 65 536 at `MAPAL_PAR=14` went
from **6/100** readings under 0.01 ms to **0/100**, values byte-identical, total wall time unchanged.
Committed at `896fb3c`, tree clean, **not pushed** (the active `gh` account cannot write to the repo).
Next step: **push, then re-measure the published `par` cells** — they all predate the fix and are
under-reported (§3). Then P1: the streaming/permutation kernels that emit scalar loops
(`shape-ladder-v2.md` §finding).
Resume command/check: `docs/next-session.md`, then `git log --oneline -3` and
`cargo test --workspace --release`.

## 1. The defect, in one paragraph

`() -> time -> t0;` is read by the **host thread**; the kernel it brackets is run by **worker
threads**. S29 fenced the read against everything written *before* it, and nothing against what was
written *after*: the moment the generation tasks completed, the DAG unlocked the kernel and the pool
started it while the host was still walking toward its clock line. `t1 - t0` then measured an
interval the work had already left. `MAPAL_PAR=1` never reproduced it — no workers, nothing to run
ahead — so every single-threaded number in the repo was always sound; the threaded ones were
flattered, and only ever downward, which is why S33 moved `par` cells onto medians.

## 2. What was built

**Realization A, in `mapal-ir`.** `CategoryIr::path_plan` (`crates/mapal-ir/src/algo.rs`):

- `is_clock` lifts `TimeMs` out of `is_host` and `is_scalar`, so the read is no longer host-only and
  is never absorbed into a neighbouring scalar component.
- It becomes its own single-morphism `Seq` task with `pinned = true` — the host spine still executes
  it, at its own topo position, via the existing `mapal_par_run_pinned` path.
- Edges **both ways**, keyed on `Morphism.loc.start` (source order, for the reason S29 recorded: the
  dataflow graph orders pure work against a clock read not at all). `task_max_loc < start` ⇒ a `dep`
  of the read (S29's fence, restated as dependencies). `task_min_loc > start` ⇒ a dependent **of**
  the read — the half that was missing.
- A `TimeMs` that became a task drops its `Checkpoint`; the pinned sequence already emits the
  `mapal_par_check` at that topo position, and a second wait list would state one ordering twice
  (§3 of the framework). A read left on the spine — inside an effectful loop region, where
  `host_loop_member` claims it — keeps its checkpoint, which is the only fence it has.

**Nothing else changed.** `crates/mapal-rt/` has a zero-line diff. The emitter needed no new
concept: pinned tasks, `mapal_par_pin`, `mapal_par_dep` and `mapal_par_run_pinned` were all built
for trap-capable calls, and `walk_filtered`'s pinned injection already emits the
wait → check → run_pinned trio at the right point. The emitted host spine for fir now reads:

```llvm
call void @mapal_par_dep(ptr %h, i32 0, i32 9)   ; the kernel waits for the clock task
call void @mapal_par_launch(ptr %h, ptr %frame)
call void @mapal_par_wait(ptr %h, ptr @pin0_entries, i32 5)
call void @mapal_par_check(ptr %h, i64 0)
call void @mapal_par_run_pinned(ptr %h, i32 0)   ; t0 — and only now is task 9 dispatchable
```

Cycle safety is structural rather than checked: the two edge sets are disjoint (`max < start` vs
`min > start`), and a read is in neither of its own.

## 3. Measurements

Baseline **re-measured on today's HEAD**, not quoted from S33 (which recorded 3–4/100):

| fir 65 536, `MAPAL_PAR=14`, 100 runs | < 0.01 ms | min | median | min/median |
| --- | ---: | ---: | ---: | ---: |
| before | **6/100** | 0.000125 | 0.0666 | 0.22 |
| after | **0/100** | 0.0423 | 0.0745 | 0.57 |

| Check | Result | What it proved |
| --- | --- | --- |
| `for i in $(seq 1 100); do MAPAL_PAR=14 …/fir65536 …` | 6/100 → 0/100 under 0.01 ms | the race is gone (acceptance 1) |
| fir stdout, both binaries | `2169` / `1405` byte-identical | no value change (acceptance 3) |
| `cargo test --workspace --release --no-fail-fast` | **972 passed, 0 failed** | S35 was 971; +1 is the new after-edge test |
| `git diff --stat crates/mapal-rt/` | empty | acceptance 4 by construction |
| `watermark_wait_can_finish_before_task_completion`, `wait_helps_while_the_background_worker_is_busy` | pass **unmodified** | the guard rail the first attempt tripped is intact |
| total wall time, n=25, `MAPAL_PAR=14` | median 2.73 → 2.62 ms, min 2.44 → 2.47 | the fix costs nothing (acceptance 5) |
| `RUNS=9 bash benches/shapes/shapes_ab.sh` | "baselines byte-equal; Mapal FMA rel-error ≤ 1e-4 — OK" | fir and conv2d verified against C++/Rust/NumPy |
| fir 1 048 576, 60 runs | min 0.327 / median 0.391, 0 sub-0.01 | the residual spread tracks kernel size, not the clock |

**The interval went UP, and that is the finding.** Total wall time is unchanged while the self-timed
kernel rose ~12% (0.0666 → 0.0745 median). The extra time is work that used to start before `t0` and
was therefore excluded from the bracket. The fix does not slow the program; it stops the measurement
from omitting work it was supposed to contain. **Published `par` numbers from S28–S35 are
under-reported by roughly this much, not over-reported.**

## 4. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Realization **A** — the node lives in `path_plan` | **kept** (Sapir's call) | Ordering is a graph fact, so ADR-0032 puts it in a mapal-ir query. B (a synthetic empty task in the emitter) is a smaller diff but leaves `PathPlan` no longer describing the real DAG |
| The runtime dispatch ceiling | **still rejected** | plan-s33b §4, unchanged: it does not fix the race, and closing the window breaks the launch contract. At launch the runtime does not know where the first checkpoint is — only the emitter does |
| Keep the `TimeMs` checkpoint alongside the task | **discarded** | The pinned sequence already emits `mapal_par_check` at that topo position; a second wait list is the same ordering stated twice. Promoting its trap-watermark entries to completion would have been catastrophic — the kernel has trap sites, so `t0` would have waited for the work it was opening |
| Measure the baseline before fixing | **kept** | The acceptance criterion is a delta against a number, and the number had moved (6/100, not S33's 3–4/100) |
| Acceptance 2, "min and median within noise" | **amended by measurement** | Unachievable and not about this race — see §5 |
| Re-confirm S32's scheduling verdict this session | **deferred** | It needs the pre-S32 A/B leg rebuilt; that is its own campaign, now unblocked |

## 5. Acceptance 2, corrected

The plan asked for "min and median within noise of each other on every par cell". After the fix,
fir 65 536 sits at min/median **0.57** and fir 1 048 576 at **0.84** — the gap shrinks as the kernel
grows, which is the signature of pool wake-up jitter, not a race. The distribution is unimodal and
continuous from the low tail up; a race leaves a cluster three orders of magnitude below the body,
which is exactly what 6/100 looked like (0.000125 against a 0.067 median).

So the honest rule, written into the plan's §7: **min is trustworthy again where the kernel is large
enough to dominate pool wake-up; the tiny cells stay on medians for a reason that has nothing to do
with the clock.** Reverting the statistic to "min everywhere" would have been declaring the
criterion met by ignoring what it measured.

## 6. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `896fb3c` | committed, clean, **not pushed** — the active `gh` account cannot write to `LessComplexity/mapal` | `git status --short` | `gh auth switch --user LessComplexity && git push` |
| gate | full suite | **972 passed, 0 failed** | `cargo test --workspace --release --no-fail-fast` | — |
| CI | run `30213380577` on `35fb681` | still `in_progress` at session open (cuda job green, macos + ubuntu running); the three runs before it show `cancelled`, each killed by the next push | `gh run view 30213380577 --json status,conclusion` | — |
| gh auth | `sapiritur` active; repo is LessComplexity's | pushes need `gh auth switch --user LessComplexity` | `gh auth status` | — |
| artifacts | `target/tmp/{fir65536,fir65536_fix,fir1m,conv512_fix}` + `{baseline,fixed}_race.txt` | the before/after binaries and their 100-run samples | `awk '$1<0.01' target/tmp/baseline_race.txt` | disposable |
| worktree | `…/scratchpad/pre` @ `1daddaa` | stale, still registered (S33) | `git worktree list` | `git worktree prune` |
| local dir | `/Volumes/LessComplex/Personal/Flow` | still the old name | — | — |

## 7. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | Push `896fb3c` | §6 | `gh auth switch --user LessComplexity`, then `git push`; watch CI with `gh run view --json conclusion` (not `gh run watch \| tail`) | CI green on `896fb3c` |
| P1 | Streaming/permutation kernels emit scalar loops | `shape-ladder-v2.md` §finding | Decide whether a non-tile `map` gets a vectorization rung; plan first | saxpy 1t within ~1.2× of naive C++ |
| P1 | Re-confirm S32's scheduling verdict (1.41–1.43×) | plan-s33b §7 check 6; S32 log | Rebuild the pre-S32 A/B leg and re-run under min AND median, now that both are trustworthy | verdict restated on a sound statistic |
| P1 | Re-measure the published `par` cells | §3 | Every `par` number in S28–S35 predates the fix and is under-reported; re-run through `shapes_ab.sh` before republishing | the ladder tables carry post-S36 numbers |
| P1 | Ladder rows 5–9 | `shape-ladder-v2.md` | scan, histogram, mandelbrot (verify loop-in-map first), binary search, bitonic sort | measured and published, losses included |
| P1 | Ladder shapes are cache-resident | `shape-ladder-v2.md` caveats | 64 MB variants before any claim about irregular access at scale | DRAM-sized cells published |
| P2 | Empty-param calls should not need `()` | S35 log §8 | Becomes **ADR-0038** | ADR written, decided |
| P2 | Halve the per-push differential cross product | S34 log §3 | Sapir's call; changes a published coverage claim | Ubuntu under ~13 min |
| P2 | Admit `Widen`/`Iota`/`Fill` to `is_pure` | rewrite STATUS | own change, own pins | `map(id)` forwards them, traps preserved |
| P2 | `MapalIcons.ttf` internal family name | ADR-0037 | regenerate via `build_mapal_icons.py` | font reports Mapal |
| P3 | Local directory still named `Flow`; stale worktree; user-side nvim/font/VS Code names | S34 log §7 | — | — |

## 8. Architecture / model changes

One morphism became total. plan-s33b §2 recorded the model as: `before : now → Task*` exists,
`after? : now → Task*` is **MISSING**, and §4.5 Law 1 in its ordering form is what that violates —
a transformation observing state a transmission has already overwritten. Making the read a `Trn`
placed in the task DAG rather than on the host spine makes `after` total, and it is not a new
concept: it is `dependents(now)`, ordinary edges enforced by the machinery that already orders
everything else. The §3 consolidation move — a clock read is not a new kind of thing needing its own
fencing rules, so it is placed the same way every other `Trn` is placed.

Known divergence: none introduced. The trap-watermark waits that a `TimeMs` checkpoint used to carry
are gone at reads that became tasks; traps are still delivered at that topo position by the pinned
sequence's `mapal_par_check`, and the ordering that matters — a trap surfacing before any observable
output that follows it — is enforced by the next `Print` checkpoint, which is unchanged.

## 9. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/components/ir/DESIGN.md` | §13 `path_plan`: the fence restated as task `deps`, the after-edge added, the host-cone rule narrowed to the cone (the read itself is a pinned task) |
| `docs/components/ir/IMPLEMENTATION.md` | `path_plan` row + the `time` scheduling-rules row: rule 4b, the new internals (`is_clock`, `clock_tasks`, `task_min_loc`), the new test |
| `docs/components/ir/STATUS.md` | S36 lead; the `path_plan` bullet's clock rules amended |
| `docs/components/backend-llvm/DESIGN.md` | `TimeMs` row: under the parallel flavor the call lands in the pinned task's body, the spine gets the trio |
| `docs/components/backend-llvm/IMPLEMENTATION.md` | `time` emission row + `time` pins row (the golden's new assertions) |
| `docs/components/backend-llvm/STATUS.md` | S36 lead; the `time` bullet's ordering-fix list |
| `docs/components/backend-llvm/plans/plan-s33b-clock-read-barrier.md` | Status OPEN → **SHIPPED**; new §7 with where it landed, the results table, the acceptance verdicts, and acceptance 2 amended |
| `docs/STATUS.md` | S36 lead; `ir` row (185 ✅) and `backend-llvm` row |
| `docs/next-session.md` | S37 handoff |

## 10. Method notes earned

1. **Re-measure the baseline before fixing something measured three sessions ago.** S33 recorded
   3–4/100; today's HEAD was 6/100. The acceptance criterion is a delta, so the reference has to be
   current.
2. **A fix that raises the number can still be the fix.** The self-timed interval went UP 12% while
   total wall time did not move — the old number was excluding work, and the direction of the change
   is the evidence for it, not against.
3. **Check whether the machinery already exists before designing a mechanism.** The whole emitter
   half of this was zero lines: pinned tasks, `mapal_par_dep` and `run_pinned` were built for
   trap-capable calls in S24.
4. **A test that pins a mechanism will fail when the mechanism moves — that is the test doing its
   job.** Three tests broke; each was restated against the new location and one was added for the
   new edge, rather than relaxed.
5. **Do not promote a watermark wait to a completion wait without asking what it waits for.** The
   tempting simplification (reuse the checkpoint's wait list as the clock task's deps) would have
   made `t0` wait for the kernel it was opening.

## 11. Files changed

Code: `crates/mapal-ir/src/algo.rs` (`path_plan` — `is_clock`, the `TimeMs` task arm, `task_min_loc`,
`clock_tasks`, the both-way edge loop, the checkpoint predicate).
Tests: `crates/mapal-ir/tests/algos.rs` (two clock tests restated, `path_time_ms_holds_back_the_work_written_after_it` added),
`crates/backends/llvm/tests/golden_ll.rs` (`time_bracket_fences_the_tasks_it_brackets` re-pinned to
the pinned-task emission).
Docs: the nine files in §9.

Gate at close: **972 passed, 0 failed.**
