# S43 — what binds the SME matmul at 2531 GF/s threaded

Live working file. **Appended the moment each number exists**, not at the end.
Machine: Apple M4 Pro (10 P + 4 E), 2 SME units, 16 MB shared L2, 128 KB L1D/P-core.
Every timed run through `benches/perflock.sh`. Worktree `agent-a00e835791cc868c5`, base commit `0518e76`.

## The question

N=4096 threaded: **54.291 ms = 2531 GF/s**. Two SME units retire **~4100 GF/s** aggregate
(`benches/sme/units.c`, zero memory traffic). What accounts for the missing 38%, given S43 already
measured that operand cache residency is worth **≤5% threaded** (54.291 → 51.788 ms)?

## The kernel's real geometry (read, not assumed)

`crates/backends/llvm/src/module.rs::sme_panel` with `apple-m4-sme`: `t=16`, `ti=tj=2`.

- Panel = **32 rows × 32 cols**; per k iteration: **`ti+tj` = 4 vector loads → `ti·tj` = 4 `fmopa`**.
- **Exactly 1 load per `fmopa`**, 64 B per load ⇒ 8 FLOP/byte. 2×2 is already the load-minimal
  factorization of 4 tiles (1×4 would need 5 loads for the same 4 `fmopa`), and 4 is the f32 ISA
  maximum. **There is no f32 arrangement with a better load:`fmopa` ratio.**
- N=4096: 128 i-panels × 128 j-panels × 4096 k × 4 `fmopa` = **268,435,456 `fmopa`** = 17.18 GB of
  64 B operand lines. Over 54.291 ms that is **316 GB/s** of load traffic — against ~95 GB/s DRAM,
  so it is already being served from cache.
- Per panel call the four streams are all unit-stride: A is `ap + k·32` (2 loads, contiguous,
  512 KB swept), B is `bk` and `bk + bj` where `bj = pack_w·k` = 256 KB apart (2 loads, each
  contiguous through a 256 KB packed panel). Working set per call ≈ 1 MB.
- Work distribution (`crates/mapal-rt/src/lib.rs::slice_ranges`) is **work-stealing**, not static:
  quantum `ti·t·c` = 32·4096 = 131072 elems ⇒ 128 blocks; `oversub = 4` for an i-invariant b
  (`func/conv.rs::slice_sizing`) ⇒ `wanted = 14·4 = 56`, `per = 128/56 = 2`, **64 slices over 14
  threads**. 64/14 = 4.57, so the quantization tail is `ceil(5)/4.57` ≈ **+9.4%** worst case.

---

## PROBE 1 — `benches/sme/unitload.c`: does a load compete with `fmopa` for the SHARED unit?

**Hypothesis (mine).** `units.c` measures 4100 GF/s with *zero* loads. `loadcost.c` measures loads
costing only 5% but at *one thread*, which is latency-bound at 4 chains and therefore has spare
issue slots to hide loads in. Neither can see the case that matters: several threads sharing one
unit while each also issues loads. Apple's SME is a per-cluster coprocessor and in streaming mode
the Z registers live there — so if the streaming loads are retired by the same shared block, loads
and `fmopa` compete and 4100 is not a 1-load-per-`fmopa` kernel's ceiling.

**Instrument.** `units.c` × `loadcost.c`: N threads, each running the same 4 independent `fmopa`
into the 4 f32 ZA tiles every iteration, with L ∈ {0,1,2,3,4} of the operands coming from memory.
Bodies are byte-for-byte `loadcost.c`'s. Buffers are **per thread** (32 KB) in the L1 pass so
residency is real per core; the second pass shares one 64 MB buffer.

**Assembly verified** (rule 2) — `clang -S`, inner loop of each variant, identical at `-O2` and
`-O3`:

| variant | vector loads in loop | `fmopa` in loop | insns |
| --- | ---: | ---: | ---: |
| ld0 | 0 | 4 | 6 |
| ld1 | 1 | 4 | 9 |
| ld2 | 2 | 4 | 11 |
| ld3 | 3 | 4 | 12 |
| ld4 | 4 | 4 | 13 |

Build: `clang -O3 -march=armv8-a+sme2 -o unitload unitload.c`; run `./unitload 40000000 3 1`
(40 M iterations/thread, best of 3, variants interleaved rep-outer/variant-inner, 300 ms warm-up).
**One cycle** of 3 reps per cell. Aggregate GF/s = wall clock over the whole join, exactly as
`units.c` computes it.

### Pass A — operands from a per-thread 32 KB buffer (L1-resident)

| threads | 0 loads | 1 load | 2 loads | 3 loads | 4 loads | L4/L0 | L4 slowest thread (ms) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 2003.4 | 2001.8 | 2003.3 | 2002.9 | **2003.5** | 1.000× | 40.9 |
| 2 | 3563.0 | 3561.4 | 3559.6 | 3560.4 | **3561.6** | 1.000× | 46.0 |
| 3 | 3630.6 | 3633.0 | 3630.3 | 3628.8 | **3633.1** | 1.001× | 67.6 |
| 4 | 4136.6 | 4127.9 | 3941.1 | 4127.5 | **3870.2** | 0.936× | 84.6 |
| 6 | 4088.9 | 4163.9 | 4126.4 | 4161.2 | **4072.4** | 0.996× | 120.6 |
| 8 | 4114.1 | 4129.2 | 4102.4 | 4115.2 | **4115.0** | 1.000× | 159.2 |
| 10 | 4104.6 | 4156.5 | 4099.2 | 4098.4 | **4049.1** | 0.986× | 202.2 |
| 12 | 4139.3 | 4174.7 | 4167.1 | 4169.3 | **4185.3** | 1.011× | 234.8 |
| 14 | 4159.6 | 4166.1 | 4177.1 | 4182.3 | **4183.8** | 1.006× | 274.0 |

### Pass B — operands from one shared 64 MB buffer (past L2)

| threads | 0 loads | 1 load | 2 loads | 3 loads | 4 loads | L4/L0 | L4 slowest thread (ms) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 2001.6 | 1024.2 | 915.1 | 843.9 | **781.8** | 0.391× | 104.7 |
| 2 | 3561.7 | 1763.9 | 1646.2 | 1540.7 | **1424.1** | 0.400× | 115.0 |
| 3 | 3097.6 | 1876.5 | 1719.9 | 1368.4 | **1292.7** | 0.417× | 190.1 |
| 4 | 4135.7 | 2295.0 | 2005.4 | 1871.3 | **1779.8** | 0.430× | 184.0 |
| 6 | 4151.9 | 2776.1 | 2578.4 | 2198.1 | **2053.4** | 0.495× | 239.3 |
| 8 | 4115.2 | 3182.1 | 2978.7 | 2820.1 | **2432.3** | 0.591× | 269.4 |
| 10 | 4150.8 | 3233.1 | 3008.4 | 2816.1 | **2887.5** | 0.696× | 283.6 |
| 12 | 4188.0 | 3155.4 | 3148.6 | 2759.7 | **2824.0** | 0.674× | 348.0 |
| 14 | 4179.7 | 3399.1 | 3232.2 | 2925.6 | **2772.5** | 0.663× | 413.6 |

### REFUTED — loads do not compete for shared-unit issue

Pass A row 14: **4183.8 GF/s with 4 loads per 4 `fmopa`** against 4159.6 with none — 1.006×, well
inside noise. The L4/L0 column is 1.00 ± 0.01 at every thread count except the single 4-thread cell
(0.936×, and its 2-load neighbour is 0.954× while 1- and 3-load are 1.00 — an outlier, not a trend).

⇒ **The ~4100 GF/s two-unit ceiling is fully reachable at 1 load per `fmopa`.** The load stream
costs nothing on the shared unit as long as the operands are L1-resident. My hypothesis is dead, and
with it the pessimistic reading that f32 SME has no lever left because 1:1 is architectural. The
1:1 ratio is **not** a ceiling: 4184 GF/s is available at exactly the emitted kernel's instruction
mix.

### What Pass B establishes instead

Same instruction mix, only the operand *source* changes, and throughput collapses. Side by side with
the real kernel's published thread-count sweep (N=4096, KC off, `docs/performance/s42-sme-roofline.md`
§5b):

| threads | real kernel N=4096 | probe, operands past L2 | probe, operands L1-resident |
| ---: | ---: | ---: | ---: |
| 1 | 787 | **782** | 2003 |
| 2 | 1454 | **1424** | 3562 |
| 3 | 1781 | 1293 | 3633 |
| 4 | 1993 | 1780 | 4137 |
| 6 | 2289 | 2053 | 4072 |
| 8 | 2350 | **2432** | 4115 |
| 10 | 2515 | **2888** | 4049 |
| 14 | 2531 / 2559 | **2773** | 4184 |

The real kernel tracks the past-L2 curve within ~10% at every thread count and sits 1.6–2.6× below
the L1-resident curve at every thread count. The 1-thread cells are an exact match (787 vs 782).

**This is in tension with S43's own integrated result** that forcing operands into an L1-resident
window is worth only 4.8% threaded (54.291 → 51.788 ms). Both cannot be right. That tension is what
the integrated N-sweep below exists to settle — the discriminator is that the packed-B working set
goes 1 MB → 4 MB → 16 MB → 64 MB across N=512/1024/2048/4096 while the instruction mix and
arithmetic intensity are identical. If threaded GF/s is FLAT across that sweep, residency is refuted
integrated (S43 is right, probe curve-matching is coincidence) and the binder is size-invariant. If
it falls with N, residency binds and the S43 instrument did not neutralize what it claimed (rule 5).

Prior data already on the record for that sweep, threaded: N=2048 = 6.783 ms = **2532 GF/s**,
N=4096 = 53.485 ms = **2571 GF/s**. Those two are flat within noise across a 4× change in B working
set, which leans toward *size-invariant*. N=512 and N=1024 threaded are not yet measured.

---

## PROBE 2 — integrated N-sweep, threaded: **residency REFUTED integrated**

Emitted from the real emitter at four sizes, `--rewrite --contract --target=apple-m4-sme`, linked
`-O2 -march=armv8-a+sme2` against `libmapal_rt.a`. SME fired at every size and every call is the
**packed** arm (`bn = 16 = t`, `bj = pack_w·k`), verified from the emitted call site:

```
N=512   mapal_sme_panel(..., i64 16, i64 8192,  i64 512,  i64 512)
N=1024  mapal_sme_panel(..., i64 16, i64 16384, i64 1024, i64 1024)
N=2048  mapal_sme_panel(..., i64 16, i64 32768, i64 2048, i64 2048)
N=4096  mapal_sme_panel(..., i64 16, i64 65536, i64 4096, i64 4096)
```

**9 alternating cycles over the four sizes**, two full discarded warm-up passes first, self-timed
region (`iter ms=`), full pool (14 lanes). Raw series in `benches/results-s43/nsweep.log`.

| N | packed B | A panel | min ms | **median ms** | max ms | GF/s | slices / 14 threads |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 512 | 1 MB | 64 KB | 0.19725 | **0.212916** | 0.237666 | 1261 | 16 / 14 |
| 1024 | 4 MB | 128 KB | 0.912417 | **0.942917** | 0.968125 | 2278 | 32 / 14 |
| 2048 | 16 MB | 256 KB | 6.563542 | **6.670041** | 6.789791 | **2576** | 64 / 14 |
| 4096 | 64 MB | 512 KB | 53.058209 | **54.163875** | 55.676333 | **2537** | 64 / 14 |

The N=4096 median **reproduces the 54.291 ms baseline exactly** (54.164 vs 54.291, 0.2% apart,
distributions overlap) — the harness is sound.

**2048 and 4096 are flat within noise (2576 vs 2537, 1.5%) across a 4× change in the packed-B
working set, 16 MB → 64 MB.** N=1024, whose entire working set (4 MB of B + 14 × 128 KB of A
panels = 5.8 MB) fits the 16 MB L2 with room to spare, is *lower* at 2278, and N=512 lower still.

⇒ **Operand cache residency does not bind the threaded kernel.** Making the whole problem fit in L2
buys nothing. This CONFIRMS S43's integrated 4.8% and REFUTES the reading of Probe 1's Pass-B curve
match — the shape agreement between the real kernel and the past-L2 probe is a coincidence, not a
mechanism. The binder is **size-invariant** and caps the kernel at ~2550 GF/s against a measured
4184 GF/s ceiling at the kernel's own instruction mix.

The small-N falloff is slice quantization, not residency: at N=1024 there are only 32 panel-aligned
blocks for 14 threads, so the makespan is `ceil(32/14) = 3` slices against an ideal 2.286 — 76%
efficiency, which recovers 2278 → ~3000. At N=4096 (64 slices) the same correction is
`4.571/5 = 91.4%`, recovering 2537 → ~2775. Both still far under 4184.

### What this leaves, and the reconciliation of the 1-thread vs threaded split

At **1 thread** residency IS the binder and is measured so (S43: 1.71×; and Probe 1 puts the real
1-thread kernel at 787 GF/s against the past-L2 probe's 782 — an exact match). One core sweeping a
64 MB packed B pulls B's half of the 17.2 GB from DRAM at ~49 GB/s, and a single core cannot reach
the part's ~95 GB/s.

At **14 threads** the cores work different i-panels but sweep the *same* packed B at the same time,
so B is shared in L2 and stops coming from DRAM. L2 is as fast as L1 for one thread
(`loadlevel.c`), which is exactly why forcing L1 residency then buys nothing. Both results are
right; they are different regimes.

**So the remaining candidate is the one no probe has yet held constant: aggregate shared-L2
bandwidth.** Probe 1's Pass A used *per-thread 32 KB* buffers — pure L1, zero L2 traffic. The real
kernel's operands are L2-resident but never L1-resident: each panel call sweeps ~1 MB (512 KB of
`ap` + 2 × 256 KB of packed-B panel) with no reuse inside the call, so **every load misses L1 and is
served by the shared L2**, at 316 GB/s aggregate. `loadlevel.c` measured 249 GB/s from L2 to ONE
thread; nothing has measured what 14 cores get at once. That is Probe 3.

---

## PROBE 3 — per-thread buffer-SIZE sweep at 4 loads

Prediction if shared-L2 refill bandwidth binds: at 14 threads, ld4 throughput falls as the
per-thread buffer grows past the 128 KB L1D *even while the aggregate stays inside the 16 MB L2*,
landing near 2537. If it stays at ~4184 all the way to 1 MB per thread, L2 bandwidth is refuted too
and the binder is inside the kernel (pack / read-out / dispatch).

### Attempt 1 — **VOID**, and the control is what caught it

`./unitload 20000000 3 0 1`, size as the OUTER loop, ld4 and ld0 interleaved within each cell:

| per-thread buffer | 1 thr | 2 thr | 4 thr | 8 thr | 14 thr |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 32 KB | 2000.8 (**1996**) | 3199.6 (3201) | 4014.2 (3638) | 4069.4 (4057) | 4160.7 (4113) |
| 128 KB | 1582.6 (**1583**) | 2790.6 (2790) | 3427.2 (3425) | 3557.7 (3367) | 3228.4 (3004) |
| 256 KB | 1201.4 (**1202**) | 2295.8 (2295) | 2833.4 (2658) | 3162.7 (3227) | 3125.9 (2977) |
| 512 KB | 1337.0 (**1336**) | 2387.5 (2481) | 3050.6 (3049) | 3194.3 (3166) | 2972.1 (2780) |
| 1024 KB | 1337.5 (**1337**) | 2481.1 (2482) | 3067.2 (3050) | 3157.5 (3171) | 3073.1 (2959) |
| 4096 KB | 1335.8 (**1335**) | 2478.0 (2481) | 3025.2 (2798) | 2922.4 (3152) | 2238.0 (2791) |

Numbers in parentheses are the **`ld0` control — a variant with ZERO loads in its inner loop**
(assembly-verified: 0 vector loads, 4 `fmopa`, 6 instructions). Its throughput *cannot* depend on
the buffer size, because it never touches the buffer inside the timed loop. It moved
**1996 → 1583 → 1202 → 1336 GF/s**, tracking `ld4` to within 1 GF/s at 1 thread for the first four
rows.

⇒ **The whole table is global clock drift, not a cache effect.** Voided. Nothing in it is quotable.
Note that the shape is also not a cache curve — it *dips* at 256 KB and recovers at 512 KB, which no
residency mechanism produces.

### METHOD NOTE, reusable and worth more than the run

**Any sweep that puts the swept parameter in the OUTER loop needs a zero-effect control arm, or
drift is indistinguishable from the effect.** This is the same failure mode as §7.1 of
`docs/performance/s42-sme-roofline.md` (the 1.73× cold/warm that put `loadcost.c`'s 1864 L1 ceiling
into the published record) and the same fix: interleave. Rule 14 says "interleave the variants" —
that is *necessary but not sufficient*. Interleaving the variants *within* a cell, as this run did,
leaves the sweep axis fully exposed: every cell of one row is measured before any cell of the next,
so a monotone clock droop is read off as a monotone parameter effect. **The rep loop must be
outermost across every cell of the sweep**, and a zero-effect arm should be carried so that when
drift does happen it is visible instead of plausible.

Cost of not having the control here: a clean, monotone, entirely fictitious 2000 → 1336 GF/s
"L1 cliff" at one thread that would have confirmed the hypothesis under test.

### Attempt 2 — rep loop outermost, all buffers faulted in up front

`clang -O2 -march=armv8-a+sme2`; `./unitload 20000000 3 0 1`, 3 reps, rep loop outermost over all
30 (size × threads) cells, ld4 and ld0 measured back to back inside each cell, every buffer
allocated and written before the first timer. Raw: `benches/results-s43/sizesweep.log`.

| per-thread buffer | 1 thr | 2 thr | 4 thr | 8 thr | 14 thr |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 32 KB | 1997.3 (1999) | 3197.1 (3198) | 3925.5 (3924) | 4092.4 (3965) | 4102.0 (4163) |
| 128 KB | 2002.8 (2005) | 3195.1 (3201) | 3932.5 (3922) | 3867.2 (3864) | 4156.9 (4158) |
| 256 KB | 1999.7 (1999) | 3199.0 (3201) | 3931.8 (3918) | 3858.2 (4073) | 4046.8 (3834) |
| 512 KB | 1768.9 (1773) | 2902.1 (2906) | 3569.3 (3568) | 3776.5 (3774) | 3722.6 (3512) |
| 1024 KB | 1581.4 (1582) | 2633.5 (2645) | 3247.1 (3250) | 3382.2 (3241) | 3360.6 (3197) |
| 4096 KB | 1461.6 (1465) | 2477.8 (2481) | 2804.1 (3049) | 2896.1 (3016) | 2206.6 (2921) |

**The `ld0` control is flat for the first three rows and droops with `ld4` for the last three.**
Rep-outer removed the drift across reps but NOT within a rep: every rep still walks the sizes in the
same order, so a monotone within-rep droop survives best-of-3 untouched. The absolute columns are
therefore still not quotable below 256 KB.

**But `ld0` is measured back-to-back with `ld4` inside each cell, so it is a per-cell clock
reference, and the RATIO cancels the drift.** That is the trustworthy readout:

| per-thread buffer | aggregate at 14 thr | 1 thr | 2 thr | 4 thr | 8 thr | 14 thr |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 32 KB | 448 KB | 0.999 | 1.000 | 1.000 | 1.032 | 0.985 |
| 128 KB | 1.75 MB | 0.999 | 0.998 | 1.003 | 1.001 | 1.000 |
| 256 KB | 3.5 MB | 1.000 | 0.999 | 1.004 | 0.947 | 1.056 |
| 512 KB | 7 MB | 0.998 | 0.999 | 1.000 | 1.001 | 1.060 |
| 1024 KB | 14 MB | 1.000 | 0.996 | 0.999 | 1.043 | 1.051 |
| 4096 KB | 56 MB | 0.998 | 0.999 | 0.920 | 0.960 | **0.755** |

### REFUTED — shared-L2 refill bandwidth does not bind either

`ld4/ld0` is **1.00 ± 0.06 in every cell whose aggregate working set fits the 16 MB L2**, including
14 threads each streaming a **1 MB per-thread buffer** — 14 MB aggregate, every load missing the
128 KB L1D and served by the shared L2, which is *exactly* the regime the real kernel runs in
(~1 MB swept per `mapal_sme_panel` call with no reuse inside the call). Four loads per four `fmopa`
cost **nothing** there. The only cell where loads cost anything is 4 MB × 14 = 56 MB, past L2
(0.755×), and at 1–2 threads even that is free because one core cannot saturate its own L1 refill.

⇒ **The load path is free at L1, free at L2, and only expensive past L2 — and the real kernel's
operands are in L2 threaded.** Combined with Probe 1 (loads do not compete for unit issue) and
Probe 2 (making the whole problem L2-resident buys nothing), **the entire operand-feeding story is
now refuted at every level of the hierarchy for the threaded case.** The kernel's instruction mix
can demonstrably run at ~4100 GF/s from where its operands actually live.

The binder is therefore inside the kernel or inside the dispatch, and Probe 2's flat N-sweep already
bounds every `O(N²)`-shaped term — the A pack, the ZA read-out, `smstart`/`zero za` per call — to a
few percent, because those all shrink as `1/N` and N=4096 is *not* faster than N=2048.

---

## PROBE 4 — `MAPAL_SLICE` sweep, N=4096 threaded  (RUNNING)

The one candidate the N-sweep does not bound: **slice quantization and the makespan tail.** The
`ti·t·c` quantum gives 128 panel-aligned blocks at N=4096; `oversub = 4` cuts them into **64 slices
for 14 lanes**, so the makespan is `ceil(64/14) = 5` slice-times against an ideal 4.571 — a fixed
**9.4%** tax that is the SAME at N=2048 (also 64 slices) and therefore invisible to Probe 2.
Four E-cores that retire SME far slower than a P-core widen it further, and the "excluding E-cores
costs 1.7%" datum does not test this: `MAPAL_PAR=10` only makes ten *lanes*, it does not stop the
scheduler putting them on E-cores.

`MAPAL_SLICE` forces the slice size (rounded up to the quantum) and sets `oversub = MAX`, so it is a
free lever on the slice count with no rebuild: 131072 → 128 slices, 262144 → 64 (identity control,
must overlap the default), 524288 → 32, 1048576 → 16. 9 alternating cycles, two discarded warm-up
passes. Raw: `benches/results-s43/slicesweep.log`.

| `MAPAL_SLICE` | slices | min ms | **median ms** | max ms | vs default |
| --- | ---: | ---: | ---: | ---: | ---: |
| (unset, ships) | 64 | 53.395 | **53.998** | 54.667 | 1.000× |
| 131072 | 128 | 52.689 | **53.528** | 54.344 | 1.009× |
| 262144 | 64 | 53.337 | **54.058** | 55.127 | 0.999× |
| 524288 | 32 | 55.381 | **55.966** | 56.707 | 0.965× |
| 1048576 | 16 | 55.044 | **57.517** | 60.059 | 0.939× |

**The identity control works**: `MAPAL_SLICE=262144` reproduces the default's 64 slices and lands
54.058 against 53.998 — 0.1% apart, distributions fully overlap. The harness is sound.

### REFUTED — slice quantization and the makespan tail are worth ≤1%

**Doubling the slice count 64 → 128 buys 0.9%** (53.998 → 53.528 ms), and the distributions overlap
heavily (128-slice range 52.689–54.344 against the default's 53.395–54.667). The modelled
quantization gain from `ceil(64/14)/4.571` = 91.4% to `ceil(128/14)/9.14` = 94% efficiency was
~2.8%; the measured gain is a third of that and inside noise. Going *coarser* does hurt, monotonically
and disjointly (32 slices −3.6%, 16 slices −6.5%), which confirms the lever is real and connected —
it simply has nothing left to give above 64 slices.

⇒ **Pool dispatch, slice quantization and load imbalance (including P/E heterogeneity, which
stealing absorbs) together cost ≤1% at the shipped configuration.** The entire span from 16 to 128
slices is 7%. This is not where 38% went.

---

## Two more refutations that fall straight out of Probe 2, for free

### The TLB wall and the shared-L2 capacity wall both cost ~0 THREADED

`docs/performance/s43-residency-and-the-thermal-artifact.md` §4b measures a TLB reach of ~2k–4k
pages (`hw.pagesize` = 16384) and §4 a shared-L2 knee at 8–12 MB. The per-i-step B footprint is what
sets both, because all of B is re-swept on every i-step. Probe 2's two large sizes straddle them:

| N | B footprint / i-step | pages | inside 16 MB L2? | inside ~2k–4k page TLB reach? | **threaded GF/s** |
| ---: | ---: | ---: | --- | --- | ---: |
| 2048 | 16 MB | 1024 | borderline (yes with A) | **yes** | **2576** |
| 4096 | 64 MB | 4096 | **no** | **no** | **2537** |

**1.5% apart, distributions overlapping.** N=2048 sits inside both walls and N=4096 outside both, and
it makes no difference. ⇒ Threaded, neither cache capacity nor translation reach binds. This is the
same conclusion `kc`, the residency window, and `nc` each reached independently — four instruments,
one answer.

### The A pack, the ZA read-out and per-call overhead are ≈0% threaded

All three scale as `N²` (elements packed = `N²`; output elements read out and stored = `N²`; panel
calls = `N²/1024`) against an `N³` k loop. Fit `T = a·N³ + b·N²` to the two clean threaded medians:

```
T(4096)/T(2048) = 54.164 / 6.670 = 8.12      pure N^3 predicts 8.00
(8 + 4x)/(1 + x) = 8.12   =>   x = -0.03
```

where `x` is the `N²` share of threaded time at N=2048. **The fit puts it at −3%, i.e. zero to within
noise** — and it is *negative*, meaning N=4096 is if anything slightly worse than pure `N³`, which no
`N²` overhead can produce. ⇒ **pack + read-out + `smstart`/`zero za` + per-call overhead together are
≈0% of threaded time.** At one thread the pack is a real 8.46 ms of 174.6 (4.9%); threaded it
disappears into other threads' `fmopa`, which is exactly what a shared unit with 14 clients should do.

## Where that leaves it, before Probe 5

Refuted threaded, each with its own measurement: unit-issue contention from loads (Probe 1) · operand
residency at L1 (Probe 1, Probe 3, S43 window) · operand residency at L2 (Probe 3) · shared-L2
capacity (Probe 2) · TLB reach (Probe 2) · slice quantization, dispatch, P/E imbalance (Probe 4) ·
A pack, ZA read-out, per-call overhead (Probe 2 fit).

**Nothing outside the k loop survives.** And the k loop is 13 instructions — 4 vector loads and
4 `fmopa` — which is *byte-for-byte the instruction mix* of `unitload.c`'s `ld4` (also 13
instructions, assembly-verified), and that mix runs at **4183.8 GF/s at 14 threads** from L1 and at
**ratio 1.00 against the zero-load control** from a 14 MB L2-resident footprint. The emitted k loop's
per-iteration instruction count is independently confirmed at 13 by
`s43-residency-and-the-thermal-artifact.md` §1 ("Treatment arms are *longer*: 13 / 15 / 16").

So the same instruction stream, fed from the same level of the hierarchy, runs 1.65× faster in a
standalone probe than inside the shipped path. **Probe 5 is the split that localizes that.**

---

## PROBE 5 — the real emitted kernel, driven standalone and threaded  (RUNNING)

`benches/sme/paneldrive.c`. `@mapal_sme_panel` is lifted **verbatim** out of the emitted module
(`target/tmp/s43bind/m4096.ll` → `panel.ll`, linkage changed from `internal` to external and nothing
else), and called with the emitter's own arguments — `bn = 16`, `bj = 65536`, `cn = 4096`,
`K = 4096` — from N threads over a dynamically stolen i-panel counter, with `mapal-rt` and the
whole runtime removed.

| arm | what it runs | what it prices |
| --- | --- | --- |
| **full** | A pack + 128 panel calls per i-panel | must reproduce ~54.164 ms at 14 threads, or the driver is not modelling the shipped path and nothing in it is readable |
| **kernel** | identical, A pack skipped (values wrong by construction, timing only) | the pack's threaded share, directly |

Value gate before any timing: 97 spread cells against an independent scalar `fmaf` reference built
from the packed-B layout `[jt][k][lane]`, so the gate proves `A·B` rather than self-agreement.
9 alternating cycles, rep-outer/arm-inner, 1 s warm-up, thread counts 1/2/4/8/14.

### RESULT — the kernel is fine. The shipped path is 1.50× slower than the same kernel.

`value gate: 0/97 cells differ against a scalar fmaf reference`. Raw:
`benches/results-s43/paneldrive.log`.

| threads | arm | min ms | **median ms** | max ms | GF/s | slowest thr | fastest thr |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | full | 155.553 | **157.096** | 162.657 | 874.9 | 162.6 | 155.5 |
| 1 | kernel | 144.757 | **146.068** | 153.014 | 940.9 | 153.0 | 144.7 |
| 2 | full | 82.961 | **83.052** | 84.531 | 1654.9 | 84.5 | 82.9 |
| 2 | kernel | 77.722 | **77.979** | 78.436 | 1762.5 | 78.4 | 77.7 |
| 4 | full | 50.939 | **53.048** | 55.236 | 2590.8 | 55.2 | 49.2 |
| 4 | kernel | 49.705 | **50.171** | 53.449 | 2739.4 | 53.4 | 48.8 |
| 8 | full | 38.186 | **38.829** | 40.961 | 3539.6 | 40.9 | 36.4 |
| 8 | kernel | 36.644 | **37.391** | 38.510 | 3675.7 | 38.4 | 35.1 |
| 14 | full | 35.821 | **36.028** | 36.345 | **3814.8** | 36.3 | 32.8 |
| 14 | kernel | 34.715 | **35.306** | 36.981 | 3892.8 | 36.9 | 32.3 |

**At 14 threads the real emitted kernel, doing the real pack and the real 16384 panel calls with the
real arguments, runs the whole N=4096 GEMM in 36.028 ms = 3815 GF/s — 93% of the ~4100 two-unit
ceiling.** The shipped path does the identical work in 54.164 ms = 2537 GF/s.

**1.50×, disjoint by a mile** (driver max 36.345 against shipped min 53.395 — 17 ms of clear air).

Three things fall out at once:

1. **The k loop is not the problem and neither is memory.** The same kernel, on the same operands,
   in the same 64 MB packed-B layout, past the L2 knee and past TLB reach, reaches 3815 GF/s. Every
   memory hypothesis is now refuted by a *positive* result, not just by a null.
2. **The A pack's threaded share is 2.0%** (36.028 → 35.306 ms), measured directly rather than
   inferred — and 7.0% at one thread (157.096 → 146.068, i.e. 11.0 ms, consistent with
   `packcost.c`'s 8.46 ms). The `N²`-fit prediction of "≈0 threaded" is confirmed.
3. **Thread balance in the driver is near-perfect**: at 14 threads the slowest thread is 36.3 ms and
   the fastest 32.8 ms on a 36.0 ms wall, from a plain `atomic_fetch_add` over 128 i-panels.

### The deficit is ADDITIVE, not multiplicative — and that is the strongest clue

Shipped minus driver, `full` arm, same work:

| threads | shipped ms | driver `full` ms | difference |
| ---: | ---: | ---: | ---: |
| 1 | 174.596 | 157.096 | **17.5** |
| 2 | ~94.5 (1454 GF/s, S42 §5b) | 83.052 | ~11.5 |
| 4 | ~69.0 (1993 GF/s, S42 §5b) | 53.048 | ~15.9 |
| 14 | 54.164 | 36.028 | **18.1** |

**~18 ms at 1 thread and ~18 ms at 14 threads.** A multiplicative cost (a slower loop, contention, a
worse layout) shrinks with thread count; this does not move. That is the signature of a **serial,
thread-count-independent term inside the timed region** — something the shipped path does once, that
the driver does not do at all.

The candidates the driver removed, in order of size: the **B pack** (the packing rung transposes
64 MB into `[jt][k][lane]`; the driver is handed `bp` pre-packed), the **output array allocation and
first touch** (64 MB = 4096 pages of 16 KB; the driver `memset`s once *before* the timer), and the
`mapal_par_begin`/`task`/`launch`/`wait` dispatch itself.

**Probe 4 already exonerated dispatch** (≤1%), and Probe 2's `N³` fit says the term cannot be a clean
`N²` either — so the next measurement has to separate them by sweeping N in the driver and
subtracting against the shipped N-sweep I already hold.

---

## PROBE 6 — the serial B pack, read out of the emitted IR

Before measuring anything I read what the shipped timed region actually contains
(`target/tmp/s43bind/m4096.ll`). The suspect is not a guess — it is in the emission:

```
  call void @mapal_par_task(ptr %h, i32 7, i32 0, ptr @task7, i64 16777216, ...)
                                            ^^^^^ kind = 0 = Seq
```

**`@task7` is a SEQUENTIAL task.** It runs exactly once, on one thread, and its body is the whole B
pack; only when that finishes does it open a *nested* parallel run for the matmul:

```
bb5:
  %t36 = call ptr @mapal_par_begin(i32 1)
  call void @mapal_par_task(ptr %t36, i32 0, i32 1, ptr @task7_slice, i64 16777216, i32 16777218, i64 131072, i32 4, i32 0)
  call void @mapal_par_launch(ptr %t36, ptr %frame)
```

So the shipped timed region is **`serial B pack` + `parallel matmul`**, and the driver measures only
the second. That is exactly the additive, thread-count-independent shape observed.

### And the pack loop is a page-walk, by construction

From `@task7` bb3/bb6/bb9, the emitted nest is

```
for jt in 0..256:  for k in 0..4096:  for lane in 0..16:
    packed[jt*65536 + k*16 + lane] = (jt*16+lane < 4096) ? b[k*4096 + jt*16 + lane] : 0
```

For fixed `jt`, consecutive `k` are `4096 floats = 16384 B` apart — and `hw.pagesize` on this part is
**16384**. **Every single iteration of the k loop crosses to a new page**, it walks all 4096 pages of
`b`, and it does that all over again for each of the 256 `jt` panels: **1,048,576 page-crossing
accesses** against a measured TLB reach of ~2k–4k pages
(`s43-residency-and-the-thermal-artifact.md` §4b, which prices crossing that knee at **1.571×** with
zero capacity pressure). It also defeats the prefetcher completely — one 64 B line used per page
visited.

**This is why Probe 2's `N²` fit came out at −3% instead of positive.** The pack is not a clean `N²`
term: at N=2048 `b` spans 16 MB = **1024 pages, inside TLB reach**, so the pack is cheap; at N=4096 it
spans 64 MB = **4096 pages, past it**, so the pack is disproportionately expensive. A super-linear
term hiding inside an `N²` slot is exactly what drives the fitted `N²` share negative.

Probe 6 prices it directly: `benches/sme/paneldrive.c` now carries the emitter's pack loop verbatim
as its own timed `bpack` arm, swept over N = 1024 / 2048 / 4096 alongside `full` and `kernel` at 1
and 14 threads, with the value gate rebuilt against the **row-major** `b` so it proves the pack and
the kernel together rather than letting them agree with each other.

**Pre-declared prediction:** `bpack(4096)` ≈ 18 ms, matching the shipped-minus-driver gap at both 1
and 14 threads; and `bpack(2048)` ≪ 18/4 ms because 2048 sits inside the TLB knee. If `bpack(4096)`
comes in at a few ms, the serial-pack hypothesis is dead and the ~18 ms is dispatch or output
first-touch instead.

### RESULT — the prediction lands. `bpack(4096)` = 16.349 ms.

9 alternating cycles per cell, rep-outer/arm-inner, 800 ms warm-up per size, value gate against the
row-major `b` passing 0/97 at every size. Raw: `benches/results-s43/paneldrive-nsweep.log`.

| N | threads | arm | min ms | **median ms** | max ms | GF/s |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 1024 | 1 | **bpack** | 0.331 | **0.346** | 0.434 | — |
| 1024 | 1 | full | 1.940 | 1.972 | 1.992 | 1089.0 |
| 1024 | 1 | kernel | 1.299 | 1.302 | 1.308 | 1649.4 |
| 1024 | 14 | full | 0.692 | **0.698** | 0.703 | 3076.6 |
| 1024 | 14 | kernel | 0.631 | 0.636 | 0.648 | 3376.5 |
| 2048 | 1 | **bpack** | 2.417 | **2.429** | 2.463 | — |
| 2048 | 1 | full | 14.895 | 15.150 | 15.345 | 1134.0 |
| 2048 | 1 | kernel | 11.950 | 12.218 | 12.487 | 1406.1 |
| 2048 | 14 | full | 4.655 | **4.785** | 4.793 | 3590.4 |
| 2048 | 14 | kernel | 4.482 | 4.489 | 4.668 | 3827.1 |
| 4096 | 1 | **bpack** | 16.219 | **16.349** | 16.466 | — |
| 4096 | 1 | full | 153.527 | 154.396 | 158.854 | 890.2 |
| 4096 | 1 | kernel | 142.957 | 143.331 | 155.560 | 958.9 |
| 4096 | 14 | full | 35.273 | **35.875** | 36.527 | **3831.1** |
| 4096 | 14 | kernel | 34.772 | 34.964 | 35.669 | 3930.9 |

### The accounting closes, with one term, at both thread counts

| N=4096 | 1 thread | 14 threads |
| --- | ---: | ---: |
| serial B pack (`bpack`) | 16.349 | 16.349 |
| parallel matmul + A pack (`full`) | 154.396 | 35.875 |
| **sum** | **170.745** | **52.224** |
| **shipped, measured** | **174.596** | **54.164** |
| residual | 3.85 (2.2%) | 1.94 (3.6%) |

**One additive constant reproduces the shipped time at both ends of a 3.2× thread-count range, to
within 3.6% — under this machine's 6% noise floor.** The residual is the `mapal_par_begin`/`task`/
`launch`/`wait` dispatch, which Probe 4 independently priced at ≤1%.

### The pack's cost tracks PAGES, not bytes — a sweep, not one point

| N | b footprint | pages touched per `jt` pass | passes | **page-crossing accesses** | **bpack median ms** |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1024 | 4 MB | 256 | 64 | 16 K | 0.346 |
| 2048 | 16 MB | 1024 | 128 | 131 K | 2.429 |
| 4096 | 64 MB | 4096 | 256 | 1049 K | 16.349 |

Bytes grow **4×** per step. Page-crossing accesses grow **8×** per step. Measured time grows
**7.02×** then **6.73×**. ⇒ **the pack is bound by page visits, not by bytes** — it tracks the 8×
axis, not the 4× one. At N=4096 it moves 128 MB in 16.349 ms = **7.8 GB/s against a ~95 GB/s DRAM
floor**, a 12× shortfall, which is what a page-walk-per-access transpose costs.

---

# THE ANSWER

**What binds the threaded SME matmul at 2531 GF/s is a serial, page-walk-bound B pack that runs on
one thread inside the timed region while the other 13 lanes idle.** It is **16.349 ms of a 54.164 ms
wall — 30.2%.**

The parallel part is *not* the problem and never was: the real emitted kernel, doing the real work on
the real operands, runs the N=4096 GEMM in **35.875 ms = 3831 GF/s, which is 93% of the measured
~4100 GF/s two-unit ceiling** — and would beat Accelerate's threaded 3113 GF/s outright.

**Why every previous instrument missed it.** `kc`, `nc`, the residency window and the lane cap all
modify the *matmul* loop. None of them touches `@task7`. And because the pack is serial its cost is
thread-count-independent, so it presents as "the parallel part stops scaling" — Amdahl's law wearing
a memory-bandwidth costume. That is also the honest explanation of the pattern that opened this
investigation: **three separate memory optimizations, each large at 1 thread and each worth nothing
threaded.** At 1 thread the pack is 9.4% of 174.6 ms and the matmul dominates, so matmul-side
optimizations pay. Threaded the matmul collapses to 35.9 ms and the *pack* becomes 30% of the wall,
so matmul-side optimizations have almost nothing left to win.

**The arithmetic tension in the brief is resolved.** At 2531 GF/s and 8 FLOP/byte the kernel appeared
to demand 316 GB/s. It does not — the real matmul phase runs at 3831 GF/s for 35.875 ms and demands
478 GB/s of *L1/L2* traffic, which Probes 1 and 3 show this part serves free; and 30% of the wall
clock is not doing `fmopa` at all.

---

## PROBE 7 — pricing the fix (`benches/sme/bpack.c`)

Two independent defects in `@task7`, separated by four arms that all produce **byte-identical**
output (checked over the full 64 MB before any timing): `base` = the emitted loop, serial;
`par` = the emitted loop, parallel over `jt` (256 independent panels); `blk` = k-blocked at 512 rows,
serial; `par+blk` = both. 15 alternating reps, rep-outer/arm-inner, 500 ms warm-up.
Raw: `benches/results-s43/bpack.log`.

| N | arm | min ms | **median ms** | max ms | GB/s | vs base |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 2048 | base (emitted) | 1.104 | **1.726** | 1.981 | 19.4 | 1.000× |
| 2048 | **par** | 0.295 | **0.346** | 0.431 | 97.0 | **4.99×** |
| 2048 | blk | 0.988 | 1.043 | 1.185 | 32.2 | 1.66× |
| 2048 | par+blk | 0.299 | 0.340 | 0.426 | 98.7 | 5.08× |
| 4096 | base (emitted) | 15.722 | **16.402** | 16.684 | 8.2 | 1.000× |
| 4096 | **par** | 1.694 | **1.721** | 2.370 | 78.0 | **9.53×** |
| 4096 | blk | 8.098 | 8.388 | 8.521 | 16.0 | 1.96× |
| 4096 | par+blk | 1.702 | 1.741 | 2.337 | 77.1 | 9.42× |

`base` at N=4096 reads **16.402 ms** here against Probe 6's **16.349 ms** in a different binary —
0.3% apart, an independent replication.

**Parallelising alone recovers 14.68 of the 16.40 ms — 9.53×, disjoint** (par max 2.370 against base
min 15.722). k-blocking alone is worth 1.96× serially, which independently confirms the page-order
diagnosis; but **on top of parallelism it adds nothing** (1.741 vs 1.721, overlapping), because once
the panels are spread over 14 cores each core has its own TLB and the aggregate lands at 78 GB/s
against the part's ~95 GB/s DRAM floor. At that point the transpose is DRAM-bound, which is the
correct place for a 128 MB transpose to be.

⇒ **The minimal fix is the whole fix: emit `@task7` as a parallel task over `jt` instead of
`kind = 0`.** No loop-order change is needed.

### Projected, and what it is worth

| N=4096, 14 threads | ms | GF/s |
| --- | ---: | ---: |
| shipped today | 54.164 | 2537 |
| serial pack replaced by the parallel one (54.164 − 16.349 + 1.721) | **39.54** | **3476** |
| Accelerate, threaded | — | 3113 |
| driver accounting without the dispatch residual (35.875 + 1.721) | 37.60 | 3655 |

**≈1.37×, and it would put the rung past Accelerate threaded for the first time.**

**This is a PRICE, not a settle** (rule 4 — `kc.c` predicted 1.448× standalone and delivered
+6.1%/−25.5% integrated). The arms above run the emitter's exact loop on the exact data, and Probe 6
already showed the serial term accounts for the shipped time to within 3.6% at two thread counts —
but the emitter change itself is not built and the integrated A/B is not run. That is the next
session's first job, and it is now a well-posed one-line-shaped change rather than a search.

---

## Raw output

- `benches/results-s43/unitload.log` — Probe 1
- `benches/results-s43/nsweep.log` — Probe 2
