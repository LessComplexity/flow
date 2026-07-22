# 2026-07-23 — S23b: report restructure, scale legs, chapel-gpu, HIP (post-close continuation)

Orchestrator: Claude Fable. Immutable log (ADR-0017). Continuation of S23 after its close, driven live by Sapir's review of the perf report.

## 0. Continuation brief

Current state: **eight commits after the S23 close** (`ed0589d` report restructure · `bde2a86` scale sources/artifacts/runner · `25d7bd1` scale rows · `3b3f076` chapel-gpu leg + naive@4096 row · `8255394` hip leg · `31c57fa` chapel-gpu rows · `1ca7a19` hip rows · plus the fold-fix era earlier). `benches/matmul/results.csv` = **121 rows**; `docs/performance/matmul/` is per-session (`s20/s21/s23.md` + thin index) in Sapir's mandated format (compute-only tables grouped GPU/CPU, ratios only vs flow, wall separate, no accounting noise — memory `perf-report-format`). No code changes — bench/docs only; workspace stays 853 green.
Next step: S24 per `docs/next-session.md` — the fmad call now has its measured price (f64 2× vs chapel-gpu); parallel orchestrator + chapel-gpu leg already landed ahead of schedule.
Resume command/check: `docs/performance/matmul/s23.md`; `git log --oneline -12`.

## 1. The measured story (all outputs cross-verified at every N)

- **Scale legs to 4096** (second same-day 4090): flow GEMM kernel f32 = **naive-CUDA parity from N=1024** (0.788/0.785 = 1.00×; 1.06× @2048; 1.07× @4096) — the S21 "equivalent or better than naive CUDA" bar is met at saturation; sub-1024 gaps are launch-scale overhead.
- **chapel-gpu** (chpl 2.9.0 source-built `CHPL_LOCALE_MODEL=gpu`, ~45 min bundled-LLVM build): f32 converges with flow and naive-CUDA at scale (53.6 ≈ 53.6 ≈ 57.2 @4096); flow FASTER below 1024 (chapel's ~0.25–0.45 ms launch floor). **f64: chapel-gpu 2.0× faster (114 vs 232 @4096) = the `-fmad=false` oracle pin, measured** — FMA halves the compute-bound f64 issue rate; memory-bound f32 is indifferent. Sapir's standing fmad question now has its number.
- **hip-naive** (HIP-on-NVIDIA, nvcc over rocm-6.2 headers): ≡ naive-CUDA at every N — portability tax = zero, measured.
- **naive-cuda@4096**: 53.6 ms / 2,563 GF/s.
- flow-llvm@1024: 5.9 s single-thread (stack-raised; 2048+ blocked on heap lowering).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Perf-report format | per-session files, GPU/CPU-grouped compute tables, ratios only vs flow, kernel-vs-kernel fairness | Sapir directives in-session; memory `perf-report-format` |
| GPU table basis | GEMM-kernel-only (k0_4) vs baselines' timed mul | Sapir's fairness challenge: baselines never time init; flow's Σ did — the Σ moved to the decomposition |
| HIP route | header-repos + nvcc, apt abandoned | repo.radeon.com throttled (2.3 GB in ~80 min); two synthesized cmake-generated headers (version, amd pt-api stub) — runtime calls macro-expand to CUDA, measurement valid |
| chapel-gpu build | source, bundled LLVM, on-box | .deb is CPU-locale-only; system llvm-15 too old for chpl 2.9 |
| Dynamic sizes question | answered: NOT in the language (ADR-0023 accepted-unimplemented; iota/fill static-count) | scale bench needs one generated program per N, no dynamic sizes |

## 3. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | committed through `1ca7a19` (+ this log) | `git status` |
| vast.ai | all session boxes destroyed (5 rentals + 4 dead/recycled — one physical host died twice, several never bootstrapped; ~$1 total) | only Sapir's pytorch box `45550035` remains — hands-off | `vastai show instances` |
| artifacts | `results.csv` 121 rows · scale/chapel/hip sources+artifacts committed | — | `docs/performance/matmul/s23.md` |

## 4. Open items

The S24 agenda (`docs/next-session.md`) unchanged except: chapel-gpu leg DONE (item 2 discharged early); the fmad decision (item 3) now carries its measured 2× f64 price; hip-naive joins the standing baseline set.

## 5. Docs reconciled

`docs/performance/matmul/{s20,s21,s23}.md` + index (new structure); `benches/matmul/` (runner.py legs: scale sizes, chapel-gpu, hip-naive, naive@4096; chapel_matmul_gpu.chpl; hip_naive.hip; 20 scale artifacts); memory `perf-report-format` (new) + MEMORY.md; this log.

**Gotchas added:** vast.ai flaky nights — hosts die mid-run (nohup saved everything twice), containers can take 15+ min to bootstrap sshd (`kex_exchange_identification` closes until then), `success: False` creates still mint contracts (destroy them), ssh-url can reassign after boot (re-query per retry); repo.radeon.com is unusably slow from datacenters (header-repo route instead); chapel GPU needs the source build, ~45 min bundled LLVM.
