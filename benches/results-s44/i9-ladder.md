# S45 — the i9 ladder: does the S44 conflict fix survive non-Apple hardware?

Worktree `agent-a51363756386bb1f1` off `1877b73` (the S44 transpose fix). Every number lands
here the moment it is taken, before it is interpreted.

## The machine, re-derived on the box (not carried)

```
$ lscpu                       Intel(R) Core(TM) i9-14900F, 24 cores / 32 threads, 1 socket
                              L1d 896 KiB (24 inst)  L2 32 MiB (12 inst)  L3 36 MiB (1 inst)
$ lscpu -e=CPU,CORE,MAXMHZ    cpu0-15  -> core0-7   5800/5500 MHz   P-cores (SMT, 2/core)
                              cpu16-31 -> core8-23  4300 MHz        E-cores (no SMT)
$ /sys/.../cpu0/cache/index0  size=48K ways=12 sets=64 line=64     <- P-core L1D
$ /sys/.../cpu16/cache/index0 size=32K ways=8  sets=64 line=64     <- E-core L1D
$ getconf LEVEL1_DCACHE_*     32768 / 8 / 64   (reports the E-core geometry only)
$ nproc 32   governor performance   perf present (paranoid=2, cpu_core+cpu_atom PMUs)
$ gcc 16.1.1, g++ 16.1.1, NO clang.  python3 numpy 2.3.5 (scipy-openblas 0.3.30, Haswell kernel)
$ cat /sys/fs/cgroup/cpu.max  -> absent (no quota; pool width comes from affinity)
```

Cross-check on the aggregate: 8 P x 48 KiB + 16 E x 32 KiB = 384 + 512 = **896 KiB over 24
instances** — matches `lscpu` exactly. The brief's numbers reproduce; nothing to correct.

## PRE-REGISTERED PREDICTION — written before a single timing run

The S44 rule is **pressure = lines_live / (sets x ways)**, predicting sign and ordering only,
saturating by pressure ~2. Transpose 1024 f32: row stride = 1024 x 4 = **4096 B = 64 lines**.
Set index advances by `64 mod sets`.

| machine | line | sets | stride (lines) | sets touched | ways | slots | lines live | **pressure** |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| M4 Pro (S44) | 128 | 128 | 32 | 4 | 8 | 32 | 1024 | **32x** |
| **i9 P-core** | 64 | **64** | **64** | **1** | 12 | **12** | 1024 | **85.3x** |
| **i9 E-core** | 64 | **64** | **64** | **1** | 8 | **8** | 1024 | **128x** |

`64 mod 64 = 0` — **every strided read lands on ONE set on both core types.** 12 usable lines
(P) or 8 (E) against 1024 needed. Far above the M4's 32x, and above its saturation point.

Predicted from the M4's measured saturating curve (0.5 -> 1.00x, 2 -> 2.00x, 8 -> 2.09x,
32 -> 2.71x, 128 -> 3.19x), interpolating at 85x:

* **standalone-probe-equivalent speedup ~3.0x** (M4 got 2.71x at pressure 32).
* **in-pipeline 1 thread: 1.6x-1.9x** (M4 got 1.578x at 32x; scale by 3.0/2.71).
* **in-pipeline threaded: LARGER than 1t** — rule 24, a per-core resource conflict is not an
  Amdahl term, so it grows with cores. M4 went 1.578 -> 1.993x. Predict i9 threaded >= 1t.
* **Optimal B will differ from the M4's 16.** 64-byte lines hold 16 f32, half the M4's 32.
* **Falsifier: if ON does not beat OFF disjointly at 1 thread, the pressure rule is refuted
  on Intel.** That outcome is the most important thing this session could produce.

The carried i9 transpose row of **0.346 ms for 8 MB = 23 GB/s** is far under this box's DDR5,
the same symptom the M4 had. Rule 19 says re-measure it; it is assumed stale until then.

## Pinning (rule: say what you pinned to)

* **1 thread:** `taskset -c 4` — a 5500 MHz P-core, not cpu0/cpu2 (the 5800 MHz favoured
  cores, which is where interrupts and the boost lottery live). `MAPAL_PAR=1`, `THREADS=1`.
* **threaded:** `taskset -c 0-31` — the whole machine, which is the ship configuration.
  `available_parallelism()` honours affinity, so the Mapal pool takes the same 32.
  `MAPAL_PAR=32`, `THREADS=32`, and the P/E clock split (5500 vs 4300) is real shipped
  behaviour, not an artefact.
* Sub-5 ms cells are reported in **cycles** as well as ms — S37b found this box holds a
  constant ~2.1 M cycles while wall time swings 0.38-2.01 ms on the boost clock.

## Instrument: how the sub-5 ms problem was actually handled

There is **no passwordless sudo on this box**, so `/sys/devices/system/cpu/intel_pstate/no_turbo`
cannot be written and the frequency cannot be pinned. The brief's other option was taken:
**the problem was scaled up** (phase C, transpose at side 2048, where the 1-thread kernel clears
5 ms). Alongside that, every single run is wrapped in `perf stat -e task-clock,cycles`, which sits
*outside* the self-timed kernel region and so cannot perturb the ms.

**A trap worth recording.** Raptor Lake exposes TWO cycle PMUs, `cpu_core` and `cpu_atom`, and perf
**multiplexes** them. A task pinned to a P-core still gets a `cpu_atom` row — enabled ~4% of the
time and then scaled up 25x into a five-figure number that is pure fiction. Summing the two PMUs
naively inflates cycles by ~35%. The harness only counts a PMU whose enabled percentage is >= 40.

The derived `Mcyc` column is `median_ms x median_GHz`, where GHz is the **whole-process** average
(`cycles / task-clock`). Generation and process startup run at a lower clock than the kernel, so
**the GHz column is a lower bound on the kernel's clock and Mcyc is a lower bound on kernel
cycles.** It is a cross-check on the ms, not a replacement for it. The shapes where the kernel
dominates the process (transpose, fir, side-2048) have believable 3.5-4.2 GHz readings; the tiny
ones (conv2d at 2.05 GHz) are startup-dominated and their Mcyc should be ignored.

## Phase A — the full shape ladder. 25 interleaved cycles, all legs in one session

1t = `taskset -c 4`, `MAPAL_PAR=1`/`THREADS=1`. par = `taskset -c 0-31` (whole box),
`MAPAL_PAR=32`/`THREADS=32`. NumPy is 1-thread by construction here (`OPENBLAS_NUM_THREADS=1`);
these are element-wise ops that never enter BLAS.

**Values checked identical at every leg before a single timing run**, and they were: fir
`[2169 1888]`, conv2d `[576 -96]`, saxpy `[-104.5 25.5]`, reduce `[-136]`, transpose `[-37 15]`,
gather `[37 -40]` — byte-equal across Mapal conf 1t/par, C++ 1t/mt and NumPy, with the FMA arms
inside 1e-4 relative.

**Median ms:**

| shape | mapal-conf-1t | mapal-fma-1t | mapal-conf-par | mapal-fma-par | cpp-1t | cpp-mt | numpy-1t |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fir 1M | 1.6671 | 1.3713 | 0.2819 | **0.1943** | 12.4368 | 1.2346 | 5.4384 |
| conv2d 1024² | 0.2945 | 0.1757 | 0.1290 | **0.0879** | 0.2521 | 0.4049 | 2.8541 |
| saxpy 1M | 0.6494 | 0.6432 | 0.1323 | **0.1123** | 0.2264 | 0.4206 | 0.4776 |
| reduce 1M | 0.3897 | 0.3883 | 0.4493 | 0.4515 | 0.3851 | 7.2728 | **0.1438** |
| transpose 1024² | 2.4154 | 2.3838 | 0.2728 | 0.2041 | 2.2872 | 0.4714 | 2.2263 |
| gather 1M | 2.1240 | 1.9962 | 0.2337 | **0.2121** | 0.8099 | 0.4802 | 1.2671 |

**min / median / max, with the clock reading:**

| shape | leg | n | min | median | max | GHz | Mcyc |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fir | mapal-conf-1t | 25 | 1.6301 | 1.6671 | 2.0829 | 3.48 | 5.80 |
| fir | mapal-fma-1t | 25 | 1.3487 | 1.3713 | 1.7128 | 3.57 | 4.90 |
| fir | mapal-conf-par | 25 | 0.1855 | 0.2819 | 0.5030 | 3.80 | 1.07 |
| fir | mapal-fma-par | 25 | 0.1591 | 0.1943 | 0.2918 | 3.47 | 0.67 |
| fir | cpp-1t | 25 | 12.4166 | 12.4368 | 12.5601 | 5.03 | 62.59 |
| fir | cpp-mt | 25 | 1.1870 | 1.2346 | 1.5044 | 5.13 | 6.33 |
| fir | numpy-1t | 25 | 5.4086 | 5.4384 | 5.4751 | 4.63 | 25.16 |
| conv2d | mapal-conf-1t | 25 | 0.1941 | 0.2945 | 0.5065 | 2.05 | 0.60 |
| conv2d | mapal-fma-1t | 25 | 0.1644 | 0.1757 | 0.2915 | 2.31 | 0.41 |
| conv2d | mapal-conf-par | 25 | 0.0554 | 0.1290 | 0.6639 | 2.41 | 0.31 |
| conv2d | mapal-fma-par | 25 | 0.0571 | 0.0879 | 0.3416 | 2.37 | 0.21 |
| conv2d | cpp-1t | 25 | 0.2144 | 0.2521 | 0.2620 | 2.85 | 0.72 |
| conv2d | cpp-mt | 25 | 0.3853 | 0.4049 | 0.4421 | 1.70 | 0.69 |
| conv2d | numpy-1t | 25 | 2.7970 | 2.8541 | 2.9080 | 4.58 | 13.06 |
| saxpy | mapal-conf-1t | 25 | 0.6394 | 0.6494 | 0.6623 | 2.67 | 1.74 |
| saxpy | mapal-fma-1t | 25 | 0.6355 | 0.6432 | 0.6657 | 2.95 | 1.90 |
| saxpy | mapal-conf-par | 25 | 0.0742 | 0.1323 | 0.4921 | 2.75 | 0.36 |
| saxpy | mapal-fma-par | 25 | 0.0739 | 0.1123 | 0.2183 | 2.65 | 0.30 |
| saxpy | cpp-1t | 25 | 0.2110 | 0.2264 | 0.2504 | 3.03 | 0.69 |
| saxpy | cpp-mt | 25 | 0.3852 | 0.4206 | 0.4626 | 2.21 | 0.93 |
| saxpy | numpy-1t | 25 | 0.4315 | 0.4776 | 0.5480 | 4.53 | 2.16 |
| reduce | mapal-conf-1t | 25 | 0.3837 | 0.3897 | 0.4173 | 2.75 | 1.07 |
| reduce | mapal-fma-1t | 25 | 0.3838 | 0.3883 | 0.4085 | 3.16 | 1.23 |
| reduce | mapal-conf-par | 25 | 0.4176 | 0.4493 | 0.7415 | 1.12 | 0.50 |
| reduce | mapal-fma-par | 25 | 0.4044 | 0.4515 | 0.7817 | 2.32 | 1.05 |
| reduce | cpp-1t | 25 | 0.3845 | 0.3851 | 0.3983 | 3.71 | 1.43 |
| reduce | cpp-mt | 25 | 6.2881 | 7.2728 | 8.6093 | 2.98 | 21.66 |
| reduce | numpy-1t | 25 | 0.1381 | 0.1438 | 0.1539 | 4.54 | 0.65 |
| transpose | mapal-conf-1t | 25 | 2.3196 | 2.4154 | 2.6367 | 3.86 | 9.32 |
| transpose | mapal-fma-1t | 25 | 2.2049 | 2.3838 | 2.5221 | 4.11 | 9.79 |
| transpose | mapal-conf-par | 25 | 0.1927 | 0.2728 | 0.7056 | 3.69 | 1.01 |
| transpose | mapal-fma-par | 25 | 0.1557 | 0.2041 | 0.3312 | 3.64 | 0.74 |
| transpose | cpp-1t | 25 | 2.1772 | 2.2872 | 2.4225 | 4.09 | 9.35 |
| transpose | cpp-mt | 25 | 0.4510 | 0.4714 | 0.7254 | 4.73 | 2.23 |
| transpose | numpy-1t | 25 | 2.1478 | 2.2263 | 2.3403 | 4.59 | 10.21 |
| gather | mapal-conf-1t | 25 | 1.9806 | 2.1240 | 2.1895 | 3.70 | 7.86 |
| gather | mapal-fma-1t | 25 | 1.9715 | 1.9962 | 2.1915 | 3.98 | 7.94 |
| gather | mapal-conf-par | 25 | 0.1679 | 0.2337 | 0.3866 | 3.63 | 0.85 |
| gather | mapal-fma-par | 25 | 0.1552 | 0.2121 | 0.3565 | 3.62 | 0.77 |
| gather | cpp-1t | 25 | 0.7847 | 0.8099 | 0.9107 | 3.68 | 2.98 |
| gather | cpp-mt | 25 | 0.4367 | 0.4802 | 0.6864 | 2.18 | 1.05 |
| gather | numpy-1t | 25 | 1.1850 | 1.2671 | 1.4915 | 4.60 | 5.83 |

**Overlap statements on the transpose row (shipped, without the rung):**
* 1t, mapal-conf vs cpp-1t: **OVERLAP over [2.3196, 2.4225]** — Mapal does not beat naive C++
  single-threaded at transpose, and the arms are not separable.
* par, mapal-conf vs cpp-mt: **OVERLAP over [0.4510, 0.7056]** — the medians favour Mapal 1.73x
  but the tails touch, so this is not a clean win either.

Two legs worth flagging because they look like instrument faults and are not.
**`fir cpp-1t` at 12.44 ms** is the naive 64-tap scalar loop, which gcc does not vectorise;
Mapal's 1.667 ms is a real 7.5x. **`reduce cpp-mt` at 7.27 ms is 18.9x SLOWER than `cpp-1t`** —
the baseline's `parallel_for` spawns 32 `std::thread`s per iteration for a 1M reduction, and the
spawn cost swamps the work. Both are properties of the baseline, reproduced from its own source.

## Phase B — the move-panel B sweep, transpose 1024². 25 interleaved cycles

`--move-panel=1024:B`, swept, never spot-checked. **B must divide W=1024 and rows=1024**
(`move_panel_index` returns the identity unless `w % b == 0 && rows % b == 0`), so B=12/24/48/96
were tried, **DECLINED the rung, and were dropped** — a declined arm emits the same text as OFF
and is not a treatment. The sweep is therefore the divisors.

Gates before timing: values bit-identical to OFF at all 10 arms at both thread counts (it is a
permutation of the loop counter, so anything but equality is a bug); every arm's emitted `.ll`
differs from OFF's, so the rung fired rather than declining.

| arm | 1t min | 1t med | 1t max | 1t GHz | par min | par med | par max | par GHz |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 2.3161 | 2.4574 | 2.6313 | 3.84 | 0.1739 | 0.2054 | 0.2690 | 3.83 |
| 2 | 1.5551 | 1.5901 | 1.7028 | 3.63 | 0.1471 | 0.1621 | 0.2022 | 4.05 |
| 4 | 1.6218 | 1.6396 | 1.7564 | 3.64 | 0.1349 | 0.1599 | 0.1849 | 3.98 |
| 8 | 1.0657 | 1.0681 | 1.0924 | 3.32 | 0.1163 | **0.1328** | 0.1682 | 3.78 |
| 16 | 0.9835 | 0.9928 | 1.1106 | 3.26 | 0.1204 | 0.1423 | 0.1853 | 3.72 |
| 32 | 1.0965 | 1.1115 | 1.2346 | 3.35 | 0.1202 | 0.1330 | 0.1881 | 3.71 |
| 64 | 0.9475 | 0.9534 | 0.9677 | 3.24 | 0.1196 | 0.1335 | 0.1631 | 3.76 |
| **128** | 0.9215 | **0.9286** | 1.0632 | 3.20 | 0.1181 | **0.1328** | 0.1729 | 3.60 |
| 256 | 0.9399 | 0.9487 | 1.0654 | 3.25 | 0.1206 | 0.1368 | 0.1673 | 3.64 |
| 512 | 1.1139 | 1.1259 | 1.2414 | 3.34 | 0.1230 | 0.1463 | 0.1896 | 3.82 |
| 1024 (identity ctl) | 2.3066 | 2.4312 | 2.5665 | 3.84 | 0.1755 | 0.1951 | 0.2392 | 3.78 |
| saxpy null ctl | 0.6365 | 0.6456 | 0.6537 | 2.68 | 0.0813 | 0.1167 | 0.1473 | 2.94 |

* **best B = 128 at 1 thread: 2.4574 -> 0.9286 ms, 2.646x, DISJOINT** (arm max 1.0632 < OFF min 2.3161).
* **best B = 128 threaded: 0.2054 -> 0.1328 ms, 1.547x, DISJOINT** (arm max 0.1729 < OFF min 0.1739 — disjoint by 1 microsecond, which is a real but *marginal* separation; B=8 ties the median at 0.1328 with max 0.1682, which is cleanly disjoint).
* **The M4's B=16 is NOT this box's optimum.** B=16 gives 0.9928 vs B=128's 0.9286 — 6.9% worse
  at 1t, and 7.2% worse threaded (0.1423 vs 0.1328). The prediction that the optimum would move
  held. The curve is a broad plateau from **64 to 256**, not a point.

### Controls (rule 22) — the run is NOT void

* **Identity arm (B=1024, the same permutation arithmetic evaluated to a no-op):** 1t
  2.4574 -> 2.4312, **-1.1%**; par 0.2054 -> 0.1951, **-5.0%** with fully overlapping ranges
  ([0.1755, 0.2392] vs [0.1739, 0.2690]). The arm that must not move, did not move.
* **saxpy null arm:** 1t 0.6365/0.6456/0.6537, a **2.7% total spread**, and it agrees with
  phase A's independently-measured `saxpy mapal-conf-1t` (0.6394/0.6494/0.6623) to within 0.6%
  on the median. The machine did not move under the sweep.

## Phase C — the scaled sides: the 5 ms gate AND a predictive pressure test

The pressure arithmetic for each side, all with 64-byte lines and 64 sets:

| side | row stride | stride in lines | `stride mod 64` | sets touched | slots (12-way) | lines live | **pressure** |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 512 | 2048 B | 32 | 32 | **2** | 24 | 512 | **21.3** |
| 1024 | 4096 B | 64 | **0** | **1** | 12 | 1024 | **85.3** |
| 2048 | 8192 B | 128 | **0** | **1** | 12 | 2048 | **170.7** |

Values gated first: side 512 ref `[-37 -4]`, side 2048 ref `[-37 -10]`, bit-identical across all
6 B arms at 1t and threaded, and against C++ 1t and NumPy.

**side 512 — pressure 21.3**

| arm | 1t min | 1t med | 1t max | 1t Mcyc | par min | par med | par max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 0.1761 | **0.1975** | 0.2267 | 0.42 | 0.0547 | 0.0743 | 0.3124 |
| 4 | 0.2540 | 0.2625 | 0.2789 | 0.72 | 0.0551 | 0.0738 | 0.2088 |
| 8 | 0.2261 | 0.2370 | 0.2583 | 0.63 | 0.0483 | **0.0626** | 0.1054 |
| 16 | 0.2137 | 0.2191 | 0.2447 | 0.57 | 0.0464 | 0.0643 | 0.1625 |
| 32 | 0.2290 | 0.2349 | 0.2538 | 0.62 | 0.0486 | 0.0829 | 0.1779 |
| 64 | 0.2258 | 0.2325 | 0.2453 | 0.61 | 0.0511 | 0.0706 | 0.0965 |
| 128 | 0.2206 | 0.2262 | 0.2407 | 0.60 | 0.0488 | 0.0670 | 0.1355 |
| cpp-1t | 0.4777 | 0.4807 | 0.5895 | 1.59 | | | |
| cpp-mt | | | | | 0.3679 | 0.3928 | 0.6728 |
| numpy | 0.1246 | 0.1261 | 0.1343 | 0.59 | | | |

**best B at 1t is 16 and it LOSES: 0.901x (a 11% slowdown), OVERLAP over [0.2137, 0.2267].**
Threaded, best B=8 gives 1.186x with a wide overlap [0.0547, 0.1054]. **No win at side 512.**

**side 2048 — pressure 170.7. Every 1-thread OFF cell is above 5 ms, so the S37b objection cannot reach this table.**

| arm | 1t min | 1t med | 1t max | 1t GHz | **1t Mcyc** | par min | par med | par max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 10.8040 | **11.9184** | 13.6267 | 4.21 | **50.12** | 0.8662 | 0.8831 | 1.0680 |
| 4 | 6.2706 | 6.3702 | 6.4768 | 3.82 | 24.33 | 0.5208 | 0.7175 | 0.9793 |
| 8 | 4.5032 | 4.9277 | 7.5350 | 3.62 | 17.81 | 0.4619 | 0.6497 | 0.7642 |
| 16 | 4.1817 | 4.3023 | 4.5893 | 3.53 | 15.17 | 0.4853 | **0.6309** | 0.7412 |
| 32 | 4.4587 | 4.4981 | 5.1723 | 3.55 | 15.98 | 0.4802 | 0.6516 | 0.8373 |
| 64 | 3.9057 | 3.9848 | 4.2588 | 3.47 | 13.81 | 0.4668 | 0.6660 | 0.8867 |
| **128** | 3.8931 | **3.9452** | 4.1000 | 3.46 | **13.67** | 0.5361 | 0.6999 | 0.8780 |
| cpp-1t | 10.0298 | 12.8964 | 17.1874 | 4.64 | 59.83 | | | |
| cpp-mt | | | | | | 0.9649 | 1.0497 | 1.5791 |
| numpy | 10.1501 | 10.5680 | 13.2668 | 4.13 | 43.64 | | | |

* **1 thread: 11.9184 -> 3.9452 ms, 3.021x, DISJOINT** (arm max 4.1000 < OFF min 10.8040).
* **In cycles, which is the S37b-approved unit: 50.12 -> 13.67 Mcyc, 3.67x.** The cycle ratio is
  *larger* than the ms ratio because the OFF arm idles at a higher clock (4.21 vs 3.46 GHz) —
  a memory-stalled core boosts harder. **Both units agree on the sign and the rough size, which
  is what the S37b caveat asked for.**
* **Threaded: 0.8831 -> 0.6309 ms, 1.400x, DISJOINT** (arm max 0.7412 < OFF min 0.8662).

## Phase D — rule 19: the carried README i9 table, re-measured at ITS OWN configuration

Phases A-C pinned threaded to the whole box, which is **not** the README's stated setup ("Median
of 100, pinned to the 8 P-cores"), so they cannot answer the reproduction question. This phase
reproduces the stated setup exactly. "The 8 P-cores" is ambiguous between 16 hardware threads
(cpu0-15) and 8 physical cores (cpu0,2,...,14); both were run rather than guessed at.
**Verdict = does the carried value fall inside this session's measured [min, max] over 100 runs.**

| shape | leg | carried | p16 med | p8 med | p16 min | p16 max | ratio | verdict |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| fir | Mapal off | 0.224 | 0.2627 | 0.2834 | 0.2443 | 0.3533 | 1.17 | **DOES NOT reproduce** |
| fir | Mapal on | 0.192 | 0.2284 | 0.2406 | 0.2124 | 0.4245 | 1.19 | **DOES NOT reproduce** |
| fir | C++ mt | 1.930 | 1.6510 | | 1.6225 | 1.7008 | 0.86 | **DOES NOT reproduce** |
| fir | NumPy | 5.160 | 5.4376 | | 5.4050 | 6.0187 | 1.05 | **DOES NOT reproduce** |
| conv2d | Mapal off | 0.104 | 0.0880 | 0.0719 | 0.0675 | 0.1151 | 0.85 | reproduces |
| conv2d | Mapal on | 0.106 | 0.0868 | 0.0747 | 0.0514 | 0.1181 | 0.82 | reproduces |
| conv2d | C++ mt | 0.280 | 0.2238 | | 0.1938 | 0.2677 | 0.80 | **DOES NOT reproduce** |
| conv2d | NumPy | 2.670 | 2.8563 | | 2.8328 | 3.2881 | 1.07 | **DOES NOT reproduce** |
| saxpy | Mapal off | 0.118 | 0.1109 | 0.1287 | 0.0765 | 0.2043 | 0.94 | reproduces |
| saxpy | Mapal on | 0.122 | 0.1047 | 0.1279 | 0.0769 | 0.1400 | 0.86 | reproduces |
| saxpy | C++ mt | 0.280 | 0.2333 | | 0.1995 | 0.4378 | 0.83 | reproduces |
| saxpy | NumPy | 0.470 | 0.4686 | | 0.4281 | 0.5663 | 1.00 | reproduces |
| reduce | Mapal off | 0.394 | 0.4259 | 0.4287 | 0.3922 | 0.5172 | 1.08 | reproduces |
| reduce | Mapal on | 0.393 | 0.4280 | 0.4307 | 0.3923 | 0.4777 | 1.09 | reproduces |
| reduce | C++ mt | 5.090 | 5.1279 | | 4.0955 | 5.4481 | 1.01 | reproduces |
| reduce | NumPy | 0.120 | 0.1434 | | 0.1382 | 0.1510 | 1.19 | **DOES NOT reproduce** |
| **transpose** | **Mapal off** | **0.346** | **0.3581** | 0.3896 | 0.3055 | 0.5521 | **1.03** | **reproduces** |
| transpose | Mapal on | 0.346 | 0.3495 | 0.3888 | 0.3102 | 0.5261 | 1.01 | reproduces |
| transpose | C++ mt | 0.400 | 0.4767 | | 0.4066 | 0.5374 | 1.19 | **DOES NOT reproduce** |
| transpose | NumPy | 2.330 | 2.2463 | | 2.1330 | 2.3447 | 0.96 | reproduces |
| gather | Mapal off | 0.221 | 0.2389 | 0.3452 | 0.1961 | 0.3544 | 1.08 | reproduces |
| gather | Mapal on | 0.223 | 0.2141 | 0.3496 | 0.1931 | 0.3462 | 0.96 | reproduces |
| gather | C++ mt | 0.290 | 0.3028 | | 0.2703 | 0.3920 | 1.04 | reproduces |
| gather | NumPy | 1.170 | 1.2621 | | 1.1740 | 1.8529 | 1.08 | **DOES NOT reproduce** |

**16 of 24 cells reproduce; 8 do not, and none of the misses exceeds 1.25x.** The pattern in the
misses is not random: **every C++ miss is in the same direction the toolchain would push it**
(gcc 16.1.1 here; fir C++ mt 0.86x and conv2d C++ mt 0.80x are *faster* than carried), and the
NumPy misses (1.05-1.19x) track a different numpy build. The Mapal fir row is 1.17-1.19x *slower*
than carried and is the one genuine unexplained regression in the table.

**The number this session was sent to check — `transpose Mapal off = 0.346 ms` — REPRODUCES**
(0.3581 median, carried value sits inside [0.3055, 0.5521]). The 23 GB/s symptom was real and is
not a stale artefact. The M4's non-reproducing C++ mt row does **not** have an i9 counterpart of
similar size.

### The fix at the README's own pinning

| configuration | OFF min/med/max | ON (B=128) min/med/max | speedup | overlap |
| --- | --- | --- | ---: | --- |
| 8 P-cores, 16 threads | 0.3055 / 0.3581 / 0.5521 | 0.1580 / **0.1935** / 0.3219 | **1.850x** | OVERLAP over [0.3055, 0.3219] |
| 8 P-cores, 8 threads | 0.3382 / 0.3896 / 0.6732 | 0.1516 / **0.1771** / 0.2858 | **2.200x** | **DISJOINT** |
| ON vs C++ mt (0.4767) | | | **2.463x ahead** | **DISJOINT** |

## VERDICT on the pre-registered prediction

| what was predicted | outcome |
| --- | --- |
| Cache geometry: 64-byte lines, 64 sets on **both** core types, stride lands every read on ONE set | **HELD** — read off `/sys/.../cache/index0` on the box, exactly as written |
| Sign: the fix wins at side 1024 on Intel, disjointly, at 1 thread | **HELD** — 2.4574 -> 0.9286 ms, **2.646x, DISJOINT** |
| Magnitude in-pipeline at 1t: 1.6x-1.9x | **UNDER-SHOT** — measured 2.646x. The rule claims sign and ordering only, and it under-called its own win |
| Ordering across pressure | **HELD across three points on one box** — 21.3 -> 0.90x, 85.3 -> 2.646x, 170.7 -> 3.021x, monotone |
| Optimal B differs from the M4's 16 | **HELD** — B=128, a broad 64-256 plateau; B=16 is 6.9% off the optimum |
| Threaded speedup **larger** than 1t (rule 24: a per-core conflict grows with cores) | **REFUTED** — it *shrinks* monotonically with thread count |

### The refutation, and it is the interesting part

Rule 24 says a per-core resource conflict is not an Amdahl term, so removing it should pay MORE as
cores are added. The M4 agreed (1.578x at 1t -> 1.993x threaded). **This box does the opposite,
monotonically:**

| threads | speedup from `--move-panel=1024:128` |
| --- | ---: |
| 1 (`taskset -c 4`) | **2.646x** |
| 8 (8 P-cores, 1 thread each) | 2.200x |
| 16 (8 P-cores, both threads) | 1.850x |
| 32 (whole box) | 1.547x |

The mechanism is visible in the scaling, not inferred from a spec sheet. Comparing 1 thread to the
whole box:

| side | OFF 1t -> par | scaling | ON 1t -> par | scaling |
| ---: | --- | ---: | --- | ---: |
| 1024 | 2.4574 -> 0.2054 | **12.0x** | 0.9286 -> 0.1328 | **7.0x** |
| 2048 | 11.9184 -> 0.8831 | **13.5x** | 3.9452 -> 0.6309 | **6.3x** |

**The OFF arm scales 12-13x across 24 physical cores; the fixed arm scales only 6-7x.** The fixed
arm runs into a shared ceiling that the unfixed arm is too slow per-core ever to reach. So rule 24
is right about the *classification* and wrong about the *conclusion*: a per-core conflict's payoff
grows with cores **only while the per-core resource is still the binding constraint.** Once a
shared wall takes over, fixing the per-core one buys progressively less. Stated as a correction:

> A per-core resource conflict scales with cores until a shared bottleneck binds, then unwinds.
> Predicting "it grows with cores" needs the shared wall's location as a second input.

(Which wall: the fixed arm at 32 threads moves 8 MB in 0.1328 ms = 60 GB/s, against 8.1 GB/s for a
single core at side 2048. This session did **not** measure this box's bandwidth ceiling, so the
identity of the wall — L3 vs DRAM — is an inference. The *existence* of the wall is a measurement.)

### The second refutation: the rule's SIGN fails at low pressure

Side 512 sits at pressure 21.3, between the M4's measured 8 -> 2.09x and 32 -> 2.71x, so the rule
predicts a solid win. **It measured a 0.901x LOSS at 1 thread** (0.1975 -> 0.2191 ms, overlapping)
and every B arm was slower than OFF. The rule's ordering survives; its sign does not, here.

The likely reason is a variable the rule does not carry. **Side 512's whole working set is
512² x 4 x 2 = 2 MB, which is exactly one P-core's private L2** (32 MB over 12 instances = 2 MB per
P-core, 4 MB per 4-core E-cluster). An L1 conflict miss there is served by L2 in ~15 cycles, so
there is almost nothing to win, and the traversal's own cost shows up instead. Sides 1024 (8 MB)
and 2048 (32 MB) both overflow L2 and their misses cost real money. The refinement:

> `pressure = lines_live / (sets x ways)` predicts how *often* the L1 is defeated. It says nothing
> about what a defeat *costs*. It only predicts a win when the working set also overflows the next
> cache level. Side 512 is the counterexample that shows the two are independent.

## Bottom line

**The S44 transpose fix works on non-Apple hardware, and works better there than on the M4** —
2.646x at 1 thread against the M4's 1.578x, disjoint, values bit-identical, controls flat, and
corroborated in cycles (50.12 -> 13.67 Mcyc, 3.67x) at a scaled side where every cell clears 5 ms
and the boost clock cannot explain it. The block size does **not** transfer: this box wants
**B=128**, the M4 wanted 16. The threaded direction predicted by rule 24 is refuted.

## Independent replication, via a second harness

`benches/shapes/i9_ladder.sh` was written afterwards to make this run reproducible (none of the
existing `benches/shapes/*.sh` can run here — they all shell out to a local `clang`, and
`movepanel_ab.sh` hardcodes `-march=armv8-a+sme2`). Running it end-to-end at `CYCLES=2` is a
**separate emission, separate cross-compile, separate link and separate timing session** from the
run of record above. It reproduces it, including the awkward part:

| measurement | run of record (25 cycles) | replication (2 cycles) |
| --- | ---: | ---: |
| side 1024, 1t, best B | **2.646x** (B=128) | **2.547x** (B=128) |
| side 1024, par, best B | 1.547x (B=128) | 1.589x (B=8) |
| side 2048, 1t, best B | **3.021x** (B=128) | **3.295x** (B=128) |
| side 2048, par, best B | 1.400x (B=16) | 1.479x (B=4) |
| **side 512, 1t, best B** | **0.901x — a LOSS** | **0.907x — a LOSS** |
| side 512, par, best B | 1.186x | 1.297x |
| fir mapal-conf-1t | 1.6671 ms | 1.6414 ms |
| transpose mapal-conf-1t | 2.4154 ms | 2.4879 ms |
| gather mapal-conf-1t | 2.1240 ms | 2.1558 ms |
| saxpy mapal-conf-1t | 0.6494 ms | 0.6508 ms |

The 1-thread arms — the ones the conclusions rest on — replicate to within 4%, and the side-512
refutation replicates to within 0.7%. The threaded best-B *identity* moves between runs (128 vs 8,
16 vs 4) because the threaded sweep is a flat plateau whose arms overlap heavily; the threaded
*speedup* is stable at 1.4-1.6x. **Do not read a threaded best-B off one run — the plateau is
flat and the winner is noise. The 1-thread optimum (128) is stable.**
