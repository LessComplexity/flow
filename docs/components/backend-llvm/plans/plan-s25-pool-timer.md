# Plan — S25 pool floor + llvm compute timer (next-session item 1)

Status: session-directive S25 (Sapir: "continue until 1 + 2 are implemented and tested"); model-first per ADR-0014/0017.
Scope: flow-rt (width), backend-llvm + flow-rt (timer). Zero conformance-path change.

## Why (the measured problem)

S24 box: vast.ai containers expose full host nproc (384 on 2×EPYC-9B14) regardless of
the ≈48-core quota; `available_parallelism` believes it → pool spawns 8× oversubscribed,
≈11 ms spawn floor, and at N=512 the 64 grains (262144/GRAIN=4096) are outnumbered
by 384 workers (steal churn). Separately: flow bench rows are process wall while every
baseline self-times compute — small-N ratios were estimates (s24.md reading 4).

## Categorical model (Dat + Trn)

| Item | Kind | Model |
| --- | --- | --- |
| `configured_threads` | `Trn : Env → ℕ` | today `FLOW_PAR ⊕ available_parallelism`; becomes `FLOW_PAR ⊕ min(available_parallelism, cgroup_quota?)` — `cgroup_quota? : Env → ℕ` is a **partial** morphism (None off-Linux / no limit) |
| `cgroup_quota?` | `Trn` (pure parse) | reads `/sys/fs/cgroup/cpu.max` (v2: `"<quota> <period>"` or `"max"`) else v1 `cpu.cfs_quota_us`/`cpu.cfs_period_us`; value = `ceil(quota/period)`; parse is a total fn on the file text (testable without cgroups) |
| `EmitOpts { perf_timing }` | `Dat` (llvm) | mirror of cuda `EmitOpts` (S20 #19a); `emit()` = `emit_with_opts(default)` — default output byte-identical |
| `flow_perf_begin/end` | `Trm` seam (flow-rt C ABI) | `begin ⊸`: warm the pool (spawn at configured width) then record monotonic start; `end ⊸`: print `FLOW_PERF total ms=%.4f` to stdout (same grammar the cuda legs already parse). Placement: emitted `flow_main` prologue/epilogue, perf mode only |

Composition rule: conformance differential never sets `perf_timing` — the perf face is
bench-only, exactly the fmad split (DESIGN §4 as amended S24b).

## Work packages

- **WP1 (flow-rt):** `cgroup_quota()` (v2 then v1, pure parser + thin fs read) folded
  into `configured_threads`; `FLOW_PAR` still absolute override. Unit tests on the
  parser (strings, not files): `"max 100000"→None`, `"4800000 100000"→48`,
  `"100 100000"→1` (ceil, min 1), garbage→None. One test pinning FLOW_PAR wins.
- **WP2 (flow-rt + backend-llvm):** `flow_perf_begin`/`flow_perf_end` externs (std-only,
  `Instant` in a static); llvm `EmitOpts { perf_timing: bool }` + `emit_with_opts`;
  emitted calls bracket `flow_main` body (after `trap`-free prologue, before epilogue;
  sequential and parallel forms both). `--perf` flag on the `emit` example (cuda parity).
  Golden: one perf-form snapshot; existing goldens byte-identical (opts off).
- **WP3 (bench):** `runner.py` llvm perf legs (`mm_ll_perf_*`) parse `FLOW_PERF total`;
  `regen.sh` emits `_perf.ll` variants alongside (cuda `_perf.cu` parity).

## Acceptance

- `cargo test -p flow-rt -p flow-backend-llvm` green; default emission byte-identical
  (goldens unchanged); differential untouched (never perf mode).
- Local: `FLOW_PERF` row ≈ wall − startup at N=512 (sanity), box: N=16 compute row
  ≈ sub-ms (the floor lives in wall, not compute — s24 open item's done-bar).
