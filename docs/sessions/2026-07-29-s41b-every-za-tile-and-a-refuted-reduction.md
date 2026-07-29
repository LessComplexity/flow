# 2026-07-29 — S41b: every ZA tile, and a refuted reduction

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Driven by Sapir.
Same-day continuation of `2026-07-29-s41-two-gates-and-a-second-unit.md`, which closed with the
SME rung landed but sequential-only and unmeasurable on matmul.

## 0. Continuation brief

Current state: **the SME rung uses every ZA tile the profile records, runs on the parallel task
path, and is measured across 512–4096 at both widths.** NumPy's single-thread lead at 1024² went
**13.5× → 1.62×**; threaded at 4096², **3.4× → 1.28×**. Gate **1026 passed / 0 failed**, fmt
clean, **159/159 emissions byte-identical** for every pre-existing profile. The `block_plan`
reduction was run and **refuted as posed**; its one surviving 2-subset has both prerequisites
landed. Four commits made; **the last one (`328fbee`) is UNPUSHED — the network went down.**

Next step: **k-loop software pipelining** — the dominant remaining term. See §5 and
`docs/next-session.md` §1, which carries the concrete first move.

Resume command/check: `git push` (network permitting), then `cargo test --workspace --release`.

## 1. Work completed

**A. The parallel lift.** The rung was sequential-only, which made every matmul benchmark
ineligible — they all run on the task path. `func/sme.rs` now asks `bulk_bounds` for its row
range, the same question four other rungs ask, and `slice_sizing` hands the runtime `ti·t·c` as
the slice quantum when the rung fires. The second part is what makes the first sound:
`mapal-rt`'s `slice_ranges` cuts slices **on the quantum**, and the predicate requires
`rows % (ti·t) == 0`, so `n = rows·c` is an exact multiple, every boundary is panel-aligned, there
is no ragged tail, and the `lo/c` division is exact by construction.

**B. Every ZA tile, derived not hardcoded (Sapir's catch).** `profile.rs` recorded `f32_tiles: 4`
and the emitter used one — a machine fact written down and then ignored, which is exactly what
`TargetProfile` exists to prevent. `sme_block()` now derives the arrangement as the most-square
factorization of the tile count, so 4 ⇒ (2,2) falls out and 1×4 is rejected **by arithmetic** (it
needs 5 operand loads per 4 MACs where 2×2 needs 4), not by taste. f64 scales off the same
recorded number by element width rather than a second constant.

**C. The `block_plan` reduction — run, and refuted.** See §2 and §6.

**D. Merge steps 0 and 0b**, the two free prerequisites of the one surviving subset. Each proven a
no-op at 159/159 emissions.

**E. Two defects found by adversarial review, both fixed.** See §2.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Share one nest across all five tile rungs (Sapir's directive) | **REFUTED by the reduction, recorded not forced** | Sapir pre-authorised this outcome: *"if the shared object ends up with a flag per rung, that is worse than five honest copies."* Five structural breaks, §6. |
| "SME is a leaf swap" | **withdrawn** | `fmopa`'s output is rank-2; the nest above it must hand it a 2-D tile address and step j by `tj·t`. That is the nest changing, not the leaf. |
| Which pairs to merge | TILE-blocked + KC only | ~350 lines of near-clone whose deltas are four parameters, three already passed as arguments. |
| `emit_tile_kc_boundary_row` ≡ `emit_tile_packed_boundary_row` | **wrong pairing, corrected** | It sits at a different level of the nest and contains a whole j loop the other structurally cannot have. Its real twin is `emit_tile_kc_boundary_tile`. |
| `MAPAL_SLICE` misalignment | **fixed at root cause in `mapal-rt`**, not guarded in the emitter | The lever may probe slice *size*; it may not violate the task's declared *quantum*, which is a correctness bound. It now rounds up. |
| Threaded 1024/512 multi-tile gain | **not claimed** | Distributions overlap (1.03×, 1.14×). Recorded as "do not quote". |
| Publish a numpy comparison before answering whether M4 SME and Apple-AMX are the same silicon | **still deferred** | plan §2.4; it decides what the comparison means. |

## 3. Tests, checks, benchmarks

| Check | Result |
| --- | --- |
| `cargo test --workspace --release` | **1026 passed / 0 failed** (1015 → 1023 → 1026) |
| emission A/B, 53 sources × 3 faces | **159/159 byte-identical**, re-run after every step |
| …× generic/apple-m/zen3/cuda-ada | **636/636 identical** — the rung is invisible to pre-existing profiles |
| merge step 0 (`emit_acc_lane`, 5 sites → 1) | 159/159 identical |
| merge step 0b (`emit_row_window`, 3 sites → 1) | 159/159 identical |
| value identity, SME vs NEON vs 1-tile | identical at 512/1024/2048/4096, `--no-pack`, `attn_*` |
| hardware value differential (review) | 0 differing cells across non-square, k∤t, non-zero base, transposed A, packed/unpacked, arena, ASan |
| `MAPAL_SLICE` 4096/16384/32768 | all exit 0, correct values (4096 **segfaulted at `7732a5f`**) |

**Measured — M4 Pro f32, medians, every row a disjoint distribution.**

Single thread:

| N | NEON | SME 1 tile | SME 2×2 | vs NEON | numpy-1t | numpy ahead: start → 1 tile → **2×2** |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 512 | 2.2515 | 0.7451 | **0.3465** | 6.50× | 0.1600 | 13.6× → 4.66× → **2.17×** |
| 1024 | 19.4333 | 5.4102 | **2.1006** | **9.25×** | 1.2977 | 13.5× → 4.17× → **1.62×** |
| 2048 | 155.147 | 40.448 | **17.5630** | 8.83× | 10.529 | 14.4× → 3.84× → **1.67×** |
| 4096 | 1286.13 | 332.536 | **192.005** | 6.70× | 84.617 | 14.8× → 3.93× → **2.27×** |

Threaded:

| N | NEON | SME 1 tile | SME 2×2 | vs 1 tile | numpy-thr | numpy ahead |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 512 | 0.5577 | 0.2530 | **0.2229** | 1.14× | 0.1075 | 2.07× |
| 1024 | 2.2581 | 0.9715 | **0.9428** | 1.03× | 0.6757 | 1.40× |
| 2048 | 18.8745 | 7.6156 | **6.9658** | 1.09× | 5.3045 | 1.31× |
| 4096 | 151.5684 | 79.1425 | **56.7021** | **1.40×** | 44.143 | **1.28×** |

The NEON leg reproduces S33 throughout, which is what makes the comparison trustworthy.

## 4. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `328fbee` | **AHEAD 1, UNPUSHED — network down** | `git status -sb` | `git push` when reachable; may need `gh auth switch --user LessComplexity` first (the `sapiritur` account is 403 on this repo; both are in the keyring, active is restored to `sapiritur`) |
| network | github.com:443 | **unreachable** (fails in ~1 ms ⇒ local connectivity/DNS, not GitHub) | `curl -sS -m5 https://github.com` | — |
| commits pushed | `7732a5f`, `62b63d2`, `da4cd1d`, `b8980ee` | on `origin/main` | `git log --oneline origin/main -1` | — |
| worktree | `…7b9157fd/…/pre-s38` @ `d3ca82c` | exists, clean | `git worktree list` | `git worktree remove` — safe |
| worktree | `…f845faa7/…/wt` @ `6168863` | exists, **13 dirty** | `git -C <path> status` | **not removed** — another session's work; Sapir's call |
| worktree | `…fe60a79a/…/pre` @ `1daddaa` | exists, **1 dirty** | same | same |
| machine | Arch box `100.81.226.103` | up; RTX 4070 Ti sm_89; CUDA 13.3.1 at `/opt/cuda`; **no checkout** | `ssh … nvidia-smi` | owned box, nothing to stop |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |

**Nothing is running.** No background job, no rented machine, no server, no port.

## 5. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **k-loop software pipelining** | `next-session.md` §1 | **First look at the emitted assembly** — LLVM may already pipeline it. Only then write anything | 1t GF/s on one unit moves off ~1,022 toward NumPy's 1,655 |
| P0 | push `328fbee` | §4 | `git push` | `git status -sb` shows no "ahead" |
| P1 | KC blocking for SME, 1 thread | `s41-sme.md` | own k-panel loop; ZA park/reload costs more than a register spill | the 1t curve stops decaying at 4096 (1,022 → 716 today) |
| P1 | Merge steps 1–3 (`i_regions`, `j_split`, `trio`) | `next-session.md` §3 | one commit each, sweep between; **do not batch** | ~350 lines gone, 159/159 each step |
| P1 | Executing SME value check in `cargo test` | `benches/sme/README.md` | wire into `differential.rs` — plain `clang -O2` works, the "needs `-march`" claim was **tested and false** | the suite compiles and runs an SME binary |
| P1 | M4 SME vs Apple-AMX: same silicon? | plan §2.4 | one SME matmul vs one Accelerate matmul | answered **before** publishing a numpy comparison |
| P2 | Predication for non-multiple-of-32 shapes | `s41-sme.md` | 16/48-wide matmuls fell back when the panel quantised | `ATTN_16` takes the rung again |
| P2 | `f32_tiles > 4` generates unselectable IR | `s41-sme.md` | not reachable (architecture has 4); the 1..=64 test sweep pins arrangements that cannot be emitted | the sweep asserts only emittable counts |
| P2 | 3 stale worktrees, 2 dirty | §4 | Sapir decides | `git worktree list` shows only `main` |

## 6. Architecture / model changes

**The reduction, and what it refuted.** ADR-0033 D5 gated `block_plan` extraction on a second
consumer appearing. SME is that consumer, so the reduction was run rather than assumed —
FRAMEWORK §3's method, the five nests' morphisms side by side with their targets.

**Identical across all five: three things, all *below* the nest** — the `bulk_bounds` call,
`emit_tile_index`, and flat `out[i·C + j]` addressing. The nest itself is shared nowhere.

The five structural breaks, kept unsoftened because they are what did the refuting: **which axes
exist is not a parameter** (conv has no runtime k loop, window no i axis, SME neither k nor lane);
**conv's reduction space is a rhombus, not a product**; **the accumulator has four incompatible
forms, not four extents** (memory alloca · `phi <TJ x elem>` · a straight SSA chain with no phi ·
ZA state the emitter cannot name); **the leaf's output rank differs**, so the nest above a rank-2
leaf changes; and **SME's task-range conversion is correct only because of `slice_sizing`** — a
cross-module correctness coupling, not a shape preference.

**A trap it would have re-introduced:** a shared `tile_i` field would re-merge two quantities S31
deliberately named apart — `tile_i` is rows of vector accumulators bounded by the register file,
`WINDOW_SUBROWS` is lanes over a memory accumulator with no register bound. Both 4 today by
coincidence.

**No `mapal-ir` change in this session.** The record was sufficient throughout, which is itself
the ADR-0033 D4(a) answer for the CPU side.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/s41-sme.md` | S41b tables (1t + threaded, 512–4096), the GFLOP/s view, the shared-unit finding, the two corrections |
| `README.md` | matmul section rewritten — both widths, four sizes, one campaign each, plus GFLOP/s and % of NumPy; "Two builds" → "Three builds" |
| `performance/matmul.md` | S41 index row |
| `next-session.md` | S42 opens on k-loop pipelining; reduction refutation; merge steps 0/0b done, 1–3 queued with their hazards |
| `benches/sme/README.md` | `mm4.c` probe, the verified IR spec, what the numbers are NOT |
| this log | new |

## 8. Files changed

`crates/backends/llvm/src/{profile,module,lib}.rs` · `crates/backends/llvm/src/func/{sme,conv,core,trio,packed}.rs` ·
`crates/mapal-rt/src/lib.rs` (the `MAPAL_SLICE` quantum fix) · `crates/backends/llvm/tests/sme_rung.rs` ·
`benches/sme/{mm4.c,sme_ab.sh,attn256_timed.mapal,spec-verified.ll,spec-driver.c,README.md}` · docs as §7.

## 9. Method notes earned

- **A recorded machine fact that the emitter ignores is worse than no fact at all.** `f32_tiles: 4`
  sat in the profile while the kernel used one tile, and the resulting deficit was ~4×. Sapir's
  rule — *"always keep stuff generic not hardcoded"* — caught it; the derivation now makes 8 tiles
  or 1 tile fall out without an edit.
- **Run the reduction before writing the abstraction.** The five nests looked like one schedule and
  are not. Writing `block_plan` first would have produced the per-rung flag bag FRAMEWORK §5 warns
  about, and no test would have failed.
- **Check whether a throughput curve rises then falls before blaming cache.** The 4096 knee was
  read as missing KC blocking; it was arithmetic intensity, and multi-tile removed it threaded
  with no blocking added. The 1-thread curve still decays, which the threaded curve hid.
- **Do not compare an all-core number against a single-core peak.** Caught by Sapir. The useful
  replacement was measured: NEON scales 8.6× across cores, both SME legs ~2×, so the matrix unit
  is shared and threaded figures must be read per-unit.
- **An env lever must not be able to violate an emitter invariant.** `MAPAL_SLICE` could produce a
  slice start the SME kernel was never told to expect, and the fix belongs in the runtime that
  cuts the slices, not in a guard at the use site.
