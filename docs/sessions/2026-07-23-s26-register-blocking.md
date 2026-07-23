# 2026-07-23 — S26: BLAS rung 2 — TI register blocking + the fixed-TJ split

Orchestrator: Kimi (category-architect skill). Immutable log (ADR-0017). Continuation
of `2026-07-23-s25b-close-addendum.md` per `docs/next-session.md` agenda items 1+2
("one emitter wave"). Plan of record:
`docs/components/backend-llvm/plans/plan-s26-register-blocking.md` (SHIPPED S26).

## 0. Continuation brief

Current state: **rung 2 shipped end-to-end, measured local + box, docs reconciled;
commits pending Sapir's confirm.** `emit_tiled_map` restructured (backend-llvm
`func.rs` only — TILE_I=4 register blocking + fixed-TJ main/remainder split, gate
`rows>1 && b.ci==0`; flow-ir untouched). Local: tile vs no-tile 512 f32 12.8×,
1024 f32 23.4×; TI sweep 2/4/8 → 4. Box (EPYC 7B12, 62-core quota, clang-18):
full-width ymm, 0 xmm; flow 7.4×/5.1× ahead of chapel-mc f32/f64@1024; numpy f64 gap
13.8×→7.4×; box destroyed (≈$0.12). 906 green. **Sapir mid-review directives (this
session's tail):** (a) benchmark reframe — two tables only (1t vs 1t; par vs par with
best-threaded baselines) → S26b; (b) fn-stripping must be wired in (it exists,
`flow-rewrite/src/inline.rs`, but is parked + capped at 64 morphisms); (c) loop→map
lifting is the unlock he cares about (matmul4 loop form pinned non-tiling this
session). Session log for S26b + commits pending.
Next step: S26b benchmark reframe (threaded cpp/rust baselines + one mini-box), then
Sapir's call on Inline wiring / loop→map rung.
Resume command/check: `docs/performance/matmul/s26.md`; `git status` (all S26 work is
uncommitted working-tree state).

## 1. Work completed

- **WP0:** plan doc `plan-s26-register-blocking.md` (model-first; TileSite record
  verified to already carry the TI fact — `b.ci == 0`; zero flow-ir change).
- **WP1+WP2 (one emitter wave, `crates/backends/llvm/src/func.rs`, +518/−2):**
  module consts `TILE_J=16` (moved) + `TILE_I=4`; gate `site.rows > 1 && site.b.ci
  == 0` → `emit_tiled_map_blocked`; else rung-1 nest byte-identical (1-D FIR sites
  stable). New: `TileCtx`, `emit_tiled_map_blocked`, `emit_tile_j_split`,
  `emit_tile_row_split_j`, `emit_tile_trio`. Nest: head boundary rows (TI=1, signed
  jw clip) → TI=4 interior full-window rows (new bounds `i_fw_lo = udiv(lo+C-1,C)`,
  `i_fw_hi = udiv(hi,C)`) → tail rows (TI=1); j loop = constant-16 main + runtime-tj
  remainder; acc = flat `[64 x elem]` entry-block scratch; per k: 4 scalar a-loads +
  ONE b load per lane reused across 4 fma chains (b-traffic ÷4). **One impl bug caught
  + fixed in-session:** remainder-tile seed splat must be one lane loop PER SUBROW
  (a flat rows*tj loop leaves subrows 1-3 unseeded — caught by N=20 A/B).
  Verified pre-test: `--no-tile` byte-identical; FIR tiled byte-identical; N=4/16/20/
  21 cap cases byte-equal at -O0/-O2; FLOW_PAR=2/3/8 byte-equal (p=3 splits mid-row ⇒
  boundary-row TI=1 paths executed).
- **WP3 (tests):** `golden_tile_map_shapes` re-pinned deliberately (64-elem alloca;
  4 udivs; 16× constant-16 lane-guards (15 loops + 1 iota coincidence, commented);
  3 min-clip selects / 7 total; snapshot regenerated via `INSTA_UPDATE=always`).
  `tile_nest_shape_1d` + `untiled_map_shape` byte-stable. Differential: matmul nest
  pins updated; shared `assert_tiled_parity`; **two new cases closing the rung-1
  coverage gap** — `r5_c20_k7` (j-remainder tj=4 + i-tail; the old 4×4 fixture never
  ran the constant-16 main body at runtime) and `r6_c32_k5` (two full main tiles +
  rows%4=2 tail). FIR differential untouched. Full gate green: **906** (ir 176 ·
  llvm 42 = differential 19 + golden 23 · rt 16 · others 672).
- **WP4 (local measure):** new repeatable `benches/matmul/tile_ab.sh` (emit
  tiled/no-tile × plain/perf via stdout redirect, clang -O2 -march=native
  -ffp-contract=fast vs `target/release/libflow_rt.a`, FLOW_PAR=1, min-of-N, R1
  byte-equality hard-fail; macOS `ulimit -s hard` fallback). A/B (1t, compute):
  512 f32 6.03 vs 77.24 (**12.8×**) · 512 f64 9.8× · 1024 f32 35.04 vs 820.79
  (**23.4×**) · 1024 f64 12.3×. TI sweep 2/4/8 → **4 shipped** (8 spills: 128
  accumulators ≫ 32 NEON regs). Disasm: 34 `fmul.4s` + 34 `fadd.4s`, zero scalar
  (f64: 56 `fmul.2d`+56 `fadd.2d`); clang/arm64 keeps split mul+add (S25 reading
  holds). Shapes: fir_256 `991/-1484` ✓, attn_16 `299680/184913` ✓; attn_256 tile
  12.1× vs no-tile (2 chained sites); fir parity class unchanged.
- **WP5 (box, agent-run):** instance 45632146 (EPYC 7B12 zen2, 128 visible, **cgroup
  v1** quota 61.44 → pool 62 via the flow-rt v1 fallback, RTX 5060 sm_120,
  $0.1869/hr, **destroyed, ≈$0.12**). clang-18 via llvm.sh (the standing gotcha fix —
  DONE). `results-s26.csv` (122 rows, one-box). Headline: chapel-mc f32@1024 15.9 vs
  117.4 (**7.4×**), f64 **5.1×**, N=256 flips flow 1.4× — chapel loses every cell
  ≥256. numpy f64 like-for-like 13.8× → **7.4×** @1024. 1t f32@1024 wall 84.9 vs
  S25's 568.3 (6.7×). f32 par@1024 flat vs S25 (memory/startup floor — rung 3 owns).
  Disasm: 28 ymm `vmulps`/`vaddps`, **0 xmm** (split worked) but **`vfmadd` absent**
  under -ffp-contract=fast (~2× FLOP density finding → S27 decision for Sapir).
  Skips/deviations: matmul128.ll stall reproduces on clang-18 znver2 (**not**
  clang-15-specific — record corrected); cuda legs via driver-590 PTX JIT (sm_120),
  byte-exact, not cross-session comparable; 25 cap `.ll` regenerated, `.cu`
  byte-stable.
- **WP6 (rollups):** backend-llvm IMPLEMENTATION.md (new symbols + pins) ·
  backend-llvm STATUS.md (S26 entry) · plan doc → SHIPPED S26 w/ deviations block ·
  runner.py spec-stamp block (`# utc/cpu/threads/core_quota/ram_gb/clang` atop every
  CSV — Sapir machine-tag rule, standing per `docs/performance/README.md`) ·
  results-s26.csv spec block · `docs/STATUS.md` S26 row · `docs/performance/matmul.md`
  index row · `docs/next-session.md` rewritten to S27 · `s26_box.sh` (llvm.sh route,
  python3-pip re-added per box finding).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| TI gate placement | emitter-side (`rows>1 && b.ci==0`), zero flow-ir change | the record already carries the fact (direction note verified); recognizer untouched |
| TILE_I | 4 shipped (sweep 2/4/8 measured) | 8 spills registers (f64 14.16 vs 9.91 ms); 2 leaves b-reuse on the table |
| Remainder discipline | split loops (head/interior/tail), never mask dead subrows | masked subrow = OOB load + neighbor corruption; T4 proofs cover real cells only |
| Box toolchain | clang-18 via llvm.sh, specs stamped on CSV | standing gotcha closed; clang version is result-changing (Sapir machine-tag rule) |
| Box pick | EPYC 7B12 CPU-cheap box ($0.19/hr) over 4090-alike ($0.79/hr) | S26 is a CPU story; .cu byte-stable from S25 |
| **Sapir, session tail: benchmark framing** | **two tables only — 1t vs 1t; par vs par with best-threaded baselines (c++/rust get their best threading lib too); mt-flow-vs-1t-baseline comparisons dead** | "conflating this is irrelevant, who wins needs to be par on par" |
| **Sapir, session tail: fn-stripping** | **wire it in — the mechanism exists (`flow-rewrite/src/inline.rs`, "functions are a human modularity construct; the optimizer's unit is the flattened primitive dataflow graph") but is parked (not in default `rewrite()`) + capped (INLINE_MAX_BODY=64)** | Sapir: the graph must be generated stripped of functions recursively, else no real reachability and the fn becomes a blocker — correct about the production effect today |
| **Sapir, session tail: loop→map** | **the second unlock; rewrite-level rung, design next** | matmul4 loop form pinned non-tiling this session (graph-shape detector; cap-form twin tiles); no loop-independence proof exists |
| mt-vs-1t rows in s26.md ("475×", "443×") | discarded by directive, reframe in S26b | Sapir |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` | exit 0 — **906 green** (ir 176 · llvm 42 · rt 16 · others 672) | full gate post-rung-2 |
| differential tile cases (4: matmul, fir, r5c20k7, r6c32k5) | green 6.4s — oracle ≡ tiled ≡ untiled, -O0/-O2 | R1 on main/remainder/TI paths |
| golden_ll (23) | green; 1-D + untiled snapshots byte-stable | deliberate re-pin only where the nest changed |
| `tile_ab.sh` A/B (1t, local M-series, min-of-3) | 512 f32 12.8× · 512 f64 9.8× · 1024 f32 23.4× · 1024 f64 12.3× vs no-tile | rung 2 local gain |
| TILE_I sweep @512 | 2: 8.57/12.44 · **4: 5.34/9.91** · 8: 6.14/14.16 (f32/f64 ms) | TI=4 = register-file sweet spot |
| disasm local f32 512 tiled | 34 `fmul.4s`+34 `fadd.4s`, 0 scalar | vectorization directive: PASS (finding would block) |
| box flow vs chapel-mc @1024 | f32 15.9 vs 117.4 (7.4×) · f64 23.3 vs 118.3 (5.1×) | flow ahead every cell ≥256 |
| box flow vs numpy f64 @1024 | 23.3 vs 3.14 → numpy 7.4× (was 13.8×) | rung 2 cashed ~1.9× at par |
| box disasm f32 512 | 28 ymm `vmulps`/`vaddps`, 0 xmm; **no `vfmadd`** | fixed-TJ split unlocked full width; contraction finding → S27 |
| stdout byte-parity | every `out=` consistent across all legs + vs s25.csv | field correctness under rung 2 |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` @ `4070aad` | **ALL S26 work uncommitted** (func.rs, tests, snap, 25× cap .ll, runner.py, s26_box.sh, tile_ab.sh, docs incl. s26.md + results-s26.csv) — commits pending Sapir's confirm | `git status` |
| vast.ai | box 45632146 | DESTROYED (≈$0.12) | — |
| vast.ai | **45622441 STILL RUNNING** — Sapir's own, hands-off (flagged since S25) | unknown | `vastai show instances` |
| uncommitted | `examples/fib.flow` modified | Sapir's own edit, untouched | `git diff examples/fib.flow` |
| untracked | `test.md`, `2026-07-23-233817-*.txt` | not Flow-session files, left in place | — |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **S26b benchmark reframe (Sapir directive)** | `docs/performance/matmul/s26.md` "Who wins" table | threaded cpp (std::thread) + rust baselines, quota-aware width; one mini-box; two tables (1t vs 1t / par vs par); drop the 475×/443× rows | both tables same-machine, par-on-par; no mt-vs-1t rows anywhere |
| P0 | commits for all S26 work | `git status` | Sapir confirm → feature commit + close-out commit | committed |
| P1 | **fn-stripping wired in (Sapir)** | `crates/flow-rewrite/src/inline.rs`, `driver.rs:65-75` | add `PassId::Inline` to the default list (or ahead of tile_plan/path_plan); verify + pin call-in-map-body stripping; lift/revisit INLINE_MAX_BODY=64; 1280-run differential gate | a map body calling a user fn tiles; dead-call refusal only on non-strippable calls |
| P1 | **loop→map lifting design (Sapir's unlock)** | matmul4 non-tiling pin; `docs/notes/tile-ladder-direction.md` | design: canonical-loop SCC proof (carried = counter+output only; disjoint affine writes; no cross-iteration read) → Loop→Map rewrite | plan doc ratified by Sapir |
| P1 | vfmadd absent under -ffp-contract=fast (box, ~2× FLOP density) | s26.md reading 2 | S27 decision for Sapir (fmad-class CPU-face split precedent: S24b) | decided + measured |
| P2 | rung 3: packing + k-panels (the par floor) | tile-ladder note | S27 headline | par f32@1024 moves off ~15.9 ms |
| P2 | shapes → runner legs | next-session S27 agenda | runner.py shape legs | attn/fir standing box numbers |
| P3 | `exp`/transcendentals; non-const fold seeds; `time` builtin; instance 45622441 | next-session open Qs | Sapir | answered |

## 6. Architecture / model changes

None in flow-ir (the rung cashes the recorded `b.ci == 0` — the direction note's claim
verified in code). backend-llvm IMPLEMENTATION.md updated: `emit_tiled_map` now
dispatches gated sites to `emit_tiled_map_blocked` + the split/trio helpers
(`TileCtx`); tile pins row refreshed. **Model-level findings from Sapir's review:**
(1) `flow-rewrite/src/inline.rs` implements his fn-stripping rule verbatim but is
wired to nothing in production (region-pipeline pre-pass only) + capped at 64
morphisms — the fn wall is real in production today; (2) no loop→map independence
proof exists anywhere — loops are canonical LoopMerge SCCs, only last-use Update
analysis consumes them.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/components/backend-llvm/plans/plan-s26-register-blocking.md` | new; SHIPPED S26 + deviations block (per-subrow seed splat fix; head/interior/tail i-split; TI sweep → 4) |
| `docs/components/backend-llvm/IMPLEMENTATION.md` | tile emission row → S26 symbols (`emit_tiled_map` :2089, `emit_tiled_map_blocked` :2375, `emit_tile_row_split_j` :2531, `emit_tile_j_split` :2579, `emit_tile_trio` :2644, `TileCtx` :305, consts :300-301); pins row refreshed |
| `docs/components/backend-llvm/STATUS.md` | S26 entry prepended; S25 demoted (byte-identical) |
| `docs/performance/matmul/s26.md` | new box report (standing format) — **tables to be reframed in S26b per Sapir** |
| `docs/performance/matmul.md` | S26 index row |
| `docs/performance/README.md` | machine-tag rule (S26, Sapir, standing) |
| `benches/matmul/runner.py` | spec-stamp block atop every CSV |
| `benches/matmul/results-s26.csv` | new (122 rows + spec block; one-box) |
| `benches/matmul/s26_box.sh` | new (llvm.sh clang-18; python3-pip; disasm check) |
| `benches/matmul/tile_ab.sh` | new (repeatable local A/B) |
| `docs/STATUS.md` | S26 row (session log + commits pending noted) |
| `docs/next-session.md` | rewritten to S27 agenda |

## 8. Files changed

Code: `crates/backends/llvm/src/func.rs` · `crates/backends/llvm/tests/golden_ll.rs`
· `crates/backends/llvm/tests/differential.rs` ·
`crates/backends/llvm/tests/snapshots/golden_ll__tile_nest_shape.snap`.
Benches: 25 × `*cap*.ll` regenerated (rung-2 emitter) · `runner.py` · `s26_box.sh` ·
`tile_ab.sh` · `results-s26.csv`. Docs: as §7. **Nothing committed** — pending
Sapir's confirm.
