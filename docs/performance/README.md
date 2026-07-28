# Performance

Measured numbers only. Each benchmark gets one file: tables + one-line notes. Method notes live at file bottom; narrative archives in `docs/notes/`.

Machine-tag rule (S26, Sapir — standing): every results CSV carries a machine-spec comment header (`# utc / cpu / threads / core_quota / ram_gb / clang` — stamped by `benches/matmul/runner.py`); every comparison table is one machine; cross-machine numbers only as explicitly labeled cross-session rows.

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

- [conv2d-per-core-gap.md](conv2d-per-core-gap.md) — **RESOLVED (S33): it was a measurement boundary, not a kernel defect.** Flow's timed window included the output array's first-touch page-zeroing; the C++ baseline pre-pays that outside its window (`std::vector` value-initialises). Exact differenced counters put Flow's kernel at 18% *fewer* cycles and 22% less real time than C++'s, IPC 2.25 vs 1.78. Fixed by `reside` in `flow_rt_alloc`; **conv2d is now 1.21x AHEAD of naive C++ per core on both NEON and AVX2.** The eight eliminated in-kernel causes were all correctly refuted — the kernel was never the problem. The recorded "IPC 3.11 vs 1.57" was process-level, contaminated by Flow's generation legs.
- [s39-guards-gate-the-flow.md](s39-guards-gate-the-flow.md) — **S39: no performance change, and the timing runs are not what proves it.** The guard-gating change (an arm that is not taken does not run) emits **byte-identical LLVM IR for 103 of 104** bench/matmul/example × face combinations — the one change is `examples/calc.mapal`, the only file in the tree with a trapping guard arm — and those link to **byte-identical binaries**. Identical machine code cannot run at a different speed, so the artifact A/B is the result and the stopwatch is not. The timings are kept as a **noise-floor measurement**: 51 alternating runs × 6 shapes on the M4 Pro spread **−5.89%…+1.18% between the same bytes**, with within-side max/min reaching **4.93×**. That is S38's measurement rule 6 in its strongest form — **under ~6% on an unpinned Mac at sub-millisecond sizes is nothing**; claiming less needs the pinned i9. Also records why there is no regression to explain: gating is legality-then-cost, and the "leave two scalar arms alone" clause exists *because* an earlier version branched inside `sepia`'s per-element map body, which the A/B caught.
- [matmul/s33.md](matmul/s33.md) — **S33: both machines, full suite.** The boundary fix above, plus the cross-machine AMX test: on the i9 (no matrix coprocessor, numpy → OpenBLAS 0.3.30 on the same AVX2 units) **Flow's generated GEMM reaches PARITY with hand-tuned OpenBLAS** — 1t a flat **1.20x behind** at 1024/2048/4096 (146 vs 174 GFLOP/s, both size-invariant), threaded within **±10%** (ahead at 2048, behind at 512 and 4096) — while the M4's AMX-backed numpy is 3.3x ahead. **The M4 matmul gap is hardware, now measured rather than argued.** Confound checked: OpenBLAS does use all 32 threads. Flow scales 9.2–9.8x vs OpenBLAS's 7.1–8.1x, but the "better scheduler" reading was **tested and refuted**: on *uniform* cores OpenBLAS wins (5% on 8 E-cores, **41%** on 8 P-cores) and Flow wins only on a mixed set (1.65x on 4P+4E). It is **heterogeneity tolerance, not a better scheduler** — see s33.md §4. Also **P0: a help-first race in `flow_par_wait` invalidates every `par` *minimum*** (3–4% of threaded runs self-time far too low — one live case read 0.0001 ms; min is the wrong statistic, median is stable) — pre-existing, unrelated to the fix.
