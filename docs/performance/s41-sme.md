# S41 — the SME rung on matmul

Date: 2026-07-29 · Machine: **Apple M4 Pro**, macOS 26.3.1, Homebrew clang/LLVM 22.1.8.
Baseline commit `aaaa5dd` + the S41 working tree. Harness: `benches/sme/sme_ab.sh`.
`FEAT_SME=1`, `FEAT_SME2=1`, `SME_F32F32=1`; SVL measured at **64 bytes** ⇒ 16×16 f32 ZA tiles, 4 of them.

**What changed:** a tile site whose record matches the matmul shape now emits an ARM SME
outer-product kernel (`fmopa` into a ZA tile) instead of the NEON register-blocked micro-kernel,
when the `apple-m4-sme` profile is selected **and** the contract face is on. Same `tile_plan`
record, different leaf — the nest above it is unchanged.

## Method

Both legs are the **contract / FMA face**. `fmopa` fuses (single rounding), so SME is a
contract-face realization under ADR-0032 D1/D3; comparing it against a conformance build would
repeat S36c's error. `--contract` is passed to both sides.

**Value identity is checked before any timing is read** and the harness refuses to print numbers
if the two legs disagree. Both legs printed identical results at every size.

Compute-only: the sources bracket their kernel with the `time` builtin and print `iter ms=`;
nothing here times data generation. Alternating runs, n=31, medians reported with minima.

**The control that validates the harness:** the NEON leg reproduces the recorded S33 numbers.

| | measured here | `s33.md:150-158` |
| --- | ---: | ---: |
| matmul512 1t NEON | 2.2128 ms | 2.1766 ms |
| matmul1024 1t NEON | 17.9611 ms | 17.5449 ms |
| matmul1024 par NEON | 2.2372 ms | 2.2281 ms |

## Single thread (`MAPAL_PAR=1`), f32

| N | NEON (ms) | **SME (ms)** | SME vs NEON | numpy-1t (ms) | numpy ahead — before → after |
| ---: | ---: | ---: | ---: | ---: | --- |
| 512 | 2.2128 | **0.7451** | 2.97× | 0.1600 | 13.6× → **4.66×** |
| 1024 | 17.9611 | **5.4102** | 3.32× | 1.2977 | 13.5× → **4.17×** |
| 2048 | 157.957 | **40.448** | **3.91×** | 10.529 | 14.4× → **3.84×** |
| 4096 | 1243.987 | **332.536** | 3.74× | 84.617 | 14.8× → **3.93×** |

Distributions **disjoint at every size**. n=31 (512, 1024), 21 (2048), 15 (4096).

## Threaded (default width), f32

| N | NEON (ms) | **SME (ms)** | SME vs NEON | numpy-thr (ms) | numpy ahead — before → after |
| ---: | ---: | ---: | ---: | ---: | --- |
| 512 | 0.3836 | **0.2530** | 1.52× | 0.1075 | 3.8× → **2.35×** |
| 1024 | 2.2372 | **0.9715** | 2.30× | 0.6757 | 3.3× → **1.44×** |
| 2048 | 18.6854 | **7.6156** | **2.45×** | 5.3045 | 3.5× → **1.44×** |
| 4096 | 151.0823 | **79.1425** | 1.91× | 44.143 | 3.4× → **1.79×** |

Disjoint at 1024/2048/4096; **overlapping at 512**, so read that row as directional only.

## The shape is the finding — read the GFLOP/s, not the ratios

| N | SME 1t | numpy 1t | SME par | numpy thr |
| ---: | ---: | ---: | ---: | ---: |
| 512 | 360 | 1678 | 1061 | 2497 |
| 1024 | 397 | 1655 | 2210 | 3178 |
| 2048 | **425** | 1632 | **2256** | 3239 |
| 4096 | 413 | 1624 | **1737** | 3113 |

Two *different* problems, and the sizes separate them cleanly:

**1. At one thread the deficit is flat and structural.** SME holds 360→425 GFLOP/s and Accelerate
holds 1678→1624 across a 512× range in problem size. **Both are size-invariant, so the ~4.0× is a
steady micro-kernel deficit, not a blocking failure** — the identical reading S33 gave the
OpenBLAS gap ("both size-invariant ⟹ a steady micro-kernel deficit"). More cache blocking will not
move it; the inner loop has to retire more work per issue.

  Recorded as the first hypothesis to test, **not** as a claim: the kernel accumulates into
  **1 of the 4** available f32 ZA tiles, and the deficit is ~4×. That is suggestive enough to test
  first and weak enough that it may be coincidence.

**2. At width, a blocking failure appears exactly where it should.** SME par climbs 1061 → 2210 →
2256 and then **falls to 1737 at 4096**, while Accelerate holds 3113. A throughput number that
rises with size and then drops is cache, not arithmetic — and 4096² f32 is 64 MB per matrix
against a 16 MB L2. That is the **missing KC rung**, precisely the one `emit_tile_packed_kc`
already implements for NEON and that the SME leaf does not share.

So the two headroom items are not speculative: one is named by the flat 1t curve (accumulator
occupancy), the other by the 4096 par knee (k-panel blocking).

## What this does and does not say

**It does not beat numpy.** Accelerate is still ahead — 4.17× at 1t and 1.44× threaded on 1024².
The honest headline is that **the gap closed by ~3.2× at one thread**, from a factor of 13.5 to a
factor of 4.2, by changing which instruction the innermost multiply-accumulate uses.

**The compiler is within 7% of hand-written SME.** The `benches/sme/mmN.c` ceiling probe — a
hand-written SME GEMM — does 5.0320 ms at 1024² 1t; the compiler-generated kernel does 5.4102 ms.
So the emitter is not leaving much on the table *at this rung*; the remaining distance to
Accelerate is rungs that do not exist yet, not emission quality.

**The remaining headroom is already named and already built for NEON.** The SME kernel:

1. accumulates into **1 of 4** available f32 ZA tiles;
2. reuses the NEON B panel only when widths coincide — no SME-specific packing;
3. has **no KC blocking**, so a k deep enough to miss L2 falls off (visible in the ceiling probe
   as 427 GFLOP/s at 1024 collapsing to 229 at 2048).

Those are `emit_tile_packed_j_outer` and `emit_tile_packed_kc`, which the NEON path already has.
That they are not shared is the subject of the recorded `block_plan` reduction (see
`docs/next-session.md`) — five rungs currently hand-roll their own i/j/k walk.

## Correctness

- Values byte-identical between the SME and NEON legs at every size measured.
- An adversarial review ran a value differential on hardware — SME vs NEON vs the **interpreter
  oracle** — over square and non-square shapes, `k` not a multiple of the tile side, non-zero
  `base`, transposed A, B row stride ≠ c, packed and `--no-pack`, the arena path, and under
  AddressSanitizer: **0 differing cells**.
- Workspace gate **1023 passed / 0 failed**; **159/159 emissions byte-identical** for
  generic/apple-m/zen3/cuda-ada, so the rung is invisible to every pre-existing profile.
- **Still owed:** the executing value check is not in `cargo test`. See `benches/sme/README.md`.

## The parallel lift, and why it is provably safe

The rung was sequential-only on first landing, which made every matmul benchmark ineligible —
they all run on the task path. The lift:

- `func/sme.rs` asks `bulk_bounds` for its row range, the same question the NEON rungs ask.
- `slice_sizing` hands the runtime `t · c` as the slice quantum when this rung fires, instead of
  `tile_i · c`.

That second change is what makes the first sound. `mapal-rt`'s `slice_ranges` cuts slices **on the
quantum**, and `sme_tile_site` requires `rows % t == 0`, so `n = rows · c` is an exact multiple of
`t · c`. Therefore every slice boundary is `t`-row aligned, there is no ragged final slice, and no
16×16 panel can straddle two tasks — so the `lo/c` and `hi/c` divisions in the emitted code are
exact by construction rather than by luck.

---

# S41b — all four ZA tiles (2×2), profile-derived

The rung above accumulated into **1 of the 4** f32 ZA tiles the profile records. `TargetProfile`
now derives the arrangement — `sme_block()` returns the most-square factorization of the tile
count, so 4 ⇒ (2,2) falls out and 1×4 is rejected by arithmetic (it needs 5 operand loads per 4
MACs where 2×2 needs 4), not by taste. f64 scales off the same recorded `f32_tiles` by element
width rather than a second constant.

Values identical to both the NEON leg and the 1-tile SME leg at every size, `--no-pack` included.

## Single thread, f32

| N | NEON | SME 1 tile | **SME 2×2** | vs NEON | numpy-1t | numpy ahead: session start → 1 tile → **2×2** |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1024 | 19.4333 ms | 5.4102 ms | **2.1006 ms** | **9.25×** | 1.2977 | 13.5× → 4.17× → **1.62×** |
| 4096 | 1286.13 ms | 332.536 ms | **192.005 ms** | 6.70× | 84.617 | 14.8× → 3.93× → **2.27×** |

Distributions disjoint at both sizes.

## Threaded, f32 — one campaign, n=21, medians, **every size disjoint**

| N | NEON | SME 1 tile | **SME 2×2** | vs NEON | vs 1 tile | numpy-thr | numpy ahead: before SME → 1 tile → **2×2** |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 512 | 0.5577 | 0.2530 | **0.2229** | 2.50× | 1.14× | 0.1075 | 3.8× → 2.35× → **2.07×** |
| 1024 | 2.2581 | 0.9715 | **0.9428** | 2.40× | 1.03× | 0.6757 | 3.3× → 1.44× → **1.40×** |
| 2048 | 18.8745 | 7.6156 | **6.9658** | 2.71× | 1.09× | 5.3045 | 3.5× → 1.44× → **1.31×** |
| 4096 | 151.5684 | 79.1425 | **56.7021** | 2.67× | **1.40×** | 44.143 | 3.4× → 1.79× → **1.28×** |

NEON and SME-2×2 measured together; the 1-tile column is this session's earlier campaign with the
same harness. NumPy's threaded lead **shrinks monotonically with size** — 2.07× → 1.28×.

### The 4096 knee is gone, and it was not what I said it was

| N | NEON | SME 1 tile | **SME 2×2** | numpy |
| ---: | ---: | ---: | ---: | ---: |
| 512 | 481 | 1061 | 1204 | 2497 |
| 1024 | 951 | 2210 | 2278 | 3178 |
| 2048 | 910 | 2256 | 2466 | 3239 |
| 4096 | 907 | **1737** | **2424** | 3113 |

GFLOP/s. The 1-tile rung climbed 1061 → 2210 → 2256 and then **fell to 1737 at 4096**, and S41
recorded that as the missing KC cache-blocking rung. **That reading was wrong.** With all four
tiles the curve is 1204 → 2278 → 2466 → 2424 — essentially flat, the collapse gone, with no cache
blocking added.

The cause was **arithmetic intensity, not cache capacity**. A 32×32 panel loads the same B row and
issues 4× the MACs against it, so bytes-per-flop fell 4× and the kernel stopped being
memory-bound. Fixing the accumulator fixed the memory problem as a side effect.

**Consequence: KC blocking is a smaller remaining item than S41 claimed, not a larger one.** The
honest remaining gaps are the 512 row — 2.07× behind, and that is launch/dispatch overhead on a
0.22 ms kernel rather than the kernel itself — and the ~22% throughput deficit at 2048/4096, which
is a steady micro-kernel gap in the S33 sense (both curves flat).

**Threaded 1024 and 512 gained little over the 1-tile rung** (1.03× and 1.14×). At those sizes the
SME leaf is not the bottleneck. The large threaded win is **4096, 1.40×**, and the large win
overall is single-thread.

## Two defects found by review, both fixed

1. **`MAPAL_SLICE` could make the kernel write past the end of the output — reproduced as a
   SIGSEGV.** The i-loop starts at `lo/c` and steps whole panels with no partial-panel path, so a
   slice starting mid-panel makes the final panel overrun. Correctness rested entirely on
   `slice_ranges` cutting on `slice_sizing`'s quantum, and the `MAPAL_SLICE` lever bypasses that
   (`slice_elems = forced`). Doubling the panel doubled the quantum, so sizes that were aligned
   before no longer were. **Fixed at root cause**: the lever now rounds up to the task's declared
   quantum, so it still probes the size — what it exists for — but can no longer produce a start
   the kernel was never told to expect. This also fixes a **pre-existing** crash at `7732a5f`
   (`MAPAL_SLICE=4096` segfaulted there too).
2. **The predicate's doc still claimed a "Sequential" clause that the parallel lift removed**,
   which is what hid (1). Replaced with the actual obligation.

## Recorded limits

- **The panel quantised.** `rows % (ti·t) == 0` now means 32, not 16, so a 16- or 48-wide matmul
  that took the rung before falls back to NEON. Pinned deliberately (`ATTN_16`). Predication would
  recover it; not built.
- **f64 is derived but unexercised** — `sme_block` returns (2,4) at f64 and a test pins it, but
  `sme_tile_site` is f32-only, so no f64 kernel is emitted. Derivation tested, emission not.
- **The derivation is only valid up to 4 f32 tiles**, which is what the architecture has: a review
  probe found `f32_tiles > 4` generates a module LLVM cannot select, because `mopa.nxv4f32` has no
  tile 4. Not reachable today (the profile records 4), but the 1..=64 sweep in the test pins
  arrangements that could not be emitted.
- **The A scratch doubled** to 128 KB at k=1024; still under `heap_min_bytes`, crossing into the
  arena at k≥2048.
