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

flow-ir records geometry only; **the backend owns tile factors** (region-plan
principle). llvm's TILE_J=16 is a v1 constant, not doctrine: per-target +
per-element-width (f64 wants half; AVX-512 vs NEON differ; cuda wants warp-shaped
factors; Verilog its own). The fixed-width split (compile-time-constant inner
loop + scalar tail) is about *known* length — compilers fully vectorize known
lengths — whatever the number is.

## The ladders per target (each rung measurable, R1-licensed unless said)

**CPU:** ✅ SIMD lanes (2.5–4.6×, S25) → TI register blocking (~2–4×) → packing +
k-panels (~1.5–3×) → numpy-class. Gap today 10–14× (f64, box).

**CUDA:** cuda consumes the SAME `tile_plan` → shared-memory tiles → **`mma`
(tensor cores)**. Two honest caveats: (1) tensor cores reorder accumulation and
f32 mma computes in tf32 — **breaks oracle bit-parity; fmad-class decision, bigger:
product recipe uses them, conformance gate doesn't** (precedent: Sapir's S24b fmad
split; ratify when reached). (2) cuBLAS = mma + memory choreography (smem
staging, swizzles, double-buffering) — the GPU siblings of register blocking and
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
