# 2026-07-29 — S41: two gates and a second unit

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Driven by Sapir.
Opened by `/category-architect start`; continuation chosen: **the GPU leg via NVPTX (P0 since S38)**.

## 0. Continuation brief

Current state: **plan-s41 is RATIFIED, step 1 of 8 is built and gated, and the two §2.2 gates that
make the whole obligation enforceable exist.** Nothing shipped moved an emitted byte: 159/159 A/B
emissions byte-identical, gate **1015 passed / 0 failed** (1006 → 1009 → 1015), fmt clean. A second
and cheaper leg was found, probed and priced: **ARM SME on the M4 Pro, 3.49× faster than the
current tuned NEON path on 1024² matmul**. All S41 work is **uncommitted** on `main` @ `aaaa5dd`.

Next step: **the SME leg**, per Sapir's stated order ("write the §2.2 gates first, then do SME in
parallel"). Gates are done; SME is next.

Resume command/check: `cargo test --workspace --release` then `cat benches/sme/README.md`.

## 1. Work completed

**A. `func.rs` → `func/` (Sapir's directive; plan-s41 §2.3).** The single 7,299-line file — of
which `impl<'a> FnEmit<'a>` alone spanned lines 562–7128, **6,567 lines** — became eleven child
submodules, each with its own `impl<'a> FnEmit<'a>` block:

| module | lines | owns |
| --- | ---: | --- |
| `func/mod.rs` | 745 | `FnAttrs`, `FrameLayout`, the `FnEmit` declaration, site predicates, llt helpers, free helpers |
| `func/core.rs` | 820 | slots, name minting, loads/stores, heap + packed buffers, trap sites |
| `func/frame.rs` | 232 | storage prep, elided arrays, local slots, `%Frame` layout |
| `func/drive.rs` | 672 | `emit`, `emit_parallel`, `emit_task`, the `topo_order` walks |
| `func/ops.rs` | 743 | `emit_morphism` + the scalar ops |
| `func/tile.rs` | 492 | tile entry, `emit_tiled_map`, the register-blocked main tile |
| `func/window.rs` | 382 | the FIR 1-D window rung |
| `func/conv.rs` | 828 | the conv2d micro-kernel rung |
| `func/packed.rs` | 971 | BLAS rung 3 — packed j-outer + KC nest |
| `func/trio.rs` | 761 | the main tile trio |
| `func/vec.rs` | 377 | the vector path |
| `func/bulk.rs` | 428 | map, fold, zip, enumerate, iota, fill |

Visibility **preserved exactly, not widened**: previously-private methods became `pub(super)`,
which is visible throughout `func` and its children — the surface a single file already had.
`pub(crate)` count unchanged at 13. Three source comments pointing at the deleted `func.rs` were
fixed, and one of them (`HEAP_MIN_BYTES`) turned out to be stale since S31.

**B. Step 1 of the plan — the machine class.** `Machine::{Cpu, Gpu(Gpu)}` on `TargetProfile`,
`TargetProfile::gpu() -> Option<&Gpu>`, and the `CUDA_ADA` profile (sm_89, warp 32, 48 KB smem,
1024 threads/block — `arch` verified against the device, which reports compute capability 8.9).
Every pre-S41 profile is pinned `Cpu` by test, so no CPU realization can observe the new field;
`cuda-ada`'s inherited CPU-shaped fields are pinned as **placeholders, not GPU measurements**.

**C. The two §2.2 gates** — the durable output of the session.

- **Gate A**, `crates/mapal-ir/tests/consumer_coverage.rs` (4 tests): ADR-0033's hand-run grep,
  automated. Every backend is `Required` or `Exempt` with a reason **and an end condition**. An
  unlisted backend directory fails (no exemption by omission); a stale exemption fails; and
  `the_gate_is_not_vacuous` guards against a path typo making every check pass by finding nothing.
- **Gate B**, appended to `crates/backends/llvm/tests/tile_sites_pin.rs` (2 tests): the counts
  pinned *that* a site is recognized; this snapshots *what the record says* — every
  `TileSite`/`TileRead` field — plus `the_record_is_a_pure_function_of_the_graph`.

**D. Docs reconciled + the SME probe artifacts preserved** (see §7, §8).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| "Forking is why CUDA stopped consuming `tile_plan`" | **RETRACTED (Sapir)** | Unsupported. `tile_plan` landed S25; the last CUDA session was S23 — the plan's own §0 says so. Work stopping is the cause; packaging is independent. The retraction is written into the plan, not quietly deleted. |
| How to prevent geometry-query drift | **a test gate, not an architecture** (Sapir) | *"the tests should guard it by checking all consumers all the time."* Works in every packaging option; would have prevented the nine-session gap outright. |
| kind-1/kind-2 taxonomy in the plan | **corrected (Sapir)** | Every matrix unit is one sentence — stage operands → issue a block MAC → accumulate into a *resident* accumulator → keep it hot across the reduction axis → store once. So the tile **nest is shared** and only the **leaf** differs. The one structural split is **cooperation**, GPU-only. |
| NVPTX packaging: own crate vs `Machine` discriminator | **deliberately UNDECIDED**; default C for the leg | Demoted from "architecture verdict" to "packaging default" once the gates made drift a solved problem. Branch budget ~6, any branch in `emit_morphism` trips it; §8.5 judges on built code. |
| Extract a shared emitter trait first (option B) | **deferred** | ADR-0033 D5's own argument: the second consumer is what tells you which parts are schedule and which are `Loc` constants. Extracting first is the premature abstraction FRAMEWORK §5 forbids. |
| Do "AMX" alongside | **reframed then ACCEPTED as SME** | Intel AMX: no hardware (i9 reports `amx flag count: 0`). Apple AMX: undocumented, Accelerate-only, not emittable. **ARM SME**: documented, in LLVM, and on the dev laptop. |
| Where Gate B lives | **`tile_sites_pin.rs`, not a new file in mapal-ir** | Tried adding `mapal-syntax`/`mapal-lower` as dev-deps to mapal-ir (cargo accepts the cycle) and reverted: the corpus also needs the rewriter, because **no tile site is recognized before `rewrite`**. The file that is already the recognition tripwire is the right home. |
| Prune the 3 stale worktrees | **NOT done** | Two are dirty (13 and 1 files) with other sessions' uncommitted work. Recorded for Sapir instead of deleted. |
| `oainotes.md` | **left untracked** | External review; filing it into the docs tree is Sapir's call. Triaged in `next-session.md` §5. |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| A/B emission sweep after the split | **159/159 byte-identical** | the split moved zero emitted bytes |
| …re-run after `cargo fmt` reflowed widened signatures | 159/159 identical | formatting did not either |
| …re-run after the three comment fixes | 159/159 identical | comments did not either |
| …re-run after step 1 + both gates | 159/159 identical | the whole session moved zero emitted bytes |
| method count preserved | 113 → 113 | nothing dropped in the split |
| `cargo test --workspace --release` ×4 | 1006 → **1006** → 1009 → **1015**, 0 failed | baseline held, then +3 (step 1) and +4/+2 (gates) |
| `cargo fmt --check` | clean | |
| profile tests | 9/9 (4 new) | pre-S41 profiles pinned `Cpu`; GPU facts never defaulted |
| Gate A | 4/4 | |
| Gate B | 3/3 incl. new snapshot | |
| SME `llc` lowering probe | ✅ `fmopa za0.s, p0/m, p1/m, z0.s, z1.s` | LLVM 22.1.8 lowers SME, and generates `smstart`/`smstop` from attributes |
| SME `rdsvl` | **SVL = 64 B (512 bits)** | ZA 64×64 B ⇒ 16×16 f32 tiles, 4 of them — measured, not assumed |
| SME 16×16 execution | **0/256 mismatched** | SME actually runs on macOS 26.3.1 |
| SME precision face | 92/256 differ vs mul+add; **0/256 vs `fmaf`** | **`fmopa` fuses** ⇒ `contract`-face realization (ADR-0032 D1/D3) |
| SME 1024² GEMM, 1t, min-of-7 | **5.0320 ms** (427 GFLOP/s) | vs Mapal `flow-fma-1t` 17.5449 (**3.49×**) and numpy-1t 1.2977 (gap **13.5× → 3.88×**) |

Baselines quoted are `docs/performance/matmul/s33.md:150-158`, M4 Pro f32.

## 4. Live handoff state

Recorded in full in `docs/next-session.md` §"Live state at S41 close" — branch dirty at `aaaa5dd`;
three stale worktrees (one clean, two dirty and deliberately not removed); the Arch box up with an
RTX 4070 Ti and CUDA 13.3.1 already installed and **no repo checkout**; `benches/sme/` and
`benches/emit_sweep_ab.sh` moved from the session scratchpad into the tree so nothing load-bearing
dies with the session.

**Nothing is running.** No background job, no rented machine, no server, no port.

## 5. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | The SME leg | `benches/sme/README.md`, plan §2.4 | build the SME realization as a leaf swap behind a capability | a `tile_plan` site emits `fmopa` and passes the differential |
| P0 | Is M4 SME the same silicon as Apple AMX? | plan §2.4 | one SME matmul vs one Accelerate matmul, same size | answered **before** any numpy comparison is published |
| P0 | NVPTX steps 2–5 | plan §5 | device module preamble; no hardware needed | `llc -march=nvptx64` accepts the emitted module |
| P1 | NVPTX steps 6–8 | plan §5 | repo checkout on the box (nothing to install) | differential bit-exact on the 4070 Ti |
| P1 | The seam re-judgement | plan §8.5 | count `Machine` branches once the leg is built | ≤6 outside the known sites, none in `emit_morphism` |
| P1 | `docs/spec/mapal-as-implemented.md` is false in two places | next-session §5 | correct the CUDA and capture claims | the spec matches the tree |
| P2 | 3 stale worktrees, 2 dirty | next-session live-state table | Sapir decides | `git worktree list` shows only `main` |
| P2 | `oainotes.md` untracked | next-session §5 | file as a `general/` review record, or discard | tree has no untracked review |

## 6. Architecture / model changes

**New component: `backend-nvptx`** — planned, no crate, grounded by a ratified plan (FRAMEWORK's
grounding rule permits code *or an explicit plan*).

The model: the GPU tile kernel is **the same `Trn`** as the CPU tile kernel at a **second `Loc`** —
one new `TrnLoc` row over one transformation (§4.2). `Dat` and `Trn` are untouched: no new
`Operation`, no new `Ty` variant, no `validate()` change, **no `mapal-ir` edit**. New atoms are all
physical: `Loc` gains `SM`/`Gmem`/`Smem`; `Trm` gains `h2d`/`d2h`/`g2s`/`s2r`/`launch`; the packed
panel gains a second `DataLoc` at `Smem`.

**Coherence.** No law is violated. The leg's main structural claim is a use of Law 1: a
transformation placed at `Smem` must have its inputs materialised or delivered there, so
**`__syncthreads` (`llvm.nvvm.barrier0`) is the morphism that completes `g2s`** — omitting it is
Law 1 failing, which is §7.6's read-before-DMA race, and `compute-sanitizer --tool racecheck`
detects it directly. `runsAt` stays a relation (law 6): this is its fibre growing from one element
to two.

**Model divergence recorded, not hidden:** `TargetProfile`'s `vec_bytes`/`vec_regs`/`l2_bytes`
describe a vector register file and a cache hierarchy. Their GPU readings (per-thread registers,
shared memory) are S38's "one record field, two readings", and `cuda-ada` currently inherits the
CPU values as placeholders. Which of them a GPU realization actually reads is ADR-0033 D4(a)/(b)
and is answered by building. A related, sharper instance surfaced on the CPU side: **`tile_i`'s
"spend half the vector register file" policy has no meaning under SME**, because ZA is separate
silicon — the SME realization needs its own derivation of the same quantity.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/backend-nvptx/plans/plan-s41-the-nvptx-leg.md` | **new** — ratified; carries the model, the §2.2 gates, the retraction, the SME section, the numpy target |
| `components/backend-nvptx/STATUS.md` | **new** — planned/no-code, built vs unbuilt, open questions |
| `components/backend-llvm/STATUS.md` | the split, `Machine`, Gate B, the byte-identity evidence |
| `components/backend-llvm/IMPLEMENTATION.md` | **21 `func.rs:X` rows rewritten** to the submodule holding `X`; multi-module rows use `func/{a,b}.rs` |
| `components/backend-llvm/suggestions.md` | 6 citations rewritten |
| `components/ir/STATUS.md` | Gate A recorded (no `mapal-ir` source changed) |
| `docs/STATUS.md` | S41 roll-up header + a `backend-nvptx` component row |
| `docs/IMPLEMENTATION.md` | S41 note: the code root changed shape; the gate now enforces the roll-up |
| `docs/next-session.md` | rewritten for S42; measurement rule 13; the live-state table |
| `benches/sme/README.md` | **new** — probes, build flags, the measured ceiling, and what the numbers are NOT |
| this log | new |

**55 stale `func.rs` citations** were rewritten across four living docs. `cuda/func.rs` was
deliberately left alone — different crate, still exists.

## 8. Files changed

Code: `crates/backends/llvm/src/func.rs` **deleted** → `crates/backends/llvm/src/func/{mod,core,frame,drive,ops,tile,window,conv,packed,trio,vec,bulk}.rs` ·
`crates/backends/llvm/src/profile.rs` (`Machine`, `Gpu`, `gpu()`, `CUDA_ADA`, 3 new tests) ·
`crates/backends/llvm/src/{lib,module}.rs` (comment citations) ·
`crates/mapal-ir/tests/consumer_coverage.rs` (new) ·
`crates/backends/llvm/tests/tile_sites_pin.rs` (+2 tests) ·
`crates/backends/llvm/tests/snapshots/tile_sites_pin__tile_record_content.snap` (new).

Benches: `benches/sme/{README.md,svl.c,run16.c,run16b.c,mmN.c,probe.ll}` (new) ·
`benches/emit_sweep_ab.sh` (new — the 159-emission A/B harness, previously scratchpad-only).

Docs: as §7.

## 9. Method notes earned

- **A causal story is not a measurement.** The plan's first draft explained CUDA's stale
  `tile_plan` with "forking causes drift"; Sapir rejected it against the record in the plan's own
  §0. The replacement — a gate — is both true and useful, where the story was neither.
- **Compiling is not running.** The SME probe compiled cleanly at `-march=armv9-a+sme2` and died
  with `EXC_BAD_INSTRUCTION` on `cntd`. Carry a probe to execution or it has proven nothing.
- **Probe before promising.** SME went from a question to a measured 3.49× and two disqualifying
  machine facts in under an hour, before any emitter existed.
- **Ask what the units have in common before deciding how much code they need.** Sapir's
  unification collapsed "different algorithm shape" to "different leaf" — which is precisely what
  makes the SME leg cheap and the GPU leg the expensive one.
- **A refactor's instrument is the emission sweep, not the suite** (measurement rule 13).
- **Check the machine before planning around it.** The box GPU (RTX 4070 Ti, CUDA already
  installed) was never recorded anywhere; prior GPU legs rented a vast.ai 4090 per session.
