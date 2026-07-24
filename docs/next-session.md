# Next Session (S28)

Written: 2026-07-24 · close of Session 27+27b · by: Claude Fable (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S27 shipped the three numpy-gap closers + fn-strip; S27b the ratified loop→map/fold lift +
panel residence (matmul4 naive loop form now lifts → tiles, `-275/3748` exact); S27c the local
same-machine matrix + shapes cross-language baselines + the measurement-fairness fix.** Fair
compute-basis verdicts @1024 (M4 Pro): flow-fma 19.6×/11.8× over cpp-mt f32/f64 par-on-par,
42.7×/22.2× 1t-on-1t — **ahead at every size incl. 256**; fir mid-pack (no 1-D rung yet);
conv2d 7.9× behind 1t-cpp (the S28 focus). Box still blocked (balance 0). Logs:
`sessions/2026-07-24-s27-*.md` (three: s27, s27b, s27c).

## Test state

`cargo test --workspace --release` green 2026-07-24 (72 suites, orchestrator-run): ir 176 ·
llvm 47 (23 differential incl. the 1,280-run lift-observing sweep + matmul4 acceptance · 24
golden) · rewrite 68 · rt 16; fmt clean. Bench artifacts regenerated on the final pipeline
(loop-form legs tiled); `.cu` byte-stable through S27, regenerated only for lifted loop forms.

## Standing direction (unchanged: `docs/notes/tile-ladder-direction.md` + S26/S26b/S26c rules)

Same-machine comparisons, specs stamped on CSV (runner.py does it). 1t-on-1t / par-on-par verdicts only.
Perf tables to 4096 minimum. Product recipe contracts (S24b GPU + S27 CPU), conformance gate bit-exact.

## The S28 agenda (Sapir, S27 close: **"focused on general ability of what we implemented
## to lift up fir & conv2d from the naive implementations to be better than other languages"**)

1. **THE FOCUS — generalize the ladder to fir & conv2d until flow wins those tables
   (Sapir directive).** Two fronts, both from the S27c measured gaps:
   a. **`tile_plan` records + optimizes non-affine/derived-var sites (Sapir: "we HAVE
      to").** conv2d's `k/3`,`k%3` (constant divisors) are affine in the decomposed
      `(k÷3, k%3)` space — extend the recognizer's derived-var walker so the site is
      RECORDED (not refused) with its decomposition, then cash it: unrolled 3×3
      micro-kernel / im2col-style conv→matmul rewrite. Demand measured S27c: flow
      7.9× behind SINGLE-thread cpp at 14 threads (clang unrolls the compile-time
      3×3; flow emits per-cell div/mod). Done-when: conv2d_512 flow ≥ cpp-mt.
   b. **FIR 1-D blocking rung:** 1-D sites still ride rung 1 (no register blocking,
      no packing) — flow-fma-par 0.36 vs cpp-mt 0.21 ms. Extend the TI/panel
      machinery to the 1-D lane form (output blocking + x-window reuse). Done-when:
      fir_65536 flow-par ≥ cpp-mt/rust-mt, 1t ≥ cpp-1t.
   Naive LOOP forms of both shapes must ride for free (the S27b lift is the vehicle —
   add fir/conv2d loop-form differential fixtures that lift → recognize → win).
2. **Finish the current optimizations to match numpy or more (Sapir).** The NEON-class
   target is OpenBLAS (box AVX2; local Accelerate-AMX is a different-silicon row):
   S27c 1t compute gap @1024 f32 = flow-fma 19.7 ms vs numpy-1t-OpenBLAS (box S26b:
   3.3× ahead of flow's then-84.9 wall — remeasure on the fair compute basis).
   Levers, in order: KC k-panel split (4096 data decides), a-panel packing, f64 TJ/
   unroll sweep refinement, alignment/prefetch tuning. Plus **heap lowering** — the
   2048/4096 local wall (see gotcha below) and the malloc-class fix cuBLAS-style
   libraries take for granted.
3. **Measurement-basis rule (standing from S27c, Sapir's catch):** every verdict table
   is compute-vs-compute (flow `FLOW_PERF` vs baselines' self-timed iteration) — the
   wall table stays separate, labeled as containing flow's process floor. runner.py's
   who-wins extraction + s27_box.sh reporting follow it.
4. **Box run — the zen2/4096 debt (one command, prepped).** Needs balance. rsync
   `benches/matmul/` + `benches/shapes/` → `s27_box.sh` (+ shapes_ab) → results-s27.csv
   + s27.md box section. Expected: packing/residence show on 512K-L2; vfmadd in fma
   legs; 2048/4096 flow rows via `ulimit -s unlimited`; chapel + true OpenBLAS rows.
5. **cuda consumes `tile_plan`** (standing): smem tiles; then extract the
   backend-generic `block_plan` schedule query (suggestions #10, rule of three).
6. **P2 standing:** `time` builtin (Sapir), P7 Verilog, ADR-0029/0031
   `flow-as-implemented` rows, region emission v2, loop-lift v2 rungs (tuple accs,
   non-identity index, non-static bounds).

## Open questions for Sapir

- ~~Commits~~ — DONE at S27c close (Sapir confirmed; see git log: feat/bench/docs split).
- **vast.ai balance** — top up to unblock the box (~$0.05–0.20 expected spend).
- **numpy pairing flag (standing from S26b):** `numpy_bench.py` runs fp32 — re-pair f32-like-for-like or
  keep the f64 convention labeled. Your call, affects the s27.md tables.
- `exp`/transcendentals in Core; non-const fold seeds (T6); ADR-0023/24/25 in-file Qs (all standing).

## Gotchas / warnings

- **vast.ai balance 0 at close** — check `vastai show user` before any box attempt (standing rule held).
- The S27 fma legs' `out=` fields are **numerically-equal-not-byte-equal** to conformance legs BY DESIGN
  (single rounding). Cross-leg byte comparisons: conformance legs only; fma legs compare by rel-error
  (tile_ab.sh does it; report tables must label the class).
- **The STACK problem, plainly (Sapir asked):** flow-llvm materializes every array as an
  `alloca` — stack memory, sized at compile time. 1024² f64 = 8 MB per matrix; at 2048 f32 the
  three matrices + the packed panel total ~64 MB, which is exactly macOS's hard stack ceiling
  (`ulimit -s hard`; Linux root allows unlimited — why the box can run 4096). Baselines use heap
  (`malloc`/`Vec`) and don't care. The fix is llvm **heap lowering** — arrays through flow-rt
  allocation (the cuda backend already has its smart-arena analogue) — which retires the whole
  ulimit dance and unblocks 2048+ everywhere. Agenda item 2.
- `matmul128.ll` znver stall + 16-core cuda pinning + `codex exec … </dev/null` + one-box CSV rule +
  balance check: all standing (S23–S26 gotcha list unchanged).
- Chapel-1t sanity check dropped from s27_box.sh (verified S26b; env pin is standing) — re-add if chapel
  version changes.

## Commands (currently working)

```sh
cargo test --workspace --release                 # full gate — green 2026-07-24
benches/matmul/tile_ab.sh <f.flow> <label> [N]   # local A/B: tile / no-pack / no-tile / fma legs
cargo run -q --release -p flow-backend-llvm --example emit -- <f.flow> - --rewrite [--perf|--no-tile|--no-pack|--contract]
./benches/matmul/regen.sh                        # re-emit artifacts (incl. _fma twins + 2048/4096)
# box (post top-up): see S28 agenda item 1 — benches/matmul/s27_box.sh is the driver
# perf home: docs/performance/matmul.md · local S27 numbers in the index row (box CSV pending)
```
