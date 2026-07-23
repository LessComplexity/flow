# 2026-07-23 — S25: tile emission v1 — bit-exact SIMD, the BLAS ladder's first rung

Orchestrator: Claude Fable. Immutable log (ADR-0017). Implementation by codex CLI
(`gpt-5.6-sol`, xhigh) in three waves per the standing delegation split; every diff
orchestrator-reviewed line-by-line (one R1 hole found + fixed + pinned).

## 0. Continuation brief

Current state: **S25 closed.** Two commits: `be4e827` (feature: tile_plan + micro-kernel
+ pool quota + timer + shapes corpus + regenerated artifacts) + the close-out docs
commit (this log, s25.md box section, STATUS/IMPLEMENTATION/next-session). Workspace
green (ir 176 · llvm 40 incl. the 1280-run -O0/-O2 differential + tiled matmul/FIR
cases · rt 16 · untouched crates 672 = 904). Box destroyed (`45615138`, ≈$0.55).
**Headline: flow-llvm tiled is 3–8.6× ahead of chapel-multicore at 512/1024 (both
widths) and the numpy gap closed 130× → 13.8× (f64@1024 like-for-like) — rung 1 of
the BLAS ladder, measured** (`docs/performance/matmul/s25.md`).
Next step: S26 per `docs/next-session.md` — rung 2 (TI register blocking) + the
fixed-TJ split (full-width x86 SIMD) are the follow-through pair.
Resume command/check: `docs/performance/matmul/s25.md`; `git log --oneline -3`;
`cargo test -p flow-ir -p flow-backend-llvm --release`.

## 1. Work completed

- **Session-start evidence pass (changed the plan before any code):** suggestion #9
  (fold-body guard proofs) found **already shipped** at S20c — regenerated emission
  byte-identical to committed artifacts with ZERO fold-body guards; s24.md reading 6's
  gap attribution stale. Local A/B showed flow 1t ≈ cpp parity at 512 f32 (scalar) —
  the ≤512 box gap re-attributed to spawn floor + 384-thread oversubscription. Disasm:
  both flow and cpp fully scalar; the per-cell fold is a strict recurrence ⇒ **the only
  bit-exact SIMD route is interleaving cells** — tiling became the session's core.
- **Plans (model-first, both SHIPPED):** `plan-s25-pool-timer.md` (WP1 quota width,
  WP2 timer, WP3 bench legs) + `plan-tile-emission.md` (TilePlan/TileSite Dat model,
  T1–T6 legality, the bit-exactness theorem, the micro-kernel shape, v2 §shape-family
  after Sapir's mid-session directive).
- **Wave 1 (codex — pool + timer):** `flow-rt` cgroup-quota width (`cpu.max` v2 / cfs
  v1 pure parsers + `thread_count`; FLOW_PAR absolute; S24-box-shape pin
  `thread_count(None,384,Some(48))=48`) + `flow_perf_begin/end` externs (pool-warm then
  `Instant`; `FLOW_PERF total ms=` grammar) + llvm `EmitOpts`/`emit_with_opts` (+
  `--perf`, `_perf.ll`, runner/regen legs, perf golden; default output byte-identical).
- **Wave 2 (codex — the matmul micro-kernel):** `flow_ir::tile_plan` (recognition per
  T1–T6) + `emit_tiled_map` (TILE_J=16 k-outer/lane-inner nest, per-row range clipping
  serving sequential AND Split-task flavors, entry-block acc scratch, exact
  seed/operand-order reproduction). **Orchestrator review found the session's one real
  defect:** `tile_trap_free` passed `Call`/nested `Map`/`Fold` silently while the
  micro-kernel skips everything outside the recognized chain — a skipped-but-trapping
  subgraph would diverge from the sequential form (R1 break). Fixed (reject outright;
  `allow_fold` for the one recognized fold) + `DeadCall` fixture pinned.
- **Wave 3 (codex — the shape family, Sapir directive):** `TileRead` affine triples
  (`base + ci·i + ck·k + clane·lane`, checked recursive walker, var×var/Sub/overflow
  reject) replacing the hardcoded matmul matcher; **1-D lane mode** (no div/mod ⇒
  `rows=1, c=M`, lanes = t) — FIR/conv1d class rides the same kernel (w ≡ matmul-a
  lane-invariant, x[t+k] ≡ matmul-b lane-stride-1). Matmul emission byte-unchanged
  under the generalization; FIR differential + 1-D golden added.
- **Shapes corpus (`benches/shapes/`, oracle-pinned):** `fir_{256,65536}`,
  `attn_{16,256}` (S=Q·Kᵀ, O=S·V — **two chained tiled sites**; kt column-major),
  `attn_256_rowmajor` (S refused — lane-stride-256; O tiles: mixed program),
  `conv2d_{16,512}` (refused — k-decomposition ⇒ non-affine; guards still elided).
  Scan: structurally a loop, tiling N/A (log-step scan = future rewrite). Softmax
  blocked on `exp` — not in the Core op set (open to Sapir).
- **Box leg (EPYC 7702P 64c, 62-core cgroup quota, ≈$0.55, destroyed):** full CPU
  matrix. Two field findings: (1) apt **clang-15 emits the tile nest fully scalar**
  (flow still 2–5× ahead of chapel on structure); clang-18 vectorizes partially
  (xmm `vmulps`+`vbroadcastss`, no `vfmadd`/ymm — the runtime `tj` bound; the
  fixed-TJ split is the named fix, with local NEON proof it vectorizes). Both raw row
  sets in `results-s25.csv` (`-clang18` suffix). (2) clang-15 **hangs >56 min** on the
  loop-form array-literal module (S13/S16 pathology) — killed, leg skip-with-reason.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Session scope | items 1+2b from next-session, re-derived from evidence | #9 already shipped; guards were not the vectorization gate — the fold recurrence was |
| SIMD mechanism | tile = interleave cells, never split a fold | per-cell fadd chain order is the oracle contract; lanes across cells are free (pure map) |
| Recognition home | `flow_ir::tile_plan` deduced query (BL7) | backend-independent (cuda consumes later); legality proved once |
| Tile factors | backend's choice (TILE_J=16 llvm v1) | region-plan principle: ir owns legality, backend owns cost |
| Tiling default | ON (`EmitOpts::tiling=true`; `--no-tile` for A/B) | product face; differential gates both forms (raw IR takes the untiled fallback) |
| Recognition on raw IR | not required (fires on rewritten) | raw lowering wraps the seed in a capture Proj; bench recipe is `--rewrite`; both forms differential-gated |
| Refuse `Call`/nested sites in tiled bodies | reject outright (review fix) | skipped-but-trapping subgraph = R1 divergence; refusal is the only sound v1 |
| FIR small-K neutrality | accept, record | clang self-SLPs constant-trip folds post-inline; tile owns large-K (2.2× at K=2048) |
| Box toolchain | clang-18 rows = the flow recipe; clang-15 rows kept | version changes the vectorization result; both recorded, suffix-labeled |
| numpy parity framing | rung 1 of 3; gap now 13.8× f64@1024 | honest ladder (TI blocking ~2–4×, packing ~1.5–3× remain); parity is engineering distance inside the same deduction, not new theory |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test` full sweep | 904 green (176+40+16+672) | nothing regressed under three emitter/query waves |
| llvm differential (1280 runs, -O0/-O2) | green incl. new tiled matmul + FIR cases | tiled output ≡ oracle ≡ untiled, both opt levels |
| tile-vs-untile stdout (local, 6 programs) | byte-equal everywhere + ≡ interp oracle | the bit-exactness theorem holds in the field |
| local 1t A/B (compute) | matmul 3.5×/2.5×/4.0× (256/512/1024 f32), 3.4×/3.5× (512/1024 f64); attn 4.6×; fir K=2048 2.2×, K=64 1.0× | rung-1 wins + the honest neutrality boundary |
| local disasm | NEON `fmul.4s`+`fadd.4s` (broadcast, 2×-unrolled) tiled; scalar untiled | SIMD claim verified at instruction level (Sapir directive) |
| **box: flow vs chapel mc @1024** | **f32 15.7 vs 135.3 = 8.6× ahead; f64 33.9 vs 141.0 = 4.2× ahead** | the S23 60×-behind story fully inverted |
| box: flow vs numpy (f64 like-for-like) | 13.8× behind @1024, 9.95× @512 (was 130×) | rung 1 landed inside the projected band |
| box: quota width | 128 visible / 61.4 quota → pool 62 | WP1 live in the field; the 384-thread floor gone |
| box: flow correctness | every `out=` field ≡ oracle values across all legs | field confirmation under tiling + quota pool |

## 4. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | committed through this log | `git status` |
| vast.ai | S25 box `45615138` DESTROYED (≈$0.55). **Unknown instance `45610428` running at close — not created by any Flow session, hands-off, flagged to Sapir (balance shows 0)** | `vastai show instances` |
| artifacts | `results-s25.csv` (131 rows incl. `-clang18` rows) · `s25_box.sh` · `benches/shapes/` corpus · 13 tiled cap `.ll` + 12 `_perf.ll` regenerated | — | `docs/performance/matmul/s25.md` |
| stray (pre-session, untouched) | `PREVIEW.md`, `PREVIEW-matmul512.{cu,ll}` at repo root (stale S23 demo copies) · `examples/fib.flow` modified in Sapir's IDE mid-session | awaiting Sapir | `git status` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P1 | BLAS rung 2: TI register blocking | next-session item 1; plan-tile-emission ceilings | design+implement with the fixed-TJ split | numpy gap < ~5× f64@1024 measured |
| P1 | fixed-TJ main/remainder split (full x86 SIMD) | next-session item 2; box disasm finding | emitter delta + next-box `vfmadd`/ymm check | ymm FMA in disasm, f32 gap tightens |
| P2 | shapes → runner legs | next-session item 3 | runner.py shape legs | attn/fir standing box numbers |
| P2 | conv2d derived-var affine | next-session item 4 | walker extension when demanded | conv2d tiles |
| P2 | cuda consumes tile_plan; streams consume path_plan | next-session item 5 | design note next cuda wave | GPU tile/overlap live |
| P3 | `exp` for softmax · non-const seeds · unknown instance `45610428` | next-session Qs | Sapir | answered |

## 6. Architecture / model changes

New objects/morphisms (all grounded, plan §model): `TilePlan`/`TileSite`/`TileRead`
(Dat, deduced — `flow-ir/src/algo.rs`), `emit_tiled_map` (functor image —
`backends/llvm/src/func.rs`), `EmitOpts` (Dat, llvm), `flow_perf_begin/end` +
`cgroup_quota`/`thread_count` (Trm seam / Trn — `flow-rt/src/lib.rs`). The tile is a
pure re-scheduling of a fanout's placements (one element-body Trn, reordered
`TrnLoc` execution) — §4.5 law 6 exercised again; no coherence law bends. Suggestion
#9 closed as shipped; plan Status lines carry the three recorded deviations
(rewritten-form recognition; Constant seeds; Add-nesting tolerance).

## 7. Docs reconciled

`docs/performance/matmul/{s25.md new, s24.md reading-6 correction, matmul.md index}` ·
`components/backend-llvm/{STATUS, IMPLEMENTATION, suggestions #9 closed, plans/plan-tile-emission (SHIPPED+deviations), plans/plan-s25-pool-timer (SHIPPED)}` ·
`components/ir/{STATUS, IMPLEMENTATION}` · `docs/{STATUS, IMPLEMENTATION, next-session}` ·
this log.

## 8. Files changed

Commit `be4e827` (54 files): `crates/flow-ir/{src/algo.rs,src/lib.rs,tests/algos.rs}` ·
`crates/flow-rt/src/lib.rs` · `crates/backends/llvm/{src/*,examples/emit.rs,tests/*,tests/snapshots ×4 new}` ·
`benches/matmul/{13 cap .ll retiled, 12 _perf.ll new, runner.py, runner.sh, regen.sh}` ·
`benches/shapes/ ×7 new` · `docs/{components/backend-llvm/plans ×2 new, performance/*}`.
Close-out commit: this log + s25.md box section + s25_box.sh + results-s25.csv +
STATUS/IMPLEMENTATION/next-session reconciliation.
