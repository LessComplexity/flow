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

### 1. SME — SHIPPED and MEASURED. What is left is the two gaps below.

`docs/performance/s41-sme.md` has the full record. Headline, M4 Pro f32, **matmul1024**, values
identical between legs and distributions disjoint:

| | NEON | **SME** | SME vs NEON | numpy | numpy ahead: before → after |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 thread | 17.9611 ms | **5.4102 ms** | **3.32×** | 1.2977 ms | 13.5× → **4.17×** |
| threaded | 2.2372 ms | **0.9715 ms** | **2.30×** | 0.6757 ms | 3.3× → **1.44×** |

The NEON leg reproduces S33's recorded numbers (17.5449 / 2.2281), which is what makes the
comparison trustworthy. **numpy is still ahead** — this closed the 1t gap by ~3.2×, it did not
beat Accelerate. The compiler-generated kernel is within **7%** of the hand-written ceiling probe
(5.4102 vs 5.0320 ms), so the remaining distance is missing *rungs*, not emission quality.

Two settled facts that must not be re-derived: build with **`-march=armv8-a+sme2`**
(`armv9-a+sme2` compiles and then SIGILLs — SME without SVE), and **`fmopa` fuses**, so SME is a
**contract-face** realization under ADR-0032 D1/D3.

**Gap 1 — no executing value check in `cargo test`.** `tests/sme_rung.rs` is `str::contains` only.
The differential's hand-run evidence (0 differing cells vs NEON *and* the interpreter, over
non-square shapes, k not a multiple of the tile, non-zero base, transposed A, packed/unpacked,
arena, ASan) is **not repeatable from the suite**. A review claimed the harness could not cover
SME because it shells out to bare clang without `-march`; **that claim was tested and is FALSE** —
plain `clang -O2` compiles, links and runs the SME module correctly, because the emitted function
carries `"target-features"="+sme,+sme2"` as a per-function attribute. So wiring SME into
`differential.rs` is easy, not blocked.

**Gap 2 — CI cannot execute SME on any hosted runner.** Linux runners are x86; GitHub's
`macos-latest` is M1/M2-class and SME arrived with M4. Layer the check instead:
IR assertions (any machine) → **compiles + links (any Apple Silicon, including M1 — the assembler
takes `fmopa` from the function attribute, no hardware needed)** → runs + value-checked (M4+ only,
skip-with-reason otherwise, the pattern `differential.rs` already uses for absent `nvcc`). Real
execution coverage needs a self-hosted M4 runner or a pre-merge local run.

**Still open from plan §2.4:** whether M4's SME and M4's Apple-AMX are the same silicon. It
decides what the numpy comparison *means* and it is one cheap measurement.

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

### 3. P0 (recorded by Sapir for a later session) — the `block_plan` reduction

**The gate ADR-0033 D5 set has now fired.** Its words: *"the second consumer is what tells us
which parts of the llvm nest are schedule (generic) and which are `Loc` constants. Extracting
first is the premature abstraction FRAMEWORK §5 forbids."* SME is that second consumer, so the
extraction is no longer premature — it is due.

Sapir's framing at S41, which is suggestion #10 re-derived from first principles: *"if it exists,
and there is a place that asks this, why do another path that will ask the same question, instead
of asking it in general, and then dispatching an answer handler based on answer + target profile
— maybe a correct way is to create an object that answers all the questions (isn't it the queries
on the graph?) and then an algorithm based on target profiles that consumes all of it at once?"*

**The concrete smell:** five rungs each hand-roll their own i/j/k walk —
`emit_tiled_map_blocked` (tile.rs), `emit_tiled_map_conv` (conv.rs), `emit_tile_window_block`
(window.rs), `emit_tile_packed_kc` (packed.rs), `emit_tiled_map_sme` (sme.rs). The S41 lift is a
small instance of the same thing: SME had to be *told* to call `bulk_bounds`, a question four
other rungs already ask.

**Do NOT start by writing the abstraction.** Start with FRAMEWORK §3's reduction, which is cheap
and can falsify the whole idea: write the five nests' morphisms side by side **with their
targets**, and check which squares commute. Whatever commutes is `block_plan`; whatever does not
is a genuine per-`Loc` difference and stays segregated as a partial morphism.

**What would falsify it** (the reason to do the reduction rather than assume): the nests may
differ *structurally*, not parametrically — conv fully unrolls its taps, window blocks over lanes
rather than rows, KC parks partial sums in `out` at panel boundaries, SME's accumulator lives in a
register file that does not compete with the vector file. If the shared object ends up with a flag
per rung, that is worse than five honest copies and the reduction should be recorded as refuted.

Immediate concrete win if it holds: the SME rung currently has no B packing and no KC blocking
(`docs/performance/s41-sme.md`), and both already exist for NEON. Sharing the schedule is how SME
gets them without a fifth copy.

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
