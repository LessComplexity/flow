# Session 16 — 2026-07-21 — Matmul benchmark: Flow vs CUDA vs cuBLAS vs CPU

Orchestrator: Kimi Code · skill: category-architect. Immutable log (ADR-0017). Session scope set by Sapir: "create a matmul kernel on best optimization levels and compare to other languages on speed over many iterations" (+ two steers: the bench code must live in-tree, and a design question — PTX backend vs CUDA C++). Not a milestone session — M3 stands; this is the first measured performance session.

## 0. Continuation brief

Current state: **M3 stands, workspace 673 green, nothing live.** The first cross-language benchmark exists and is written up: `docs/notes/bench-matmul.md` (sources in `benches/matmul/`, readable program in `examples/matmul4.flow`). Headline: today's Flow-CUDA matmul is bounded by Θ(N³) kernel launches at ~24 µs/op, not by algorithm or GPU; the real GEMM blocker is expressibility (L1108 — no captured map bodies). Same `.flow` source: flow-llvm is 38× faster than flow-cuda at N=64 on a laptop CPU.
Next step: **P7 backend-verilog (M4)** per next-session.md; the capture/cartesian ADR conversation awaits Sapir's word.
Resume command/check: `cargo test --workspace` (expect 673); `cargo run -p flow-interp --example run -- examples/matmul4.flow` (expect `-275\n3748`).

## 1. Work completed

- **The benchmark, end to end.** Generator + Flow sources (`benches/matmul/gen_flow.py` → `matmul{4..128}.flow`, the proven flattened `cell`/Update pattern — the only Core-expressible shape, since L1108 blocks captures in map bodies); verified against interp + numpy at every N. Emitters added (`crates/flow-backend-{llvm,cuda}/examples/emit.rs` — dev tools, the future `flow build` embryo). Baselines written (`naive_cuda.cu`, `cublas_gemm.cu`, `numpy_bench.py`, `rust_naive.rs`) + box runner. GPU leg on a fresh vast.ai 4090 (`45448060`, $0.36/hr, ~30 min, ≈ $0.18, destroyed after): flow-cuda compiled with the pinned M3 recipe and ran all five sizes with correct output; full baseline sweeps (CUDA events, warmup + per-iteration medians).
- **The numbers** (all in `docs/notes/bench-matmul.md` + `benches/matmul/results.csv`): flow-cuda 12.16 s @N=64 / 99.8 s @N=128 (≈23–24 µs per launch+sync+readback, Θ(N³) of them) · naive one-kernel CUDA 0.0032 ms @64 → 3.1 TF/s peak @512 · cuBLAS 55.9 TF/s @4096 (~68% peak) · numpy 1.33 TF/s @2048 · rust-naive 2.6 GF/s @64 (cache collapse at 1024) · flow-llvm (macOS, same source!) 316 ms @64.
- **Five findings recorded:** (1) the wall is per-op launch latency (DESIGN §3's correct-first price made visible), 3.8M× at N=64 — not the algorithm; (2) backend strategy dominates (flow-llvm 38× on a laptop — VISION truth #1); flow-llvm's own W3 N⁴ Update-copy wall shows at N=64; (3) **the real GEMM blocker is expressibility** — L1108 forbids captured bodies, so no one-kernel form exists in Core (an ADR-level conversation, queued for Sapir); (4) the algorithm gap is 22× (naive → cuBLAS tiling/tensor cores); (5) CPU context numbers.
- **Sapir's PTX question answered and recorded** (`components/backend-cuda/suggestions.md` #11 + the bench note's toolchain section): nvcc/C++ kept; NVRTC is the interesting variant (same emitted text, runtime compilation, kills the nvcc binary dependency — relevant to a future `flow run --gpu`); raw LLVM-NVPTX would not reuse the host emitter and adds glue for zero perf.
- **In-tree artifacts (Sapir steer):** `examples/matmul4.flow` (verified `-275\n3748` through interp; harnesses enumerate examples by name, so nothing else is affected), `benches/matmul/` (generator, sources, baselines, runner, results.csv), the two `emit` examples.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Bench home | `benches/matmul/` + `examples/matmul4.flow` | examples/ is name-enumerated in every harness (no glob) — a new file enters no sweep; `benches/` holds the perf harness |
| flow-cuda optimization flags for the bench | the **pinned M3 recipe, unmodified** (`-fmad=false -arch=sm_89`) | the point is measuring the realized backend, not a flag-tuned special; flag variants are the recorded `-O3`-row headroom |
| N range for flow-cuda | 4..128 (adaptive cap) | Θ(N³) launches: N=256 would be ~15 min for no new information |
| cuBLAS math mode | `CUBLAS_DEFAULT_MATH` (no TF32) | honest fp32 comparison |
| PTX vs CUDA C++ | **keep nvcc/C++; record NVRTC as the interesting variant** | host IR ≠ device IR (no reuse of flow-backend-llvm); driver-API glue for zero perf (nvcc's pipeline IS Clang→NVPTX→ptxas); NVRTC keeps the emitted text and enables JIT |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| Correctness chain at every N | interp = flow-cuda = naive-cuda = numpy (e.g. N=64 → 1047/2107; N=4096 → 74348/-302529) | the perf numbers measure the right program |
| flow-cuda wall | 12.16 s @64, 99.8 s @128; ~24 µs/op (launch+sync+readback) | the Θ(N³)-launch execution strategy, priced |
| flow-llvm (same source, macOS) | 2.34/2.64/5.14/316 ms @4..64 | backend-strategy gap (38× at 64); the W3 N⁴ wall |
| naive-cuda | 0.0032 ms @64 … 7.36 ms @2048 (peak 3.1 TF/s @512) | the algorithm without launch overhead |
| cuBLAS | 55.9 TF/s @4096 | the optimization ceiling (~68% peak) |
| numpy / rust-naive | 1.33 TF/s @2048 · 2.6 GF/s @64 (0.33 @1024) | CPU context |
| `cargo test --workspace` | 673 green (final verification with the new emit examples) | no regressions |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume | Stop / cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` | **uncommitted** (S14+S15+S16 work; Sapir owns commits) | `git status` | none |
| machine | vast.ai 45448060 (S16 bench box) | **destroyed** (≈ $0.18) | `vastai show instances` | done |
| machine | vast.ai 45170851/52/45181070 | running — **Sapir-declared unrelated to Flow; do not use or destroy** | n/a | not ours |
| artifact | `benches/matmul/`, `examples/matmul4.flow`, `docs/notes/bench-matmul.md`, two `emit` examples | committed-to-tree (uncommitted branch) | `cargo run -p flow-interp --example run -- examples/matmul4.flow` | keep |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | P7 backend-verilog (M4) | `components/backend-verilog/STATUS.md` (stub) + HANDOFF §4.3 | model-first DESIGN (D2 backend-seam practice; done-protocol) | M4 line green on verilator |
| P1 | **Capture/cartesian ADR** (the GEMM expressibility blocker) | bench note finding #3; lower `typing.rs` L1108 | Sapir's design call → orchestrator writes the candidate | ADR exists |
| P1 | ADR-0024 decision; coproducts ADR on Sapir's word | `decisions/ADR-0024…`, `ADR-0026…` | Sapir answers / orchestrator writes | status flips |
| P2 | interp `read-before-write` panic on exit-only-payload shape (pre-existing) | `flow-interp/src/eval.rs:297` | reproduce standalone; fix or document | pinned |
| P2 | NVRTC spike — only if JIT becomes a requirement | backend-cuda suggestions #11 | M5-CLI timeframe | toolkit-free `flow run --gpu` or declined |
| P3 | rewrite `loop_plan` migration (S12 P3); doc leftovers; heavy/fast test split | next-session.md | small edits | reconciled |

## 6. Architecture / model changes

None in code (no milestone work). New recorded knowledge: the backend-cuda headroom list is now *measured* (the per-op launch floor is priced at ~24 µs; the W3 N⁴ wall shows in flow-llvm at N=64); the capture/cartesian question is promoted from a language gap to the documented critical path for a Flow GEMM. The `emit` examples are new system entry points (recorded in `docs/IMPLEMENTATION.md`).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/notes/bench-matmul.md` | new — the full report (method, numbers, five findings, ladder, toolchain note, repro) |
| `benches/matmul/` | new — generator, sources, baselines, runner, results.csv |
| `examples/matmul4.flow` | new — the readable N=4 (verified) |
| `crates/flow-backend-{llvm,cuda}/examples/emit.rs` | new — dev emitters |
| `components/backend-cuda/STATUS.md` | performance notes: the measured numbers + the two named walls |
| `components/backend-cuda/suggestions.md` | +#11 NVRTC/PTX toolchain variant; capture ADR promoted |
| `docs/suggestions.md` | roll-up row 8 rewritten (S16, 11 rows, capture-ADR headline) |
| `docs/IMPLEMENTATION.md` | entry points: the two `emit` examples |
| `docs/next-session.md` | rewritten for S17 (P7 first; capture-ADR conversation) |

## 8. Files changed

`benches/matmul/**` (new) · `examples/matmul4.flow` (new) · `crates/flow-backend-{llvm,cuda}/examples/emit.rs` (new) · docs per §7 · uncommitted (Sapir owns commits).

**Next `start` path:** read `sessions/2026-07-21-s16-matmul-benchmark.md` (this log) → `docs/next-session.md` → P7 → `cargo test --workspace` (expect 673).
