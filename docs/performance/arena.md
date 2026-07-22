# Arena allocation (backend-cuda, v1.0) — structural perf contract + measured notes

Shipped 2026-07-21 (suggestions #18, `docs/components/backend-cuda/plans/plan-smart-arenas.md` — status: shipped v1.0). One arena `cudaMalloc` per fn-scope zone (capacity deduced from the graph, `abi_sizeof` C-layout-exact, 256 B-aligned offsets, 4 GiB `EmitError` guard); per-site `arena0 + OFF` pointer inits; one zone release at fn exit under the pointer-range escape veto. Loop-cone sites keep per-buffer `cudaMalloc` (v1.1 debt, then partially discharged by W3's in-place `Update` — see the last-use row).

## Structural counts (static text, deterministic — asserted in `tests/golden_cu.rs::arena_gates_plan_section_7`)

| program | `cudaMalloc` before → after | `cudaFree` before → after | note |
|---|---|---|---|
| one-kernel matmul (capture) | 8 → **1** | 8 → **1** | 2,048 B zone, 8 members, zero per-buffer mallocs |
| vector_add | 7 → **1** | 7 → **1** | |
| fir | 4 → **3** | 4 → **3** | both readback cells are advance-cone sites (v1.0 scoping) |
| micro_loop_update | 3 → **2** | 3 → **2** | W3 in-place `Update` killed the per-iteration malloc (plan §7's 3/3 superseded) |

(d_trap is excluded everywhere — a separate process-global allocation.)

## Measured notes

- Process wall on the capture form is **unchanged** (313–328 ms flat at N=16–256) — startup-bound (~270 ms context init); ~10 × 10–20 µs of malloc latency is below the floor. The arena's payoff is API-call count, fragmentation, and the compile-time-address property (enables CUDA-Graph capture later), not wall time at this scale.
- Kernel-time rows (FLOW_PERF) showed no regression: 0.193–0.223 ms totals at N=16–256, both widths (see `matmul.md`).
- Remote differential (S20, box B): 144 green on the 4090 under the arena'd emitter — 640 compile-and-runs, zero divergences; escape-guard range-veto shape re-pinned.
- v1.1 remaining: last-use interference coloring (merge non-overlapping buffers — `death` intervals from `flow_ir::last_use_plan`, shipped W3) + loop-cone zones for the still-allocating carried class.
