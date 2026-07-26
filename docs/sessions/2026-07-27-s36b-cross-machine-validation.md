# 2026-07-27 — S36b: the barrier validated on every shape, on two machines

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-27-s36-clock-read-barrier.md`, same day, same session block. Repository:
`github.com/LessComplexity/mapal`.

Driven by Sapir: *"CI done can push switch to correct user. Also re run the leg, can use the arch
machine too. To validate performance on ALL tasks."*

## 0. Continuation brief

Current state: **the S36 fix is validated A/B on all seven shapes on both machines — 8,400 timed
runs.** Pre-fix, 11 of 21 shape-machine pairs (7 shapes x 3 configurations) reported speedups the hardware cannot deliver; post-fix,
**zero on either machine, pinned or unpinned**. `17e10b3` is pushed to `main` and CI is green.
Next step: the published `par` tables still carry pre-S36 numbers. `benches/results-s36/` now holds
correct ones for seven shapes on two machines — folding them into `shape-ladder-v2.md` and the
README is the next edit.
Resume command/check: `benches/results-s36/README.md`, then `docs/next-session.md`.

## 1. Pushed, CI green

CI on `35fb681` finished **success** (27m36s) while S36 was being written. `gh auth switch --user
LessComplexity` then `git push`: `35fb681..17e10b3`. The two S36 commits (`896fb3c` the fix,
`17e10b3` the close) are on `main`.

## 2. The campaign

Seven shapes — the four ladder-v2 classes (saxpy/streaming, reduce/reduction, transpose/data
movement, gather/irregular), the two published compute shapes (fir, conv2d) and matmul1024 f32 —
each emitted **twice**: once by the compiler at `35fb681` (clock read on the host spine) and once at
`896fb3c` (clock read as a pinned DAG node), from a `git worktree` at the older commit so both
binaries exist at the same time. Nothing else differs.

Three machine configurations, n = 100 per cell:

- **M4 Pro**, 10 P + 4 E, unpinned, `MAPAL_PAR` ∈ {1, par}.
- **i9-14900F**, 8 P + 16 E = 32 threads, Arch Linux, gcc 16.1.1 — Mapal legs cross-compiled on the
  Mac (`clang -target x86_64-unknown-linux-gnu -march=raptorlake`), linked on the box with gcc.
- **i9 pinned**: 1t on one P-core thread, par on the 8 P-cores (`taskset -c 0-15`).

Raw logs, drivers and the machine tags are committed in `benches/results-s36/`.

## 3. The test that matters is not the 0.01 ms threshold

S33 and S36 both counted "readings under 0.01 ms" as the race signature. That threshold was
calibrated on fir, whose kernel is ~0.1 ms. It is a proxy, and this campaign found where it fails:

> **matmul1024 on the Mac, pre-fix: par min 0.0209 ms against a 31.22 ms single-threaded median.**
> An apparent **1494× on 14 cores** — and `0.0209 > 0.01`, so the counter recorded zero.

The bound that does not need calibrating is the machine itself: a parallel run cannot beat the
single-threaded time by more than the thread count. Define a cell **impossible** when
`1t_median / par_min > threads`. On that test:

| Configuration | impossible cells, PRE | impossible cells, POST | sub-0.01 ms, PRE | sub-0.01 ms, POST |
| --- | ---: | ---: | ---: | ---: |
| M4 Pro, unpinned (14) | **3 / 7** | **0 / 7** | 13 | **0** |
| i9, unpinned (32T) | **5 / 7** | **0 / 7** | 12 | **0** |
| i9, pinned (8P/16T) | **3 / 7** | **0 / 7** | 9 | **0** |

Worst pre-fix readings: fir 154× and conv2d 99× on the pinned box, transpose 59×, matmul 1494× on
the Mac — and 24,666× for matmul on the unpinned box. Post-fix maxima: **Mac 8.6×** (matmul),
**pinned box 11.6×** (gather, on 16 hardware threads), **unpinned box 24.4×** (transpose). That last
one is inside the 32-thread bound but above the machine's 24 physical cores, and it is the governor
ramp again: the cell's 1t *median* is 10.1216 against a 1t *min* of 1.5598, and measured against the
min the same ratio is 3.8×. The pinned column is the one to quote.

**Two things this campaign does not establish, stated because they are easy to over-read.** First,
`reduce` and `saxpy` never showed the defect in any pre log (worst pre ratio 5.6×, `sub0.01=0` in
all six of their pre cells) — of the 14 shape × machine pairs, at most 8 ever exhibited it, so for
the rest "post-fix clean" is a control, not a repair. Second, n = 100 bounds the post-fix rate
rather than zeroing it: no hits in 100 runs puts the one-sided 95% bound near 3% per run, well under
the 5–8% pre-fix rates on the affected cells, but not a proof that a rarer variant does not exist.

## 4. The single-threaded control

`MAPAL_PAR=1` has no workers, so it cannot race, and the fix should not touch it. Pinned box,
1t medians:

| shape | pre | post |
| --- | ---: | ---: |
| conv2d_512 | 0.0496 | 0.0503 |
| fir_65536 | 0.1080 | 0.1076 |
| matmul1024_f32 | 17.3485 | 17.3874 |
| reduce_1048576 | 0.3832 | 0.3839 |

Six of the seven pinned 1t cells move less than 1.5%; `saxpy` is the outlier at −5.0%, post
*faster*. The Mac is the same picture.

**The unpinned box is not within noise — and that is evidence, not an embarrassment.** Four of its
seven 1t medians swing past 5% between the two runs (`transpose` +34%, `conv2d` +24%, `gather`
−21%, `saxpy` −7%). A leg with no worker threads cannot race, so that swing is the machine, and it
is the measurement that tells you to discount the unpinned column rather than read a fix into it.
This was caught by an independent verification pass over the write-up, which refuted the first
draft's "every 1t cell on both machines is within noise" — the claim was true of the pinned box and
the Mac, and false of the unpinned box sitting in the same directory.

Value identity is its own artifact (`benches/results-s36/value_identity.log`): each pre and post
binary run, timing line stripped, the rest compared verbatim. All seven shapes identical on the par
leg and on the 1t leg — fir `2169 1405`, matmul `11107 91690`, and so on.

## 5. Two things the campaign found that are not the race

**The box governor is `powersave`** (intel_pstate, no passwordless sudo), so unpinned i9 numbers
carry the frequency ramp: `conv2d_512` reads 1t min 0.055 / median 0.283 there — a 5× spread on a
leg that *cannot* race. Pinning to a P-core collapses it to 0.0492 / 0.0503 (min/median 0.98). The
pinned log is the one to quote, and "pin the CPU" earns its place in the standing rules again.

**Small par cells stay wide after the fix, and it is scheduling.** Pinned conv2d par spans
0.017–0.181 ms around a 0.105 median while its 1t median is 0.050: the fast runs are a real 2.9× on
8 P-cores, the slow ones are pool dispatch dominating a 50 µs kernel. Both ends are physically
reachable — which the pre-fix 0.0005 ms readings were not. This is the same finding S36 recorded as
"the residual gap tracks kernel size", now confirmed on a second machine with a different core
topology.

## 6. A portability bug, fixed in passing

`benches/shapes/ladder2_baseline.cpp` used `std::string` without including `<string>`. libc++ leaks
it transitively; **gcc 16's libstdc++ does not**, so the C++ baseline would not compile on the
measurement box at all — the baseline leg of a benchmark, unbuildable on the machine the benchmark
exists to run on. One include. `shapes_baseline.cpp` was checked too and needs nothing (it uses
`strcmp`).

## 7. Post-fix numbers, M4 Pro (the published harnesses, re-run)

`RUNS=15`, compute-only, min for 1t / median for par. The fir/conv2d rows come from `shapes_ab.sh`,
whose final line is a real cross-leg check — "baselines byte-equal; Mapal FMA rel-error ≤ 1e-4 —
OK". The four ladder rows come from `ladder2_ab.sh`, which is **timing-only**: it has no correctness
comparison at all (`grep` for `cmp|allclose|assert` in it returns nothing). Their values are
verified by the shapes' own printed outputs and by `value_identity.log`, not by that harness — a
gap worth closing in the harness itself:

| shape | class | mapal-1t | mapal-par | cpp-1t | cpp-mt | numpy |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| saxpy 1M | streaming | 0.541 | 0.183 | 0.293 | 0.155 | 0.175 |
| reduce 1M | reduction | 0.620 | 0.589 | 0.497 | 0.876 | 0.099 |
| transpose 1024² | data movement | 0.960 | **0.255** | 0.776 | 0.249 | 0.762 |
| gather 1M | irregular reads | 0.526 | **0.153** | 0.460 | 0.149 | 1.956 |
| fir 65 536 | compute | — | **0.043** (fma) | 1.130 | 0.188 | 0.365 |
| conv2d 512 | compute | — | **0.042** (fma) | 0.064 | 0.104 | 0.396 |

Read these as *the first post-race ladder numbers*, not as a comparison against S35's table: the
S35 `par` cells were measured with the race live and are biased fast.

## 8. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Validate with a paired A/B rather than "post-fix looks fine" | **kept** | A single-sided measurement cannot show a race is gone; the pre leg is what makes 0/7 mean something |
| Build the pre leg from a `git worktree` at `35fb681` | **kept** | Both binaries exist simultaneously, on the same machine, in the same session — no cross-session drift |
| Keep the 0.01 ms counter | **kept, demoted** | It is a cheap tripwire, but it is calibrated to one shape; the thread-count bound is the real test and is now reported alongside |
| Pin the box to P-cores | **kept** | Unpinned i9 numbers are dominated by a powersave ramp that the 1t control proves is not the race |
| Change the box governor | **not done** | No passwordless sudo. Recorded as a caveat, not silently ignored |
| Fold the new numbers into the published tables this session | **deferred** | It is a separate edit to `shape-ladder-v2.md` and the README, with its own review — the numbers are committed and citable meanwhile |

## 9. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ pushed | clean, pushed through the S36b docs | `git status --short` | — |
| CI | run on `17e10b3` | started at push | `gh run view --json status,conclusion` | — |
| gh auth | **`LessComplexity` is now the active account** | switched this session | `gh auth status` | — |
| perf box | `<perf-box>`, i9-14900F | idle at close (load 0.33), governor `powersave`, no passwordless sudo | `ssh … uptime` | — |
| box dirs | `~/s36bench` (post), `~/s36bench_pre` (pre) | ~22 MB each, binaries + logs left in place | `du -sh ~/s36bench*` | delete when done |
| worktree | `/tmp/s36pre` @ `35fb681` | the pre-fix compiler — **removed at close**; its IR and binaries survive in `target/tmp/i9pre/` | `git worktree list` | done |
| worktree | `…/scratchpad/pre` @ `1daddaa` | stale since S33, still registered; `prune` does NOT clear it (the directory is live under `/private/tmp/claude-501/…`) | `git worktree list` | `git worktree remove --force <path>` |
| artifacts | `target/tmp/i9`, `target/tmp/i9pre` | both binary sets + raw samples | `ls target/tmp/i9` | disposable |

## 10. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | Republish the `par` tables | `benches/results-s36/`, `shape-ladder-v2.md`, README | Fold the post-fix numbers in; every `par` cell published before S36 is biased fast | no pre-S36 `par` number is still quoted |
| P1 | Streaming/permutation kernels emit scalar loops | `shape-ladder-v2.md` §finding | Decide whether a non-tile `map` gets a vectorization rung; plan first | saxpy 1t within ~1.2× of naive C++ |
| P1 | Re-confirm S32's scheduling verdict | plan-s33b §7 check 6 | Rebuild the pre-S32 leg; the statistic is now sound | verdict restated |
| P1 | Ladder rows 5–9, and DRAM-sized variants | `shape-ladder-v2.md` | scan, histogram, mandelbrot, binary search, bitonic sort | measured and published |
| P2 | The box's `powersave` governor | §5 | Needs root on the box; until then pin and say so | pinned protocol is the default in the harnesses |
| P2 | `ladder2_ab.sh` has no correctness check | §7 | It times four shapes and never compares their values; `shapes_ab.sh` does. Add the same cross-leg comparison | the ladder harness fails loudly on a wrong answer |
| P2 | Empty-param calls should not need `()` | S35 log §8 | Becomes ADR-0038 | ADR written |
| P2 | Halve the per-push differential cross product | S34 log §3 | Sapir's call | Ubuntu under ~13 min |
| P3 | One stale worktree left; local dir still named `Flow` | §9 | `git worktree remove --force` the S33 scratchpad entry (`prune` will not clear a live directory) | `git worktree list` shows one entry |

## 11. Method notes earned

1. **A fixed threshold is a proxy; a physical bound is a test.** "Under 0.01 ms" missed a 1494×
   reading because it was calibrated on a shape 300× smaller. "Faster than the thread count allows"
   needs no calibration and caught every case on both machines.
2. **Measure the control you already have.** `MAPAL_PAR=1` cannot race, so any spread it shows is
   the machine. That single comparison separated a 5× i9 spread (powersave ramp) from the defect,
   with no extra runs.
3. **Build the "before" from a worktree, not from memory.** The pre leg compiled at `35fb681` in
   `/tmp/s36pre` made the A/B paired and same-session; quoting S33's recorded 3–4/100 would have
   compared across machines, compilers and months.
4. **A benchmark's baseline must build on the measurement machine.** `ladder2_baseline.cpp` could
   not compile under gcc 16 — discovered only by actually running it there.
5. **Check the emitted artifact, not the intent, when labelling an A/B.** `grep -c
   'call void @mapal_par_run_pinned'` — 0 pre, 2 post — is what proved the two binaries really
   differ; the first attempt grepped a string that matched the *declaration* in both.
6. **Have the write-up refuted before publishing it.** An independent pass over the draft against
   these logs killed a universal claim ("every 1t cell on both machines is within noise") that was
   contradicted by a file in the same directory, and caught a claim with no committed artifact
   behind it (byte-identity, which had been checked in the session and never saved). Both are now
   scoped and evidenced. A number being *right* is not the same as its sentence being right.

## 12. Files changed

`benches/shapes/ladder2_baseline.cpp` (the `<string>` include).
New: `benches/results-s36/` — 7 raw logs, 3 drivers, README with the machine tags and caveats.
Docs: this log, `docs/next-session.md`, `docs/STATUS.md`,
`docs/components/backend-llvm/plans/plan-s33b-clock-read-barrier.md` §7.
