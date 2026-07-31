# S43 — parallel B pack: results

Plan: `docs/components/backend-llvm/plans/plan-s43-parallel-bpack.md` (written BEFORE the code).
Worktree: `.claude/worktrees/agent-a718d8faeee0ea4b4`, base commit `0518e76`.
**Nothing committed — left for review.**

Appended as results land. A number that exists only in an agent's context does not exist.

## 0. The finding being acted on

`docs/performance/s43-residency-and-the-thermal-artifact.md` §4c: at N=4096 the shipped threaded
SME GEMM is 54.164 ms, of which the emitter's B pack (`@task7`, emitted `kind=0` = Seq) is
**16.349 ms on ONE thread while 13 lanes idle — 30.2% of the wall**. Priced in
`benches/sme/bpack.c` (`benches/results-s43/bpack.log`): parallel over `jt` is
16.402 → 1.721 ms, **9.53×, disjoint**. Projected shipped 39.54 ms = 3476 GF/s ≈ **1.37×**.

k-blocking the pack is worth 1.96× serially and **nothing** on top of parallelism (9.531× vs
9.421×) — it is not built.

## 1. What was built

| where | what |
| --- | --- |
| `crates/backends/llvm/src/func/core.rs::FnEmit::emit_pack_copy` | the `jt` loop's init and bound become `self.bulk_bounds(tiles)`. Two lines. At `split_range == false` this reproduces the `0`/`tiles` literals character-identically, so the sequential inline pack (`bulk.rs`) is untouched |
| `crates/backends/llvm/src/func/drive.rs::FnEmit::emit_task` (packed branch) | a THIRD emitted function `@task{id}_pack(i64 %lo, i64 %hi, ptr %frame)` — `split_range = true`, body = `packed_buffer` + `emit_pack_copy` + `ret void`; the wrapper drops the inline pack and gains a nested `begin(1)/task/launch/finish` for it, ahead of the unchanged matmul dispatch |
| `benches/sme/bpack_sweep.sh` | the A/B + oversub sweep harness (before-binary is the `off` arm; value gate first; rule-22 null control; patch assertion on swept arms) |

**`mapal-ir` untouched** (ADR-0032). **`mapal-rt` untouched** — the runtime already carries every
semantic used. No `EmitOpts` field, no profile field, no flag (plan §3.3).

Run-once is preserved: the OUTER registration is still `kind = 0` (`drive.rs:266-275`), so the
wrapper body executes exactly once. Only the pack's WIDTH changed.

The emitted wrapper, N=4096 f32, `--target=apple-m4-sme`:

```llvm
define internal void @task7(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %t0 = call ptr @mapal_par_begin(i32 1)
  call void @mapal_par_task(ptr %t0, i32 0, i32 1, ptr @task7_pack,  i64 256,      i32 16777218, i64 1,      i32 4, i32 0)
  call void @mapal_par_launch(ptr %t0, ptr %frame)
  call void @mapal_par_finish(ptr %t0)
  %t1 = call ptr @mapal_par_begin(i32 1)
  call void @mapal_par_task(ptr %t1, i32 0, i32 1, ptr @task7_slice, i64 16777216, i32 16777218, i64 131072, i32 4, i32 0)
  call void @mapal_par_launch(ptr %t1, ptr %frame)
  call void @mapal_par_finish(ptr %t1)
  ret void
}
```

`i64 256` = j-tile count (`c.div_ceil(tile_j)`, `tile_j` = 16 at f32 here). `i64 1, i32 4` =
`slice_elems`, `oversub` — the silent-nothing tripwire: `slice_elems = 0` would fall to
`min(T, ceil(256/4096))` = **one slice**, and the pack would stay serial with nothing failing.
With (1, 4) at T=14: blocks 256, wanted 56, per 4 ⇒ **64 slices of 4 j-tiles**.

And `@task7_pack`'s loop head, showing the split actually reaches the emission:

```llvm
  store i64 %lo, ptr %s0
bb3:
  %t15 = load i64, ptr %s0
  %t16 = icmp uge i64 %t15, %hi
```

## 2. Emission gate — the byte-identity that would have MISLED

`packing_site` is geometry, not ISA. Every profile that packs moves. **A 159/159 byte-identical
result would be FAILURE here**, and the first attempt produced exactly that for a different
reason (see §2b).

`benches/emit_sweep_ab.sh`, 159 emissions (53 sources × 3 faces), `emit.before` (base commit
`0518e76`) vs `emit.after`:

| target | moved | unmoved |
| --- | ---: | ---: |
| `generic` (default) | **48** | 111 |
| `apple-m4-sme` | **48** | 111 |
| `apple-m` | **48** | 111 |
| `zen3` | **48** | 111 |

**Every moved emission moved *because* it packs — proved, not asserted.** Each of the 159
emissions was classified on two independent axes: did its hash change, and does its
before-emission contain a packed parallel wrapper (`define internal void @task{n}_slice`)?

```
 111 moved=no  packs=no
  48 moved=yes packs=yes
```

Zero cells off the diagonal: `moved ⟺ packs`, exactly. 48 is the same count on all four
profiles because `packing_site` is a geometry predicate the profile does not enter.

### 2b. Instrument defect found and recorded (rule: assert the gate, do not trust it)

`benches/emit_sweep_ab.sh` is `#!/bin/zsh` and uses `${=flags}` for word splitting. Run under
**bash** it emits `${=flags}: bad substitution` on stderr, passes NO flags at all, still writes
159 well-formed hash lines, and still exits 0 — so all 159 emissions are silently the `raw`
face, which never packs, and the diff is **empty**. That is a fabricated 159/159 pass on exactly
the gate this change was warned about. It was caught only because 48 was predicted in advance.
**Run it with `zsh`.** A stricter fix would be a shebang-respecting invocation or a `set -o`
guard; recorded here rather than patched.

## 3. Function-level diff, N=4096 `--target=apple-m4-sme`

Whole-module `.ll`, before vs after, hashed per `define`:

```
task7                        CHANGED
task7_pack                   NEW
identical functions: 16 / before=17 after=18
```

`@task7_slice` and `@mapal_sme_panel` are **byte-identical** — the in-repo negative control that
the matmul was not touched. The only moved function is the wrapper; the only new one is the pack.

### 3b. And in the machine code (rule 15/18)

`clang -O2 -march=armv8-a+sme2 -S` on both emissions, functions parsed out and `LBB<n>_<m>`
label indices normalized (adding a function renumbers every later label, which makes raw `.s`
text differ spuriously):

| function | off | on | instructions | |
| --- | --- | --- | ---: | --- |
| `@mapal_sme_panel` | `78b4ef18b029` | `78b4ef18b029` | 54 / 54 | **IDENTICAL** |
| `@task7_slice` | `1dbe50d18225` | `1dbe50d18225` | 71 / 71 | **IDENTICAL** |
| `@task7` (wrapper) | `9e3342bc9464` | `64753570e680` | 78 → 45 | the pack loop left it |
| `@task7_pack` | absent | present | 68 | **0 `fmopa`**, 2 `ldr`, 9 `str` |

The transpose is real in the pack function's machine code, and it contains no kernel. The kernel
this project spent three sessions on is bit-for-bit where it was.

**Instrument defect (recorded, second of two).** The first attempt at this gate used
`sed '/_task7_pack:/,/\.cfi_endproc/p'` and reported "**4 `fmopa` inside `@task7_pack`**" — the
range had spilled into the following function. It also hashed raw `.s` text, which differs on
label renumbering alone. A `sed` range over assembly is not a function extractor; parse the
labels.

## 4. Test gate

`cargo test --workspace --release`, both sides of the change:

| | passed | failed | ignored |
| --- | ---: | ---: | ---: |
| before (`0518e76`, clean) | 1031 | 0 | 1 |
| after | **1032** | **0** | 1 |

(+1 is the new coverage test below; the pre-existing 1031 are unchanged, run twice with no
shifting failing set.)

`benches/results-s43/test-gate-before.log`, `test-gate-after.log`. No `cargo clean` was needed —
the failing set never shifted between runs.

**ZERO insta snapshots moved, and the plan predicted one would.** §5.3 expected
`parallel_matmul_cap` to gain `@task0_pack`. It did not, and the reason is a *stronger* negative
control than the plan claimed: `parallel_matmul_cap`'s program has no packing site at all (its 7
tasks carry no `@task{n}_slice`), and `tile_nest_shape` / `_f64` / `_kc` snapshot **only the
slice function** (`function_containing(...)` selects the tiled nest's body, which is
`@task3_slice`) — precisely the function this change leaves byte-identical.

⇒ **Before this change, nothing in `cargo test` covered the packed parallel wrapper at all.** A
regression that put the pack back on one thread would have moved no snapshot and failed no test.
Closed with one test —
`crates/backends/llvm/tests/golden_ll.rs::packed_wrapper_dispatches_the_b_pack_across_the_pool`
(+76 lines) — asserting the pack is registered `kind=1` with `0 < slice_elems <= tiles`, that the
wrapper opens exactly two nested single-task runs, that a `mapal_par_finish` separates them (the
happens-before edge), and that the pack body walks `[%lo, %hi)`.

**And the test was verified to FAIL, not merely to pass.** Injecting the exact silent-nothing bug
(`slice_elems` 1 → 0) into `drive.rs`:

```
test packed_wrapper_dispatches_the_b_pack_across_the_pool ... FAILED
slice_elems=0 silently collapses the pack to ONE slice (GRAIN=4096):
  call void @mapal_par_task(ptr %t0, i32 0, i32 1, ptr @task3_pack, i64 1, i32 17, i64 0, i32 4, i32 0)
```

The bug was then reverted. A gate that has never been seen to fail is the `resid_ab.sh` Gate 2
mistake this repo already recorded; this one has been seen to fail on the failure it names.

## 5. THE RESULT — N=4096 f32, SME leg, THREADED

`benches/results-s43/bpack-4096-sme-threaded.log`. 12 round-robin cycles + 1 discarded warm-up,
arm order rotated per cycle, `-O2 -march=armv8-a+sme2`, `--rewrite --contract
--target=apple-m4-sme`, machine exclusive through `benches/perflock.sh`. Values identical to the
`off` leg on every arm (`74348 -302529`) **before any timing was read**; the slice function
hashes to 1 distinct value across `off` and `on`.

| arm | min ms | median ms | max ms | vs off | distributions |
| --- | ---: | ---: | ---: | ---: | --- |
| off (serial pack) | 52.7820 | **53.9835** | 57.9170 | 1.000× | — |
| **ctl** (byte-identical to off) | 53.3791 | **53.9245** | 54.6914 | 1.001× | OVERLAP — **0.11% drift, own spread 2.5%, run stands** |
| on (oversub 4, shipped) | 37.4337 | **39.0781** | 41.9089 | **1.381×** | **disjoint** |
| on-o1 | 37.9333 | **38.7762** | 40.3091 | **1.392×** | **disjoint** |
| on-o2 | 37.9429 | **38.5451** | 39.4598 | **1.401×** | **disjoint** |
| on-o8 | 37.8412 | **38.6070** | 40.4501 | **1.398×** | **disjoint** |

> **H is CONFIRMED.** The bar was "some `on` arm at least 20% below `off` with disjoint
> distributions, `ctl` overlapping". Every `on` arm is **27–29% below**, every one disjoint
> (max 41.91 < min 52.78), and `ctl` overlaps `off` at 0.11%.

**39.08 ms = 3517 GF/s at the shipped default; 38.55 ms = 3565 GF/s at the sweep optimum.**
§4c projected **39.54 ms / 3476 GF/s / 1.37×**. Measured **1.381× shipped, 1.401× best** — the
projection materialized and was slightly conservative. Both pass Accelerate's threaded 3113.

**The oversub sweep is FLAT.** 38.55 / 38.61 / 38.78 / 39.08 across o2 / o8 / o1 / o4 — a 1.4%
spread inside a noise floor the control puts at 2.5%, and every pair overlaps. There is no knee,
so per plan §6 the emitter **ships oversub 4** (the `Invariant` value `slice_sizing` already
uses) rather than a number a flat curve cannot justify. Recorded rather than tuned: a
single-point pick at o4 would have been *right* here, but only the sweep can say so.

## 6. N=4096 f32, SME leg, ONE THREAD (`MAPAL_PAR=1`) — the predicted null

`bpack-4096-sme-1thread.log`. 12 cycles. Values identical on every arm.

| arm | min ms | median ms | max ms | vs off | distributions |
| --- | ---: | ---: | ---: | ---: | --- |
| off | 169.2773 | **171.4303** | 175.3430 | 1.000× | — |
| ctl | 169.0519 | **170.3670** | 172.9743 | 1.006× | OVERLAP — 0.62% drift, own spread 2.3%, **run stands** |
| on | 168.8492 | **171.6948** | 175.1491 | 0.998× | OVERLAP |
| on-o1 | 168.8956 | **170.9418** | 172.6390 | 1.003× | OVERLAP |
| on-o2 | 169.6569 | **170.8195** | 173.8995 | 1.004× | OVERLAP |
| on-o8 | 169.8508 | **171.8373** | 173.2814 | 0.998× | OVERLAP |

Every arm within **0.6%** of `off`, all overlapping — inside the control's own 2.3% spread.
**The secondary prediction holds: the two extra handle round-trips and the four inline slice
calls cost nothing measurable at width 1.** No trade was made to get §5.

## 7. NEON leg, N=4096 f32 — THE CELL THAT IS VOID BY ITS OWN CONTROL

The pre-declared rule: **void the run if the control's own spread exceeds ~6%.** It does, on
both attempts. Reported in full anyway, because what survives the void is still informative.

`bpack-4096-neon-threaded.log` (12 cycles) and `bpack-4096-neon-threaded-2.log` (18 cycles):

| arm | run 1 median | run 2 median | run 1 vs off | run 2 vs off | distributions |
| --- | ---: | ---: | ---: | ---: | --- |
| off | 148.8600 | 152.1352 | 1.000× | 1.000× | — |
| **ctl** | 148.8555 | 152.2060 | 1.000× | 1.000× | OVERLAP — drift **0.00% / 0.05%**, own spread **6.5% / 8.5%** ⇒ **VOID** |
| on | 132.1180 | 131.9260 | 1.127× | 1.153× | disjoint |
| on-o1 | 135.4477 | 134.9177 | 1.099× | 1.128× | disjoint |
| on-o2 | 132.4213 | 132.7430 | 1.124× | 1.146× | disjoint |
| on-o8 | 131.3479 | 134.8713 | 1.133× | 1.128× | disjoint |

**What is void:** the precise magnitude. The NEON leg's noise floor at this size is 6.5–8.5%,
wider than the SME leg's 2.5%, so 1.127× and 1.153× are not distinguishable from each other or
pinned to two decimals.

**What survives the void, stated as the weaker claim it is:**
- the control's MEDIAN tracks `off` to **0.00% / 0.05%** in both runs — there is no drift on the
  swept axis, only per-sample variance;
- the treatment distributions are **disjoint** in both runs, by a margin (run 2: on max 137.81 vs
  off min 147.52, a 6.6% gap) that is itself comparable to the control spread;
- two independent runs agree on direction and land within 2.6 points of each other.

⇒ **The NEON leg improves, by roughly 13%, and does not regress.** That was the bar the plan set
for this leg ("no numeric bar; required: value identity, absolute ms, no disjoint regression").
Anyone quoting a NEON number to three digits from this table is over-reading it.

`bpack-4096-neon-1thread.log`, by contrast, is the tightest cell in the whole run — control
spread **0.5%**:

| arm | min ms | median ms | max ms | vs off |
| --- | ---: | ---: | ---: | ---: |
| off | 1237.4769 | **1239.3459** | 1240.8928 | 1.000× |
| ctl | 1237.8835 | **1240.1179** | 1243.7807 | 0.999× |
| on | 1238.8626 | **1241.2638** | 1245.2827 | 0.998× |
| on-o1/o2/o8 | 1237.5–1238.5 | **1239.6 / 1239.8 / 1239.9** | 1242.3–1245.4 | 1.000× |

All within **0.2%**, all overlapping. Another clean null at width 1.

## 8. N=2048 f32, SME leg, threaded — the secondary prediction is REFUTED

`bpack-2048-sme-threaded.log`. 12 cycles. Values identical on every arm (`-1045 51275`).

| arm | min ms | median ms | max ms | vs off | distributions |
| --- | ---: | ---: | ---: | ---: | --- |
| off | 6.8167 | **6.8985** | 7.0216 | 1.000× | — |
| ctl | 6.6478 | **6.8763** | 7.0056 | 1.003× | OVERLAP — 0.32% drift, own spread 5.4%, **run stands** |
| on | 5.0410 | **5.1570** | 5.2353 | **1.338×** | **disjoint** |
| on-o1 | 5.1003 | **5.1464** | 5.2711 | **1.340×** | **disjoint** |
| on-o2 | 5.0829 | **5.1287** | 5.1704 | **1.345×** | **disjoint** |
| on-o8 | 5.0910 | **5.1568** | 5.2868 | **1.338×** | **disjoint** |

A real, disjoint 1.34× — but the plan predicted **"N=2048 relative win at least N=4096's"**, and
it is not: **1.345× at 2048 against 1.401× at 4096.** The prediction is refuted.

**It does not falsify §0's accounting — it falsifies the N² model the prediction was built on**,
and `bpack.log` had already measured the contradiction before the plan was written. The serial
pack does not scale as N²: 1.726 ms at N=2048 → 16.402 ms at N=4096 is **9.5× for 4× the bytes**,
because it is page-visit-bound, not byte-bound. A super-quadratic serial term against an ~N³/T
parallel term means the pack's share *grows* with N, so the larger win belongs at 4096. The
additive model itself closes at both sizes:

| | off median | pack saving from `bpack.log` | predicted `on` | measured `on` |
| --- | ---: | ---: | ---: | ---: |
| N=4096 | 53.98 | 16.402 − 1.721 = 14.681 | 39.30 | **39.08** |
| N=2048 | 6.899 | 1.726 − 0.346 = 1.380 | 5.519 | **5.157** |

4096 lands within 0.6% of the isolated probe's prediction; 2048 beats it by 0.36 ms. **§4c's
one-term additive model survives at both sizes; only the plan's guess about how the ratio moves
with N was wrong.**

## 9. Verdict

| claim | outcome |
| --- | --- |
| **H (primary)**: some `on` arm ≥20% below `off`, disjoint, `ctl` overlapping, N=4096 SME threaded | **CONFIRMED** — 27.6% below at the shipped default, disjoint, ctl 0.11% |
| §4c's projected **1.37× / 3476 GF/s** | **MATERIALIZED and slightly exceeded** — 1.381× / 3517 GF/s shipped, 1.401× / 3565 GF/s at the sweep optimum |
| passes Accelerate's threaded 3113 GF/s | **yes**, at every `on` arm |
| 1-thread cost, both legs | **none measurable** (≤0.6% SME, ≤0.2% NEON, all overlapping) |
| NEON leg no regression | **holds**; ~13% improvement, but the cell is VOID by its own control spread (§7) |
| N=2048 relative win ≥ N=4096's | **REFUTED** — 1.345× vs 1.401×, and the reason is that the serial pack is super-quadratic (page-visit-bound), which `bpack.log` had already measured |
| oversub sweep has a knee | **no** — flat within 1.4%, every pair overlapping ⇒ ship 4, do not tune |
