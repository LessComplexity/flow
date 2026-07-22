# Plan: region-based emission (backend v2) — "strip, then partition, then one kernel per region"

Status: plan (model-first, HANDOFF §7.1.5) · Written: 2026-07-21 · Session 16, on Sapir's two directives:
(a) "not fusion — correct mapping": emission granularity is the *region*, not the IR morphism; (b) "strip functions to the smallest primitive graph": functions are a human modularity construct — the optimizer's unit is the flattened primitive dataflow graph.
Evidence: `docs/notes/bench-matmul.md` (the per-op launch wall measured: ~24 µs × Θ(N³) launches; the same algorithm as one kernel is 3.8M× faster at N=64).
Scope: backend line (CUDA v2 now; Verilog P7 designed in from the start). Language semantics untouched; R1 differential contract untouched (ADR-0020 §3 — the functor's *image* changes shape, not meaning).

**The architecture (Sapir's pipeline, S17):** `Flow → Cat IR (the primitive execution graph) → zoning/combining → backend → runtime`. The source is the *algorithm*; the stripped graph is its deepest presentation — functions inline, named bindings dissolve, and only **meaning** survives (effects/token, loops, types are semantics, not human constructs; readability boundaries become span metadata for diagnostics, never structure). Zoning/combining then computes the *machine*-optimal consolidation from the graph — **what is human-readable is not necessarily optimal; the optimal separation is graph-computed** (the algorithm/schedule separation). Two zoning decisions, kept separate by kind: the **legal** zones (semantic — computed once, in the IR, shared by all targets) and the **optimal** zones (cost — refined per backend: CUDA kernels, Verilog stage-sets, ASIC dataflow blocks). R1 is what makes "optimal" a cost question rather than a correctness question: every zoning of the same graph must produce the same observable behavior.

## 1. Why the current mapping is wrong for op-rich shapes (the honest review of v1)

The M3/v1 emitter was the correctness-first scaffold: every array-bulk **morphism** (`Map/Zip/Enumerate/Fold/Index/Update`) is one launch + (for host-consumed results) one D→H readback; scalars are host C++; the residency rule (scalars host, arrays device) is applied **per op**. Two consequences the bench priced:

- `Index` in a loop (the matmul's `cell` fn) costs a launch + sync + readback *per element read* (~24 µs × 2N per cell × N² cells). The correct mapping computes the whole dot product on-device and crosses **once** with the scalar result.
- Elementwise chains (`map → zip → enumerate`) allocate and launch per op; the correct mapping composes them in registers in one kernel and materializes only the region's live outputs.

The v1 structure is not *unsound* — it is the M3 contract, differential-green — it is the wrong **default granularity** (the DESIGN §3 sync-per-launch price paid at the finest possible grain). The fix is architectural, and Sapir named it: regions, not instructions; and the region pass must see the **primitive** graph, not the fn-partitioned presentation.

## 2. The three moves

### Move 1 — Strip: inline calls to the primitive graph (a rewrite-layer pass)

Functions exist for humans. Before region formation, strip `Call` morphisms by **graph substitution**: a `Call(g)` with source aggregate `S` and target `T` is replaced by `g`'s body with `g.input ↦ S`, `g.output ↦ T` (the callee's objects get fresh ids, deterministically ordered; the callee's own calls recurse). The result is one flattened dataflow graph per entry fn — the "smallest primitive graph representation."

- **Where it lives:** the **rewrite layer** (a new plan+replay pass `inline`), not the backends — so (i) it is R1-property-tested like every other pass, (ii) every backend inherits it for free, (iii) the existing differential (raw **and rewritten**) automatically tests region-mapped programs against the oracle. Stripping is semantics-preserving by construction (substitution of equals); R1 makes that a theorem under test, not an argument.
- **Cost model (strip is not free):** duplication — a callee called at K sites is copied K times. Policy v2.0: inline a call site iff (callee morphism count ≤ `INLINE_MAX_BODY`, start 64) ∧ (callee is not the entry) ∧ (no `Call` cycle — the call graph is a DAG today; if recursion ever arrives, cycles stay as calls, see §7). Sites kept as calls remain region boundaries (the callee is its own region graph). `INLINE_MAX_BODY` is a recorded constant, tuned by the perf harness, never semantics-bearing.
- **Order dependence:** none — strip-all-then-partition is confluent for a DAG (the result is the fully elaborated graph); the perf harness asserts the launch-count deltas per example.
- **Body fns are graphs too (Sapir, S17):** a `map`/`zip`/`enumerate` body is a *parameterized fanout subgraph* (one body graph quantified over n) — strip, partition, and legality analyses descend into it uniformly with the outer graph; the fanout is **never** elaborated to n physical nodes (sepia's 65,536 would be the W2 wall as a graph). `fold` descends the same way but is a dependence *chain*, not a fanout — parallel-vs-series is read off the graph's path structure, not the op name. Capture legality is likewise a graph property (ADR-0027 D2b): no path from a fanout's output back into a captured binding, and no captured name as a rebind target inside a body.

### Move 2 — Partition: region formation on the primitive graph

**Two levels (Sapir, S17):** the Category IR itself owns the *maximal legal* partition — `CategoryIr::region_plan(f)`, a deduced query like `loop_plan` (BL7), computing the maximal effect-free, loop-free components once for all consumers. Each backend then *refines* within those legal regions per its own cost model (merge what launch price favors, split what register pressure forces) — the cost model never defines regions, only prunes or coalesces within them.

A **region** (the IR-level, legal one) is a maximal connected set of morphisms with no semantic boundary inside. On the flattened graph, the semantic boundary rules are:

**Boundary rules (an op starts a new region / stays host):**
1. **Effects** — `Print` (host, L4) and the token thread: always host; cuts the graph.
2. **Live-out values are ports, not boundaries** — a value leaving a region (a scalar consumed by an effect/guard/another region) crosses at the region's end, **once** — realization per backend (CUDA: one D→H readback per live-out scalar, never per op — this is the matmul fix). A live-out edge never splits a region; it defines its output arity.
3. **Loop structure** — a canonical loop is a region *scope*: its decide/advance cones are regions (the `loop_plan` cone machinery generalized); the merge/back edge is the boundary (carried arrays stay device-resident across iterations — buffers persist, no crossing; carried scalars live in host or device per the cost model). Loops proven **independent-iteration** (the `par` case — see the companion ADR track) collapse to a single elementwise region: the whole loop becomes one kernel.
4. **Kept calls** (unstripped by the cost model) — region boundaries by construction.
5. **Launch-forbidden ops on device** — none at the primitive level: every Core op has a device realization (L3); what changes is that `Index`/`Update` *inside* a region are plain global reads/writes (guarded, trap-flag semantics unchanged).

**Determinism (L2):** the partition is a pure function of the sealed graph + the cost model constants — same IR, same regions, byte-identical text.

**CUDA cost model (v2.0):** launch+sync price ≈ 24 µs (measured, S16); D→H/H→D per-byte price; per-region intermediate buffer allocation. Regions merge while (combined kernel count × launch price + transfer price) decreases; elementwise-compatible neighbours (map/zip/enumerate feeding each other) always merge (register composition, no intermediate buffer unless the value has a second consumer — then it materializes once).

**Verilog cost model (P7, designed-in):** a region is a clocked pipeline/FSM stage-set; boundaries are the E1 restriction's own (feedforward = one streaming region; a single loop = one FSM region). The same partition algorithm with a different price vector — one reason to build the machinery once (§5).

### Move 3 — Emit: one kernel per region (host residuals on the host)

- **Elementwise regions** (map/zip/enumerate compositions): one kernel, ops composed per-thread in registers; inputs are the region's entry handles; outputs are the region's live-out values (buffers materialized once, only if consumed outside).
- **Scalar-producing regions** (the `cell` shape: index/guard/arithmetic over device arrays): one kernel (typically `<<<1,1>>>` today — parallel reduction is the `reduce` ADR's business); the result scalar(s) read back once at region end.
- **Fold regions**: the single-thread kernel (BC4) until the canonical-tree `reduce` ADR lands; then tree reductions with oracle-pinned order.
- **Traps (§3 of the backend DESIGN survives, re-grained):** one flag check **per region launch**, not per op. Class parity + first-trap-wins are preserved: inside a region, ops execute in oracle topo order per thread (guards store and unwind exactly as today's twin code does — the fold kernel's per-step `if (*trap) return;` pattern generalized to the region walk). Fewer launches = strictly less sync than v1, never more.
- **Buffers/ownership:** the allocation registry and the epilogue pointer-value escape guard (S15) transfer unchanged; region-intermediate buffers are registry entries like any other (last-use freeing becomes *more* valuable inside regions — follow-on, recorded).
- **`Update` in a region:** functional semantics at region granularity — the region allocates the fresh output buffer and fills it (naive-copy today; last-use in-place is the recorded optimization).

## 3. The matmul, end to end (the acceptance example)

Raw: two fns (`matmul` outer loop t, `cell` inner loop k with 2 `Index` + mul/add; `c[t] <- v` per t).

1. **Strip:** `cell` inlines into `matmul`'s loop body (one primitive graph: outer loop t, inner loop k, two array reads, FMA, one update).
2. **Partition (today's semantics, sequential loops):** the inner-k loop over device arrays with a scalar live-out = one region per cell call → **N² launches** (vs v1's 2N³+N²): at N=64, 4096 launches ≈ **0.1–0.2 s** (vs 12.16 s) — a ~100× win from mapping alone, semantics untouched.
3. **Partition + independence (the `par` track):** the outer loop is independent-iteration (each `c[t]` written once, no cross-iteration reads) → the whole matmul is one elementwise region with an inner per-thread k-loop → **one kernel** ≈ naive-CUDA territory (0.0032 ms at N=64) — the S16 bench's control number, reached from the Flow source.
4. The differential (existing, unchanged) is the acceptance test: raw and stripped-and-region-mapped IR must both stay oracle-equal — that is what R1 *is for*.

## 4. What does NOT change

- The `emit(&CategoryIr) -> Result<String, EmitError>` contract, `flow-rt`, the trap flag + kind+1 encoding, the exit-102 infra protocol, `-fmad=false`, the width rule, the qualifier analysis (HostDevice/Twin still governs kept-call device visibility), the L3 `loop_plan` ceiling, the three recorded `Unsupported` cells.
- The oracle, the language, the rewrite laws (inline is additive). The v1 emitter remains in-tree as the reference until v2's differential matches it on the full corpus (then v1 retires or stays as a debug flag — decided at the v2 gate).

## 5. Where the machinery lives (shared, once — the two-level structure, Sapir S17)

Region knowledge is split by *kind*, not by backend:

- **Level 1 — `flow-ir` owns the maximal legal partition (the semantic fact).** A new query `CategoryIr::region_plan(f) -> RegionPlan` (deduced on demand from the sealed graph, never stored — the BL7 `loop_plan` pattern exactly): the maximal effect-free, loop-free components of the flattened graph. Boundary rules are semantic only: the token thread + `Print` cut (E2/L4); loop scopes cut (guard-first; the loop's decide/advance cones **are** regions — the query generalizes the cone machinery `loop_plan` already computes); entry/output are ports. Deterministic, total, property-testable in one place. Every backend must agree on this partition — it is semantics, not cost.
- **Level 2 — each backend refines and realizes (the cost fact).** A backend consumes the `RegionPlan` and, within legal regions, applies its cost model: CUDA merges elementwise-compatible regions into one kernel (launch/sync price) and picks residency + readback points (host/device); Verilog realizes a region as a pipeline/FSM stage-set (E1's feedforward/single-loop restriction prunes); LLVM treats a region as straight-line code (the degenerate case — calls are cheap). **The cost model refines the legal partition, never defines it.**
- **`flow-rewrite`:** the `inline` pass (strip) + its R1 property pins (determinism, idempotence, oracle equality over testgen) — runs before Level 1, since the partition sees the flattened graph.
- **interp (optional, later):** may consume the same plan for a fused-eval fast path — never a second definition of regions.

## 6. Perf contract (Sapir's per-step gate, first application)

Structural, CI-safe assertions (deterministic, machine-independent), added to `golden_cu`/`golden_ll`-class tests per example shape:
- **launch counts**: `vector_add` = 1 elementwise kernel + 0 readbacks; `matmul64` ≤ N² launches today, = 1 under `par`; any fused map chain = 1.
- **transfer counts**: zero whole-array D→H (existing theorem, now also asserted per region plan); scalar readbacks ≤ region live-out count.
- **measured baselines** (informational, box-pinned): the S16 matmul row re-run at each step (target: N=64 ≤ 0.3 s after move 2; ≤ 0.05 s with `par`).

## 7. Risks / honest unknowns

- **Compile-time/text blowup from stripping** (pathological call fan): the cost-model cap exists for this; the perf harness watches emission time + text size.
- **Region-intermediate liveness** (a region value with two external consumers): materialize once, first consumer's region owns it; the escape guard (S15) already reasons by pointer value — reuse it.
- **Trap-order subtleties in wide regions** (an elementwise region whose body can trap in two different ops): first-trap-in-oracle-order per thread is preserved by the region walk's topo order + per-op store-and-return; cross-thread kind races remain R1-unobservable (the S15 race note).
- **Recursion, if it ever arrives:** strip stops at cycles (they stay calls/regions); noted, out of scope today.
- **Register pressure in giant elementwise regions:** nvcc spills (correct, slow); the perf harness is the evidence; splitting by pressure is a later cost-model row, not v2.0.

## 8. Sequencing

1. `inline` pass in flow-rewrite + R1 pins (small; unblocks everything).
2. `regions.rs` partition + the CUDA cost model + structural perf gates.
3. CUDA v2 emitter per region (the matmul acceptance: 12.16 s → ≤0.3 s at N=64; then 1-kernel with `par`).
4. P7 Verilog DESIGN with the same partition (region = pipeline/FSM stage-set).
5. Retire-or-flag v1 at the v2 differential-equivalence gate.
6. **Emitter-quality follow-ons (Sapir's S18 read of `matmul_cap.cu`, suggestions #12–18):** kernel shape dedup (one `__global__` per shape, not per site — the consolidation question), arena allocation (one `cudaMalloc` per fn + offsets, replacing per-buffer mallocs — the registry becomes offset bookkeeping), copy-propagation at IR level (the re-pack chains — see §7), guard elision for provably-constant divisors, trap-param trimming on trap-free kernels, per-iteration invariant hoisting. Each is a measured perf-contract row; none changes semantics.
