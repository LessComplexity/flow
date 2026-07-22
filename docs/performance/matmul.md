# Matmul (GEMM) — all measured runs

Square `c[i][j] = Σₖ a[i,k]·b[k,j]`. Times in ms unless noted. ✓ = output cross-verified at that size (see Verification).

**READ THIS FIRST — two measurement kinds:** **(a) per-iteration compute** = CUDA-event timing around the kernel / best-of wall around the library call (startup EXCLUDED — naive CUDA, cuBLAS, numpy, rust, cpp, chapel, and the flow-cuda **kernel** legs). **(b) process wall** = the whole binary, startup INCLUDED (the other flow legs; CUDA context startup ≈ 270 ms on this box, llvm binaries ≈ 1.5–2.4 ms floor). At N≤512 the flow-cuda wall legs are startup-bound — compare like-with-like only. **Boxes differ:** CPU legs (llvm/cpp/rust/chapel) move between boxes with the CPU — S23's znver3 shifted f64 down and rust up vs S21's host; compare within one table, never across archives.

## Main table (S23, 2026-07-22 — post minimal-emission (S22 WP-A/B/C) + WP-D hoisting + the Fold force-Named fix; first hardware run of those emitters) — time by backend and size

| N | flow-cuda loop (wall) | flow-cuda cap (wall) | **flow-cuda cap kernel f64/f32 (compute)** | flow-llvm loop (wall) | flow-llvm cap f64 (wall) | flow-llvm cap f32 (wall) | naive CUDA (compute) | cuBLAS (compute) | chapel f64 (compute) | cpp f64 (compute) | rust (compute) | numpy (compute) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 4 | 279.8 ✓ | — | — | 2.37 ✓ | — | — | — | — | — | — | — | — |
| 16 | 418.1 ✓ | 274.8 ✓ | 0.193 / 0.202 | 2.39 ✓ | 1.77 ✓ | 2.23 ✓ | — | — | — | — | — | — |
| 32 | 1,274.2 ✓ | — | — | 2.57 ✓ | — | — | — | — | — | — | — | — |
| 64 | 8,515.8 ✓ | 278.4 ✓ | 0.222 / 0.200 | 6.58 ✓ | **2.69 ✓** | 1.49 ✓ | 0.0031 ✓ | 0.0069 ✓ | 0.0740 ✓ | 0.3418 ✓ | 0.1650 ✓ | 0.0057 ✓ |
| 128 | 68,937 ✓ | 266.2 ✓ | 0.217 / 0.209 | — (skip: znver3 clang stall) | **8.08 ✓** | 3.03 ✓ | 0.0047 ✓ | 0.0043 ✓ | 0.1880 ✓ | 2.004 ✓ | 1.582 ✓ | 0.0302 ✓ |
| 256 | — | 271.8 ✓ | 0.264 / 0.215 | — | **69.92 ✓** | 40.53 ✓ | 0.0120 ✓ | 0.0063 ✓ | 1.051 ✓ | 38.37 ✓ | 13.94 ✓ | 0.0902 ✓ |
| 512 | — | 272.3 ✓ | **0.706 / 0.318** | — | **436.8 ✓** | 311.6 ✓ | 0.0782 ✓ | 0.0118 ✓ | 6.435 ✓ | 377.5 ✓ | 183.7 ✓ | 0.1994 ✓ |
| 1024 | — | — | — | — | — | — | 0.7849 ✓ | 0.0431 ✓ | 206.5 ✓ | 3,249 ✓ | 3,275 ✓ | 1.039 ✓ |
| 2048 | — | — | — | — | — | — | 6.718 ✓ | 0.3021 ✓ | — | — | — | 4.440 ✓ |
| 4096 | — | — | — | — | — | — | — | 2.326 ✓ | — | — | — | 47.31 ✓ |

**The S23 firsts:** the whole matrix comes from ONE box in one run (chapel appended post-install, CPU-only), over emitters whose text the S22 minimal-emission mandate reshaped — the first hardware verification of that text (15/15 differential green), and it caught one real S22 bug pre-sweep (Fold force-Named, `acdb319`). Chapel legs are first-time f32+f64 at five sizes.

## Ratios

| Comparison | Value | Note |
|---|---|---|
| **flow-cuda GEMM kernel alone (k0_4) vs naive-CUDA, N=512 f32** | **1.60× slower** | 0.1249 / 0.0782 — the closest Flow has ever been; the 0.318 sum carries ~0.155 ms one-time module load |
| flow-cuda kernel sum vs naive-CUDA, N=512 f32 | 4.07× slower | 0.3184 / 0.0782 — module load dominates the sum (see decomposition) |
| flow-cuda kernel sum vs naive-CUDA, N=512 f64 | 9.0× slower | 0.706 / 0.0782 (k0_4 alone: 6.7×) |
| flow-cuda kernel f64 vs cuBLAS, N=512 | 60× slower | 0.706 / 0.0118 — tiling/tensor-core algorithm gap |
| flow-llvm cap f32 vs single-thread C++ f32, N=512 | **parity** | 311.6 vs 309.3 (as in S21) |
| flow-llvm cap f64 vs single-thread C++ f64, N=512 | 1.16× slower | 436.8 / 377.5 — REVERSED vs S21's 1.30× faster; znver3 + this clang build favor the C++ loop (box variance, not a code change — the `.ll` is S21's modulo nothing) |
| capture vs loop form (cuda wall), N=128 | **259×** | 68,937 / 266.2 |
| chapel f64 vs cpp f64, N=512 | 59× faster | 6.435 / 377.5 — 48-core `forall` vs single thread |
| numpy vs flow-cuda kernel f32, N=512 | 1.6× faster | 0.1994 / 0.3184 — CPU BLAS vs our one square kernel + module load |

## Throughput anchors (GFLOP/s)

| Leg | Peak | At N |
|---|---|---|
| cuBLAS | 59,083 | 4096 |
| numpy | 3,869 | 2048 |
| naive CUDA | 3,433 | 512 |
| **flow-cuda capture kernel f32** | **843** | **512** |
| **flow-cuda capture kernel f64** | **380** | **512** |
| chapel f32 | 63.7 | 256 |
| rust naive | 3.18 | 64 |
| flow-llvm cap f32 | 1.38 | 128 (single-thread CPU) |

## Kernel decomposition (FLOW_PERF, N=512 f32)

Sum 0.3184 ms = `k0_0` first launch **0.155** (CUDA module load — one-time, not the kernel) + `k0_0` second 0.005 (iota dedup pair) + `k0_2`/`k0_3` fill-class maps 0.021 + **`k0_4` the GEMM map kernel 0.1249** + `k0_5` readbacks 0.013. f64 same shape with k0_4 = 0.522. The remaining k0_4 gap to naive CUDA (1.6× f32): `-fmad=false` (oracle-pinned; Sapir's labeled-row question standing) and untuned launch geometry (256/block). The S22 text rework (minimal emission + WP-D hoisting) is confirmed perf-neutral-or-better on hardware — kernel times moved 0.331→0.318 f32 / 0.729→0.706 f64 at 512 vs S21 (different box; treat as ~flat, not a win claim).

## Verification

Outputs `c[0]`/`c[N²−1]` agree across **six independent implementations** (flow-cuda, flow-llvm, naive CUDA, cuBLAS, rust, cpp, chapel — and numpy) at every shared N: 1047/2107 (64), −7312/−933 (128), −3694/10946 (256), −22592/−38634 (512), 11107/91690 (1024), −1045/51275 (2048), 74348/−302529 (4096, cublas=numpy). flow legs additionally interp-oracle-pinned at N=4/16/32 (−275/3748, 1815/6944, −1219/4392). **The remote cuda differential (10 examples + 320 testgen, raw+rewritten) ran green on this box — 15/15 at 16-core pinning — over the S22 minimal-emission + S23 WP-D emitters**; its first run caught the in-twin Fold Inline-classification bug (values silently dropped; 27/640 emissions panicked) which was fixed, pinned, and given a local no-nvcc emission-sweep harness (`emit_sweep.rs`) before any number here was recorded.

## Notes (one line each)

- flow-cuda capture: ONE arena cudaMalloc/free; deduped kernels; trap-free kernel path; minimal-emission text (S22) + hoisted invariant assemblies (WP-D) — hardware-verified this run.
- flow-llvm capture: unchanged since S21 (WP-E assessed and deferred with sroa measurements — see backend-llvm/suggestions #10); CPU legs moved with the box's znver3, both directions.
- flow-cuda loop form unchanged (Θ(N³) launches — the per-op mapping's price; region emission v2 is the planned fix).
- flow-llvm LOOP N=128 leg skipped this box: clang 15 `-O2 -march=native` stalls >25 min cc1 on the loop-form `.ll` (znver3-specific; S21's box compiled it) — killed per adaptive-skip, S21's 63.09 ms stands as the reference point.
- cuBLAS `CUBLAS_DEFAULT_MATH` (no TF32); flow recipe pinned `nvcc -std=c++17 -fmad=false -arch=sm_89`; llvm legs `clang-15 -O2 -march=native`.
- Chapel 2.9.0 (`chpl --fast`, CHPL_TARGET_CPU=native), installed post-sweep (box blocked port-80 apt mirrors — https sources fixed it) and appended; CPU-only legs, measurement-kind safe.
- Box startup ≈ 270 ms this box (S21: ≈ 355; S20: ≈ 313) — wall legs are box-dependent below N≈512.
- 48-vCPU boxes starve the differential's 15 s run timeout via CUDA context-init serialization under full fan-out — pin `cargo test` to ~16 cores (`taskset -c 0-15`); the 3 "divergences" of the first attempt were all this timeout, zero real.

## Method

| Item | Value |
|---|---|
| GPU box | vast.ai RTX 4090 #45539759 (48 vCPU znver3), nvcc 12.4.131, clang 15.0.7, Chapel 2.9.0, rust 1.97.1, numpy (pip) — one box for ALL legs (destroyed after; ≈ $0.42 incl. one recycled `loading` boot) |
| flow legs | S23 artifacts (regen.sh — rewrite-emission + WP-D); process wall min-of-3 with adaptive cap; kernel legs = Σ `FLOW_PERF launch=` CUDA-event times (min of 3) |
| Sources | `benches/matmul/` (v2 generators, `--width f32`), driver `s21_box.sh` (differential re-run 16-core-pinned; bench phase relaunched standalone after the Fold fix) |
| Raw data | `benches/matmul/results.csv` (S23, 84 rows) · `results-s21.csv` (S21 archive, committed pre-sweep) · `results-pre-s20.csv` (S16–S19) |

---

## Archive — S21 main table (2026-07-22 box #45516809; superseded above — CPU legs are NOT comparable across boxes)

| N | flow-cuda loop (wall) | flow-cuda cap (wall) | cap kernel f64/f32 (compute) | flow-llvm loop (wall) | flow-llvm cap f64 (wall) | flow-llvm cap f32 (wall) | naive CUDA | cuBLAS | chapel f64 | cpp f64 | rust | numpy |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 4 | 357.7 | — | — | 1.32 | — | — | — | — | — | — | — | — |
| 16 | 530.2 | 359.1 | 0.234 / 0.233 | 1.41 | 1.32 | 1.90 | — | — | — | — | — | — |
| 32 | 1,727.0 | — | — | 2.05 | — | — | — | — | — | — | — | — |
| 64 | 11,676 | 356.7 | 0.234 / 0.234 | 4.66 | 1.77 | 1.88 | 0.0031 | 0.0099 | 0.0770 | 0.3399 | 0.2616 | 0.0104 |
| 128 | 95,053 | 359.2 | 0.265 / 0.246 | 63.09 | 4.12 | 6.20 | 0.0048 | 0.0052 | 0.1660 | 2.594 | 2.493 | 0.0318 |
| 256 | — | 355.8 | 0.289 / 0.243 | — | 29.20 | 24.31 | 0.0115 | 0.0096 | 1.434 | 37.15 | 19.51 | 0.1080 |
| 512 | — | 357.3 | 0.729 / 0.331 | — | 701.1 | 239.9 | 0.0831 | 0.0118 | 106.7 | 909.0 | 236.2 | 0.4928 |
| 1024 | — | — | — | — | — | — | 0.8370 | 0.0443 | 589.7 | 7,390 | 6,819 | 1.807 |
| 2048 | — | — | — | — | — | — | 7.097 | 0.3115 | — | — | — | 94.90 |
| 4096 | — | — | — | — | — | — | — | 2.344 | — | — | — | 422.0 |

S21 context kept: first llvm cap legs at N≥128 (ADR-0029 procedural sources + WP3b); S21's flow-llvm-cap-f64-beats-C++ ratio was real on that box and reversed on S23's znver3 — the honest statement is "within ±35% of single-thread C++, box-dependent". The S20 archive table lives in git history (`results-pre-s20.csv` + the S20/S21 doc revisions).
