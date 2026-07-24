# Next Session (S29)

Written: 2026-07-24 · close of Session 28 · by: Kimi (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S28 shipped the shapes ladder (Sapir's focus): the FIR 1-D window rung + the conv2d
k-split micro-kernel, and paid the S27 box debt.** fir: both tables WON local AND box.
conv2d: kernel 3× over cpp-mt, box par table won; the M4 par leg is OPEN on the gen
measurement boundary (untiled img-gen inside the FLOW_PERF bracket — the S28 finding).
OpenBLAS frontier now measured on the box (threaded 9.7×/5.9× @1024/@4096; 1t 2.7×).
Gate green; commits pending Sapir. Log: `sessions/2026-07-24-s28-shapes-ladder.md`.

## Test state

`cargo test --workspace --release` green 2026-07-24 (69 suites, orchestrator-run): ir 180 ·
llvm 53 (28 differential incl. conv2d 16/20/92 + fir remainder/split · 25 golden incl. the
re-pinned structural 1-D snap + the conv pin) · rewrite 68 · rt 16; fmt clean. matmul .ll
artifacts byte-identical through S28 (regression-verified).

## Standing direction (unchanged: `docs/notes/tile-ladder-direction.md` + S26/S27c rules)

Same-machine comparisons, specs stamped on CSV. 1t-on-1t / par-on-par verdicts only.
Perf tables to 4096 minimum. Product recipe contracts, conformance gate bit-exact.
Every verdict table is compute-vs-compute (S27c rule) — and now: watch WHICH work the
bracket covers (S28 gen-boundary rule — the legs must time the same work as the baselines).

## The S29 agenda

1. **The gen measurement boundary (the conv2d M4 leg's blocker; suggestion #14 — Sapir's
   call on the fix shape).** The FLOW_PERF bracket spans untiled data-gen maps
   (conv2d_512: ~0.38–0.47 ms gen vs ~0.04 kernel) that cpp/rust/numpy exclude.
   Options: (a) gen-map fusion/inlining (map over iota feeding the conv map — the S27
   fn-strip machinery's sibling), (b) a kernel-scoped perf bracket (`FLOW_PERF` regions
   instead of whole-main), (c) restructure the shapes so gen is outside the timed main.
   Done-when: conv2d_512 flow-par leg ≥ cpp-mt on the M4 too (box already won).
2. **OpenBLAS levers (agenda-2, now with the box frontier):** KC k-panel split (4096
   data decides), a-panel packing, f64 TJ/unroll refinement, alignment/prefetch tuning.
   Target: numpy-1t 2.7× gap @1024 (fair basis), threaded 9.7×/5.9×.
   **Heap lowering** rides here too (the 2048/4096 local wall; alloca → flow-rt arenas).
3. **Conv ceilings, in order (suggestions #11/#12):** TI over output rows (img-row
   reuse ×3, cashes `b.ci == cq`); im2col as a `DataLoc` sibling of `emit_pack_copy`
   (record becomes matmul-shaped → rungs 2+3 unchanged).
4. **GRAIN slicing policy (suggestion #15, measured):** fir box 61T 0.526 → 16T 0.287
   (16 slices = 0.26 waves @61T). Slice-count-aware grain at sub-ms N — flow-rt policy,
   not emitter.
5. **cuda consumes `tile_plan`** (standing): smem tiles; ksplit/window sites included in
   the consumption design; then the backend-generic `block_plan` (suggestions #10).
6. **P2 standing:** `time` builtin (Sapir), P7 Verilog, ADR-0029/0031 `flow-as-implemented`
   rows, region emission v2, loop-lift v2 rungs (tuple accs, non-identity index,
   non-static bounds), attn legs once `exp` lands.

## Open questions for Sapir

- **Commits** — S28 work uncommitted at close (feat: ksplit record + window/conv rungs;
  bench: results-s27.csv; docs: reconciliation + this rewrite). Your confirm splits them.
- **Gen-boundary fix shape** (agenda 1): fusion vs perf-bracket vs bench restructure.
- **Box #1 (45692618) vanish** — was that you, or did it die on its own? It was on-demand
  class; if vast preempted it, that changes the trust model (S28's answer: on-demand +
  incremental log pulls). Box lifecycle: S28 destroyed after use per project norm (~$0.45).
- **numpy pairing flag (standing from S26b):** fp32-like-for-like or keep the f64
  convention labeled.
- `exp`/transcendentals in Core; non-const fold seeds (T6); ADR-0023/24/25 in-file Qs (standing).

## Gotchas / warnings

- **vast.ai: read `credit`, not `balance`** (`vastai show user --raw`); check
  `vastai show instances` for already-running boxes BEFORE renting. Create on-demand
  (omit `--bid_price`); pull logs incrementally (rsync every poll) — box #1's vanish
  lost an hour of matrix because the CSV only exists at the end.
- **Repo moved** (`/Users/lesscomplex/Personal/Flow` → `/Volumes/LessComplex/...`):
  stale build fingerprints bake `env!("CARGO_MANIFEST_DIR")` with the OLD path —
  example-reading tests fail with ENOENT. Fix: `cargo clean -p <path-baking pkgs>`
  (flow-syntax, flow-check, flow-lower, flow-rewrite, flow-interp, flow-backend-cuda)
  after any move. S28 hit this; gate v2 green after the clean.
- macOS stack wall: flow 2048+ local needs the box (or heap lowering — agenda 2).
  `shapes_ab.sh`/`tile_ab.sh` already `ulimit -s hard`.
- The fma legs' `out=` fields are numerically-equal-not-byte-equal BY DESIGN (S27 rule:
  conformance legs only for byte compares; fma by rel-error).
- GRAIN quantization at sub-ms N: FLOW_PAR > slice count LOSES to a balanced count
  (fir: 61T 0.53 vs 16T 0.29). For A/B legs at small N, sweep FLOW_PAR or pin it.
- `matmul128.ll` znver stall + 16-core cuda pinning + `codex exec … </dev/null` +
  one-box CSV rule: all standing (S23–S26 gotcha list unchanged).

## Commands (currently working)

```sh
cargo test --workspace --release                 # full gate — green 2026-07-24
benches/shapes/shapes_ab.sh                      # shapes A/B (par); FLOW_PAR=1 → 1t; FLOW_PAR=N → N threads
benches/matmul/tile_ab.sh <f.flow> <label> [N]   # matmul A/B: tile / no-pack / no-tile / fma legs
cargo run -q --release -p flow-backend-llvm --example emit -- <f.flow> - --rewrite [--perf|--no-tile|--no-pack|--contract]
./benches/matmul/regen.sh                        # re-emit artifacts (matmul only; byte-stable through S28)
# box (next time): on-demand instance + rsync repo/benches + benches/matmul/s27_box.sh driver
#   (see the S28 log §4 for the exact sync/launch/poll/destroy protocol)
# perf home: docs/performance/matmul.md · S27 report (local + box + S28 shapes): docs/performance/matmul/s27.md
```
