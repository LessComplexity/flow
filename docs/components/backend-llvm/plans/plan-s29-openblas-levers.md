# Plan — S29: close the OpenBLAS gap (KC k-panel + A-pack) + heap lowering

**Status:** written alongside the KC build (design fixed pre-build, doc same-session).
Predecessor: `plan-s28-shapes-ladder.md`. Target (measured, `results-s27.csv`,
box EPYC 7B13 zen3): numpy-OpenBLAS threaded 9.7×/5.9× ahead of flow-fma
@1024/@4096 f32; numpy-1t 2.7× ahead of flow-fma-1t wall @1024. Sapir: close the
gap in one session — KC k-panel split, A-panel packing, heap allocation; and a
`time` builtin (separate plan doc) to time the kernel from inside the program.

## Why (evidence)

Per-core, flow is ~8 GF/s vs OpenBLAS ~47 GF/s on the box — not a FLOP problem,
a **traffic** problem. The S27b nest is j-tile-outer over i-blocks: every A
element is re-read once per j-tile (C/TJ = 64× @1024, 256× @4096). At 4096 f32
that is ~16GB of A traffic vs 64MB ideal; B already rides the packed panel
(L2/L3-resident per j-slice). The heap wall is separate: every array is an
`alloca` — 1024² f64 ×3 + packed panel ≈ 28 MB, 2048+ exceeds macOS's 64 MB
hard stack ceiling (flow 2048/4096 rows absent locally).

## Categorical model

Both levers are the rung doctrine again — the emitter cashes facts the record
already holds; **zero mapal-ir change**. The recorded `TileSite` carries the
full address formula per read (`base + ci·i + clane·lane + ck·k`); depth `k`,
strides `ci`, and panel geometry ARE the blocking inputs. The loop-nest
transformation is placement-level (`TrnLoc`): same per-cell morphism chain,
different traversal order over the recorded iteration space. Heap lowering is
a `DataLoc` change: the same `Dat` array placed in a heap arena instead of the
stack frame — the model's "one type, three `DataLoc`s" made literal.

**Backend-genericity rule (Sapir, S29 — the CUDA contract).** Every rung lands
as either (a) a *generic* graph fact added to a mapal-ir query (like
`TileRead.ksplit` — no machine constants, no backend vocabulary), or (b)
emitter-local cashing with **zero mapal-ir change** (gates + emission +
backend-owned constants). mapal-ir never learns machine facts; backends never
re-derive graph analysis. Consequence: the cuda backend's smem/mma path is
pure consumption of the same `tile_plan` — lane independence → thread lanes,
broadcast/unit-stride → smem broadcast/coalesced, `k` depth → the k-panel
loop, `ksplit` → conv windows — with the tf32 `mma` parity split the one
policy item (product face, S24b precedent). This plan's items are all (b).

| Item | Kind | Model |
| --- | --- | --- |
| `TILE_KC = 128`, `NC = TILE_J × 32` | emitter const table | k-panel depth / j-block width: the (kc × jb) B working set = 256KB f32 / 128KB f64 — sized to L2 (zen3 512K, M4 larger) |
| jb → kc → ib → jt nest | `TrnLoc` (strategy) | new parallel realization of the packed-site contract; per-cell k stays ascending (R1 bit-exactness) |
| `acc [TI×TJ x elem]` (1KB) | `DataLoc` | the live j-tile's accumulators — the SAME width as the jt-outer nest's. **Amended in build (S29):** this was first sized `TI×NC` to hold a whole jb block across the kc loop. With `kc` OUTSIDE `ib`, other i-blocks run between two panels of the same block, so nothing survives in scratch; partial sums park in `out` instead (next row) and 31/32 of a `TI×NC` block would be dead space |
| partial-sum parking in `out` | `DataLoc` | what the (jc, kc, ic) order costs: each j-tile spills its acc to `out` at every panel end and reloads at the next; the peeled `kc==0` panel seeds instead of reloading. Value-preserving, per-cell k stays ascending ⇒ R1 |
| `apack [TI×KC x elem]` (2KB) | `DataLoc` | the A-panel pack: strided rows → contiguous 64-aligned scratch, packed per **(jb, kc, ib)** — not once per (ib,kc): with `jb` outermost every block re-packs, C/NC times per element |
| `mapal_rt_alloc` arena | `DataLoc` (heap) | the **entry function's** big blocks (≥ `HEAP_MIN_BYTES` = 256 KB) placed in a runtime arena, freed after `mapal_par_finish`/fn end; everything smaller, and every non-entry function, keeps `alloca`. **Amended in build (S29):** the plan said "big arrays", but in the parallel flavor there are no per-array allocas — `build_frame_layout` packs them all into ONE `%Frame` and the entry emits `alloca %Frame`, so the block that blows the stack IS the frame (67 MB at 2048² f32). The gate is therefore on any entry-block block: frame, packed panel, or (sequential flavor) per-object slot |

**Traffic math (the point).** Per A element, reads drop from C/TJ (once per
j-tile) to K/KC adjusted — A traffic ÷ ~16 at 4096. Each (kc,jt) B slice is
KC×TJ (8–16KB, L1); acc 1KB (L1); B panel slices stream from L2. The parking
traffic is the counterweight: `out` is read+written once per k-panel, i.e.
K/KC × 2 × (rows×C×elem) — 64 MB at 1024 f32 (8 panels) against the ~250 MB of
A traffic the nest removes. Per-cell accumulation order is unchanged (kc outer,
k inner ascending) ⟹ bit-exact vs the S27 nest — the differential suite
enforces.

**Measured outcome (S29, after the build).** The traffic model above is sound and
irrelevant: the nest is 3× SLOWER at 1024 f32, and the cause is not traffic at all.
A `TILE_KC` sweep varies parking traffic 4× and moves the clock 1.3%; disassembly shows
the `[64 x float]` accumulator alloca is register-promoted in the jt-outer leg and NOT in
this one (92 `str q…,[sp]` vs 0). The traversal's own costs — parking, the A-pack, the
extra loop levels — total ~3%. See `docs/performance/matmul/s29.md` §1 and suggestions
#16; the follow-up is a codegen fix (promotable accumulators), not a re-tune.

**Composition rules.**
1. The jb/kc nest applies only to packed sites with `site.k > TILE_KC`; smaller
   K keeps the S27b nest byte-for-byte (negative control).
2. The unpaced (`--no-pack`) path, rung 1, window1d, conv rungs: byte-identical.
3. acc seed/store discipline: seeded in the peeled `kc==0` panel, reloaded from
   `out` in every later panel, and stored back at EVERY panel end (parking —
   amended from "stored at the last kc" when the nest order fixed it); j
   remainder tiles get the same treatment inside their jb block.
4. Heap arena frees happen only after all tasks that can read the arrays
   finished (after `mapal_par_finish` in the par flavor). Realized as ONE
   `mapal_rt_free_all()` immediately before the entry fn's `ret` — which is
   past `mapal_par_finish` and past the return value's load, so it satisfies the
   rule in both flavors with a single emission point. Only the entry fn
   registers blocks, so "free everything" is exactly "free mine".

## Emission work items (backend-llvm only)

1. `emit_tile_packed_kc` nest replacing `emit_tile_packed_j_outer` for
   `packing_site(site) && site.k > TILE_KC`: jb blocks (runtime-short last),
   kc panels (runtime-short last), i-regions per (jb,kc) reusing the existing
   head/interior/tail logic, A-pack scratch per (ib,kc), acc scratch per jb.
2. A-pack copy loops (per (ib,kc), rows of THIS block).
3. Prefetch discipline inside panel bounds.
4. Heap lowering: `HEAP_MIN_BYTES = 256 KB` (a backend emission constant beside
   `TILE_I`/`TILE_KC`) gates `FnEmit::entry_alloc`, the one seam every
   entry-block block now goes through — `allocate_local_slots`,
   `packed_buffer`, `allocate_frame_packs` — plus the `%Frame` block in
   `emit_parallel`. Sizing is `func.rs:llt_bytes`, an LLVM-`StructLayout` walk
   over the emitted type text (the closed `ptr`/`float`/`double`/`iN`/`[n x
   T]`/`{ … }` grammar `ty.rs` produces); it is exact, checked against
   `ptrtoint (ptr getelementptr (%Frame, ptr null, i32 1) to i64)`. An `alloca`
   result and a `mapal_rt_alloc` result are both a `ptr`, so no `getelementptr`
   consumer changes. Teardown per composition rule 4. `mapal-rt` gains
   `mapal_rt_alloc`/`mapal_rt_free_all` (a `Mutex<Vec<(addr, Layout)>>` registry
   over `std::alloc`; uninitialized, like the `alloca` it replaces — every
   emitted consumer writes before it reads). The `module.rs:HEAP_DECLS` block
   is gated on the **emitted text** containing the call, not on a re-derived
   predicate: the call is the requirement, so the two cannot drift, and a
   program that heap-allocates nothing is byte-identical to before.

## Tests

- differential (byte-equal vs untiled + interp oracle, -O0/-O2): K % KC ≠ 0,
  C % NC ≠ 0, rows % TI ≠ 0, MAPAL_PAR splits mid-jb and mid-kc.
- golden: packed-nest goldens re-pinned DELIBERATELY (structural: jb/kc loops,
  apack scratch, acc shape); small-K packed golden byte-stable.
- heap lowering: 2048/4096 flow legs run locally on macOS (the stack wall
  retired); matmul artifact outputs byte-equal to pre-lowering runs.
  Landed as `golden_ll:golden_heap_lowered_frame` (structural: the arena decls,
  no `alloca %Frame`, the pinned `sizeof(%Frame)` operand, exactly one teardown
  after the join — plus the n=64 negative control, which gains not even a
  declaration) and `differential:differential_heap_lowered_arrays` (above/below
  the threshold, byte-equal to the interp oracle at -O0 and -O2; the fold reads
  every cell, so a short block corrupts rather than passes). Every pre-existing
  golden is untouched — the byte-identity claim, enforced not asserted.
- Full gate: `cargo test --workspace --release`.

## Measurement (standing rules)

- Compute-only legs; numpy (OpenBLAS box / Accelerate local) in every verdict
  table; sizes to 4096 minimum, shapes scaled up (fir 1M+, conv2d 1024+).
- tile_ab.sh local A/B per rung; box re-run for the frontier comparison.
- Done-when: numpy-1t gap @1024 < 2× (stretch: parity); threaded gap shrinking
  toward 2×; honest report whatever lands.

## Ceilings (recorded, not built)

- Heap lowering is **entry-function only**. A big array local to a called
  `Named` fn or a Map/Fold body keeps its `alloca`: those functions run an
  unbounded number of times, so arena placement without a per-call free would
  grow without bound — and a per-call free needs a last-use point the emitter
  does not compute yet. Consequence: a `matmul2048` written with the kernel in
  a named fn still hits the stack wall; `matmul2048_cap_f32.mapal` (all in
  `main`) does not. Upgrade path: `mapal_rt_free(ptr)` + `LastUsePlan`-driven
  free points, at which stage `mapal_rt_free_all` becomes the fallback.
- The arena is one global `Mutex<Vec<…>>` with a free-everything teardown —
  sound because allocation happens a handful of times in the entry prologue and
  never in a hot loop. Per-allocation lifetimes only matter alongside the row
  above.
- KC/NC/TJ per-target sweep (the constants are v1 guesses; 4096 data decides).
- Parking-free variant: reorder to jb → ib → kc → jt so a `TI×NC` acc survives
  the kc loop and `out` is written once. Trades the parking traffic for a B
  working set of K×NC per i-block (2 MB @1024 f32, L2-blown) — the reason the
  (jc, kc, ic) order is the BLAS one. Recorded, not built.
- Prefetch clamp: the KC trio inherits the j-outer trio's unclamped
  `panel_base + (kk+2)*TJ` line-ahead, which reads one line past the panel on
  the last k pair. `llvm.prefetch` cannot fault, so this is deliberate.
- f64 unroll/TJ refinement; alignment tuning; NUMA-first-touch on the box.
- OpenBLAS-class inner-kernel intrinsics (flow stays textual-IR + clang —
  the gap that remains after traffic parity is the assembly margin, recorded).
