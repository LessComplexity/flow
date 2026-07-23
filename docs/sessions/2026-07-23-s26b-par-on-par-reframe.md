# 2026-07-23 — S26b: par-on-par benchmark reframe (Sapir directive)

Orchestrator: Kimi (category-architect skill). Immutable log (ADR-0017). Post-close
continuation of S26 (main log: `2026-07-23-s26-register-blocking.md`), driven by
Sapir's review directive: **"remove comparison of multithread flow to 1t \<lang\>…
two comparisons — 1t, then multithread with c++/rust at their best threading too.
Who wins needs to be par on par."**

## 0. Continuation brief

Current state: **S26b closed.** Threaded quota-aware baselines (`cpp_mt`/`rust_mt`)
exist, one mini-box measured both tables same-machine, `s26.md` reframed (mt-vs-1t
rows deleted). Par-on-par headline: the 475× row's honest form is **10.9×** (flow vs
cpp-mt f32@1024) — flow still beats every threaded naive-class baseline everywhere
(cpp-mt 3.1–10.9×, rust-mt 3.0–9.5×, chapel-mc 3.1–9.6×); OpenBLAS stays ahead
(numpy-threaded 3.9× f32 like-for-like / 6.2× f64 pairing; numpy-1t beats flow-1t
3.3× f32@1024). Box destroyed (≈$0.036). **Commits for all S26+S26b work still
pending Sapir's confirm.**
Next step: S27 per `docs/next-session.md` (rung 3 packing; vfmadd decision; and
Sapir's two review rungs — Inline wiring, loop→map design — awaiting his go).
Resume command/check: `docs/performance/matmul/s26.md` (the two tables);
`git status`.

## 1. Work completed

- **Threaded baselines** (`benches/matmul/cpp_mt.cpp`, `rust_mt.rs`, new): mirror
  the naive sources (same per-cell algorithm, byte-equal outputs — verified locally
  N=64/256 both widths), row-partitioned `std::thread` (rust: scoped threads, no
  deps — the box rustc recipe holds). **Quota-aware width** (cgroup v2 `cpu.max` →
  v1 `cfs_quota` fallback → `hardware_concurrency`; `THREADS` env override) — a
  128-thread baseline on the 61.44-core quota would be the mt-vs-1t sin in reverse.
- **Runner legs:** `runner.py` + `runner.sh` — cpp-mt/rust-mt legs, per-leg ENV,
  numpy-1t (`OPENBLAS_NUM_THREADS=1`), chapel-1t (`CHPL_RT_NUM_THREADS_PER_LOCALE=1`,
  verified: 0.92→25.4 ms @256), optional argv leg filter; spec-stamp preserved.
  **Latent bug fixed:** the v1 quota probe missed the `cpu/` dir (stamped "unknown"
  on v1 boxes — the S26 box class); s26b CSV comment row carries the measured 61.44.
- **Mini-box leg:** instance 45636495 (same offer as the S26 box — EPYC 7B12,
  quota 61.44 → flow pool 62 / cpp_mt T=62 / rust_mt T=61), ubuntu:22.04 CPU-only,
  **destroyed, ≈$0.036**. 18/18 legs ran, zero failures; `results-s26b.csv`
  (spec-stamped, one-box rule); every `out=` field byte-equal to results-s26.csv.
- **`docs/performance/matmul/s26.md` reframed:** "Who wins" → two tables —
  **1t vs 1t** and **par vs par**, self-contained on the s26b box (s26 rows
  corroborate in the ~20% box band); the 475×/443× rows deleted, referenced only
  as history; intro addendum records the standing framing rule.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Comparison framing | two tables only: 1t-on-1t, par-on-par with best-threaded baselines — **standing** | Sapir directive (session tail, S26 review) |
| Baseline thread width | quota-aware (cgroup v2→v1→hardware_concurrency, THREADS override) | symmetric fairness: flow-rt's own rule; 128-on-61 oversubscription would be the same sin in reverse |
| Baseline threading impl | std::thread row-partition (rust: scoped threads, zero deps) | best-performance class for this pattern; keeps the box's direct-rustc recipe |
| numpy pairing | kept the standing f64-pairing convention in S26b tables; **flagged: `numpy_bench.py` runs fp32** — the "f64 BLAS" rows overstate flow's numpy gap (true f32 cells: numpy 3.9× par, 3.3× 1t) | re-pairing is Sapir's decision, separate from the thread framing |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| baselines vs naive, local N=64/256 f32/f64 | byte-equal outputs | threading didn't change the algorithm |
| chapel-1t env pin | 0.92→25.4 ms @256, same c0 | the 1t chapel row is really 1t |
| `out=` fields s26b vs s26 CSVs | byte-equal at every shared cell | cross-box field correctness |
| 1t table (f32@1024) | flow 75.3 vs cpp 7,300 (97×) · rust 7,044 (94×) · chapel 6,256 (83×) · numpy-1t 22.7 (**numpy 3.3× faster**) | the naive-class 1t gap; OpenBLAS kernel gap isolated |
| par table (f32@1024) | flow 12.7 vs cpp-mt 138.1 (**10.9×**) · rust-mt 120.8 (9.5×) · chapel-mc 122.8 (9.6×) · numpy 3.28 (numpy 6.2×, f64 pairing; 3.9× f32 like-for-like) | flow ahead of every threaded naive-class baseline; BLAS still the target |
| threaded scaling sanity | cpp-mt 53× / rust-mt 58× over their own 1t | the baselines' threading is real (near-linear) |

## 4. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | all S26+S26b work uncommitted — commits pending Sapir | `git status` |
| vast.ai | 45636495 | DESTROYED (≈$0.036) | — |
| vast.ai | 45622441 STILL RUNNING — Sapir's own, hands-off | unknown | `vastai show instances` |
| total S26+S26b box spend | ≈$0.16 | recorded | — |

## 5. Open items

Unchanged from the S26 log except: **S26b reframe CLOSED (this log).** New:
| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P2 | numpy fp32-vs-f64 pairing flag | `benches/matmul/numpy_bench.py`; s26.md readings | Sapir decides: re-pair f32-like-for-like or keep the f64 convention (with the label honest) | decided + tables re-labeled |

## 6. Architecture / model changes

None. Measurement-harness change only (new baselines + legs).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/performance/matmul/s26.md` | reframed per directive (two tables, s26b box section, readings updated) |
| `benches/matmul/runner.py` / `runner.sh` | mt + 1t legs, per-leg ENV, leg filter, v1-quota probe fix |
| `benches/matmul/{cpp_mt.cpp,rust_mt.rs,s26b_box.sh,results-s26b.csv}` | new |
| `docs/STATUS.md` | S26 row amended: S26b closed, two-table framing, spend ≈$0.16 total |
| `docs/performance/matmul.md` | S26 index row amended with the s26b par-on-par headline |
| `docs/next-session.md` | S26b noted closed; numpy-pairing flag added to Sapir's open questions |
| this log | new |

## 8. Files changed

`benches/matmul/cpp_mt.cpp` · `benches/matmul/rust_mt.rs` ·
`benches/matmul/runner.py` · `benches/matmul/runner.sh` ·
`benches/matmul/s26b_box.sh` · `benches/matmul/results-s26b.csv` ·
`docs/performance/matmul/s26.md` · `docs/STATUS.md` · `docs/performance/matmul.md` ·
`docs/next-session.md` · this log. **Nothing committed** — pending Sapir's confirm.
