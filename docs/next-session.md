# Next Session (S27)

Written: 2026-07-23 · close of Session 26 · by: Claude Fable (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S26 closed: BLAS rung 2 — TI register blocking + the fixed-TJ split, shipped and measured.**
backend-llvm `func.rs` only (TILE_I=4/TILE_J=16; gate `rows>1 && b.ci==0`; flow-ir untouched;
per-cell op order preserved ⇒ stdout byte-equal — R1, differential-gated). Local A/B: tile vs
no-tile 12.8× (512 f32) → 23.4× (1024 f32) at FLOW_PAR=1. Box (EPYC 7B12 zen2, 62-thread quota
pool, clang-18): **flow 7.4×/5.1× over chapel-multicore** f32/f64@1024, chapel loses every cell
≥256; **numpy f64 gap 13.8× → 7.4× @1024**; 1t f32@1024 6.7× vs S25. `docs/performance/matmul/s26.md`.
**Commits pending — Sapir to confirm commit.** Workspace 906 green; box destroyed (≈$0.12).
**S26b closed (Sapir framing directive):** verdicts are 1t-on-1t / par-on-par ONLY — quota-aware
threaded `cpp_mt`/`rust_mt` baselines + numpy-1t/chapel-1t legs, mini-box (≈$0.036, destroyed):
**flow beats every threaded naive-class baseline** (cpp-mt 3.1–10.9×, rust-mt 3.0–9.5×, chapel-mc
3.1–9.6×; f32@1024 12.7 vs 138.1 = 10.9×); numpy-1t beats flow-1t 3.3× f32@1024 (the pure kernel
gap). Logs: `sessions/2026-07-23-s26-register-blocking.md` + `-s26b-par-on-par-reframe.md`.

## Test state

`cargo test --workspace --release` green 2026-07-23: **906** (ir 176 · llvm 42 = the 1280-run
-O0/-O2 differential, now 19 cases — `differential_tiled_matmul_r5_c20_k7` + `_r6_c32_k5` closed
the remainder/main-path coverage gap — + golden_ll 23 re-pinned · rt 16 · remaining crates 672;
+1 ignored perf baseline). matmul4 loop form verified non-tiling (byte-identical emission —
the recorded answer, below).

## Standing direction (Sapir, S25 close — read `docs/notes/tile-ladder-direction.md` first)

**cuBLAS/cuDNN-class out of the box, every backend, from naive source — the language's
edge claim.** The ladder per target lives in the note (CPU: TI blocking → packing;
CUDA: same tile_plan → smem tiles → tensor-core mma, fmad-class parity decision
pending Sapir; conv2d: derived-var walker extension + conv→matmul rewrite; FPGA/ASIC:
a recognized site IS a systolic-array spec — P7's inheritance). **S26 addition (Sapir,
standing): every performance comparison is same-machine, and machine specs
(utc/cpu/threads/quota/RAM/clang) are stamped machine-readable ON the results CSV —
runner.py stamps them; comparison tables stay one machine, cross-machine numbers only
as explicitly labeled cross-session rows.** **S26b addition (Sapir, standing): every
verdict is 1t-on-1t or par-on-par — no multithread-flow vs 1t-baseline rows, ever;
baselines get their best threading too (cpp_mt/rust_mt are quota-aware, flow-rt's own
cgroup rule).**

## The S27 agenda (from the S26 numbers + standing items)

1. **BLAS rung 3 — packing + k-panel L2 blocking (the headline):** owns the 62-thread
   parallel floor — par f32@1024 flat vs S25 (15.9 vs 15.7): the ≥4× 1t kernel gain never
   reached the parallel cell (memory/startup wall, s26.md reading 3). Packing + k-panels
   is exactly the rung that addresses this floor.
2. **vfmadd / fma contraction — DECISION FOR SAPIR:** clang-18 keeps mul/add split under
   `-ffp-contract=fast` (box disasm: 28 ymm vmulps/vaddps, 0 xmm, zero vfmadd) — ~2× FLOP
   density left on the table. Emitting `llvm.fma` changes rounding (single- vs
   double-rounding) = the fmad-class product-vs-conformance split again, this time on the
   CPU face (S24b settled it GPU-side). Spec/oracle implications first, emitter second.
3. **Shapes → runner legs** (standing): `benches/shapes/` corpus (fir/attn/conv2d) has
   oracle pins + local A/B only; box runner legs so attention/FIR carry standing numbers.
4. **cuda consumes `tile_plan`** (+ streams consume `path_plan`) — S25/S24 queries, standing.
5. **conv2d derived-var walker:** `k/3`,`k%3` in the fold body → non-affine in raw k →
   refused today. Extending the walker over derived vars tiles the conv class.
   Gate: measured demand.
6. **fn-strip wiring — the Call wall (Sapir, S26 review; gates item 7):** the mechanism EXISTS
   (`flow-rewrite/src/inline.rs` — "functions are a human modularity construct; the optimizer's
   unit is the flattened primitive dataflow graph", Sapir's rule verbatim, tested) but is PARKED
   (not in default `rewrite()` — region-pipeline pre-pass only) and CAPPED (`INLINE_MAX_BODY=64`).
   So a user `fn` call stays an opaque wall in production today (`tile_trap_free` refuses map
   bodies with Calls). Wire `PassId::Inline` into the default pipeline (or ahead of
   tile_plan/path_plan), pin call-in-map-body stripping, revisit the cap. 1280-run gate holds.
7. **loop→map lifting — the matmul4 gap (Sapir call):** `examples/matmul4.flow`
   (mut/Update/cross-fn Call) does NOT tile — verified byte-identical emission, zero
   tile-nest markers; its cap-form twin `benches/matmul/matmul4_cap.flow` tiles
   automatically. The detector is graph-shape-based, not math-based; lifting the loop
   form to a map is a rewrite-level equivalence proof (canonical-loop SCC: carried =
   counter+output only, disjoint affine writes, no cross-iteration reads) — future rung
   candidate, Sapir's "write it naively, it optimizes" unlock.
8. **P2 standing:** llvm heap lowering (the 1024 ulimit dance + N≥2048 CPU legs), `time`
   builtin (Sapir), P7 Verilog, ADR-0029/0031 `flow-as-implemented` rows ("on ledger
   close"). Region emission v2 (S17 directive; plan exists).

## Open questions for Sapir

- **`exp`/transcendentals in Core?** Real attention needs softmax; the op set has no
  `exp`. Language question (ADR-scale: op or builtin family, backend duty, oracle parity).
- **Non-const fold seeds for tiling** (T6 ceiling) — worth lifting when a real program hits it.
- `time` builtin language-vs-harness (standing since S19); ADR-0023/24/25 in-file Qs.
- **matmul4 loop form — ANSWERED this session** (agenda 6: does not tile; the detector is
  graph-shape-based; lifting is Sapir's call). Recorded; close it.
- **numpy pairing flag (S26b finding):** `benches/matmul/numpy_bench.py` runs **fp32** — the
  standing "f64 BLAS" pairing overstates flow's numpy gap. True f32 cells: numpy 3.9× par,
  3.3× 1t @1024. Re-pair f32-like-for-like or keep the f64 convention honestly labeled — your call.
- **Unknown vast.ai instance `45622441` STILL RUNNING at close** — not created by any Flow
  session. Yours? Confirm it's intended; it bills someone. (S26's own box 45632146 destroyed.)

## Gotchas / warnings

- ~~Box clang version is result-changing~~ → **DONE (S26):** s26_box.sh installs clang-18
  via llvm.sh on every box; version + full machine specs stamped atop results.csv by runner.py.
- **The `matmul128.ll` hang is NOT clang-15-specific:** clang-18 on znver2 also stalled
  >15 min on the loop-form array-literal module (S26; killed per protocol, skip-with-reason).
  Kill + skip-with-reason on ANY znver box; cap forms unaffected.
- **nvidia/cuda:devel images have no pip** — s26_box.sh apt-installs `python3-pip` in the
  clang-18 branch (S26 box deviation; the numpy leg needs it).
- 1024+ llvm binaries need `ulimit -s` (alloca stack; runner wraps it — direct ssh runs
  must too). Heap lowering (agenda 7) retires this.
- **CODEX: always `codex exec "..." </dev/null`** (S23 stdin gotcha — held S24–S26).
- All S08–S25 gotchas stand (results.csv one-box rule — now WITH the runner-stamped spec
  header; 16-core pinning for cuda differential boxes; `emit_sweep` before trusting cuda
  emitters; ssh re-query per retry; vast.ai balance check before box time).

## Commands (currently working)

```sh
cargo test --workspace --release                # full gate (~10 min) — 906 green
cargo run -q --release -p flow-backend-llvm --example emit -- <f.flow> - --rewrite [--perf|--no-tile]
./benches/matmul/regen.sh                       # re-emit bench artifacts (tiled default)
benches/matmul/tile_ab.sh <f.flow> <label>      # local tile-vs-no-tile A/B (FLOW_PAR=1, min-of-3, byte-equal assert)
FLOW_PAR=1 ./mm_ll_cap_1024                     # sequential lever; FLOW_PERF row on _perf builds
# box driver: benches/matmul/s26_box.sh (clang-18 via llvm.sh + python3-pip; runner.py stamps specs; rung-2 disasm check)
# perf home: docs/performance/matmul.md · raw: benches/matmul/results-s26.csv (this session, spec-tagged)
```
