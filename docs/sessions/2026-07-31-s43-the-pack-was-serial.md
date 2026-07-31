# 2026-07-31 — S43: the pack was serial

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Driven by Sapir.
Continues `2026-07-31-s42-the-constant-that-cost-a-session.md`, which opened S43 on a hierarchical
tile→cache mapping. Governing record: **`docs/performance/s43-residency-and-the-thermal-artifact.md`**.

## 0. Continuation brief

Current state: **S43's stated P0 was verified, then re-sized, then relocated — and the relocation is
the session.** Operand residency is real (1.71× at one thread, assembly-verified) but worth ≤5%
threaded. What actually binds threaded is a **serial B pack consuming 30.2% of the wall while 13
lanes idle**. Parallelizing it is built and measured: **N=4096 threaded 53.98 → 39.08 ms, 1.381×**,
and against a same-session numpy baseline **1.139×, disjoint, values identical** — the first time
this project has passed numpy/Accelerate on a matmul cell. Along the way S42's published `1864` L1
ceiling was retracted as a thermal artifact, and the repo's byte-identity gate was found capable of
reporting a clean pass while measuring nothing.

Next step: **decide what merges.** Four agent worktrees hold uncommitted work; `main` holds only docs
and instruments. See §5 P0.

Resume command/check: `git worktree list`, then read
`docs/performance/s43-residency-and-the-thermal-artifact.md` §4c and §4e.

## 1. Work completed

**A. The P0 verification (plan-s43-operand-residency-verification.md).** An operand-window instrument
— two `and` masks on the k-derived offsets, patched into the emitted `.ll`, **not** an `EmitOpts`
flag — priced residency without `kc`'s confound. Threshold declared before the run. **Confirmed:
174.596 → 101.854 ms, 1.71×, 64.7 ms of clear air.** B carries all of it; A ≤1.2% and not separable.

**B. The published ceiling was retracted.** `loadcost.c`'s own binary, re-run three times, reads
~2000 GF/s at 32 KB / 4 loads against the published 1864.2. Its 64 MB row still reproduces exactly —
the table was **half-valid**. `1043 GF/s` was also never the N=4096 cell (803 is).

**C. There is no L1 cliff and no L2-slice cliff** (`loadlevel.c`): flat ~1990 GF/s / 249 GB/s from
32 KB to 8 MB. The only capacity cliff is shared-L2 → DRAM at 8–12 MB.

**D. A TLB wall, measured** (`tlbreach.c`): bytes held constant, page span varied 64× via odd stride
multipliers. Knee at **2k–4k pages**, reproduced at two byte counts. **Pages beat bytes**: 16× more
bytes at constant pages costs 6%; crossing the page knee at constant bytes costs 36%.

**E. `nc` blocking built and swept** — ships **OFF**. Threaded parity at best; every arm that
actually shrinks the working set loses disjointly.

**F. THE FINDING — the B pack is serial.** `@task7` is registered `kind = 0`; `mapal-rt` runs
`kind == 0` exactly once on one thread. It holds 0 `fmopa`, 8 stores and a nested parallel call. The
accounting closes additively at both ends of the thread range (2.2% / 3.6% residual). The emitted
matmul kernel, driven standalone, is already **93% of the two-unit ceiling** — it was never the
problem.

**G. The parallel B pack, built** (plan-s43-parallel-bpack.md). Pack dispatched over `jt` in its own
nested run ahead of the matmul. **N=4096 threaded 1.381×; 1 thread 0.998× (no cost).**

**H. vs numpy, same session** (`numpy_ab.sh`): **4096 wins 1.139× disjoint; 2048 parity; 1024 loses
0.826×.** One thread still ~2× behind.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| the four-rung L1→L2→L3 cascade as framed | **abandoned** | L1-vs-L2 costs nothing on this part; it buys a benefit the machine does not pay |
| verify before building (next-session §1 step 1) | **kept, and it paid** | it re-sized the prize and relocated the bottleneck |
| window instrument vs an `EmitOpts` flag | **`.ll` text patch** | the repo already owes a `kc_nest` deletion; a probe must not add shipped surface |
| `nc` default ON | **rejected** | threaded parity at best; every working-set-shrinking arm loses disjointly |
| `begin(2)` + `mapal_par_dep` for pack→matmul | **rejected** | `complete_slice` would schedule the matmul `Placement::Local`, silently changing its placement. Two sequential `begin(1)` runs instead |
| k-blocking the parallel pack | **rejected** | 1.96× serially, nothing on top of parallelism — DRAM-bound at 78 GB/s once spread |
| running three investigations in parallel | **retired mid-session** | cross-agent contention produced a 70% noise floor; Sapir serialized them |
| trusting the carried numpy baseline | **rejected, re-measured** | rule 19. It reproduced within 0.5%, but is now verified rather than assumed |

## 3. Tests, checks, benchmarks

| Check | Result |
| --- | --- |
| residency arms, N=4096 1t, 21 cycles | 174.596 → **101.854 ms**, disjoint, ≈35σ |
| …threaded | 54.291 → 51.788 (**4.8%, under the 6% floor — a bound, not a number**) |
| `loadcost.c` re-run ×3 | **2004.2 / 2000.4 / 1996.1** vs published 1864.2 — retracted |
| `loadlevel.c` 17-size sweep | flat ~1990 GF/s 32 KB → 8 MB; cliff 8–24 M |
| `tlbreach.c`, 72 cells | page knee **2112–4096**, reproduced 2304–4352 at 4 MB |
| `nc` sweep, 6 points × 2 widths | best threaded 0.997× (overlap); 1t optimum `nc`=1024 at 1.187× |
| parallel pack, N=4096 threaded | 53.98 → **39.08 ms, 1.381×, disjoint** |
| …N=2048 threaded | 6.899 → 5.157, 1.338×, disjoint |
| …N=4096 1 thread | 171.4 → 171.7, **0.998×, overlap — no cost** |
| vs numpy 4096 / 2048 / 1024 | **1.139× disjoint** / 1.026× overlap / 0.826× disjoint |
| value identity, every cell | identical to numpy's `c0`/`clast` at all three sizes |
| emissions moved (parallel pack) | **48 moved / 111 unmoved** — only matmul, only `rew`/`con`, never `raw`; zero cells off the diagonal |
| `cargo test --workspace --release` (bpack worktree) | **1032 passed, 0 failed** (1031 → 1032) |

## 4. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `0518e76` | **in sync with origin**; 2 modified + 14 untracked, all docs/instruments | `git status -sb` | Sapir's call |
| worktree | `agent-a718d8faeee0ea4b4` | **the parallel B pack — the one that matters** | `git -C … status -s` | merge or discard |
| worktree | `agent-a03f9b23183f1440c` | `nc` blocking, ships OFF, gates green | " | merge or discard |
| worktree | `agent-a00e835791cc868c5` | threaded-ceiling probes | " | probes already copied to main |
| worktree | `agent-a9c8b56e24b5dee89` | cache-vs-TLB probes | " | probes already copied to main |
| worktree ×3 | `…-Personal-**Flow**/…` | **still prunable** (pre-rename paths) | `git worktree list` | `git worktree prune` — **only after the four agent worktrees are resolved** |
| machine | Arch box `100.81.226.103` | up, no SME | `ssh … nproc` | owned box |
| artifact | box `~/mapal-s42/` | **107 MB, still there** | `ssh … 'du -sh ~/mapal-s42'` | delete when done |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |

**Nothing is running.** No background job, no server, no port.

## 5. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **Merge the parallel B pack** | worktree `a718d8faee` | review the 3-file diff, merge, re-run the gate on the merged tree | on `main`, gate green |
| P0 | decide `nc` blocking's fate | worktree `a03f9b2318` | ships OFF; merge as a documented lever or discard | merged or dropped |
| P1 | the NEON leg's pack win is **VOID** | §4d | re-measure (control spread was 6.5–8.5%); it looked ~1.15× | a clean number or a retraction |
| P1 | one thread is ~2× behind numpy | §4b/§4e | the operand-residency 1.71× is real and unclaimed; needs a design that is not `kc` | 1t GF/s moves off ~800 |
| P1 | `examples/vector.mapal` does not parse | §4d | 3 of 159 gate cells have always failed | it parses, or it leaves the sweep |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | unchanged from S42; lost on every machine | gone, or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | unchanged from S42 | the suite runs an SME binary |
| P2 | box scratch `~/mapal-s42` (107 MB) | §4 | delete when box work is done | gone |
| P2 | 3 prunable pre-rename worktrees | §4 | `git worktree prune` **after** the agent worktrees resolve | only `main` listed |
| P2 | f16/bf16 rung (2× MAC density) | S42 §5e | unchanged from S42; plan first | `svmopa_za32_f16_m` emitted |

## 6. Architecture / model changes

**No `mapal-ir` change. No `Dat`/`Trn` change. `mapal-rt` untouched.** Everything is backend `Loc`
facts and one placement correction.

**The model defect S42 recorded was real but mis-located.** S42 §6 said the operand window is one
`DataLoc` where it should be a chain of `DataLoc`s over one `Dat` (register → L1 → L2 → L3), with the
`Trm`s between them being tile swaps. Measurement says **that chain is nearly flat on this part** —
L1 and L2 are one `Loc` for throughput purposes, and only the L2→DRAM link and the TLB carry a cost.
The chain is real but has **two live links, not four.**

**The actual defect was a `TrnLoc`, not a `DataLoc`.** The B pack is one transformation placed at
**one** location while the matmul it feeds is placed across fourteen. Both are in the same component,
both were declared, and nothing in the model said the pack's fibre was a singleton by mistake — this
is FRAMEWORK §4.5 law 6 (`runsAt` is a relation) failing quietly in the *cheap* direction: not a
transformation assumed to have one location, but one **left** with one location because "run exactly
once" and "run on one thread" were conflated in the emitter. **`kind = 0` encoded both at once.**
That conflation is the session's architectural finding, and the fix separates them.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/s43-residency-and-the-thermal-artifact.md` | **new** — the whole session; §4c is the finding, §4d the build, §4e the numpy comparison, §5 what is not claimed |
| `performance/s42-sme-roofline.md` | retraction boxes at §0 and §5e; three new §8 entries. **Immutable log untouched; the perf record carries its own retractions, as it already did** |
| `components/backend-llvm/plans/plan-s43-operand-residency-verification.md` | **new** — written pre-build |
| `components/backend-llvm/plans/plan-s43-nc-blocking.md` | **new** — written pre-build, reconciled after |
| `components/backend-llvm/plans/plan-s43-parallel-bpack.md` | **new** (in worktree) — written pre-build by Fable, reconciled in §8 |
| `benches/emit_sweep_ab.sh` | **hardened** — bash shebang, failed emission is a hard error |
| `benches/perflock.sh` | **new** — the measurement mutex |
| `benches/matmul/numpy_ab.sh` | **new** — same-session interleaved numpy comparison |
| this log | new |

## 8. Files changed

`benches/{perflock.sh,emit_sweep_ab.sh}` · `benches/matmul/numpy_ab.sh` ·
`benches/sme/{winmask.py,resid_ab.sh,loadlevel.c,tlbreach.c,unitload.c,paneldrive.c,bpack.c,nc_sweep.sh}` ·
`benches/results-s43/**` · docs as §7. **Code changes live only in agent worktrees** (§4).

## 9. Method notes earned

- **A gate that cannot fail is not a gate (rule 23).** `emit_sweep_ab.sh` was `#!/bin/zsh` using
  `${=flags}`; under bash it printed `bad substitution`, passed **no flags**, hashed the raw face 159
  times and **exited 0** — a clean "159/159 identical" on precisely the gate that had to detect a
  packing change. It was caught only because the count 48 had been predicted in advance. A second
  silent-pass path lived beside it: a failed emission hashed empty output, the same constant every
  time, so two broken runs "matched". Verify the instrument reports an **injected** failure first.
- **Re-run the baseline binary before trusting a published table (rule 19).** S42's 1864 was thermal
  drift *during* the run — and the 64 MB row of the same table still reproduces exactly. **A table
  can be half-valid.**
- **A sweep needs a control arm that should not move (rule 22)** — and rep-outer interleaving is
  **not sufficient**, because every rep walks the swept axis in the same order, so a within-rep droop
  survives best-of-N intact. Measure the null arm back-to-back *inside* each cell and read the ratio.
- **Name every mechanism that predicts your table (rule 21).** Cache reach and TLB reach predicted
  the residency arms identically; no arm separated them, and saying so is part of the result.
- **Walls size the benefit; re-sweeps size the cost.** Both walls prescribed `nc` ≤ 512 and `nc`=512
  *lost* at both widths, with the optimum at 1024. A single-point test at the predicted value would
  have been wrong by 25 points. Rule 4 earned twice this session.
- **A concurrency guard must be bounded below the caller's liveness timeout.** The first `perflock`
  blocked indefinitely and the harness watchdog killed the agents it was protecting; its quiet check
  also ran only at acquire, so a build starting *inside* a measurement went unnoticed and produced a
  70% noise floor. Both fixed; on a machine that will not go quiet it now **refuses** rather than
  measuring.
- **A finding held only in an agent's context does not exist.** Eight agent runs died today. Nothing
  written to disk was lost; everything not written was.
- **Three sessions optimized the wrong 66%.** `kc`, operand residency and `nc` all modify the matmul
  loop; none touched the pack. Because the pack is serial its cost is thread-count-independent, so it
  presents as "the parallel part stops scaling" — **Amdahl wearing a memory-bandwidth costume.**
