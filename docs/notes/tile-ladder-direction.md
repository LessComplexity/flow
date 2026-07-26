# The tile ladder — standing direction (Sapir, S25 close)

**The target, verbatim intent: cuBLAS/cuDNN-class performance out of the box, from
naive intuitive source, on every backend — "you can just use it and it will be
optimal for all uses. Even for FPGAs/ASICs by detecting those properties out of
the box."** No special libraries; the graph analysis deduces what the libraries
hand-encode. This note records the S25 close discussion so later sessions inherit
the concepts without re-derivation. Companion: `graph-advantage.md` (the founding
bet), `plan-tile-emission.md` (the shipped rung).

## What the detector actually is (general, not matmul)

The graph gives seq and par **by node meaning**, free: map-over-counted-range =
"these M results are independent" (par); fold in the body = "this chain is
order-pinned" (seq). The only analysis is address regularity: walk each read's
address arithmetic (adds + multiplies-by-constants = affine) and ask *"how does
the address move when the neighboring lane moves?"* — moves 0 = broadcast; moves
1 = vector load; else refuse. One broadcast + one unit-stride + proven bounds +
nothing skippable-that-traps = a recorded tile site (lane count, chain depth,
per-read coefficients). Matmul/FIR/attention matched ONE rule; no matrix concept
exists in the recognizer. This is `path_plan`'s principle one level deeper: paths
across nodes → threads (S24); independent lanes inside a bulk node → SIMD (S25).

## The recorded facts ARE the optimization menu

Every zero coefficient in a recorded address formula = "this load is reused
across that variable" — reuse is fanout the graph can see:

- lane-coefficient 0 → broadcast across lanes (**shipped**: the TJ tile);
- row-coefficient 0 on the other read → the same vector serves every row →
  **register blocking** (keep TI rows' accumulators in registers; each fetched
  vector feeds TI rows: ~3× less memory traffic at TI=4). The register file's
  size is the budget that caps TI×TJ. No new graph analysis — the emitter cashes
  a fact the record already holds.

## Per-backend width (correcting v1)

mapal-ir records geometry only; **the backend owns tile factors** (region-plan
principle). llvm's TILE_J=16 is a v1 constant, not doctrine: per-target +
per-element-width (f64 wants half; AVX-512 vs NEON differ; cuda wants warp-shaped
factors; Verilog its own). The fixed-width split (compile-time-constant inner
loop + scalar tail) is about *known* length — compilers fully vectorize known
lengths — whatever the number is.

**The backend-genericity contract (Sapir, S29).** Every optimization rung lands
as either (a) a generic graph fact in a mapal-ir query (e.g. `TileRead.ksplit` —
no machine constants) or (b) emitter-local cashing with zero mapal-ir change
(gates, emission, backend-owned constants). mapal-ir never learns machine facts;
backends never re-derive graph analysis. CUDA/Verilog consume the SAME record:
smem tiles = the pack `DataLoc` at another location, `mma` = another parallel
`TrnLoc` (tf32 parity split = the one policy item, S24b precedent).

**Amendment 2026-07-25 — the two halves of "schedule", and the one unmeasured
claim** (review record: `docs/notes/2026-07-25-thesis-review.md`; decisions:
ADR-0033, ADR-0034).

*Geometry vs constants.* What this note calls the schedule is two things with
different portability, and conflating them is what makes the ladder read as
"matmul tuning" to an outside reader:

- **Geometry** — which reads broadcast vs. stride, which axes split, the nest
  order, what is legally interleavable. **Deduced**, exact, backend-independent.
  **Proven portable across shapes**: one affine rule, no matrix concept, and S28
  carried the matmul ladder to a FIR window rung (zero mapal-ir change) and a
  conv2d micro-kernel in a single session, both winning.
- **Constants** — TJ/TI/KC/NC, unroll depth, prefetch distance, `GRAIN`. Facts
  about a cache hierarchy and a register file, **not** about the program. Today
  hand-set literals swept on one machine and applied on every target (local M4
  Pro NEON/14T and the EPYC box zen3/AVX2/61T run identical numbers).

Therefore the residual OpenBLAS gap is **not** evidence the geometry is wrong —
it is substantially evidence the constants are untuned for the target, and
tuning them is a *search*, which is itself backend-generic (ADR-0034). Standing
phrasing: **the geometry is deduced and portable; the constants are measured per
target.** Never describe this work as "matmul performance" — it is geometry
recognition validated on matmul, and BLAS is the oracle for whether the geometry
is right, not the product.

*The unmeasured claim.* Verified 2026-07-25: `tile_plan` has exactly one consumer
(`backends/llvm/{lib,func}.rs`; zero hits in `backends/cuda/src/`). "CUDA/Verilog
consume the SAME record" above is a design intent that has never been executed —
every rung since S25 was written against a single `Loc`. Sapir's sequencing
(2026-07-25) is CPU to full advantage first, then GPU, so the CUDA leg is the
**named exit condition of the CPU phase** (ADR-0033 D1), with a per-rung paper
guard in the meantime: each rung's plan doc states which record fields it
consumes, its CUDA realization named against the record — or `cpu-local` if it
has none — and any machine fact the record does not carry (ADR-0033 D2).

## The ladders per target (each rung measurable, R1-licensed unless said)

**CPU:** ✅ SIMD lanes (2.5–4.6×, S25) → TI register blocking (~2–4×) → packing +
k-panels (~1.5–3×) → **tuned constants (ADR-0034 — the rung that is currently
missing entirely: every factor above is a literal swept on one machine)** →
numpy-class. Gap today 10–14× (f64, box). This ladder's completion is what
"full advantage on CPU" should mean (ADR-0033 Q1 — the bar is still unwritten).

**CUDA:** cuda consumes the SAME `tile_plan` → shared-memory tiles → **`mma`
(tensor cores)**. Precision is arch-specific: **tf32 mma exists on Ampere+
only** (sm_80/86/89/90; 8-exp/10-mantissa inputs, f32 accumulate) — consumer
cards have tf32 but **no f64 tensor cores** (f64 = CUDA cores, 1/64 rate);
**f64 DMMA is datacenter-only** (A100/H100). Either way the fragment summation
order is hardware-fixed, so mma breaks oracle bit-parity by construction and
lands **product-face only** (rel-tol gated, the S24b fmad precedent) while the
conformance face keeps the SIMT path. This mirrors the industry contract:
cuBLAS/cuDNN default to tf32 for f32 gemm/conv on Ampere+ (exact fp32 is
opt-out). Second caveat: cuBLAS = mma + memory choreography (smem staging,
swizzles, double-buffering) — the GPU siblings of register blocking and
packing; rungs, not one jump.

**conv2d / cuDNN:** refused today only because the walker is affine-in-raw-k; the
`k/C`,`k%C` split inside the fold is the SAME derived-var move the map body
already gets. Extension → direct tiled conv. The bigger move — **conv→matmul
rewrite (implicit GEMM, cuDNN's core trick) — is a graph rewrite on the same
recorded facts.** Winograd-class = later rung, same family.

**FPGA/ASIC (P7):** the tile record maps to hardware structure natively — the
order-pinned fold chain = pipeline stages, independent lanes = parallel PEs, the
broadcast/unit-stride split = wiring — i.e., **a recognized site is a systolic
array specification**. Aligns with the standing E1/Verilog restriction
(feedforward + single-loop FSM). Unproven until P7; the record is designed for it.

## The claim, honestly bounded

The edge is real and measured where the shapes match (GEMM/conv/attention ARE
today's money shapes): naive source beat chapel 3–8.6× multicore (S25), no
annotations, no libraries — because the graph exposes by construction what other
IRs can't see (lane independence, shared reads, pinned chains, no aliasing).
Outside recognized shapes we are at parity-class, not ahead — the claim "optimal
for all uses" is earned shape-family by shape-family, each with a differential
gate and a number. That is the method: **deduce the transform family libraries
hand-encode, prove it bit-exact or gate it product-vs-conformance, measure the
rung.**
