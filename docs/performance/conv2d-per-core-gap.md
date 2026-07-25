# conv2d's per-core gap — five eliminated causes, and what is left

**Status: OPEN.** Flow's conv2d kernel is **1.55× slower per core** than the naive C++ baseline
and the mechanism is **not identified**. This file exists so the next session does not re-run
the experiments that already came back negative.

Basis: M4 Pro (10 P + 4 E), clang 22.1.8, `-O2 -march=native -ffp-contract=fast`, conv2d 3×3
over 1024×1024 f32, compute-only via the `time` builtin, `FLOW_PAR=1`, min-of-N.
**Order-checked**: running either binary first changes the *median* by 2–6% (frequency ramp
carries between processes) and the **minimum not at all**, so min is the only figure quoted.

| leg | min ms |
| --- | ---: |
| flow, tiled + row-blocked (shipped) | 0.395–0.426 |
| cpp, naive triple loop | **0.256** |
| flow, `--no-tile` | 3.411 |

## What is NOT in question

- **Our optimizations work.** Tiling + row blocking is **8.6×** over our own untiled path
  (3.411 → 0.398). The 1.55× is a residual on top of a large win, not a failed rung.
- **Our scheduling is better than the baseline's.** C++ spawns 14 `pthread`s *inside* its
  timed region every iteration, one static row-block each, no pool, no balancing, no stealing.
  We have a parked persistent pool with quantised over-decomposition. C++ is doing strictly
  less and still wins per core, so the gap is in the kernel.

## The two inner loops, isolated by back-edge

Not by symbol — C++'s `conv_range` is inlined into `_main`, so both bodies were located by
finding backward branches and taking the innermost range containing `fmla`.

| | body | outputs/trip | fmla | vec loads | **instr/output** | spills |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| cpp | 24 instr | 4 | 9 × `fmla.4s` | 9 | **6.00** | 0 |
| flow | 274 instr | 64 | 144 × `fmla.4s` | 53 | **4.28** | 10 |

**We issue 30% fewer instructions per output and less than half the loads — and lose by 55%.**

### Cycle accounting against each kernel's own floor

| | FMA floor (÷4 FP pipes) | load floor (÷3 per cycle) | actual | off its floor |
| --- | ---: | ---: | ---: | ---: |
| cpp | 2.25 cyc | **3.0 cyc** | 3.9 cyc | **1.3×** |
| flow | **36 cyc** | 17.7 cyc | 104 cyc | **2.9×** |

C++ is **load-port bound** and runs at 77% of that ceiling. We are bound by neither: 34% of FP
capacity, 17% of load capacity, with 16 independent accumulator chains (so not ILP-starved
either). **The gap is stalls, not instruction mix** — which is why issuing fewer instructions
does not help.

## Five hypotheses, all refuted by measurement

| # | hypothesis | test | result |
| --- | --- | --- | --- |
| 1 | Register pressure — 16 accumulator regs + 9 weights + b tiles spill | forced `TI=2` | **refuted.** Spills 10 → **0**, and it got **slower** (0.446 vs 0.426). The spills cost less than the reuse lost |
| 2 | We splat the weights; C++ uses by-element FMA | disasm of both | **refuted.** Both emit `fmla.4s vd, vn, vm[i]`, **zero** `dup`/`ins`. LLVM folds our splats |
| 3 | Arithmetic intensity | load counting | **refuted.** We load **half** as much per output; row blocking's 2× reuse is real (FMA:load 0.80 → 1.20) |
| 4 | Heap-lowered arrays alias — `a_ptr`/`out_ptr` both loaded from the frame, so weight loads cannot be hoisted past stores | raised `heap_min_bytes` so arrays became distinct `alloca`s | **refuted.** Identical 69 scalar weight loads, marginally slower (0.476) |
| 5 | Stores block hoisting for want of alias information | added `!invariant.load` to all 112 weight loads in the emitted `.ll` and recompiled | **refuted.** Byte-for-byte the same 69 loads, same time. LLVM already had permission and still did not hoist |

### The register-budget model, checked and found not to predict a violation

The budget is `TI × regs_per_acc + broadcast + b_tile ≤ vec_regs`, where
`regs_per_acc = TJ × sizeof(elem) / vec_bytes`. For conv2d f32 on NEON: `16 + 9 + 4 = 29 of
32`. **Under budget.** The formula never predicted the spill — the spill was observed first and
the model reverse-engineered onto it. Hypothesis 1 then falsified the causation directly. A
`broadcast` term was drafted for `TargetProfile::tile_i` and **not shipped**, because it would
have been justified by a refuted model.

### A methodological correction worth keeping

Several of the above were argued from **static** instruction counts (e.g. "69 weight loads in
`task7`"). A static count cannot distinguish *hoisted into a preheader, executed once per outer
trip* from *executed every inner trip*. Only the back-edge-isolated inner body (274 instructions)
is dynamically meaningful, and there the weight loads number ~5. The weight-reload story was
probably never large enough to explain 1.55×.

## What is left

Every surviving explanation is a **stall attribution** — cache behaviour, TLB, store buffer,
frontend — and separating them needs counters, not disassembly.

**`xctrace` now works on this machine** (Instruments 16.0, `CPU Counters` template, "CPU
Bottlenecks" mode). It records per-sample PMC arrays; the first two counters are cycles and
instructions (deltas give IPC ≈ 1.4 for the whole flow process), and the template also carries
Instruction Delivery / Processing / Discarded / Useful bottleneck breakdowns.

**The blocker is sampling density, and it is ours to fix:** the conv2d kernel is ~0.4 ms inside
a ~700 ms process dominated by data generation, and sampling is 1 kHz — about **one** sample
lands in the kernel. Before counters can attribute anything:

1. **A repeat-loop bench.** `benches/shapes/*.flow` run their kernel once. A variant that runs
   the kernel N times inside the timed region (or a driver that does) would give thousands of
   in-kernel samples. This is the prerequisite for every counter measurement, not just this one.
2. Then record both binaries and compare cycles, instructions, IPC and the bottleneck split.
3. **The alternative that needs no new bench:** run the same comparison on the Linux box, where
   `perf stat` reports cycles / instructions / L1-dcache-misses / dTLB-misses directly and per
   process. That would also validate the `zen3` profile, which is still untested on hardware.

## Leads not yet tested

- **Access-pattern width.** Our blocked tile touches 6 image rows + 4 output rows per inner
  trip, striding ~4 KB (1026 f32); C++ touches 3 + 1. Ten concurrent ~4 KB-strided streams is
  the classic L1 set-conflict pattern. **Weak counter-evidence already exists**: `TI=2` (6
  streams instead of 10) was *slower*, which argues against a simple stream-count story.
- **Two dead instructions per trip, in both kernels.** Each re-zeroes its accumulators
  (`movi.2d v4, #0`) and then adds into them, instead of folding tap 0 into an `fmul`. Ours is
  16 of 274 (~6%), C++'s 1 of 24 (~4%). Real but small, and an easy emitter change in
  `emit_conv_block_tile`.
