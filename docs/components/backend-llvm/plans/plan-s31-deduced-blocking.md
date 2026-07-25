# Plan — S31: blocking is deduced, not set — and the conv accumulator that must land first

**Status:** written pre-build, S31. Origin: Sapir's S31 directive, restated on the row-blocking
item — *"conv2d row blocking should be generic per algorithm and not tailored for conv2d, TI
should be detected for best case on execution graph."* Supersedes the framing of suggestion #11
("TI over output rows in `emit_tiled_map_conv`"), which is conv-shaped by construction.
Related: ADR-0032 D4 (backend config = performance tailors), ADR-0034 (candidate — constants
are searched), `plan-s31-target-profiles.md` (the machine half), `docs/notes/tile-ladder-direction.md`
§"The recorded facts ARE the optimization menu".

## Why (one paragraph)

`TILE_I = 4` is a literal swept once on an M4 Pro and applied on every target — the same defect
ADR-0034 names for the whole constant family. But TI is not one question; it is two, and they
split exactly where everything else in this project splits. *Does blocking pay here, and by how
much* is **geometry**: it falls out of the recorded address coefficients by arithmetic, for any
site, with no new graph analysis and no conv special case. *How large may TI be before the
accumulators spill* is a **machine fact**: the vector register file, which lives nowhere in this
repository today. Deducing the first and looking up the second replaces the literal with a
derivation that reproduces `4` on the machine it was swept on — and produces the right number on
a machine nobody has swept.

## The finding that reorders the work

The queue says conv2d's per-core gap is the `TI=1` ceiling (`STATUS.md`, suggestions #11/#17,
`s29.md` §"What conv2d's row tells us"). Counting memory operations in the emitted shape says
`TI=1` is the smaller half. Per output element at `K=9`, `TJ=16`, from `emit_tile_conv_tile`
(`func.rs:3630-3805`):

| term | today | TI=4, memory acc | TI=1, vector acc | both |
| --- | ---: | ---: | ---: | ---: |
| `b` loads (`3711-3722`) | 9 | 4.5 | 9 | 4.5 |
| acc loads (`3738-3745`) | 9 | 9 | 0 | 0 |
| acc stores (`3746-3753`) | 9 | 9 | 0 | 0 |
| seed store (`3654-3663`) | 1 | 1 | 0 | 0 |
| out store (`3779-3799`) | 1 | 1 | 1 | 1 |
| **mem ops / 9 FMAs** | **29** | **24.5** | **10** | **5.5** |
| FMA:mem | 0.31 | 0.37 | 0.90 | 1.64 |

Arithmetic on the emission shape, not a measurement — the S30 disasm figure (24 loads / 36 FMAs,
FMA:mem 1.29) already includes clang's CSE, so the absolute numbers differ. The *ratio* is the
point: **row blocking alone buys ~1.2×; the vector accumulator alone buys ~2.9×.** The conv rung
keeps a `[TJ x elem]` alloca and does `load`/`fadd`/`store` per (tap, lane) because S30's phi
conversion was gated `main && rows == TILE_I && bound == TILE_J` (`func.rs:4476`, `5109`) and conv
has no `rows` block at all. S29 recorded conv as "register-resident already" — true, and true
only because a 4-register accumulator at TI=1 is small enough for clang to promote. That is the
same grant S29 watched LLVM withdraw (92 `str q…,[sp]`), and TI=4 makes the accumulator 16
registers, i.e. exactly big enough to lose it.

Second correction: the reuse factor at TI=4 is **2.0×, not 3×**. Distinct tap lane-runs for a
TI-block are `(TI − 1 + k/div)·div` = 18 for four rows against 36 unblocked. The `×3` in
suggestions #11 is the `TI → ∞` limit (`div`), reached at `TI ≥ 12`, which no register file pays
for.

**Therefore the order is: accumulator first, blocking second.** Both are in this plan; shipping
them in the other order produces a small win, a re-pinned snapshot, and no test that reports the
disappointment.

## Categorical model

No `Dat`/`Trn` change. Per-cell morphism chains keep their operands, their k order and their
rounding at every TI — blocking permutes *which cell is computed when*, never the chain inside a
cell, so R1 holds by construction and the differential suite is the enforcement. The change is a
`DataLoc` one (accumulators to the register file) plus a **deduced morphism** replacing a stored
constant.

| Item | Kind | Model |
| --- | --- | --- |
| `i_reuse : TileRead → Reuse` | `Trn`, **deduced**, emitter-local | `Invariant` iff `ci == 0`; `Sliding{q}` iff `ksplit` present and `cq ≠ 0` and `ci = q·cq` with `q < k/div`; `None` otherwise. Pure arithmetic over recorded fields — no flow-ir change |
| `distinct_runs : (Reuse, TI) → ℕ` | `Trn`, deduced | `Invariant → 1` · `Sliding{q} → (TI−1)·q + k/div` · `None → TI`. The reuse ceiling and the honest factor at any TI, for any site |
| `tile_i : (Ty, TargetProfile) → ℕ` | `Trn`, **deduced** (was: the literal `TILE_I`) | `max { TI = 2^m : TI·regs_per_acc + headroom ≤ vec_regs }`, `regs_per_acc = TJ·sizeof(elem)/vec_bytes`, `headroom = regs_per_acc + 2` (one b tile, one splat, one product) |
| `acc_r : <TJ x elem>`, `r < TI` | `DataLoc` (register) | the S30 form, extended to the conv rung's constant-TJ main tile |
| `vec_regs`, `vec_bytes` | machine facts | **not** in the record and never in flow-ir (ADR-0032). `TargetProfile` fields; seeded as emitter constants until that lands |

**Composition rules.**

1. `tile_i` reads **only** `(elem, profile)` — never `rows`, `c`, `k`, or a lane count. Any
   geometry in the magnitude moves five pinned snapshots whose fixtures are too small to execute
   the blocked path they pin (matmul golden `rows=4, c=4, k=4`; FIR `N=16` against a `TI·TJ = 64`
   block). Geometry decides *whether* to block, magnitude does not depend on it.
2. On `apple-m` (32 regs × 16 B) and on `zen3` (16 regs × 32 B) the rule yields `TI = 4` for both
   widths. **Rule 1 (safety): every emission that exists today is byte-identical after work items
   1 and the constants swap.** The matmul and FIR goldens, all 72 checked-in `benches/matmul/*.ll`,
   and the KC snapshot must not move by one byte.
3. Blocking is applied where `i_reuse ≠ None` on either read. Today's gate `rows > 1 && b.ci == 0`
   is exactly `i_reuse(b) == Invariant`, so the matmul path is unchanged by definition; the conv
   path becomes reachable because `b.ci == cq` (verified 18 == 18, `flow-ir/tests/algos.rs:2100-2112`)
   is `Sliding{1}`.
4. Row-blocked taps must be **hoisted once per block**, not emitted as TI copies of the tap nest.
   TI independent nests put the matching loads in different basic blocks separated by aliasing
   alloca stores — the precise situation S29 recorded GVN failing on. This is a restructure of
   `emit_tile_conv_tile`, not a loop around it.
5. `TILE_I`'s use in the FIR rung is **a different quantity** and does not join this deduction.
   `func.rs:3127/3165/3201/3290/3377` use it as a lane-block multiplier over a `[TI·TJ x elem]`
   *memory* accumulator; there are no vector accumulators on that path (`emit_tile_trio_vec` is
   unreachable from `emit_tile_window_block`), so a register-file budget does not bind it. It gets
   its own name at its current value and its own future justification.

## Work items

1. **Name the three quantities apart, zero behaviour change.** `TILE_I` → `tile_i_for(&Ty)` for
   the matmul rung (row block, register-budget-derived), `WINDOW_SUBROWS = 4` for the FIR rung
   (lane block, unjustified constant, marked so), and the KC a-panel keeps `tile_i · TILE_KC`.
   Seed `VEC_REGS = 32` / `VEC_BYTES = 16` next to `tile_j_for`, documented as the two
   `TargetProfile` fields this rung needs. **Gate: every golden and every `benches/matmul/*.ll`
   byte-identical** (`./benches/matmul/regen.sh && git diff --stat benches/matmul/` clean).
2. **The conv vector accumulator.** Route the conv rung's constant-TJ main tile through the S30
   form: seed splat → taps over `phi <TJ x elem>` → store-out; remainder and boundary tiles keep
   today's memory emission (the negative control, same carve-out as S30). Re-pins
   `tile_nest_shape_conv.snap` deliberately, with a structural assert that the main path contains
   no accumulator `alloca`/`load`/`store` and does contain `phi <TJ x elem>`.
3. **`i_reuse` + `distinct_runs`**, ~30 lines of arithmetic beside `conv_site`, with unit tests
   over the three recorded oracles (matmul `Invariant`, conv `Sliding{1}`, FIR `None` on `b` /
   `Invariant` on `a`) and over the non-conv k-split site that must stay refused.
4. **Row-block the conv rung** on `Sliding` reuse, reusing the existing head/interior/tail
   i-region discipline rather than forking it; taps hoisted per rule 4. `emit_tile_vec_k_loop` is
   already `rows`-generic (`seeds.len()`, `func.rs:5654`), so this is gate and stride work.
5. **Measure**: conv2d 1024 1t against cpp 0.353 (the standing basis, compute-only), and the hot
   loop's FMA:mem from the disasm against 1.29. Both numbers are the done-check.

## Tests

- **The differential suite is the R1 gate** and must stay green unchanged — it catches a TI that
  is *wrong*, never a TI that merely differs (value-invariance is the point).
- **New: conv differential cases at `rows % TI ≠ 0`.** Today's remainder coverage (`r5_c20_k7`,
  `r6_c32_k5`, `r6_c20_k5_f64`) is matmul-shaped and happens to exercise TI=4 remainders by luck.
  Conv blocked rows need their own.
- **New: a correctness check in the shapes bench.** `shapes_ab.sh` prints times; `matmul_ab.sh`
  has no correctness assertion at all. Any A/B that reports a conv speedup must also report
  output equality, or the measurement cannot distinguish a win from a broken kernel.
- Disasm gate, cheap and the actual deliverable: accumulator `str q…,[sp]` count in the conv task
  is **0**, and FMA:mem is read from the same disassembly.

## ADR-0033 D2 — the three-line record

- **Record fields consumed:** `TileRead.{ci, ck, clane, ksplit{div, cq, cr}}`, `TileSite.{rows, c,
  k, elem}`. No new field; no flow-ir change (ADR-0032 category (b), emitter-local cashing).
- **CUDA realization against the record:** `i_reuse`/`distinct_runs` are the same facts a CUDA
  emitter needs to decide how many output rows one thread owns — register tiling is the GPU
  sibling of this rung, and it reads the identical coefficients. `tile_i`'s *formula* is shared;
  its inputs (`vec_regs`, `vec_bytes`) become registers-per-thread and occupancy. Unexecuted:
  `tile_plan` still has exactly one consumer.
- **Machine facts the record does not carry:** vector register count and vector width. Absent from
  the codebase entirely today; `plan-s31-target-profiles.md` proposes `vec_regs`/`vec_bytes` and
  is the intended home. Until it lands they are two named constants in `func.rs`, which is the
  same honesty as `TILE_J` but with the derivation written down.

## What this does not fix (the honest ceiling)

- **The per-tap lane loops.** Nine runtime lane loops per (row, j-tile), each round-tripping
  `lane_ctr` through memory (`func.rs:3700-3758`). Work item 2 removes the accumulator traffic
  inside them; TI *multiplies* their count. Collapsing them is a further rung.
- **The rest of the matmul ladder stays unreachable for conv.** `packing_site` refuses k-split
  (`func.rs:344-348`), so no packed panel, no KC nest, no prefetch, and there is no k loop to
  unroll (taps are Rust-level unrolled). Suggestion #12 (im2col as a `DataLoc`) is the move that
  opens those doors; this plan does not.
- **The reuse ceiling is `div`.** For a 3×3 conv that is 3×, approached only as `TI → ∞`. At the
  register-feasible TI=4 the factor is 2.0×.
- **Nothing here touches the thread-count question**, which is the other S31 P0 and needs a
  different missing fact (work per element — a genuine graph fact, absent from flow-ir, legal to
  add there).

## Open questions for Sapir

1. **Order.** This plan ships the conv accumulator (item 2) before the blocking (item 4), because
   the mem-op accounting says it is 2.9× against 1.2×. The queue lists row blocking as the P0.
   Confirm the inversion.
2. **`vec_regs` seeded as a constant vs waiting for `TargetProfile`.** Seeding keeps this rung
   independent and makes the profile's job a lookup swap; waiting keeps the constant count at
   today's number. Plan assumes seeding.
3. **The FIR rung's `4`** is now explicitly an unjustified constant with its own name. Leaving it
   at 4 is the byte-identical choice; justifying or searching it belongs to ADR-0034.

---

## As built — item 2, and the prediction it refutes

**Built (S31, worktree `s31-deduced-blocking`):** `emit_tile_conv_tile_vec` — the conv rung's
constant-TJ main tile on `<TJ x elem>` SSA values. Conv has **no runtime k loop** (the taps
are unrolled at emission), so the accumulator needs no `phi` at all: one splat of the seed,
one `fmul`/`fadd` pair per tap, one vector store. The runtime-`tj` remainder keeps the memory
form, the same carve-out S30 used. `tile_vec_llt`/`emit_vec_splat` were split into
`vec_llt`/`emit_splat` over the two fields they use, so the conv context shares them instead
of duplicating.

Emission, before → after (in the conv function):

| | before | after |
| --- | ---: | ---: |
| `getelementptr [16 x float]` (accumulator memory) | 22 | **11** (remainder only) |
| `fmul`/`fadd float` | 19 | 10 |
| `fmul`/`fadd <16 x float>` | 0 | **9** |
| `load` / `store <16 x float>` | 0 | 9 / 1 |
| lane loops (`icmp uge i64 %t…, 16`) | 29 | 18 (main tile: **0**) |

### The measurement, and the correction it forces

conv2d, min-of-15 and median-of-15, compute-only, product face, M4 Pro:

| shape | old min / med | new min / med | Δ |
| --- | --- | --- | ---: |
| conv2d_512 1t | 0.1223 / 0.1377 | 0.1086 / 0.1227 | **−11%** |
| conv2d_1024 1t | 0.5343 / 0.5692 | 0.4810 / 0.5222 | **−10% / −8%** |
| conv2d_1024 par | 0.4286 / 0.5075 | 0.4195 / 0.4921 | −2% |
| conv2d_512 par | 0.1258 / 0.1704 | 0.1033 / 0.1750 | noise (sub-0.2 ms) |

Output byte-equal on every leg (`576 / -96`) — R1 holds.

**This refutes the mem-op accounting above.** That table predicted the vector accumulator
was worth ~2.9× (29 → 10 memory operations per 9 FMAs). Measured: **~10% at one thread**,
and the assembly says why — whole-function counts move from 81 fmla / 106 `ldr q` / 31
`str q` / 804 instructions to 81 / 101 / 29 / 776. Almost nothing was removed, because
**almost nothing was there**: LLVM was already promoting the conv accumulator alloca and
vectorizing the lane loops. S29 recorded exactly this — *"fir's window rung and conv2d's conv
rung … are register-resident already"* — and the table still counted those operations as if
they reached the machine. The caveat was even written into the table ("the S30 disasm counts
already include clang's CSE") and the ratio was allowed to drive the ordering decision anyway.

**What survives, and what does not:**

- **Survives — the durable half of the S30 argument.** The register form is now *emitted*
  rather than *granted* by a promotion heuristic. That grant is precisely what S29 watched
  LLVM withdraw (92 `str q…,[sp]`) when unrelated code touched the same slot, and TI>1 would
  have quadrupled this accumulator into exactly that risk. So item 2 remains the right
  prerequisite for item 4 — but as insurance, not as the payoff.
- **Does not survive — the 2.9× and the ordering argument built on it.** Counting operations
  in emitted IR predicts nothing about a machine when the optimizer deletes them first. Any
  future ordering claim in this plan must come from a measurement or a disassembly, never
  from an IR op-count table.
- **Item 4's predicted 1.2× is under the same doubt** and must be measured, not asserted. So
  is suggestion #11's premise that `TI=1` is *the* cause of conv2d's 3.4× loss at 1024 — the
  reuse arithmetic (2.0× at TI=4) is sound, but whether it converts to time is now an open
  question rather than a projection.

### Where conv2d actually stands after item 2

1t at 1024: **0.481 ms** against C++'s 0.353 (S30 basis) — the gap narrows from ~2.0× to
~1.8×. The remaining distance is not the accumulator. Candidates, in the order the evidence
supports: the nine per-tap lane loops in the remainder path, the TI=1 image-row reuse
(item 4), and the fact that no matmul rung above register blocking reaches conv at all
(`packing_site` refuses k-split — suggestion #12's im2col is the move that opens them).

---

## As built — items 3 and 4: blocking is deduced, and it pays

**Item 3 (`crates/backends/llvm/src/reuse.rs`, new).** `Reuse` + `i_reuse` + `distinct_runs`
— ~60 lines of arithmetic over recorded `TileRead` fields, zero flow-ir change (ADR-0032
category (b)). The load-bearing claim of the plan is now code: **`ci == 0` and `ci == cq` are
the same predicate at `q = 0` and `q = 1`**, so matmul's row-invariance and conv's sliding
window are one rung, not two. Five unit tests over the recorded oracles pin it, including the
correction that conv's reuse at TI=4 is **2.0×, not suggestion #11's 3×** (that is the
`TI → ∞` limit and needs TI ≥ 12).

**Item 4 (`emit_conv_blocked_range` + `emit_conv_block_tile`).** The conv nest becomes three
row regions threaded on one counter — head (TI=1) → interior full-window rows TI at a time →
tail (TI=1) — entered **because the record says the read slides**, not because the site is
conv2d. Composition rule 4 is honoured: the block hoists its tap-row union once
(`(TI−1)·q + k/div` = 6 rows for 4 output rows) and each loaded vector is consumed by every
row that uses it, so the loop nests row *inside* tap. Emitting TI copies of the tap nest would
have reproduced the GVN failure S29 recorded. R1 holds because `kq = kqp − q·r` rises with
`kqp`, keeping each cell's chain k-ascending.

`reuse::distinct_runs` is not decoration: the emitter `debug_assert`s that its own union size
equals what the query predicts, so the two cannot drift.

### Measured

conv2d, min/median of 15, compute-only, product face, M4 Pro. `item2` is the vector
accumulator alone; `item4` adds row blocking.

| shape | item2 | item4 | Δ |
| --- | --- | --- | ---: |
| conv2d_512 1t | 0.1113 / 0.1303 | **0.0910 / 0.1018** | **−18% / −22%** |
| conv2d_1024 1t | 0.4740 / 0.5070 | **0.3992 / 0.4125** | **−16% / −19%** |
| conv2d_1024 par | 0.4082 / 0.4785 | 0.4075 / 0.4720 | flat |

Output byte-equal on every leg. **Row blocking is worth more than the accumulator was** —
~17% against ~10% — which inverts the ordering this plan argued for. The ordering was still
operationally right (the accumulator had to land first so TI>1 would not quadruple a memory
accumulator into the promotion risk S29 lost), but it was right for a reason the plan did not
give, and the 2.9×-vs-1.2× arithmetic that justified it was wrong in both terms.

**The reuse reached the machine.** Hot-function disassembly, `fmla` against vector loads:

| | fmla | `ldr q` | FMA:load |
| --- | ---: | ---: | ---: |
| item2 | 81 | 101 | 0.80 |
| item4 | 306 | 255 | **1.20** |

A 50% rise in arithmetic intensity, which is the deduced 2.0× reuse showing up as fetched
vectors — diluted by the head/tail TI=1 regions and the remainder paths, as expected.

### Where conv2d stands now

| | 1t min | vs cpp-1t 0.2553 |
| --- | ---: | ---: |
| session start | 0.5343 | 2.09× behind |
| + item 2 (vector accumulator) | 0.4810 | 1.88× |
| + item 4 (deduced row blocking) | **0.3992** | **1.56×** |

**−25% total at one thread**, and the per-core gap that S30 called the highest-value item in
the queue is down by a quarter without a single machine constant being hand-set.

Parallel is unchanged, and the width sweep says why — optimum is still 4 threads (0.235 ms)
against 0.411 at the default 14. **The remaining conv2d deficit is now majority scheduling,
not kernel**, which is the thread-count P0.

### Still open

- The 1-D window rung (`WINDOW_SUBROWS`) does not consume `i_reuse`; it blocks lanes, not
  rows, and its `4` is still unjustified.
- `distinct_runs` is consumed only by the conv path and the assert. The matmul rung still
  keys on the raw `site.b.ci == 0` rather than on `i_reuse(b) == Invariant` — same predicate,
  but the shared name is not yet the shared call.
- Nothing here touches `packing_site`'s k-split refusal, so the packed/KC rungs remain
  unreachable for conv (suggestion #12).
