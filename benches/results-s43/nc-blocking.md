# S43 — `nc` blocking in the SME rung: results

Plan: `docs/components/backend-llvm/plans/plan-s43-nc-blocking.md` (written BEFORE the code).
Worktree: `.claude/worktrees/agent-a03f9b23183f1440c`, branch `worktree-agent-a03f9b23183f1440c`,
base commit `0518e76`. **Nothing committed — left for review.**

Appended as results land. A number that exists only in an agent's context does not exist.

## 1. What was built

| where | what |
| --- | --- |
| `crates/backends/llvm/src/lib.rs::EmitOpts::sme_nc` | the lever: `Option<u64>`, default `None`. **Swept, not derived — the type says so** (measurement rule 4) |
| `crates/backends/llvm/src/func/sme.rs::emit_tiled_map_sme` | the `jc` loop, emitted **outside** the i loop, plus the j loop's start/end becoming the block bounds |
| `crates/backends/llvm/examples/emit.rs` | `--nc=<cols>` — the sweep instrument |
| `crates/backends/llvm/src/func/{mod,core,drive}.rs` | the lever threaded through `FnEmit::new` / `emit_parallel` / `emit_task` |
| `crates/backends/llvm/tests/sme_rung.rs` | 3 new tests: the `jc` level adds exactly one loop and does not touch the kernel; every illegal `nc` moves no byte; the lever is default-OFF |
| `benches/sme/nc_sweep.sh` | the round-robin sweep harness (value gate first, order rotated per cycle, absolute ms) |

**`mapal-ir` untouched** (ADR-0032). **`module.rs::sme_panel` untouched** — `nc` is a placement
change, not an algorithm change, which is what the bit-identical-values claim rests on.

The nest, after:

```text
for jc0 in (0..c).step(nc):           # NEW, outermost, inside the task body
    for i0 in (i_lo..i_hi).step(32):
        pack ap[k][i]                 # unchanged code, now run c/nc times — the price
        for j0 in (jc0..jc0+nc).step(32):
            mapal_sme_panel(...)      # call site and kernel byte-identical
```

## 2. Byte-identity — proved, not asserted

`benches/emit_sweep_ab.sh` against a baseline `emit` built from the same tree with the change
stashed (`target/tmp/nc/emit.before`) vs the change applied (`emit.after`):

```
before.txt: 159 emissions
after.txt : 159 emissions
diff       : EMPTY  ⇒  159/159 byte-identical
```

That sweep is `--target=generic` only, so the SME profile was swept separately — 40 emissions
(`benches/matmul/*_cap_f32`, `benches/shapes/*`, `examples/*`) at
`--rewrite --contract --target=apple-m4-sme`:

```
apple-m4-sme: 40 emissions compared, 0 differ
```

⇒ **199/199 emissions unchanged.** `nc` defaults to `None` and mints no name when off, so the
byte-identity holds for the SME profile too, not just the profiles that cannot reach the rung.

## 3. Value identity

Gated in `benches/sme/nc_sweep.sh` **before any timing is printed**: every arm's non-timing output
is compared against the **NEON** leg's, and the script exits non-zero on any mismatch. Unlike the
S43 window instrument, a mismatch here is a defect rather than an expected artifact — `nc` does not
split k, so every output block is still written exactly once by `PanelWrite::Store`.

(results below)

## 4. The sweep

### 4a-clean. N=4096 f32, THREADED — **the leg that decides the default, machine exclusive**

`benches/results-s43/nc-4096-threaded-clean.log`. Taken after the other two S43 investigations were
stood down, so nothing else built or benchmarked during it. 15 round-robin cycles + 1 discarded
warm-up, arm order rotated per cycle, explicit zero-effect control, `-O2 -march=armv8-a+sme2`,
`--rewrite --contract --target=apple-m4-sme`, packed, `--kc` off. Values identical to the NEON leg
on every arm (`74348 -302529`) before any timing was read; panel kernel byte-identical across all
seven arms (1 distinct hash).

| `nc` | B per i-step | pages (16 KB) | min ms | median ms | max ms | vs off | distributions |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| off | 64 MB | 4096 | 53.2675 | **54.1472** | 55.8814 | 1.000× | — |
| **ctl** (control) | 64 MB | 4096 | 52.8651 | **54.0213** | 56.0362 | 1.002× | OVERLAP — **0.23% drift, spread 6.0%, run stands** |
| 128 | 2 MB | 128 | 79.2283 | 81.9267 | 82.9303 | **0.661×** | disjoint — a 34% LOSS |
| 256 | 4 MB | 256 | 63.8994 | 65.0842 | 66.4098 | **0.832×** | disjoint — a 17% LOSS |
| 512 | 8 MB | 512 | 56.5850 | 57.9958 | 60.9927 | **0.934×** | disjoint — a 7% LOSS |
| 1024 | 16 MB | 1024 | 54.5300 | 55.3078 | 56.9945 | 0.979× | OVERLAP — noise |
| 2048 | 32 MB | 2048 | 53.7772 | 54.2902 | 55.2905 | 0.997× | OVERLAP — noise |

**This reproduces run 4a below to within noise on every arm** (4a: off 54.1491, 256 → 0.840×,
512 → 0.928×, 1024 → 0.975×, 2048 → 1.000×). Two independent clean threaded runs, one before the
machine was quiesced and one after, agreeing arm for arm.

⇒ **§6's hypothesis is REFUTED threaded, and the refutation is monotone.** The best arm is parity
(2048, 0.997×, overlapping). Every arm that actually shrinks the B working set loses, and loses more
the more it shrinks it: 0.979× → 0.934× → 0.832× → 0.661×.

**The most informative cell is `nc` = 512.** It puts B per i-step at 8 MB / 512 pages — **inside the
capacity knee (8–12 MB) AND inside the TLB reach (~2k–4k pages)**, i.e. it is exactly the
configuration `docs/performance/s43-residency-and-the-thermal-artifact.md` §4b prescribes. It still
loses 7%, disjointly. So the walls are cleared and the transformation still does not pay threaded:
**the `c/nc` = 8 re-pack costs more than the residency it buys, and the reason it buys so little is
that 14 threads sweeping `jc` in lockstep already share B in the 16 MB L2.** That is §3's cost model,
confirmed rather than merely predicted.

### 4a. N=4096 f32, THREADED (first clean run, other agents active but control held)

`benches/results-s43/nc-4096-threaded.log`. 13 round-robin cycles + 1 discarded warm-up, arm order
rotated per cycle, `-O2 -march=armv8-a+sme2`, `--rewrite --contract --target=apple-m4-sme`, packed,
`--kc` off. Values identical to the NEON leg on every arm (`74348 -302529`) before any timing was
read. Panel kernel byte-identical across all six arms (1 distinct hash).

| `nc` | B block | min ms | median ms | max ms | vs off | distributions |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| off | 64 MB (all of B) | 53.2962 | **54.1491** | 55.2544 | 1.000× | — |
| 256 | 4 MB | 62.5379 | 64.4457 | 67.1213 | **0.840×** | disjoint — a 16% LOSS |
| 512 | 8 MB | 57.1004 | 58.3475 | 59.7905 | **0.928×** | disjoint — a 7% LOSS |
| 768 | 12 MB | 53.1583 | 53.9409 | 54.4290 | 1.004× | OVERLAP — noise |
| 1024 | 16 MB | 54.8624 | 55.5278 | 58.2425 | 0.975× | OVERLAP — noise |
| 2048 | 32 MB | 53.9862 | 54.1345 | 56.8244 | 1.000× | OVERLAP — noise |

⇒ **The hypothesis (§6 of the plan: some `nc` ≥6% below off, disjoint) is REFUTED.** The best arm
is 768 at +0.4%, well inside the 6% noise floor and overlapping. Every arm small enough to put
B under the measured 8–12 MB knee **loses, disjointly**, and loses more the smaller it gets —
0.928× at 8 MB, 0.840× at 4 MB. That is the pack-multiplier curve the plan derived in §3 before
the run: `c/nc` = 8 and 16 packs against a saving the threaded leg does not have to collect,
because 14 cores already share B in the 16 MB L2.

**No third outcome was admitted, and none is claimed.** `nc` does not pay threaded.

#### 4a-void — one threaded re-run is DISCARDED, and why

`benches/results-s43/nc-4096-threaded-ctl.log` was a repeat of 4a with the explicit control and
legal widths only. **It is void and nothing is read from it.** `off`'s own distribution came back
53.9066 / 75.3287 / 92.7373 ms — a spread of **72%** against the 3.7% of the run above. Every arm
overlaps every other arm; the run measures the machine, not the parameter. `perflock` reported
`0s quiet`, so no compiler was running: its busy-list catches compilers, not another worktree's
benchmark binary.

It is recorded rather than deleted because "the control's *median* was within 1.45%" would have
passed a median-only check. **A control has to be read on its spread, not only its centre.**

A second attempt (`nc-4096-threaded-ctl2.log`) is **also void, same cause**: `off` came back
55.5880 / 77.1326 / 91.2720 ms — a 64% spread, control median 4.03% off, every arm overlapping.

**Cause, established afterwards and not by me:** two other agents were building and benchmarking on
this machine during both runs. `perflock.sh` checked for a quiet machine **once, at acquire**, so a
build starting *inside* a 20-minute measurement window was invisible to it. Sapir has since (a)
made every heavy command — builds included — take the lock, and (b) stood the other two
investigations down. **Two voided runs are what justified that fix**, which is the entire reason a
control arm that costs one arm per sweep earns its keep: without it these two runs would have been
published as "`nc` = 1024 is a 4–7% threaded loss, overlapping" and nobody could have told that
from a real result.

| void run | `off` min/median/max | spread | control drift |
| --- | --- | ---: | ---: |
| `nc-4096-threaded-ctl.log` | 53.9066 / 75.3287 / 92.7373 | **72%** | 1.45% (median only) |
| `nc-4096-threaded-ctl2.log` | 55.5880 / 77.1326 / 91.2720 | **64%** | 4.03% (median only) |
| `nc-4096-1thread.log` (stands) | 168.6059 / 170.3160 / 173.2089 | 2.7% | **0.01%** |
| `nc-4096-threaded.log` (4a, stands) | 53.2962 / 54.1491 / 55.2544 | 3.7% | 0.4% (the accidental 768 arm) |

### 4b-clean. N=4096 f32, 1 THREAD (`MAPAL_PAR=1`) — machine exclusive

`benches/results-s43/nc-4096-1thread-clean.log`. 13 cycles + warm-up, explicit control.

| `nc` | B per i-step | pages | min ms | median ms | max ms | vs off | distributions |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| off | 64 MB | 4096 | 170.1460 | **171.0693** | 175.9594 | 1.000× | — |
| **ctl** (control) | 64 MB | 4096 | 169.1205 | 171.4070 | 172.5627 | 0.998× | OVERLAP — **0.20% drift, spread 2.0%, run stands** |
| 128 | 2 MB | 128 | 441.4737 | 445.7400 | 460.7738 | **0.384×** | disjoint — a 2.6× LOSS |
| 256 | 4 MB | 256 | 265.9321 | 268.0966 | 275.2316 | **0.638×** | disjoint — a 36% LOSS |
| 512 | 8 MB | 512 | 180.6352 | 181.6610 | 187.4045 | **0.942×** | disjoint — a 6% LOSS |
| **1024** | **16 MB** | **1024** | 143.7260 | **144.1583** | 147.5360 | **1.187×** | **disjoint — a 19% WIN** |
| 2048 | 32 MB | 2048 | 160.1402 | 162.1472 | 166.1203 | **1.055×** | disjoint — a 5% win |

**Reproduces 4b below arm-for-arm** (1024: 1.180× / 144.33 ms then, 1.187× / 144.16 ms now).

⇒ **At one thread the curve is unimodal with a real optimum at `nc` = 1024**, and the optimum is
**above** both walls, not under them: B per i-step at 16 MB / 1024 pages is *past* the 8–12 MB
capacity knee and only just inside TLB reach. The arms that sit comfortably inside both walls —
512 (8 MB / 512 pg) and 256 (4 MB / 256 pg) — **lose**, and 128 loses 2.6×.

The reason is that the walls size the *benefit* while `c/nc` sizes the *cost*, and the cost is
steep: each halving of `nc` doubles the A re-pack (`benches/sme/packcost.c`: ~8.5 ms per sweep at
N=4096, so 4 → 8 → 16 → 32 packs is +34 → +68 → +136 → +272 ms). The optimum is where the two
marginal costs cross, and it lands at the largest `nc` that still cuts the reuse distance
meaningfully — one whole shared L2, not one knee's worth.

**A sweep that tested only the plan's own predicted value (512, the width both walls prescribe)
would have reported "`nc` loses 6%" and been wrong by 25 percentage points at one thread.** That is
measurement rule 4 repeating the `sme_kc` error verbatim, and the only thing that stopped it landing
again is that the sweep was a sweep.

### 4b. N=4096 f32, 1 THREAD (first clean run)

`benches/results-s43/nc-4096-1thread.log`. Same protocol, plus a **zero-effect control** (`ctl`) —
a second binary compiled from the *same* `.ll` as `off`, so it is byte-identical and must track it.

| `nc` | B block | min ms | median ms | max ms | vs off | distributions |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| off | 64 MB (all of B) | 168.6059 | **170.3160** | 173.2089 | 1.000× | — |
| **ctl** (control) | 64 MB | 168.4007 | **170.3072** | 172.5749 | 1.000× | OVERLAP — **0.01% drift, run stands** |
| 256 | 4 MB | 266.6645 | 267.7507 | 270.2248 | **0.636×** | disjoint — a 36% LOSS |
| 512 | 8 MB | 180.8069 | 181.8582 | 183.6526 | **0.937×** | disjoint — a 6% LOSS |
| ~~768~~ | — | 167.9147 | 169.8431 | 173.2869 | 1.003× | **REJECTED BY THE GATE — see below** |
| 1024 | 16 MB | 143.4090 | **144.3291** | 145.8402 | **1.180×** | **disjoint — an 18% WIN** |
| 2048 | 32 MB | 153.4091 | 156.2960 | 160.6713 | **1.090×** | disjoint — a 9% win |

**`nc` = 768 was never measured.** 4096 is not a multiple of 768, so the legality gate
(`c % nc == 0` — the rung has no ragged-final-block path) rejected it and that arm emitted
`off`'s bytes. It is therefore an **accidental second zero-effect control**, and it came in at
1.003× (1 thread) and 1.004× (threaded) — independently corroborating `ctl`. Both legs carry a
control that did not move. **The legal widths at c=4096 are exactly {32, 64, 128, 256, 512, 1024,
2048}, so the sweep covers the whole legal space from 256 up; there is no unmeasured point between
1024 and 2048.**

⇒ At **one thread the curve is unimodal with a real optimum at `nc` = 1024**: 0.636× → 0.937× →
**1.180×** → 1.090×. This is exactly the shape measurement rule 4 exists for — three of the five
arms would each, alone, have supported a different conclusion.

**And the optimum is NOT where the plan bet it would be.** §3 sized `nc` to put B under the
measured 8–12 MB knee (`nc` ≤ 512…768); those are the arms that *lose*. The winner puts B at
**16 MB — the whole shared L2** — because the benefit is bounded by the reuse-distance cut
(64 MB → 16 MB) while the cost is `c/nc` re-packs, and at `nc` = 1024 that is 4 packs instead of
8 or 16. The knee sizes the *benefit*; the pack multiplier sizes the *cost*; the optimum is where
their marginal costs cross, and that is above the knee, not at it.

## 5. Test gate

`benches/results-s43/test-gate.log` — `cargo test --workspace --release`, run under `perflock`:

```
78 test binaries · 1034 passed · 0 failed · 1 ignored (the opt-in perf baseline) · exit 0
```

Including the 3 new SME tests (`nc_blocks_the_b_panel_without_touching_the_kernel`,
`an_nc_the_rung_cannot_honour_moves_no_byte`, `nc_is_default_off`) and the whole differential suite,
which is what enforces that a profile field is value-invariant.

## 5b. The transformation is in the assembly (measurement rule 2)

`clang -O2 -march=armv8-a+sme2 -S` on the `nc`-off and `nc`=1024 modules
(`target/tmp/nc/a_{off,nc}.s`):

```
@mapal_sme_panel:  BYTE-IDENTICAL in the assembly (441 lines)
  per k iteration: ld1w=2  fmopa=4  — the same in both arms
task-slice fn:  off  7 backward branches,  7 labels, 1 bl _mapal_sme_panel
                nc  10 backward branches,  9 labels, 1 bl _mapal_sme_panel
```

⇒ the `jc` loop **is a real outer loop in the machine code** (+3 backward branches, +2 labels), it
was not folded away, and the kernel it wraps is untouched down to the instruction. A transformation
you cannot find in the assembly is not a variant; this one is findable, and so is its absence.

**Trap for the next person:** `cargo test --workspace` **overwrites**
`target/release/examples/emit` with the CUDA backend's same-named example (cargo warns "output
filename collision"). The first assembly run silently used the CUDA binary and reported
`unknown flag: --contract` plus a "byte-identical, 0 lines" kernel. Rebuild
`-p mapal-backend-llvm --example emit` after any workspace build.

## 6. Verdict

**Default: OFF.** The number that justifies it is the **threaded** one: the best threaded arm is
`nc` = 2048 at **54.290 ms against `off`'s 54.147 ms** — parity, overlapping, inside the noise floor
— while every arm that actually shrinks the working set loses *disjointly*: 55.308 (0.979×),
57.996 (0.934×), 65.084 (0.832×), 81.927 (0.661×). Rule 5 is explicit that threaded decides, and
threaded says no.

The 1-thread 19% win at `nc` = 1024 (171.069 → 144.158 ms, disjoint, reproduced across two clean
runs) is recorded as what it is: a **one-thread-only optimization on this part**, the same verdict
`kc_nest` already carries (+6.1% 1t / −25.5% threaded). It is why the lever ships rather than being
deleted.

Byte-identity re-verified after every source edit including the doc comments:
**159/159 generic + 40/40 apple-m4-sme = 199/199 emissions unchanged.**

## 7. What this selects next, and what it retires

**Retired:** sizing an SME block against a wall and stopping. Both walls
(`docs/performance/s43-residency-and-the-thermal-artifact.md` §4 capacity 8–12 MB, §4b TLB ~2k–4k
pages) prescribe `nc` ≤ 512, and `nc` = 512 **loses at both thread counts** — 0.934× threaded,
0.942× at one thread, both disjoint. A wall sizes the *benefit*; it says nothing about the cost the
transformation pays to reach it. The `nc` = 128 arm makes the point unmissable: 2 MB of B, 128
pages, deep inside every wall, and it runs **2.6× slower** at one thread.

**Selected:** the **`mc` rung** — pack an `mc`-row block of A **once**, then walk `jc` inside it, so
every row of A is packed exactly once no matter how many `jc` blocks there are. It removes the one
term that beat this design. It is not a loop change: `ap` is an `entry_alloc` inside a **task**
function and task functions keep their `alloca`s (`heap_ok` is false for them), so an `mc·k·sizeof`
buffer — 4 MB at `mc` = 256, k = 4096 — needs the parallel path to heap-lower it. That is the
scoping the evidence now justifies rather than merely permits.

**Cheaper fallback if `mc` is not authorized:** a serpentine `j` sweep (reverse `j` on alternate
i-steps). It retains the tail of B across the turnaround at **zero** pack cost — strictly weaker
than `nc`, but the only variant on this axis that pays no multiplier.

**Unchanged and untouched:** `Sme::panel_l1d_ratio`, `sme_kc`, `kc_nest`, `module.rs::sme_panel`,
and all of `mapal-ir`.
