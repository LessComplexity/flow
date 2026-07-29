# Next Session (S42)

Written: 2026-07-29 · end of S41 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-29-s41-two-gates-and-a-second-unit.md`
Previous: S40b (`sessions/2026-07-29-s40b-the-compiler-is-also-a-program.md`), S40, S39.
The plan that governs S42: **`components/backend-nvptx/plans/plan-s41-the-nvptx-leg.md` — RATIFIED.**

## READ THIS FIRST

**THE GATE IS GREEN — 1023 passed, 0 failed — fmt clean — 159/159 emissions byte-identical.**
Nothing shipped this session moved an emitted byte for any pre-existing profile. The count went
1006 → 1009 (step 1) → 1015 (the two gates) → 1023 (the SME rung).

**Work is UNCOMMITTED on `main` @ `aaaa5dd`.** Sapir's call on committing. S39+S40+S40b were
committed by Sapir before this session opened — several docs still said "uncommitted @ `8b40442`"
and are now corrected.

**The two §2.2 gates are the durable output of S41; treat them as load-bearing.**
`crates/mapal-ir/tests/consumer_coverage.rs` (Gate A) and the additions to
`crates/backends/llvm/tests/tile_sites_pin.rs` (Gate B). They exist because Sapir corrected the
plan: *"forking didn't stop the second consumer from being a consumer, it is just because we
stopped working on it… the tests should guard it by checking all consumers all the time."* If a
future change makes one of these inconvenient, that is the gate working.

## FIRST commands

```sh
git status --short                       # S41, uncommitted
cargo test --workspace --release         # GREEN, 1023
benches/emit_sweep_ab.sh <emit-binary> /tmp/now.hashes   # the 159-emission A/B harness
git worktree list                        # 3 stale entries, two DIRTY — see §Live state
```

## S42 opens on — Sapir's stated order

### 1. SME — SHIPPED, MEASURED, and now using all four ZA tiles.

`docs/performance/s41-sme.md` has the full record (S41 = 1 tile, S41b = 2×2). Headline, M4 Pro
f32, **single thread**, values identical to the NEON leg, distributions disjoint:

| N | NEON | SME 2×2 | vs NEON | numpy-1t | numpy ahead: start → 1 tile → **2×2** |
| ---: | ---: | ---: | ---: | ---: | --- |
| 1024 | 19.4333 ms | **2.1006 ms** | **9.25×** | 1.2977 | 13.5× → 4.17× → **1.62×** |
| 4096 | 1286.13 ms | **192.005 ms** | 6.70× | 84.617 | 14.8× → 3.93× → **2.27×** |

Threaded: 1024 0.9383 ms (numpy 1.39× ahead), 4096 57.4170 ms (numpy 1.30× ahead). **Threaded
1024/512 gained nothing defensible over the 1-tile rung — those distributions overlap.** At width
the SME leaf is not the bottleneck; the 1t result is the finding. Do not quote a threaded 1024
multi-tile number.

Settled, do not re-derive: build with `-march=armv8-a+sme2` (`armv9-a+sme2` compiles then
SIGILLs — SME without SVE); `fmopa` **fuses**, so SME is a contract-face realization under
ADR-0032 D1/D3; the kernel uses `aarch64_pstate_sm_body`, not `_sm_enabled`.

**Remaining SME headroom**, both named by measurement and both blocked on the same thing:
no B packing of its own (it borrows NEON's only when widths coincide) and no KC blocking, which
is why threaded 4096 still trails. Those live in other rungs' nests — see §3.

**Still owed**: (a) no executing SME value check in `cargo test` — `tests/sme_rung.rs` is
`str::contains` only. A review claimed the differential harness cannot cover SME without
`-march`; **that was tested and is FALSE** — plain `clang -O2` compiles, links and runs the SME
module, because the target features ride on the function attribute. Easy, not blocked.
(b) No hosted CI runner has an SME core (Linux runners are x86; `macos-latest` is M1/M2, SME is
M4+). Layer it: IR assertions anywhere → **compiles+links on any Apple Silicon** → runs+value-checked
on M4+ with skip-with-reason, the pattern `differential.rs` already uses for absent `nvcc`.
(c) The panel quantised to 32 rows, so 16/48-wide matmuls fell back to NEON; predication would
recover them. (d) f64 derives but is not emitted. (e) `f32_tiles > 4` would generate unselectable
IR — not reachable (the architecture has 4), but the 1..=64 test sweep pins arrangements that
could not be emitted.

**Open from plan §2.4:** whether M4's SME and M4's Apple-AMX are the same silicon. It decides what
the numpy comparison *means* and it is one cheap measurement.

### 2. P0 — the NVPTX leg, steps 2–5 (no hardware needed)

`llc -march=nvptx64` is on the Mac, so "is this valid NVPTX" is laptop-checkable before any GPU
run. Step 2 is the device module preamble (triple, datalayout, `ptx_kernel` cc, `addrspace`).
Step 5's launch glue is confined to `func/drive.rs` after the split.

Steps 6–8 need the box, and the box needs **only a repo checkout** — CUDA 13.3.1 is already
installed at `/opt/cuda` (`nvcc`, `ptxas`, `cuobjdump`, `nvdisasm`, `compute-sanitizer`), driver
`libcuda.so.610.43.03`, RTX 4070 Ti sm_89. `sudo` there needs a password, so any install is
Sapir's to run; as of 2026-07-29 none is required. Use `compute-sanitizer --tool racecheck` on the
smem kernel and on a barrier-removed negative control — it detects a missing `__syncthreads`
directly instead of waiting for the differential to get unlucky.

### 3. The `block_plan` reduction — RUN, and **REFUTED** as posed. One 2-subset survives.

ADR-0033 D5's gate fired (SME is the second consumer), so the reduction was run on 2026-07-29
rather than assumed. FRAMEWORK §3's method: the five nests' morphisms written side by side **with
their targets**, then check which squares commute. Result: **they do not.**

Nests compared: `tile.rs::emit_tiled_map{,_blocked}` · `conv.rs::emit_tiled_map_conv` ·
`window.rs::emit_tiled_map_blocked_1d` · `packed.rs::emit_tile_packed_{j_outer,kc}` ·
`sme.rs::emit_tiled_map_sme`.

**Identical across all five: exactly three things, and every one sits BELOW the nest** — the
`bulk_bounds` call, `emit_tile_index` for address synthesis, and flat `out[i·C + j]` addressing.
The nest itself is shared nowhere.

**Why it cannot collapse** (the list that did the refuting — do not soften it on a re-read):

1. **Which axes exist is not a parameter.** CONV has no runtime `k` loop (`ConvTileCtx` has no
   `k_ctr` field; its reduction is unrolled at emission time). WIN has no `i` axis (`rows == 1` by
   predicate). SME has neither `k` nor `lane` in emitted code — both live inside the callee. A
   shared nest needs i/j/k/lane each independently optional, which is the bag of per-rung flags
   this reduction exists to avoid.
2. **CONV's reduction space is a rhombus, not a product** — the outer index runs over the union of
   tap-rows, each tap serving a filtered subset with a skewed `kq = kqp − q·r`. No rectangular
   block plan with per-axis bounds emits that.
3. **The accumulator has four incompatible FORMS, not four extents** — memory alloca ·
   `phi <TJ x elem>` · a straight SSA chain with no phi (CONV has no loop to carry one) · ZA state
   the emitter cannot name. Swapping the first two changes the emitted control-flow graph.
4. **The leaf's output RANK differs.** FMA yields a 1-D lane run; `fmopa` yields a 2-D `t×t` tile
   with its own read-out loop. The nest above a rank-2 leaf must hand it a 2-D tile address and
   step `j` by `tj·t`. **That is the nest changing, not the leaf** — and it is where S41's
   "SME is a leaf swap" framing breaks down. Correct the framing; do not re-derive it.
5. **SME's task-range conversion is correct only because of `slice_sizing`.** Four nests do
   biased-ceil rows plus a signed per-row lane clip; SME does floor/floor with no bias and no clip,
   valid *only* because the slice quantum is `ti·t·C`. Unifying the conversion either adds dead
   clip code to SME or silently breaks its alignment argument. This is a **correctness** coupling,
   not a shape preference.
6. Also: rung-1 `emit_tiled_map` is a pinned byte-identity control (`tests/tile_sites_pin.rs`,
   `tests/golden_ll.rs`) and has **no** main/remainder split at all, so it is not even
   parameter-equal to rung 2 at `TI=1`. If it should go, delete it as a rung and re-bless the
   goldens — a different and honest decision. Do not "unify" it.

**Steps 0 and 0b of that merge are DONE and landed** (each proven a no-op, 159/159 emissions
byte-identical): five copies of the accumulator-lane offset decision became one
`core.rs::emit_acc_lane`, and the row-window clip — written out three times, character-identical —
became `core.rs::emit_row_window`. Step 0 also converts the trio merge's `acc_lane` divergence
from structural to parameterizable, which is why it was done first.

**Steps 1–3 remain**, in this order and one commit each with `benches/emit_sweep_ab.sh` between:
`i_regions` (~100 lines, cleanest — its only structural deltas are one guarded `emit_tile_kc_apack`
insert and widening an existing 2-arm `Option` match to a 3-arm enum), then `j_split` (4 call
sites, one of them `window.rs`), then `trio` (largest and riskiest). **Do not batch them** — with a
single shared ordinal counter a batched failure gives no bisect signal, only "everything after
byte N differs".

Three hazards the plan named, all of which must be preserved verbatim rather than "cleaned up":
the `out_start` hoist sits ~200 instructions apart in the two paths and must branch on nest; the
remainder `tj` is a 3-instruction clamped `select` in the direct nest and a plain `sub` in KC, and
the select is **provably dead but emitted and in the goldens**; and `emit_tile_trio_vec` /
`emit_tile_kc_trio_vec` are completely reordered — do not merge those two.

**Correction to the pairing:** `emit_tile_kc_boundary_row` is NOT the twin of
`emit_tile_packed_boundary_row` — it sits at a different level of the nest and contains a whole j
loop the other structurally cannot have. Its real twin is `emit_tile_kc_boundary_tile`.

**The one reduction the evidence DOES support: TILE-blocked + KC.**
`emit_tile_kc_i_regions` is a line-by-line clone of `emit_tile_i_regions`; likewise
`_j_split` / `_trio` / `_boundary_row`. Same phases, same order, same labels. The deltas are four
parameters — `(k_lo, k_hi)`, `first: bool`, a-operand source, `acc_base` — and **three are already
passed as arguments today**. ≈350 lines of duplication where a shared nest with four arguments is
strictly smaller.

**A trap the reduction would have re-introduced, recorded so nobody re-proposes it:** a shared
`tile_i: u64` field would re-merge two quantities S31 deliberately named apart — `tile_i` is *rows
of vector accumulators bounded by the register file*, `WINDOW_SUBROWS` is *lanes over a memory
accumulator with no register bound*. They are both 4 today by coincidence, and a shared field
would make a future retune of one silently retune the other.

**Consequence for SME, which is what prompted this.** "Can SME take advantage of B packing and KC
blocking?" — **yes to both, and neither needs a shared nest.** The two questions are different and
S41 conflated them:

- **B packing**: SME declines it today only because the pack width is hardcoded to NEON's
  `tile_j`. Make the width a parameter of the realization — one change in `sme.rs` — and it packs
  always instead of on a width coincidence.
- **KC blocking**: SME gets its own k-panel loop. Note the real cost the analysis surfaced: SME
  *stores* ZA rather than accumulating into `c` (hence the `seed == 0` precondition), so parking
  partials at each panel boundary means `read.horiz` out and re-accumulate back in — more
  expensive than spilling vector registers. A measurable crossover, not a blocker; the KC rung is
  already default-OFF on `apple-m` for a comparable reason.

### 4. The seam re-judgement (plan §8.5)

Packaging is deliberately undecided. Budget: **~6 `Machine` branches** outside the four known
sites, and **any** branch inside `emit_morphism` is the trip signal. Exceeding it means option A
(own crate) was right — a measured answer, recorded, not a failure.

### 5. P1 — unchanged queue

- Per-task enable predicates in `mapal-rt` (`ponytail:` marker in `path_plan`).
- **ADR for "guards gate the flow"**, amending ADR-0026 Q8; also owes the S40 unit rule.
- Beat OpenBLAS at ONE thread (flat 1.20× behind, size-invariant, untuned `generic`).
- Inlining must stamp spliced morphisms with the call-site position (plan-s38 §6.1).
- Oracle clones captured arrays per fold step (plan first; forbid agents writing into the repo).
- The S40 coverage debts: testgen builds only topology (a); no test pins an untaken arm's EFFECT.

### 6. The external review (`oainotes.md`, untracked at the repo root)

Not filed into the docs tree — Sapir's call whether it becomes a `general/` review record. Triaged
2026-07-29:

- **Already fixed, unread by the reviewer**: its sharpest complaint (guards evaluate every arm;
  the calculator divides by zero) — S39/S40 shipped exactly that fix. Its `open_inline` trap-order
  claim is likewise stale, fixed at S38 (`35c06c1`).
- **Live and verified true**: `docs/spec/mapal-as-implemented.md:198` says CUDA "has not started"
  (163 tests say otherwise) and `:109` says bodies cannot capture (L1108 now fires only on a
  *write* to an enclosing name, `typing.rs:694`); `crates/mapal-cli/src/main.rs` is a 4-line stub
  printing "not yet implemented" while the getting-started guide teaches commands around it.
- Its roadmap item 2 ("make CUDA the second consumer of the shared graph facts") **is** ADR-0033
  and is what S41 started.

## Live state at S41 close

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `aaaa5dd` | **dirty — all S41 work uncommitted** | `git status --short` | Sapir's call |
| worktree | `…7b9157fd/scratchpad/pre-s38` @ `d3ca82c` | exists, **clean** | `git worktree list` | `git worktree remove` — safe |
| worktree | `…f845faa7/scratchpad/wt` @ `6168863` | exists, **13 DIRTY files** | `git -C <path> status` | **NOT removed** — another session's uncommitted work; Sapir's call |
| worktree | `…fe60a79a/scratchpad/pre` @ `1daddaa` | exists, **1 dirty file** | `git -C <path> status` | **NOT removed** — same reason |
| machine | Arch box `100.81.226.103` | up; RTX 4070 Ti sm_89; CUDA 13.3.1 at `/opt/cuda`; **no repo checkout** | `ssh … nvidia-smi` | nothing to stop — owned box, not rented |
| artifact | `benches/sme/` | **committed to the tree** (was scratchpad-only) | `cat benches/sme/README.md` | keep |
| artifact | `benches/emit_sweep_ab.sh` | the 159-emission A/B harness, now in-tree | `benches/emit_sweep_ab.sh <bin> <out>` | keep |
| data | session scratchpad `…/760762e1-…/scratchpad` | baseline hashes, 4 gate logs, SME binaries | — | **session-local, will vanish**; everything load-bearing was copied into the tree |

## Measurement rules (S37's six + S38's four + S39's three + S40's one + S41's one)

See prior logs. S41 adds:

13. **A refactor's gate is the emission sweep, not the test suite.** A change that silently
    reordered emission passes every unit test and moves goldens only where a golden happens to
    exist. `benches/emit_sweep_ab.sh` covers 53 sources × 3 faces = **159 emissions**; the S41
    split was verified against it three times (after the split, after `cargo fmt` reflowed the
    widened signatures, and after the comment fixes). Rule 9/10's A/B is the instrument for
    "nothing moved"; the suite is the instrument for "nothing broke". They are not substitutes.

## Method notes earned in S41

- **A causal story is not a measurement.** The plan's first draft explained CUDA's stale
  `tile_plan` by "forking causes drift". Sapir rejected it against the record in the plan's own
  §0 (`tile_plan` S25, last CUDA session S23). The replacement is a *gate*, which is both true and
  useful, where the story was neither.
- **Compiling is not running.** The SME probe compiled cleanly at `-march=armv9-a+sme2` and died
  with `EXC_BAD_INSTRUCTION`. Carry a probe to execution or it has proven nothing — the same
  lesson S23 learned when a fold bug survived every local check.
- **Probe before promising.** SME went from "can we do AMX too?" to a measured 3.49× and two
  disqualifying machine facts in under an hour, before a line of emitter was written.
- **Ask what the units have in common before deciding how much code they need.** Sapir's
  unification (SIMD/SME/AMX/tensor cores are one sentence with different sizes) collapsed the
  plan's "different algorithm shape" category down to "different leaf", which is what makes the
  SME leg cheap.

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction.
- Backend-genericity contract (ADR-0032): mapal-ir never learns machine facts.
- Query, not rewrite: record that something *could* be skipped; never delete it.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- Proof over suggestion — a change arrives with the measurement of what it did.
- Speak simply, base claims on empirical results.
