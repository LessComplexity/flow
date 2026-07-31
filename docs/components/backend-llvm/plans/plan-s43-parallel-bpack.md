# plan-s43 — the B pack runs at the width of the machine, exactly once

Status: **BUILT AND MEASURED — H confirmed. §8 reconciles this plan against the code and the
machine.** Written before any code, per FRAMEWORK §6.1.
Written: 2026-07-31 · S43 · authorized by Sapir
Governs: `crates/backends/llvm/src/func/drive.rs::FnEmit::emit_task` (the packed-flavor branch)
and `crates/backends/llvm/src/func/core.rs::emit_pack_copy` (two lines).
Component: `backend-llvm`. **No `mapal-ir` change (ADR-0032). No `mapal-rt` change — the runtime
already carries every semantic this plan needs. No profile field, no `EmitOpts` field, no flag.**
Predecessor: `docs/performance/s43-residency-and-the-thermal-artifact.md` §4c — *"what actually
binds threaded is a serial B pack, and it is Amdahl in a memory-bandwidth costume."*
Results land in: `benches/results-s43/parallel-bpack.md` (stub exists, appended as they land).

## 0. What this plan builds, and the evidence that selects it

Three sessions of matmul-loop optimization went three-for-three losing threaded, and §4c found
why. The evidence, measured not argued:

| evidence | what it establishes |
| --- | --- |
| the emitted `@mapal_sme_panel`, lifted verbatim and driven without `mapal-rt`: N=4096 GEMM in 35.875 ms = 3831 GF/s, **93% of the ~4100 GF/s two-unit ceiling** | the kernel is done. Optimizing it further buys percent, not factors |
| `@task7` emitted `kind=0` (Seq): **0 `fmopa`, 8 stores, 1 nested parallel call** — it packs all of B on ONE thread inside the timed region, then opens the nested matmul run | the serial term is in the emission, verified, not inferred |
| one-term additive model: pack 16.349 ms (thread-count-independent) + matmul (154.396 / 35.875) closes to shipped (174.596 / 54.164) with **2.2% / 3.6% residual at both ends** | **30.2% of the threaded wall is one thread packing B** |
| priced (`benches/sme/bpack.c`, `benches/results-s43/bpack.log`): the same pack parallel over `jt` is **16.402 → 1.721 ms, 9.53×, disjoint** | the fix exists at the thread count that ships. Projected 39.54 ms = 3476 GF/s ≈ **1.37×**, past Accelerate's threaded 3113 |
| k-blocking the pack: 1.96× serially, **9.421× vs 9.531× on top of parallelism** — spread over 14 cores the transpose is DRAM-bound at 78 GB/s | **k-blocking is retired, not deferred. It is not designed in, anywhere below** |

⇒ **Emit the B pack as a range-parameterized task and dispatch it across the pool, inside the
run-once wrapper that already exists.** "Exactly once" and "on one thread" are separable
properties; today's emission couples them by accident of inlining, and the whole change is the
separation.

## 1. The categorical model

| Atom | Today | After this plan |
| --- | --- | --- |
| `Trn` (the pack: b → `packed[jt][k][lane]`) | `core.rs::emit_pack_copy`, inlined into the wrapper | **the same loop nest, byte-for-byte, in its own function** — only the `jt` trip bounds become `(%lo, %hi)` via the existing `bulk_bounds` |
| `Trn` (the matmul: `sme_panel` / the NEON tile nest) | `@task{id}_slice` | **untouched.** Not one byte of the slice function moves — this is §5's in-repo negative control |
| `TrnLoc` (the pack's placement) | inside the wrapper's body ⇒ multiplicity **and** width both fixed at 1 by the same line of emission | multiplicity stays 1 (the outer `kind=0` registration, `drive.rs:266-275`, unchanged — S27's run-once wrapper, "help-first finish ⇒ width-1 sound"); **width becomes the pool's**, via a nested `kind=1` dispatch |
| `Loc`/`DataLoc` (the packed panel) | one frame-field buffer, allocated by the host, layout `[jt][k][lane]` | **unchanged** — same field, same allocation, same layout, same lifetime |
| `Trm` (the pack's DRAM stream) | 1 lane, 16.35 ms for the N=4096 panel (latency/TLB-bound serial) | all lanes, measured 78 GB/s, 1.72 ms |

**No coherence law was being violated.** Unlike `nc`, this is not a placement-honesty repair —
the operands were where the emission said. It is Amdahl repair: a `Trn` whose width was 1 for no
recorded reason, in a region whose every other `Trn` runs at machine width.

## 2. The emission, before and after

Today (`drive.rs::emit_task`, packed flavor — emits two functions per packed Split task):

```text
@task{id}_slice(i64 %lo, i64 %hi, ptr %frame)   ; split_range=true, the tile nest + A pack
@task{id}(i64 %lo, i64 %hi, ptr %frame):        ; the wrapper, dispatched kind=0 (run-once)
    <emit_pack_copy inline: jt over [0, tiles), serial>
    %h = mapal_par_begin(1)
    mapal_par_task(%h, 0, kind=1, @task{id}_slice, n, rank, min_slice, oversub, 0)
    mapal_par_launch(%h, %frame) ; mapal_par_finish(%h)
```

After — three functions, two nested runs, the outer registration untouched:

```text
@task{id}_slice(...)                            ; UNTOUCHED, byte-for-byte
@task{id}_pack(i64 %lo, i64 %hi, ptr %frame):   ; NEW — emit_pack_copy with split_range=true,
    <the same pack nest, jt over [%lo, %hi)>    ;   covering j-tiles [lo, hi)
    ret void
@task{id}(i64 %lo, i64 %hi, ptr %frame):        ; wrapper, still dispatched kind=0
    %h1 = mapal_par_begin(1)
    mapal_par_task(%h1, 0, kind=1, @task{id}_pack, tiles, rank, /*min_slice*/ 1, /*oversub*/ 4, 0)
    mapal_par_launch(%h1, %frame) ; mapal_par_finish(%h1)   ; <- pack complete before this returns
    %h2 = mapal_par_begin(1)
    mapal_par_task(%h2, 0, kind=1, @task{id}_slice, n, rank, min_slice, oversub, 0)
    mapal_par_launch(%h2, %frame) ; mapal_par_finish(%h2)
```

with `tiles = site.c.div_ceil(profile.tile_j(&site.elem))` — 256 at N=4096 f32 on every shipped
CPU profile (`tile_j` = 16 for generic/apple-m f32 and for the SME leg, where `sme_tile_site`
requires `tile_j == t`; 512 at f64's `tile_j = 8`; 128 on zen3's `tile_j = 32`).

### What was verified by reading, line by line, before this plan relied on it

1. **`jt` is embarrassingly parallel — VERIFIED** (`core.rs:494-614`). Per `jt`: writes land in
   `packed[panel_base + k·tile_j + lane]` with `panel_base = jt·(k·tile_j)` — exactly the
   half-open element range `[jt·k·tile_j, (jt+1)·k·tile_j)`, disjoint across `jt` by
   construction. Reads are `b[b.base + b.ck·k + (jt·tile_j + lane)]` — reads only, shared
   freely. The final panel's dead-lane zero-padding (`icmp ult j, c` → `pad`) is inside its own
   `jt` iteration. No reduction, no carried state: the three loop counters are function-local
   scratch. Any partition of `[0, tiles)` into slices at whole-`jt` boundaries is sound.
2. **The buffer is reachable from every lane — VERIFIED, no guard needed** (question 5's
   landmine, defused by reading). `frame.rs::build_frame_layout:200-218` inserts a `packed`
   frame field under the predicate {`self.packing` ∧ `tile_plan` present ∧ `TaskKind::Split` ∧
   `packing_site`}; `emit_task`'s packed branch (`drive.rs:409-415`) fires under the *same*
   predicate (`tile_plan` is `tiling.then(|| ir.tile_plan(f))`, `core.rs:54`, both sides). So in
   the parallel flavor `packed_buffer` (`core.rs:446-466`) **always** takes the frame-field arm.
   The backing storage is allocated by the HOST (`allocate_frame_packs`, called at
   `drive.rs:127`), where `heap_ok = true`: the 64 MB N=4096 panel goes through
   `mapal_rt_alloc` (it is over `heap_min_bytes` = 256 KB), tiny panels are host-stack
   `alloca`s — and the host cannot pass `heap_teardown`/`ret` before `mapal_par_finish`, so
   either way the pointer is live and valid for every worker that loads it through `%frame`.
   The `entry_alloc` fallback arm of `packed_buffer` belongs exclusively to the **sequential**
   flavor (`bulk.rs:27-40`, where `self.frame` is `None`), which this plan does not touch.
3. **The A pack is already parallel — VERIFIED, untouched** (question 6). It lives inside
   `sme.rs::emit_tiled_map_sme` (`pack ap[k][i]`, hoisted per i-panel — sme.rs:106, 383), i.e.
   inside `@task{id}_slice`. §4c's measured arm "parallel matmul + A pack = 35.875 ms" is the
   shipped slice, confirmed. The NEON jt-outer nest packs no A at all outside `kc_nest`
   (default OFF).
4. **`bulk_bounds` substitution is a proven no-op on the sequential text** (question 4).
   `bulk_bounds(tiles)` with `split_range == false` returns `("0", tiles.to_string())`
   (`core.rs:899-905`); substituted into `emit_pack_copy`'s two hardcoded lines it reproduces
   `store i64 0, ptr {jt_ctr}` and `icmp uge i64 {jt}, {tiles}` **character-identically**. The
   same function body therefore serves the sequential inline pack (unchanged bytes) and the new
   pack task (`split_range = true` ⇒ `%lo`/`%hi`). No other `bulk_bounds` consumer exists in
   the pack function — its body is exactly `packed_buffer` + `emit_pack_copy` + `ret void`.
5. **The runtime semantics** (grounding for §3). `mapal_par_finish` = `help_until(remaining ==
   0)` (`mapal-rt/src/lib.rs:1113-1121`): the calling thread executes/steals work **scoped to
   this run** (`take_for`, `lib.rs:545`) until the run drains — so a `finish` inside a task
   cannot deadlock on, or accidentally execute, outer-run work, and at `MAPAL_PAR=1` it runs
   the slices inline (test `wait_helps_while_the_background_worker_is_busy`; production T==1 is
   thread-free). Nested `begin(1)` inside a task is exercised in production (today's wrapper)
   and pinned by test (`nested_run_uses_innermost_tls_and_restores_outer`). Completion of the
   last slice under the state mutex, then queue handoff under the queue mutexes, gives the
   happens-before edge from every pack store to every matmul load.

## 3. The design decisions, each with the code that made it

### 3.1 Two sequential nested runs — the dep-edge single handle is cut

The expected shape was one `begin(2)` + `mapal_par_dep(0, 1)`. The runtime supports it
(`mapal_par_dep` → `deps_left`; `complete_slice` unlocks and schedules dependents). It is cut
anyway, for two reasons read out of `Run::schedule`:

- **Placement.** A dep-unlocked task is scheduled `Placement::Local(lane)`
  (`complete_slice`, `lib.rs:749`): every matmul slice lands on ONE lane's deque, reached by
  the other 13 only through steals, and the rank-sorted round-robin seeding
  (`Placement::Seed`, `lib.rs:635-692`) that the shipped matmul dispatch gets today is skipped.
  The dep-edge design silently changes the matmul's launch placement as a side effect. Two
  sequential runs keep the matmul's `begin(1)`/`launch` **byte- and behavior-identical** to
  today's.
- **Exercised surface.** Nested `begin(1)` runs are today's production path and are tested.
  A nested `begin(2)` with a dep edge is exercised **nowhere** — the emitter's only nested
  handle is `begin(1)` (`drive.rs:442`), and every `mapal_par_dep` test drives an outer run.

What the second handle costs: one `Box` + a few mutex operations + one condvar wake, **once per
outer-task invocation** — once per program run at these shapes, against a 39+ ms wall. The
ordering guarantee is identical (`finish` returns only when every pack slice completed; the dep
edge offers task-level, not slice-level, ordering too — no overlap is lost). Laziness bias and
the evidence agree: two sequential runs, each the shape the codebase already trusts.

Also considered and retired: promoting the pack to the **outer** DAG as its own task. It would
delete the nested run entirely, but the outer task list, dep edges, checkpoint wait globals and
pin globals are all emitted positionally from `path_plan` (`drive.rs:247-307, 341-364` — task
indices are baked into `@ckpt{n}_entries`/`@pin{n}_entries` constants). Inserting a backend-mint
task renumbers a DAG that `mapal-ir` planned. The run-once wrapper exists precisely to keep the
outer arity fixed; this plan keeps it.

### 3.2 The pack task's sizing — and the one way this change silently does nothing

`n` = `tiles` (the j-tile count: 256 at N=4096 f32). `slice_elems` (min_slice) = **1** — the
task's axis IS j-tiles, each iteration writes one disjoint `k·tile_j` panel (256 KB at N=4096
f32), so the quantum is one tile and every cut at an integer is panel-aligned by construction.
`oversub` = **4** to start (the `Invariant` value `slice_sizing` gives the matmul; slice
boundaries cost the pack nothing, so over-decomposition is free and feeds stealing on the P/E
asymmetric pool), **swept in §6**. `width` = 0, `rank` = `task.rank` (single-task run; inert).

**Do NOT pass `slice_elems = 0`.** `slice_ranges` (`lib.rs:209-292`) falls to the legacy rule
`min(T, ceil(n/GRAIN))` with `GRAIN = 4096`: at `n = 256` that is `min(14, 1)` = **one slice —
the pack stays serial and nothing anywhere fails.** This is the silent-nothing failure mode of
the whole plan, and it is why §5 gate 4 greps the emitted registration for `i64 1, i32 4`.
With `slice_elems = 1, oversub = 4, T = 14`: blocks = 256, wanted = 56, per = floor(256/56) = 4
⇒ **64 slices of 4 j-tiles (1 MB of writes each)** — real work on every lane. At `MAPAL_PAR=1`:
wanted = 4, per = 64 ⇒ 4 slices, executed inline by help-first `finish` — same work, four calls.

`slice_sizing(site)` is **not** reused for the pack: its floor is `rows_per_block · site.c` in
*output elements* — the matmul's axis, not the pack's. Wrong quantum, wrong units; the pack's
pair is two literals with a comment pointing here.

### 3.3 No lever

`nc` and `kc` were tuning parameters with a numeric space; each shipped a flag because "off"
had to remain emittable next to "on". This change is a structural placement fix with its price
already measured on the isolated term (9.53×, disjoint). The A/B baseline is the **before
binary** — emitted from the base commit by the harness — so no flag is needed to measure, and a
flag would only ship a dead serial path. **Confirmed ⇒ ships unconditionally. Refuted ⇒ the
emitter change is reverted**, and the plan + logs stay behind as the record.

## 4. The edits, named

1. `crates/backends/llvm/src/func/core.rs::emit_pack_copy` — two lines: bind
   `let (jt_lo, jt_hi) = self.bulk_bounds(tiles);` and use them in the `jt` init
   (`store i64 {jt_lo}, ptr {jt_ctr}`) and bound (`icmp uge i64 {jt}, {jt_hi}`). Proven
   byte-identical when `split_range == false` (§2, verification 4).
2. `crates/backends/llvm/src/func/drive.rs::FnEmit::emit_task`, packed branch —
   a) new third emitter for `@task{task_id}_pack`: `FnEmit::new(...)`, `guard_flavor = Task`,
   **`split_range = true`**, `prepare_storage()`, `frame = Some(frame.clone())`, then the
   `source`/`packed_buffer`/`emit_pack_copy` triple that today lives in the wrapper, then
   `ret void`;
   b) the wrapper drops `packed_buffer` + `emit_pack_copy` and gains the first nested run
   (`begin(1)` / `par_task(..., @task{task_id}_pack, i64 {tiles}, i32 {rank}, i64 1, i32 4,
   i32 0)` / `launch` / `finish`) ahead of the existing matmul nested run, which is unchanged;
   c) return `{slice_fn}\n{pack_fn}\n{wrapper_fn}`.

Nothing else. The outer `kind=0` dispatch (`drive.rs:266-275`), the slice function, the frame
layout, `allocate_frame_packs`, the sequential inline pack (`bulk.rs`), `slice_sizing`, every
profile, `mapal-ir`, `mapal-rt`: untouched.

## 5. Gates — every one runs before a timing is read

**The gate that will mislead, stated first: this change moves bytes on EVERY profile that
packs.** `packing_site` (`func/mod.rs:351-355`) is geometry, not ISA — the generic/NEON and
zen3 legs pack through the same wrapper as the SME leg. **A 159/159 byte-identity result is
FAILURE**: it means the packed branch never fired and the change did nothing.

1. **Value identity, before any timing, on every affected leg.** The §6 harness runs the
   before-binary, the after-binary, and the NEON-leg binary of the same source and refuses to
   print a timing unless all three agree (the `nc_sweep.sh` / `sme_ab.sh` discipline —
   `benches/sme/sme_ab.sh` already implements the NEON-vs-SME half). Values are bit-identical
   by construction: the pack writes the same bytes to the same buffer in a different
   interleaving, all of it before any reader is launched. Plus
   `cargo test -p mapal-backend-llvm --release --test differential` (interpreter oracle,
   -O0/-O2) — noting honestly that the differential examples contain no `packing_site`
   parallel program, so the harness triple, not the suite, is the load-bearing value gate.
2. **Byte-identity confined and enumerated, both directions.** `benches/emit_sweep_ab.sh
   <before-emit> a.txt` vs `<after-emit> b.txt` (159 emissions: 53 sources × 3 faces), and the
   same loop rerun with `--target=apple-m4-sme` appended (the `nc` reconciliation's second
   sweep). Every CHANGED line's after-emission must contain `@task{n}_pack` and its
   before-emission the inline pack in `@task{n}`; every UNCHANGED line must contain no packed
   parallel wrapper at all. Report **counts both ways** (moved because it packs / unmoved
   because it does not), with the classification greps in the log.
3. **Snapshots** (`cargo test -p mapal-backend-llvm --release --test golden_ll`, insta):
   exactly ONE snapshot may move — `parallel_matmul_cap` (whole-module golden of a parallel
   packed program; it gains `@task0_pack` and the second nested run; refresh via
   `cargo insta review`). `tile_nest_shape`, `tile_nest_shape_f64`, `tile_nest_shape_kc`,
   `tile_nest_shape_conv` and every other snapshot pin the slice/sequential paths and MUST NOT
   move — the in-repo negative control that the slice is untouched.
4. **Function-level diff + assembly (rule 15/18)** on the N=4096 SME emission:
   `@task{n}_slice` and `@mapal_sme_panel` byte-identical in the `.ll`; the only moved function
   is the wrapper, the only new one `@task{n}_pack`; `clang -S -O2 -march=armv8-a+sme2` shows
   the pack loop real in `@task{n}_pack`'s machine code and the kernel `.s` unchanged. Grep the
   wrapper for the pack registration carrying `i64 {tiles}` and `i64 1, i32 4` (§3.2's
   silent-nothing tripwire).
5. **`cargo test --workspace --release`** (includes `mapal-rt`'s scheduler tests and
   `sme_rung`, which pins the slice, not the wrapper).
6. **`git diff --stat`** touches only `crates/backends/llvm` (+ the refreshed snapshot).
   `mapal-ir` clean (ADR-0032), `mapal-rt` clean.

## 6. The falsifiable hypothesis, and the measurement that decides it

Harness: `benches/sme/bpack_sweep.sh` (new, `nc_sweep.sh` structure verbatim: round-robin
with per-cycle rotation, one discarded warm-up cycle, value identity first, absolute ms
min/median/max with explicit overlap statements, **every run through `benches/perflock.sh`**).
Flags `-O2 -march=armv8-a+sme2` — NEVER `armv9-a` (implies `+sve`; this part SIGILLs). Raw
series to `benches/results-s43/bpack-*.log`; tables appended to
`benches/results-s43/parallel-bpack.md`.

Arms, one emitted `.ll` each:
- `off` — emitted by the base-commit `emit` binary (the before binary IS the off lever);
- `ctl` — a second binary linked from `off`'s own `.ll`, byte-identical: the rule-22 null arm.
  **If `ctl` and `off` medians split by more than ~6%, the run is VOID** — that spread is the
  standing noise floor of this unpinned machine;
- `on` — the after emission (pack oversub 4);
- `on-o1`, `on-o2`, `on-o8` — rule 4 sweep of the one parameterized value this plan sets.
  Produced by substituting the oversub immediate on the `@task{n}_pack` registration line of
  the `on` arm `.ll`; the harness **asserts exactly one changed line per arm and fails
  otherwise** (the `resid_ab.sh` §6 lesson: assert the patch, never just print it). A flat
  curve ships 4; a non-flat curve ships its optimum and §7 gets the derivation follow-up.

Cells: {N=4096, N=2048} × {SME leg (`--target=apple-m4-sme`), NEON leg (default target)} ×
{threaded (pool default), `MAPAL_PAR=1`}. At least 12 cycles, a **multiple of the arm count**
(the §4c slot-balance lesson). Sources `benches/matmul/matmul4096_cap_f32.mapal` /
`matmul2048_cap_f32.mapal`, `--rewrite --contract`.

> **H** — At N=4096, SME leg, threaded: some `on` arm median is **at least 20% below** the
> `off` median (baseline 54.1 ms ⇒ at most 43.3 ms) **with disjoint distributions**, while
> `ctl` overlaps `off`.

§4c projects 39.54 ms (minus 27%); the bar sits at 20% so that dispatch overhead can eat a
third of the projection without the result decaying into a noise-floor argument. **No third
outcome** — confirmed ⇒ ships (no lever, §3.3); refuted ⇒ reverted with the table recorded.

Secondary predictions, recorded before the run:
- **1-thread, both legs: inside the noise floor of `off`.** The pack work is unchanged; the
  added cost is two handle round-trips and 4 inline slice calls. A disjoint 1-thread loss over
  6% is a defect to explain, not a trade to accept.
- **N=2048 threaded relative win at least N=4096's.** The serial term scales ~N² against a
  parallel term ~N³/T, so the pack's share GROWS as N falls. A 2048 win smaller than the 4096
  win falsifies §0's accounting, whatever the absolute numbers say.
- **NEON leg: no numeric bar** (its wall/pack split was never measured) — required: value
  identity, absolute ms reported, and no disjoint threaded regression. The pack it loses is
  the same absolute transpose, so the honest expectation is "positive, smaller share".

## 7. Deferred and retired, with reasons

- **k-blocking the pack** — RETIRED by measurement before this plan was written (9.421× vs
  9.531×; DRAM-bound at 78 GB/s across the pool). Not deferred: do not build it on this part.
- **Single-handle `begin(2)` + `mapal_par_dep`** — retired by §3.1 (unexercised nested shape;
  `Placement::Local` side effect on the matmul dispatch). Reopen only with a measurement that
  the second handle's cost is visible, which §3.1's arithmetic says it cannot be at once per run.
- **Pack task in the outer DAG** — retired by §3.1 (renumbers the planned DAG; wait globals
  bake task indices).
- **Deriving the pack's `oversub` / unifying with `slice_sizing`** — only if §6's sweep is
  non-flat; a derivation for a constant nobody has swept is `nc` §4's lesson.
- **A `width` cap for the pack (P-cores only)** — not built; the pack is bandwidth-bound and
  oversub-4 stealing already absorbs the P/E asymmetry. A measurement saying E-cores hurt the
  pack would reopen it.
- **`mc`, serpentine j, everything from `nc` §7** — untouched by this plan; the §4c accounting
  says the matmul side is 93% done and the next factor was never there.

## 8. Reconciliation — what the code became, and what the machine said

Status: **BUILT AND MEASURED.** Results and raw series: `benches/results-s43/parallel-bpack.md`
and `benches/results-s43/bpack-*.log`. Nothing committed; left for review.

### 8.1 The code became exactly what §4 named

Both edits landed as written, and nothing else did. `git diff --stat`:

```
 crates/backends/llvm/src/func/core.rs  | 11 +++++--
 crates/backends/llvm/src/func/drive.rs | 58 +++++++++++++++++++++++++++++++---
 2 files changed, 63 insertions(+), 6 deletions(-)
```

`mapal-ir` and `mapal-rt` clean (0 files). No `EmitOpts` field, no profile field, no flag, as
§3.3 committed. `bulk_bounds` reuse (§2 verification 4) held: the sequential inline pack is
byte-identical, proved by the 111 unmoved emissions. Every one of verifications 1–5 survived
contact with the build; none needed a guard, and §2's landmine (question 5) was correctly
defused by reading rather than by defensive code.

The plan's §9 cost estimate — "two lines in `core.rs`, ~35 lines in `drive.rs`" — came in at
+11/−2 and +58/−4 including comments. Close enough that the estimate was doing work.

### 8.2 H is confirmed; the projection materialized and was conservative

N=4096 f32, SME leg, threaded, 12 rotated cycles, machine exclusive: **53.98 → 39.08 ms median
at the shipped default, 1.381×, disjoint**, with the null control at 0.11% drift and 2.5% own
spread. The best sweep arm is 38.55 ms / 1.401×. §4c projected 39.54 ms / 1.37× — measured
**39.08 / 1.381×**, inside 1.2% of a projection made from an isolated probe. **3517 GF/s
shipped, past Accelerate's threaded 3113.**

### 8.3 Three things the plan got wrong, recorded rather than quietly dropped

1. **§5.3 predicted one snapshot would move (`parallel_matmul_cap`). ZERO moved.** That golden's
   program has no packing site, and `tile_nest_shape*` snapshot only the *slice* function — the
   one this change leaves byte-identical. So **no test in the repo covered the packed parallel
   wrapper at all**: a regression putting the pack back on one thread would have moved no
   snapshot and failed nothing. The negative control is stronger than claimed and the coverage was
   thinner. Closed by one test, `golden_ll.rs::packed_wrapper_dispatches_the_b_pack_across_the_pool`,
   which asserts `kind=1`, `0 < slice_elems ≤ tiles`, two nested runs with a `finish` between
   them, and a `[%lo, %hi)` pack body — and which was **verified to fail** by injecting §3.2's
   `slice_elems = 0` bug before being reverted.
2. **§6's secondary prediction "N=2048 relative win ≥ N=4096's" is REFUTED** — 1.345× against
   1.401×. It rested on the pack scaling as N², and `bpack.log` had already measured otherwise:
   1.726 → 16.402 ms is **9.5× for 4× the bytes**, because the pack is page-visit-bound. A
   super-quadratic serial term against ~N³/T means the pack's share *grows* with N. §0's
   additive accounting is untouched — it closes at both sizes (predicted 39.30/5.52 against
   measured 39.08/5.16).
3. **The NEON cell is VOID by the plan's own rule-22 gate** — the control's own spread is
   6.5% and 8.5% across two runs (its *median* drift is 0.00%/0.05%). Direction and rough
   magnitude survive (~13%, disjoint in both runs, two runs agreeing within 2.6 points); a
   three-digit NEON number does not.

### 8.4 The sweep was flat, and that is why it had to be run

`oversub` ∈ {1, 2, 4, 8} spans 38.55–39.08 ms — 1.4%, inside the control's 2.5% noise floor,
every pair overlapping. **No knee.** The emitter ships 4 (the `Invariant` value `slice_sizing`
already uses). A single-point test at 4 would have been *right* here — and rule 4 says that is
not the same as *justified*; only the sweep can distinguish a defensible default from a lucky
one, and §7's "deriving the pack's oversub" stays retired because there is nothing to derive.

### 8.5 Two instrument defects found while running the gates

- **`benches/emit_sweep_ab.sh` fabricates a clean pass under bash.** It is `#!/bin/zsh` and uses
  `${=flags}`; run with `bash` it prints `bad substitution` to stderr, passes NO flags, emits 159
  well-formed hash lines of the `raw` face only — which never packs — and exits **0**. The diff
  is empty: a 159/159 "byte-identical" result on precisely the gate §5 warns is misleading. Caught
  only because 48 was predicted in advance. **Invoke it with `zsh`**, or give it a shebang-
  respecting wrapper and a `set -o pipefail`-style guard.
- **A naive `sed '/_fn:/,/.cfi_endproc/p'` range on the `.s` reports the WRONG function.** The
  first attempt at §5.4 read "4 `fmopa` inside `@task7_pack`" — the range had spilled into the
  next function. It also compared raw `.s` text, which differs spuriously because adding a
  function renumbers every later `LBB<n>_<m>` label. Both fixed by parsing functions properly and
  normalizing label indices before hashing; with that, `@mapal_sme_panel` is **54/54 instructions
  identical** and `@task7_slice` **71/71 identical**, while `@task7_pack` is 68 instructions with
  **0 `fmopa`**, 2 `ldr`, 9 `str` — the transpose, and only the transpose.

### 8.6 Verdict

**Confirmed ⇒ ships** (§3.3 admits no third outcome). The change is unconditional, the pack now
runs at the width of the machine exactly once, and the 30.2% Amdahl term §4c identified is gone.

## 9. Cost

Two lines in `core.rs::emit_pack_copy` (proven no-op on the sequential text), ~35 lines in
`drive.rs::emit_task` (one new `FnEmit` block cloned from the wrapper's own setup, four emitted
runtime calls moved/added), one insta snapshot refreshed (`parallel_matmul_cap`), one sweep
harness, and 2 sizes × 2 legs × 2 thread widths × 6 arms of measurement. No new types, no
flags, no runtime change, no IR change.
