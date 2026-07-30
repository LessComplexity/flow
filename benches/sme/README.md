# SME probes (S41 + S42)

**S42's finding is `loadcost.c`: the gap is operand cache residency, worth ~1.79×.** Jump to
"THE FINDING" below, or read `docs/performance/s42-sme-roofline.md` §5e. Everything else in this
file is the road to it, including several dead ends kept so they are not re-walked.

Four probes (S41) that priced the ARM SME leg before any emitter work, following the S38 method note:
*"A verdict from a fleet of agents is a hypothesis, not a result."* Each is standalone, runs on an
Apple M4 Pro, and takes seconds. Recorded here because the findings are load-bearing for
`docs/components/backend-nvptx/plans/plan-s41-the-nvptx-leg.md` §2.2 and because two of them are
machine facts that cost real time to discover.

Machine: Apple M4 Pro, macOS 26.3.1, Homebrew clang/LLVM 22.1.8.
`hw.optional.arm.FEAT_SME=1`, `FEAT_SME2=1`, `FEAT_SME_F64F64=1`, `SME_F32F32=1`; `FEAT_SVE` unset.

## THE BUILD FLAG THAT MATTERS

```sh
clang -O3 -march=armv8-a+sme2 …      # correct
clang -O3 -march=armv9-a+sme2 …      # SIGILLs at runtime
```

**`armv9-a` implies `+sve`.** This part has SME but **not** full SVE, so LLVM emits non-streaming
SVE (the fault is `cntd` in the prologue, before `smstart`) and the process dies with
`EXC_BAD_INSTRUCTION`. It compiles cleanly either way — only running catches it. Use
`armv8-a+sme2`, `armv8.5-a+sme2`, or explicit `armv9-a+sme2+nosve`; all three verified working.

Related C syntax trap: `__arm_new("za")` is a **declaration** attribute (before the function) and
`__arm_streaming` is a **type** attribute (after the parameter list). Swapping them is a compile
error, not a silent misbuild.

## The probes

| File | Question | Result |
| --- | --- | --- |
| `probe.ll` | does LLVM lower an outer-product accumulate to ZA? | `llvm.aarch64.sme.mopa.nxv4f32` → `fmopa za0.s, p0/m, p1/m, z0.s, z1.s`, with `zero {za}` / `smstart za` / `smstop za` generated automatically from the function attributes |
| `svl.c` | what is the tile shape? | **SVL = 64 bytes (512 bits)** ⇒ ZA is 64×64 B, one f32 tile is **16×16**, **4** f32 tiles (8 at f64) |
| `run16.c` | does SME execute correctly? | 16×16 matmul, K=32, **0/256 mismatched** vs a scalar reference |
| `run16b.c` | which precision face is `fmopa`? | vs separate mul+add (`exact`): **92/256 differ**; vs fused `fmaf` (`contract`): **0/256 differ** ⇒ **`fmopa` fuses** |
| `mmN.c` | what is the ceiling worth? | see below |

```sh
llc -mtriple=aarch64-apple-darwin -mattr=+sme,+sme2,+sve -o - probe.ll   # lowering only, not run
clang -O2 -march=armv8-a+sme2 -o svl    svl.c    && ./svl
clang -O2 -march=armv8-a+sme2 -o run16  run16.c  && ./run16
clang -O2 -march=armv8-a+sme2 -ffp-contract=off -o run16b run16b.c && ./run16b
clang -O3 -march=armv8-a+sme2 -o mmN    mmN.c    && ./mmN 1024 7
```

## The ceiling probe (`mmN.c`)

Hand-written SME GEMM, f32, **one thread**, min-of-7, A packed per i-panel inside the timed
region. Against the recorded M4 Pro baselines in `docs/performance/matmul/s33.md:150-158`:

| N | Mapal `flow-fma-1t` (NEON) | **SME hand-written** | numpy-1t (Accelerate) |
| ---: | ---: | ---: | ---: |
| 512 | 2.1766 ms | **0.7550 ms** (355 GFLOP/s) | 0.1600 ms |
| 1024 | 17.5449 ms | **5.0320 ms** (427 GFLOP/s) | 1.2977 ms |
| 2048 | 152.07 ms | **75.0020 ms** (229 GFLOP/s) | 10.529 ms |

At 1024: **3.49× faster than the current tuned NEON path**, and the numpy gap narrows from
**13.5× to 3.88×**.

**This is a floor, not a ceiling.** The kernel is deliberately unoptimized in exactly three ways
the LLVM backend already implements for NEON, and which an SME realization inherits because it is
a leaf swap inside the same nest:

1. accumulates into **1 of 4** available f32 ZA tiles;
2. **no B packing** — B streams from memory on every i-panel (rung 3, `emit_tile_packed_j_outer`);
3. **no KC blocking** — visible in the data as 427 GFLOP/s at 1024 collapsing to 229 at 2048 as B
   falls out of cache (the KC nest, `emit_tile_packed_kc`).

Accelerate is achieving ≈1655 GFLOP/s in that 1024 cell, so this naive kernel reaches ≈26% of it.

## What these numbers are NOT

- **Hand-written C, not Mapal output.** They price the leg; they are not a Mapal result and must
  never be quoted as one.
- `mmN.c` is **not value-verified** against a reference at N — correctness was established exactly
  at 16×16 by `run16.c`. The N-scale kernel is sanity-checked by magnitude only. Any Mapal SME
  realization goes through the differential duty (ADR-0020) before any number is published.
- Unpinned laptop, min-of-7. Fine for an order-of-magnitude price, not for a ≤10% claim
  (measurement rule: ≥50 alternating runs for that).

## `spec-verified.ll` — the exact IR a Mapal SME realization must emit

Hand-written, lowered, linked and **run**: `0/256 differ` against a fused reference, `0` cells
written outside the 16-wide panel. This is the emitter's target, not a sketch.

```sh
clang -O2 -march=armv8-a+sme2 -c spec-verified.ll -o spec.o
clang -O2 -march=armv8-a+sme2 -o drv spec-driver.c spec.o && ./drv
```

Everything it needs is three intrinsics and one attribute set:

```llvm
declare void @llvm.aarch64.sme.zero(i32 immarg)
declare void @llvm.aarch64.sme.mopa.nxv4f32(i32 immarg, <vscale x 4 x i1>, <vscale x 4 x i1>,
                                            <vscale x 4 x float>, <vscale x 4 x float>)
declare <vscale x 4 x float> @llvm.aarch64.sme.read.horiz.nxv4f32(<vscale x 4 x float>,
                                            <vscale x 4 x i1>, i32 immarg, i32)

attributes #0 = { "aarch64_new_za" "aarch64_pstate_sm_body" vscale_range(1,16)
                  "target-features"="+sme,+sme2,+neon,+fp-armv8,+v8a" }
```

Notes that cost a SIGILL each to learn:

- **`aarch64_pstate_sm_body`, NOT `aarch64_pstate_sm_enabled`.** `_enabled` means "the caller has
  already entered streaming mode" and pushes the transition onto every call site — with it, the
  emitted `ptrue p0.s` runs *before* `smstart sm` and the process dies with
  `EXC_BAD_INSTRUCTION`. `_body` emits `smstart za` + `smstart sm` at entry and both `smstop`s at
  exit, so the kernel is self-contained and **no other emitted function needs to know streaming
  mode exists**. That is what keeps an SME realization a leaf swap rather than an ABI change.
- Predicates are plain `<vscale x 4 x i1> splat (i1 true)` — no `ptrue` intrinsic call to emit.
- Operand loads are plain `load <vscale x 4 x float>, ptr %p` — no SVE load intrinsic.
- `<vscale x 4 x float>` is 16 floats **only because SVL is 512 bits here**. The surrounding
  index arithmetic (`k*16`, 16 output rows) is fixed at 16 and is therefore an
  SVL-specific constant: it must come from the profile as a per-`Loc` machine fact, never from
  `mapal-ir`.

## The Mapal SME rung — landed, and what is NOT yet measured (S41 close)

The realization is in `crates/backends/llvm/src/func/sme.rs`, selected at one point in
`func/tile.rs` by `sme_tile_site`, behind the `apple-m4-sme` profile + the contract face.
Gate **1023 passed / 0 failed**, fmt clean, and **636/636 emissions byte-identical** across
generic/apple-m/zen3/cuda-ada — the rung is invisible to every pre-existing profile.

**Correctness is strong.** An adversarial review ran a value differential on hardware — SME leg
vs NEON leg vs the interpreter oracle — over square and non-square shapes, `k` not a multiple of
the tile side, non-zero `base`, transposed A, B row stride ≠ c, packed and `--no-pack`, the arena
path, and under AddressSanitizer: **0 differing cells everywhere**.

### Three things that are NOT done, stated plainly

1. **No executing value gate in the test suite.** `tests/sme_rung.rs` is `str::contains` only — it
   never compiles or runs anything. The existing differential harness *cannot* cover SME as
   written: it shells out to bare `clang -O0`/`-O2` with no `-march=armv8-a+sme2`, which per this
   file yields a module that SIGILLs. The differential evidence above was produced by hand during
   review and **is not repeatable from `cargo test`**. This is the first thing to fix.

2. **No matmul benchmark can take the rung.** The selection predicate is sequential-only
   (`GuardFlavor::Host`, `!split_range`) — a deliberate first-landing scope — but **every**
   `benches/matmul/*.mapal` source lands on the parallel task path, and there is no emit-time flag
   to disable it (`MAPAL_PAR=1` is a *runtime* knob; the tasks are already in the IR). So the
   published matmul cells this leg exists to attack are unreachable until the predicate is lifted
   to `GuardFlavor::Task`/`TaskBody` with `split_range` respected. The kernel is self-contained
   (`aarch64_new_za` gives it its own ZA per call), so this is scope, not a safety bound.

3. **No trustworthy kernel-level speedup number yet.** `attn256_timed.mapal` (the one self-timed
   sequential shape that fires the rung) reports **0.0345 ms** for a 256³ chained matmul — about
   **1945 GFLOP/s, faster than Accelerate, i.e. impossible.** The timed region is not capturing
   the work; the likely cause is that only `o[0]` and `o[65535]` are read, so the map is largely
   elided (the S37 write-only-array class). Whole-binary wall time, 15 alternating runs, does show
   **2.90 → 2.26 ms median (1.28×)** — but data generation is common to both legs and dominates,
   so that is a floor on a diluted measurement, not a kernel result.

**Do not quote a Mapal SME speedup until (2) and (3) are fixed.** The only defensible SME number
today remains the hand-written ceiling probe above (`mmN.c`, 5.0320 ms at 1024² vs the recorded
NEON 17.5449 ms), and that is explicitly not compiler output.

## `mm4.c` — the accumulator-occupancy probe (4 ZA tiles vs 1)

`mmN.c` accumulates into **one** ZA tile. `mm4.c` is the same GEMM accumulating into **four**, in
a 2×2 arrangement — a 32×32 output block per panel. Hand-written, f32, 1 thread, min-of-7, values
identical (`c[0]=1172.0831` in both):

| N | 1 tile | **4 tiles (2×2)** | gain | numpy 1t | gap: 1 tile → 4 tiles |
| ---: | ---: | ---: | ---: | ---: | --- |
| 1024 | 423 GFLOP/s | **777 GFLOP/s** | 1.84× | 1655 | 4.1× → **2.1×** |
| 2048 | 237 GFLOP/s | **619 GFLOP/s** | 2.61× | 1632 | 6.9× → **2.6×** |

```sh
clang -O3 -march=armv8-a+sme2 -o mm4 mm4.c && ./mm4 1024 7
```

**Two independent reasons it wins**, and both matter for how the emitter should be written:

1. **Dependency chains.** With one accumulator every `fmopa` depends on the previous one and the
   loop is latency-bound. Four tiles give four independent chains.
2. **Operand traffic.** 2×2 feeds 4 MACs from 4 loads (1 load per MAC); one tile feeds 1 MAC from
   2 loads. That is also why the arrangement should be the **most-square** factorization of the
   tile count and not a taste call — 1×4 would need 5 loads for the same 4 MACs. It generalizes:
   8 tiles (f64) ⇒ 2×4.

**The second loss is still visible in this probe and is a different problem:** 777 GFLOP/s at 1024
decays to 619 at 2048 even with all four tiles. That is cache, not accumulator occupancy — the
missing k-panel blocking, which `emit_tile_packed_kc` already implements for NEON.

So the two headroom items are independent and both measured: accumulator occupancy is worth
~1.8–2.6×, and cache blocking is what remains after it.

---

# S42 probes — the ceiling, two non-payments, and KC sized

Full write-up: **`docs/performance/s42-sme-roofline.md`**. Six files, all f32, 1 thread, M4 Pro.

## READ THIS FIRST — what a standalone probe can and cannot settle

Sapir's bound on every number in this directory:

> *"it is not a good measure — because we don't really know how it will act with ALL optimizations
> that already exist together. Maybe with the existing optimizations this jumps to more than what
> you see, and maybe scales better too on a threaded environment — so while this test gives a
> standalone result, it doesn't fully apply to a fully integrated optimization/pipeline."*

So a standalone **win is a floor**, and a standalone **null is not a refutation**. Settle both in
the emitter, threaded, at scale. `sme_pack_ab.sh` is the integrated instrument; the `.c` files price
and diagnose.

| File | Question | Result |
| --- | --- | --- |
| `roofline.c` | what is the `fmopa` ceiling with **zero** memory traffic? | **~2000 GFLOP/s** on one unit, reproduced in 4 processes |
| `units.c` | **how many matrix units does this part have?** | **exactly 2**, ~2000 GF/s each, **~4100 aggregate**. Flat from 3 threads on, `per_thread × n ≈ 4100` throughout |
| **`loadcost.c`** | **what does a load actually cost?** | **THE FINDING.** 5% when it hits L1; throughput **halves at the first L2 miss**. The load *count* is nearly free ⇒ 1-load-per-`fmopa` is **not** a ceiling |
| `mm4p.c` | does software-pipelining the k loop help? | **+0.6%** — and one of its two arms **did not survive compilation** |
| `mv.c` | does folding 4 `ld1w` into 2 (`ld1w x2`) help? | **+1.8%** — now explained: loads were never the bottleneck |
| `bp.c` | does packing B panel-major help? | 1.885× at 2048 — but the emitter **already packs B** |
| `pipe2.c` | does unrolling help the **packed** kernel, done properly? | **1.001–1.002×, overlapping**, 1024/2048/4096 |
| `kc.c` | how much is KC blocking worth at 4096? | 1.448× standalone — **did NOT transfer**: the emitter gets +6.1% at 1 thread, −25.5% threaded, and even the optimum DEPTH differed (512 vs 1024) |
| `bslice.c` | is the b layout (whole-k slice vs kc repack) the gap? | **1.065×** — no |
| `smcost.c` | what does the streaming-mode ABI cost per call? | **1.0 ms** over 131072 calls — no |
| `packcost.c` | does the A pack's memory order break under blocking? | **no** — same loops in C: 8.46 ms unblocked, 8.05 blocked |

## THE FINDING — `loadcost.c`

Compute held exactly constant (four independent `fmopa` into the four f32 ZA tiles, every iteration);
only the operand source varies:

| operands from | 0 loads | 1 | 2 | 3 | 4 loads |
| --- | ---: | ---: | ---: | ---: | ---: |
| **32 KB buffer (L1-resident)** | 1956.7 | 1913.7 | 1928.9 | 1910.0 | **1864.2 (95%)** |
| **64 MB buffer (past L2)** | 1915.5 | 941.8 | 841.8 | 755.3 | **760.8 (40%)** |

| | GFLOP/s |
| --- | ---: |
| the emitted kernel today | 1043 |
| Accelerate, 1 thread | 1655 |
| **4 loads, operands L1-resident** | **1864** |

**~1.79× is available and it would pass Accelerate.** The gap is *operand cache residency* — not
instruction count, not scheduling, not the ZA-tile ratio, not silicon. See
`docs/performance/s42-sme-roofline.md` §5e.

```sh
clang -O3 -march=armv8-a+sme2 -o roofline roofline.c && ./roofline 1024 9
clang -O3 -march=armv8-a+sme2 -o pipe2    pipe2.c    && ./pipe2 4096 15
clang -O3 -march=armv8-a+sme2 -o kc       kc.c       && ./kc 4096 9
benches/sme/sme_pack_ab.sh                                  # the INTEGRATED A/B (packed vs --no-pack)
```

## The finding that matters: KC blocking, N=4096, 9 alternating runs

| KC | median ms | GFLOP/s | % ceiling | working set |
| ---: | ---: | ---: | ---: | ---: |
| 256 | 162.622 | 845.1 | 42% | 64 KB |
| **512** | **124.813** | **1101.2** | **55%** | **128 KB** |
| 1024 | 131.310 | 1046.7 | 52% | 256 KB |
| 2048 | 155.278 | 885.1 | 44% | 512 KB |
| 4096 | 180.679 | 760.7 | 38% | 1024 KB ← no blocking |

Unimodal (256 is *worse* than 512, so there is a real optimum), **disjoint** against unblocked, and
1101.2 at N=4096 lands level with 1089.0 at N=1024 — the size decay is gone. It wins *despite*
paying the ZA read-modify-write that blocking forces, because the kernel stores ZA rather than
accumulating into `c`.

## `sme_pack_ab.sh` — B packed vs unpacked, in the real emitter

Same source, target, and face on both legs; the only difference is `--no-pack`. Verified from the
emitted call: `(bn=t, bj=t·k)` is the packed arm, `(bn=b.ck, bj=t)` is not. `MAPAL_PAR=1`, 21
alternating runs, values identical before any timing, commit `06ac50a`:

| N | B packed | B unpacked | worth | distributions |
| ---: | ---: | ---: | ---: | --- |
| 512 | 775.5 | 862.0 | 0.900× | **overlap — do not quote** |
| 1024 | 1043.3 | 1024.6 | 1.018× | **overlap — do not quote** |
| 2048 | **1003.8** | 643.3 | **1.560×** | **disjoint** |

## Two traps these probes cost, recorded so they are paid once

**1. Warm the clock (rule 14).** The same unchanged binary measured the `roofline.c` ceiling at
**1.852 ms** cold and **1.069 ms** warm — **1.73× on identical code**. Best-of-N does not save you:
a first run's whole sample set is cold. Every probe from `mv.c` on spins `fmopa` for 300 ms before
the first timer and interleaves its variants.

**2. Check the transformation survived (rule 15).** `mm4p.c`'s `panel_rotate` wrote "k+1's loads,
then k's `fmopa`"; LLVM emitted the exact inversion at both `-O2` and `-O3`, putting every load
immediately before its consumer across the back edge. That arm measured base against base, and its
null was indistinguishable from a real one. `pipe2.c` keeps only arms verified in `-S` output.

**Method fixed in `pipe2.c`/`kc.c` and missing from the earlier three:** ≥15 alternating runs with
**medians** and an explicit overlap check (not min-only), `c` zeroed before every timed region so a
skipped panel cannot inherit the previous variant's correct output, and an **independent scalar
`fmaf` reference** over 97 cells — so the gate proves `A·B` rather than mutual agreement. `mm4p.c`,
`mv.c` and `bp.c` predate these and their sub-10% rows are **not results**.
