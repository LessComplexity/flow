# Matmul (GEMM) — benchmark index

One file per measured session, newest first. Compute tables are the comparison; wall tables are separate (startup-dominated). Ratios are vs flow only, derivation shown. CPU legs move with each box's CPU — compare within one session file, never across.

| Session | Box | What changed | File |
|---|---|---|---|
| **S26** (2026-07-23) | EPYC 7B12 zen2 (61.44-core cgroup-v1 quota → 62-thread pool; clang-18) | **BLAS rung 2: TI=4 register blocking + the fixed-TJ main/remainder split** — flow **7.4× f32 / 5.1× f64 over chapel-multicore @1024** (15.9 vs 117.4 · 23.3 vs 118.3), N=256 flips flow 1.4× — chapel loses every cell ≥256; **numpy f64 gap 13.8× → 7.4× @1024**; 1t f32@1024 84.9 vs S25's 568.3 (6.7×); full-width AVX2 ymm (0 xmm), vfmadd absent (recorded finding); par f32@1024 flat vs S25 — rung 3's floor. **S26b (Sapir framing directive):** 1t-on-1t / par-on-par tables only — quota-aware threaded cpp/rust baselines; flow 10.9× over cpp-mt f32@1024, beats every threaded naive-class baseline; numpy-1t 3.3× ahead of flow-1t (kernel gap) | [matmul/s26.md](matmul/s26.md) |
| **S25** (2026-07-23) | EPYC 7702P (62-core quota; CPU-only box) | **tile emission v1: bit-exact SIMD via cell interleaving** — flow-llvm **3–8.6× ahead of chapel-multicore** at 512/1024 both widths; **numpy gap 130× → 13.8× f64@1024** (rung 1 of the BLAS ladder); tile-vs-untile 2.5–4.6× 1t local + NEON/xmm disasm-verified; compute timer ends the wall-vs-floor estimates; shapes corpus (fir/attn/conv2d) coverage mapped | [matmul/s25.md](matmul/s25.md) |
| **S24+S24b** (2026-07-23) | 4090 ×2 (EPYC 9B14 CPU leg; fmad mini-box) | **parallel orchestrator v1: flow-llvm goes multicore** — N=1024 f32 at chapel-multicore parity (184 vs 193 ms, flow ahead), 19× over its own single thread; **S24b: `-fmad=true` decided+shipped+measured — f64 kernel 232.4→114.0 ms, parity with naive-CUDA-f64 (0.99×@4096, new column) and chapel-gpu** | [matmul/s24.md](matmul/s24.md) |
| S23 (2026-07-22) | 4090, znver3 host | minimal emission (S22) + WP-D hoisting, hardware-verified; **GEMM kernel = naive-CUDA = chapel-gpu at f32 saturation; f64 2× behind chapel = the measured -fmad price**; scale to 4096 | [matmul/s23.md](matmul/s23.md) |
| S21 (2026-07-22) | 4090 | ADR-0029 procedural sources + WP3b; first llvm cap legs at N≥128; first N=512 flow legs | [matmul/s21.md](matmul/s21.md) |
| S20 (2026-07-22) | 4090 | pre-trap-free baseline; raw CSV lost (backup rule born here) | [matmul/s20.md](matmul/s20.md) |
| S16–S19 | various | first numbers, walls named (`docs/notes/bench-matmul.md`); raw: `results-pre-s20.csv` | — |

Raw CSVs: `benches/matmul/results.csv` (S24) · `results-s25.csv` · `results-s24b.csv` · `results-s23.csv` · `results-s21.csv` · `results-pre-s20.csv`.
