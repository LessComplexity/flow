# Plan: smart arena allocation (backend-cuda) — "capacity from the graph, one arena per zone, var = base + offset"

Status: **shipped (v1.0), 2026-07-21 wave** (suggestions #18 DISCHARGED; v1.1 = suggestions #18a, **scope updated 2026-07-22**: the last-use wave landed the query's other two consumers — the cone Update site no longer allocates where rule 4 proves the carried source dead (its offset stays reserved-but-unused, the recorded v1 simplification) and map-class carried buffers free at the back edge; what remains in v1.1 is coloring (capacity → max-clique, reclaiming those unused offsets) and loop-cone zones for the carried shapes that still allocate per iteration) · deviations: (i) §7's **fir row lands at 3/3** excl d_trap (4/4 with), not ≤3/≤2 — BOTH of fir's Index readback cells are advance-cone sites, so §5's own recorded scoping (cone sites stay per-buffer) bounds it; the §7 cell was calibrated without noticing, and even with the cells zoned the frees would be 3 (two fn zones + d_trap); (ii) §7's matmul "before" of 12 was a pre-W1 measure — the W1 text was 8 + d_trap, the after is 1/1 excl d_trap (the ≤3/≤2 gates hold); (iii) the §8 `static_assert` drift guard is emitted for **every** `FlowProd_*` struct (uniform), not only arena-used ones; (iv) §7's **micro_loop_update row lands at 2/2** excl d_trap since the 2026-07-22 last-use wave (was 3/3 — the in-place Update killed the cone site's malloc+free; the offset stays reserved-but-unused). · Written: 2026-07-21 · Session 20, on Sapir's S18/S19 spec (suggestions #18, elaborated S19 as the S19 P0) and the optimization-marathon goal (match/beat native CUDA).
Evidence: `docs/notes/bench-matmul.md` (per-launch/sync pricing), suggestion #18 (each `cudaMalloc` ~10–20 µs and can sync), the Futhark memory-block-merging algorithm (Munksgaard PhD 2023, ch. static analyses: last-use table → interference graph → greedy coloring → allocation hoisting), HVM2's one-giant-arena existence proof (`gnet_create`: the whole heap is one `cudaMalloc`).
Scope: **host-side device allocation in `flow-backend-cuda` only.** Twins allocate nothing (per-thread locals, F7-budgeted); `d_trap` stays a separate process-global allocation (carve-out, §4). Language semantics, the oracle, and the R1 differential contract untouched — this is a representation change in the functor's image (ADR-0020 §3).

## 1. Why (one paragraph)

Today every buffer construction site emits its own `cudaMalloc` (`FnEmit::malloc_buffer`, func.rs:674 — 9 call families) and every fn exit emits one guarded `cudaFree` per buffer (`emit_frees`, func.rs:260). Each `cudaMalloc` costs ~10–20 µs and can synchronize; in loop cones the malloc executes **per iteration** (the O(k·n) case, loops.rs:37-38). All array sizes in Core are static `u64`s (`Ty::Array{size}`), so the capacity of every buffer is a compile-time number: the allocation problem is not a runtime problem at all, it is a **deduced morphism of the execution graph** — exactly the Futhark result (compile-time block merging) and Sapir's directive (a)–(d). Modeling it as such collapses N mallocs + N frees per fn into 1 malloc + ≤1 free per zone, makes device addresses compile-time constants (which later enables CUDA-Graph capture — research item #5), and deletes a whole class of per-iteration API traffic.

## 2. The categorical model

New deduced query, same shape as `loop_plan` (BL7 — computed from the sealed graph, never stored):

```
arena_plan : CategoryIr × FuncId → ArenaPlan        (deduced, total, deterministic — L2)
ArenaPlan  = { zones : Zone*, assign : ObjectId →? (Zone, Offset), capacity : Zone → ℕ }
Zone       = FnScope | LoopCone(merge)               (v1.0: FnScope only; LoopCone is v1.1, §6)
```

Objects and morphisms (the `Dat` olog of the change):

| Object | Meaning |
| --- | --- |
| `Buffer` | a device-resident array object (bulk-op target, array-typed literal, array-elem Index/Fold result, 1-cell readback temp) |
| `Zone` | a lifetime class of buffers; one `cudaMalloc` per zone per fn invocation |
| `ArenaPlan` | the deduced assignment `Buffer → (Zone, Offset)` + per-zone capacity |
| `EscapeSet` | the pointer values that may escape the fn (`escape_lvalues`, func.rs:1015 — existing) |

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `arena_plan` | `IR × FuncId → ArenaPlan` | Deduced | the whole plan — a pure function of the sealed graph |
| `assign` | `Buffer →? (Zone, Offset)` | Partial | defined on every fn-owned constructed buffer; absent for params/call results (borrowed, never registered) |
| `capacity` | `Zone → ℕ` | Deduced | `Σ align256(sizeof(b))` over the zone's buffers — compile-time bytes |
| `zone_of` | `Buffer →? Zone` | Deduced | `fst ∘ assign` |
| `escapes` | `FuncId → EscapeSet` | Deduced | existing `escape_lvalues`; which pointer values may outlive the fn |
| `in_zone` | `EscapeSet × Zone → 𝔹` | Deduced | pointer-range test: an escaped value's address lies in a zone's `[base, base+capacity)` |

Composition rules (the invariants the implementation must preserve):

1. **No overlap.** `assign(b₁) = (z, o₁) ∧ assign(b₂) = (z, o₂) ∧ b₁ ≠ b₂ ⟹ [o₁, o₁+size(b₁)) ∩ [o₂, o₂+size(b₂)) = ∅`. (Futhark step 3's coloring; in v1.0 every fn-scope buffer is live simultaneously at fn granularity, so coloring degenerates to a disjoint layout — interference refinement is v1.1 with last-use, §6.)
2. **Alignment.** Every offset is a multiple of 256 B and the arena base is `cudaMalloc`-aligned (256 B) — every buffer address stays 256 B-aligned, so the epilogue guard's pointer-value comparisons remain unique per buffer (no two buffers share an address).
3. **Pointer honesty (L1/escape guard).** A zone's release at fn exit is vetoed iff `in_zone(escapes(f), z)` — the guard keeps comparing pointer values, exactly as today (func.rs:260-286), but the unit is the zone: an escaping buffer pins its whole zone (caller inherits the bounded-leak duty, today's DESIGN §2 amendment (ii) shape).
4. **Guard honesty (max cap).** `capacity(z) > ARENA_MAX_BYTES ⟹ EmitError::Unsupported` at emit time (the F7 `check_local_array_budget` precedent, kernel.rs:517). Runtime allocation failure stays the `cu_check` exit-102 channel. `ARENA_MAX_BYTES` is a recorded constant (§5), not device-derived — compile-time honesty, no device query.
5. **Borrowed buffers have no assignment.** Parameters and call results are never in `assign`'s domain (today: never registered) — no zone ever frees borrowed memory. `d_trap` is outside the model entirely (§4).
6. **Determinism (L2).** `arena_plan` is a pure function of the sealed graph + recorded constants — same IR, same byte-identical text.

What this is NOT (consolidation check, §3 of FRAMEWORK): no new runtime object — the registry (`FnEmit::allocs`, `slots`) is the same object with richer morphisms (offset/zone metadata instead of name-only). No second allocation path: `malloc_buffer` remains the single choke point; it becomes "bump/assign within the pre-allocated zone" rather than "call cudaMalloc".

## 3. The emission change (functor image, per fn)

```
// before (per site):                      // after (once per fn, plus per-site pointer init):
cu_check(cudaMalloc((void**)&o8, B8), …);  cu_check(cudaMalloc((void**)&arena0, CAP), "cudaMalloc(arena0)");
…                                          o8 = (double*)(arena0 + OFF_o8);   // OFF_* = aligned literal constants
// before (per buffer at exit):            // after (per zone at exit):
if (o8 != o1) cu_check(cudaFree(o8), …);   if (!escaped0) cu_check(cudaFree(arena0), …);
                                           // escaped0 = disjunction of escape_lvalues range tests
```

- **Offsets as literal constants.** Byte sizes must be **ABI-exact** numerics in Rust: new `kernel::abi_sizeof(ty) -> Option<u64>` implementing C layout rules for `FlowProd_*` structs (component alignment + tail padding — `nominal_sizeof`, kernel.rs:488, explicitly undercounts and stays the budget measure; `abi_sizeof` is the arena measure, unit-tested against static asserts in emitted text where feasible). `capacity = Σ align256(abi_sizeof(elem) * flat_count)`.
- **`slots` resolution.** `FnEmit::slots` (func.rs:65) continues to map object → lvalue text; for arena buffers the hoisted declaration stays a `{ct}* o{n}` local, initialized once from `arena_base + OFF` at the point where today `malloc_buffer` runs (loop-cone sites included — re-initializing the pointer per iteration is free and keeps zero-iteration `nullptr` semantics).
- **Escape guard.** `escaped{z}` is emitted as a boolean: `o8 >= (char*)arena0 && o8 < (char*)arena0 + CAP` per escape lvalue in the zone, short-circuit; the free is `if (!escaped0)`. Same comparison class as today (pointer values), same exemption semantics (rule 3).
- **The 1-cell readback temps** (index/fold `t{n}`) are ordinary buffers in the plan — they join the fn zone.

## 4. What does NOT change

- `d_trap`: separate process-global malloc/memset/free in the prelude (module.rs:60-90), threaded by literal name into every launch. Not a zone member; excluded from capacity and count contracts (counts are reported with and without it, per the harness convention).
- Trap design (kind+1, check-after-every-launch), BC1 residency (zero whole-array D→H), BC5 (Update full copy — **amended 2026-07-22: in place where the last-use plan proves the source dead; an in-placed site's offset stays reserved-but-unused — zones are not recomputed**), BC11, the width rule, `-fmad=false`, BC8/Twin qualifier machinery, the L3 `loop_plan` ceiling, the three `Unsupported` cells, exit-102 protocol, the oracle, R1.
- Twins: zero device allocation today, zero after.
- Call-boundary ownership conventions (params borrowed; caller-frees unmechanized, DESIGN §2 (ii)) — arenas inherit them verbatim via rule 3.

## 5. Recorded constants & decisions (marathon mandate: best option takes the case)

| Decision | Choice | Why |
| --- | --- | --- |
| `ARENA_MAX_BYTES` | **4 GiB per zone** (compile-time guard) | covers the bench family by 100×+; a program exceeding it is genuinely unsupported-scale today; honest `EmitError`, revisit with ADR-0023 dynamic sizes |
| Zone granularity v1.0 | **one FnScope zone per fn** | region/loop zones need last-use + trip counts (runtime) — v1.1, designed with `region_plan` (the two analyses are cousins, next-session.md item 4) |
| Loop-cone sites v1.0 | **stay per-buffer cudaMalloc** (status quo, still counted) | per-iteration capacity is not statically bounded (runtime trip counts); the honest scoping — capture-form matmul (the fast path) has **zero** cone sites |
| Alignment | 256 B (cudaMalloc's own guarantee, preserved per-buffer) | rule 2 |
| `d_trap` | excluded from arenas | §4 |
| Size model | new ABI-exact `abi_sizeof` (C layout rules), tested | `nominal_sizeof` undercounts padded products (kernel.rs:494-505 doc) |
| Guard exit classes | emit-time ⇒ `EmitError::Unsupported` (emit example exit 1); runtime ⇒ `cu_check` exit-102 | agent-2 §4 naming; deliberate, recorded |

## 6. Sequencing

1. `abi_sizeof` + alignment arithmetic + unit tests (kernel.rs; pure).
2. `ArenaPlan` computation in the backend (fn-scope buffer enumeration reuses `kernel::collect_sites` + `literal_pair_counts` + the readback-temp sites; assignment walk over topo order for determinism).
3. Emission swap in `FnEmit`: fn-entry arena malloc, per-site pointer init, zone-release epilogue with range-test veto; registry gains zone/offset metadata.
4. Structural perf gates (below) + 13 insta snapshots re-pinned + the escape-guard text pins re-pointed at the range-test shape.
5. Box measurement leg (with the marathon sweep): malloc-count is the structural proof; wall deltas recorded (capture form is startup-bound — kernel-time instrumentation (#19a) lands in the same wave and separates the kinds).
6. **v1.1 (recorded, not built here; scope updated 2026-07-22):** the last-use analysis in flow-ir **shipped** (plan-last-use §2) and its other two consumers landed with it — in-place `Update` (the cone Update site allocates nothing where rule 4 proves the carried source dead; its zone offset stays reserved-but-unused) and back-edge freeing (suggestion #2 — map-class carried buffers release per iteration, residency solved). What remains here: (a) **interference-graph coloring** to merge non-overlapping buffers (the Futhark step-3 refinement — capacity becomes max-clique, not sum; it also reclaims the in-placed sites' unused offsets); (b) **loop-cone zones with two-slot rotation** for the carried shapes that still allocate per iteration (borrowed/escape-vetoed updates, map-in-loop producers) — kills their per-iteration malloc traffic.

## 7. Perf contract (structural, CI-safe — counted on emitted text, deterministic by L2)

Static text counts, asserted in `crates/backends/cuda/tests/golden_cu.rs` (idioms per agent-2 §5: whole-module `matches().count()` + fn-slice counts + per-line filter pins). **Before** = current snapshots (measured by agent-2); **After** = this plan's gates. Counts exclude `d_trap` (noted +1 where relevant).

| program | `cudaMalloc` before → after | `cudaFree` before → after | notes |
| --- | --- | --- | --- |
| one-kernel matmul (inline source, golden_cu.rs:380) | 12 → **≤3** (fn zone + 0 cone sites; readback cells join zone) | 12 → **≤2** | the marathon's flagship shape |
| `vector_add` example | 8 → **≤3** | 8 → **≤2** | two fns → per-fn zones |
| `fir` example | 5 → **≤3** | 5 → **≤2** | |
| `micro_loop_update` (inline source, golden_cu.rs:118) | 4 → 4 (unchanged: cone sites stay per-buffer in v1.0) | 4 → 4 | records the v1.1 debt honestly — **superseded 2026-07-22: 4 → 3 (2/2 excl d_trap), the in-place Update killed the cone site's malloc+free; its offset stays reserved-but-unused** |
| escape-guard pins (golden_cu.rs:238-251) | guard text re-pinned to range-test veto shape | | rule 3 preserved |

Plus: every emitted `o{n} = nullptr;`-initialized array local that receives a zone pointer compiles clean; zero-iteration loops leave the pointer `nullptr` (no site executed → no init → no use, same as today).

Measured rows (box, marathon sweep): malloc/API-call **dynamic** counts (instrumented or nsys) and wall deltas go in `docs/performance/arena.md` (new file per the one-file-per-benchmark convention + README index row); matmul.md gets arena rows only under kernel-time instrumentation — never mix process-wall with per-iteration kinds (the S19 gotcha).

## 8. Risks / honest unknowns

- **ABI-size drift** between `abi_sizeof` and nvcc's `sizeof` for nested-product elements ⟹ mitigated by rule 1's disjoint layout (offsets from the same model) + 256 B alignment slop; a static_assert emission per product type is the belt-and-suspenders check (emitted, compiled on the box leg).
- **Escape-set range tests** add O(|escapes|) compares per zone release — trivial (≤ a handful).
- **Fn-scope over-reservation**: a fn with many conditionally-executed sites reserves for all (today: mallocs only executed ones). Static capacity vs dynamic frugality — recorded tradeoff; v1.1's last-use coloring shrinks it. Zero-iteration loops cost one arena malloc per fn call regardless (bounded, honest).
- **Golden churn**: 13 snapshots + text pins re-pinned in the same change (reconcile discipline §6.3).
- **Interaction with kernel-dedup (#17) and instrumentation (#19a)** landing in adjacent waves: disjoint code (kernel registry vs allocation; launch wrappers vs mallocs) — sequenced waves, tests green after each.
