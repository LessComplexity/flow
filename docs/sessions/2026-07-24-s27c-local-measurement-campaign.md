# 2026-07-24 — S27c: local measurement campaign (matmul cross-language + shapes)

Orchestrator: Claude Fable (category-architect skill). Immutable log (ADR-0017).
Second same-day continuation (after `-s27b-loop-lift-panel-residence.md`), driven by
Sapir: "if it is only cpu you can run here locally too… I want cpu comparison across
the languages for attn, conv2d, fir (attn later — no exp)… I don't see s27
performance comparison for matmul."

## 0. Continuation brief

Current state: **the S27 matchup exists — locally, same-machine (Apple M4 Pro), full
report `docs/performance/matmul/s27.md` + raw `benches/matmul/results-s27-local.csv`
(spec-stamped; runner.py gained macOS probes + a `FLOW_BENCH_MAX_N` clamp).**
Matmul @1024 wall: par-on-par flow-fma f32 11.7 ms vs cpp-mt 132.4 (**11.3×**) /
rust-mt 131.3; 1t-on-1t flow-fma 24.7 vs cpp 842.6 (**34×**); f64 7.1×/19.4×.
numpy = Apple Accelerate AMX (3.2 TF/s — coprocessor row, labeled; env pin can't
make a 1t leg). Flow 2048 rows: macOS 64 MB stack refuses (recorded enabler = heap
lowering; box owes 2048/4096). **Shapes (new cross-language baselines,
`benches/shapes/{shapes_baseline.cpp,shapes_baseline.rs,shapes_numpy.py,shapes_ab.sh}`):
fir mid-pack (flow-fma-par 0.36 vs cpp-mt 0.21 — 1-D sites have no TI/packing rung);
conv2d = the priced refusal — flow 7.9× BEHIND single-thread cpp at 14 threads (the
derived-var walker's demand gate has fired).** Panel residence local: flat (M-series
SLC already held b — zen2 box is its test). All legs' outputs byte-verified across
languages. Box still blocked (balance 0).
**Close addendum (Sapir's end-review, this session):** (1) **measurement-fairness
catch (his):** wall-vs-selftimed was unfair — baselines self-time compute only;
verdict tables rewritten to compute-vs-compute (flow `FLOW_PERF` vs their iteration
ms; +8 manual 1t-compute CSV rows) — **flow ahead at EVERY size on the fair basis,
256 included (2.8× cpp-mt)**; standing rule recorded (next-session agenda 3).
(2) S28 focus directive: generalize the ladder to fir & conv2d until flow wins —
`tile_plan` MUST record non-affine/derived-var sites ("we HAVE to"); FIR 1-D rung.
(3) Match-numpy-or-more = agenda 2 (OpenBLAS the NEON-class target). (4) Stack
problem explained + recorded (alloca arrays vs macOS 64 MB hard ceiling; heap
lowering the fix). (5) Commits confirmed and executed at close.
Next step: S28 agenda 1 (fir/conv2d generalization); box when balance lands.
Resume command/check: `docs/next-session.md`; `docs/performance/matmul/s27.md`.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Local = a first-class same-machine matrix | yes (Sapir) — `results-s27-local.csv`, own report section | CPU story runs anywhere; box still owed for zen2/4096/chapel |
| numpy locally | keep, labeled Accelerate-AMX coprocessor row; numpy-1t dropped as unmeasurable | env pin doesn't bind Accelerate; silicon class differs |
| Flow 2048 local | absent, recorded (macOS 64 MB hard stack) | heap lowering = enabler; not a harness bug |
| conv2d walker gate | **fired** — 7.9×-behind-1t-cpp measured | was "gate: measured demand" since S25 |
| attn baselines | deferred | no `exp` in Core (standing Sapir question) |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| cross-language `out=` fields, every leg every shared N | byte-equal (matmul + both shapes; fma legs rel≤1e-4) | six-way semantic agreement incl. LIFTED loop-form legs |
| matmul campaign (21 legs, ≤2048) | `results-s27-local.csv`, zero mismatches | the S27 local matrix |
| shapes_ab.sh (par + FLOW_PAR=1) | green ×2, verification hard-gates passed | fir/conv2d cross-language contract |

## 4. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | all S27/b/c work uncommitted — pending Sapir | `git status` |
| vast.ai | — | balance 0, no instances | `vastai show user` |
| scratch binaries | scratchpad `bench/` | disposable (CSV copied into repo) | — |

## 5. Open items

Delta only: **conv2d derived-var walker promoted** (demand measured — next-session
agenda item annotated); **FIR 1-D blocking rung recorded** (mid-pack result). Box +
commits + numpy pairing + loop→map v2 rungs: unchanged from S27b log.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/performance/matmul/s27.md` | new — local matrix report (wall/compute/1t/par tables, caveats, shapes section, lift note) |
| `benches/matmul/results-s27-local.csv` | new (spec-stamped) |
| `benches/matmul/runner.py` | macOS cpu/ram probes; `FLOW_BENCH_MAX_N` clamp (box-safe: unset = full lists) |
| `benches/shapes/{shapes_baseline.cpp,.rs,shapes_numpy.py,shapes_ab.sh}` | new (codex WP6, orchestrator-reviewed + independently rerun) |
| `docs/performance/matmul.md` | S27 row → link the report |
| `docs/next-session.md` | conv2d item annotated with the measurement |
| this log | new |

## 8. Files changed

As §7. Nothing committed — pending Sapir's confirm.
