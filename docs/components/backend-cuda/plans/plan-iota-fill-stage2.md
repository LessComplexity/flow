# Plan: ADR-0029 stage 2 — CUDA `Iota`/`Fill` realization

Written: 2026-07-22 (S21) · Status: **SHIPPED** (same day; 161 green)
As-built deviations, both improvements kept: (i) the count is a `long long n` **launch argument**, not a baked literal — equal-shape kernels dedup across different counts under #17 (`iota(4)`/`iota(5)` → ONE kernel; the golden pins it); (ii) elem ctype is **derived from the target type** in both paths (orchestrator review fix — the plan's `(int)i` hardcode would silently truncate the ADR's `[i64; n]` form if it ever lands).
Scope: discharge the 5th `Unsupported` cell — device kernels + arena membership for
`Operation::Iota`/`Fill`. Companion waves in this stage: `widen` (ADR-0029 amendment
§Widen, cross-pipeline) and procedural-v2 bench generators (benches/matmul).

## Categorical model (Dat + Trn)

Stage 1 (shipped) fixed the logical pair: `Iota : Const(n) → [i32; n]`,
`Fill : (x, Const(n)) → [T; n]` — total, trap-free, pure (TrapCaps already
classifies both `false`, kernel.rs:355). Stage 2 is **placement only**: give each
op a `TrnLoc` at the GPU and its output a `DataLoc` in the fn's arena zone.

| Atom | Row | Realization |
| --- | --- | --- |
| `Trn` | `iota_kernel : () → [i32; n]` | `__global__` writing `out[i] = (int)i` |
| `Trn` | `fill_kernel : T → [T; n]` | `__global__` writing `out[i] = x` |
| `Loc` | GPU grid (BC3 elementwise geometry) | same launch shape as Zip/Enumerate |
| `Trm` | launch args | `out` (device ptr), `n` (long long), `x` (by value, Fill only) |
| `DataLoc` | output buffer | arena-zone member via the existing `alloc_buffer` path |

Coherence (§4.5): Law 1 holds with **no input array** — the only delivered inputs
are `n` (emit-time constant, rides as a launch arg like every elementwise count)
and Fill's `x` (host scalar → device by launch-arg `Trm`). No trap param, no
bounds guards (nothing indexes beyond the `i < n` grid guard). Law 2: `carries`
is exactly `{out, n[, x]}`.

## Kernel shapes (the contract)

```cuda
__global__ void k_iota(int* out, long long n) {
  long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < n) out[i] = (int)i;
}
__global__ void k_fill(T* out, long long n, T x) {   // T = element ctype
  long long i = ...same...;
  if (i < n) out[i] = x;
}
```

- 64-bit indexing, BC3 geometry, no trap param (S20c trap-free class).
- #17 kernel-shape dedup applies unchanged (two iotas of equal shape → one kernel).
- Fill's `x` feeder is an ordinary host expression (Constant or computed) passed
  by value — never a device buffer.

## Where the code goes (pointers for the implementer)

| Site | Today | Change |
| --- | --- | --- |
| `func.rs:729` (`Operation::Iota \| Fill` host arm) | `EmitError::Unsupported` | join the bulk-site family (the `func.rs:748` arm): registered kernel site + `emit_bulk_site`-style launch; no trap plumbing |
| `kernel.rs` site registration / kernel emission (`Enumerate` rows: 144, 392, 562, 957, 1656) | Iota/Fill absent | add arms; `iota_kernel`/`fill_kernel` siblings of `enumerate_kernel` (:957); elementwise prologue (:1092) minus the input array |
| `kernel.rs:2069` (DevEmit twin arm) | `Unsupported` | device-local realization: `for` loop writing the local buffer, sibling of `emit_enumerate` (:2078) |
| arena (`arena.rs`, `buffer_bytes_of`) | n/a | outputs are ordinary zone members — verify capacity accounting picks them up, no special case |

Launch-count source: stage-1 deviation (i)/(ii) — `n` is the `Constant` source
object (Iota) / internal 2-tuple slot-1 `Constant` (Fill). Read it the same way
llvm's `emit_iota`/`emit_fill` (backend-llvm func.rs) already do.

## Tests (done when all green)

1. lib unit tests: kernel text for iota/fill (both paths: host site + twin body).
2. golden `.cu` snapshot: a program with `iota` + `fill` + a consuming `map`
   (exercises dedup + arena rows; pin arena capacity includes the new buffers).
3. gate: `arena_gates_plan_section_7`-style pin extended or sibling pin.
4. differential rows: iota/fill programs join the local harness
   (skip-with-reason off-box, as today) — run remotely in the S21 box leg.
5. `Unsupported` cell count drops 5 → 4 in STATUS/capability matrix (reconcile).

## Constraints

- No `git` commands; no edits to `docs/STATUS.md` / `docs/sessions/`.
- Default emission stays byte-identical for programs without iota/fill
  (golden_cu snapshots must not churn).
- Honest failure: any shape this plan doesn't cover (e.g. iota inside a
  loop-cone carried position hitting arena v1.0's per-buffer path) falls back to
  the existing allocation path, not a new special case.
