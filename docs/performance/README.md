# Performance

Measured numbers only. Each benchmark gets one file: tables + one-line notes. Method notes live at file bottom; narrative archives in `docs/notes/`.

| Benchmark | Compares | Latest | File |
|---|---|---|---|
| Matmul GEMM | flow-cuda · flow-llvm · interp vs naive CUDA · cuBLAS · numpy · Rust · C++ · Chapel | 2026-07-22 (S20) | [matmul.md](matmul.md) |
| Arena allocation | malloc/free API-call counts, structural | 2026-07-21 (v1.0) | [arena.md](arena.md) |

## Headline (current truth)

| What | Number | Note |
|---|---|---|
| flow-cuda matmul kernel time (N=256) | **0.205 ms · 163 GFLOP/s** | FLOW_PERF-instrumented (S20); ~10–17× off naive CUDA — the next target (Index-guard elimination) |
| flow-cuda matmul process wall (N=64) | **313 ms** | startup-bound; still beats naive-C/cuBLAS binaries at equal terms |
| speedup vs own loop form | **40× (N=64) · 321× (N=128)** | wall time |
| flow-llvm vs C++ (znver2, N=64) | 9.17 vs 0.254 ms (loop) · 29.17 vs 0.254 (capture) | was 316 / 2,076 ms pre-W1/W2 — Index-guard vectorization is the residual |
| chapel vs flow-cuda kernel | chapel f64 0.897 ms @ N=256 | direct-competitor row; chapel wins ≤256, cliff at 512+ |
| arena allocation | **8 → 1 cudaMalloc/free** (capture matmul) | smart arenas v1.0; structural, CI-asserted |
| correctness | interp = flow-cuda = naive CUDA = numpy at every N | outputs byte-checked |
| peak measured anywhere | cuBLAS **58.8 TF/s** (N=4096) | the tiling ceiling, untouched by language |
