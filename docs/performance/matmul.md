# Matmul (GEMM) — all measured runs

Square `c[i][j] = Σₖ a[i,k]·b[k,j]`. Times in ms unless noted. ✓ = output cross-verified at that size (see Verification).

**READ THIS FIRST — two measurement kinds:** **(a) per-iteration compute** = CUDA-event timing around the kernel / best-of wall around the library call (startup EXCLUDED — naive CUDA, cuBLAS, numpy, rust, cpp, chapel, and the flow-cuda **kernel** legs). **(b) process wall** = the whole binary, startup INCLUDED (the other flow legs; CUDA context startup ≈ 355 ms on this box, llvm binaries ≈ 1.3 ms floor). At N≤256 the flow-cuda wall legs are startup-bound — compare like-with-like only.

## Main table (S21, 2026-07-22 — v2 procedural sources; post-WP3b llvm; trap-free S20c cuda kernels) — time by backend and size

| N | flow-cuda loop (wall) | flow-cuda cap (wall) | **flow-cuda cap kernel f64/f32 (compute)** | flow-llvm loop (wall) | flow-llvm cap f64 (wall) | flow-llvm cap f32 (wall) | naive CUDA (compute) | cuBLAS (compute) | chapel f64 (compute) | cpp f64 (compute) | rust (compute) | numpy (compute) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 4 | 357.7 ✓ | — | — | 1.32 ✓ | — | — | — | — | — | — | — | — |
| 16 | 530.2 ✓ | 359.1 ✓ | 0.234 / 0.233 | 1.41 ✓ | 1.32 ✓ | 1.90 ✓ | — | — | — | — | — | — |
| 32 | 1,727.0 ✓ | — | — | 2.05 ✓ | — | — | — | — | — | — | — | — |
| 64 | 11,676 ✓ | 356.7 ✓ | 0.234 / 0.234 | 4.66 ✓ | **1.77 ✓** | 1.88 ✓ | 0.0031 ✓ | 0.0099 ✓ | 0.0770 ✓ | 0.3399 ✓ | 0.2616 ✓ | 0.0104 ✓ |
| 128 | 95,053 ✓ | 359.2 ✓ | 0.265 / 0.246 | 63.09 ✓ | **4.12 ✓** | 6.20 ✓ | 0.0048 ✓ | 0.0052 ✓ | 0.1660 ✓ | 2.594 ✓ | 2.493 ✓ | 0.0318 ✓ |
| 256 | — | 355.8 ✓ | 0.289 / 0.243 | — | **29.20 ✓** | 24.31 ✓ | 0.0115 ✓ | 0.0096 ✓ | 1.434 ✓ | 37.15 ✓ | 19.51 ✓ | 0.1080 ✓ |
| 512 | — | 357.3 ✓ | **0.729 / 0.331** | — | **701.1 ✓** | 239.9 ✓ | 0.0831 ✓ | 0.0118 ✓ | 106.7 ✓ | 909.0 ✓ | 236.2 ✓ | 0.4928 ✓ |
| 1024 | — | — | — | — | — | — | 0.8370 ✓ | 0.0443 ✓ | 589.7 ✓ | 7,390 ✓ | 6,819 ✓ | 1.807 ✓ |
| 2048 | — | — | — | — | — | — | 7.097 ✓ | 0.3115 ✓ | — | — | — | 94.90 ✓ |
| 4096 | — | — | — | — | — | — | — | 2.344 ✓ | — | — | — | 422.0 ✓ |

**The S21 firsts:** the flow-llvm capture legs at N=128/256/512 exist for the first time — ADR-0029 procedural sources (72 KB total, was 3.8 MB) + WP3b (no first-class aggregate array moves) killed all three BL1 faces; clang -O2 on matmul256_cap.ll went from OOM-kill to 0.08 s/57 MB. The N=512 legs are new across all flow forms.

## Ratios

| Comparison | Value | Note |
|---|---|---|
| **flow-llvm cap f64 vs single-thread C++ f64, N=256** | **1.27× FASTER** | 29.20 / 37.15 — post-WP3b clang autovectorizes the clean by-ref loops |
| **flow-llvm cap f64 vs single-thread C++ f64, N=512** | **1.30× FASTER** | 701.1 / 909.0 (f32: 239.9 vs rust 236.2 — parity) |
| flow-llvm cap f64 N=64, S20 → S21 | **16.5× faster** | 29.17 → 1.77 (WP3b staging elimination) |
| flow-llvm loop N=64, S20 → S21 | 2.0× faster | 9.17 → 4.66 |
| flow-cuda kernel f32 vs naive-CUDA, N=512 | **4.0× slower** | 0.331 / 0.0831 — was 17.7× at N=256 in S20; and the sum still carries ~0.16 ms first-launch module load (see decomposition) |
| flow-cuda kernel f64 vs naive-CUDA, N=512 | 8.8× slower | 0.729 / 0.0831 |
| flow-cuda kernel f64 vs cuBLAS, N=512 | 62× slower | 0.729 / 0.0118 (tiling/tensor-core gap; cuBLAS is 3,230→22,749 GF here) |
| capture vs loop form (cuda wall), N=128 | **265×** | 359.2 / 95,053 |
| chapel f64 vs cpp f64, N=256 | 26× faster | 1.434 / 37.15 — multi-task `forall` vs single thread |
| numpy vs flow-cuda kernel f32, N=512 | 1.5× faster | 0.493 / 0.331 — BLAS on CPU vs our one square kernel |

## Throughput anchors (GFLOP/s)

| Leg | Peak | At N |
|---|---|---|
| cuBLAS | 58,644 | 4096 |
| naive CUDA | 3,230 | 512 |
| numpy | 1,188 | 1024 |
| **flow-cuda capture kernel f32** | **812** | **512** |
| **flow-cuda capture kernel f64** | **368** | **512** |
| chapel f32 | 39.0 | 256 |
| flow-llvm cap f32 | 1.38 | 256 (single-thread CPU) |
| rust naive | 2.00 | 64 |

## Kernel decomposition (FLOW_PERF, N=512 f32)

Sum 0.331 ms = `k0_0` first launch ~0.157 (CUDA module load — one-time, not the kernel) + `k0_0` second ~0.006 (the two iotas SHARE one kernel — #17 cross-count dedup live in production) + `k0_2` fill-class map ~0.014 + the GEMM map kernel + readbacks. The S20 kernel-gap program (bounds guards) is largely discharged: the S20c trap-free kernel scales 0.23 → 0.33 ms across 16→512 (f32) where S20 sat startup-flat. Remaining gap to naive CUDA (4×): `-fmad=false` (oracle-pinned), launch geometry (256/block untuned), and the module-load constant in the sum.

## Verification

Outputs `c[0]`/`c[N²−1]` agree across **five independent implementations** (flow-cuda, naive CUDA, cuBLAS, rust, cpp, chapel — and numpy) at every shared N: 1047/2107 (64), −7312/−933 (128), −3694/10946 (256), −22592/−38634 (512). flow legs additionally interp-oracle-pinned at N=4/16 (−275/3748, 1815/6944). The remote cuda differential (10 examples + 320 testgen, raw+rewritten) ran green on this box's 4090 against the S21 emitter (one rewrite-driver fixpoint bug found by the rewritten iota/fill leg and fixed in-session — `fill_from` replay faithfulness).

## Notes (one line each)

- flow-cuda capture: ONE arena cudaMalloc/free; deduped kernels incl. cross-count iota dedup; trap-free end-to-end (S20c proofs + ADR-0029 total ops) — no trap params, no readback syncs in the kernel path.
- flow-llvm capture: WP3b — array staging is pointer-fields + `llvm.memcpy` only; f64 beats single-thread C++ at 256/512; f32≈rust parity at 512.
- flow-cuda loop form unchanged (Θ(N³) launches — the per-op mapping's price; region emission is the planned fix).
- cuBLAS `CUBLAS_DEFAULT_MATH` (no TF32); flow recipe pinned `nvcc -std=c++17 -fmad=false -arch=sm_89`; llvm legs `clang-15 -O2 -march=native` (clang ≥ 15 REQUIRED — 14 predates opaque-`ptr` and skips every llvm leg silently).
- Chapel 2.9.0 (`chpl --fast`, CHPL_TARGET_CPU=native).
- Box startup ≈ 355 ms this box (S20 box: ≈ 313) — wall legs are box-dependent below N≈512.

## Method

| Item | Value |
|---|---|
| GPU box | vast.ai RTX 4090 #45516809 (16 vCPU), nvcc 12.4.131, clang 15.0.7, Chapel 2.9.0, rust stable, numpy 2.2.6 — one box for ALL legs (destroyed after) |
| flow legs | v2 procedural artifacts (ADR-0029; deterministic emission); process wall min-of-3 with adaptive cap; kernel legs = Σ `FLOW_PERF launch=` CUDA-event times (min of 3) |
| Sources | `benches/matmul/` (`gen_flow_capture.py`/`gen_flow.py` v2, `--width f32`), driver `s21_box.sh` |
| Raw data | `benches/matmul/results.csv` (S21, 86 rows) · `results-pre-s20.csv` (S16–S19) · **S20's raw CSV was overwritten by the S21 runner before archiving (benches/ was uncommitted); its numbers survive in the archive below + the S20 session log** |

---

## Archive — S20 main table (box A, 2026-07-22, pre-S20c-re-measure; superseded above)

| N | flow-cuda loop (wall) | flow-cuda cap (wall) | cap kernel f64/f32 (compute) | flow-llvm loop (wall) | flow-llvm cap f64 (wall) | naive CUDA | cuBLAS | chapel f64 | cpp f64 | rust | numpy |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 16 | 499.8 | 321.8 | 0.197 / 0.200 | 2.28 | 2.38 | — | — | — | — | — | — |
| 64 | 12,681 | 313.0 | 0.193 / 0.195 | 9.17 | 29.17 | 0.0032 | 0.0136 | 0.0650 | 0.2541 | 0.1911 | 0.0106 |
| 128 | 101,183 | 315.5 | 0.206 / 0.223 | 70.48 | — (BL1) | 0.0048 | 0.0055 | 0.1730 | 2.651 | 2.591 | 0.0464 |
| 256 | — | 316.2 | 0.205 / 0.210 | — | — (BL1) | 0.0116 | 0.0109 | 0.8970 | 35.11 | 21.20 | 0.1422 |

S20 context lines kept for history: the kernel legs sat startup-flat (~0.2 ms at every N — the pre-S20c guard/trap overhead masked scaling); llvm cap N≥128 was BL1-walled (23 MB literal `.ll`, clang OOM at 500 GB RSS). Both are S21-discharged. The S20 "kernel-gap finding" (bounds guards → trap-free proofs) is implemented; its residue is the 4× gap noted in the decomposition above.
