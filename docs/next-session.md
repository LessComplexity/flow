# Next Session (S26)

Written: 2026-07-23 · close of Session 25 · by: Claude Fable (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S25 closed: tile emission v1 — the BLAS ladder's first rung, shipped and measured.**
`flow_ir::tile_plan` (affine-triple recognition, 2-D + 1-D lane modes) + the backend-llvm
`TILE_J=16` micro-kernel (per-cell chain order exact ⇒ stdout byte-equal at any thread
count/opt level) + cgroup-quota pool width + the `FLOW_PERF` llvm compute timer. Box
(EPYC 7702P, 62-core quota): **flow 3–8.6× ahead of chapel-multicore** at 512/1024 both
widths; **numpy gap 130× → 13.8×** (f64@1024 like-for-like). `docs/performance/matmul/s25.md`.
Commits `be4e827` + the close-out. Workspace 904+ green; box destroyed (≈$0.55).

## Test state

`cargo test --workspace --release` green 2026-07-23 (ir 176 · llvm 40 incl. the
1280-run -O0/-O2 differential + tiled matmul/FIR cases · rt 16 · remaining crates 672).
Suggestion #9 closed as shipped-at-S20c (proof pinned at the query level).

## The S26 agenda (from the S25 numbers + standing items)

1. **BLAS rung 2 — TI register blocking (the headline):** hold a TI×TJ accumulator
   block, reuse each b-vector across TI rows (b-traffic ÷TI). Same deduction, contained
   `TileSite`/emitter delta, per-cell order preserved (R1-legal). Expected ~2–4× on the
   13.8× numpy gap. Design with the fixed-TJ split (item 2) — one emitter wave.
2. **Fixed-TJ main body + scalar remainder:** the runtime `tj` bound is what holds x86
   clang at xmm/partial vectorization (box disasm: `vmulps` xmm, no `vfmadd`, no ymm;
   local Apple clang does 4-lane+2× on the same nest). Emit the TILE_J-constant main
   loop + a scalar tail → full-width `ymm`/`vfmadd`. Cheap, measured next box.
3. **Shapes → runner legs:** `benches/shapes/` corpus (fir/attn/conv2d) has oracle pins
   + local A/B only; give the box runner shape legs so attention/FIR carry standing
   numbers (attn already: 2 chained tiled sites, 4.6× local).
4. **conv2d derived-var affine forms:** `k/3`,`k%3` inside the fold body → non-affine in
   raw k → refused today (guards still elided). Extending the walker over derived vars
   tiles the conv class. Gate: measured demand.
5. **cuda consumes `tile_plan` + streams consume `path_plan`** (S25/S24 queries, standing).
6. **Region emission v2** (S17 directive; plan exists) · **P2 standing:** arena v1.1,
   17b dedup key, llvm heap lowering (the 1024 ulimit dance + N≥2048 CPU legs), `time`
   builtin (Sapir), P7 Verilog, ADR-0030 protocol.
7. **Docs debt:** ADR-0029/0031 `flow-as-implemented` rows (standing "on ledger close").

## Open questions for Sapir

- **`exp`/transcendentals in Core?** Real attention needs softmax; the op set has no
  `exp`. Language question (ADR-scale: op or builtin family, backend duty, oracle parity).
- **Non-const fold seeds for tiling** (T6 ceiling) — worth lifting when a real program hits it.
- **Unknown vast.ai instance `45610428` running at close** — not created by any Flow
  session (S24's foreign pair gone). Yours? Billing someone. Balance shows 0.
- `time` builtin language-vs-harness (standing since S19); ADR-0023/24/25 in-file Qs.

## Gotchas / warnings

- **Box clang version is a result-changing variable for the tile nest:** apt clang-15 =
  fully scalar (still beat chapel); clang-18 = partial xmm. Install clang-18 via
  llvm.sh on every future box (add to the box script) and record the version in the CSV.
- **This box's clang-15 HANGS >56 min on loop-form array-literal modules**
  (matmul128.ll; S13/S16 pathology) — kill + skip-with-reason; cap forms unaffected.
- 1024+ llvm binaries need `ulimit -s` (alloca stack; runner wraps it — direct ssh runs
  must too). Heap lowering (agenda 6) retires this.
- vast.ai balance 0 — creation still worked (credit), but check before planning box time.
- **CODEX: always `codex exec "..." </dev/null`** (S23 stdin gotcha — held S24+S25).
- All S08–S24 gotchas stand (results.csv one-box rule; 16-core pinning for cuda
  differential boxes; `emit_sweep` before trusting cuda emitters; ssh re-query per retry).

## Commands (currently working)

```sh
cargo test --workspace --release                # full gate (~10 min)
cargo run -q --release -p flow-backend-llvm --example emit -- <f.flow> - --rewrite [--perf|--no-tile]
./benches/matmul/regen.sh                       # re-emit bench artifacts (tiled default)
FLOW_PAR=1 ./mm_ll_cap_1024                     # sequential lever; FLOW_PERF row on _perf builds
# box driver: benches/matmul/s25_box.sh (CPU sweep; + clang-18 via llvm.sh — see gotcha)
# perf home: docs/performance/matmul.md · raw: benches/matmul/results-s25.csv (this session)
```
