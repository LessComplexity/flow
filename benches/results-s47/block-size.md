# S47 — where the move-panel block `B` actually wants to be

Worktree `agent-a03516dc70d077892` off `b4af6a0`. Question: the derived `B`
under-shoots the measured optimum on the i9 by a lot; find what predicts the
optimum and derive it, or show it is not derivable and say so with the numbers.

**VERDICT UP FRONT: `B` is not derivable from any fact the compiler can read, and
the brief's leading candidate is refuted by direct measurement.** The block rule
is left unchanged. Two things were found that the two-point record could not show:

1. The optimum ordering between the two parts is the **inverse** of every
   capacity and geometry fact in `L1d`/`TargetProfile`. The M4 — bigger L1D,
   bigger line, more sets, bigger L2 share — wants **B = 16**; the i9 wants
   **B = 64–128**. Only `ways` is larger on the i9, by 1.5x, against an 8x
   difference in the answer.
2. **A regression the record did not have: at side 1536 on the M4 the rung fired
   and LOST 1.41–1.75x.** That is larger than every i9 shortfall this session was
   sent to close. **This one IS fixed** (§5): one clause, no machine fact —
   *decline when the width is not a power of two*. The `B` question and the
   fire/decline question are separable, and only the second was answerable.

---

## 0. Instruments, and what was pinned

| leg | harness | machine | pinning |
| --- | --- | --- | --- |
| i9 sweep | `benches/shapes/blocksweep_i9.sh 100.81.226.103` (new) | i9-14900F, Arch, governor `performance` | 1t `taskset -c 4` (a P-core, not the favoured cpu0/cpu2); par `taskset -c 0-15` = the **eight P-cores**, `MAPAL_PAR=16` |
| i9 counters | `perf stat -e cpu_core/…/ -r 3`, `taskset -c 4` | same | as above |
| M4 sweep | `benches/shapes/movepanel_ab.sh` per side x thread count, under `perflock.sh` | M4 Pro (10 P + 4 E) | 1t `MAPAL_PAR=1`; par `MAPAL_PAR=14`, the record's convention |

`benches/shapes/blocksweep_i9.sh` is `i9_ladder.sh`'s sweep half without the
ladder legs (numpy/g++/fir/conv2d — ~80% of its runtime and none of it moves with
`B`), at four sides over every legal divisor. Same split: emit and `.ll -> .o` on
the Mac (the box has gcc and no clang), link and run on the box.

**Why `taskset -c 0-15` and not `0-31`.** cpu16-31 are E-cores with a 32K/8-way
L1D against the P-core's 48K/12-way. Mixing two L1 geometries into one argmin
makes the argmin unattributable, which is the whole question here.

### The non-power-of-two side

`benches/shapes/transpose_1536.mapal` (new). 1536 = 2^9 x 3, so 3, 6, 12, 24,
48, 96, 192, 384 are legal blocks and a power-of-two-shaped optimum is
falsifiable. Both machines fire there: i9 row stride 6144 B = 96 lines,
96 mod 64 = 32, gcd 32 -> 2 sets, 24 slots; M4 48 lines, 48 mod 128 = 48,
gcd 16 -> 8 sets, 64 slots.

### Gates before any number was read

* **Values.** Every arm's stdout minus `iter ms=` byte-equal to the OFF arm's,
  at every side, **at both thread counts**, before timing. Passed on all four
  sides on the box (`-37 -4`, `-37 15`, `-37 13`, `-37 -10`) and on the Mac.
* **Emission.** Every forced arm's `.ll` differs from OFF's — a declined gate
  reported as a treatment would make every "no effect" reading meaningless.
* **Controls.** `B = side` is the identity permutation and must not move; `saxpy`
  is a null arm the flag never touches. The identity control overlapped OFF in
  every cell (e.g. M4 1536 1t: off 1.4018, identity 1.3958). saxpy 1t spread
  2.7% across the whole i9 run. **The threaded saxpy control spread 48% on the
  i9** (0.0918–0.1466) — a 1 M-element saxpy on 16 threads is ~90 us and is
  dominated by pool spin-up, so the threaded i9 columns are read as ordering,
  not as ratios, and nothing under ~10% is read as a result there.

---

## 1. `optimum(machine, side)` — the sweep

Medians over interleaved cycles; the argmin is over `B < side` (B = side is the
identity control). Full per-arm tables in §2.

`derived B` is what shipped before this session; `after §5` is what the width
guard leaves. Only the 1536 rows change.

| machine | side | rung | derived `B` | **after §5** | **1t argmin** | **par argmin** | plateau within ~5% of best |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| M4 Pro | 512 | declines | — | — | — (no arm beats off) | — | decline is correct |
| M4 Pro | 1024 | fires | **16** | 16 | **16** | **16** | 8–32 (1t), 16 (par) |
| M4 Pro | 1536 | fires | **32** | **declines** | **none — every B loses** | **none — every B loses** | — |
| M4 Pro | 2048 | fires | **16** | 16 | **16** | **16** | 16 (8 is +7.8% / +4.1%) |
| i9 | 512 | declines | — | — | — (no arm beats off) | — | decline is correct |
| i9 | 1024 | fires | **8** | 8 | **128** | **128** | 64–256 |
| i9 | 1536 | fires | **12** | **declines** | **16** | **256** | 16–256, powers of two only |
| i9 | 2048 | fires | **8** | 8 | **64** | **32** | 64–128 (1t), 16–32 (par) |

**The shape, in one line each.** The M4's optimum is 16 at every side that wins,
and 16 is *half* a line's worth of elements (`line/w = 32`). The i9's optimum is
a broad flat plateau from `line/w = 16` up to 8x that, argmin wandering inside it
with side and thread count. **The two parts want blocks 8x apart, in the
direction opposite to every readable fact.**

### Derived vs optimum, per cell, before and after the width guard

| cell | shipped `B` | ms | best `B` | ms | gap | **after §5** |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| i9 1024 1t | 8 | 1.0680 | 128 | 0.9291 | 1.15x | unchanged |
| i9 1024 par | 8 | 0.1764 | 128 | 0.1679 | 1.05x | unchanged |
| i9 1536 1t | 12 | 5.2027 | 16 | 4.3194 | 1.20x | **declines** — forgoes a 1.060x win |
| i9 1536 par | 12 | 0.6872 | 256 | 0.5539 | 1.24x | **declines** — forgoes a 1.044x win |
| i9 2048 1t | 8 | 5.0310 | 64 | 3.9661 | 1.27x | unchanged |
| i9 2048 par | 8 | 0.7885 | 32 | 0.6837 | 1.15x | unchanged |
| M4 1024 1t | 16 | 0.5637 | 16 | 0.5637 | **1.00x — exact** | unchanged |
| M4 1024 par | 16 | 0.1600 | 16 | 0.1600 | **1.00x — exact** | unchanged |
| M4 2048 1t | 16 | 2.7948 | 16 | 2.7948 | **1.00x — exact** | unchanged |
| M4 2048 par | 16 | 0.5128 | 16 | 0.5128 | **1.00x — exact** | unchanged |
| M4 1536 1t | 32 | 2.4525 | **off (1.4018)** | | **0.57x — a 1.75x LOSS** | **declines — loss removed** |
| M4 1536 par | 32 | 0.4580 | **off (0.3243)** | | **0.71x — a 1.41x LOSS** | **declines — loss removed** |

Eight of the twelve cells are untouched, and the four that move are the four at
the one non-power-of-two width. The trade in one line: **give up 5.7% and 4.2% on
the i9 to stop losing 43% and 29% on the M4.**

---

## 2. The per-arm tables

### 2.1 i9-14900F, `--target=raptorlake`, 15 interleaved cycles, medians (ms)

| B | 1024 1t | 1024 par | 1536 1t | 1536 par | 2048 1t | 2048 par |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 2.4380 | 0.3608 | 5.6288 | 0.7311 | 12.2653 | 1.4720 |
| deduce | 1.0727 | 0.1830 | 5.3112 | 0.7002 | 4.8327 | 0.8334 |
| 2 | 1.5804 | 0.2608 | 7.2943 | 0.8831 | 9.0414 | 1.0917 |
| 3 | — | — | 7.6914 | 0.8193 | — | — |
| 4 | 1.6339 | 0.2189 | 6.2010 | 0.7029 | 6.3498 | 0.8024 |
| 6 | — | — | 5.6115 | 0.7059 | — | — |
| 8 | 1.0680 | 0.1764 | 4.4923 | 0.5914 | 5.0310 | 0.7885 |
| 12 | — | — | 5.2027 | 0.6872 | — | — |
| 16 | 0.9909 | 0.1835 | **4.3194** | 0.5730 | 4.2649 | 0.6958 |
| 24 | — | — | 5.1806 | 0.6749 | — | — |
| 32 | 1.1146 | 0.1793 | 4.3952 | 0.5797 | 4.4968 | **0.6837** |
| 48 | — | — | 5.2912 | 0.6875 | — | — |
| 64 | 0.9521 | 0.1694 | 4.3441 | 0.5760 | **3.9661** | 0.7362 |
| 96 | — | — | 5.2690 | 0.6942 | — | — |
| 128 | **0.9291** | **0.1679** | 4.3331 | 0.5793 | 3.9699 | 0.8176 |
| 192 | — | — | 5.3779 | 0.6956 | — | — |
| 256 | 0.9465 | 0.1685 | 4.3202 | **0.5539** | 4.8059 | 1.1282 |
| 384 | — | — | 5.8736 | 0.7334 | — | — |
| 512 | 1.1231 | 0.1940 | — | — | 11.5272 | 1.9356 |
| 1024 | 2.4204 (id) | 0.3468 (id) | — | — | 12.0693 | 1.7623 |
| 1536 | — | — | 5.5216 (id) | 0.6998 (id) | — | — |
| 2048 | — | — | — | — | 12.4065 (id) | 1.4780 (id) |

**The 1536 column is a division-cost artifact, not a memory effect, and the
counters say so.** Every `B` with a factor of 3 (3, 6, 12, 24, 48, 96, 192, 384)
sits ~20% above its power-of-two neighbours, and `instructions` moves with it:
180.7 M at B=8, **223.2 M at B=12**, 185.4 M at B=16, **223.2 M at B=24**,
185.4 M at B=32, **232.6 M at B=96**. `move_panel_index` does four `udiv`/`urem`
by `B` and two by `w/B`; a power-of-two divisor lowers to shifts and a
non-power-of-two one to a magic multiply. **The derived B=12 on the i9 at side
1536 is doubly wrong — below the plateau AND on the expensive lowering.**

### 2.2 M4 Pro, `--target=native`, medians (ms). 9-cycle sweep, 21-cycle confirm

Confirm run (21 cycles, narrower arm set) in the right-hand block.

| B | 1024 1t | 1024 par | 2048 1t | 2048 par | 1536 1t | 1536 par |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 0.8349 | 0.2800 | 6.2780 | 1.4942 | **1.4018** | **0.3243** |
| deduce | 0.5799 | 0.1623 | 2.8980 | 0.5461 | 2.4525 | 0.4580 |
| 8 | 0.5654 | 0.1774 | 3.0142 | 0.5339 | 3.5950* | 0.4373* |
| 16 | **0.5637** | **0.1600** | **2.7948** | **0.5128** | 2.4167 | 0.4377 |
| 32 | 0.5884 | 0.1915 | 3.2184 | 0.6128 | 2.4122 | 0.4550 |
| 64 | 0.7311 | 0.2025 | 3.6540 | 0.7153 | 2.3665 | 0.4145 |
| 128 | 0.8923 | 0.2620 | 3.8860 | 0.7960 | 2.4183 | 0.4329 |
| 256 | 1.0004* | 0.2511* | 4.0025* | 0.8636* | 2.3870 | 0.4254 |
| identity | 0.8359 | 0.2773 | 6.2770 | 1.4730 | 1.3958 | 0.3255 |

`*` from the 9-cycle sweep; the rest from the 21-cycle confirm.

**Read the 1536 column against `off`.** The identity control lands on `off`
(1.3958 vs 1.4018, 0.3255 vs 0.3243), so the permutation's own arithmetic is
free — but **every real block is 1.7x slower than not blocking at all**, and the
shipped `deduce` arm is one of them.

Per element, 1 thread, so the sides are comparable (ns/element, medians):

| | M4 off | M4 best blocked | i9 off | i9 best blocked |
| --- | ---: | ---: | ---: | ---: |
| side 1024 | 0.796 | **0.538** (1.48x) | 2.325 | **0.886** (2.62x) |
| side 1536 | **0.594** | 1.003 (**0.59x**) | 2.386 | **1.831** (1.30x) |
| side 2048 | 1.497 | **0.666** (2.25x) | 2.924 | **0.946** (3.09x) |

The M4's **unblocked** walk at side 1536 costs 0.594 ns/element — better than at
side 1024 (0.796) and within 10% of the best blocked traversal anywhere on that
machine (0.538). There is simply no miss cost left to recover, so the
permutation's own arithmetic is the whole delta. The i9 at the identical side
still wins 1.30x, because its unblocked walk costs **4.0x** as much per element.

**Full 1536 M4 sweep, 9 cycles, 1t, medians (ms):** off 1.9254, identity 1.8948,
and then 2: 3.168, 3: 4.538, 4: 3.382, 6: 4.425, 8: 3.595, 12: 4.243, 16: 3.218,
24: 4.254, 32: 3.006, 48: 4.199, 64: 2.966, 96: 4.047, 128: 2.966, 192: 3.974,
256: **2.954**, 384: 4.103. The same divisor-cost alternation as the i9, and not
one arm reaches `off`.

### 2.3 Side 512 — both machines decline, and the decline is right

| machine | off | best forced arm | verdict |
| --- | ---: | ---: | --- |
| M4 1t | 0.1485 | 0.1442 (B=2) | inside noise; **decline correct** |
| M4 par | 0.0878 | 0.0751 (B=4) | inside noise; decline correct |
| i9 1t | 0.2093 | 0.2230 (B=16) | **every forced arm LOSES** (0.94x); decline correct |
| i9 par | 0.0572 | 0.0614 (B=64) | every forced arm loses (0.93x); decline correct |

The cost term (`read_bytes > l2_per_core`) is doing real work here and S45's
reading of it survives the wider sweep unchanged.

---

## 3. The binding resource, priced with counters rather than argued

`perf stat -r 3`, `taskset -c 4`, `cpu_core/` PMU only so nothing multiplexes
against `cpu_atom`. **Only power-of-two `B` arms are compared to each other:**
`--move-panel=W:B` permutes every eligible map including the untimed generator,
and that generator term is identical across power-of-two `B` (same shift
lowering, same streaming behaviour) — visible as a flat `instructions` = 33.192 M
across every blocked arm at side 1024. `off` is shown for scale with that caveat.

### side 1024, 1 thread

| B | ms | l1_miss | **l2_hit** | **l2_miss** | stalls_total | fb_full | MLP | dTLB |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 2.321 | 1.021 M | 0.005 M | **1.007 M** | 8.057 M | **47.2% of cyc** | **5.95** | 0 |
| 8 | 1.077 | **0.206 M** | 0.074 M | 0.132 M | 1.689 M | 2.8% | 4.00 | 0 |
| 16 | 0.942 | 0.499 M | 0.434 M | 0.066 M | 1.295 M | 4.6% | 2.34 | 0 |
| 32 | 1.086 | 1.051 M | 1.018 M | 0.033 M | 1.481 M | 7.3% | 3.66 | 0 |
| 64 | 0.912 | 1.053 M | 1.031 M | 0.022 M | 1.072 M | 3.7% | 2.48 | 0 |
| **128** | **0.885** | 1.053 M | 1.033 M | **0.019 M** | **1.025 M** | **3.0%** | **2.23** | 0 |
| 256 | 0.913 | 1.052 M | 1.012 M | 0.039 M | 1.070 M | 3.3% | 2.21 | 0 |
| 512 | 1.056 | 1.050 M | 0.969 M | 0.081 M | 1.557 M | 3.7% | 2.45 | 0 |

### side 2048, 1 thread

| B | ms | l1_miss | l2_miss | stalls_total | fb_full | MLP |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 16.925 | 3.905 M | **3.911 M** | 49.763 M | **50.0%** | 5.65 |
| 8 | 5.102 | **0.365 M** | 0.554 M | 19.300 M | 11.4% | 3.43 |
| 16 | 4.966 | 1.503 M | 0.178 M | 9.753 M | 14.1% | 2.31 |
| 32 | 4.534 | 1.927 M | 0.101 M | 9.085 M | 16.2% | 3.53 |
| 64 | 4.077 | 1.806 M | 0.062 M | 6.304 M | 7.2% | **2.01** |
| **128** | **3.856** | 1.723 M | **0.056 M** | 7.341 M | 12.7% | 2.20 |
| 256 | 4.810 | 3.341 M | 0.124 M | 10.455 M | 9.9% | 2.29 |
| 512 | 11.504 | 2.854 M | **3.155 M** | 38.143 M | 32.0% | 5.43 |

### What that says, candidate by candidate

**L1 capacity — rejected, as the brief already showed.** `l1_miss` is
*anti*-correlated with speed across the blocked arms: 0.206 M at the slowest
blocked arm, 1.053 M at the fastest, 5x apart, at side 1024.

**Outstanding-miss capacity (MSHR / line-fill buffers) — MEASURED AND REJECTED.
This was the brief's leading candidate and it is refuted directly.** At the
optimum, `l1d_pend_miss.fb_full` is **3.0% of cycles** and MLP is **2.23 — the
LOWEST of any arm**. The slow arms have *more* misses in flight, not fewer
(B=8: MLP 4.00; B=32: 3.66). The fill buffers do bind — but only on the arms
nobody would ship: `off` (47–50% fb_full, MLP ~5.9) and `B=512` at side 2048
(32%, MLP 5.43). **A rule that sized `B` to saturate outstanding-miss capacity
would be optimising a resource that is 97% idle at the optimum.**

**dTLB — rejected.** `mem_inst_retired.stlb_miss_loads` reads 0 on every arm at
every side.

**LLC capacity — rejected.** A 4 MB array in a 36 MB L3.

**L2 residency — THIS IS IT, and it is monotone.** Order the side-2048 arms by
`l2_miss` and you get the ms order exactly: 0.056 (B=128, 3.86 ms) < 0.062 (64,
4.08) < 0.101 (32, 4.53) < 0.124 (256, 4.81) < 0.178 (16, 4.97) < 0.554 (8, 5.10)
< 3.155 (512, 11.50). **Eight arms including `off`, no inversion.** At side 1024
the same order holds with two inversions (B=32, and the B=8/512 pair, both inside
a 2% ms band). `stalls_total` tracks ms with the same fidelity at both sides.

Blocking's whole 2.6–3.1x win is one conversion: `off` serves **1.007 M of its
1.021 M** L1 misses from beyond L2; every blocked arm serves nearly all of them
from L2. The residual ordering *between* blocked arms is the same quantity at a
finer grain.

**And the mechanism behind `l2_miss` is the prefetcher — the brief's third
candidate.** Each column stream inside a block reads `B` consecutive elements,
i.e. `B·w/line` consecutive lines:

| B (i9) | run per column stream | `l2_miss` at side 2048 | vs compulsory (0.262 M) |
| ---: | --- | ---: | --- |
| 8 | 32 B — **half a line** | 0.554 M | **2.1x** — each line demand-fetched twice, no stream to detect |
| 16 | 64 B — exactly one line | 0.178 M | 0.7x — one demand miss per line |
| 32 | 128 B — two lines | 0.101 M | 0.4x — adjacent-line prefetch engages |
| 64–128 | 256–512 B — 4–8 lines | 0.062 / 0.056 M | **0.2x** — the L2 streamer locks on |

That is why `B` wants to be **several lines deep per stream** on this part, and
it is a property of the prefetcher, not of any capacity.

---

## 4. Why this cannot be derived, stated as a comparison rather than a claim

The M4 wants **16** and the i9 wants **64–128**: the i9 wants a block **4–8x
larger**. Every quantity the compiler can read is **larger on the M4**:

| fact | source | M4 Pro | i9 P-core | M4 / i9 |
| --- | --- | ---: | ---: | ---: |
| `l1d.bytes` | sysctl / sysfs | 131072 | 49152 | **2.67x** |
| `l1d.line_bytes` | sysctl / sysfs | 128 | 64 | **2.00x** |
| `l1d.sets` | page bound / sysfs | 128 | 64 | **2.00x** |
| `l1d.ways()` | derived | 8 | 12 | 0.67x |
| `l2_per_core()` | sysctl / sysfs | 3.36 MB | 2.00 MB | **1.68x** |
| `line/w` (the rule's floor) | derived | 32 | 16 | **2.00x** |
| `slots` at side 1024 | derived | 32 | 12 | **2.67x** |
| **measured optimum `B`** | **this sweep** | **16** | **128** | **0.125x** |

Only `ways` is larger on the i9, by 1.5x, against an 8x difference in the answer;
`ways^5` is not a derivation. **No monotone function of the readable facts orders
these two parts the way the measurement does.** The quantity that does order them
— how much strided reach the hardware prefetcher has before a block has to hand
it a contiguous run — is not in `sysctl`, not in `sysfs`, and not published for
Apple silicon.

The measured witness for that asymmetry is in the unblocked arms: the M4's
unblocked strided walk costs **0.796 ns/element** at side 1024 and the i9's costs
**2.325** — a 2.9x gap (4.0x at side 1536) on the identical instruction stream
over the identical geometry, with the M4 carrying the *larger* line. The M4's
prefetcher already handles the strided pattern; the i9's needs the block to build
it a contiguous stream. That is the whole 8x.

### Both of the rule's terms are refuted, each on the other machine

* **Floor (`line/w`), measured on the i9:** a block row shorter than a line
  refetches it. It holds on the i9 (B=8 is 8–18% slower than B=16 at both sides).
  On the M4 it is **false**: B=16 covers 16 of the 32 f32 in a 128 B line — the
  same half-line case — and is the **optimum** at both sides, beating B=32 by
  4.4% at 1024 and **15%** at 2048.
* **Ceiling (`slots/2`), measured on the M4:** the block's read lines share the
  reachable sets with the write stream. It holds on the M4 (B=64 and beyond fall
  off hard). On the i9 it is **false**: `slots/2` is **6**, and B = 16, 32, 64,
  128 and 256 — up to 42x the ceiling — all beat B=8.

Each term is real on the machine it was measured on and wrong on the other. Their
geometric mean lands inside the M4's plateau (exactly, at 16, at both sides) and
below the left edge of the i9's. **That is not a bug in the arithmetic; it is two
machines disagreeing about the shape of the cost.**

### The candidate rules that were priced, and why none ships

| rule | M4 1024 / 2048 | i9 1024 / 2048 / 1536 | verdict |
| --- | --- | --- | --- |
| current: `sqrt(floor · slots/2)` | 16 / 16 — **exact** | 8 / 8 / 12 — 1.15–1.27x short | the incumbent |
| clamp to `>= floor` | 32 / 32 — **-4.4% / -15.2%** | 16 / 16 / 16 — recovers most | **regresses the M4 15%** |
| `ceiling = slots` (drop the /2) | 32 / 16 — -4.4% / exact | 8 / 8 / 16 — helps 1536 only | ~nothing, and a regression |
| `B = line/w` | 32 / 32 — -4.4% / -15.2% | 16 / 16 / 16 | same M4 regression |
| a swept per-profile multiplier | by construction exact | by construction exact | **this is the flag with extra steps** — rejected |

The fourth row is the honest test of the brief's escape hatch. A recorded
`Sme::panel_l1d_ratio`-style scalar would have to be **1** on the M4 and **4–8**
on the i9 as a multiplier of `line/w`, i.e. it would carry the entire answer and
derive nothing — `2.6960` vs `3.1183` in S45's words, "a fitted constant wearing
a derivation's clothes". It is not a microarchitectural quantity with an
independent reading; there is no counter, no sysfs node and no ISA rule that
pins it. **So it is not taken, and the rule is left where the evidence is
strongest.**

---

## 5. The finding this session did NOT go looking for — and the one thing that WAS derivable

**M4 Pro, side 1536, both thread counts: the rung fired with B=32 and lost
1.75x (1t) and 1.41x (threaded).** Confirmed at 21 interleaved cycles with the
identity control landing on `off` in both cells.

It is a **fire/decline** defect, not a block-size one — no block wins there: the
best of the 17 arms in the wide sweep is **1.53x** slower than not blocking, and
the best of six in the 21-cycle confirm is **1.69x** slower. Both existing terms vote
to fire and both are individually reasonable: pressure is 1536 live lines against
64 slots (24x), and the 9.44 MB read array is well past the 3.36 MB per-core L2
share. What they cannot see is that on **this** part the unblocked walk is
already cheap — **0.594 ns/element, better than side 1024's 0.796 and within 10%
of the best blocked traversal on the whole machine** — so there is no miss cost
left to recover, while the permutation at a non-power-of-two width costs two
magic-multiply divisions per element that a power-of-two width gets as shifts
(the i9's counters put that at +20 to +25% instructions, §2.1).

### The fix: decline when the width is not a power of two

**The benefit is unreadable; the COST is not.** `move_panel_index` divides the
counter by `b` four times and by `w/b` twice. When `w` is a power of two, every
divisor of `gcd(w, rows)` is one too — so `b` and `w/b` both are, and `-O2`
lowers all six to shifts. When `w` is not, `b` and `w/b` **cannot both be**, so a
magic-multiply sequence lands on every element. That is arithmetic over a graph
fact (`MoveSite::width`), with no machine fact anywhere in it, and the i9's
counters price it: **+20 to +25% instructions** (185.4 M at B=16/32 against
223.2 M at B=12/24 and 232.6 M at B=96/192, same loop, same data, §2.1).

So the clause is: a large **known** cost meeting an **unpredictable** benefit is a
bet, and this is the one cell where the bet was measured losing.

```rust
if !site.width.is_power_of_two() {
    return None;
}
```

One `if`, placed after the existing pressure/conflict/cost gate so every test
that documents one of those clauses still exercises it (sides 1025 and 544 both
decline on the conflict clause, before this one is reached).

**What it costs, said plainly.** It forgoes the i9's win at the same width. The
plateau there was B = 16..256 and the best arm measured 1.30x (1t) / 1.32x
(par) — but the **shipped** block was B=12, on the expensive lowering, and it
measured only **1.060x (1t) / 1.044x (par)**. So what is actually given up is
**5.7% and 4.2%**, to stop giving away **43% and 29%** on the M4. Revisit if the
benefit ever becomes predictable; reaching that plateau needs the fact §4 shows
is not readable, so a later session would have to close §4 first.

**Blast radius, checked rather than assumed.** Over the 174-cell emit sweep —
57 sources x 3 faces, every shape, matmul and example in the tree — **exactly 3
cells move, and all three are `transpose_1536`**. It is the only non-power-of-two
width in the suite that reaches this rung. The declined emission is byte-identical
to `--move-panel=off` under both `native` and `raptorlake`, so the decline
produces today's flat loop rather than a third code path.

**And it was measured, not just reasoned.** M4, width 1536, 21 interleaved cycles,
after the clause — `deduce` now lands on `off` while the forced arms still carry
the loss, which is the direct proof that the loss was the rung's and the decline
removes it:

| arm | 1t min / med / max | par min / med / max |
| --- | --- | --- |
| off | 1.3746 / **1.4268** / 2.8519 | 0.2575 / **0.2930** / 0.4131 |
| **deduce (declines)** | 1.3787 / **1.4166** / 2.4464 | 0.2624 / **0.2971** / 0.4470 |
| 16 forced | 2.4177 / 2.4765 / 3.6302 | 0.3813 / 0.4214 / 0.7792 |
| 32 forced | 2.3722 / 2.4415 / 3.1003 | 0.3808 / 0.4386 / 0.6665 |
| 1536 (identity ctl) | 1.3991 / 1.4630 / 1.7576 | 0.2705 / 0.3039 / 0.4369 |
| saxpy null ctl | 0.0993 / 0.0999 / 0.4052 | 0.0772 / 0.0980 / 0.1411 |

**deduce vs off: 1.007x at 1 thread and 0.986x threaded — both inside this
machine's ~6% binary-to-binary noise floor, against 0.572x and 0.708x before.**
The forced arms are unchanged at 0.576x and 0.667x, so nothing about the machine
moved; only the decision did.

The alternative was rejected: a threshold on `pressure` that declines M4-1536
(24) and keeps M4-1024 (32) would be fitted at its own boundary, which is exactly
the defect `move_block`'s existing note says the conflict clause exists to
prevent. Sides 512, 1024 and 2048 on both machines are unaffected either way.

---

## 6. Gates

| gate | result |
| --- | --- |
| `cargo test --workspace --release` | **1047 passed / 0 failed / 1 ignored** — the stated baseline, unmoved, before AND after the clause |
| `cargo fmt --all --check` | **clean** |
| `benches/emit_sweep_ab.sh` byte-identity | **exactly 3 of 174 cells move, all `transpose_1536`** — see below |
| values before timing | identical to OFF at every arm, every side, both thread counts, both machines |

### Byte-identity, and the gate proved able to fail FIRST

Order, because it is the point: **the injected-failure check was run before the
sweep it validates.** `benches/shapes/zz_injected_failure.mapal` (a deliberately
malformed source) was dropped in and the sweep run — it reported **6 of 177
FAILED**, named all three injected cells as `EMIT-FAILED-rc1`, and **refused the
whole run with rc=1** rather than hashing empty output to a constant that would
match another broken run. The file was then removed. Only then was the real sweep
read. The LLVM `emit` was rebuilt as `-p mapal-backend-llvm` each time: the
workspace test build does overwrite `target/release/examples/emit` with the CUDA
one, and that is the third silent-pass path the preflight exists for.

Three comparisons, all at 174 cells (3 known failures throughout —
`examples/vector.mapal` does not parse):

| comparison | moved | added |
| --- | --- | --- |
| pre-session source set -> `transpose_1536` added | **0** | 3 (`raw`/`rew`/`con`) |
| before -> after the `move_block` doc comment | **0** | 0 |
| **before -> after the width guard** | **3, all `transpose_1536`** | 0 |

**That third row is the blast-radius answer.** Across 57 sources x 3 faces —
every shape, every matmul, every example in the tree — `transpose_1536` is the
**only** cell the clause touches. No other non-power-of-two width in the suite
reaches this rung. And the moved emission is byte-identical to
`--move-panel=off` under both `native` and `raptorlake`, so the decline produces
today's flat loop rather than a third code path.

The three `transpose_1536` faces hash identically to each other, which is
expected: the shape holds no fold, so neither `--rewrite` nor `--contract` has
anything to change.

### Ladder impact

The `deduce` arm — the shipped path, no flag — against OFF, before and after:

| | M4 1024 | M4 1536 | M4 2048 | i9 1024 | i9 1536 | i9 2048 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1t, before | 1.440x | **0.572x** | 2.166x | 2.273x | 1.060x | 2.538x |
| **1t, after** | 1.440x | **1.007x** | 2.166x | 2.273x | **1.000x** | 2.538x |
| par, before | 1.725x | **0.708x** | 2.736x | 1.972x | 1.044x | 1.766x |
| **par, after** | 1.725x | **0.986x** | 2.736x | 1.972x | **1.000x** | 1.766x |

Four cells move: two losses removed (M4 1536, measured), two small wins forgone
(i9 1536, now byte-identical to OFF so 1.000x by construction). Every other
ladder cell is bit-for-bit what it was.

The whole diff: `crates/backends/llvm/src/profile.rs` (one `if` plus its note and
two test assertions), `benches/shapes/transpose_1536.mapal` (a new swept source,
+3 gate cells), `benches/shapes/blocksweep_i9.sh` (the sweep harness), and this
document. **Not committed.**
