# S43 — operand residency verified, and the ceiling it was measured against was a thermal artifact

Date: 2026-07-31 · Machine: **Apple M4 Pro**, 10 P + 4 E, 2 SME units.
`hw.pagesize` = 16384 · L1D 128 KB · L1I 192 KB · per-core L2 slice ~3.2 MB · **shared L2 16 MB**.
Baseline commit `0518e76`, clean. Plan: `components/backend-llvm/plans/plan-s43-operand-residency-verification.md`.
Instruments: `benches/sme/{winmask.py,resid_ab.sh,loadlevel.c}` · mutex `benches/perflock.sh`.

S43 opened on the S42 P0 — **the hierarchical tile→cache mapping** — with `next-session.md` §1's own
instruction to verify the diagnosis in the emitted kernel before writing any of it. That instruction
paid. The diagnosis is **confirmed at one thread and re-sized everywhere else**, and the table it was
sized against turned out to be a measurement artifact.

## 0. The headline

| | verdict |
| --- | --- |
| operands miss L1 in the emitted kernel, and it costs | **CONFIRMED — 1.71× at 1 thread**, 174.596 → 101.854 ms, disjoint, assembly-verified |
| which stream carries it | **B, essentially all of it.** A's term is ≤1.2% and not separable |
| the mechanism | **CONFOUNDED** — cache reach vs TLB reach; no arm in this design separates them |
| what it is worth **threaded** | **≤5%** (54.291 → 51.788 ms) — below this machine's 6% noise floor |
| S42's `1864` L1 ceiling | **RETRACTED — a thermal artifact.** The same binary reads ~2000 today |
| S42's `1043 GF/s` "emitted kernel today" | **wrong cell** — N=4096 is **803 GF/s**; 1043 is ~N=1024 |
| "1.79× would pass Accelerate" | **false** — perfect residency reaches 1349, Accelerate is 1655 |
| L1-vs-L2 price on this part | **zero.** Flat ~1990 GF/s from a 32 KB buffer to an 8 MB one |
| the one cache boundary that costs anything | **shared L2 → DRAM**, 8–12 MB knee, ~95 GB/s → ~765 GF/s |
| there is also a **TLB wall** | ~2k–4k pages; at constant bytes, crossing it costs **1.571×** (§4b) |
| `nc` blocking, built and swept | **ships OFF** — threaded parity at best, every working-set-shrinking arm loses disjointly |
| **what actually binds THREADED** | **a serial, single-thread B pack — 16.349 ms of the 54.164 ms wall, 30.2%** (§4c) |
| the emitted matmul kernel itself | **already 93% of the two-unit ceiling** (3831 of ~4100 GF/s) — it was never the problem |
| the fix, priced not built | parallelize `@task7` over `jt`: 16.402 → 1.721 ms; projected **3476 GF/s, past Accelerate's 3113** |

**The direction survives; the prescription was aimed one level off.** A cascade of "registers → L1 →
L2 → L3, each residing a bigger tile" buys L1 residency, and this part does not pay for L1 residency.
The only boundary with a price is the ~8–12 MB knee ⇒ **one blocking level (`nc` over B), not four.**

## 1. The instrument — an operand window patched into the emitted `.ll`

`kc` cannot answer "what is residency worth" because it is a **confounded instrument**: it buys
residency *and* pays `k/kc` extra output read-modify-write sweeps (the kernel `Store`s its ZA tiles
rather than accumulating). That is why the S42 depth sweep is unreadable as a residency measurement,
and why its optimum sits at 2× L1D rather than at L1D.

`winmask.py` masks the two k-derived operand offsets inside `@mapal_sme_panel` — `%aoff` (A panel)
and `%boff` (B panel) — so the four loads wrap inside a window of chosen size. Same instruction
sequence, same `fmopa` count, same pack, same ZA read-out, same output stores. **It buys residency
and pays nothing.** Values are wrong by construction; only timing is read. It is a text patch on the
emitted `.ll`, applied before `clang` — **not** an `EmitOpts` flag, because the repo is already
carrying a P1 to delete `kc_nest`, a measurement lever that outlived its welcome.

### What was verified in the assembly, not asserted

Whole-`.s` diff, control vs arm 3, is **confined to the k-loop preheader and body**; the read-out
block is byte-identical. Trip count identical (4096 iterations in both). No hoist, no CSE, no
vectorizer change. The masked stream's base pointer stops incrementing and an index register walks
instead (`and x13, x10, #0xff0` / `add x14, x1, x13, lsl #2`) — **address arithmetic only.**
Treatment arms are *longer*: 13 / 15 / 16 instructions per iteration for arms 1 / 3 / 4.

*A load hoisted out of the k loop would have produced this exact result for the wrong reason. It did
not happen, and that was checked in the assembly rather than in the patcher.*

## 2. The result — N=4096, 1 thread, 21 round-robin cycles

| arm | window | min | median | max | GF/s | vs control |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | shipped, unpatched | 172.231 | 174.779 | 178.266 | 786.4 | 0.999× overlaps |
| 1 | **control** (mask 2⁴⁴−1) | 172.118 | **174.596** | 177.898 | **787.2** | — |
| 2 | A → 16 KB | 169.086 | 172.608 | 195.950 | 796.3 | 1.012× **overlaps** |
| 3 | B → 2×16 KB | 100.948 | **101.764** | 107.414 | 1350.6 | **1.716× disjoint** |
| 4 | both → 48 KB total | 100.952 | **101.854** | 107.781 | 1349.4 | **1.714× disjoint** |
| 5 | both → 384 KB total | 126.423 | **129.782** | 146.921 | 1059.0 | **1.345× disjoint** |

The plan's threshold — **declared before the first timed run** — was ≤128 ms with distributions
disjoint from control. Arm 4 median 101.854, max 107.781 < control min 172.118. **64.7 ms of clear
air, ≈35σ. CONFIRMED.**

**The strongest thing in the run is not the magnitude, it is the asymmetry.** arm2 ≈ 0 / arm3 ≈ huge
was *predicted by the reuse-distance audit before the run*, not fitted after it:

| stream | footprint | reuse distance | predicted residence |
| --- | ---: | ---: | --- |
| `ap` (A panel) | 512 KB | 1 MB (one panel call) | L2 — never L1 |
| `b`-panel, 2 streams | 512 KB | **64 MB** (all 128 panels, one i-step) | **DRAM** |

### Replications

**N=4096, full thread width:** control 54.291 ms (2531 GF/s) → arm 4 51.788 (2654) = **1.048×**.
Permutation p=5e-05, so it is resolved *within this run* — but 4.8% is under the standing 6%
cross-build floor (S39 measured −5.9%…+1.2% between **byte-identical binaries** on this machine).
**Report it as a bound, not a number.**

**N=1024, 1 thread:** every arm overlaps control (2.085–2.105 ms). Arm 3's win does not shrink, it
**vanishes** — B fits shared L2 at that size, which is the design's own built-in consistency check,
and it passes. (Arm 5 was correctly **VOID** here: both masks fold at that size, asm confirms 0
`and`.)

## 3. The ceiling was a thermal artifact

`loadcost.c`'s **own unmodified binary**, re-run three times:

| 32 KB buffer | 0 loads | 1 | 2 | 3 | 4 loads |
| --- | ---: | ---: | ---: | ---: | ---: |
| published (S42 §5e) | 1956.7 | 1913.7 | 1928.9 | 1910.0 | **1864.2** |
| today, run 1 | 2005.9 | 2003.9 | 2005.4 | 2004.7 | **2004.2** |
| today, runs 2 / 3 | 2002.7 / 1999.6 | – | – | – | **2000.4 / 1996.1** |

The 64 MB row still reproduces (752.4 / 746.4 / 760.7 against 760.8). **Only the L1 row fails**, and
today it is dead flat at the zero-load roofline — L1-resident loads cost **nothing**, not 4.7%.

The published row's monotone slide 1956.7 → 1864.2, and its second-pass roofline of 1915.5 against
today's 2005, are the signature of a machine drifting downward **during** the run. That is a rule-14
failure inside the data rule 14 was written from. **Anything quoting 1864 as the L1 ceiling is
quoting thermal drift.**

## 4. There is no L1 cliff, and no L2-slice cliff

`benches/sme/loadlevel.c` — 60 M iterations/cell, 15 interleaved reps, 1 s warmup, drift 1.004×.
Two variants: (a) `loadcost`'s 4-load pattern; (b) the real kernel's stream shape (one A stream
advancing 128 B/iter with loads at +0/+64, two B streams at 64 B/iter displaced 256 KB apart). Both
issue 256 B of fresh operand line per iteration. **Assembly-verified: there is exactly ONE loop body
serving all 17 sizes** — buffer size enters only as the inner trip count — so a size-dependent
transformation is impossible by construction.

| buffer | 32K | 128K **=L1D** | 512K | 3M **=L2 slice** | 8M | 12M | 16M | 24M | 64M |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| (a) GF/s med | 1991 | **1996** | 1995 | **1991** | 1986 | 1750 | 959 | 771 | 752 |
| (b) GF/s med | 1993 | 1992 | 1995 | 1993 | 1985 | 1882 | 1723 | 1401 | 833 |
| GB/s med (a) | 249 | 249 | 249 | 249 | 248 | 219 | 120 | 96 | 94 |

- **128 KB (L1D): no cliff.** 1995.7 against 1990.9 at 32 KB.
- **3.2 MB (per-core L2 slice): no cliff.** One thread alone effectively gets the whole 16 MB.
- **The one cliff starts between 8 M and 12 M and completes by 24 M**, tracking the **16 MB shared
  L2** — arriving early because a single thread never has all 16 MB. 12 M is the noisiest cell in
  the sweep (1749.6 vs 1402.4 across runs) precisely because it sits on the knee.
- **The "L2 plateau" IS the L1 plateau — there is only one**, ~1990 GF/s / **249 GB/s**, unbroken
  from 32 KB to 8 MB. At 8 MB every operand line is a *guaranteed* L1 miss served by L2, and it
  costs nothing.
- **The floor is DRAM bandwidth, not a cache level:** ~95 GB/s → ~765 GF/s, flat 24 M → 64 M. The
  kernel needs 249 GB/s to hold roofline; DRAM gives 95, and throughput lands on that ratio (38%).
- Past L2 the two variants diverge (64 M: a 752, b 833) because (b)'s B streams trail the A stream
  through the same buffer at half its rate and re-hit lines A already pulled in. **Inside L2 they
  are identical to within noise.**

> **An operand living in L2 rather than L1 costs nothing on this part.** The price appears only when
> it falls out of the 16 MB shared L2 into DRAM.

**And that reframes the shipping kernel.** At 1 thread it runs **787 GF/s against a measured DRAM
floor of 765**. It is not L1-starved. It is DRAM-bandwidth-bound.

## 4b. The TLB wall is real, and it is a sharper wall than capacity

`benches/sme/tlbreach.c` holds **bytes touched constant** and varies **page span** 64×, by placing
each 256 B chunk at `j · 256 · M` for odd `M`. Odd is load-bearing: a power-of-two stride puts every
chunk in the same cache set and measures conflict misses — the classic trap that would have produced
a confident wrong answer. Every cell runs **one bit-identical loop body** (`n`, `reps`, `stride`,
`rev` are runtime arguments; `clang -S` confirms 4 `ld1w` + 4 `fmopa` + 1 `rbit` + 1 `csel`, and the
compiler did *not* unswitch on `rev`). Calibration gate: (32 KB, M=1, seq) = **1999.2 GF/s** against
`loadlevel.c`'s 1997 — **+0.1%**, so the index arithmetic costs nothing and the sweep measures what
it claims to.

Two visit orders, because raising `M` destroys spatial locality as well as page locality: `seq`, and
`rev` (bit-reversed index order, maximally prefetch-hostile at *every* M, so it holds prefetch
hostility constant and moves only the page count).

### Pre-faulting — the validity condition this whole section rests on (Sapir's catch)

**A page FAULT is a kernel trap costing microseconds; a TLB MISS with a valid PTE is a page-table
walk costing tens of cycles.** Only the second is the quantity under test, and a single fault inside
a timed region would swamp it — a sweep that faults as it grows would manufacture exactly the knee
this section reports. Verified in the source, not assumed from the design:

- `tlbreach.c` **writes** 64 floats into every chunk of every live cell before timing (writes, not
  reads, so zero-page and copy-on-write mappings are resolved), then warms 1000 ms, then takes **one
  untimed visit per cell** so no cell pays another's first-touch. Timed reps come after all three.
- `loadlevel.c` writes **every float of the full 64 MB buffer** before timing, then warms.

So every page any timed walk touches is already resident and mapped, and the page tables are already
populated. What remains free to vary — deliberately — is whether the *translation* for that page is
still in the TLB. That is the measurement.

### The pre-declared control fires, and it selects the trustworthy order

The design declared before the run: at `M ≥ 64` every chunk owns its page, so **M = 65/129/257 hold
bytes AND pages constant and vary only span** — if they differ, a span effect is live and the reading
is suspect. At 1 MB / 4096 pages:

| order | M=65 | M=129 | M=257 | |
| --- | ---: | ---: | ---: | --- |
| `seq` | **364.6** | **564.5** | **800.5** | 2.2× spread — **SUSPECT, do not read the seq column** |
| `rev` | 1118.7 | 1154.0 | 1121.9 | within 3% — **clean** |

The `seq` order at large strides is contaminated (16640 B = one 16 KB page + 256 B — a DRAM
row/bank-conflict pattern, and *larger* spans are *faster*, which no capacity effect does). **Every
number below is `rev`.** Carrying both orders is what caught this.

### Constant bytes, rising pages — the separation

| pages | 1 MB row | | pages | 4 MB row |
| ---: | ---: | --- | ---: | ---: |
| 64 | 1949.1 | | 256 | 1683.3 |
| 192 | 1938.4 | | 768 | 1676.8 |
| 320 | 1751.2 | | 1280 | 1750.5 |
| 576 | 1733.9 | | 2304 | **1766.5** |
| 1088 | 1730.3 | | 4352 | **1235.0** |
| 2112 | **1761.9** | | 8448 | 1098.6 |
| 4096 | **1121.9** | | 16384 | 1080.6 |

**A knee between 2112 and 4096 pages, reproduced independently at 4 MB between 2304 and 4352.**
⇒ **TLB reach on this part is ~2k–4k pages.**

### Pages beat bytes, and it is not close

| axis | measurement | cost |
| --- | --- | ---: |
| **bytes**, at ~constant ~1000 pages | 256 KB (1784.7) → 1 MB (1730.3) → 4 MB (1676.8) | **−6% over 16×** |
| **pages**, at constant 1 MB | 2112 pg (1761.9) → 4096 pg (1121.9) | **−36%, a 1.571× penalty** |

**With zero capacity pressure — 1 MB of bytes touched, comfortably L2-resident — crossing the page
knee alone costs 1.571×.** The kernel's whole measured effect is 1.71×.

### It maps onto the kernel arms, and it dissolves §5's contradiction

The window is **per panel call**; what sets residency is the **per-i-step footprint**, because all of
B is re-swept on each of the 128 i-steps. Re-mapped (128 j-panels × 2 streams × window):

| arm | B footprint / i-step | pages | `loadlevel` (b) at that footprint | kernel measured |
| --- | ---: | ---: | ---: | ---: |
| 4 | 4 MB | 256 | ~1990 | 1349 |
| 5 | 32 MB | 2048 | ~1004 | 1059 |
| control | 64 MB | 4096 | 833 | **803** |

**Monotone-consistent, and the two ends nearly coincide.** The apparent probe/kernel contradiction in
§5 was an artifact of comparing the per-*call* window (48 KB / 384 KB) against the probe's
working-*set* axis. The residual gap at arm 4 is the fixed overhead (pack ≈ 8 ms + ZA read-out +
stores), which matters proportionally more the faster the loop runs. **Contradiction resolved.**

And the kernel arms **straddle both walls at once**: control is past shared-L2 capacity (64 MB > 16 MB)
*and* past TLB reach (4096 pages); arm 4 is inside both (4 MB, 256 pages); arm 5 sits past capacity
and exactly at the page knee. So the in-kernel 1.71× still cannot be split between the two mechanisms
— but both are now measured, and **translation alone is large enough to account for most of it.**

**An in-kernel bytes-vs-pages arm was refuted on arithmetic before it was built** (a real result, and
the cheapest kind): any offset transform must preserve `inbounds`, so it is bounded by the real max
offsets — B `≤ 65520` floats = 256 KB = **≤16 pages**, A `≤ 131040` floats = 512 KB = **≤32 pages**.
Both are inside any L1 DTLB, so the in-kernel instrument **cannot reach a TLB effect at all.** That is
why the separation had to be standalone.

## 4c. THE FINDING — what actually binds threaded is a **serial B pack**, and it is Amdahl in a memory-bandwidth costume

Everything above optimizes the **matmul loop**. Threaded, the matmul loop is not the problem.

**The real emitted `@mapal_sme_panel`, lifted verbatim and driven without `mapal-rt`, runs the whole
N=4096 GEMM in 35.875 ms = 3831 GF/s — 93% of the measured ~4100 GF/s two-unit ceiling.** The kernel
this project has spent three sessions optimizing is already nearly optimal.

What is not: **the B pack runs on ONE thread, inside the timed region, while the other 13 lanes idle.**
It is in the emission, not inferred — verified independently in a fresh emit of
`matmul4096_cap_f32.mapal`:

```
call void @mapal_par_task(ptr %h, i32 7, i32 0, ptr @task7, ...)
                                          ^^^^^^  kind = 0 = Seq
```

`mapal-rt/src/lib.rs:832` — *"`kind == 0` executes exactly one `f(0, n, frame)` call."* And `@task7`
contains **0 `fmopa`, 8 stores, and 1 nested parallel call**: it packs all of B on one thread, then
opens a *nested* parallel run for the matmul.

### The accounting closes, additively, at both ends of the thread range

| N=4096 | 1 thread | 14 threads |
| --- | ---: | ---: |
| serial B pack | 16.349 | **16.349** |
| parallel matmul + A pack | 154.396 | 35.875 |
| **sum** | **170.745** | **52.224** |
| **shipped, measured** | 174.596 | 54.164 |
| residual | 2.2% | 3.6% |

A one-term additive model that closes to 3.6% at *both* ends, with the serial term
**thread-count-independent by construction**, is the strongest structural evidence in S43.

⇒ **30.2% of the threaded wall is one thread packing B.**

### This explains the pattern the whole session was circling

At 1 thread the pack is 9.4% of the wall and the matmul dominates, so matmul optimizations show up
large. Threaded, the matmul collapses 154.4 → 35.9 ms and the pack becomes **30%** — so the same
optimizations vanish. That is the honest explanation of the three-for-three result:

| optimization | 1 thread | threaded | why |
| --- | ---: | ---: | --- |
| `kc` blocking | +6.1% | −25.5% | all three modify the **matmul loop** |
| operand residency | +71% | +5% | none of them touches `@task7` |
| `nc` blocking | +18.7% | parity | so threaded they optimize 66% of the wall, badly |

**It presents as "the parallel part stops scaling", which reads as a memory wall.** It is Amdahl.

### Every alternative was refuted with its own measurement

| candidate | measurement | verdict |
| --- | --- | --- |
| loads compete with `fmopa` for shared-unit issue | 14 threads, 4 loads per 4 `fmopa`, L1-resident: **4183.8** vs 4159.6 GF/s at zero loads | **1.006× — refuted.** The 4100 ceiling is fully reachable at the kernel's exact instruction mix |
| operand residency / L2 capacity / TLB reach, *integrated* | N=2048 (inside both walls) 2576 vs N=4096 (outside both) 2537 GF/s | **1.5% — refuted threaded** |
| shared-L2 refill bandwidth | 14 threads × 1 MB each (14 MB, every load missing L1), ld4/ld0 | **1.00 — refuted** |
| slice quantization / dispatch / P-E imbalance | 64 → 128 slices | **0.9%, overlapping — refuted** |
| A pack, ZA read-out, per-call overhead | measured directly, threaded | **2.0%** |

### The fix, PRICED not settled (rule 3)

Emitting `@task7` parallel over `jt`: **16.402 → 1.721 ms, 9.53×, disjoint.** k-blocking the pack is
worth 1.96× serially but adds nothing on top of parallelism — spread over 14 cores the transpose is
DRAM-bound at 78 GB/s.

**Projected shipped: 39.54 ms = 3476 GF/s ≈ 1.37×, which passes Accelerate's threaded 3113.**

That is a price, not a result: the emitter change is not built. But unlike every prize S42 and S43
have chased, **this one exists at the thread count that ships.**

## 4d. BUILT — the parallel B pack. **1.381× shipped, and it passes Accelerate.**

`plan-s43-parallel-bpack.md` (Fable, written before the code, reconciled after). Three files,
+139/−6. `mapal-ir` and `mapal-rt` untouched.

- **`func/core.rs::emit_pack_copy`** — the `jt` loop's init/bound become `self.bulk_bounds(tiles)`.
  Two lines; at `split_range == false` it reproduces the old literals character-identically.
- **`func/drive.rs::emit_task`** (packed branch) — a third emitted function
  `@task{id}_pack(i64 %lo, i64 %hi, ptr %frame)`; the wrapper drops the inline pack and gains a
  nested `begin(1)/task/launch/finish` ahead of the unchanged matmul dispatch.

**Design change from the sketch, and the reason matters.** The proposed `begin(2)` + `mapal_par_dep`
was **cut**: `complete_slice` schedules dep-unlocked tasks `Placement::Local(lane)`, which would put
every matmul slice on one deque instead of the rank-sorted `Placement::Seed` it gets today — the dep
edge would have silently changed the *matmul's* placement. Two sequential `begin(1)` runs keep the
matmul dispatch byte-identical. **Run-once is preserved: the outer registration is still `kind = 0`.**

| N=4096 | off | on (shipped) | |
| --- | ---: | ---: | --- |
| SME threaded | **53.98** | **39.08** | **1.381×, disjoint** (ctl drift 0.11%) |
| SME 1 thread | 171.4 | 171.7 | 0.998×, overlap — **no cost** |
| SME 2048 threaded | 6.899 | 5.157 | 1.338×, disjoint |
| NEON threaded | 152.1 | 131.9 | ~1.15× disjoint — **VOID** (control spread 6.5–8.5%) and **NOT PURSUED**: the NEON leg is not the matmul path, matmul ships SME (Sapir) |

**39.08 ms = 3517 GF/s, against Accelerate's threaded 3113.** The oversub sweep {1,2,4,8} is flat
(1.4%, all overlapping) ⇒ ship 4; nothing to derive.

**The plan's N=2048 prediction is refuted** (1.345× vs 4096's 1.401×) — not a failure of §4c's
accounting, which closes at both sizes, but of the N² model behind the prediction. The pack is
**page-visit-bound**, so its share *grows* with N.

### Gates, re-verified independently in the main checkout

| gate | result |
| --- | --- |
| value identity | identical every arm, every cell, before any timing (`74348 -302529` / `-1045 51275`) |
| emissions moved | **48 moved / 111 unmoved** — and **only matmul sources, only `rew`/`con`, never `raw`** (no tile site is recognised before `rewrite`, S37). 24 × 2 = 48, zero cells off the diagonal |
| `@mapal_sme_panel`, `@task{n}_slice` | byte-identical in `.ll` **and** in machine code (54/54, 71/71 instructions) |
| `cargo test --workspace --release` | 1031 → **1032** passed, 0 failed |

**There was no test covering the packed parallel wrapper at all** — a regression putting the pack
back on one thread would have failed nothing. One added, and verified to *fail* by injecting a
`slice_elems = 0` silent-nothing bug before reverting.

### The instrument that nearly fabricated this gate

`benches/emit_sweep_ab.sh` was `#!/bin/zsh` using `${=flags}`. **Run under bash it printed `bad
substitution` per line, passed NO flags, hashed the raw face 159 times, and exited 0** — a clean
"159/159 identical" on precisely the gate that had to detect a packing change. Caught only because
the count 48 was predicted in advance.

Hardened and re-run: shebang `bash` with plain word-splitting, and **a failed emission is now a hard
error** (it used to hash empty output, the same constant every time, so two broken runs "matched").
The fixed gate immediately found **3 of 159 cells have always failed** — `examples/vector.mapal` does
not parse (P0001/P0012/P0108) — so every "159/159" in this repo's history was **156 real cells plus
3 vacuous**. `nc` blocking was re-checked with the fixed instrument and is **genuinely identical**.

> **23. A gate that cannot fail is not a gate.** Verify the instrument reports a *failure* you
> injected before trusting the pass. Two independent silent-pass paths lived in this repo's
> byte-identity gate — a wrong shell and an unchecked empty output — and one of them was live.

## 4e. vs numpy — same session, interleaved, value-gated

The threaded numpy figure this project quotes was taken in S42 and carried across sessions. Rule 19
says re-take it. `benches/matmul/numpy_ab.sh` runs both legs **alternating in one session**, gates on
values first, and compares Mapal's **median** against numpy's **best-of-N** — a deliberate handicap
against Mapal. numpy's BLAS on this machine **is** Accelerate, so the two are one measurement path,
not two independent baselines.

15 alternating cycles per size, machine exclusive, f32, threaded, N=4096 with the parallel pack:

| N | Mapal med (GF/s) | numpy med (GF/s) | ratio | distributions | verdict |
| ---: | ---: | ---: | ---: | --- | --- |
| **4096** | 38.583 ms (**3562**) | 43.938 ms (3128) | **1.139×** | **DISJOINT** (40.562 max < 43.669 min) | **Mapal wins** |
| 2048 | 5.170 ms (3323) | 5.302 ms (3240) | 1.026× | **OVERLAP** | parity, not a win |
| 1024 | 0.814 ms (2637) | 0.672 ms (3193) | 0.826× | DISJOINT | **numpy wins** |

**Values identical on every leg**, checked before any timing: 4096 `74348 / -302529`, 2048
`-1045 / 51275`, 1024 `11107 / 91690`, each matching numpy's `c0`/`clast` exactly.

**The stale baseline turned out to be sound**: numpy re-measured today is 3128 GF/s against S42's
3113 — 0.5%. It is now verified rather than assumed, which is the whole of rule 19.

### The honest scope

> **Mapal beats numpy/Accelerate at N=4096, f32, threaded, on the M4 Pro, by 1.139× with disjoint
> distributions and identical values. At 2048 it is parity. At 1024 it loses 1.21×. At one thread it
> loses ~2× at every size.**

The size dependence is the mechanism showing through: the pack is **page-visit-bound**, so its share
of the wall grows with N, and fixing it therefore pays more the larger the problem. At 1024 the whole
kernel is 0.8 ms and B is 4 MB — cache-resident, little pack cost to recover, and pool dispatch is a
visible fraction. Nothing here contradicts §4c; it is §4c's prediction.

## 5. What is NOT claimed

- **The in-kernel 1.71× is still not SPLIT between cache reach and TLB reach**, though §4b now
  measures and sizes both. The kernel's arms straddle both walls simultaneously — control is past
  shared-L2 capacity (64 MB > 16 MB) *and* past TLB reach (4096 pages > ~2k–4k); arm 4 is inside
  both. §4b shows translation alone is worth 1.571× at zero capacity pressure, which is **large
  enough to account for most of 1.71× but does not prove it did.** An in-kernel arm that could
  separate them is impossible on arithmetic (§4b, last paragraph).
- **§2/§4's apparent contradiction is RESOLVED and the earlier hypothesis was wrong.** It was an
  axis error, not a physical effect: the window is per *call*, residency is set by the per-*i-step*
  footprint (128 j-panels × 2 streams). Re-mapped, probe and kernel are monotone-consistent and
  nearly coincide at both ends (control 803 vs 833). The earlier guess — that A-pack and output
  traffic evict a 384 KB window but not a 48 KB one — is **not needed and is withdrawn.**
- **`seq`-order numbers in `tlbreach.c` at M ≥ 65 are SUSPECT** by the design's own pre-declared
  control (364.6 / 564.5 / 800.5 at constant bytes *and* constant pages). Only `rev` is quoted.
- **TLB reach is bracketed, not pinned.** The knee is somewhere in 2112–4096 pages (1 MB row) and
  2304–4352 (4 MB row). No finer sweep was run, so "~2k–4k pages" is the honest statement.
- **The control's `and` cannot exist.** At `-O2`, IPSCCP propagates `K=4096` into the internal
  kernel, proving `%k·32 ≤ 131040 < 2⁴⁴`, so the control's mask folds and arms 0 and 1 assemble
  **byte-identically**. The plan's "control and treatment differ in one immediate" is not
  realizable. The repair — treatment carries strictly more instructions (15/16 vs 13), so every win
  is a lower bound — is **directionally** sound only; the residual is bounded in sum (arm 2 prices
  it at −1.2% net) and never isolated.
- **A's 1.1% is real but not separable.** gain(2) + gain(3) = 74.82 ms against gain(4) = 72.74 —
  sub-additive by 2.1 ms. A's gain vanishes once B is windowed.
- **The threaded 4.8% is not a confirmed win.** Reporting it as "disjoint by 0.28 ms" was wrong and
  is withdrawn: arms 3 and 4 are the same effect threaded (p=5e-05 for both) and 0.28 ms is not a
  distinction between them.
- Arm 2's 195.950 max is **cycle 1, slot 1** — the cold-clock cycle, a rotation artifact rather than
  instability. Excluding cycle 1 its max drops to 178.848 and the 1.71× is unmoved (1.7163/1.7153).
- 21 cycles over a 6-arm rotation is **not slot-balanced** (3 or 4 samples per (arm, slot) cell).
  Use a multiple of 6, or drop cycle 1.
- Unpinned laptop, wall-clock. Nothing under ~6% is a result unless distributions are disjoint.

## 6. Instrument defects found, and open

- `resid_ab.sh` **prints the `and` count but never asserts it.** An arm where only *one* of two masks
  folds still prints wrong values, passes the inverted value tripwire, and gets tabled under the
  wrong window label. N=1024 arm 5 was caught only because *both* folded. **Assert the expected
  per-arm count and fail.**
- `resid_ab.sh`'s verdict gates on a min/max range test that **one sample can flip** — threaded arm 1
  min is 53.066 and its second-lowest 53.391; removing one sample flips arm 3 from overlapping to
  disjoint. Use a permutation test or a bootstrap CI on the median.
- `resid_ab.sh`'s Gate 2 ("arm 1 stdout bit-identical to arm 0") is **vacuous** — they assemble
  byte-identically, so it cannot fail, yet it is reported as if it validated the control.
- `winmask.py` validates `2ⁿ−1` but not the `n ≥ 5` floor; a smaller mask breaks the 32-float stride
  alignment and would measure split-load cost instead of residency.

## 7. Measurement rules earned

> **19. Re-run the baseline binary before trusting a published table.** S42's L1 ceiling (1864) was
> thermal drift *during* the run; the same binary reads ~2000 today, and the 64 MB row of the same
> table still reproduces exactly. A table can be half-valid. Rule 14 says warm and interleave — this
> is its corollary: **a number that was never re-taken has never been checked.**

> **20. A window instrument beats a blocking parameter for pricing residency.** `kc` buys residency
> and pays `k/kc` output sweeps; you cannot read one from the other. Mask the addresses instead: the
> instruction stream is byte-constant and only the bytes' origin moves.

> **21. Name every mechanism that predicts your table, not just the one you were testing.** Cache
> reach and TLB reach predict these six arms identically. An experiment that cannot distinguish two
> mechanisms has not established either — say so where the number is written down.

> **22. A sweep needs a control arm that should NOT move, or drift is indistinguishable from the
> effect.** Found the hard way inside S43: a threaded probe swept buffer size with size as the
> *outer* loop and carried a zero-load arm. The zero-load arm — which touches no memory at all —
> tracked the 4-load arm **exactly**. That is global clock drift landing on the swept axis, and it
> voided the run. Without the control the sweep would have read as a clean cache curve. This is
> rule 14's teeth: interleaving is the fix, and **a null arm is how you prove the interleave
> worked.**

**Does rule 22 threaten §4?** `loadlevel.c` interleaved 15 reps per cell and reported end-to-end
drift of 1.004×, but it carried **no zero-effect control**. The load-bearing claim there is the *flat*
region (32 KB → 8 MB), and drift produces a monotone slope, not flatness — so "no L1 cliff, no
L2-slice cliff" survives. The **magnitude** of the 12 M–24 M cliff is less protected and should be
re-taken with a null arm before it is quoted precisely.

## 8. Concurrency discipline — `benches/perflock.sh`

S43 ran three investigations concurrently in separate worktrees on one machine with two SME units and
one thermal envelope. Every timed run goes through a machine-wide mutex that acquires an exclusive
lock **and** waits for build activity to drain. A concurrent `cargo build` is as fatal to an SME
timing as a concurrent benchmark. Given §3, this is not a precaution — it is the failure mode that
already put a thermal artifact into the published record.

**The first version of the mutex was itself a defect, and the fix is the interesting part.** It
blocked indefinitely. An agent parked on a blocking wait produces no output, so the harness watchdog
killed it as stalled — the mutex starved exactly the agents it was protecting, and two investigations
died mid-run holding results that had never been written to disk. Two rules came out of it:

1. **A machine-wide wait must be bounded below the caller's own liveness timeout.** `perflock` now
   gives up at 240 s (lock busy) or 120 s (machine never quiet) and exits **75 = retry later, your
   command did not run** — never a silent block, and never confusable with a result.
2. **On a machine that will not go quiet, refuse rather than measure.** The default is now to release
   and return 75 instead of running through another agent's build. `MAPAL_PERF_FORCE=1` overrides and
   obliges the caller to mark the run SUSPECT. §3 is the argument: a poisoned number that enters the
   record costs more than a measurement never taken.

And the operational lesson, which cost this session two sets of results twice over: **a finding held
only in an agent's context does not exist.** Every probe result is appended to
`benches/results-s43/*.md` the moment it lands, not at the end of the investigation.
