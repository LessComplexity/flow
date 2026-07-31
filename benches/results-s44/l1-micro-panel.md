# S44 — the L1 micro-panel rung: build it, measure the whole ladder

Machine: Apple M4 Pro, 10P+4E, 2 SME, L1D 128 KB, per-core L2 slice ~3.2 MB, shared L2 16 MB, pagesize 16384.
Worktree `agent-aa6edc589b1483b51` off `6d6302c`. Every number lands here the moment it is taken.

## Baseline — shape ladder v2, unmodified `main`, 2026-07-31

`RUNS=9 benches/shapes/ladder2_ab.sh`, machine exclusive via perflock. N=1048576, SIDE=1024.
Statistic: min for 1t, median for par (the harness's own choice, S33 §5b).

| shape | mapal-1t | mapal-par | cpp-1t | cpp-mt | numpy |
| --- | ---: | ---: | ---: | ---: | ---: |
| saxpy | 0.1451 | 0.0871 | 0.5225 | 0.1630 | 0.1786 |
| reduce | 0.7500 | 0.6607 | 0.5544 | 0.9412 | 0.1022 |
| transpose | **1.1206** | **0.2737** | 0.8530 | 0.2467 | 0.7624 |
| gather | 0.6450 | 0.1690 | 0.5152 | 0.1499 | 2.0205 |

**Read on the transpose row before any blocking work.** 1024² f32 moves 8 MB (4 in + 4 out).
1.1206 ms = **7.5 GB/s**; the C++ leg 0.8530 ms = 9.8 GB/s. This machine's measured DRAM floor is
~95 GB/s and its L2 plateau 249 GB/s (S43 §4). So the shipped transpose runs at **8% of DRAM
bandwidth** — an order of magnitude of headroom that is not a bandwidth limit at all.

The suspected mechanism, stated before the probe: **power-of-two stride conflict misses.** The read
stride is `side · 4 = 4096 B = 2¹²`. With 128 B lines the set index advances by `4096/128 = 32` sets
per read, so a 128-set L1D sees only **4 distinct sets** across the whole strided walk. At 8-way that
is 32 lines of usable capacity out of 1024. This is the exact trap `tlbreach.c` avoided by using odd
strides — and it is a *conflict* effect, not the *capacity* effect S43 priced at zero.

## Probe run 1 — `benches/shapes/tblock.c`, side 1024, 15 cycles — **RUN VOID (rule 22), effect real**

Values identical at every arm before any timing (checksum −34848).

| bs | min | median | max | ctl med | GB/s |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 0.307 | **0.310** | 0.374 | 0.499 | 27.1 |
| 16 | 0.305 | **0.317** | 0.325 | 0.497 | 26.5 |
| 32 | 0.310 | **0.317** | 0.362 | 0.501 | 26.5 |
| 64 | 0.626 | 0.636 | 0.814 | 0.499 | 13.2 |
| 128 | 0.788 | 0.792 | 0.924 | 0.497 | 10.6 |
| 256 | 0.798 | 0.802 | 0.925 | 0.499 | 10.5 |
| 512 | 0.798 | 0.806 | 0.928 | 0.499 | 10.4 |
| 1024 | 0.769 | **0.774** | 0.902 | **0.000** | 10.8 ← unblocked |

**2.497× at bs=8, disjoint** (arm max 0.374 < unblocked min 0.769). A cliff between 32 and 64, not a
curve. The control is flat to **0.8%** across the seven arms that measured it.

**Instrument defect, and it is a rule-23 catch.** The eighth arm's control reads 0.000 ms. Cause:
`transpose` takes `a` as `const float *restrict`, so clang knows it does not write `a`; the arm loop
has a constant trip count over a `static const` table, so after unrolling the eight identical
`control(a, n)` calls are **CSE'd into fewer real ones**. A null arm the compiler can delete is not a
null arm. Fixed by pointing the control at `b`, which `transpose` writes — CSE is then impossible.
Run 1 is VOID by its own gate; run 2 is the number of record.

## Probe run 2 — same probe, control repointed at `b`. **CLEAN. side 1024, 15 cycles, 1 thread, ms**

Values identical at every arm before any timing (checksum −34848, same as run 1).

| bs | min | median | max | ctl med | GB/s |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 0.367 | 0.381 | 0.414 | 0.498 | 22.0 |
| 8 | 0.301 | 0.311 | 0.315 | 0.500 | 27.0 |
| 16 | 0.315 | 0.319 | 0.379 | 0.506 | 26.3 |
| **24** | 0.296 | **0.302** | 0.342 | 0.500 | **27.8** |
| 32 | 0.310 | 0.311 | 0.332 | 0.498 | 27.0 |
| 48 | 0.455 | 0.462 | 0.545 | 0.503 | 18.2 |
| 64 | 0.622 | 0.629 | 0.697 | 0.498 | 13.3 |
| 128 | 0.788 | 0.797 | 0.958 | 0.498 | 10.5 |
| 256 | 0.800 | 0.805 | 0.976 | 0.500 | 10.4 |
| 512 | 0.798 | 0.801 | 0.853 | 0.498 | 10.5 |
| 1024 | 0.770 | **0.773** | 0.946 | 0.498 | 10.9 ← unblocked |

**Control spread 1.016× — CLEAN by rule 22.** The null arm is flat to 1.6% across eleven cells
measured back-to-back inside each cell, so the axis carries no drift.

**Best 0.302 ms at bs=24 against 0.773 unblocked = 2.56×, DISJOINT** (arm max 0.342 < unblocked
min 0.770; 0.428 ms of clear air). The whole plateau bs ∈ [8, 32] is within 6% of itself, so the
optimum is a **plateau with cliffs on both sides**, not a point: bs=4 loses 26% to loop overhead and
bs=48 has already given back a third of the win.

**The cliff is at the associativity, not the capacity.** bs=32 → 0.311, bs=48 → 0.462, bs=64 → 0.629.
A 32-row block holds 32 lines that map to 4 sets = 8 lines/set = exactly 8-way. 48 rows overflows it.
This is why the optimum is not near any L1D-capacity number: 32 rows × 128 B = 4 KB, **1/32 of L1D**.

## Probe run 3 — side 2048, 11 cycles, 1 thread, ms (blocking axis only)

Values identical at every arm (checksum −17581). **Control spread 1.014× — clean.**

| bs | min | median | max | ctl med | GB/s | vs unblocked |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 2.385 | 2.556 | 3.141 | 2.007 | 13.1 | 2.01× |
| 8 | 1.649 | 1.723 | 2.456 | 1.994 | 19.5 | 2.98× |
| **16** | 1.554 | **1.607** | 2.727 | 1.993 | **20.9** | **3.19×** |
| 24 | 1.717 | 1.744 | 2.589 | 2.006 | 19.2 | 2.94× |
| 32 | 1.826 | 1.981 | 2.612 | 2.009 | 16.9 | 2.59× |
| 48 | 2.366 | 2.442 | 2.869 | 1.997 | 13.7 | 2.10× |
| 64 | 2.923 | 3.024 | 3.252 | 1.993 | 11.1 | 1.70× |
| 128 | 3.322 | 3.381 | 3.873 | 2.003 | 9.9 | 1.52× |
| 512 | 3.316 | 3.378 | 3.632 | 1.993 | 9.9 | 1.52× |
| 2048 | 4.885 | **5.129** | 6.282 | 2.021 | 6.5 | — unblocked |

**3.19× at bs=16, disjoint** (arm max 2.727 < unblocked min 4.885). The win GROWS with side
(2.56× at 1024, 3.19× at 2048) and the optimum block SHRINKS (24 → 16) — both consistent with a
conflict mechanism whose pressure rises with the stride, not with a capacity mechanism.

## Instrument verification — rule 2, in the assembly

`clang -O3 -march=armv8-a+sme2 -S`. `_transpose` contains a versioned inner loop, and the version
predicate is **`lda == 1`** (`ldp x9,x15,[sp,#48]` reloads the spilled 5th argument, `ccmp x9,#1,#0,hi`
/ `cset w27,eq`, then `cbz w27, LBB1_19`). Every arm in this probe runs `lda ≥ side ≥ 1024`, so
`w27 == 0` and **every arm takes the same scalar strided path `LBB1_19`**. The contiguous vector path
(`ldr q0,[x25],#16` / `str q0,[x19],#16`) is unreachable at every `lda` this probe uses. One loop
body serves every arm, as designed; `bs` and `lda` are runtime arguments and appear nowhere in a
version predicate.

## Probe run 4 — THE MECHANISM TEST. side 1024, 15 cycles, 1 thread, ms

Two axes in one interleaved rotation: `bs` (block the traversal) and `pad` (widen the read array's
row stride, `lda = side + pad`, **traversal untouched**). Values identical at every arm (checksum
−34848). **Control spread 1.006× — clean.** `sets` = distinct L1D sets the strided read walks,
computed from `hw.cachelinesize`=128 and a 128-set 8-way reading of the 128 KB L1D.

| bs | pad | sets | min | median | max | ctl med | GB/s | vs ref |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1024 | 0 | **4** | 0.769 | **0.820** | 0.851 | 0.497 | 10.2 | 1.000× ← reference |
| 8 | 0 | 4 | 0.297 | 0.320 | 0.372 | 0.497 | 26.2 | 2.563× |
| 16 | 0 | 4 | 0.301 | 0.315 | 0.345 | 0.497 | 26.6 | 2.603× |
| **24** | 0 | 4 | 0.291 | **0.303** | 0.350 | 0.499 | 27.7 | **2.706×** |
| 32 | 0 | 4 | 0.303 | 0.310 | 0.357 | 0.499 | 27.1 | 2.645× |
| 48 | 0 | 4 | 0.448 | 0.451 | 0.572 | 0.497 | 18.6 | 1.818× |
| 64 | 0 | 4 | 0.603 | 0.610 | 0.658 | 0.497 | 13.8 | 1.344× |
| 128 | 0 | 4 | 0.777 | 0.789 | 0.848 | 0.497 | 10.6 | 1.039× |
| **1024** | **1** | 128 | 0.314 | **0.391** | 0.489 | 0.497 | 21.5 | **2.097×** |
| **1024** | **2** | 128 | 0.297 | **0.344** | 0.397 | 0.497 | 24.4 | **2.384×** |
| **1024** | **4** | 128 | 0.306 | **0.334** | 0.366 | 0.497 | 25.1 | **2.455×** |
| **1024** | **8** | 128 | 0.337 | **0.352** | 0.400 | 0.496 | 23.8 | **2.330×** |
| **1024** | **16** | 128 | 0.305 | **0.317** | 0.388 | 0.497 | 26.5 | **2.587×** |
| **1024** | **32** | 128 | 0.347 | **0.367** | 0.447 | 0.499 | 22.9 | **2.234×** |
| 1024 | 33 | 128 | 0.718 | 0.905 | 1.014 | 0.497 | 9.3 | 0.906× |
| 16 | 16 | 128 | 0.329 | 0.344 | 0.409 | 0.497 | 24.4 | 2.384× |

### What this settles

1. **Padding alone, with the traversal COMPLETELY UNBLOCKED, recovers 2.10×–2.59× of the 2.71×
   blocking achieves.** `pad=16` at `bs=1024` is 0.317 ms against the best blocked arm's 0.303 —
   within 4.6%, overlapping distributions. The iteration order was never the problem.
2. **The two levers do not compose.** `{bs=16, pad=16}` = 0.344 ms is *worse* than either alone
   (0.317 / 0.315). If blocking bought capacity residency and padding bought conflict relief they
   would stack. They do not, because **there is one defect and both levers treat it.**
3. ⇒ **The mechanism is CONFLICT — set-index collapse under a power-of-two stride — not CAPACITY.**
   This is fully consistent with S43 rather than a contradiction: S43 measured capacity (L1-vs-L2 =
   free, 32 KB → 8 MB) and `tlbreach.c` used **odd strides on purpose** to keep this effect out.
   Nobody had measured it. It is worth up to 3.19×.
4. **Not claimed:** `pad=33` (0.906×, a loss) is not explained. `lda=1057` gives a 4228 B stride that
   is not 128 B-aligned, so it is the one arm where reads are not line-aligned to the row start; its
   min 0.718 / max 1.014 is also the widest spread in the table. Flagged, not interpreted.

## The pre-registered predictor, and the arithmetic for every shape in the ladder

**Predictor:** a walk with byte stride `S` advances the L1D set index by `(S/128) mod 128` and
therefore touches `128 / gcd((S/128) mod 128, 128)` distinct sets when `S` is a multiple of 128, and
all 128 otherwise. **Blocking pays iff that count is small.** Working-set size against cache size
predicts nothing here — the winning block is 32 rows × 128 B = **4 KB, 1/32 of L1D**.

| shape | the strided walk | byte stride | sets touched | predicts | measured |
| --- | --- | ---: | ---: | --- | --- |
| saxpy | `x[i]`, `y[i]` sequential | 4 | 128 | **no win** | — |
| reduce | `x[i]` sequential | 4 | 128 | **no win** | — |
| gather | data-dependent indices | — | ~128 (unbiased) | **no win** | — |
| FIR | `x[t+k]` sliding, 512 B window | 4 | 128 (4 lines) | **no win** | — |
| conv2d | `img` along `j`; tap-rows 1026·4 apart | 4 / 4104 | 128 (4104 ∤ 128) | **no win** | — |
| matmul SME | packed A/B panels, contiguous | 4 | 128 | **no win** | `kc` at L1D = 0.785× ✓ |
| **transpose** | `a[j·1024 + i]` | **4096** | **4** | **big win** | **2.71× ✓** |

Two consequences worth naming:

- **conv2d escapes the trap by accident.** Its image is 1026 wide (1024 + a 2-pixel halo), so the
  tap-row stride is 4104 B — not a multiple of the 128 B line, hence no collapse. A conv2d over a
  1024-wide image would have a 4096 B tap-row stride and land squarely in it.
- **The matmul pack was already the conflict fix.** Packing linearizes both operand panels to unit
  stride, so there is no set collapse left for `kc` to remove — which is *why* sizing `kc` to L1D
  measured 0.785× rather than a win. The negative matmul prior and this positive transpose result
  are the same predictor evaluated at two different strides.

## PREDICTOR TEST 1 — transpose at an ODD side. **PREDICTION MADE BEFORE THE RUN, AND IT HOLDS.**

Prediction (written in the §"predictor" table above, before this run): side 1025 gives a
`1025·4 = 4100 B` read stride; `128 ∤ 4100`, so the walk reaches **all 128 sets** and there is no
collapse ⇒ **the UNBLOCKED, UNPADDED traversal should already run near the treated speed, and
blocking should have little or nothing left to buy.**

side 1025, 13 cycles, 1 thread, values identical at every arm (checksum −30683),
**control spread 1.010× — clean**, and the control's absolute level (0.498–0.503 ms) matches the
side-1024 run's (0.496–0.506) to within 1%, so the two runs are comparable machine states.

| bs | pad | sets | min | median | max | ctl med | vs ref |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **1025** | 0 | **128** | 0.271 | **0.386** | 0.460 | 0.499 | 1.000× ← unblocked, untreated |
| 8 | 0 | 128 | 0.403 | 0.423 | 0.451 | 0.503 | 0.913× |
| 16 | 0 | 128 | 0.600 | 0.620 | 0.648 | 0.500 | **0.623× — blocking HURTS** |
| 32 | 0 | 128 | 0.331 | 0.343 | 0.391 | 0.498 | 1.125× |
| 64 | 0 | 128 | 0.266 | 0.271 | 0.321 | 0.498 | 1.424× |
| 1025 | 16 | 128 | 0.347 | 0.429 | 0.528 | 0.500 | 0.900× |

**Side 1024 unblocked: 0.820 ms. Side 1025 unblocked: 0.386 ms. 2.12× on the medians, 2.84× on the
mins, distributions DISJOINT** (1025 max 0.460 < 1024 min 0.769). **One element of row stride, no
blocking, no padding, no compiler change — 2.12×.**

And the second half of the prediction holds too: at an odd side **blocking no longer pays and can
hurt** — bs=16 costs 0.623×, bs=8 costs 0.913×. The best blocked arm (bs=64, 0.271) does not beat
the unblocked arm's own minimum (0.271). There is nothing left for a working-set constraint to buy
once the set collapse is gone.

⇒ **The predictor called a 2× result in advance and got it.** It is no longer a retrodiction.

## PREDICTOR TEST 2 — conv2d at a 1024-wide image. **THE PREDICTION FAILS. The predictor was wrong.**

Two Mapal sources identical except the image row stride (`benches/shapes/conv2d_s1024.mapal` /
`conv2d_s1026.mapal`, same 1022×1022 output, same 9 taps, same generation law), run alternating in
one session by `benches/shapes/stride_ab.sh`, `-O2 -march=armv8-a+sme2`, MAPAL_PAR=1, 11 cycles.

**Structural gate, before any timing:** the two emitted `.ll` files differ on 332 lines raw but
**0 lines once every integer literal is normalised to `N`.** The instruction sequence, the rung that
fires, and the 9-tap unroll are identical; only extent and stride constants move. This is the
controlled experiment it claims to be. (The first gate — a raw line-count threshold — fired a false
VOID, because the conv rung unrolls 9 taps at compile-time offsets. Replaced with the normalised
comparison, which is both stricter and correct.)

| arm | min | median | max |
| --- | ---: | ---: | ---: |
| conv2d **stride 1024** (4096 B tap-row step, predicted to COLLAPSE) | 0.3124 | **0.3692** | 0.7060 |
| conv2d **stride 1026** (4104 B, predicted safe) | 0.3064 | **0.3627** | 1.3872 |
| saxpy (null control) | 0.1001 | 0.1169 | 0.5863 |

**1.018× — 1.8%, overlapping, no effect.** The predicted cliff is not there. A 2× effect could not
hide in this: the medians are 6.5 µs apart.

### Why it failed, and the corrected rule

The naive predictor counted **distinct sets reachable** and stopped there. It should have counted
**lines the walk needs live at once, against the slots those sets provide.**

| shape | lines live at once (reuse distance) | sets reachable | slots at 8-way | pressure | predicts |
| --- | ---: | ---: | ---: | ---: | --- |
| **transpose 1024** | **1024** — one A row per output element, all re-read next output row | 4 | 32 | **32× over** | big win |
| conv2d s1024 | 96 — 3 image rows, and **each row is 32 contiguous lines, exactly the set stride**, so rows tile sets [s,s+32), [s+32,s+64), [s+64,s+96) and never collide | 128 | 1024 | **0.09×** | **no win** |
| FIR | 4 (a 512 B window) | 128 | 1024 | 0.004× | no win |
| saxpy / reduce | 1 | 128 | 1024 | ~0 | no win |
| matmul SME | packed panels, unit stride, lines consumed fully | 128 | 1024 | ~0 | no win |

> **Corrected predictor: a power-of-two stride only hurts when the traversal touches a small part of
> each line AND needs many lines live at once.** Transpose uses 4 B of every 128 B line and needs
> 1024 lines live. conv2d uses *every* byte of every line and needs 96. The stride is the same 4096 B
> in both. **Stride alone predicts nothing; stride × line-utilisation × reuse distance does.**

That is what a predictive test is for, and it is worth more than the confirmation it replaced: the
rule that survived test 1 was over-general, and conv2d is the arm that found the boundary.

## PREDICTOR TEST 3 — the corrected rule made QUANTITATIVE, and declared before the run

The corrected rule (`pressure = lines_live / (sets × ways)`) was derived from the conv2d
refutation, so it currently explains its own counterexample. It becomes a law only if it calls a
number in advance. Arithmetic re-derived here from `hw.cachelinesize`=128 and L1D=131072 B
(1024 lines; 128 sets at 8-way), **not taken on trust**:

for side `S` at f32: byte stride `4S`; set stride `(4S/128) mod 128 = (S/32) mod 128`;
distinct sets `128 / gcd(S/32, 128)`; lines live ≈ `S` (one `a` row per output element, all of them
re-read on the next output row).

| side | stride B | set stride | sets | slots @8-way | lines live | **pressure** | **PREDICTION** |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 128 | 512 | 4 | 32 | 256 | 128 | **0.5×** | **NO WIN — blocking does nothing** |
| 256 | 1024 | 8 | 16 | 128 | 256 | **2×** | small win |
| 512 | 2048 | 16 | 8 | 64 | 512 | **8×** | solid win |
| 1024 | 4096 | 32 | 4 | 32 | 1024 | 32× | large — **measured 2.71× ✓** |
| 2048 | 8192 | 64 | 2 | 16 | 2048 | 128× | largest — **measured 3.19× ✓** |

**Side 128 is the load-bearing cell:** a power-of-two stride with pressure < 1. If blocking still
pays there, the pressure formula is wrong. *Confound named in advance:* at side 128 both arrays
together are 128 KB = L1D exactly, so "everything fits" is an alternative explanation for a null
result there. **Side 256 (512 KB total, outside L1D, pressure 2×) is the discriminating cell** — it
must show a *small* win, not none and not a large one.

### Threaded prediction, declared before the run

L1D is **per-core**, and each worker owns a contiguous band of output *rows* — but the read
`a[j·S + i]` sweeps **all S rows of `a` regardless of which output rows the worker owns.** So the
read walk, the lines live, and the set collapse are **identical per worker**: conflict pressure does
not dilute with thread count.

⇒ **I expect the blocked win to SURVIVE threading but to be SMALLER than at 1 thread**, because the
threaded baseline is additionally limited by shared L2/DRAM bandwidth that blocking does not remove.
This is the S43 pattern (`kc` +6.1% at 1t / −25.5% threaded; residency +71% / +5%). Concretely:
**≥1.3× threaded against 2.56× at one thread.** A threaded win *larger* than 2.56× would refute the
"already partly bandwidth-bound" half of this.

### PREDICTOR TEST 3 — RESULT. Sign and ordering: **held.** Magnitude: **refuted, it saturates.**

1 thread, `tblock.c`, 21 cycles each, values identical every arm, **control spread 1.000× in all
three runs** (side 128 ctl 0.008, side 256 ctl 0.031, side 512 ctl 0.124 — each flat to 3 digits).

| side | pressure | unblocked med | best blocked (bs) | best padded (pad) | blocked win | verdict vs prediction |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 128 | **0.5×** | 0.004 | 0.004 (32–128) | 0.004 (all) | **1.000×** | **NO WIN — HELD** |
| 256 | **2×** | 0.032 | **0.016** (64) | 0.017 (1–32) | **2.000×** | win, but not "small" |
| 512 | **8×** | 0.138 | **0.066** (48) | 0.064 (2–4) | **2.09×** | held |
| 1024 | 32× | 0.820 | 0.303 (24) | 0.317 (16) | 2.71× | held |
| 2048 | 128× | 5.129 | 1.607 (16) | — | 3.19× | held |

- **Side 128 confirms the load-bearing prediction: pressure < 1 ⇒ no win at a power-of-two stride**,
  and blocking actively costs (bs=8/16/24 all read 0.800×). *Weak evidence, and I am saying so:* the
  whole cell is 4 µs, at the timer's resolution floor, and the pre-declared confound (both arrays =
  128 KB = L1D exactly, so "it all fits" also explains a null) is live. **Side 256 is the
  discriminating cell and it is not resolution-limited** — 0.032 ms, 8× the tick.
- **The quantitative half of the prediction is REFUTED.** Pressure 2× was predicted to give a
  *small* win; it gives **2.00×**, already two thirds of what pressure 128× gives. The relationship
  is **monotone but saturating**, not proportional: 1.00 → 2.00 → 2.09 → 2.71 → 3.19 across
  0.5× → 128× of pressure.
- Mechanistically that is what it should do. Once the walk is oversubscribed *at all*, every line
  reuse is lost and the traffic amplification jumps straight to its ceiling (32 lines fetched per
  line used at f32); more pressure cannot lose the same reuse twice. What still grows past that is
  only the *cost per fetched line*, as the footprint walks out of L2 toward DRAM.

> **The law, as it now stands: `pressure = lines_live / (sets × ways)` predicts the SIGN and the
> ORDERING of a blocking win, and NOT its magnitude. Below 1 there is nothing to win; above ~2 the
> win is already near its ceiling.** Stated this way it survives all seven shapes, both conv2d
> strides, five transpose sides, and the matmul prior.

And at every side the **padding arms track the blocking arms** (256: 1.88 vs 2.00; 512: 2.16 vs 2.09;
1024: 2.59 vs 2.71), which is the conflict mechanism reproducing itself at four geometries.

## THE EMITTER RUNG — built, and measured inside the real pipeline

`EmitOpts::move_panel: Option<(u64, u64)>`, default `None`; `--move-panel=<W>:<B>`.
`crates/backends/llvm/src/func/bulk.rs::FnEmit::move_panel_index` — **a permutation of the map's
loop counter, not a loop nest**:

```text
p = ((rb·CB + cb)·B + dr)·B + dc      -- the counter, decomposed
t = (rb·B + dr)·W + cb·B + dc         -- the index it stands for
```

That shape is the whole correctness argument and it is why the diff is 60 lines rather than 300:
`perm` is a bijection of `[0,n)`, the parallel slices partition the counter, so their images still
partition the outputs — every element visited exactly once, by exactly one worker, values bit-
identical. A blocked *nest* would have needed a head and a tail for the partial rows at a slice
boundary; this needs neither, and `%lo`/`%hi` are untouched.

### Gates, before any timing

- **Values identical to OFF at every arm** (`-37 15`), including the declining and identity arms.
- **Gate 2 fired and caught a real defect in my own arm list**: B=24 emitted text byte-equal to OFF,
  because `1024 % 24 ≠ 0` and the rung correctly declines. Reported as VOID rather than tabled as
  "no effect" — which is exactly the failure mode (a declined gate read as a treatment) that would
  have made every null reading in this table meaningless. Arms restricted to divisors of W.

### transpose 1024, **MAPAL_PAR=1**, 11 cycles, absolute ms

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| **off** | 0.8046 | **0.8996** | 1.9391 | 1.000× |
| B=8 | 0.5606 | 0.5736 | 1.0734 | 1.568× |
| **B=16** | 0.5591 | **0.5700** | 0.9672 | **1.578×** |
| B=32 | 0.5899 | 0.6244 | 0.9070 | 1.441× |
| B=64 | 0.7074 | 0.7290 | 1.0451 | 1.234× |
| B=128 | 0.8719 | 0.9773 | 1.2659 | 0.920× |
| **B=1024 (identity)** | 0.8112 | 0.8469 | 1.1440 | 1.062× **overlaps OFF** |
| saxpy (null) | 0.0992 | 0.0998 | 0.3033 | flat |

- **1.578× at B=16.** Medians 0.8996 → 0.5700, far outside the 6% floor.
- **Honest on overlap: the min/max ranges DO overlap** ([0.8046, 0.9672]). The maxima are inflated by
  per-cycle process launch — each cycle is a separate `exec`, unlike `tblock.c`'s in-process loop.
  Reported as a median result with the overlap stated, not as "disjoint".
- **The identity arm overlaps OFF (1.062×)**, so the permutation arithmetic costs nothing measurable
  — the win is the traversal, not an accident of the extra address math.
- The B curve has the same shape as the standalone probe's, shifted: optimum at 8–16, degrading
  through 32/64, and **losing at 128**.
- **1.578× in the emitter against 2.71× standalone.** The emitter does not reach the C nest's speed
  (0.570 vs 0.303 ms) and its OFF arm is already slower (0.900 vs 0.820). Rule 3, in the direction it
  is usually stated: **the probe over-priced the integrated win by 1.7×.** Same direction as S42's
  `kc.c` (predicted 1.448×, delivered +6.1%).

### transpose 1024, **MAPAL_PAR=par** (full pool), 15 cycles, absolute ms

Values identical to OFF at every arm; every arm's emission differs from OFF (the rung fired).

| arm | min | median | max | vs OFF |
| --- | ---: | ---: | ---: | ---: |
| **off** | 0.2264 | **0.2890** | 0.3848 | 1.000× |
| B=8 | 0.1285 | 0.1673 | 0.2694 | 1.727× |
| **B=16** | 0.1304 | **0.1450** | 0.1924 | **1.993× — DISJOINT** (max 0.1924 < OFF min 0.2264) |
| B=32 | 0.1357 | 0.1875 | 0.2343 | 1.541× |
| B=64 | 0.1788 | 0.2001 | 0.2559 | 1.444× |
| B=1024 (identity) | 0.2368 | 0.2664 | 0.4413 | 1.085× **overlaps OFF** |
| saxpy (null) | 0.0758 | 0.0996 | 0.1420 | flat |

**THE THREADED PREDICTION I WROTE DOWN BEFORE THE RUN IS REFUTED, AND IN THE INTERESTING DIRECTION.**

I predicted the win would **survive but shrink** threaded (≥1.3×), on the S43 pattern that threaded
baselines are additionally bandwidth-bound. Measured: **1.993× threaded against 1.578× at one
thread — the win GREW**, and threaded is the only leg where the distributions come out disjoint.

The reason is visible in the scaling. OFF scales 0.8996 → 0.2890 = **3.11×** on the full pool.
ON (B=16) scales 0.5700 → 0.1450 = **3.93×**. **The set conflict was itself limiting parallel
scaling.** L1D is per-core, so every worker independently suffers the same collapse — the walk
`a[j·S + i]` sweeps all S rows of `a` no matter which output rows a worker owns, so pressure does not
dilute with thread count. Removing it frees all fourteen lanes at once, and the shared-bandwidth
ceiling I expected to mask the win turns out to sit above it.

This is the **opposite** of every optimization in S43's three-for-three table (`kc` +6.1%/−25.5%,
residency +71%/+5%, `nc` +18.7%/parity), and the mechanism explains why: those all optimized a
matmul loop that threading had already shrunk to 66% of the wall. A per-core conflict is not an
Amdahl term — it scales with the cores.

## PREDICTOR TEST 4 — side 544: pressure < 1 **off the timer floor, with no confound**

Side 128 was the weak cell (4 µs, and both arrays = L1D exactly). Side 544 replaces it. Arithmetic
re-derived here rather than taken on trust: `32 | 544`; stride `544·4 = 2176 B`; `2176/128 = 17`
exactly; `gcd(17, 128) = 1` ⇒ **all 128 sets**, 1024 slots, lines live 544 ⇒ **pressure 0.53** — the
same sub-1 pressure as side 128, at 1.1 MB/array (**outside L1D, so no "it all fits" confound**) and
0.088 ms (**20× the timer tick**).

15 cycles, values identical every arm, **control spread 1.000× — clean.**

| bs | pad | sets | min | median | max | GB/s | vs ref |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| **544** | 0 | 128 | 0.080 | **0.088** | 0.114 | **26.9** | 1.000× ← untreated |
| 8 | 0 | 128 | 0.082 | 0.106 | 0.135 | 22.3 | **0.830× — hurts** |
| 16 | 0 | 128 | 0.087 | 0.088 | 0.092 | 26.9 | 1.000× |
| 32 | 0 | 128 | 0.075 | 0.076 | 0.084 | 31.2 | 1.158× |
| 48 / 64 | 0 | 128 | 0.074 | **0.074** | 0.076 | 32.0 | **1.189×** best |
| 544 | 1–16 | 128 | 0.069 | 0.071 | — | 33.3 | 1.239× |
| **544** | **32** | **64** | 0.092 | **0.098** | 0.110 | 24.2 | **0.898× — see below** |

**The rule holds, and the absolute number is the strongest part.** Untreated side 544 runs at
**26.9 GB/s**. Untreated side 512 — one power of two away, pressure 8× — runs at **15.2 GB/s**.
**1.77× apart with no treatment on either.** And blocking, which was worth 2.09× at side 512, is
worth **1.189×** at 544 and *hurts* at bs=8.

**The pair 128 + 544 is what separates the two candidate rules.** Side 128 *does* have a set collapse
(stride 512 B ⇒ set stride 4 ⇒ only 32 of 128 sets) and still shows **no win**, because pressure is
0.5. So it is not "power-of-two strides are bad" and not "a set collapse is bad" — **it is pressure,
and only pressure.** Side 544 then confirms the same verdict from the other direction (no collapse,
non-power-of-two, sub-1 pressure), off the floor and without the confound.

**A bonus at the margin, unplanned and worth having:** the `pad=32` arm gives `lda = 576`,
`576·4 = 2304 B`, `2304/128 = 18`, `gcd(18,128) = 2` ⇒ **64 sets**, 512 slots, so pressure rises to
`544/512 = 1.06` — just over 1. It measures **0.898×, a small loss**. The rule's own threshold,
crossed by one arm of a sweep that was not designed to cross it.

## THE LADDER BOUNDARY — transpose vs the baselines, re-taken in ONE session

The README quotes transpose as one of exactly two shapes that "still go to C++ — the boundary of the
claim", on a carried threaded row of Mapal 0.290 / C++ 0.26 / NumPy 0.83. **Rule 19: a number that
was never re-taken has never been checked.** `benches/shapes/transpose_vs_baselines.sh` runs every
leg **alternating in one session**, from the same `ladder2_baseline.cpp` / `ladder2_numpy.py` the
published row came from, with values gated first. 1024², f32, B=16, 13 cycles, absolute ms.

**Values identical across OFF/ON at 1 thread and threaded** (`-37 15`), checked before any timing.

| leg | min | median | max |
| --- | ---: | ---: | ---: |
| mapal off, 1t | 0.8151 | 0.8352 | 1.1912 |
| **mapal ON, 1t** | 0.5596 | **0.5617** | 0.8222 |
| mapal off, par | 0.3538 | 0.3718 | 0.4149 |
| **mapal ON, par** | 0.1220 | **0.1406** | 0.1556 |
| cpp-1t | 0.8117 | 0.8345 | 4.0578 |
| cpp-mt | 0.3351 | 0.4963 | 0.7407 |
| numpy | 0.9297 | 0.9641 | 1.1745 |

| comparison | ratio | distributions |
| --- | ---: | --- |
| **ON threaded vs C++ mt** | **3.53×** | **DISJOINT** (0.1556 < 0.3351) |
| **ON threaded vs NumPy** | **6.86×** | **DISJOINT** |
| ON 1t vs C++ 1t | 1.49× | overlap of 0.0105 ms at the extreme tails only |
| **ON 1t vs NumPy** | **1.72×** | **DISJOINT** (0.8222 < 0.9297) |
| off threaded vs C++ mt | 1.33× | overlap |

> **Transpose stops being the boundary of the claim.** With the rung on it beats the C++ leg by
> **3.53× threaded with disjoint distributions**, and beats it at one thread too. The README's
> "transpose and gather still go to C++" is, for transpose, **no longer true on this machine** — and
> gather is untouched by this work, so the sentence needs narrowing to gather alone rather than
> deleting.

**Two honesty notes, both rule 19.**
1. **The carried numbers do not reproduce.** The published row has C++ mt at 0.26 ms; measured today
   it is 0.4963 median / 0.3351 min. Mapal OFF measures 0.3718 rather than 0.290. So *even without
   the rung* the published ordering does not hold in this session — which is the whole reason the
   comparison had to be re-taken rather than quoted.
2. **The C++ mt leg is the noisy one** — 0.3351 to 0.7407, a 2.2× spread — so the OFF-vs-C++ threaded
   comparison is not resolvable. The ON-vs-C++ comparison is, because 0.1556 sits clear of 0.3351.

## GATES

### Byte-identity — `benches/emit_sweep_ab.sh`, baseline tree vs this worktree

| | |
| --- | --- |
| cells swept | **165** (159 as of S43, plus the 2 new `conv2d_s*` sources × 3 faces) |
| known-failing cells | **3** — `examples/vector.mapal` does not parse (P0001/P0012/P0108), unchanged |
| real cells | **162** |
| **MOVED with no flags** | **0** |

### Rule 23 — the gate must report an INJECTED failure before its pass is worth anything

Re-run with `--move-panel=1024:16`: **70 of 162 cells move.** The gate is not blind.

And the *pattern* of which 70 is itself a check on the rung's precision:

- Sources whose map length is a multiple of 1024 move; `conv2d_s1026` (n = 1044484, `1024 ∤ n`) does
  **not**.
- `fir_65536|raw` and `conv2d_1024|raw` move, but their `rew` and `con` faces do **not** — because
  the rewritten faces are recognised as tile sites and take the tile rung, never reaching the generic
  `emit_map` path the flag gates. **The rung cannot touch a recognised tile site**, and the gate says
  so cell by cell rather than by assertion.

### The rule the threaded refutation earns — three categories, and they predict thread behaviour

The threaded result (1.578× at 1 thread → **1.993× threaded**, disjoint) is the opposite of every
optimization S42/S43 measured. Put together with them, a rule falls out that would have saved both
sessions work:

| what the optimization removes | 1 thread | threaded | why | example |
| --- | ---: | ---: | --- | --- |
| **a shared bottleneck** in a term threading already shrank | large | **shrinks** | the term is a smaller share of the threaded wall | `kc` +6.1% / **−25.5%**; operand residency +71% / **+5%**; `nc` +18.7% / **parity** |
| **a serial fraction** | nothing | **grows** | Amdahl, in reverse | parallel B pack: 0.998× / **1.381×** |
| **a per-core resource conflict** | real | **grows** | every core suffers it independently, so it does not dilute with thread count | **the move panel: 1.578× / 1.993×** |

> **Which of the three an optimization touches predicts its thread-count behaviour before it is
> measured.** S43's "three for three vanished threaded" was not bad luck: all three were the first
> category. A per-core conflict is not an Amdahl term.

### Test coverage — there was none, and adding it found a wrong claim of mine

`crates/backends/llvm/tests/move_panel.rs`, 5 tests. Before it, a change letting the flag reach the
matmul / FIR / conv2d tile rungs would have failed nothing.

| test | what it pins |
| --- | --- |
| `move_panel_is_a_bijection_of_the_iteration_space` | the property every value-identity claim rests on, at three geometries |
| `default_off_is_character_identical` | `EmitOpts::default()` is exactly the `None` emission |
| `the_rung_fires_when_the_panel_tiles_the_geometry` | the positive control — without it every negative below could pass vacuously |
| `a_panel_that_does_not_tile_the_geometry_declines_silently` | six decline cases, each byte-identical to OFF |
| `the_flag_cannot_reach_a_recognized_tile_site` | the tile rung shields its site from the flag |

**The last one failed on its first spelling, and the failure was informative.** I asserted "a matmul
source emits identical text OFF vs ON". **False** — a matmul source also contains ordinary *generator*
maps (`tq -> map {...} -> q`), which are eligible and do move. The module moves; the **site** does
not. Re-spelled differentially: turn tiling off, the site falls into the generic path, and the flag
must reach **strictly more** maps (counted by `urem i64`, two per permuted map). That is the fact
that is actually true, and the first version would have shipped a claim the byte-identity sweep
already contradicted.

*(This is also the rule-23 evidence for the tests themselves: the suite demonstrably fails when an
assertion is wrong, rather than passing everything put in front of it.)*

### Final gate status

| gate | result |
| --- | --- |
| `cargo fmt --all --check` | **clean** |
| `cargo test --workspace --release` | **1037 passed / 0 failed** (1032 baseline + 5 new) |
| byte-identity, no flags, 162 real cells | **0 moved** |
| rule-23 injection (`--move-panel=1024:16`) | **70 cells move** — the gate is not blind |
| values, every arm, every leg | **identical, checked before any timing** |
| assembly (rule 2) | the probe's only versioned loop is guarded on `lda == 1`; every arm runs `lda ≥ 1024` |
| control arm | flat in every timed run (1.000×–1.016×); no run voided for drift |
