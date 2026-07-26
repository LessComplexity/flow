# 2026-07-25/26 — S31+S32: deduced blocking, deduced scheduling, and eight refuted assumptions

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-25-s30b-measurement-and-readme.md`. 16 commits, `53efd8b..b6a1663`.

Driven by Sapir across the session, in order: *"row blocking should be generic per algorithm,
TI detected from the execution graph"* → *"verify performance, did it land in assembly"* →
*"let's do the threading, but per-step not per-program"* → *"what is the peephole C++ finds"* →
*"is the target profile tailored for the machine we upload to?"*.

## 0. Continuation brief

Current state: **everything committed, gate green (72 suites), tree clean.** Two rungs shipped
with measured wins; one diagnosis is **OPEN** with eight hypotheses eliminated.
Next step: the conv2d per-core gap, using the Arch i9 box where `perf` works
(`docs/performance/conv2d-per-core-gap.md` §"What is left").
Resume command/check: `docs/next-session.md`; then
`ssh -o BatchMode=yes <perf-box>` (key auth installed, no password).

## 1. Work completed

**S31 — `TargetProfile`** (`crates/backends/llvm/src/profile.rs`). Six hand-swept constants
become one named table plus arithmetic (`generic`/`apple-m`/`zen3`, `EmitOpts::target`,
`--target=`). `generic` reproduces every literal, so emission is byte-identical — checked as
**66 A/B emissions** against the previous commit. `tile_i = vec_regs/(2·acc_vecs_per_row)`
reproduces S26's swept 4 *and* its recorded TI=8 spill. The KC gate now closes by derivation.

**S31 — deduced row blocking** (`crates/backends/llvm/src/reuse.rs`, new). `i_reuse` /
`distinct_runs`: ~60 lines over recorded `TileRead` fields, **zero flow-ir change**. The
load-bearing claim is now code: **`ci == 0` (matmul) and `ci == cq` (conv) are the same
predicate at `q = 0` and `q = 1`**, so conv is blocked because the record says its read slides,
not because it is conv2d. conv2d **−25% at 1t** (0.5343 → 0.3992), FMA:load 0.80 → 1.20.

**S32 — deduced scheduling** (`plan-s32-deduced-scheduling.md` + flow-rt). The pool stops
inventing sizes and starts receiving them; slices are cut on the region quantum; over-
decomposition is deduced from `i_reuse`. **matmul512 1.43×, matmul1024 1.41× at the default
width**, conv2d deliberately unchanged.

**Cross-machine validation.** vast.ai Zen 3 (destroyed, $0.0188) and Sapir's Arch i9.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| "Deduce *the* thread count" (next-session S31 framing) | **discarded** | A program with conv2d@1024 and matmul@8192 has two right answers. The question is per dispatch |
| Order: conv accumulator before row blocking | kept, **for the wrong reason** | The 2.9×-vs-1.2× arithmetic that justified it was wrong in both terms. Still operationally right: TI>1 over a memory accumulator re-enters the promotion risk S29 lost |
| `oversub` shipped as 4 for `Invariant` | **rejected, then accepted after the real fix** | Regressed matmul1024 34% until the ragged-slice defect was found; then delivered 1.41× |
| Broadcast term for `tile_i` | **rejected** | The budget math put conv at 29 of 32 — under budget. It never predicted the spill it was invented to explain, and TI=2 falsified the causation |
| `native` profile | deferred | `vec_regs` is not probeable; a half-succeeding probe is worse than none |
| `kc_nest` tri-state auto-default | rejected | Its *auto* would enable the nest for every K>128 site under `generic`, breaking byte-identity |
| Publishing the 1.46–1.78× scheduling number in the README | **rejected** | Reachable only via an env lever at the time. Publishing a number nobody gets from a default build is the S30b framing failure |
| `examples/matmul4.flow` → `matmul4_loop.flow` | followed through | Sapir's rename; 3 test call sites + 2 snapshots |

## 3. Eight refuted assumptions — do not re-run these

Every one was killed by a measurement, not an argument. **This is the most valuable section of
this log.**

| # | Assumption | How it was tested | Result |
| --- | --- | --- | --- |
| 1 | conv's vector accumulator is worth **~2.9×** (from an IR mem-op table) | built it, measured | **~10%.** LLVM was already promoting that accumulator — S29 had said so. *Counting IR operations predicts nothing when the optimizer deletes them first* |
| 2 | Row blocking is the smaller half (~1.2×) | built it, measured | **~17% — bigger than the accumulator.** The ordering argument was backwards |
| 3 | Over-decomposition fails *through the compiler* but works via env | forced identical counts through both paths | **No path difference.** I had compared 128 slices (env) against 56 (compiler) |
| 4 | The pool is slow (thundering herd / steal contention) | 2-slice probe; 16× coarser GRAIN | Dispatch cost ≈ 0; coarser slices moved 14t by 20%/7%. **Not slow — under-specified** |
| 5 | conv's gap is register pressure / spills | forced `TI=2` | **0 spills and SLOWER** (0.446 vs 0.426) |
| 6 | We splat weights; C++ uses by-element FMA | disassembled both | Both emit `fmla.4s vd,vn,vm[i]`, **zero** `dup`/`ins` |
| 7 | Heap-pointer aliasing blocks weight hoisting | forced arrays to stack (distinct allocas) | **Identical** 69 loads, marginally slower |
| 8 | Missing alias info blocks hoisting | added `!invariant.load` to all 112 weight loads, recompiled | **No change at all.** LLVM already had permission |

**Two methodological errors that produced several of the above:**

- **Static instruction counts were read as dynamic.** "69 weight loads in `task7`" cannot
  distinguish a hoisted preheader load from a per-trip one. Only the back-edge-isolated inner
  body (274 instructions, 5 weight loads) is dynamically meaningful. The weight-reload story was
  never large enough to explain 55%.
- **A model was reverse-engineered onto an observation and then trusted.** The register budget
  was derived *after* seeing spills; when checked, it said 29 of 32 — no violation.

## 4. Measurement hygiene learned the hard way

Four separate times the *environment*, not the code, produced the number:

1. **Machine load.** An early C++ reading of 0.761/1.395 ms was contaminated; re-run clean, both
   binaries agreed at ~0.29.
2. **Run order.** Whichever binary runs *second* gains 2–6% on the **median** (frequency ramp
   carries between processes) and **0% on the minimum**. → *Quote min, never median, for
   cross-binary comparison.*
3. **`-march` mismatch (Sapir's catch).** Flow cross-compiled `-mavx2 -mfma` against C++ at
   `-march=native` read 1.28×; matched to `-march=raptorlake` it read **1.55×**. → *Flag-match
   or the comparison is void.*
4. **The profiler itself.** `perf` costs **+40% (cpp), +45% (flow-gen), +31% (flow-zen3)** —
   large and **asymmetric**. → *Bare pinned timings are the only quotable ones; perf is for
   counters only.*

Also: `vastai show instances` on a shared host gave bimodal Flow timings (1.56–2.81 ms). Pin
(`taskset`) or discard.

## 5. Discoveries

- **The conv2d gap is architecture-independent.** 1.54× on M4 Pro (NEON, 32 regs) and **1.55×
  on i9-14900F (AVX2, 16 YMM)** — same ratio on unrelated architectures, so the cause is
  structural in what we emit. The vast.ai Zen 3 "7×" is an outlier (shared host, bimodal,
  clang-15 vs clang-22).
- **Cache is exonerated on both machines.** i9 counters: L1D-load-misses 10,557 (cpp) vs 11,308
  (flow), and **flow-zen3 has fewer cache misses than either while still losing**. Zen 3
  cachegrind agreed independently (LL misses 0–1 both). **IPC is the gap: cpp 3.11 vs flow
  1.57.**
- **Our tiling works.** 8.6× over Flow's own untiled path (3.411 → 0.398 ms). The 1.55× is a
  residual on a large win.
- **The slicer had a 1.8× defect**: it derived a slice *count* then equal-divided `n`, so any
  count not dividing the block total left ragged pieces. Counts dividing 256 ran 2.49–2.73 ms;
  neighbours (43/52/57) ran 4.4–5.8.
- **A 7× cliff below one register block.** A slice under `TI × c` drops every piece onto the
  TI=1 fallback: matmul1024 2.45 → 17.97 ms at 2 rows/slice. The floor is a coherence
  constraint, not a preference.
- **conv2d's parallel deficit is majority scheduling, not kernel.** At its best width it is
  1.83× behind cpp-mt; at the default 14 it is 2.67×. The wrong width alone costs 1.46×.
- **The `zen3` profile is a coin-flip on real AVX2**: −1.4% on Zen 3 (worse), +0.5% on i9
  (better). Both within noise of `generic`. Its derivation is unvalidated.
- **`perf` is impossible in vast.ai containers** (`CAP_PERFMON` dropped, `perf_event_paranoid=4`,
  `/proc/sys` read-only). Sapir's Arch i9 is the measurement machine.

## 6. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **conv2d per-core gap — diagnose the stall** | `docs/performance/conv2d-per-core-gap.md` | On the i9: `perf stat` with backend-stall and store-forward events, pinned, `cpu_core/` prefixed. NOT `cache-misses` — twice exonerated | A named counter accounts for IPC 3.11 vs 1.57 |
| P0 | **A repeat-loop bench** | this log §7 | Needed for any kernel-isolated counter work. The obvious construction FAILS — see §7 | Kernel dominates its process so samples land in it |
| P1 | `work_per_element` in flow-ir | plan-s32 §2 | The one legal flow-ir addition; nothing deduces a size from the *program* without it | `intensity` computable per task |
| P1 | Step 3 — plan composition | plan-s32 §2.7 | `levels` over `path_plan.deps`; `∥` apportions lanes by width, `▸` maxes them | Two independent 4-lane tasks share 8 lanes |
| P1 | The five benchmark programs | plan-s32 §4 | `mixed_widths` (conv2d@1024 then matmul@8192) is Sapir's own case and cannot be expressed today | Wide DAGs are exercised at all |
| P2 | `zen3` profile derivation | this log §5 | Two hardware runs disagree in sign | A profile beats `generic` on its own hardware |
| P2 | Fold tap 0 into `fmul` | `emit_conv_block_tile` | Removes 16 of 274 instructions (~6%) — both kernels waste this | `movi` count drops |
| P2 | Stale `benches/matmul/*.ll` | plan-s31-target-profiles as-built | 72 files stale at HEAD, pre-existing | `regen.sh` clean |
| P3 | Runtime half of `TargetProfile` | plan-s32 §7 Q1 | Core count / P-E split are runtime facts; the emitter table has no home for them | Settled before width deduction |

## 7. The repeat-loop bench: why the obvious version fails

Written, measured, **deleted**. `benches/shapes/conv2d_1024_rep.flow` looped the map 50 times
accumulating `y[0]`. It ran 50 reps in 1.508 ms against 0.4 ms for one — because **the map does
not depend on the loop variable, so LLVM hoisted the whole kernel out and ran it once.** Correct
compiler behaviour, useless bench.

Making it loop-dependent without changing what is measured is the hard part: a runtime fold seed
**de-recognises the tile site** (the recogniser requires `ObjectKind::Constant` for `site.seed`),
and perturbing the image index alters the affine form the ksplit recognition keys on. Viable
routes not yet tried: a driver that re-randomises the image *between* reps (gen cost paid,
outside the timed region), or a `main` calling the kernel fn N times with different arrays.

## 8. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ `b6a1663` | **clean**, 16 commits this session | `git status --short` |
| worktrees | none | `s31-deduced-blocking` merged and removed | `git worktree list` |
| vast.ai | account | **0 instances**, credit **$13.845** (spent $0.0188) | `vastai show instances` |
| arch box | `<perf-box>` | idle; `~/flowbench` left in place with built binaries | `ssh -o BatchMode=yes …` |
| processes | none | — | `pgrep -fl conv2d` |
| artifacts | session scratchpad | disposable — every number is in the perf docs | — |

**The Arch i9 is the measurement machine.** SSH key auth is installed, so no password is
needed. A password was pasted in chat on 2026-07-26; it is deliberately recorded **nowhere** and
should be rotated. Memory: `arch-perf-box`, `conv2d-gap-is-architecture-independent`.

## 9. Docs reconciled

| Doc | Change |
| --- | --- |
| `plan-s31-target-profiles.md` | SHIPPED + as-built, four deviations, corrections to pre-build text |
| `plan-s31-deduced-blocking.md` | new; as-built for items 2/3/4 incl. the refuted 2.9× |
| `plan-s32-deduced-scheduling.md` | new; granularity nest, composition law, the two-sided grain bound |
| `docs/performance/matmul/s31.md` | S31 (KC past threshold), S31b (conv vs competitors), S31c (row blocking), S32a (slice sweep) |
| `docs/performance/conv2d-per-core-gap.md` | new; the OPEN diagnosis and eliminated causes |
| `docs/performance/matmul.md` | index rows S31/S31b/S31c/S32a |
| `components/backend-llvm/{IMPLEMENTATION,STATUS}.md` | every tile factor cites its derivation; `WINDOW_SUBROWS` named apart |
| `docs/STATUS.md` | S31 header + row, 78 llvm tests |
| `README.md` | S31 results; scheduling number kept OFF the table with an explicit caveat |

## 10. Files changed

New: `crates/backends/llvm/src/{profile,reuse}.rs`. Modified: `backends/llvm/src/{func,lib,module}.rs`,
`backends/llvm/examples/emit.rs`, `crates/flow-rt/src/lib.rs`, llvm+rewrite tests, snapshots,
`examples/` rename. Gate `cargo test --workspace --release`: **72 suites, 0 failed**; fmt clean.
