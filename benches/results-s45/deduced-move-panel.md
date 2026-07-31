# S45 — the deduced move panel: every number lands here the moment it is taken

Worktree `agent-a5e299a1ffeefd714` off `1877b73` (the S44 flag). Plan:
`docs/components/backend-llvm/plans/plan-s45-deduced-move-panel.md`.
S44 shipped `--move-panel=W:B`, a hand-typed width and block size. This session deletes the
hand-typing: `W` becomes a graph fact, the cache geometry becomes a detected machine fact, and
the fire/decline decision and `B` become backend arithmetic.

## 0. The machine facts, READ ON BOTH BOXES TODAY (not carried)

```
M4 Pro                                     i9-14900F (100.81.226.103)
hw.cachelinesize          128              getconf PAGESIZE            4096
hw.pagesize             16384              cpu0/cache/index0 (P L1D)
hw.perflevel0.l1dcachesize            131072    size=48K  line=64  sets=64  ways=12
hw.perflevel0.l2cachesize           16777216    shared_cpu_list 0-1
hw.perflevel0.cpusperl2                    5    cpu0/cache/index2 (P L2)
hw.perflevel0.physicalcpu                 10      size=2048K ways=16 sets=2048
hw.perflevel1.l1dcachesize             65536      shared_cpu_list 0-1  <- PER PHYSICAL CORE
hw.perflevel1.l2cachesize            4194304    index3 (L3) 36864K ways=12 shared 0-31
hw.perflevel1.cpusperl2                    4    cpu16 (E L1D) 32K/8-way/64 sets/64 line
                                                cpu16 index2 (E L2) 4096K shared 16-19
```

Both machines reproduce the brief's verified values exactly. Two facts the brief did not carry
and that turn out to be load-bearing:

* **`hw.perflevel0.cpusperl2 = 5`** — the M4 Pro's 16 MB P-cluster L2 is shared by **five**
  P-cores, so the per-core share is **3.3554 MB**, not 16 MB. `TargetProfile::l2_bytes` is
  documented "per-core" and `apple-m` sets it to the shared 16 MB; that ambiguity is a defect
  this session has to fix explicitly, because the cost term below divides by it.
* **The i9's L2 `shared_cpu_list` is `0-1`** — two SMT threads of ONE physical core, so 2 MB
  per P-core, against 36 MB of L3 shared by all 32.

### Associativity is not exposed on macOS, and does not need to be

`sets = page_bytes / line_bytes` and `ways = l1d_bytes / (line_bytes · sets)`. A VIPT L1 that
must not alias cannot take index bits beyond the page offset, and every one of these parts sits
exactly at that limit. Checked against the two geometries where the truth is readable:

| part | page | line | derived sets | derived ways | measured (sysfs) |
| --- | ---: | ---: | ---: | ---: | --- |
| M4 Pro P-core | 16384 | 128 | **128** | **8** | not exposed; matches the brief's verified 128 sets / 8-way |
| i9 P-core | 4096 | 64 | **64** | **12** | `number_of_sets=64`, `ways_of_associativity=12` ✓ |
| i9 E-core | 4096 | 64 | **64** | **8** | `number_of_sets=64`, `ways_of_associativity=8` ✓ |

Three parts, two of them checkable, no recorded associativity anywhere.

## 1. The byte-identity gate, verified to be able to FAIL before it is trusted

`benches/emit_sweep_ab.sh` now sweeps **171 cells** (57 sources x 3 faces) — the 159 of S44 plus
`transpose_512`/`transpose_2048`, carried in from the S45 i9 worktree so both A and B runs see the
same source set. 3 cells always fail (`examples/vector.mapal` does not parse).

**Injected-failure check (rule: a gate that cannot fail is not a gate).** A deliberately malformed
`benches/shapes/zz_injected_failure.mapal` was dropped in and the sweep run:

```
benches/shapes/zz_injected_failure.mapal|raw|EMIT-FAILED-rc1
benches/shapes/zz_injected_failure.mapal|rew|EMIT-FAILED-rc1
benches/shapes/zz_injected_failure.mapal|con|EMIT-FAILED-rc1
emit_sweep_ab: 6 of 174 emissions FAILED — this run is NOT a valid baseline   (rc=1)
```

It reports the failure per cell **and** refuses the whole run, instead of hashing empty output to a
constant that would match another broken run. The file was then removed.

## 2. What `--target=native` moves TODAY, before any S45 change

Baseline `1877b73`, 171 cells, `generic` vs `native`: **13 cells differ, all `con` faces of
matmul/attn** (`tile_kc` off a 16 MB L2 instead of `generic`'s 512 KB). **No transpose cell moves.**
So switching the transpose ladder to `--target=native` is a clean instrument change: anything that
moves on a transpose source after this session is this session's rung, not the profile switch.

## 3. PRE-REGISTERED PREDICTIVE TEST — M4 side 512, run with the S44 FLAG, before any S45 code

The cost term this session proposes is **"an L1 defeat costs real money only when the re-swept read
array does not fit the private level below L1"** — `read_bytes > l2_bytes / cores_sharing_l2`. On
this M4 that budget is 16 MB / 5 = **3.3554 MB**, so the term **DECLINES side 512** (read array
1 MB) and fires at 1024 (4 MB) and 2048 (16 MB).

That prediction is in tension with S44's own standalone probe, which measured **2.09x at side 512**
and **2.00x at side 256** on this machine (`benches/results-s44/l1-micro-panel.md`, PREDICTOR TEST
3). Those were `tblock.c` numbers, never the emitted pipeline, and S44's rule 3 records that the
probe over-prices the integrated win by 1.7x.

**Written down before the run: if the emitted M4 side-512 pipeline shows a disjoint win at any B,
the cost term is REFUTED on this machine and I say so rather than shipping it.** If side 512 shows
no win in the pipeline — the i9's outcome — the term is confirmed at a second, independent point
and the probe/pipeline gap is what explains S44's 2.09x.

This test needs no new code: it is `--move-panel=512:B` against OFF, both at `1877b73`.

### RESULT — M4 side 512, 1 thread, emitted pipeline, 11 interleaved cycles

`SIDE=512 N=262144 BLOCKS="4 8 16 32 64" MAPAL_PAR=1 movepanel_ab.sh`, under `perflock.sh`.
Values identical to OFF at every arm (`-37 -4`); every arm's `.ll` differs from OFF's.

| arm | min | median | max | vs OFF (median) |
| --- | ---: | ---: | ---: | ---: |
| **off** | 0.1612 | **0.1877** | 0.1898 | |
| 4 | 0.1580 | **0.1658** | 0.1735 | 1.132x, **OVERLAP** |
| 8 | 0.1612 | 0.1687 | 0.1743 | 1.113x, OVERLAP |
| 16 | 0.1609 | 0.1704 | 0.1751 | 1.102x, OVERLAP |
| 32 | 0.1584 | 0.1664 | 0.1734 | 1.128x, OVERLAP |
| 64 | 0.1635 | 0.1693 | 0.1776 | 1.109x, OVERLAP |
| 512 (identity ctl) | 0.1690 | 0.1887 | 0.1999 | 0.995x — **overlaps OFF, as it must** |
| saxpy null ctl | 0.1023 | 0.1212 | 0.1237 | — |

**The cost term's prediction HELD.** Every arm overlaps OFF, and the tell is the minima:
OFF's min is **0.1612** against the best arm's **0.1580** — **1.02x**, i.e. the two distributions
share a floor and only the medians differ (OFF's spread is 0.1612–0.1898, 18%). By this project's
standard — disjoint or it is not a result, and nothing under ~6% on the unpinned Mac is a number —
**there is no win at M4 side 512 in the emitted pipeline.**

The S44 standalone probe measured **2.09x** at this exact geometry. It over-priced by a factor that
swallows the whole effect, which is S44's own rule 3 reproduced at a fourth point. So the tension
named above resolves in the cost term's favour: **the term declines side 512 on both machines, and
on both machines the pipeline agrees.** The i9's 0.901x LOSS and the M4's overlapping 1.13x are the
same verdict measured on two different parts.

### RESULT — M4 side 512, threaded (`MAPAL_PAR=14`), same 11 cycles

| arm | min | median | max |
| --- | ---: | ---: | ---: |
| **off** | 0.0665 | **0.0810** | 0.0952 |
| 4 | 0.0767 | 0.0941 | 0.1273 |
| 8 | 0.0722 | 0.0861 | 0.1038 |
| 16 | 0.0714 | 0.0835 | 0.1271 |
| 32 | 0.0626 | 0.0834 | 0.1174 |
| 64 | 0.0742 | 0.0840 | 0.0944 |
| 512 (identity ctl) | 0.0545 | 0.0820 | 0.1302 |
| saxpy null ctl | 0.0620 | 0.0876 | 0.1403 |

**Every B arm is SLOWER than OFF threaded** (0.0834–0.0941 against 0.0810), all overlapping. So at
M4 side 512 the rung is worth nothing at 1 thread and slightly negative threaded — the same shape
as the i9's 0.901x. Prediction 4 holds at both thread counts; the cost term stands.

## 4. THE DEDUCTION, read off the emitted text — no flag anywhere

`emit <src> - --rewrite --target=<t>`, nothing else on the command line. "FIRE B=n" is read from
the permutation's own divisor in the `.ll`; "DECLINE" means the emission is byte-identical to
`--move-panel=off`.

| source | `generic` (default) | `native` / `apple-m4-sme` (M4) | `raptorlake` (i9) |
| --- | --- | --- | --- |
| `transpose_16` | DECLINE | DECLINE | DECLINE |
| `transpose_512` | DECLINE | **DECLINE** | **DECLINE** |
| `transpose_1024` | DECLINE | **FIRE B=16** | **FIRE B=8** |
| `transpose_2048` | DECLINE | **FIRE B=8** | **FIRE B=8** |

**Every cell matches the measured ground truth**, including the two that pressure alone gets wrong
(both 512s) and the one the whole session turns on (i9 512). `generic` carries no L1D geometry, so
the DEFAULT target cannot fire the rung and its emission is unchanged — rule 1, at the type level.

**`B` is per machine from the same source file**: 16 on the M4 (S44 swept exactly this), 8 on the
i9. Two machines, two blocks, one program, zero constants typed.

### One defect this session's own test caught, and the term that fixes it

The first version of the decision was `pressure > 1 AND cost`. `move_block_reproduces_both_machines`
was written to include **side 1025** — S44 measured it running **2.12x faster unblocked than side
1024**, with blocking costing 0.623x — and the first version **fired there**, with pressure
**1.0009**: 1025 lines live against 1024 slots, one line over.

The fix is not a bigger threshold (any number in `(1, 2]` would be fitted at exactly the boundary
it had to clear). It is the session's own headline as a third term: **`sets_touched < sets` —
conflict, not capacity.** Side 1025's stride is not a multiple of the line, so the walk reaches
every set; it is capacity-limited, and capacity is measured free on this part (S43: flat 32 KB to
8 MB). Each of the three terms now has its own measured witness, and none is a fitted number:

| term | what it says | witness |
| --- | --- | --- |
| `sets_touched < sets` | the walk is defeated by conflict, not capacity | **side 1025**: pressure 1.0009, and blocking measures **0.623x** |
| `lines_live > slots` | it needs more lines than it can reach | **side 128**: a real 32-of-128 set collapse, and it measures **1.000x** |
| `read > l2/core` | and losing them costs something | **i9 side 512**: pressure 21.3 and it measures **0.901x** |

## 5. GATES — all green, and the byte-identity result cell by cell

| gate | result |
| --- | --- |
| `cargo test --workspace --release` | **1046 passed / 0 failed** (1037 before — nine new pins) |
| `cargo fmt --all --check` | clean |
| `emit_sweep_ab.sh`, **`generic`**, before vs after | **0 of 171 cells moved** |
| `emit_sweep_ab.sh`, **`--target=native`**, before vs after | **exactly 6 cells moved** — listed below |

**The 6 cells that moved, and the reason for each:**

| cell | faces | why it moved |
| --- | --- | --- |
| `transpose_1024.mapal` | raw, rew, con | pressure 32, read 4 MB > 3.36 MB ⇒ **fires, B=16** |
| `transpose_2048.mapal` | raw, rew, con | pressure 128, read 16 MB > 3.36 MB ⇒ **fires, B=8** |

**And the ones that did NOT move, with the reason for each** — this is the half that says the
deduction is precise rather than eager:

| cell | why it stayed |
| --- | --- |
| `transpose_512` (3 faces) | pressure 21.3, but the 1 MB read array fits the 3.36 MB share — **cost term** |
| `transpose_16` (3 faces) | pressure 0.06 — nothing to win |
| `saxpy`, `reduce` (all sizes) | `captures == 0` / a fold body: **not a move site at all** |
| `fir`, `conv2d`, `attn`, every `matmul` | the map body holds a `Fold` ⇒ a `TileSite`, and the two recognizers are disjoint by construction |
| `gather` | the read address is data-dependent, not affine in `(t÷C, t%C)` |
| every generator map inside every source | `captures == 0` |

The `raw` face moving as well as `rew` is worth noting: recognition works on the un-rewritten IR
too, so the record does not depend on a rewrite pass having run.

**The gate had to be repaired mid-run, and it caught the failure it documents.** After
`cargo test --workspace --release`, the sweep's preflight refused the binary: *"does not accept
--contract — wrong emit binary?"*. Two crates build an example named `emit` and the workspace test
build had rebuilt the CUDA one over the LLVM one at `target/release/examples/emit`. Rebuilt with
`-p mapal-backend-llvm` and re-run. **That is the third silent-pass path S44 added the preflight
for, firing on a real occurrence.**

## 6. M4 Pro — transpose 1024, deduced against the S44 flag it replaces

`movepanel_ab.sh` under `perflock.sh`, `--target=native`, `-O2 -march=armv8-a+sme2`, 11 interleaved
cycles. **`deduce` is the shipped path with NO flag on the command line**; the numbered arms are
`--move-panel=1024:B` forced, kept only so the deduction can be scored against them.
Values identical to OFF at every arm (`-37 15`) before any timing.

### 1 thread

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 0.8175 | **0.9121** | 1.9977 | |
| **deduce (no flag, B=16 derived)** | 0.5747 | **0.5892** | 1.1490 | **1.548x** |
| 8 forced | 0.5611 | 0.5819 | 0.9951 | 1.567x |
| 16 forced | 0.5606 | 0.6161 | 0.9553 | 1.481x |
| 32 forced | 0.5760 | 0.5906 | 0.9245 | 1.544x |
| 1024 (identity ctl) | 0.8164 | 0.8735 | 1.3058 | 1.044x — **overlaps OFF** |
| saxpy null ctl | 0.0993 | 0.0998 | 0.5338 | — |

Ranges overlap ([0.8175, 1.9977] vs [0.5747, 1.1490]); each cycle is a separate `exec` and the
maxima carry launch noise, so this is a **median result with the overlap stated**, exactly as S44
reported the same cell. **S44's hand-typed B=16 measured 1.578x here; the deduction measures
1.548x — the same number, with nothing typed.**

### Threaded (`MAPAL_PAR=14`)

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 0.2467 | **0.3499** | 0.3699 | |
| **deduce (no flag)** | 0.1323 | **0.1489** | 0.2670 | **2.350x** |
| 8 forced | 0.1337 | 0.1686 | 0.1976 | 2.075x |
| **16 forced** | 0.1300 | **0.1364** | 0.2219 | **2.565x, DISJOINT** (max < OFF min) |
| 32 forced | 0.1393 | 0.1830 | 0.2249 | 1.912x |
| 1024 (identity ctl) | 0.2366 | 0.2748 | 0.3976 | overlaps OFF |
| saxpy null ctl | 0.0748 | 0.1025 | 0.1680 | — |

S44 measured 0.1450 for its hand-typed B=16 threaded; the deduced arm measures 0.1489 and the
forced one 0.1364, i.e. **the same cell**. The win still GROWS with thread count on this machine
(1.55x -> 2.35x), so S44's rule 24 classification survives the deduction.

### The deduction is strictly MORE PRECISE than the flag it replaces

`deduce` and `--move-panel=1024:16` are **not** byte-identical, and the difference is the finding:
the diff is **one hunk**, and it is the *generator* map `ia -> map { t -> (t*7+13)%101-50 }`.
S44's flag permutes **every** eligible map — its `move_panel_index` gates on `w`/`b`/divisibility
only — so it also permutes a `captures == 0` generator that has no cross-element reach and cannot
benefit. The deduction touches exactly the one recognized move site: 3 `urem i64` against the
flag's 6.

That generator runs before `t0`, outside the timed region, so the two arms are timing-equivalent
by construction — which makes them an **internal noise control**: 0.5892 vs 0.6161 at 1 thread
(4.6%) and 0.1489 vs 0.1364 threaded (9%), in opposite directions. That is the S39 finding
reproduced (±6% between binaries that cannot differ in the measured region) and it is why nothing
under ~6% is read as a result here.

## 7. M4 Pro — transpose 2048: the derivation FIRES correctly and UNDER-SHOOTS on B

Prediction P4, written down before the run: the rule derives **B=8** here (slots = 2 sets x 8 ways
= 16, and 16+1 > 16 excludes 16), while S44's standalone probe found **16** best. 9 cycles, 1 thread.

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 6.0533 | **6.2647** | 8.9199 | |
| **deduce (no flag, B=8 derived)** | 2.9626 | **3.1183** | 3.9900 | **2.009x, DISJOINT** |
| 8 forced | 2.7826 | 2.9741 | 3.4451 | 2.106x |
| **16 forced** | 2.5525 | **2.6960** | 2.9900 | **2.324x** |
| 32 forced | 2.9512 | 3.0135 | 3.3969 | 2.079x |
| 2048 (identity ctl) | 5.9380 | 6.0506 | 6.4238 | overlaps OFF |
| saxpy null ctl | 0.0993 | 0.0996 | 0.1082 | flat to 0.3% |

**P4 REFUTED, in the direction it was pre-registered for: B=16 beats the derived B=8 by 15.7% on
medians** (2.6960 vs 3.1183), which is well outside the ~6% noise band this machine has, and the
probe called it. The deduction still fires correctly and still wins **2.009x disjointly**; it
leaves ~14% of the available win on the table at this side.

**And the fix is not "drop the write term", because that term is load-bearing at the other side.**
Without it the rule would return `B = slots`: 16 here (better) but **32 at side 1024**, which
measured **0.1830 threaded against B=16's 0.1364 — 34% worse**, reproducing S44's 29%. So the two
sides disagree about what happens at `B = slots`, and the rule is kept where the evidence is
strongest (side 1024, both machines, both thread counts, disjoint) with the shortfall reported as
a number rather than closed with a special case.

### M4 side 2048, threaded (`MAPAL_PAR=14`), 9 cycles

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 1.3704 | **1.7432** | 1.8385 | |
| **deduce (no flag, B=8)** | 0.4828 | **0.5313** | 0.6668 | **3.281x, DISJOINT** |
| 8 forced | 0.4768 | 0.5674 | 0.9044 | 3.072x |
| **16 forced** | 0.4596 | **0.4813** | 0.7665 | **3.622x, DISJOINT** |
| 32 forced | 0.6065 | 0.7489 | 1.0037 | 2.328x |
| 2048 (identity ctl) | 1.3129 | 1.7236 | 1.8460 | overlaps OFF |
| saxpy null ctl | 0.0740 | 0.0984 | 0.1318 | — |

Same direction, smaller: the derived B=8 wins **3.281x disjointly** and B=16 would have given
3.622x — **9.4% unclaimed** threaded against 15.7% at one thread.

## 8. i9-14900F — the cross-compiled leg, `--target=raptorlake`, no flag

`benches/shapes/i9_ladder.sh 100.81.226.103`, 7 interleaved cycles, emitted and cross-compiled on
the Mac (the box has gcc and no clang), linked and run on the box. Pinning as the S44 run of
record: 1t = `taskset -c 4` (a 5500 MHz P-core), par = `taskset -c 0-31` with `MAPAL_PAR=32`.
**Values gated first and they passed on every shape** — `transpose [-37 15]` byte-equal across
Mapal 1t/par, C++ 1t/mt and NumPy.

**The emission gate, before any timing:**

```
side=512:  deduction DECLINED (byte-identical to OFF)
side=1024: deduction FIRED with B=8
side=2048: deduction FIRED with B=8
```

### 1 thread (`Mcyc` = median ms x mean GHz — the S37b unit for sub-5 ms cells)

| side | arm | min | median | max | Mcyc | vs OFF |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 512 | off | 0.1823 | **0.1964** | 0.2144 | 0.42 | |
| 512 | **deduce (DECLINED — same binary)** | 0.1876 | 0.2118 | 0.2339 | 0.49 | (noise: it IS OFF) |
| 512 | 8 forced | 0.2216 | 0.2309 | 0.2466 | 0.61 | **0.851x LOSS** |
| 512 | 16 forced | 0.2166 | 0.2194 | 0.2352 | 0.57 | **0.895x LOSS** |
| 512 | 128 forced | 0.2209 | 0.2258 | 0.2369 | 0.60 | **0.870x LOSS** |
| 1024 | off | 2.3569 | **2.4074** | 2.6415 | 9.21 | |
| 1024 | **deduce (B=8)** | 1.0720 | **1.0814** | 1.0858 | 3.44 | **2.226x, DISJOINT** |
| 1024 | 8 forced | 1.0646 | 1.0686 | 1.1901 | 3.53 | 2.253x |
| 1024 | 16 forced | 0.9849 | 0.9868 | 1.0061 | 3.30 | 2.439x |
| 1024 | 128 forced | 0.9255 | **0.9287** | 0.9432 | 3.04 | **2.592x** — the machine's optimum |
| 2048 | off | 11.1506 | **12.5589** | 14.4259 | 53.81 | |
| 2048 | **deduce (B=8)** | 4.6499 | **5.0659** | 6.4086 | 17.80 | **2.479x, DISJOINT** (3.02x in cycles) |
| 2048 | 16 forced | 4.2570 | 4.3505 | 5.0766 | 15.48 | 2.887x |
| 2048 | 128 forced | 3.9146 | **3.9872** | 4.0205 | 13.84 | **3.150x** (3.89x in cycles) |

**Side 512 is the whole point of the session and it lands:** the deduction declines, and every arm
that could have been forced there **loses 0.85–0.90x**, reproducing S44's 0.901x/0.907x. Pressure
alone scores it 21.3 and would have fired.

**The B gap, measured rather than argued:** the derived B=8 leaves **14.2%** at side 1024 (1.0814
vs 0.9287) and **21.3%** at side 2048 (5.0659 vs 3.9872) against this machine's optimum of 128.
Predicted before the run at ~13%; held at 1024, larger at 2048.

### Threaded (32 threads, whole box)

| side | arm | min | median | max | vs OFF |
| ---: | --- | ---: | ---: | ---: | ---: |
| 512 | off | 0.0552 | **0.0619** | 0.0885 | |
| 512 | 8 / 16 / 128 forced | | 0.0626 / 0.0662 / 0.0724 | | **all ≤ 1.0x** — nothing to win |
| 1024 | off | 0.1734 | **0.1928** | 0.2464 | |
| 1024 | **deduce (B=8)** | 0.1270 | **0.1628** | 0.1969 | 1.184x, overlapping |
| 1024 | 8 / 16 / 128 forced | | 0.1387 / 0.1420 / 0.1375 | | 1.39x / 1.36x / 1.40x |
| 2048 | off | 0.8569 | **0.8963** | 1.1165 | |
| 2048 | **deduce (B=8)** | 0.5940 | **0.6882** | 0.7764 | **1.302x, DISJOINT** |
| 2048 | 8 / 16 / 128 forced | | 0.6792 / 0.6712 / 0.6978 | | 1.32x / 1.34x / 1.28x |

The threaded plateau is as flat as S44 found it (0.1375–0.1420 across B=8…128 at side 1024), so no
threaded best-B is read off one run. The win still **shrinks** with thread count on this box
(2.23x → 1.18x at 1024) against **growing** on the M4 — S44's i9 correction to rule 24, unchanged
by the deduction.

**Controls (rule 22): the run is not void.** The saxpy null arm measured 0.6459 / **0.6482** /
0.6503 at 1 thread — a **0.7% total spread**, and it agrees with the S44 i9 run of record's
independently-measured 0.6494 to **0.2%**. The machine did not move between sessions.

### The shape ladder on the i9, and an honest confound

| shape | this session (`raptorlake`, deduced) | S44 record (`generic`, no rung) |
| --- | ---: | ---: |
| **transpose 1t** | **1.0812** | 2.4154 |
| **transpose par** | **0.1517** | 0.2728 |
| transpose C++ 1t / mt | 2.2538 / 0.4773 | 2.2872 / 0.4714 |
| transpose NumPy | 2.1995 | 2.2263 |
| saxpy 1t | 0.6506 | 0.6494 |
| gather 1t | 2.0071 | 2.1240 |
| reduce 1t | 0.3971 | 0.3897 |
| conv2d 1t | 0.2501 | 0.2945 |
| fir 1t | **1.9546** | **1.6671** |

**Transpose now beats naive C++ at one thread on this box — 1.0812 against 2.2538, 2.08x,
DISJOINT** (arm max 1.0882 < C++ min 2.1720), where S44 recorded them overlapping and inseparable.
Threaded it is 3.15x ahead of C++ mt, disjoint.

**The confound, stated rather than buried: the i9 legs moved profile as well as rung.** S44's i9
run emitted under `generic`; this one names `raptorlake`, which also carries AVX2 vector facts
(`vec_bytes` 32, `vec_regs` 16). Checked directly: `fir` emits **differently** under the two
profiles, and **identically** with the move rung on or off — so **fir's 17% regression and conv2d's
15% improvement are the AVX2 tile factors, not this session's rung.** The clean attribution is the
byte-identity sweep: under a *fixed* profile, the only cells this session moves are the two
transposes. The fir regression under `raptorlake` is a real, separate finding and is left as one.

## 9. BOTTOM LINE

**The flag's two hand-typed numbers are gone and the win survives, on both machines, with nothing
on the command line but the name of the machine.**

| | S44 | S45 |
| --- | --- | --- |
| `W` | typed on a flag | `mapal_ir::MoveSite.width`, a graph fact |
| which maps | every eligible map, globally | exactly the recognized move sites (3 `urem` vs 6 on this source) |
| fire / decline | always, wherever the flag divided | three-term arithmetic over record × profile |
| `B` | typed on a flag, swept | derived from the L1 geometry: **16 on the M4, 8 on the i9** |
| cache geometry | assumed 8-way, never read | line + L1D + sets detected; **ways is arithmetic** |
| default emission | unchanged (flag off) | unchanged (`generic` carries no L1D) |

**The acceptance table, measured:**

| machine | side | deduction | measured |
| --- | ---: | --- | --- |
| M4 | 128 | decline | 1.000x — nothing to win ✓ |
| M4 | 512 | decline | overlapping at 1t, *slower* threaded ✓ |
| M4 | 1024 | **fire, B=16** | **1.548x 1t / 2.350x threaded** ✓ |
| M4 | 2048 | **fire, B=8** | **2.009x 1t / 3.281x threaded, both DISJOINT** ✓ |
| i9 | 512 | **decline** | every forced arm **loses 0.85–0.90x** ✓ |
| i9 | 1024 | **fire, B=8** | **2.226x 1t DISJOINT** / 1.184x threaded ✓ |
| i9 | 2048 | **fire, B=8** | **2.479x 1t / 1.302x threaded, both DISJOINT** ✓ |

**What is NOT claimed.**

* **`B` is the largest block the L1 geometry can hold, not the optimum.** It is exactly S44's swept
  optimum at M4-1024 — the geometry both machines' main case was measured at — and it under-shoots
  by **15.7% at M4-2048**, **14.2% at i9-1024** and **21.3% at i9-2048**. The i9's optimum of 128 is
  10.7x what that L1 can hold in the reachable set, so it is **not an L1-residency effect and this
  model cannot produce it**. Named, not hidden, and not closed with a per-machine table.
* **The cost term is measurement-selected, not derived from first principles.** Two forms fit the
  i9's six points; they disagree only on the M4, and the M4-512 run was made **first**, before the
  code existed, to decide between them. It could have refuted the design and did not.
* **`sets` on macOS is a checked heuristic** (`page / line`), not an architectural law. It
  reproduces the two geometries where truth is readable and the M4's verified reading; it is wrong
  for PIPT L1s larger than their page reach and for alias-handling designs. Linux reads the truth.
* **The i9 ladder legs changed profile as well as rung** (`generic` → `raptorlake`, which carries
  AVX2 vector facts). Only the transpose rows are this session's; fir's 17% regression under the
  new profile is a separate, real finding.

## 10. Final gate state

| gate | result |
| --- | --- |
| `cargo test --workspace --release` | **1047 passed / 0 failed** (1037 at `1877b73`) |
| `cargo fmt --all --check` | clean |
| `emit_sweep_ab.sh` `generic`, before vs after | **0 of 171 moved** |
| `emit_sweep_ab.sh` `--target=native`, before vs after | **6 moved**, all transpose 1024/2048 faces |
| the OFF arm is the old default, proven | `--move-panel=off --target=native` hashes **byte-equal to the pre-S45 emission** at all three transpose sides — so every A/B above compares against exactly what S44 measured |
| values, M4 | identical to OFF at every arm, every side, both thread counts |
| values, i9 | identical across Mapal 1t/par, C++ 1t/mt, NumPy, all six shapes |
| controls | identity arm overlaps OFF everywhere; saxpy null flat to 0.3% (M4) and 0.7% (i9, agreeing with the S44 record to 0.2%) |

---

# BLOCKER PASS — the two merge blockers

## 11. BLOCKER 1 — the fir regression: the recorded fact was right, the rung was wrong

**Bisected at the emission level first.** `zen3` = `generic` + `vec_bytes 32` + `vec_regs 16`;
`raptorlake` = `zen3` + the L2/L1D facts. fir, conv2d and attn emit **byte-identically under `zen3`
and `raptorlake`** — so the cache facts this session added are not responsible, and the culprit is
`vec_bytes`. The FIR window rung reads exactly one profile fact, `tile_j` (= `vec_bytes` at f32).

**And the full survey, since fir was only found because someone looked:** with the rung held OFF,
the `generic` → `raptorlake` switch moves **11 of 27 shape sources** — every `fir`, `conv2d` and
`attn` size, i.e. every tile/window rung — and **no other shape** (saxpy, reduce, transpose, gather
are untouched).

**Root cause.** The block is `subrows × tile_j` lanes and one `<tile_j x elem>` accumulator
legalizes to `acc_vecs_per_row` machine registers, so a block costs `subrows · acc_vecs_per_row`
of them. `WINDOW_SUBROWS` was a hardcoded **4**: 16 of NEON's 32 registers — half the file, which
is why 4 was never wrong on the machine it was swept on — and **16 of AVX2's 16**, the entire file
with nothing left for the window operands. Its own doc comment argued no register budget applied
("a memory accumulator") and recorded the value as "unjustified at 4, swept once alongside the
matmul rung and never separately". The i9 refuted the argument.

**Fix: delete the literal, read the budget that already exists.** `func/mod.rs::window_subrows` is
`TargetProfile::tile_i` — `vec_regs / (2 · acc_vecs_per_row)`, the "spend at most half the vector
file on accumulators" policy that already documents this exact failure ("8 spills: 128 accumulators
≫ 32 NEON regs"). It is **4 on every profile that existed before S45** (byte-identical) and **2** on
a 16-register file.

**Measured on the i9, three arms back-to-back, 15 interleaved cycles, values identical (`2169 1888`):**

| arm | min | median | max | vs `generic` |
| --- | ---: | ---: | ---: | ---: |
| `generic` (S44's ladder profile) | 1.6295 | **1.6831** | 1.6952 | |
| `raptorlake` **before** (4 subrows × 32 lanes) | 1.7784 | **1.8020** | 4.7643 | **0.934x — the regression** |
| `raptorlake` **after** (subrows = tile_i = 2) | 1.6591 | **1.6892** | 2.0419 | **0.996x — gone** |

**The regression is closed: 1.8020 → 1.6892, level with `generic` to 0.4%** (well inside noise, and
the ranges overlap). Note the honest correction to the size: measured back-to-back the regression is
**6.7%**, not the 17% quoted from the cross-session ladder comparison — same sign, smaller magnitude,
and the cross-session number was inflated by the session gap.

**Verdict: the recorded fact (`vec_bytes: 32`) is right for this box and stays; it exposed a real
derivation defect in a rung that had none.** `generic`, `apple-m`, `apple-m4-sme`, `native` and
`cuda-ada` all keep `tile_i() == 4`, so no existing emission moves.

## 12. BLOCKER 2 — what actually binds at the i9's optimum, measured with counters

The old rule derived `B` from an L1 residency budget. The i9's optimum is 128, which is 10.7x what
its L1 can hold in the reachable set — so the first job was to **price the candidates rather than
fit a bigger constant.** `perf stat` on the box, `taskset -c 4` (P-core), `cpu_core` PMU only
(Raptor Lake multiplexes `cpu_core`/`cpu_atom` — the S44 trap), transpose side 1024, 1.15 M loads:

| arm | ms | cycles | instructions | **L1-dcache-load-misses** | dTLB-load-misses | LLC-load-misses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| off | 2.526 | 16.21 M | 18.81 M | **1 053 802** | 289 | 683 |
| B=8 | 1.072 | 9.75 M | 33.03 M | 206 414 | 291 | 259 |
| B=16 | 0.940 | 9.14 M | 33.03 M | 499 456 | 282 | 316 |
| **B=128** | **0.929** | 8.83 M | 33.03 M | **1 053 618** | 287 | 253 |

**`off` and `B=128` miss L1 the same number of times — 1 053 802 against 1 053 618, a 0.02%
difference — and B=128 is 2.7x faster.** B=8 misses **five times less** than B=128 and is
**slower** than it. The L1 miss count does not merely fail to predict the ordering, it
**anti-correlates** with it.

Ruled out by the same table: **TLB** (flat at ~290 misses across every arm, including `off`) and
**LLC** (~250–680, i.e. zero — a 4 MB array in a 36 MB L3). Instruction counts are identical
across the three blocked arms (33.026 M), so the ordering is not instruction overhead either; and
the blocked arms execute **76% more instructions than `off`** (33.0 M vs 18.8 M) while running 2.7x
faster, which is what a latency-bound kernel looks like.

What is left, and what the numbers point at: **memory-level parallelism.** IPC rises monotonically
with B — 3.39 → 3.61 → 3.74 — while misses rise with it. Larger blocks put more independent misses
in flight for a 512-entry reorder buffer to absorb; smaller ones keep a tiny resident set the
machine was never stalling on. At side 2048 a second effect appears and is also visible: LLC misses
collapse **204 639 → ~6 300** the moment any blocking is applied (32 MB working set against 36 MB
of L3), which is why every blocked arm wins big there regardless of B.

> **The binding resource at the i9's optimum is memory-level parallelism, not cache residency.**
> No quantity in `L1d` prices it, so no L1-derived rule can produce 128. Stated with the
> measurement, and carried as a known gap.

### The rule that ships, and what it fixes

Two costs, both measured, and the block is their geometric mean:

```text
floor   = line / sizeof(elem)   traffic: a block row shorter than a line refetches it
ceiling = slots / 2             conflict: the block's read lines share the reachable sets
                                          with its write stream
B       = largest divisor of gcd(width, rows) <= sqrt(floor * ceiling)
```

Both multipliers are 1 inside `[floor, ceiling]`; that window is normally empty, and a product of
two opposing multipliers is minimised at the geometric mean. **No threshold, no per-machine number.**
Each bound is a measurement: B=8 on the i9 (half a line per block row) is 15% slower than B=16 at
side 1024 and 24% at 2048; B=`slots` on the M4 is 29% (S44) and 34% (S45) slower threaded at side
1024 and 56% at side 2048.

| case | floor | ceiling | mean | **B** | measured optimum |
| --- | ---: | ---: | ---: | ---: | --- |
| M4 1024 | 32 | 16 | 22 | **16** | **16 ✓ exact** |
| M4 2048 | 32 | 8 | 16 | **16** | **16 ✓ exact** (was 8, 15.7% off) |
| i9 1024 | 16 | 6 | 9 | **8** | 128 — 14% short, unreachable by construction |
| i9 2048 | 16 | 6 | 9 | **8** | 128 — 21% short |

### Re-measured on the M4 with the new derivation — side 2048, 9 cycles

**1 thread** (derived B is now 16, was 8):

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 6.0753 | **6.2912** | 9.7187 | |
| **deduce (no flag, B=16 derived)** | 2.9039 | **2.9786** | 3.7465 | **2.112x, DISJOINT** |
| 8 forced | 2.9815 | 3.0964 | 3.8774 | 2.032x |
| 16 forced | 2.7979 | **2.8623** | 3.3193 | 2.198x |
| 32 forced | 3.2151 | 3.2954 | 3.7970 | 1.909x |
| 2048 (identity ctl) | 5.8615 | 6.2659 | 6.9549 | overlaps OFF |
| saxpy null ctl | 0.0994 | 0.0998 | 0.1197 | flat to 0.4% |

**Threaded (`MAPAL_PAR=14`):**

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| off | 1.4330 | **1.5303** | 1.9250 | |
| **deduce (B=16)** | 0.5365 | **0.5535** | 0.6498 | **2.765x, DISJOINT** |
| 8 forced | 0.5592 | 0.6041 | 0.6230 | 2.533x |
| 16 forced | 0.5001 | **0.5260** | 0.5857 | 2.909x |
| 32 forced | 0.6049 | 0.6502 | 0.6853 | 2.354x |
| 2048 (identity ctl) | 1.3759 | 1.4749 | 1.8480 | overlaps OFF |

**P4 is now satisfied at both thread counts.** The derivation picks the fastest arm at side 2048:
B=16 beats B=8 by 4.0% at 1 thread and **13% threaded**, and beats B=32 by 15% / 24%. The residual
between the `deduce` arm and the `16 forced` arm (2.9786 vs 2.8623; 0.5535 vs 0.5260) is the same
generator-map difference as at side 1024 — it is outside the timed region, so those two arms are an
internal noise control, and they agree to 4–5%.

## 13. MERGE GATE — re-run after both blocker fixes

| gate | result |
| --- | --- |
| `cargo test --workspace --release` | **1047 passed / 0 failed** (1037 at `1877b73`) |
| `cargo fmt --all --check` | clean |
| `emit_sweep_ab.sh` **`generic`**, pre-S45 vs now | **0 of 171 cells moved** — the fir fix is byte-identical here because `tile_i() == 4` on every pre-S45 profile |
| `emit_sweep_ab.sh` **`--target=native`**, pre-S45 vs now | **the same 6 cells**, all transpose 1024/2048 faces. Nothing new moved |
| injected-failure check, re-run | 3 malformed cells reported as `EMIT-FAILED`, **6 total failures, rc=1** — the gate still refuses a run it cannot measure |
| M4 values | identical to OFF at every arm, both sides, both thread counts |
| i9 values | identical across Mapal 1t/par, C++ 1t/mt, NumPy, all six shapes; fir arms identical (`2169 1888`) |
| i9 emission still current | `transpose_{512,1024,2048}` under `raptorlake` hash **byte-equal to the `.ll`s that were timed** — the derived B did not move on that machine, so §8's tables stand without a re-run |

### ADR-0032 — the boundary, checked rather than asserted

`MoveSite` carries `width`, `rows`, `cq`, `cr`, `elem`, `len`. Every one is a property of the
program. Grepping the whole `mapal-ir` diff for machine vocabulary
(`cache|line_bytes|l1d|l2|ways|sets|page|slots|pressure|threshold`) returns **one** hit, and it is
the doc sentence that states the boundary:

> *"…what to do about it, needs a line size and an associativity — machine facts this crate must
> never learn."*

No cache size, no line size, no threshold crossed. The join happens only in
`TargetProfile::move_block`, where both halves are in scope.

### Fires and declines, final, on both machines

| source | `generic` (default) | `native` (M4) | `raptorlake` (i9) | measured |
| --- | --- | --- | --- | --- |
| `transpose_16` | DECLINE | DECLINE | DECLINE | nothing to win |
| `transpose_512` | DECLINE | **DECLINE** | **DECLINE** | M4 overlapping / i9 **0.85–0.90x LOSS** |
| `transpose_1024` | DECLINE | **FIRE B=16** | **FIRE B=8** | 1.548x / 2.350x · 2.226x / 1.184x |
| `transpose_2048` | DECLINE | **FIRE B=16** | **FIRE B=8** | 2.112x / 2.765x · 2.479x / 1.302x |

## 14. WHAT CHANGED IN THE BLOCKER PASS

| | before | after |
| --- | --- | --- |
| M4 side 2048 block | 8 (15.7% off the optimum) | **16 — the optimum, at both thread counts** |
| M4 side 1024 block | 16 | 16 (unchanged) |
| i9 block | 8 | 8 (unchanged; the 14–21% gap is now explained by counters, not guessed at) |
| fir on the i9 under `raptorlake` | 1.8020 ms (0.934x vs `generic`) | **1.6892 ms (0.996x — regression closed)** |
| `WINDOW_SUBROWS` | a hardcoded 4, documented "unjustified" | derived from the register budget that already existed |
| why B is what it is | an L1 residency budget | the geometric mean of two **measured** costs, with counter evidence for what binds where it under-shoots |
