# S46 — the i9 shape ladder with the compiler DETECTING the machine by default

Worktree `agent-a98f8d6d074264322` off `233d7eb` ("the default target is `native`, not
`generic`"). Every number lands here as it is taken, before it is interpreted.

## 0. The detection gate — run before anything was timed

`detect()` reads the box's `/sys`, so **emission had to happen ON the box**. The i9 has
`cargo`/`rustc` 1.90 (edition 2024 builds fine), so `mapal-backend-llvm`'s `emit` example was
built there and run there with no `--target=` flag at all. Only the `.ll -> .o` step ran on the
Mac, because the box still has no clang.

```
$ ssh i9: ~/mapal-s46/src/target/release/examples/emit benches/shapes/transpose_1024.mapal - --rewrite [FLAG]

<default>              perm-ops(udiv+urem)=6   bytes=16361
--target=generic       perm-ops(udiv+urem)=0   bytes=16039
--target=raptorlake    perm-ops(udiv+urem)=6   bytes=16361
--target=native        perm-ops(udiv+urem)=6   bytes=16361
```

**DETECTION WORKS ON THIS BOX.** A plain build emits the permutation; the old default does not.
`<default>`, `--target=native` and `--target=raptorlake` are byte-identical for transpose.

The sysfs facts it actually read, dumped from the box:

```
cache/index0/size                 48K       cache/index2/size              2048K
cache/index0/coherency_line_size  64        cache/index2/shared_cpu_list   0-1
cache/index0/number_of_sets       64        topology/thread_siblings_list  0-1
cache/index0/ways_of_associativity 12   ->  l2_cores = 2 / 2 = 1
```

Every one matches the hand-written `RAPTORLAKE` profile field for field.

**Deduced block for transpose 1024²: B = 8** (`urem 8 / udiv 8 / urem 128` on the loop counter).
S44's sweep measured B=8 at 1.0681 ms 1t and named B=128 the optimum at 0.9286 ms — the
deduction picks a point on the plateau's low end, not the peak. See §4.

### What detection does NOT get, and it matters

`native` inherits `GENERIC`'s `vec_bytes: 16, vec_regs: 32` — **NEON geometry on an AVX2 part**.
The named `raptorlake` profile overrides them to `32 / 16`. Nothing in `detect()` reads the
vector ISA. Emitted consequence, measured by diffing the `.ll` on the box:

| shape | default vs `generic` | default vs `raptorlake` | why |
| --- | --- | --- | --- |
| fir | **SAME** | DIFF | TI×TJ = 4×16 vs 2×32; same 64-elem accumulator, different shape |
| conv2d | **SAME** | DIFF | `<16 x float>` vs `<32 x float>` accumulator |
| saxpy | SAME | SAME | no rung reads the profile |
| reduce | SAME | SAME | " |
| transpose | **DIFF** | **SAME** | the move-panel rung fires; this is the whole change |
| gather | SAME | SAME | no rung reads the profile |

So on this box the new default buys **exactly one shape** (transpose) and costs the AVX2 register
tiling on two (fir, conv2d) that a named profile would have supplied. Both are timed below.

## The machine and the instrument

i9-14900F, 24C/32T, Arch, gcc 16.1.1, no clang, python3 + numpy 2.3.5, governor `performance`.

* **1 thread:** `taskset -c 4` — a 5500 MHz P-core, not cpu0/cpu2 (the 5800 MHz favoured cores
  where interrupts and the boost lottery live). `MAPAL_PAR=1`, `THREADS=1`.
* **threaded:** `taskset -c 0-31` — the whole box, the ship configuration.
  `available_parallelism()` honours affinity so the Mapal pool takes the same 32.
  `MAPAL_PAR=32`, `THREADS=32`.
* **Every cell here is sub-5 ms except `fir cpp-1t` and `reduce cpp-mt`, so cycles are reported
  for all of them.** `perf stat -e task-clock,cycles` wraps every run, outside the self-timed
  kernel region. Raptor Lake exposes `cpu_core` AND `cpu_atom` cycle PMUs and perf multiplexes
  them; only a PMU enabled >= 40% of the run is counted. `Mcyc = median_ms × median_GHz` where
  GHz is the whole-process average, so **Mcyc is a lower bound on kernel cycles** and a
  cross-check on the ms, not a replacement.
* 25 interleaved cycles, every leg in one session. Toolchain identical to S45's harness
  (`clang -O3 -target x86_64-unknown-linux-gnu -march=raptorlake -ffp-contract=fast`,
  `gcc -O3 … libmapal_rt.a`, `g++ -O3 -march=native -ffp-contract=fast`), so the S44 rows are
  comparable.

## Value gate — nothing was timed until every leg agreed

Byte-equal across Mapal conf 1t/par, C++ 1t/mt and NumPy, FMA arms inside 1e-4 relative, and the
three profile-comparison binaries checked against their shape's ref at both thread counts:

```
fir [2169 1888]   conv2d [576 -96]   saxpy [-104.5 25.5]
reduce [-136]     transpose [-37 15] gather [37 -40]        GATE PASSED
```

Identical to the refs S44 recorded. Detection changed emission; it changed no value.

## 1. The ladder — median ms (25 interleaved cycles)

| shape | mapal-conf-1t | mapal-fma-1t | mapal-conf-par | mapal-fma-par | cpp-1t | cpp-mt | numpy-1t |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fir 1M | 1.6492 | 1.3686 | 0.2245 | **0.1870** | 12.4400 | 1.2412 | 5.4411 |
| conv2d 1024² | 0.3000 | 0.1912 | 0.1181 | **0.0844** | 0.2451 | 0.4115 | 2.8494 |
| saxpy 1M | 0.6462 | 0.6403 | 0.1368 | 0.1300 | **0.2295** | 0.4157 | 0.4668 |
| reduce 1M | 0.3937 | 0.3914 | 0.4488 | 0.4708 | 0.3857 | 7.3204 | **0.1433** |
| transpose 1024² | 1.0862 | 1.0768 | 0.2002 | **0.1762** | 2.3114 | 0.4748 | 2.2222 |
| gather 1M | 2.1137 | 1.9844 | 0.2363 | **0.2028** | 0.7975 | 0.4805 | 1.3322 |

## 2. min / median / max, with the clock and the cycle count

| shape | leg | n | min | median | max | GHz | **Mcyc** |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fir | mapal-conf-1t | 25 | 1.6311 | 1.6492 | 1.9239 | 3.46 | 5.713 |
| fir | mapal-fma-1t | 25 | 1.3504 | 1.3686 | 1.5836 | 3.54 | 4.842 |
| fir | mapal-conf-par | 25 | 0.1897 | 0.2245 | 0.3026 | 3.67 | 0.825 |
| fir | mapal-fma-par | 25 | 0.1564 | 0.1870 | 0.3026 | 3.39 | 0.633 |
| fir | cpp-1t | 25 | 12.4063 | 12.4400 | 12.5488 | 5.03 | 62.573 |
| fir | cpp-mt | 25 | 1.2107 | 1.2412 | 1.7562 | 5.00 | 6.205 |
| fir | numpy-1t | 25 | 5.4110 | 5.4411 | 5.9024 | 4.62 | 25.138 |
| conv2d | mapal-conf-1t | 25 | 0.1946 | 0.3000 | 0.5050 | 2.08 | 0.625 |
| conv2d | mapal-fma-1t | 25 | 0.1640 | 0.1912 | 0.2903 | 2.20 | 0.421 |
| conv2d | mapal-conf-par | 25 | 0.0504 | 0.1181 | 0.6348 | 2.60 | 0.307 |
| conv2d | mapal-fma-par | 25 | 0.0586 | 0.0844 | 0.2815 | 2.33 | 0.197 |
| conv2d | cpp-1t | 25 | 0.2176 | 0.2451 | 0.2646 | 2.83 | 0.695 |
| conv2d | cpp-mt | 25 | 0.3856 | 0.4115 | 0.4312 | 1.70 | 0.701 |
| conv2d | numpy-1t | 25 | 2.8330 | 2.8494 | 3.2468 | 4.58 | 13.047 |
| saxpy | mapal-conf-1t | 25 | 0.6407 | 0.6462 | 0.6619 | 2.65 | 1.711 |
| saxpy | mapal-fma-1t | 25 | 0.6362 | 0.6403 | 0.6494 | 2.92 | 1.870 |
| saxpy | mapal-conf-par | 25 | 0.0814 | 0.1368 | 0.4150 | 1.79 | 0.244 |
| saxpy | mapal-fma-par | 25 | 0.0780 | 0.1300 | 0.4981 | 2.70 | 0.350 |
| saxpy | cpp-1t | 25 | 0.2046 | 0.2295 | 0.2701 | 3.04 | 0.697 |
| saxpy | cpp-mt | 25 | 0.3992 | 0.4157 | 0.4396 | 2.21 | 0.919 |
| saxpy | numpy-1t | 25 | 0.4336 | 0.4668 | 0.4774 | 4.52 | 2.112 |
| reduce | mapal-conf-1t | 25 | 0.3860 | 0.3937 | 0.4184 | 2.73 | 1.074 |
| reduce | mapal-fma-1t | 25 | 0.3839 | 0.3914 | 0.4192 | 3.14 | 1.230 |
| reduce | mapal-conf-par | 25 | 0.4104 | 0.4488 | 0.7453 | 1.21 | 0.544 |
| reduce | mapal-fma-par | 25 | 0.3929 | 0.4708 | 0.7949 | 2.20 | 1.035 |
| reduce | cpp-1t | 25 | 0.3845 | 0.3857 | 0.3949 | 3.74 | 1.443 |
| reduce | cpp-mt | 25 | 6.2410 | 7.3204 | 8.5417 | 2.97 | 21.771 |
| reduce | numpy-1t | 25 | 0.1381 | 0.1433 | 0.1937 | 4.54 | 0.650 |
| transpose | mapal-conf-1t | 25 | 1.0500 | **1.0862** | 1.1131 | 3.05 | **3.313** |
| transpose | mapal-fma-1t | 25 | 1.0466 | 1.0768 | 1.1044 | 3.34 | 3.600 |
| transpose | mapal-conf-par | 25 | 0.1360 | 0.2002 | 0.5915 | 3.24 | 0.648 |
| transpose | mapal-fma-par | 25 | 0.1315 | 0.1762 | 0.4712 | 3.19 | 0.561 |
| transpose | cpp-1t | 25 | 2.2089 | 2.3114 | 2.8415 | 4.10 | 9.472 |
| transpose | cpp-mt | 25 | 0.4470 | 0.4748 | 0.5703 | 4.78 | 2.271 |
| transpose | numpy-1t | 25 | 2.1584 | 2.2222 | 2.3414 | 4.59 | 10.196 |
| gather | mapal-conf-1t | 25 | 1.9718 | 2.1137 | 2.2120 | 3.69 | 7.793 |
| gather | mapal-fma-1t | 25 | 1.9710 | 1.9844 | 2.1470 | 4.00 | 7.938 |
| gather | mapal-conf-par | 25 | 0.1700 | 0.2363 | 0.5645 | 3.66 | 0.866 |
| gather | mapal-fma-par | 25 | 0.1575 | 0.2028 | 0.4051 | 3.65 | 0.740 |
| gather | cpp-1t | 25 | 0.7680 | 0.7975 | 0.8641 | 3.67 | 2.930 |
| gather | cpp-mt | 25 | 0.4245 | 0.4805 | 0.5511 | 2.17 | 1.044 |
| gather | numpy-1t | 25 | 1.1994 | 1.3322 | 1.6737 | 4.60 | 6.124 |

The `conv2d … 1t` GHz readings of 2.08–2.20 mean the process is startup-dominated, not
kernel-dominated; those Mcyc are the least trustworthy cells in the table and their *ranking*
should not be read hard. `fir`, `transpose`, `gather` at 3.4–4.0 GHz are kernel-dominated.

## 3. CONTROLS — the run is NOT void

**Within-session drift control (`S_ctl`, saxpy run at a different point in every cycle):**
0.6369 / **0.6489** / 0.6627 at 1t — a 4.0% total spread, and it agrees with the independently
scheduled `saxpy mapal-conf-1t` row (0.6407 / 0.6462 / 0.6619) to **0.4% on the median**. The arm
that must not move, did not move.

**Cross-session control.** saxpy's emitted text is byte-identical to what S44 timed (`def ==
raptorlake == generic` for this shape), so it must reproduce S44 or the box changed:
S44 0.6494 → S46 0.6462, **-0.5%**. It reproduces.

**The whole baseline column is a second cross-session control** — none of it touches the
compiler at all:

| leg | S44 | S46 | Δ |
| --- | ---: | ---: | ---: |
| fir cpp-1t | 12.4368 | 12.4400 | +0.0% |
| fir cpp-mt | 1.2346 | 1.2412 | +0.5% |
| fir numpy | 5.4384 | 5.4411 | +0.1% |
| conv2d cpp-1t | 0.2521 | 0.2451 | -2.8% |
| conv2d numpy | 2.8541 | 2.8494 | -0.2% |
| saxpy cpp-1t | 0.2264 | 0.2295 | +1.4% |
| reduce cpp-1t | 0.3851 | 0.3857 | +0.2% |
| transpose cpp-1t | 2.2872 | 2.3114 | +1.1% |
| transpose numpy | 2.2263 | 2.2222 | -0.2% |
| gather cpp-1t | 0.8099 | 0.7975 | -1.5% |
| gather numpy | 1.2671 | 1.3322 | +5.1% |

Eleven independent legs, all within 5.1%, most within 2%. **The machine did not move between S44
and S46**, so the shape rows below can be differenced directly.

## 4. The profile-comparison arms — what the new default costs and buys

Same source, same flags, different profile, interleaved into the same cycles.

| arm | 1t min | 1t med | 1t max | 1t Mcyc | par min | par med | par max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **fir default (detected)** | 1.6311 | **1.6492** | 1.9239 | 5.713 | 0.1897 | **0.2245** | 0.3026 |
| fir `--target=raptorlake` | 1.6404 | 1.6822 | 1.7359 | 5.780 | 0.2006 | 0.2841 | 0.4320 |
| fir default, fma | 1.3504 | 1.3686 | 1.5836 | 4.842 | 0.1564 | 0.1870 | 0.3026 |
| fir raptorlake, fma | 1.3526 | 1.3770 | 1.5004 | 4.949 | 0.1622 | 0.2003 | 0.3495 |
| **conv2d default (detected)** | 0.1946 | 0.3000 | 0.5050 | 0.625 | 0.0504 | 0.1181 | 0.6348 |
| conv2d `--target=raptorlake` | 0.2160 | **0.2279** | 0.4850 | 0.472 | 0.0569 | 0.0902 | 0.1506 |
| conv2d default, fma | 0.1640 | 0.1912 | 0.2903 | 0.421 | 0.0586 | 0.0844 | 0.2815 |
| conv2d raptorlake, fma | 0.1594 | **0.1641** | 0.2969 | 0.345 | 0.0546 | 0.0877 | 0.1602 |
| **transpose default (detected)** | 1.0500 | **1.0862** | 1.1131 | 3.313 | 0.1360 | 0.2002 | 0.5915 |
| transpose `--target=generic` (OLD default) | 2.2879 | 2.4679 | 2.6016 | 9.427 | 0.1742 | 0.2017 | 0.2897 |
| transpose default, fma | 1.0466 | 1.0768 | 1.1044 | 3.600 | 0.1315 | 0.1762 | 0.4712 |
| transpose generic, fma | 2.2824 | 2.3434 | 2.5538 | 9.530 | 0.1626 | 0.1957 | 0.2551 |

* **fir: no regression.** default 1.6492 vs raptorlake 1.6822 — ranges [1.6311, 1.9239] and
  [1.6404, 1.7359] **OVERLAP**, not separable, and the sign favours the default if anything.
  In cycles, 5.713 vs 5.780. Threaded the default's median is 1.27× better (0.2245 vs 0.2841)
  with fully overlapping ranges. **The NEON-shaped 4×16 window tile is not worse than the
  AVX2-shaped 2×32 one here** — the accumulator is 64 elements either way and LLVM legalizes it.
* **conv2d: a small, real, non-separable cost.** default 0.3000 vs raptorlake 0.2279 at 1t
  (1.32×), 0.1912 vs 0.1641 with fma (1.17×); in cycles 0.625 vs 0.472 and 0.421 vs 0.345, so
  both units agree on the sign. Ranges **OVERLAP** in every pairing, and these are the
  startup-dominated cells, so this is "the medians lean, consistently, by ~1.2–1.3×", not a
  separated result. Cause is named and not mysterious: `<16 x float>` instead of `<32 x float>`
  because detection does not read the vector ISA.
* **transpose: this is what the change is for.** 1t 2.4679 → **1.0862 ms, 2.272×, DISJOINT**
  (default max 1.1131 < old-default min 2.2879). In cycles 9.427 → 3.313, **2.85×** — the cycle
  ratio is larger because the defeated arm idles at a higher clock (3.82 vs 3.05 GHz), the same
  signature S44 recorded at side 2048. **Threaded: 0.2017 → 0.2002, OVERLAP, no separable gain**
  (the min moves 0.1742 → 0.1360, which is suggestive and is not a claim); with fma 0.1957 →
  0.1762, also OVERLAP.
* **The deduction picks B=8; S44's sweep named B=128 the optimum** (0.9286 ms 1t). B=8 measured
  1.0681 there and 1.0862 here — a 1.7% reproduction. So the automatic rung captures 2.27× of the
  2.65× that was on the table, and **leaves ~15% at 1t to a block choice the deduction does not
  make.** That gap is the honest open item, not a defect: no flag was typed.

## 5. Versus S44 — did anything regress?

S44 ran every shape at `--target=raptorlake`. S46 runs the shipped default.

| shape | leg | S44 | S46 | Δ | separable? |
| --- | --- | ---: | ---: | ---: | --- |
| fir | conf-1t | 1.6671 | 1.6492 | **-1.1%** | no |
| fir | fma-1t | 1.3713 | 1.3686 | -0.2% | no |
| fir | conf-par | 0.2819 | 0.2245 | -20.4% | no (overlap) |
| fir | fma-par | 0.1943 | 0.1870 | -3.8% | no |
| conv2d | conf-1t | 0.2945 | 0.3000 | +1.9% | no |
| conv2d | fma-1t | 0.1757 | 0.1912 | **+8.8%** | no (overlap) |
| conv2d | conf-par | 0.1290 | 0.1181 | -8.4% | no |
| conv2d | fma-par | 0.0879 | 0.0844 | -4.0% | no |
| saxpy | conf-1t | 0.6494 | 0.6462 | -0.5% | control |
| saxpy | conf-par | 0.1323 | 0.1368 | +3.4% | control |
| reduce | conf-1t | 0.3897 | 0.3937 | +1.0% | no |
| reduce | conf-par | 0.4493 | 0.4488 | -0.1% | no |
| **transpose** | **conf-1t** | **2.4154** | **1.0862** | **-55.0%** | **DISJOINT** |
| transpose | fma-1t | 2.3838 | 1.0768 | **-54.8%** | DISJOINT |
| transpose | conf-par | 0.2728 | 0.2002 | -26.6% | no (overlap) |
| transpose | fma-par | 0.2041 | 0.1762 | -13.7% | no (overlap) |
| gather | conf-1t | 2.1240 | 2.1137 | -0.5% | no |
| gather | conf-par | 0.2337 | 0.2363 | +1.1% | no |

**Nothing regressed separably. `fir` — the shape flagged to watch — did not regress at any leg**
(-1.1% / -0.2% / -20.4% / -3.8%, every one an improvement or a wash). The only positive Δ above
noise is `conv2d fma-1t` at +8.8%, which the §4 arms attribute to the missing AVX2 vector facts
rather than to any rung newly firing, and which does not separate.

`transpose conf-1t` moving -55% while S44 used `--target=raptorlake` is not a contradiction: at
S44's commit the move rung still required an explicit `--move-panel=W:B` and phase A did not
pass one, so S44's phase A row is the OFF arm. `d52c136` made the rung deduce itself and
`233d7eb` made the profile that feeds it the default. The old-default arm in §4 reproduces S44's
number (2.4679 vs 2.4154, and vs phase B's OFF at 2.4574) and confirms this.

## 6. Per-shape verdict versus C++ and NumPy

Ranges are [min, max] over the 25 cycles; "DISJOINT" means the ranges do not touch.

* **fir 1M — Mapal wins everywhere, by a lot.** 1t 1.6492 vs C++ 12.4400 **DISJOINT 7.54×**, vs
  NumPy 5.4411 **DISJOINT 3.30×**. Threaded 0.2245 vs C++ mt 1.2412 **DISJOINT 5.53×** (6.64×
  with fma). The C++ 12.44 ms is the naive 64-tap scalar loop gcc will not vectorize — a real
  property of the baseline, reproduced from its own source.
* **conv2d 1024² — Mapal wins vs NumPy and vs threaded C++; ties naive C++ at 1t.** 1t conf
  0.3000 vs C++ 0.2451 **OVERLAP**, medians favour C++ 1.22×; with fma 0.1912 **OVERLAP**,
  medians favour Mapal 1.28×. vs NumPy 2.8494 **DISJOINT 9.50×**. Threaded fma 0.0844 vs C++ mt
  0.4115 **DISJOINT 4.88×**.
* **saxpy 1M — Mapal loses at 1 thread, wins threaded.** 1t 0.6462 vs C++ 0.2295 **DISJOINT, C++
  wins 2.82×**; vs NumPy 0.4668 **DISJOINT, NumPy wins 1.38×**. Threaded 0.1368 vs C++ mt 0.4157
  **OVERLAP** (medians favour Mapal 3.04×), vs NumPy 1t **DISJOINT 3.41×**.
* **reduce 1M — NumPy wins; Mapal ties naive C++ and does not scale.** 1t 0.3937 vs C++ 0.3857
  **OVERLAP**, a tie; vs NumPy 0.1433 **DISJOINT, NumPy wins 2.75×**. Threaded 0.4488 is *slower
  than 1t* — the reduction gets nothing from the pool. C++ mt at 7.3204 is 19× slower than its
  own 1t leg because the baseline spawns 32 `std::thread`s per iteration; ignore that column.
* **transpose 1024² — the shape the change was for. Mapal now beats both, single-threaded.**
  1t 1.0862 vs C++ 2.3114 **DISJOINT 2.13×**, vs NumPy 2.2222 **DISJOINT 2.05×**. In S44 this
  cell OVERLAPPED C++ and lost on the median. Threaded 0.2002 vs C++ mt 0.4748 **OVERLAP**
  (Mapal max 0.5915 crosses C++'s min 0.4470), medians favour Mapal 2.37×.
* **gather 1M — Mapal loses at 1 thread, wins threaded.** 1t 2.1137 vs C++ 0.7975 **DISJOINT,
  C++ wins 2.65×**; vs NumPy 1.3322 **DISJOINT, NumPy wins 1.59×**. Threaded fma 0.2028 vs C++ mt
  0.4805 **DISJOINT 2.37×**.

## 7. Reproduction

Emission on the box, `.ll -> .o` on the Mac, link and run on the box:

```
# on the i9
rsync -a --exclude target --exclude .git ./ i9:~/mapal-s46/src/
ssh i9 'cd ~/mapal-s46/src && cargo build --release -p mapal-backend-llvm --example emit'
ssh i9 './target/release/examples/emit benches/shapes/<shape>.mapal - --rewrite [--contract] > x.ll'
# on the Mac (the box has no clang)
clang -O3 -target x86_64-unknown-linux-gnu -march=raptorlake -ffp-contract=fast -c x.ll -o x.o
cargo build -p mapal-rt --release --target x86_64-unknown-linux-gnu
# on the i9
gcc -O3 x.o libmapal_rt.a -lpthread -ldl -lm -o bin/x
```

`benches/shapes/i9_ladder.sh` is the S45 harness this reuses; it cannot be run unmodified for
this question because it emits on the Mac with a hardcoded `--target=raptorlake`, which is
precisely the flag under test. Its remote value gate, `perf` wrapper, pinning and interleaving
were reused verbatim.
