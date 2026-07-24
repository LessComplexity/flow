# Plan — S27: FMA contraction (product face) + BLAS rung 3 packing + micro-kernel finishing

Status: **SHIPPED S27** (agenda items 1–3; codex-implemented WP1/WP2/WP2b, orchestrator
line-by-line reviewed). Deviations from the plan as written: (1) the parallel-flavor
pack placement shipped as a **wrapper + nested-session split** — the packed Split task
becomes a run-once wrapper (`@task{i}`: pack, then `flow_par_begin(1)` → slice dispatch
→ help-first `finish`) rather than a pack inside the same task body; sound because the
pool is global (no thread spawn per session), `finish` is help-first (width-1 safe), and
tiled sites are trap-free (nested `check_trap` vacuous); (2) **orchestrator review found
a correctness hole codex's WP2 shipped**: every packing site got a Frame field but only
Split tasks got the wrapper — a tiled site inside a LOOP (Seq task, loop-carried b)
read an uninitialized buffer; fixed (WP2b): Frame fields for Split sites only, all
other contexts pack inline at the site per iteration (also the loop-varying-b
correctness rule); regression `differential_tiled_matmul_loop_carried_pack`, pre-fix
output garbage; (3) **k-panel deferral REVISED at Sapir's S27-close review** — the deferral
under-counted: packing made b-reads sequential but not smaller (per thread every
i-block re-streams the whole packed b, N³/TI — the measured par floor). The win
decomposes: (3a) **panel residence** — j-tile OUTER / i-block inner per thread, the
panel resident in private L2 across the thread's i-blocks (b-traffic ÷4 @1024, ÷16
@4096 per thread), ZERO acc spill (k unsplit; cell visit order only ⇒ byte-exact,
the rung-1 theorem) — **shipped S27b (WP4)**; (3b) true KC split (acc persist/reload,
a/b balance — full BLIS shape) stays the box-gated refinement; (4) local numbers
pre-WP4: 1024 f32 tile 32.8/fma 19.3 ms (1.81× vs S26), f64 64.0/38.6 (1.73×);
packing +16% f32@1024 local only (M-series SLC — the zen2 box is packing's real
test, pending balance). Original text below.
Predecessor: `plan-s26-register-blocking.md` (rung 2, SHIPPED). Direction:
`docs/notes/tile-ladder-direction.md`.

## Why (evidence)

- **FMA (~2×):** S26 box disasm — 28 ymm `vmulps`/`vaddps`, zero `vfmadd`, under
  `-ffp-contract=fast`. **Hypothesis verified locally (S27 open):** since LLVM 14
  contraction is gated on per-instruction fast-math flags in the IR; the driver flag
  does not retrofit textual IR. Golden test on `matmul512_cap_f32.ll`: plain IR +
  `-ffp-contract=fast` → **0** fused ops; same IR with `fmul contract`/`fadd contract`
  → **34 `fmla.4s` + 2 `fmadd`, 0 unfused vector mul/add**. The flag is the whole fix.
- **Packing (~1.5–2×, owns the par floor):** per j-tile the k-loop walks b rows at
  N-elem stride — K cache-line regions per tile, re-walked for every i-block
  (N/TI per column sweep). b does not fit L2 at N≥1024 (4 MB f32) ⇒ DRAM-bound;
  par f32@1024 flat S25→S26 (15.7→15.9 ms) is this wall. numpy-1t (OpenBLAS packed
  kernels) beats flow-1t 3.3× f32@1024 — packing + FMA is that gap's decomposition.
- **Micro-kernel finishing (~1.1–1.3×):** f64 today runs TJ=16 ⇒ 4 subrows × 8
  2d-vectors = 32 acc regs = the whole NEON file ⇒ spill pressure (f64 gains lag f32
  everywhere: 12.3× vs 23.4× local @1024). Per-width TJ (f64→8) + k-unroll ×2 +
  packed-panel prefetch are standard finishing, measured in the same loop.

## Categorical model

FMA changes the **value** of the per-cell chain (single rounding) — it is a *product-face
semantics choice*, not a scheduling change; R1 (oracle byte-equality) cannot absorb it.
Packing changes only **where** the kernel reads b from — a new `DataLoc` over the same
`Dat`, values and per-cell chain order identical; R1 absorbs it by construction.

| Item | Kind | Model |
| --- | --- | --- |
| `contract` flags | `Trn` semantics (product face) | per-cell chain becomes `fma`-contractible: `fmul contract`/`fadd contract` on the tile-kernel chain only. Opt-in `EmitOpts::contract=false` default — the conformance face (differential, goldens, oracle) never sees it. S24b precedent (GPU `-fmad`): product recipe contracts, conformance gate stays bit-exact. |
| packed b panel | `Dat` placement (`DataLoc`) | one contiguous j-tile-major copy of b: `packed[jt][k][lane]`, lane-contiguous per k, k-sequential per j-tile — the k-loop's b-read becomes stride-TJ sequential. A pure layout functor: same values, same read order per cell. |
| pack pass | `Trn` placement | one pass over b in the site-owning task, **before** the parallel slice dispatch (happens-before all slices); sequential v1 (`ponytail:` ceiling — parallelize via the same slice mechanism if it shows in FLOW_PERF). |
| per-width TJ | emitter const table | `TILE_J: f32→16, f64→8` (TI×TJ acc regs: 16 4s vs 16 2d — both half the file, headroom for b/a lanes). Packed layout width follows the same const. |
| k-unroll ×2 | `Trn` placement | two sequential k steps per iteration body — ascending k order preserved, pure branch-count reduction. |
| prefetch | `Trn` placement | `llvm.prefetch` of the next packed k-line; hint-only, no semantic content. |

### Bit-exactness split (the gate structure)

- **Conformance face (default, `contract=false`):** packing + per-width TJ + unroll +
  prefetch all preserve the per-cell chain (same values, same order) ⇒ differential
  stdout byte-equal at -O0/-O2, tiled == untiled == oracle, any thread count, any
  `FLOW_PAR`. The existing 1280-run gate + tile differentials run UNCHANGED.
- **Product face (`--contract`):** single-rounding ⇒ bits legitimately differ from the
  oracle. Verification is numeric: rel-error vs the plain build ≤ 1e-4 (f32) / 1e-12
  (f64) on every printed value, checked by `tile_ab.sh`'s fma leg (byte-equality is
  asserted only plain-vs-plain). Runner box legs carry `fma` as a separate leg label.

## Emission (backend-llvm only; flow-ir untouched again)

1. `EmitOpts` gains `contract: bool` (default false) and `packing: bool` (default
   **true** — packing is rung 3 of tiling; `--no-pack` exists only for A/B
   attribution). CLI: `--contract`, `--no-pack` on the emit example.
2. `emit_tile_trio` k-body: when `contract`, emit `fmul contract`/`fadd contract`
   (both faces share one code path — a flag on the instruction formatter, not a fork).
3. Pack emission: at the site-owning task, before slice dispatch: alloca/frame buffer
   `[K*C x elem]` (64-aligned), pack loop j-tile-major; kernel b-loads rewritten to
   `packed_base + jt*(K*TJ) + k*TJ + lane`. Remainder j-tiles pack at their runtime
   `tj` width padded to TJ (dead lanes zero — never read: lane loops bound by `tj`).
4. Per-width TJ: `tile_j_for(elem)` replaces the `TILE_J` const at all five S26 use
   sites + packed layout; TI stays 4.
5. k-unroll ×2 + `llvm.prefetch` inside the TI-blocked main body only (boundary/tail
   rows stay simple).
6. 1-D sites (FIR): packing degenerates to a straight contiguous copy — keep them on
   the rung-1/2 path unpacked v1 (gate: `tile_nest_shape_1d` byte-stable).

## Tests

- Default-face: full workspace gate (906) + tile differentials + goldens — zero new
  divergence tolerated; `golden_tile_map_shapes` re-pinned deliberately for the packed
  nest (pack loop present, packed-base b-loads, per-width TJ); new differential case
  at f64 (TJ=8 main/remainder paths: e.g. r6×c20×k5 f64 — today's f64 fixtures all ride
  TJ=16 shapes).
- Product-face: fma leg in `tile_ab.sh` — numeric-tolerance check + disasm assert
  (`fmla`/`vfmadd` present, zero unfused vector mul in the kernel: the S26 "vectorize
  or it's a finding" directive extended to contraction).
- `--no-pack` byte-identical to pack on stdout (R1 attribution control).

## Measurement

- Local: `tile_ab.sh` grows fma + no-pack legs; sweep matrix {pack on/off} ×
  {contract on/off} × {512, 1024, 2048} × {f32, f64}, FLOW_PAR=1 min-of-3; f64 TJ
  sweep {8, 16} to confirm the table; disasm checks per above.
- Box (one box, clang-18 via llvm.sh, runner-stamped specs): standing legs + fma leg,
  **2048/4096 rows on every table** (S26c directive — verify `ulimit -s unlimited`
  holds for 3×134 MB f64 allocas at 4096; if not, heap lowering becomes the recorded
  enabler and 4096 ships cuda-only), 1t-on-1t + par-on-par framing only (S26b
  standing). Report `docs/performance/matmul/s27.md`. Budget ~1–2 h box time
  (naive 1t baselines at 4096 ≈ 8 min/run — run them once, min-of-1, labeled).

## Ceilings (recorded, not built)

kc-panel L2 blocking (acc spill/reload per panel — only if 4096 measurement shows the
packed walk missing L2); a-panel packing; parallel pack; shared-vs-per-thread pack
tradeoffs beyond v1; FIR/1-D packing; cuda `tile_plan` consumption (same record,
smem tiles = the packing analogue); contraction outside the tile kernel.
