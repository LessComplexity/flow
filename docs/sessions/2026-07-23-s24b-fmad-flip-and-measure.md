# 2026-07-23 — S24b: fmad flipped + measured; report amendments; the graph-advantage note

Orchestrator: Claude Fable. Immutable log (ADR-0017). Post-close continuation of S24, driven live by Sapir's review of `matmul/s24.md`.

## 0. Continuation brief

Current state: **five commits after the S24 close** (`ba9de17` report amendments + naive-cuda f64 baseline · `6867bdc` the fmad flip · `541a37a` fmad measured + graph-advantage note · plus this close-out). The standing S21 fmad question is **decided by Sapir ("flip to true by default") and its result measured same-day**: product/bench recipe `-fmad=true` (+ clang `-ffp-contract=fast`); the conformance differential keeps `-fmad=false` (byte-parity gate; DESIGN §4 amended). **Result: flow f64 GEMM kernel 232.4 → 114.0 ms @4096 — 0.99× parity with the brand-new naive-CUDA-f64 column and with chapel-gpu-f64 (S23: 114.2); f32 tightens to 1.00×.** `matmul/s24.md` now leads with the `-fmad=true` table as THE GPU numbers. Workspace green (cuda 163 after 14 comment-only snapshot re-pins; emit_sweep 640/640). No live boxes from this session (S24b mini-box `45603626` destroyed, ≈$0.10).
Next step: S25 per `docs/next-session.md` — item 1 (pool floor + suggestion #9 guards) and item 2b (the SIMD ladder) are the CPU follow-through; fmad item CLOSED.
Resume command/check: `docs/performance/matmul/s24.md`; `git log --oneline -6`.

## 1. Work completed

- **Report amendments (Sapir close-review):** GPU table extended to N=16–4096 with an explicit f64÷f32 column (launch floor 1.0× → 4.2× at saturation); dedicated "fmad measure" section (the price lives in S23: 232.4 vs chapel-gpu 114.2 = 2.0×); readings 5/6 answer "why chapel/numpy win on CPU" — numpy = BLAS algorithm class (cpp/rust equally ~2,300× behind), chapel's ≤512 edge = pool-spawn floor + fold-body bounds guards (suggestion #9; chapel `--fast` runs checks-off), memory-bound 1024 already at parity.
- **naive-CUDA f64 baseline created** (`naive_cuda.cu` templated, `--width f64`; runner leg `naive-cuda-f64`) — flow-f64 GPU had no like-for-like row before this.
- **The fmad flip (`6867bdc`):** emitted `.cu` headers state the new default + the conformance pin; `runner.sh` nvcc lines → `-fmad=true`, clang lines → `-ffp-contract=fast`; cuda differential KEEPS `-fmad=false` with the rationale comment; DESIGN §4 amendment block; BC9 narrowed to conformance context; 14 golden snapshots re-pinned (comment-only, hand-checked); regen.sh over all bench artifacts (38 files, comment-only).
- **The measurement (`541a37a`):** third-box mini-sweep (fmad_box.sh + drive script; 4090, ≈20 min) — FLOW_PERF kernels at `-fmad=true` ×3 runs × 4 sizes × both widths + naive f32/f64 + cuBLAS same box; raw in `results-s24b.csv` (one-box rule; own file).
- **`docs/notes/graph-advantage.md`:** Sapir's founding assumption (graph vs AST) made precise + the five-row shipped-evidence ledger + reuse-is-fanout generalization + the honest boundary.
- Local CPU A/B: `-ffp-contract=fast` alone is noise at 1024 f32 (0.14 → 0.13 s) — guards block vectorization; #9 is the gate (recorded in agenda 1/2b).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| fmad default | **`-fmad=true` product/bench; `-fmad=false` conformance-only** | Sapir: "flip to true by default, don't dwell"; the gate measures semantics (bytes vs oracle), the product measures speed; DESIGN §4 hazard analysis unchanged — it's why the gate pins false |
| CPU face | `-ffp-contract=fast` on bench clang lines only | same decision, same split: differential stays contraction-off |
| S24b raw rows | own file `results-s24b.csv` | third box; the one-box comparability rule |
| naive-cuda leg names | `naive-cuda` stays f32; new `naive-cuda-f64` | report/CSV continuity over naming symmetry |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test -p flow-backend-cuda` (after flip) | 163 green | snapshot re-pins comment-only; conformance path untouched |
| `emit_sweep` | 640/640, 0 panics | emitters deterministic under the header change |
| **f64 kernel @4096, `-fmad=true`** | **114.0 vs naive-CUDA-f64 115.4 = 0.99× — parity** | the 2× f64 gap was exactly the fmad price (232.4 pre-flip; 2.04× recovered) |
| f64 @1024 | 1.811 vs 1.840 — flow ahead | — |
| f32 @4096 / @2048 | 1.00× / 0.99× | f32 saturation parity tightens under the same recipe |
| f32 @512 | 2.15× | launch geometry remains (standing item #5) |

## 4. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | committed through this log | `git status` |
| vast.ai | S24b box `45603626` DESTROYED (≈$0.10). Foreign instances `45591095`, `45602038` still running at close — NOT this session's, hands-off, ownership question open to Sapir | `vastai show instances` |
| artifacts | `results-s24b.csv` (20 rows) · `fmad_box.sh` · templated `naive_cuda.cu` | — | `docs/performance/matmul/s24.md` |

## 5. Open items

Unchanged from the S24 log except: **fmad (S25 item 2) CLOSED — decided, shipped, measured**; item 2b (SIMD ladder: #9 guards-off auto-vectorization → tiling-as-rewrite) added; naive-cuda-f64 column now standing in the leg set. Foreign-instance ownership (P3) still open.

## 6. Architecture / model changes

None structural. DESIGN §4 (cuda) amended: the fmad pin is re-scoped from "the recipe" to "the conformance recipe"; BC9 narrowed accordingly. The graph-advantage note is methodology/vision documentation, not model change.

## 7. Docs reconciled

`docs/performance/matmul/s24.md` (full-N GPU table + fmad section + readings 5/6 + S24b table promoted to THE GPU numbers) · `matmul.md` index row · `docs/components/backend-cuda/DESIGN.md` §4 amendment · `docs/next-session.md` (fmad closed; 2b SIMD ladder; #9 named in item 1) · `docs/notes/graph-advantage.md` (new) · this log.

## 8. Files changed

`crates/backends/cuda/{src/lib.rs,tests/differential.rs,tests/snapshots ×14}` · `benches/matmul/{naive_cuda.cu,runner.py,runner.sh,fmad_box.sh,results-s24b.csv,*.cu ×38 comment-only,s24_box.sh(prior)}` · `docs/{performance/*,components/backend-cuda/DESIGN.md,next-session.md,notes/graph-advantage.md}` · this log.
