# S36 raw benchmark logs — the clock-read barrier, validated A/B on two machines

Every log here is a **paired A/B**: the same seven shapes, emitted twice — once by the
compiler at `35fb681` (**pre**, the clock read on the host spine) and once at `896fb3c`
(**post**, the clock read as a pinned DAG node) — then run under an identical protocol.
Nothing else differs: same runtime, same clang/gcc flags, same machine, same session.

Machine tags (S26 standing rule):

- `mac_pre.log` / `mac_post.log` — Apple M4 Pro, 10 P + 4 E, NEON, Homebrew clang 22.1.8,
  **unpinned**, `MAPAL_PAR` ∈ {1, par}, n = 100 per cell.
- `box_pre.log` / `box_post.log` — Intel i9-14900F, 8 P + 16 E (32 threads), AVX2, Arch
  Linux 7.1.3, gcc 16.1.1, numpy 2.3.5 — **unpinned**, n = 100 per cell. Mapal legs
  cross-compiled on the Mac (`clang -target x86_64-unknown-linux-gnu -march=raptorlake`)
  and linked on the box with gcc; the C++ and NumPy legs are native.
- `box_pinned.log` — the same box with `taskset`: 1t on **one P-core thread**, par on the
  **8 P-cores** (CPUs 0–15; 16–31 are E-cores at 4.3 GHz). Contains both the post and pre
  sections, in that order. This is the cleanest data in the set — see the caveat below.
- `mac_shapes_ab.log` / `mac_ladder2_ab.log` — the published harnesses re-run post-fix
  (`RUNS=15`), including the C++/Rust/NumPy legs and the value verification.

`run_box.sh`, `run_mac.sh`, `run_pinned.sh` are the drivers, kept so the protocol is
reproducible rather than described. Each keeps **every sample** and reports min, median,
max, n and the sub-0.01 ms counter per cell; `run_pinned.sh` also reports min/median.

## What the logs show

| Configuration | runs | impossible cells (pre) | impossible cells (post) | sub-0.01 ms (pre) | sub-0.01 ms (post) |
| --- | ---: | ---: | ---: | ---: | ---: |
| M4 Pro, unpinned | 1400 + 1400 | **3 / 7** | **0 / 7** | 13 | **0** |
| i9, unpinned (32T) | 1400 + 1400 | **5 / 7** | **0 / 7** | 12 | **0** |
| i9, pinned (8P/16T) | 1400 + 1400 | **3 / 7** | **0 / 7** | 9 | **0** |

**"Impossible" is the sharp test, and it is not the sub-0.01 ms counter.** A cell is
impossible when its reported parallel minimum implies a speedup larger than the machine
has threads to deliver: `1t_median / par_min > threads`. Pre-fix, `matmul1024` on the Mac
reported a par minimum of 0.0209 ms against a 31.22 ms single-threaded median — an
apparent **1494×** on 14 cores. It never tripped the 0.01 ms counter, because that
threshold was calibrated on fir, whose kernel is three orders of magnitude smaller. Any
fixed millisecond threshold is a proxy; the thread ceiling is a bound.

Post-fix every cell is inside the bound on both machines, in all three configurations:
the largest apparent speedup anywhere is gather at 11.6× on the box's 16 hardware threads.

## Two caveats, recorded rather than corrected

**The box governor is `powersave` (intel_pstate) and there is no passwordless sudo**, so
the frequency ramp is in the unpinned numbers: `conv2d_512` reads 1t min 0.055 / median
0.283 there, a 5× spread that has nothing to do with any race — `MAPAL_PAR=1` has no
workers to race with. Pinning to a P-core collapses it to min 0.0492 / median 0.0503
(min/median 0.98), which is why `box_pinned.log` is the log to quote.

**Small par cells stay wide after the fix, and that is scheduling, not the clock.** Pinned
`conv2d_512` par spans 0.017–0.181 ms around a 0.105 median while its 1t median is 0.050:
the fast runs are a real 2.9× on 8 P-cores and the slow ones are pool dispatch dominating
a 50 µs kernel. Both ends are physically reachable, which is exactly what the pre-fix
0.0005 ms readings were not.

## The 1t cells are the control

`MAPAL_PAR=1` has no workers, so it cannot race, and it should be untouched by this
change. In the two configurations quiet enough to see that: **the pinned box** reads
`conv2d_512` 0.0496 pre / 0.0503 post, `matmul1024` 17.3485 / 17.3874, `fir_65536`
0.1080 / 0.1076, `reduce` 0.3832 / 0.3839 — six of seven within 1.5%, `saxpy` the
outlier at −5.0% (post faster) — and **the Mac** likewise.

**The unpinned box is not within noise, and that is the point.** Four of its seven 1t
medians move well past 5% between the two runs: `transpose_1024` +34% (7.5462 → 10.1216),
`conv2d_512` +24% (0.2825 → 0.3499), `gather` −21% (9.3537 → 7.3852), `saxpy` −7%. A leg
with no worker threads cannot race, so a third of a run's spread on that machine is the
`powersave` ramp — which is exactly why the pinned log is the one to quote, and why the
unpinned box numbers are published as context rather than as a measurement.

Value identity is a separate check with its own artifact: `value_identity.log` runs each
pre and post binary, strips the timing line, and compares the rest verbatim. All seven
shapes are identical on the par leg and on the 1t leg.
