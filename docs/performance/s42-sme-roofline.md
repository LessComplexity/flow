# S42 — the SME roofline, and a constant that cost a whole session

Date: 2026-07-30 · Machine: **Apple M4 Pro**, macOS 26.3.1, Homebrew clang/LLVM 22.1.8.
Baseline commit `06ac50a`. Probes: `benches/sme/{roofline,mm4p,mv,bp,pipe2,kc}.c`.
`FEAT_SME=1`, `FEAT_SME2=1`, `SME_F32F32=1`; SVL = 64 B ⇒ 16×16 f32 ZA tiles, **4** of them.

S42 opened on one stated P0 — **k-loop software pipelining** — with an instruction in capitals to
look at the emitted assembly before writing anything, because S31 once predicted 2.9× from a change
LLVM was already making and measured ~10%.

That instruction paid, twice. The assembly showed the k loop is **not** pipelined, so the P0 looked
live; measuring it showed pipelining is worth **nothing**. And a second candidate (folding load
instructions) was also null. What the session produced instead is a **roofline** — the number this
leg should have been measured against from the start — and a **measured, sized prize for KC
blocking**, which was sitting at P1.

> ## READ THIS BEFORE QUOTING ANY NUMBER HERE
>
> **Every number in this file is a standalone hand-written C kernel, except §4's A/B table.** Per
> Sapir, that bounds what they can settle:
>
> > *"it is not a good measure — because we don't really know how it will act with ALL
> > optimizations that already exist together. Maybe with the existing optimizations this jumps to
> > more than what you see, and maybe scales better too on a threaded environment — so while this
> > test gives a standalone result, it doesn't fully apply to a fully integrated
> > optimization/pipeline."*
>
> So: a standalone null is **not** a verdict that an optimization is worthless in the emitter,
> where it composes with B packing, the arena, the task path and the rest. A standalone win is a
> **floor**, not a forecast. Both kinds of result are settled in the emitter, threaded, at scale.
> §2 and §3 are recorded as *"did not pay standalone"*, not *"refuted"*, for exactly this reason.

## 0. The headline

**What the "ceiling" column is, before reading it.** It is `benches/sme/units.c`: hand-written C in
which each thread issues `fmopa` on two registers loaded once before the loop — **no loads inside the
loop, no output written, no GEMM.** It is what the silicon can retire if memory were free, so
**nothing achieves it and nothing is supposed to.** It is a roofline, not a competitor. The number
that matters competitively is the numpy column.

**One SME unit, one thread:**

| N | Mapal SME rung | best hand kernel | numpy 1t | `fmopa` roofline (synthetic) |
| ---: | ---: | ---: | ---: | ---: |
| 1024 | 1043.3 | 1089.0 | **1655** | 2008.9 |
| 2048 | 1003.8 | 1074.1 | **1632** | 2007.7 |
| 4096 | 803 | 760.7 | ~1640 | 2001.0 |

Mapal is at **94–100% of the best kernel we can hand-write**, so the emitter is not the problem. Both
sit at about half the roofline, and **~62–65% of numpy** — that second figure is the real gap. The
residual is per-thread memory stalling.

**On all 14 cores it is much better and flat**, and here is the honest comparison:

| N | Mapal SME threaded | numpy threaded | behind by | roofline (2 units, synthetic) |
| ---: | ---: | ---: | ---: | ---: |
| 2048 | 2533 | 3239 | **1.28×** | ~4100 |
| 4096 | **2570** | 3113 | **1.21×** | ~4100 |

So threaded we are **1.21–1.28× behind Accelerate**, with no size decay at all — the 1-thread decay
S41b chased is a single-thread phenomenon. Quoting "62% of the roofline" is fine as a
machine-utilisation figure; quoting it as a competitive gap is not, and the 1.21× is the number to
carry.

KC blocking (§5c) is a **+6.1% single-thread win at 4096 and a −25.5% threaded loss**, so it ships
OFF. Getting to that answer took most of the session and one wrong constant — §5c is written as much
about the process failure as the result.

**The most useful result is §5e**, and it arrived last: the gap is **operand cache residency**, worth
**~1.79×** (1043 → 1864 GF/s, which would pass Accelerate's 1655). Loads cost 5% when they hit L1 and
halve throughput when they miss L2, so it is not instruction count, not scheduling, not the ZA-tile
ratio, and not silicon. Read §5e first.

## 1. The ceiling — ~2000 GFLOP/s, and what that number does and does not prove

`roofline.c` runs `fmopa` with **zero memory traffic**: operands are two registers loaded once
before the loop and never touched again. Each ZA tile is one loop-carried dependency chain, exactly
as in the GEMM, so sweeping the chain count is meant to separate *the port is saturated* from *we
ran out of chains*. Same total `fmopa` count in every row (`N³/256`), N=1024:

| chains (= ZA tiles) | ms | Gfmopa/s | GFLOP/s | vs 1 chain |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 4.2880 | 0.978 | 500.8 | 1.00× |
| 2 | 2.1400 | 1.959 | 1003.5 | 2.00× |
| 4 | 1.0690 | 3.924 | **2008.9** | 4.01× |

**What holds.** ~2000 GFLOP/s is a sound *practical* f32 ceiling, and it reproduces
independently: **2007.7** at 2048 (`pipe2.c`), **2001.0** at 4096 (`kc.c`), **2018.3** at 512
(`mv.c`) — four separate processes, and in `pipe2.c`/`kc.c` the roofline is re-measured *interleaved
with* the variants rather than in its own block. It is a real ceiling because **4 tiles is the ISA
maximum for f32**, so no kernel can present more chains than this.

Also solid: at 1 and 2 chains the loop is clearly **latency-bound**, which is the mechanism behind
S41b's ~4× from moving 1 tile → 4.

**What does NOT hold, and was corrected by adversarial audit.** An earlier draft of this file
derived "**4-cycle `fmopa` latency, 1-per-cycle issue, and 4 tiles exactly cover it**". That is one
equation with two unknowns — latency `L` in cycles and the streaming-mode core clock `f`. The probe
measures one rate per variant; it pins only `L ≥ 4·I`. The draft got `f ≈ 3.9 GHz` by assuming
`L = 4`, then got `I = 1 cycle` by assuming `f = 3.9 GHz`. Nothing here measures the clock, and the
M4 Pro's P-core maximum is 4.512 GHz, not 3.9. At 4.512 GHz the same data fits `L ≈ 4.6` cycles with
4 chains at 1 `fmopa` per 1.15 cycles — **still latency-bound at 4 chains**, which is the opposite
conclusion from identical numbers.

Two further checks that the draft failed:

- The measured ratio **exceeds its own model's ceiling**. Under `L=4`/`I=1`, four chains can be at
  most exactly 4.000× one chain. Measured: **4.0114×** and **2.0038×** — both above, and both
  biases in the probe (un-unrolled loop overhead, fixed `smstart`/`zero {za}` cost per call) push
  the *other* way. The 0.2–0.4% excess is therefore unmodelled drift, so "exactly 2× per doubling"
  is quoted to a precision this channel does not have.
- `roofline.c`'s three variants are **not interleaved** — three sequential best-of-9 blocks in the
  order 1, 2, 4, each right after 300 ms of maximum-power spinning. Any monotone drift lands
  directly on the scaling ratio. (`pipe2.c` and `kc.c` interleave; `roofline.c` should be fixed the
  same way before its ratios are quoted again.)

⇒ **Use ~2000 GFLOP/s as the ceiling. Do not quote the cycle-level story.** Whether the issue port
is saturated at 4 chains is **unresolved**, and it matters: it decides whether §5's residual is
"loads stealing slots from a busy port" or "the f32 `fmopa` stream never saturating the port at
all". Resolving it needs a clock measurement, which no probe here does.

One thing the ceiling *does* retire: Accelerate's 1655 is **82% of a ceiling measured on `fmopa`
itself**, so its number is reachable on this instruction and needs no second coprocessor to explain
it. The "is Accelerate secretly on Apple-AMX?" question is still open, but it is no longer
load-bearing for the gap.

## 2. Did not pay standalone — k-loop pipelining and unrolling

### 2a. The first attempt, and the arm that was not real

`mm4p.c` tried two transformations against a base kernel, N=1024, best-of-7:

| variant | ms | GFLOP/s | vs base | status |
| --- | ---: | ---: | ---: | --- |
| base | 2.0590 | 1043.0 | 1.000× | control, faithful to `mm4.c` |
| k unrolled ×2, 8 loads above 8 `fmopa` | 2.0460 | 1049.6 | 1.006× | **survived compilation** |
| rotation: k+1's loads before k's `fmopa` | 2.0530 | 1046.0 | 1.003× | **VOID — did not survive** |

**The rotate arm never existed.** Source order was "k+1 loads, then k `fmopa`"; LLVM emitted the
exact inversion at both `-O2` and `-O3`:

```
LBB3_4:  fmopa za0,z3,z2 / fmopa za1,z3,z0 / fmopa za2,z1,z2 / fmopa za3,z1,z0
         ld1w z0,[x15,x10] / ldr z2,[x15] / ldr z1,[x12] / ldr z3,[x13]
```

Every load sits immediately before its consumer across the back edge — which is precisely what
rotation exists to prevent. That arm measured base against base, and its 1.003× is not evidence of
anything. **Recorded as a method rule (§7.2).**

`mm4p.c` also had two design faults, both fixed in `pipe2.c`: no warmup (before §7.1 was known), and
b read **unpacked** at stride `N·4`, re-streamed once per i-panel — ~128 MB of b traffic per GEMM at
N=1024 against a ~1.05 ms `fmopa` floor. That loop is bandwidth-limited *by construction*, so a null
from unrolling it says little about the kernel the emitter actually emits, which packs b.

### 2b. Redone properly, on the packed kernel

`pipe2.c` keeps only the arm that provably survives, on the **packed** kernel, with the project's
standing method: ≥15 **alternating** runs, medians alongside minima, explicit overlap check, `c`
zeroed before every timed region so a skipped panel cannot inherit the previous variant's output,
and an **independent scalar `fmaf` reference** over 97 cells so the gate proves `A·B` rather than
mutual agreement.

| N | runs | base (median) | unroll ×2 (median) | worth | distributions |
| ---: | ---: | ---: | ---: | ---: | --- |
| 1024 | 51 | 1.9720 ms · 1089.0 | 1.9700 ms · 1090.1 | 1.001× | **OVERLAP** |
| 2048 | 31 | 15.9950 ms · 1074.1 | 15.9580 ms · 1076.6 | 1.002× | **OVERLAP** |
| 4096 | 15 | 181.7650 ms · 756.1 | 181.5110 ms · 757.2 | 1.001× | **OVERLAP** |

Both arms verified in the assembly at every size — base is 4 loads then 4 `fmopa`, unroll2 keeps all
8 loads above all 8 `fmopa`, no spills.

**+0.1–0.2%, overlapping, at three sizes on both kernel layouts.** Nothing here would justify the
emitter change *on its own* — but per the banner, that is "did not pay standalone", not "worthless
integrated". If it is ever built, it should be built for a reason that composes (e.g. alongside KC
blocking, where the k loop's operands are cache-resident and the balance may differ), and measured
threaded.

## 3. Did not pay standalone — folding 4 load instructions into 2

SME2's contiguous two-vector load halves the load *instruction* count. `mv.c`, control and
treatment sharing the **same** interleaved A pack so the only difference is the load form:

| N | 4× `ld1w` | 2× `ld1w x2` | + k unrolled ×2 | ceiling |
| ---: | ---: | ---: | ---: | ---: |
| 512 | 897.8 | 900.8 (1.003×) | 897.8 | 2018.3 |
| 1024 | 1071.6 | 1091.2 (1.018×) | 1092.9 | 2008.9 |
| 2048 | 632.9 | 641.5 (1.014×) | 638.1 | 2007.2 |

The mechanical side of this probe audited **clean, and stronger than the claim needs**: `ld1w {z,z}`
really is emitted; `svget2_f32` costs zero `mov z`; `svptrue_c32()` lowers to a hoisted `ptrue pn8.s`
and a wrong counter would zero two tiles and fail the gate; and the two functions are
**byte-identical for 273 instructions** apart from the load form — with the control mildly
*disadvantaged* on addressing, which biases toward the treatment.

**But the interpretation in the first draft was wrong.** It concluded "the limit is bytes, not
slots". `ld1w {z,z}` writes two architectural destinations and is **cracked into 2 load uops** on
Apple cores, so the transformation takes load *instructions* 4→2 while leaving load *uops* at 4.
The probe holds uop count constant and varies nothing about bytes — so it **cannot discriminate
slots from bytes in either direction**. The honest statement is the narrow one: *folding the
instruction encoding buys nothing, because the uop count is unchanged.*

## 4. B packing — real, and **already shipped in the emitter**

`bp.c` packs b panel-major once per GEMM instead of striding it:

| N | B strided | B packed | ratio | status |
| ---: | ---: | ---: | ---: | --- |
| 512 | 916.2 | 854.9 | 0.933× | **not a result** — 6.7%, min-only, no median |
| 1024 | 1066.8 | 1107.0 | 1.038× | **not a result** — 3.8%, min-only, no median |
| 2048 | **623.8** | **1175.6** | **1.885×** | far above the noise floor |

The 512 and 1024 rows are withdrawn as data: `bp.c` reports minima over 9 reps with no median and no
overlap check, and the project's standing rule (`sme_ab.sh:13-14`) is that a sub-10% difference on
an unpinned Mac at these sizes is noise. Only the 2048 row is a probe result.

**And this is the hand kernel catching up to the emitter, not headroom for Mapal.** The SME rung
already takes the packed path by default — verified from the emitted call arguments rather than by
reading the predicate:

```
default:    call void @mapal_sme_panel(… i64 16,   i64 16384, i64 1024, i64 1024)   ; bn=t, bj=t·k  -> PACKED
--no-pack:  call void @mapal_sme_panel(… i64 1024, i64 16,    i64 1024, i64 1024)   ; bn=b.ck, bj=t -> unpacked
```

**The one integrated measurement in this file.** A/B'd in the real emitter through
`sme_pack_ab.sh` — contract face both legs, `MAPAL_PAR=1`, 21 alternating runs, values identical
before any timing, commit `06ac50a`:

| N | B packed | B unpacked | packing worth | distributions |
| ---: | ---: | ---: | ---: | --- |
| 512 | 775.5 | 862.0 | 0.900× | **overlap — do not quote** |
| 1024 | 1043.3 | 1024.6 | 1.018× | **overlap — do not quote** |
| 2048 | **1003.8** | 643.3 | **1.560×** | **disjoint** |

The emitter reproduces the hand kernel's shape at all three sizes. `sme_tile_site` clause 8 gates
the layout on `tile_j == t`, which holds on this part, so nothing is being left on the table.

**Corollary that corrects a `next-session.md` §3 item.** "SME declines B packing today because the
pack width is hardcoded to NEON's `tile_j`" describes a *fallback* condition that does not fire on
this profile. Parameterising the width is still right for **portability** — it stops the rung
falling back where the widths differ — but it is **not a performance item here** and should not be
queued as one.

## 5. KC blocking in a standalone probe — **superseded by §5c, do not quote**

> **This section measured a hand-written kernel and predicted 1.448×. The real emitter delivers
> +6.1% at one thread and −25.5% threaded (§5c), and the probe's optimum depth (512) is not the
> emitter's (1024). It is kept only because §7.3 is the lesson drawn from the gap.**

With b packed, unrolling null, and the ceiling flat at ~2000 across every size, 4096 still loses a
third of its throughput (760.7 GF/s, 38%). Count the working set: at k=4096 one panel streams
`ap` = 32·4096·4 = 512 KB **plus** `bp` = 512 KB = **1 MB per panel call**, which no per-core cache
holds. That is KC blocking, and `kc.c` sizes it.

It pays the cost `next-session.md` §3 flagged: the kernel **stores** its ZA tiles rather than
accumulating into `c`, so every k-block after the first is a read-modify-write of the output block
(`read.horiz` out, add, store back) — more expensive than spilling vector registers. Both kernels
are in the probe so the crossover is measured, not assumed.

N=4096, 9 alternating runs, medians, independent scalar gate on every KC:

| KC | min | median | max | GFLOP/s | % ceiling | working set |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 256 | 161.258 | 162.622 | 163.500 | 845.1 | 42% | 64 KB |
| **512** | **123.972** | **124.813** | **126.432** | **1101.2** | **55%** | **128 KB** |
| 1024 | 129.485 | 131.310 | 134.019 | 1046.7 | 52% | 256 KB |
| 2048 | 147.789 | 155.278 | 164.577 | 885.1 | 44% | 512 KB |
| 4096 | 179.712 | 180.679 | 186.178 | 760.7 | 38% | 1024 KB ← no blocking |

**In this probe KC=512 was worth 1.448×, disjoint. It did not transfer — see §5c.**

Three things make this a real cache-blocking curve rather than an artifact:

1. **It is unimodal.** KC=256 (64 KB) is *worse* than 512 — smaller is not monotonically better, so
   there is a genuine optimum rather than a trend.
2. **It flattens the size curve.** 1101.2 at N=4096 against 1089.0 at N=1024 (§2b) — the decay that
   S41b attributed to arithmetic intensity, and that survived B packing, is **gone**.
3. **It wins despite the ZA read-modify-write.** The crossover the plan warned about is real and
   lands in blocking's favour at 4096.

This promotes KC blocking from P1 to **the measured P0**, with an optimal parameter (a 128 KB
two-panel working set) to derive from the profile rather than hardcode.

## 5b. The matrix unit count — **measured at 2**, and what follows from it

S41b inferred "roughly two usable SME units" from the *scaling ratio* of the real kernel (NEON 8.61×
across cores against SME 2.23×). `benches/sme/units.c` measures it directly: N threads each running
the `fmopa` issue loop with **zero memory traffic**, so the aggregate can only be limited by how many
`fmopa` the machine retires per second, and the knee is the unit count.

| threads | aggregate GFLOP/s | vs 1 | per-thread | fastest ms | slowest ms |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1997.8 | 1.00× | 1998 | 123 | 123 |
| 2 | **3849.6** | **1.93×** | 1925 | 128 | 128 |
| 3 | 3383.8 | 1.69× | 1128 | 128 | 218 |
| 4 | 4173.1 | 2.09× | 1043 | 129 | 236 |
| 6 | 4178.8 | 2.09× | 697 | 249 | 353 |
| 10 | 4062.7 | 2.03× | 406 | 417 | 605 |
| 14 | 4076.1 | 2.04× | 291 | 639 | 844 |

**Two threads scale 1.93× and both finish in 128 ms; from three on the aggregate is flat at
~4100 GFLOP/s through 14 threads, with `per_thread × n ≈ 4100` at every point.** The ms columns show
the queueing outright — at 4 threads two threads finish in 129 ms and two take 236 ms.

⇒ **2 units × ~2000 GFLOP/s, aggregate ceiling ≈ 4100 GFLOP/s.** An inference became a measurement.

**The E-core half of that probe is INCONCLUSIVE and is not a result.** Its own stated validation was
that `QOS_CLASS_BACKGROUND` must measure slower per thread than `USER_INTERACTIVE` or the QoS steering
did not take. It did not: 1991 vs 1998 GFLOP/s. So either the request was ignored or E-cores reach the
same units; the probe cannot distinguish them. All that can be said is that BACKGROUND threads
collectively cap at ~2100 GFLOP/s, about one unit's worth.

### What follows: there is no lane-count win, and that is the finding

The obvious move from "the unit is shared" is to stop spending 14 lanes on 2 units. The real kernel
says no. KC off, N=4096, GFLOP/s by lane count:

| 1 | 2 | 3 | 4 | 6 | 8 | 10 | 14 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 789 | 1454 | 1781 | 1993 | 2289 | 2350 | 2515 | **2559** |

**Monotonically increasing — more lanes never hurt.** One thread reaches only 39.5% of a single
unit's 1998 GFLOP/s because it stalls on memory, so extra lanes usefully interleave into those
stalls rather than contending. Capping lanes at 6 would cost 11% and at 4 would cost 22%; excluding
the E-cores would cost 1.7%. Every version of the idea is a throughput trade, not a win, so **none
of it is built** — a change has to arrive with a measurement of it helping.

The ratio that a lane policy would need is not a constant either: per-thread utilization is **0.395
at N=4096** and **0.512 at N=1024**, i.e. a property of memory behaviour at a size, not a machine
fact the profile or a one-off probe could record.

**What the ceiling does say is where the real gap is.** Threaded peak 2559 against the 4100
two-unit ceiling is **62%**, and it is the same deficit as the 1-thread 52%-of-roofline in §0 —
per-thread memory stalling, not scheduling. The lever that would improve per-thread locality is KC
blocking, and §5c measures that as a loss. That is an honest dead end, recorded so the next session
does not re-derive it.

**Layering note, because this is where it would go wrong.** The unit count must not reach the
emitter: baked into a module it would be wrong on a part with a different count. It is the same class
of fact as lane count, which `mapal-rt` already owns for the stated reason —
*"the runtime picks the count, because lane count is the one input the compiler does not have"*
(`slice_ranges`). `mapal-ir` learns nothing here; the emitter contributes only per-unit **sizes**
(the `ti·t × tj·t` panel, the `ti·t·c` slice quantum, `sme_kc`), all count-agnostic.

## 5c. KC blocking for SME — built, swept, and NOT enabled

Implemented behind `EmitOpts::kc_nest` (default OFF). Emission verified line by line: `ap` alloca
`panel_rows·kc`, pack bound `kc`, a's k coordinate `k0 + pk`, k0 loop outside the i loop, A pack
hoisted out of j, b offset `k0·pack_w`, `bj` left at the whole-panel stride, first block stores and
later blocks accumulate. Work counts exact: 268M `fmopa` = `N³/256`, A-pack elements = `rows·k`.
**Values identical to the NEON leg at every size and every depth**, checked before any timing.

### The depth sweep — and the constant that was wrong

For most of S42 this rung measured as a **1.27× loss**, and four separate write-ups in this file
said so. All of them were taken at **one depth**, `kc = 512`, which `sme_kc` derived by fitting two
packed panels into L1D. Sweeping the depth in the real emitter shows 512 is two steps down a sharp
curve. N=4096 and N=2048, f32, 1 thread, alternating, values identical at every row:

| working set | kc | N=4096 | N=2048 |
| ---: | ---: | ---: | ---: |
| 32 KB | 128 | — | 0.220× |
| 64 KB | 256 | 0.501× | 0.387× |
| 128 KB ← the derived depth | 512 | **0.785×** | 0.639× |
| **256 KB** | **1024** | **1.064×** | 0.986× |
| 512 KB | 2048 | 1.027× | **1.000×** (unblocked) |
| 1024 KB | 4096 | 1.000× (unblocked) | — |

The optimum working set is **256 KB at both sizes**, and every depth *below* it is catastrophic.
⇒ **The defect was the constant, not the technique and not the implementation.**

**256 KB is not a machine fact on this part** — L1D is 128 KB, L1I 192 KB, the per-core L2 slice
~3.2 MB. So it is recorded as `Sme::panel_l1d_ratio`, a **policy ratio explicitly documented as
search space**, the same honest category `acc_vecs_per_row` and `nc_tiles` already occupy (ADR-0034).
Folding a `2 ×` into `sme_kc` would be a fitted constant wearing a derivation's clothes.

### At the corrected depth

15 alternating runs, values identical in every cell:

| N | config | KC off | KC on | | distributions |
| ---: | --- | ---: | ---: | ---: | --- |
| 2048 | 1 thread | 18.014 ms | 17.751 | +1.5% | overlap |
| 2048 | threaded | **6.783** | 7.779 | **−12.8%** | disjoint |
| 4096 | 1 thread | 171.179 | **161.360** | **+6.1%** | disjoint |
| 4096 | threaded | **53.485** | 71.796 | **−25.5%** | disjoint |

**A one-thread-only optimization on this part.** The depth fix turned the single-thread case from a
loss into a small win and left threaded a large loss, so it stays OFF: enabling it would take
threaded 4096 — the headline matmul cell — from **53.5 ms to 71.8 ms**.

One explanation covers all four cells. The A panel is `ti·t × k` = 512 KB **regardless of thread
count**, because slices are cut on the `ti·t·c` quantum and a core still works one panel at a time.
So threaded, 14 cores hold ~7 MB of A panels plus ~4 MB of packed B — ~11 MB inside a 16 MB L2,
which fits. Blocking shrinks each panel to 256 KB but adds 4× the `c` sweeps; at one thread there is
spare bandwidth so the cache win shows, and threaded the cores contend so the `c` traffic wins.

### Six candidate causes measured and refuted

Recorded so they are not re-tested. Every one of these was investigated while the depth was wrong,
which is why none of them was the answer:

| candidate | verdict |
| --- | --- |
| the loop nest is wrong | no — verified index by index, work counts exact, values identical |
| the b layout (whole-k slice vs kc-deep repack) | **1.065×** (`benches/sme/bslice.c`) |
| the read-out code is bad | no — emitted asm is 4 instructions per tile, no spills |
| the streaming-mode ABI (`_body` transitions + `d8–d15` spills) | **1.0 ms** over 131072 calls (`benches/sme/smcost.c`) |
| the pack's memory order breaks under blocking | no — the same loops in C: 8.46 ms unblocked, 8.05 blocked (`benches/sme/packcost.c`) |
| the pack spills its row pointers (it did) | fixed — scalar float loads 51 → 5 — worth **3%** |

The pack reorder is kept because it is strictly better code (row-outer needs one live row pointer
instead of `ti·t` of them), and it helps both paths, but it was not the answer either.

**Two claims made earlier in this session are retracted:** "the accumulate read-out costs 85.8 ms"
and "the blocked kernel runs at 1598 GFLOP/s". Both came from a probe that forced the kernel's `K`
argument to 1 — which shrinks the k loop but leaves the full 16-row × 4-tile ZA read-out intact, so
it counted read-outs as pack cost. **Do not reuse that probe design.**

## 5d. The same question on the box — and the S29 open item, closed

`kc_nest` is a different code path from §5c's SME k-panel — the NEON/AVX rung — and `lib.rs` had
carried an open item since S29: kept *"because the lever was designed against BOX-scale traffic
(16 GB of A re-reads at 4096 on zen3) **where it has not yet been measured**."*

Machine: **Intel i9-14900F**, 24 cores / 32 threads, 48 KB L1d, **2 MB per-core L2**, 36 MB L3,
**AVX2, no AVX-512** (`avx512f` absent, so `ZEN3`'s 32-byte/16-register shape is right), governor
`performance`. The box has `gcc` but **no clang**, so the emitted IR was cross-compiled on the Mac
(`clang -target x86_64-unknown-linux-gnu -mavx2 -mfma -c`) and linked there with `gcc` — the emitted
`.ll` carries no target triple, which is what makes that work. `mapal-rt` was built as a staticlib
for `x86_64-unknown-linux-gnu`, needing no C toolchain on the box.

**Values identical to the Mac's at every size and depth** (`74348 -302529` at 4096) — an independent
cross-ISA confirmation of the ADR-0032 claim that every profile field is value-invariant.

For `zen3` at f32 the chain is `lanes 8 → tile_j 32 → nc 1024`, so **`tile_kc = l2_bytes / 8192`**.
Swept by varying `l2_bytes`, N=4096:

| depth | blocks | 1 thread | threaded (32) |
| ---: | ---: | ---: | ---: |
| unblocked | 1 | **926.759 ms** (148.3 GF/s) | **112.919 ms** (1217.2 GF/s) |
| 4096 | 1 | 0.978× (overlap — this depth closes the gate, so it *is* unblocked) | 1.000× (overlap) |
| 2048 | 2 | 0.548× | 0.595× |
| 1024 | 4 | 0.557× | 0.759× |
| 512 | 8 | 0.551× | **0.771×** best |
| 256 | 16 | 0.560× | 0.767× |
| 64 | 64 | 0.454× | 0.668× |

**It is a step function, not a curve.** Any blocking at all costs ~1.8× at one thread and ~1.3× at
best threaded, at *every* depth from 2 to 64 blocks. Unlike the Mac there is no optimum to find —
the depth is irrelevant. That `tile_kc = 4096` overlaps unblocked validates the harness, since that
depth closes the `k > tile_kc` gate and therefore emits the unblocked path.

⇒ **The S29 open item is closed properly — with a sweep, not a single point.** "Unmeasured on the
box" is no longer a reason to keep `kc_nest`. Either a machine justifies it in writing, or the lever
should be deleted.

**Two incidental facts.** The box's untuned vector path reaches **148.3 GFLOP/s at one thread**,
about 71% of the i9's AVX2 FMA peak, so the deficit this project chases is not a broken vector leg.
And threaded at 4096 the box does **1217 GFLOP/s** against the M4 Pro's **2570** with SME — the
laptop with a matrix unit is **2.1×** the desktop without one.

## 5e. Where the gap actually is: **operand cache residency**, sized at ~1.79×

`benches/sme/loadcost.c` holds the compute exactly constant — four independent `fmopa` into the four
f32 ZA tiles, every iteration, in every row — and varies only how many operands come from memory
rather than from registers loaded once before the loop.

| operands from | 0 loads | 1 | 2 | 3 | 4 loads |
| --- | ---: | ---: | ---: | ---: | ---: |
| **32 KB buffer (L1-resident)** | 1956.7 | 1913.7 | 1928.9 | 1910.0 | **1864.2 (95%)** |
| **64 MB buffer (past L2)** | 1915.5 | 941.8 | 841.8 | 755.3 | **760.8 (40%)** |

**Loads cost 5% when they hit L1.** The load *count* is nearly free, so 1 load per `fmopa` is **not**
a ceiling and the 4-ZA-tile arrangement is not the limit. But when operands come from beyond L2,
throughput **halves at the first load** and then flattens — the count stops mattering entirely.

⇒ **The gap is operand cache residency.** Not instruction count (§3, and 5% here), not scheduling
(§2), not the tile ratio, not silicon (§5b: Accelerate sits under the SME roofline at both thread
counts). This retires all three of the standing hypotheses at once.

### It sizes the prize, and the prize is large

| | GFLOP/s |
| --- | ---: |
| the emitted kernel today | 1043 |
| Accelerate, 1 thread | 1655 |
| **4 loads, operands L1-resident** | **1864** |

**~1.79× is available, and it would put the rung past Accelerate.** It also reframes §5c: making
operands cache-resident is exactly what k-blocking is *for*. Ours delivered **6.1% of a possible
79%**, so the technique is not wrong — our realisation captures almost none of it. That is a far
better-posed problem than "KC loses", and it is the P0 the next session should open on.

### Two approaches to carry forward (Sapir)

1. **A layout with more reuse per load** — structure so loads have minimal friction with `fmopa`,
   e.g. separating the load and compute streams deliberately rather than interleaving them per k.
   The measurement above says the win is in *where the bytes come from*, so this is about residency,
   not about issue order (which §2 already refuted).

2. **SME2 multi-vector forms.** Checked against `arm_sme.h`, not assumed:
   - **multi-vector loads** (`ld1w {z0.s, z1.s}`): fewer load instructions, same bytes — **measured
     1.018×** (§3), and now explained, since loads were never the bottleneck.
   - **multi-vector MAC into ZA slices** (`svmla_za32_f32_vg1x2` / `_vg1x4`): these exist for f32 but
     are **element-wise, not outer products** — one `vg1x4` is ~64 MACs against `fmopa`'s **256**, so
     for f32 they are strictly *less* dense. Not a win here.
   - **widening outer products** (`svmopa_za32_f16_m`, `bf16`, `s8`, `u8`, `s16`, `u16` → 32-bit ZA):
     **this is the real density lever.** Narrower input means more elements per vector and therefore
     more MACs per instruction — f16/bf16 ≈ **2×**, i8 ≈ **4×**.
   - For f32→f32 there is exactly one form, `svmopa_za32_f32_m`, with no multi-vector variant. So no
     instruction-density win exists at f32 — but an f16/bf16 rung would get 2× per instruction, and
     that is a separate, unexplored direction.

## 6. What this does to the S42 plan

| item | was | is now |
| --- | --- | --- |
| k-loop software pipelining | **P0** | **did not pay standalone** — +0.1–0.2%, overlapping, 3 sizes, both layouts |
| KC blocking for SME | P1 | **BUILT and swept. +6.1% at 1 thread / −25.5% threaded at N=4096. Ships OFF** (§5c) |
| the SME k-panel depth | derived from L1D ⇒ 512 | **WRONG — 0.785×.** Swept optimum is 1024 (a 256 KB window), now a documented *policy ratio*, not a pretend derivation |
| `kc_nest` on the box | "designed for box traffic, unmeasured" since S29 | **CLOSED by a depth sweep**: a step function, ~0.55× at 1 thread and ~0.77× at best threaded, at *every* depth (§5d) |
| how many matrix units | inferred "roughly two" | **measured: exactly 2**, ~2000 GFLOP/s each, ~4100 aggregate (§5b) |
| cap SME lanes to the unit count | the obvious next move | **refuted** — more lanes never hurt; every cap is a throughput trade (§5b) |
| per-core-class geometry (P vs E) | proposed | **no measured win**: E-cores add 1.7%, so excluding them loses 1.7%. Slices are already over-decomposed (`oversub: 4`) and stolen, so there is no straggler |
| B packing for SME | queued as a consequence | **already shipped**; 1.560× at 2048 in-emitter (§4) |
| make the pack width a parameter | queued for performance | **portability only** — reclassify |
| the A pack's loop order | not on the list | **fixed** — row-outer needs one live row pointer instead of `ti·t`; scalar float loads 51 → 5, worth 3% on both paths |
| whether the `fmopa` port saturates at 4 chains | assumed yes | **UNRESOLVED** — needs a clock measurement (§1) |
| **where the remaining gap is** | "per-thread memory stalling", unlocalised | **operand cache residency**, sized at **~1.79×** (1043 → 1864 GF/s), which would pass Accelerate. **The next P0** (§5e) |
| is the 1 load : 1 `fmopa` ratio a ceiling? | assumed yes (4 ZA tiles) | **no** — 4 loads cost 5% from L1; the count is nearly free (§5e) |

## 7. Measurement rules earned

### 7.1 — Warm the clock before timing SME, and interleave the variants

The same binary, unchanged, measured the §1 ceiling at **1.852 ms** on one invocation and
**1.069 ms** on the next — a **1.73× swing on identical code**. Best-of-N does not save you: a first
run's *entire* sample set is cold, so best-of-7 returns the best cold number and reports it as a
measurement.

Every probe from `mv.c` onward spins `fmopa` for **300 ms before the first timer**, and interleaves
its variants so residual drift hits them equally.

> **Rule 14. On this part, warm the clock before timing SME, and interleave the variants.** A cold
> process measures the frequency ramp, not the kernel — 1.73× worth. A best-of-N over a cold window
> is still cold.

### 7.2 — A transformation you cannot find in the assembly is not a variant

`mm4p.c`'s rotate arm was inverted by LLVM at both `-O2` and `-O3` and measured base against base
(§2a). The null it produced was indistinguishable from a real null.

> **Rule 15. Verify in the emitted assembly that the transformation under test survived, before
> reading its timing.** This is the companion to the S31 lesson: S31 was about the compiler *already
> doing* what you were about to add; this is about the compiler *undoing* what you just added. Both
> produce a null for the wrong reason.

### 7.3 — A standalone probe prices a change; it does not settle it (Sapir)

> **Rule 16. A standalone C probe cannot settle what an optimization is worth in the emitter.** It
> lacks every other optimization the change would compose with, and it says nothing about threaded
> scaling. A standalone win is a floor; a standalone null is not a refutation. Settle it integrated,
> threaded, at scale — and scale the sizes up, because the terms reorder with N.

S42 is the proof of this rule and the cost of ignoring it. `kc.c` predicted **1.448×**; the emitter
delivered **+6.1% at one thread and −25.5% threaded**. Worse, the probe's optimum *depth* (512) was
not the emitter's (1024), so the probe could not even have found the right constant.

### 7.4 — Sweep the parameter; never test one point

`sme_kc` returned 512 and every KC measurement in S42 was taken there. It measured 0.785×, and four
separate write-ups in this file concluded "KC loses". A depth sweep — the plainest possible
experiment — showed 512 was two steps down a sharp curve and 1024 wins.

> **Rule 17. Before concluding that a parameterised optimization does not pay, sweep the parameter.**
> A single point cannot distinguish "the technique does not work" from "this constant is wrong", and
> those have opposite consequences. Six candidate causes were investigated and refuted at the wrong
> depth before the sweep was run; every one of them was wasted work.

### 7.5 — A probe that changes one input must not silently change another

The probe used to attribute cost forced the kernel's `K` argument to 1, intending to isolate the
pack. It did not: `K` bounds the k loop but the ZA read-out is a separate loop over `t` rows, so the
probe counted 131072 full read-outs as pack cost, and produced two confident wrong attributions.

> **Rule 18. When a probe neutralises part of a kernel, verify in the emitted code that the part is
> actually gone.** The same discipline as rule 15, applied to subtraction instead of transformation.

## 8. What is NOT claimed

- **The banner.** §1–§3 and §5 are hand-written C, not Mapal output. The integrated numbers are §4's
  A/B table, §5c, §5d and the Mapal column of §0.
- §1's cycle-level mechanism (latency 4, issue 1/cycle, port saturated) — **withdrawn**, see §1.
- §5's 1.448× — **superseded**, see §5c. §5 is retained for §7.3 only.
- **Retracted outright:** "the accumulate read-out costs 85.8 ms" and "the blocked kernel runs at
  1598 GFLOP/s". Both came from the `K=1` probe described in §7.5, which left the ZA read-out intact.
- §4's 512 and 1024 probe rows, and §4's 512/1024 emitter rows — **overlapping distributions**.
- §5c's N=2048 single-thread row (+1.5%) — **overlaps**. Only the 4096 single-thread row (+6.1%) and
  both threaded rows are disjoint.
- §5b's E-core measurement — **inconclusive**: `BACKGROUND` matched `USER_INTERACTIVE` per thread
  (1991 vs 1998 GFLOP/s), which by the probe's own stated criterion means the QoS steering did not
  take. Nothing about E-core matrix units is established.
- **`panel_l1d_ratio: 2` is a swept constant, not a derivation**, and it is swept on **one part at
  two sizes**. It should not be assumed to hold on any other machine — and §5d shows the box does not
  even have an optimum to sweep for.
- KC blocking threaded has been measured only on the M4 at N=2048/4096, and on the box at N=4096.
  Neither machine was swept threaded across depths; only single-thread was.
- Unpinned laptop, no CPU pinning, wall-clock `CLOCK_MONOTONIC`. Per rule 6/11 nothing below ~10% is
  a result unless its distributions are disjoint, which is stated per row.
- The box runs are cross-compiled on the Mac and linked with `gcc` on the box; the same IR, but not
  the same C toolchain end to end.
