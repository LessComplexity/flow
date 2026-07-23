# Matmul (GEMM) — benchmark index

One file per measured session, newest first. Compute tables are the comparison; wall tables are separate (startup-dominated). Ratios are vs flow only, derivation shown. CPU legs move with each box's CPU — compare within one session file, never across.

| Session | Box | What changed | File |
|---|---|---|---|
| **S24+S24b** (2026-07-23) | 4090 ×2 (EPYC 9B14 CPU leg; fmad mini-box) | **parallel orchestrator v1: flow-llvm goes multicore** — N=1024 f32 at chapel-multicore parity (184 vs 193 ms, flow ahead), 19× over its own single thread; **S24b: `-fmad=true` decided+shipped+measured — f64 kernel 232.4→114.0 ms, parity with naive-CUDA-f64 (0.99×@4096, new column) and chapel-gpu** | [matmul/s24.md](matmul/s24.md) |
| S23 (2026-07-22) | 4090, znver3 host | minimal emission (S22) + WP-D hoisting, hardware-verified; **GEMM kernel = naive-CUDA = chapel-gpu at f32 saturation; f64 2× behind chapel = the measured -fmad price**; scale to 4096 | [matmul/s23.md](matmul/s23.md) |
| S21 (2026-07-22) | 4090 | ADR-0029 procedural sources + WP3b; first llvm cap legs at N≥128; first N=512 flow legs | [matmul/s21.md](matmul/s21.md) |
| S20 (2026-07-22) | 4090 | pre-trap-free baseline; raw CSV lost (backup rule born here) | [matmul/s20.md](matmul/s20.md) |
| S16–S19 | various | first numbers, walls named (`docs/notes/bench-matmul.md`); raw: `results-pre-s20.csv` | — |

Raw CSVs: `benches/matmul/results.csv` (latest session) · `results-s23.csv` · `results-s21.csv` · `results-pre-s20.csv`.
