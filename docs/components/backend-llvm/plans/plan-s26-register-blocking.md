# Plan — BLAS rung 2: TI register blocking + the fixed-TJ split (S26 items 1+2)

Status: **SHIPPED S26** (one emitter wave, as the S25 close agenda directed).
Deviations from the plan as written: (1) **the seed splat is per-subrow, not a
flat `rows*tj` loop** — subrow r's lanes live at acc offset `r*TILE_J + lane`,
so a flat seed range leaves the strided remainder lanes (`tj < TILE_J`) of
subrows > 0 unseeded; the shipped trio emits one seed lane-loop per subrow
(found in implementation); (2) the "interior full-window rows" rule shipped as
a concrete three-region i-split — head boundary rows (TI=1, signed jw clip) →
TI-blocked interior `[ceil(lo/C), floor(hi/C))` via `i_fw_lo = udiv(lo + C-1, C)`
/ `i_fw_hi = udiv(hi, C)` → tail rows (TI=1); (3) `TILE_I` swept 2/4/8 → **4**
confirmed by local measurement. Measured local (`benches/matmul/tile_ab.sh`,
min-of-3, tile vs `--no-tile`): matmul 512 f32 **12.8×**, f64 **9.8×**; 1024
f32 **23.4×**, f64 **12.3×**; attn_256 **12.1×**; fir parity class unchanged.
Box leg in flight (`s26_box.sh`; `docs/performance/matmul/s26.md` lands with
it). Original text below.
Predecessor: `plan-tile-emission.md` (shipped rung 1). Direction:
`docs/notes/tile-ladder-direction.md` ("the emitter cashes a fact the record
already holds" — verified S26: `TileSite.b.ci == 0` IS the row-invariance fact;
**no mapal-ir change**).

## Why (evidence)

- **Fixed-TJ split:** box disasm (S25) shows the runtime `tj = min(remaining, TILE_J)`
  bound (`func.rs:2170-2177`) holds x86 clang at xmm/partial vectorization — no `vfmadd`,
  no ymm — while local Apple clang fully vectorizes the same nest. Compilers fully
  vectorize *known* trip counts: emit a TILE_J-constant main body + a scalar remainder.
  Cheap; measured next box.
- **TI blocking:** today each row re-loads the full b-strip (K×TJ per row-tile). With
  `b.ci == 0` the b-vector at `(k, j0)` is row-independent — one load feeds TI rows'
  accumulators: b-traffic ÷TI. Direction note: ~2–4× on the numpy gap at TI=4; the
  register file is the budget that caps TI×TJ.

## Categorical model

Rung 1 recorded the reuse structure of the fanout (each zero lane/row coefficient =
"this load is reused across that variable"). Rung 2 changes only **how many independent
per-cell chains share one loaded value in registers** — again a pure re-scheduling of
independent placements, never a per-cell value change.

| Item | Kind | Model |
| --- | --- | --- |
| TI gate | `Trn` (emitter-local predicate) | `site.rows > 1 && site.b.ci == 0` — the record's row-coefficient zero, cashed. No new analysis. |
| TI×TJ accumulator block | `Dat` (emission-time) | `[TI*TJ x elem]` entry-block scratch; subrow r at `r*TJ + lane`; clang promotes to registers (the budget that caps TI). |
| b-load hoisting | `Trn` placement | one `b[k·ck + j0 + lane]` load per (k, lane) feeding TI `fmul`/`fadd` chains — reuse-is-fanout, materialized. |
| main/remainder splits (both axes) | `Trn` placement | compile-time-constant trip counts on the main body; runtime-bound remainder for partial tiles/boundary rows. |

### Bit-exactness (R1, extended over rows)

Per cell `(i,j)` the chain is still `acc ← add(acc, mul(a,b))`, k ascending
`0..site.k`, operand order `mul_a_first`/`add_acc_first`, constant seed. TI interleaves
TI independent row chains exactly as TJ interleaves TJ independent lane chains — the
rung-1 theorem with "map cells independent, output slots disjoint" applied twice. No
reassociation, no k-reorder, no cross-cell fusion. Gate unchanged: differential stdout
byte-equal at -O0/-O2, tiled == untiled == interp oracle, any thread count.

### Remainder discipline (the correctness-critical design point)

- **j axis:** main loop steps TJ while `j0 + TJ ≤ jw_hi`, lane loops bound by the
  literal `TJ`; one runtime-`tj` remainder tile after. Task-grain splits make
  `remaining` runtime in general — the remainder path is never dead code.
- **i axis:** TI main covers only **interior full-window rows** (whole block has
  `jw == [0, C)`). A task range's first/last rows may be mid-row clipped
  (`jw_lo > 0` / `jw_hi < C`) — those go through the TI=1 path, as do the
  `rows % TI` tail rows. **Never clamp or mask subrow indices**: a dead subrow's a-load
  reads out of bounds (T4 proofs cover real cells only) and its store corrupts
  neighbors.
- 1-D sites (`rows == 1`, FIR/attention-O): TI degenerates off; the nest stays exactly
  the rung-1 shape (`tile_nest_shape_1d` golden must not move).

## Emission (backend-llvm only; `emit_tiled_map`, func.rs:2064-2331)

- `const TILE_I: u64 = 4` beside `TILE_J` (initial value; swept locally 2/4/8 and the
  shipped constant picked by measurement — per-backend width is doctrine, 16/4 are the
  llvm v2 constants, not the language's).
- Refactor the seed/k/store lane-loop trio into one helper taking the lane bound;
  call it with constant `TJ` (main) and runtime `tj` (remainder).
- Gated on the TI predicate: acc `[TI*TJ x elem]`, i-loop TI-blocked main + boundary/
  tail rows at TI=1, k body = TI scalar a-loads (`emit_tile_index` reuse:
  `a.base + a.ci·(i+r) + a.ck·k`) + one b-vector load reused across the TI chains.
- Shared sequential/task flavors unchanged (`bulk_bounds`); `EmitOpts` unchanged;
  `--no-tile` byte-stable (A/B control); `untiled_map_shape` snapshot must not move.

## Tests (the gate, plus closing the rung-1 coverage gap)

Rung 1's differential fixtures are 4×4 / sizes ≡ 0 mod 16 — **the full-tile main path
and both remainder paths are runtime-unexercised today.** This rung restructures
exactly those paths, so new shapes are part of the change, not a follow-up:

- **Re-pin deliberately:** `golden_ll__tile_nest_shape.snap` + structural assertions
  (constant lane bound `… 16` in the main body, TI×TJ alloca, TI FMAs per b-load);
  differential nest pins (`differential.rs:570-573, 633-636`); `select i1` count
  (`golden_ll.rs:506`). Untiled + 1-D snapshots byte-stable.
- **New differential cases** (interp-oracle, -O0/-O2 × tiled/untiled byte-equal):
  matmul C % 16 ≠ 0 (e.g. 20 cols, K=7 — j remainder + full main tiles) with
  rows % TI ≠ 0 (i remainder); rows ≥ TI+1 so the TI main body runs.
- Full gate: `cargo test --workspace --release` (incl. the 1280-run testgen sweep).

## Measurement

- Local A/B script (S25's was ad-hoc; make it repeatable): emit tiled/`--no-tile` via
  stdout redirect (the emit example's `_perf.ll` naming keys on `--perf` only),
  `clang -O2 -march=native -ffp-contract=fast`, `MAPAL_PAR=1`, `MAPAL_PERF total` parse,
  stdout byte-equality asserted, disasm vector-FMA presence check (a shape that
  recognizes but fails to vectorize is a finding, not a pass — rung-1 directive).
- TILE_I sweep 2/4/8 on matmul 512 f32/f64; shapes corpus (fir/attn) oracle pins hold.
- Box: `s26_box.sh` from `s25_box.sh` **with clang-18 via llvm.sh** (the recorded
  gotcha fix), clang version into the CSV, `results-s26.csv` (one-box rule), report
  `docs/performance/matmul/s26.md` matchup/conditions/verdict. Conditional on vast.ai
  balance (0 at S25 close).

## Ceilings (recorded, not built)

Packing + k-panel L2 blocking (rung 3 — numpy-class). Transposed reads (attention
row-major K — needs packing). conv2d derived-var affine forms. cuda consumption of
`tile_plan` (same record; smem tiles → mma, fmad-class parity decision Sapir's).
Non-constant fold seeds (T6). Per-target tile-factor tables (AVX-512 vs NEON vs
cuda warp shapes) — v2 keeps the two constants; the sweep informs the table later.
