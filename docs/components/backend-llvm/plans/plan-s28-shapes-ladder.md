# Plan — S28 shapes ladder: conv2d k-split recording + FIR 1-D rung

Status: PLANNED (2026-07-24, S28 start). Predecessors: `plan-s26-register-blocking.md`,
`plan-s27-fma-packing.md`, `../ir/plans/plan-loop-to-map-fold.md` (the lift vehicle).
Direction: `docs/notes/tile-ladder-direction.md` (§"conv2d / cuDNN": the `k/C`,`k%C`
split inside the fold is the SAME derived-var move the map body already gets).
Sapir directive (S27c close): **"general ability of what we implemented to lift up
fir & conv2d from the naive implementations to be better than other languages."**

## Why (evidence)

S27c local matrix (M4 Pro, fair compute basis, `docs/performance/matmul/s27.md`):

- **conv2d_512: flow 7.9× BEHIND single-thread cpp at 14 threads** — the priced
  refusal. `tile_affine` (`crates/mapal-ir/src/algo.rs:770-830`) walks Add +
  literal-Mul only; conv2d's `img[(i + k/3)*18 + j + k%3]` hits `_ => None`
  (`algo.rs:828`) → site refused → per-cell `sdiv/srem` + untiled body calls,
  while clang unrolls the compile-time 3×3. Everything else conv2d needs already
  passes: bounds proven through captures (guards elided today), trap-free
  (divisor 3 is a safe literal), the map-body `t/C`,`t%C` split found.
- **fir_65536: flow-fma-par 0.36 ms vs cpp-mt 0.21 / rust-mt 0.24 / cpp-1t 0.61** —
  mid-pack. The site IS recorded (`rows=1`, `a=w{ck:1,clane:0}`,
  `b=x{ck:1,clane:1}`) but the rung-2/3 gate (`site.rows > 1 && site.b.ci == 0`,
  `func.rs:2394-2400` = `packing_site` `func.rs:317-319`) is never met, so FIR
  rides rung 1: one acc vector (FMA-latency chain of length K per tile),
  runtime-`tj` select on EVERY tile, w-load per (k, tile).

Done-when (Sapir): `conv2d_512` flow ≥ cpp-mt; `fir_65536` flow-par ≥ cpp-mt /
rust-mt AND flow-1t ≥ cpp-1t. Naive loop forms of both shapes must ride for free
(the S27b lift is the vehicle — differential fixtures included below).

## Categorical model

Two independent moves on the SAME recorded-facts principle (§"the recorded facts
ARE the optimization menu"): extend what the record can say (Part A), then cash
records the emitter already holds (Parts A3, B). No new graph analysis beyond the
one walker extension.

| Item | Kind | Model |
| --- | --- | --- |
| `TileKSplit { div: u64, cq: u64, cr: u64 }` | `Dat` (mapal-ir) | the k-decomposition: address += `cq·(k÷div) + cr·(k%div)` |
| `TileRead.ksplit? : TileRead → TileKSplit` | `Dat` morphism, **Partial** | §3 consolidation: NOT a new site type — the same `TileRead` with one more morphism. `None` = affine-in-raw-k (today's sites, bit-identical); `Some` = derived-var site. Doubles as the emitter-gate discriminator. |
| fold-body k-split detection | `Trn` (recognizer) | the `tile_split` move (`algo.rs:654-671`) one level down: scan fold morphisms for `Div`/`Mod` of the fold element proj (slot `captures+1`) with one shared literal `div`; bind the two target objects as the `kq`/`kr` axes — the same axis-binding-by-identity the map body gets for `(t÷C, t%C)` → `(i, lane)`. |
| `conv_site(site)` gate predicate | `Trn` (emitter-local) | rung doctrine (S26/S27): gates are emitter-local predicates cashing record facts; zero mapal-ir change beyond the record. |
| `emit_tiled_map_conv` | `TrnLoc` (strategy, §4.4) | parallel realisation of the site's `t_from→t_to` contract, selected by the record. Unrolls the (kq,kr) taps: `div`,`cq`,`cr` compile-time ⇒ per-tap constant offsets; div/mod vanish. |
| `window1d_site(site)` + `emit_tiled_map_blocked_1d` | `Trn` + `TrnLoc` | FIR dual of rung 2: TI blocks over the LANE axis (rows==1); one scalar `a` load per k shared across TI subrows (a is the invariant read — roles swapped vs matmul); constant-TJ everywhere on the main path. |
| `acc [TI·TJ x elem]` | `DataLoc` | register-resident accumulators, TI independent chains (ILP) — same placement as rung 2. |

**Composition rules.**

1. A read is affine in raw `k` XOR in `(k÷div, k%div)`: `ksplit.is_some() ⇒ ck == 0`
   on that read (v1 refusal otherwise — no mixed forms).
2. `depth % div == 0` (rectangular window) else refuse — mirrors the
   `mapped_size % c` check (`algo.rs:604`).
3. `ksplit.is_some()` site ⇒ emission takes the conv branch or the untiled
   fallback — NEVER the affine tile path. (Correctness: `emit_tiled_map`
   hardcodes lane coeff 1 and ignores ksplit — `func.rs:2566`; feeding it a
   k-split site emits silently wrong addresses. The guard lands WITH the record.)
4. Every new emission branch keeps the per-cell fold chain **k-ascending**
   (kq outer, kr inner IS k-ascending) — the R1 bit-exactness invariant.
5. Remainder discipline (S26): never mask dead lanes/subrows; constant-TJ main,
   runtime-`tj` only on remainder tiles; par split-range clipping unchanged.

**Bit-exactness.** Conformance face: per-cell add order unchanged ⇒ byte-equal
vs untiled and vs interp oracle (same R1 argument as rung 2). Product (fma) face:
single-rounding class, rel ≤ 1e-4, labeled as today.

## Work items

**A1 — record the k-split site (mapal-ir).** `algo.rs`: add `TileKSplit` +
`TileRead.ksplit: Option<TileKSplit>` (`:148-154`); fold-body analog of
`tile_split` called from `tile_fold_shape` (`:674-768`): find `Div`/`Mod` leaves
on the fold element proj with shared literal `div`, check rule 2, bind `kq`/`kr`
as walker leaves in `tile_affine` (identity pattern of `:779-788`); accumulate
`cq`/`cr` through Add/Mul like the other axes. conv2d_16 img read must record
`{base:0, ci:18, clane:1, ck:0, ksplit:Some{div:3, cq:18, cr:1}}`; `w[k]` stays
`(0,0,0,1,None)`. No change to `bounds_proof`/`tile_trap_free` (already pass).

**A2 — emitter guard (llvm, lands WITH A1).** `func.rs` site dispatch
(`emit_map` `:3622-3626`): `site.a.ksplit.is_some() || site.b.ksplit.is_some()`
⇒ untiled body-call fallback until A3. conv2d emission byte-stable vs today.

**A3 — cash it: unrolled conv micro-kernel (llvm).** New branch at the rung
dispatch (`func.rs:2394-2400`), gate `conv_site`: `b.ksplit = Some{div,cq,cr}`
∧ `a.ksplit = None` ∧ `b.ck == 0` ∧ `a.clane == 0` ∧ `b.clane == 1`. Emission:
per (row `i`, j-tile): fully unrolled `(kq, kr)` taps; per tap, address =
`b.base + b.ci·(i+kq) + b.cr·kr + j0 + lane` — `cq·kq + cr·kr` constant-folds;
`w[kq·div+kr]` constant-index scalar; one `fmul/fadd{ contract}` into the acc
vector per tap. TI=1 in v1. j-remainder via the existing runtime-`tj` split
shape; seq + par-split flavors both (same dispatch points as other rungs).
Unhandled ksplit shapes keep the A2 fallback.

**B — FIR 1-D blocked rung (llvm, zero mapal-ir change).** Gate
`window1d_site`: `site.rows == 1 && site.b.ck == 1 && site.b.ksplit.is_none()`
⇒ `emit_tiled_map_blocked_1d`: full blocks `jb` step `TI·TJ` while
`jb + TI·TJ ≤ jw_hi`; `acc [TI·TJ x elem]`; per-subrow seed splat; k loop
(×2 unroll on the constant-width body, reuse the trio's shape): ONE scalar
`a[a.base + a.ck·k]` shared across subrows; per subrow `r`: constant-TJ lane
loop `b[b.base + k + jb + r·TJ + lane]`, FMA into `acc[r]`; per-subrow stores.
Head/tail: TI=1 constant-TJ main + runtime-`tj` remainder (the
`emit_tile_j_split` shape). Reuse `tile_j_for` (`func.rs:310-315`), `TILE_I`
(`:321`), `TileCtx` (`:331-349`). Non-window 1-D sites stay byte-stable
(negative control).

## Tests

- `crates/mapal-ir/tests/algos.rs`: `tile_conv2d_fixture` (clone the matmul
  fixture `:1398`, add fold-body `k/3`,`k%3`) asserting the full recorded site
  incl. `ksplit`; the two existing `TileRead` literals (`:1595-1620`,
  `:1784-1809`) gain `ksplit: None`; refusal pins: `depth % div != 0`, Div/Mod
  divisor mismatch, mixed `ck≠0 ∧ ksplit` (rule 1).
- `crates/backends/llvm/tests/differential.rs`: `differential_tiled_conv2d`
  (conf byte-equal vs untiled + interp oracle; fma rel≤1e-4; -O0/-O2) at 16
  (oracle) + a `C % TJ ≠ 0` remainder case + MAPAL_PAR split landing mid-tile;
  extend `differential_tiled_fir` (`:741-797`) with `N % (TI·TJ) ≠ 0` and
  par-split-mid-block cases. Loop-form fixtures: naive loop conv2d + fir that
  lift → recognize → win (the S27b vehicle; `tests/golden.rs` example pins).
- `golden_ll.rs`: conv2d tile-nest shape pin (constant-offset taps, ZERO
  `sdiv/srem` in the nest); re-pin `golden_ll__tile_nest_shape_1d.snap`
  DELIBERATELY (FIR nest changes: block loop, `TI·TJ` acc, constant-TJ main)
  with structural assertions in the `golden_tile_map_shapes` style; untiled +
  non-window-1-D snapshots stay byte-stable.
- Full gate: `cargo test --workspace --release` (72 suites green at S27 close).

## Measurement

- Local: `benches/shapes/shapes_ab.sh` then `MAPAL_PAR=1 benches/shapes/shapes_ab.sh`
  (min-of-3; byte-equal conf + rel≤1e-4 fma hard gates). Done-when numbers above.
- Box: the S27-baseline shapes legs are in the box run launched this session
  (instance 45692618); re-run `shapes_ab.sh` on-box after landing for the S28
  column (same box — one-box CSV rule).
- `docs/performance/matmul/s27.md` shapes table gets the S28 rows (or a new
  s28 report if the table shape changes).

## Ceilings (recorded, not built)

- conv2d TI over output rows (img row `i+1` serves output rows `i..i+2` — reuse
  ×3 cashes `b.ci == cq`); **im2col** as a `DataLoc` sibling of `emit_pack_copy`
  (patches buffer ⇒ the record IS matmul-shaped ⇒ rungs 2+3 unchanged);
  Winograd-class transforms (same family, later rung).
- FIR true register rotation of the sliding window (shuffle-level reuse) —
  beyond what the emitter expresses today; clang CSE of L1 reloads is the v1
  approximation.
- cuda consumption of ksplit / 1-D-window sites (smem tiles) — standing agenda 5.
- General ksplit with `ck ≠ 0` (rule 1 relaxed) — needs a 6-wide coefficient
  space; no measured demand.
