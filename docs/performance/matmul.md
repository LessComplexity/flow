# Matmul (GEMM) — benchmark index

One file per measured session, newest first. Compute tables are the comparison; wall tables are separate (startup-dominated). Ratios are vs flow only, derivation shown. CPU legs move with each box's CPU — compare within one session file, never across.

| Session | Box | What changed | File |
|---|---|---|---|
| **S23** (2026-07-22) | 4090, znver3 host | minimal emission (S22) + WP-D hoisting, hardware-verified; **GEMM kernel at naive-CUDA parity from N=1024 f32**; scale legs to 4096; first chapel legs | [matmul/s23.md](matmul/s23.md) |
| S21 (2026-07-22) | 4090 | ADR-0029 procedural sources + WP3b; first llvm cap legs at N≥128; first N=512 flow legs | [matmul/s21.md](matmul/s21.md) |
| S20 (2026-07-22) | 4090 | pre-trap-free baseline; raw CSV lost (backup rule born here) | [matmul/s20.md](matmul/s20.md) |
| S16–S19 | various | first numbers, walls named (`docs/notes/bench-matmul.md`); raw: `results-pre-s20.csv` | — |

Raw CSVs: `benches/matmul/results.csv` (latest session) · `results-s21.csv` · `results-pre-s20.csv`.
