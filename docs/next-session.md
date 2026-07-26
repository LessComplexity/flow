# Next Session (S33)

Written: 2026-07-26 · end of S31+S32 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-26-s31-s32-deduced-blocking-and-scheduling.md` — **read §3 before
proposing any conv2d hypothesis.**

## Where things stand (≤6 lines)

S31+S32 are committed and green (`b6a1663`, 72 suites, fmt clean, tree clean). Shipped:
`TargetProfile` (machine facts as data), `i_reuse`-driven row blocking (**conv2d −25% at 1t**),
and per-region slice sizing (**matmul 1.41–1.43× at the default width**). One diagnosis is
**OPEN**: conv2d's kernel is **1.55× slower than naive C++ on BOTH NEON and AVX2**, with
**eight hypotheses already eliminated**. Cache is exonerated on both machines; IPC is the gap
(cpp 3.11 vs flow 1.57).

## FIRST commands (resume checks, in order)

```sh
git log --oneline -6                       # S31/S32; HEAD should be b6a1663
git status --short                         # expect empty
cargo test --workspace --release 2>&1 | grep -c "test result: ok"   # expect 72
cat docs/performance/conv2d-per-core-gap.md          # the OPEN diagnosis
ssh -o BatchMode=yes <perf-box> 'nproc'   # the perf box, key auth
```

## S33 focus: finish the diagnosis, then finish the plan

### 1. The conv2d per-core gap (P0)

**Do not re-propose any of the eight refuted hypotheses** (session log §3): the 2.9×
accumulator, the ordering argument, an env-vs-compiler path difference, a slow pool, register
pressure, splat-vs-by-element, heap aliasing, or missing alias metadata. Each was killed by a
measurement and re-running them wastes a session.

What is established: same FMA count per output, **fewer** instructions on M4 (more on x86),
half the loads, equal cache behaviour, **2× lower IPC**, on two unrelated architectures.
It is a backend stall.

The measurement, on the Arch i9 (`perf` works there; vast.ai containers cannot run it):

```sh
taskset -c 0 perf stat -e cpu_core/cycles/u,cpu_core/instructions/u,\
cpu_core/ld_blocks.store_forward/u,cpu_core/resource_stalls.any/u ./binary
```

Hybrid CPU: events need the `cpu_core/` prefix and the process must be pinned, or counters come
back `<not counted>`. **Do not pull `cache-misses`** — exonerated twice, independently.

**Prerequisite: a repeat-loop bench.** The kernel is ~0.4 ms in a ~2 ms process, so counters are
process-level. The obvious construction fails and why is recorded in session log §7 (LLVM hoists
a loop-invariant map; a runtime fold seed de-recognises the tile site). Untried routes: a driver
re-randomising the image between reps, or a `main` calling the kernel fn N times.

### 2. Finish plan-s32 (P1)

Shipped: step 1 (pool receives sizes) and half of step 2 (the floor + deduced over-decomposition).
Missing:

- **`work_per_element` in flow-ir** — the one legal flow-ir addition, and without it *nothing*
  derives a size from the program itself; today's sizing comes from `TI × c` only.
- **`width` deduction** — always emitted 0.
- **Step 3, plan composition** — `levels` over `path_plan.deps`; `∥` apportions lanes by width
  with `rank` as tie-break, `▸` maxes them. Not started.
- **The five benchmark programs** (plan §4). `mixed_widths.flow` (conv2d@1024 then matmul@8192)
  is Sapir's own case and **cannot be expressed today** — every bench is a single-site pipeline,
  which is why the DAG rung has never been exercised.

### 3. Cheap wins

- Fold tap 0 into an `fmul` instead of `movi` + `fmla` — 16 of 274 instructions (~6%);
  both kernels waste it.
- Refresh the 72 stale `benches/matmul/*.ll` (pre-existing; they predate S30b's `time` migration
  and `regen.sh` exits 1 on the CUDA leg, which rejects `time`).

## Measurement rules (learned the hard way — session log §4)

1. **Quote min, never median.** Whichever binary runs second gains 2–6% on the median and 0% on
   the min.
2. **Match `-march` across binaries** or the comparison is void. This flipped an i9 result from
   1.28× to 1.55×.
3. **Bare timings only.** `perf` costs +31…45%, **asymmetrically** between binaries.
4. **Pin with `taskset`**; shared/hybrid hosts give bimodal results.
5. **Static instruction counts are not dynamic ones.** Isolate the inner loop by back-edge.

## Gotchas / warnings

- **The Arch i9 is the measurement machine**: `<perf-box>`, **SSH key auth
  installed — no password needed**. No clang there: cross-compile `.ll` on the Mac
  (`clang -target x86_64-unknown-linux-gnu -march=raptorlake -c`) and link with gcc.
  `flow-rt` needs a standalone `Cargo.toml` (the repo's uses workspace inheritance).
  `~/flowbench` holds the built binaries.
- **vast.ai cannot run perf** — `CAP_PERFMON` dropped, `perf_event_paranoid=4`, `/proc/sys`
  read-only. Its Zen 3 "7×" result is an outlier; do not cite it.
- **`zen3` profile is unvalidated**: −1.4% on Zen 3, +0.5% on i9, both within noise.
- **`oversub` is 1 for `Sliding` reads by design** — conv re-pays its window overlap at every
  slice boundary. Do not "fix" this.
- `kc_nest` stays default-OFF: it loses even past its own derived threshold (K=8192).
- Repo lives on `/Volumes` — after any path move, `cargo clean -p` the CARGO_MANIFEST_DIR-baking
  packages.
- The fma legs are numerically-equal-not-byte-equal BY DESIGN.

## Standing direction (Sapir — unchanged)

- **Compute-only legs; numpy in every verdict table; scale everything up.**
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a flow-ir
  query or emitter-local cashing with zero flow-ir change. flow-ir never learns machine facts.
- **Type system = precision contracts; backend config = performance tailors.**
- **Compile time decides the SIZES, runtime decides the ASSIGNMENT** (plan-s32 §2.5) — the rule
  that settles what belongs in the emitter vs the pool.
- **Nothing goes in the README that a default build does not deliver.**
