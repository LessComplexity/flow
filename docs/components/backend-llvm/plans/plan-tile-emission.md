# Plan — tile emission v1: `tile_plan` + the register micro-kernel (S25 item 2b)

Status: **SHIPPED S25** (v1 matmul kernel + v2 affine/1-D generalization; commit
`be4e827`). Deviations from the plan as written: (1) recognition fires on
**rewritten** IR only — raw lowering wraps the constant seed in a capture `Proj`
(bench recipe is `--rewrite`; the differential gates both forms, raw taking the
untiled fallback); (2) T6 tightened to `Constant` seeds (non-const = recorded
ceiling); (3) address `Add` accepts any nesting/order via the affine walker, but
`Sub`/var×var reject. Measured + shape-corpus results: `docs/performance/matmul/s25.md`.
Original text below.
Family: region-emission (ir owns legality once; each backend refines with its own cost
model — the `region_plan` principle, here instantiated for iteration-space structure).

## Why (evidence, S25 local + S24 box)

- Suggestion #9 is **already shipped** (S20c capture-range recursion, `algo.rs:1611`):
  current matmul emission has ZERO fold-body guards (6 `icmp` = loop conditions only,
  verified on regenerated `matmul512_cap.ll` ≡ committed). s24.md reading 6's gap
  attribution is stale; corrected this session.
- Local M-series A/B @512 f32: flow `MAPAL_PAR=1` ≈ 80 ms ≈ cpp single-thread 89 ms —
  **scalar parity already holds**; the box ≤512 gap = spawn floor + 8× oversubscription
  (plan-s25-pool-timer), not instruction residue.
- Disassembly: both flow and cpp are scalar-FMA (no vector registers). The per-cell
  fold is a strict recurrence — **no compiler can vectorize it without reassociation**
  (forbidden: oracle bit-parity). The only bit-exact route to SIMD is **interleaving
  cells** (independent accumulator lanes), which is exactly iteration-space tiling.
  SIMD ≈4–8× is the next rung; cache/register blocking beyond it is the BLAS constant.

## Categorical model (Dat + Trn)

The map site over a flattened 2-D space is one `Trn` placed at many points; the graph
already knows the structure (`t ↦ (t/C, t%C)` decomposition, affine reads over captured
arrays — the reuse IS fanout, `notes/graph-advantage.md`). Tiling changes the **order
the placements execute**, never a per-cell value chain — a pure re-scheduling of a
fanout, R1-licensed by construction.

| Item | Kind | Model |
| --- | --- | --- |
| `TilePlan` | `Dat` (deduced, BL7) | `tile_plan(f) → { sites: MorphismId ⇀ TileSite }`; never stored |
| `TileSite` | `Dat` | `{ rows, cols C, depth K, a: (arr, coeff Ca), b: (arr, coeff Cb), seed slot, elem ty }` — geometry + operands only; **tile factors are the backend's** (cost model per backend, as region-plan) |
| recognition | `Trn` (analysis) | conditions T1–T6 below; partial — unmatched sites simply absent |
| micro-kernel | `Trn` placement | same per-cell `fmul`/`fadd` chain, k-ascending per cell, emitted as k-outer / j-inner over `TJ` accumulator lanes |

### Legality (T1–T6) — all must hold or the site is not in the plan

- **T1** site is `Map { body: B, captures ≥ 1 }`, element source `Iota(M)`, `M = rows·C`.
- **T2** `B` computes `i = elem / C`, `j = elem % C` (literal `C`, both present), contains
  exactly one `Fold { body: F }` whose array is a captured `Iota(K)` used as `k`, and
  `B`'s output **is** the fold output (no post-processing).
- **T3** `F`'s output = `Add(acc, Mul(x, y))` (operand order recorded and reproduced
  exactly) with `{x,y} = { Index(A, i·Ca + k), Index(B₂, k·Cb + j) }` — one read j-free,
  one j-stride-1; affine coefficients literal.
- **T4** every `Index` in `F` proven by `bounds_proof`; no other trap-capable op in
  `B`/`F` (Div/Mod only by nonzero literals) → the micro-kernel carries **zero trap
  machinery** (guard-free is a precondition, not a hope).
- **T5** acc/element types identical (f32/f64/int widths uniform).
- **T6** the fold seed is a scalar operand available at the site (any producer).

### The bit-exactness theorem (why R1 holds at -O0)

For each cell `(i,j)` the emitted sequence is `acc ← fadd(acc, fmul(a[i·Ca+k], b[k·Cb+j]))`
for `k = 0..K-1` ascending — identical ops, operand order, and k-order to today's
per-cell loop. Tiling only interleaves **different** cells' chains (pure map: output
slots disjoint, order unobservable). Stdout is byte-equal at any `TJ`, any thread
count, any opt level. (Same argument shape as the S24 schedule-invisibility tests.)

## Emission (backend-llvm, both sequential and task flavors)

At the bulk-site loop, when `tile_plan` has the site (else today's path unchanged):

```
for i in i_lo..i_hi:                      ; task range [lo,hi) clipped per row:
  jw = [max(lo−i·C,0), min(hi−i·C,C))    ;   GRAIN-agnostic — mid-row splits legal
  for j0 in jw step TJ:                   ; TJ = 16 initial; measured locally
    tj = min(TJ, jw_hi − j0)
    acc[0..tj] ← seed                     ; alloca [TJ × T], clang promotes
    for k in 0..K:
      a_ik ← A[i·Ca + k]                  ; j-invariant scalar load
      for t in 0..tj:                     ; ← clang auto-vectorizes (independent lanes,
        acc[t] ← fadd(acc[t], fmul(a_ik, B₂[k·Cb + j0 + t]))   ;    contiguous B₂)
    for t in 0..tj: out[i·C + j0 + t] ← acc[t]
```

No ISA knowledge in the emitter (textual LLVM, clang vectorizes) — project style.
`EmitOpts { tiling: bool }` default **on**; `--no-tile` on the example for A/B legs only.

## Work packages

- **WP-T1 (mapal-ir):** `tile_plan` + tests: matmul shape recognized (512/1024, f32/f64);
  refusals pinned (post-processed output, unproven index, j-stride≠1, no div/mod pair,
  captured-expression seed ok). Property: recognition never fires on testgen programs
  unless conditions hold (spot via targeted builders, not the random suite).
- **WP-T2 (backend-llvm):** micro-kernel emission at the shared bulk-site point
  (sequential + `@task` flavors); range clipping; goldens re-pinned for matmul-class;
  one new golden pinning the tile nest shape (`tile_nest_shape`).
- **WP-T3 (gates + measure):** full differential -O0/-O2 (the real gate); `emit_sweep`
  640/640; local A/B (tile on/off, disasm shows vector FMA, times recorded); box sweep
  in the standing matrix format.

## v2 — the shape family (Sapir, S25 in-session: "not only matmul — fir, scan,
## convolution, attention-shaped patterns; verify the optimizations follow")

The matmul kernel's real structure is two loads with **lane-strides {0, 1}** feeding
one fma — not anything 2-D-specific. Generalization (WP-T1b/T2b, after the matmul
kernel lands):

- **Reads as affine triples.** `TileSite` reads become `base + cᵢ·i + cₖ·k + c_lane·lane`
  with `c_lane ∈ {0,1}`, one read lane-invariant, one lane-stride-1 — replacing the
  hardcoded `(i·ca+k, k·cb+j)` pair. Bounds side already general (`bounds_proof`).
- **1-D lane mode.** No div/mod decomposition ⇒ lanes are `t` itself (`rows=1, c=M`).
  Covers **FIR/conv1d**: `w[k]` lane-invariant ≡ matmul-a; `x[t+k]` lane-stride-1 ≡
  matmul-b. Same micro-kernel, different address formulas.
- **Shape corpus** (`benches/shapes/`, oracle + perf sizes): `fir_{256,65536}`,
  `attn_{16,256}` (S=Q·Kᵀ, O=S·V — two *chained* recognized sites; kt column-major;
  no softmax — `exp` absent from the Core op set, recorded for Sapir),
  `attn_256_rowmajor` (score site refused: lane-stride-D read; O site still tiles —
  mixed program), `conv2d_{16,512}` (refused: k-decomposition inside the fold makes
  reads non-affine in raw k; guards still elided via `bounds_proof`). Oracle values
  pinned locally: fir `991/-1484`, attn `299680/184913`, conv2d `240/-1154`.
- **Scan**: cross-cell dependence — structurally a loop, not a pure map of folds;
  tiling N/A; SIMD scan = the log-step/block-scan algorithm as a future rewrite.
- **Optimization-follows verification (the directive's first half):** for every
  recognized shape, compile the tiled `.ll` at `-O2 -ffp-contract=fast`, disassemble,
  and assert vector FMA presence (NEON `fmla.4s`/AVX `vfmadd`) + measure tile-on vs
  `--no-tile` A/B. A shape that recognizes but fails to vectorize is a finding, not
  a pass.

## Ceilings (recorded, not built)

TI>1 i-blocking (b-tile register reuse), A/B packing, k-panel L2 blocking — the rest of
the BLAS constant; only after TJ SIMD is measured. Transposed reads (attention row-major
K — needs packing). Derived-var affine forms (conv2d's `k/3`,`k%3`). Non-constant fold
seeds. `exp`/transcendentals for real softmax (language question for Sapir). cuda
consumption of `tile_plan` (streams item rides first).
