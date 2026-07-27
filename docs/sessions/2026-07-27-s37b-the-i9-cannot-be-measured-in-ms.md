# 2026-07-27 — S37b: the i9 refresh is blocked, and both instruments say so

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Addendum to
`2026-07-27-s37-elem-plan-and-the-dead-array.md` — that log was already written and committed when
this came up, so this is a new log rather than an edit.

Driven by Sapir: *"run the full refresh on the i9"*.

## 0. Continuation brief

Current state: **the shape-table refresh cannot be done on the i9 today.** Not a tooling gap on our
side — the box is clock-limited and the obvious fallback instrument measures the wrong thing. No
numbers from this session are publishable and none were committed.
Next step: **ask Sapir for one of two unblocks** (§4), then redo the refresh.
Resume command/check: `ssh <perf-box> 'cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'`

## 1. What was set up (and works)

Cross-compiled both faces of all six ladder shapes on the Mac
(`clang -O2 -target x86_64-unknown-linux-gnu -march=raptorlake -c`) and linked on the box with gcc
against S36's `libmapal_rt.a` — verified still valid, `crates/mapal-rt` has no commits since
`c5f48c9`. Baselines built on the box from the repo's own sources with the harness's flags
(`g++ -std=c++17 -O3 -march=native -ffp-contract=fast`, `-pthread`).

Faces verified on x86, not assumed: conformance **0** `vfmadd`, FMA face **28** (fir) and **144**
(conv2d). Everything landed in `~/s37bench` and runs.

## 2. Instrument 1 — wall-clock ms — unusable

The box runs `powersave` with no passwordless sudo (recorded since S36, re-confirmed here:
`sudo: a password is required`). Measured clock during the run: **1100 MHz**, against a 5.5 GHz
boost ceiling.

Consequence, on the same binary, same pinning (`taskset -c 0-15`), median of 30:

| pass | fir conformance |
| --- | ---: |
| first | 0.7014 ms |
| later, four independent medians | 1.1226 / 1.1299 / 1.1222 / 1.1235 ms |

**Rock stable within a session, 60% apart between them.** Which is the point: the number is a
property of where the ramp happened to be, not of the code. Any A/B whose two legs are measured
minutes apart is measuring the governor.

## 3. Instrument 2 — process-level `perf stat` — measures the wrong thing

The obvious fallback is frequency-invariant `ref-cycles`, which is what S36c prescribed for this
box's sub-5 ms cells. Run naively over the whole process it is worse than useless:

```
self-timed kernel : 1.1326 ms
whole process     : 39,869,596 ref-cycles
clock             : 1100 MHz
kernel            ≈ 1,246,197 cycles = 3.1% of what perf counted
```

**97% of the counters are generation, startup and page faults.** The shapes self-time their kernel
with the `time` builtin precisely so the generation legs stay outside the measurement; wrapping the
process throws that away.

This is a documented trap and it was walked into anyway. S33, in `docs/STATUS.md`: *"The recorded
'IPC 3.11 vs 1.57' was **process-level**, contaminated by Mapal's generation legs (IPC 0.86–1.04)
whose instruction counts differ 7× from C++'s."* A cycles table was produced and briefly believed
before the 3.1% check killed it — and its numbers disagreed wildly with a single-run probe of the
same binaries (fir conformance 39.3M vs 17.2M), which is what prompted the check.

## 4. What would unblock it

| | what | cost |
| --- | --- | --- |
| **(a)** | Set the governor to `performance` — it is listed in `scaling_available_governors`, and `no_turbo=0` | one `sudo` line, or a passwordless sudoers entry for `cpupower`. **Sapir's password.** Cheapest path, and it makes ms measurement valid |
| **(b)** | Differenced counters around the `time` bracket — read counters at `t0`/`t1` rather than wrapping the process | real runtime work; the durable answer, and what S33 did by hand for the conv2d investigation |

Until one lands, **the i9 produces no publishable ms and no publishable cycles** for these shapes.

## 5. Consequence for the README

The table Sapir asked to refresh is titled **"Other shapes — M4 Pro"**. Even with the i9 working it
would not have replaced that table — it is a different machine, and the README keeps the i9 in its
own equal-hardware section. The M4 Pro refresh still wants **an idle Mac** with the C++/NumPy
baselines re-run in the same pass; this laptop had a load average of 3.65 after a day of gates.

Already corrected in the README this session, without needing new measurement (`3f96c5f`): the
saxpy row (0.179 → 0.093 ms conformance, threaded, median of 30, both faces measured together) and
the paragraph that claimed `%Frame` costs 2.3× — which is false for emitted code.

## 6. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Publish the i9 ms table | **rejected** | 60% between-session drift at a 1.1 GHz governor |
| Publish the i9 process-level cycles table | **rejected** | the timed kernel is 3.1% of the counted process |
| Ask for the governor rather than work around it | **kept** | (b) is the durable fix but (a) is one line and unblocks today |
| Refresh the M4 Pro table from this laptop | **deferred** | load average 3.65; needs an idle machine and baselines in the same pass |

## 7. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `s37-elem-plan` @ 9 commits | clean, not pushed | `git log --oneline -9` |
| perf box | `<perf-box>` i9-14900F | idle; governor `powersave` @ 1100 MHz | `ssh … 'cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'` |
| box dir | `~/s37bench` | **new** — 12 objects + 12 linked Mapal legs (both faces), `ladder2_cpp`, `shapes_cpp`, baselines, `i9run.sh`, `i9cyc.sh` | `ls ~/s37bench` |
| box dirs | `~/s36bench*`, `~/mapalbench` | untouched from S36 | `du -sh ~/s36bench*` |

`~/s37bench` is ready to re-run the moment the governor changes: `RUNS=30 ~/s37bench/i9run.sh`.

## 8. Method notes earned

1. **Check the clock before believing a wall-clock number on a shared box.** One `grep "cpu MHz"`
   would have saved the first measurement pass.
2. **`perf stat` on a self-timing program measures the wrong region.** The whole design of these
   benchmarks is that the kernel brackets itself; wrapping the process discards that. Quantify the
   fraction before trusting a counter — 3.1% here.
3. **Disagreement between two passes is a signal, not noise to average away.** fir conformance read
   39.3M cycles once and 17.2M as a median-of-9; that gap is what exposed the contamination.
4. **`cmd 2>&1 | head` takes head's exit status.** `sudo -n true 2>&1 | head -2 && echo AVAILABLE`
   printed "AVAILABLE" while sudo was refusing. Test the command, not the pipeline.
