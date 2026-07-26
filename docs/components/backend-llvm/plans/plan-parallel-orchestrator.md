# Plan: parallel orchestrator — "the execution graph's own paths, scheduled on each platform's best concurrency layout"

Status: **SHIPPED v1 (code) 2026-07-23 — S24 same-day: mapal-ir `path_plan` + mapal-rt scheduler + backend-llvm parallel `mapal_main`, workspace green, R-PAR pinned live; the bench leg (work item 5) is the open tail.** Deviations from the ratified text: trap protocol upgraded in-flight to v3 speculate-and-order (§2 rule 3 — the abandon-and-skip draft had deadlock/print-order corners); wait entries carry watermark thresholds; pinning is registration-time (`mapal_par_pin` — review find: run-time pinning raced the pool); checkpoints inside effectful loops also hoist before `LoopEnter` (review find). v2 RATIFIED by Sapir 2026-07-23 (S24, in-session) — implementation go. Scheduler amendment at ratification: static schedule first (compile-time ranks seed the workers), work stealing as the runtime backstop. v1 (same day) proposed per-bulk-site loops only; superseded in-session by Sapir's correction: *the orchestrator is GIVEN the parallelizable parts from the execution graph; paths are the unit; each backend maps paths to its own concurrency layout.* · Written: 2026-07-23 · Session 24.
Evidence: S23 matrix — flow-llvm single-thread 5.9 s @1024 f32, multicore CPU class ~60× away at N=512; Category-IR is already the parallelism oracle (edge-only dataflow: two nodes with no path between them are independent **by construction**; effects ride ONE linear token chain — ADR-0013); the deduced-query pattern is established (`loop_plan`, `last_use_plan`, `bounds_proof`, `emission_plan`); the cuda trap-flag protocol is the working template for "worker fails, host observes at the right point".
Scope: one new deduced query in `mapal-ir` (backend-independent), a small scheduler runtime in `mapal-rt`, and `backend-llvm` emission that consumes both. Zero language/IR-op/oracle change. cuda/verilog consume the same query in their own later waves (§3) — nothing there changes now.

## 0. The main idea (the invariant this plan must not diverge from)

Mapal is parallel-first: the execution graph separates into **execution paths**; each path is its own concurrent unit; the backend maps paths onto whatever its platform runs concurrently — CUDA blocks/streams, FPGA/ASIC regions, CPU threads. The graph's divergence is known at **compile time**, so each backend gets a statically-known schedule and builds the best runtime for it — for a CPU with 12 logical cores: 12 threads, each executing a path, the next path waiting in line.

The compiler's side of that bargain is a **deduced query**: the parallel structure is read off the sealed graph and handed to every backend — never re-derived ad hoc inside one emitter, never stored (FRAMEWORK §5 deduce-don't-store).

## 1. Why (one paragraph)

The legality of running paths concurrently is already a theorem of the language: bulk-op bodies and fanout branches carry no token (effect-free — lower's signature synthesis + check's E2 pass), captures are read-only (ADR-0027), every value has one producer (edge-only dataflow), and effects form one linear token chain. The cuda backend cashes this per bulk op (kernels); nothing today cashes the **graph-level** independence — two independent array fills execute back-to-back on every backend. This plan adds the missing structure at the right layer: `mapal-ir` deduces the path/task DAG once, backend-independently; `flow-llvm` + `mapal-rt` are its first consumer (CPU threads); cuda streams and Verilog spatial regions are later consumers of the *same* query. What was v1's per-site loop splitting survives only as a detail: a bulk op inside a path splits into element-range slices — the data-parallel floor under the path-parallel structure.

## 2. The categorical model (Dat + Trn — FRAMEWORK §2/§4)

New deduced query, `loop_plan`'s sibling:

```
path_plan : CategoryIr × FuncId → PathPlan          (deduced, total, deterministic — L2)
PathPlan  = { tasks : Task*, deps : Task → Task*,
              kind  : Task → Chain | Bulk | Loop | Token,
              trap_min : Task →? TopoIdx, checkpoints : Checkpoint* }
```

| Object | Meaning |
| --- | --- |
| `Task` | a maximal sequential unit: a chain of morphisms each feeding the next (Chain), one bulk op (Bulk — internally splittable by element range), one **pure** loop SCC (Loop — iterations are sequential by definition), or the token chain (Token — the effect spine, always exactly one). **An effectful loop (token in its SCC) is never a task** — the whole loop stays on the spine; its per-iteration prints reuse their static checkpoint wait-sets |
| `PathPlan` | the task DAG: tasks + "waits-for" edges = the execution paths and their joins |
| `Checkpoint` | a point where the runtime must observe the world exactly as the oracle would: every token op (each Print) and the fn exit |
| `TrapFlag` | one shared cell `(topo_idx, kind)`; the runtime's cross-thread trap carrier (the cuda `d_trap` shape) |

| Morphism | Signature | Partiality | Semantics |
| --- | --- | --- | --- |
| `path_plan` | `IR × FuncId → PathPlan` | Deduced | the whole schedule — a pure function of the sealed graph |
| `deps` | `Task → Task*` | Total | dataflow edges between tasks; no edge ⟹ concurrent-legal (this IS the language's parallelism, read off the graph) |
| `split` | `Bulk × ℕ → Range*` | Deduced | element-range slices of a bulk task; disjoint, exact cover (the data-parallel floor) |
| `trap_min` | `Task →? TopoIdx` | Partial | smallest topo index of a trap-capable op in the task, **transitive**: a bulk site inherits its body fn's closure capability (at the site's topo), a call its callee's; absent for provably trap-free tasks (`bounds_proof` + per-fn capability fixpoint) |
| `pinned` | `Task → 𝔹` | Deduced | task must execute on the host spine at its topo position: v1 pins tasks containing a trap-capable pure **Named** call — shared callee fns carry host-flavor guards and cannot speculate (recorded limitation; dual-flavor emission is the lift). Map/Fold body fns are single-site, so they take task-flavor guards and never pin |
| `watermark` | `Task → TopoIdx` | Runtime (per run) | monotone published "trap sites ≤ W decided"; the checkpoint wait primitive (rule 3) |
| `guards` | `Checkpoint → (Task × Threshold?)*` | Deduced | wait entries: a trap-guard entry `(t, W)` is satisfied when `t`'s watermark ≥ W (W = the max trap-site topo of `t` below the checkpoint) or `t` completed; a data-producer entry (no threshold) requires completion. One list, strongest requirement wins per task |

**Composition rules (the laws the implementation must preserve):**

1. **R-PAR (the one big law):** for every schedule, thread count, and interleaving, observable behavior (stdout bytes + exit code) is **byte-identical to the interpreter's sequential run in its deterministic topo order**. The oracle stays normative; parallelism must be invisible.
2. **Purity of concurrency.** Only the Token task performs effects; all other tasks are pure (no token in any signature — deduced, not checked at runtime). Two concurrent tasks never write the same slot: distinct producers write distinct objects (edge-only IR), and bulk slices write disjoint element ranges.
3. **Trap protocol — speculate-and-order (v3, revised during implementation review; supersedes the abandon-and-skip draft).** Task-context trap guards never call `mapal_trap`: a failing guard **records** `(site topo, kind)` into `TrapFlag` (CAS-min by topo) and **continues with a zero dummy** — task execution is total and defined everywhere (no abandon, no skip propagation, no partial-task corners). *Soundness:* every morphism topo-before the true first trap `t*` computes on real values (dataflow sources precede consumers in topo), so the minimum recorded topo IS `t*`; garbage-induced records are always > `t*` and lose the CAS-min. Each task publishes a **decided-watermark** (release atomic): "all my trap sites ≤ W have executed their guards" — scalar tasks bump it per site; bulk/loop tasks publish only at completion (their sites topo-follow all their inputs by the LoopEnter-deferral theorem, so no per-iteration store — the GEMM inner fold pays nothing). A `Checkpoint` waits until every guard task's watermark passes its topo (or the task completed) and its consumed values' producer tasks completed, **polling the flag each spin**: `flag.topo <` checkpoint ⟹ the HOST calls `mapal_trap(kind)` (exit 101) — else it prints. Exit checkpoint: all tasks + final flag check. Host-spine sites (token chain, effectful loops, pinned tasks) keep today's direct `mapal_trap`. Workers never exit, park, or unwind; a garbage-fed diverging loop can only stall a point whose oracle run also diverges (same theorem). Still the cuda shape — record on the worker, observe at the spine's sequential point — with the observation rule made order-exact.
4. **Fold order.** A fold is sequential within its task, all types (float non-associativity; ADR-0028's exact-op tree class is a later toolchain-wide wave). Fold-in-map still fans out — over the *outer* map's slices.
5. **Determinism (L2).** `path_plan` is a pure function of the sealed graph; same IR ⟹ same plan ⟹ byte-identical emission.

**Loc/Trm honesty (§4.5).** One shared-RAM `DataLoc` for every operand (the fn frame); workers are `Loc`s adjacent to it; the spawn/dispatch and join/checkpoint are the `Trm`s whose happens-before fences deliver operand visibility (law 1: no worker reads bytes no fence delivered; the spine reads no result before its join). `runsAt` is honestly a relation (law 6): one element-body `Trn`, up to T placements.

**Consolidation (§3).** No parallel twin of anything: the sequential run is the same plan executed by a pool of one (or `MAPAL_PAR=1`) — one emission form, one runtime, degenerate placement. And the query is ONE object consumed by all backends — not a per-backend re-derivation.

## 3. Per-backend consumption (the platform-layout table — the point of the idea)

| Backend | Path → | Bulk slice → | Trap carrier → | Status |
| --- | --- | --- | --- | --- |
| **llvm (CPU)** | pool task on one of T threads (T = logical cores) | element-range chunk task | `AtomicU64` flag + checkpoints | **this plan** |
| cuda | stream (concurrent kernels; today: one stream = paths serialized) | already a kernel grid | `d_trap` + post-launch check (exists) | queued — consumes the same `path_plan`, next cuda wave |
| verilog | spatial region / parallel FSM | unrolled lanes | done-protocol error line | planned with P7 |
| interp | — (stays sequential; IT IS the oracle) | — | — | never changes |

## 4. The CPU runtime (`mapal-rt` — the scheduler)

Small static-DAG executor, dependency-free (std only, threads ride pthreads):

- **Pool:** T = `MAPAL_PAR` if set else `available_parallelism()` (12 logical cores ⟹ 12 threads), spawned once per process, parked when idle.
- **Queue (Sapir amendment, at ratification):** per-worker deques + **work stealing**, seeded by the **static schedule**: `path_plan` ranks every task by critical-path weight at compile time (bulk n is static ⟹ costs are estimable), launch distributes ready tasks by rank, a completing worker keeps its unlocked dependents (locality), an idle worker steals. The static plan is the fast path; stealing only corrects runtime variance. A thread at a join never blocks — it helps (pops/steals within the same run) until its guards are done.
- **Static tables:** the emitted program hands mapal-rt, per fn invocation, a compile-time table: task fn pointers, kinds, ranks, dep counts, dependent lists, guard sets — the "best runtime built from compile-time knowledge": the scheduler discovers nothing, it executes a known DAG.
- **Bulk tasks** enqueue as `min(T, ⌈n/GRAIN⌉)` range slices (n known at compile time; slice bounds computed once).
- **Joins:** a thread reaching a checkpoint/join with pending deps helps — pops ready work instead of blocking (no idle cores, no deadlock on nested fn calls: a `Call` runs inline in its task; the callee's own plan enqueues on the same pool).
- **Trap flag:** one `AtomicU64` (`topo_idx << 32 | kind+1`), CAS-min, written by speculating guards (§2 rule 3); per-task decided-watermarks are the checkpoint wait primitive; the host polls the flag in every wait spin and is the only place `mapal_trap` fires for task-context traps. Workers run tasks to completion — recorded traps make later values garbage-but-defined, and nothing topo-after a recorded trap is ever observable.
- Small-n floor: a bulk op below `PAR_GRAIN` stays one task; a fn whose plan is one path runs entirely on the calling thread — zero scheduler traffic for sequential programs (graceful degeneration, FRAMEWORK §7.1).

## 5. Emission (`backend-llvm`)

- Each task's chain emits as its own internal fn (`@task{n}(ptr %frame)` — reads/writes the fn frame through one base pointer; bulk slices get `(i64 %lo, i64 %hi, ptr %frame)`), the same code the inline path emits today, rebased — guards, `bounds_proof` elisions, body calls all verbatim.
- The host fn becomes: materialize frame → register the static task table (`mapal_par_begin/task/pin/dep/launch`) → host walk with `mapal_par_wait`+`mapal_par_check` at each checkpoint's injection point → `mapal_par_finish` before the epilogue. Two injection refinements (both review finds, S24): a checkpoint's wait fires at the earliest host glue morphism that reads task-produced data (not at the token op itself), and a checkpoint living inside an effectful loop ALSO fires once before the loop's first `LoopEnter` — the loop's seed reads are otherwise unordered against the tasks.
- `FnAttrs`: task fns write memory → plain; inner body fns keep their attributes.
- L2: emit-twice byte-equal (plan is deterministic); L1: R-PAR (rule 1).

## 6. What does NOT change

Language surface, IR ops, lower, check, rewrite, the interp oracle (stays sequential-normative), ADR-0020's `emit` contract, fold order, loop CFG within a task, token linearity, guard/`bounds_proof` machinery, cuda emission (this wave), the differential duty (raw + rewritten, `-O0`/`-O2`, byte-compare vs oracle — now also the proof that parallelism is invisible).

## 7. Recorded constants & decisions

| Decision | Choice | Why |
| --- | --- | --- |
| Unit of scheduling | task DAG from `path_plan` (paths), NOT per-site loops | Sapir's correction — the graph already knows; backends consume, never re-derive |
| Where the query lives | `mapal-ir` (backend-independent) | every backend maps the same paths to its own layout (§3) |
| Trap delivery | flag + checkpoint (cuda shape), never worker `exit()` | worker exit races prints ⟹ would break R-PAR; checkpoints make trap order oracle-deterministic |
| `PAR_GRAIN` | 4096 elements (mapal-rt const) | below it slice overhead rivals work; *measured knob, bench wave revisits* |
| Thread count | `MAPAL_PAR` env else all logical cores | one knob; `=1` is the sequential A/B lever |
| Scheduling | **static list schedule (compile-time critical-path ranks) + work-stealing backstop**, dep counters, help-first joins | Sapir at ratification: "because we know the dispatching pattern, we can schedule even smarter" — the plan schedules, stealing insures against runtime variance; help-first prevents nested-call deadlock |
| Fold | sequential in-task, all types | order-observable; ADR-0028 tree class = separate toolchain-wide wave |

## 8. Work items (plain words)

1. **Graph analysis** (`mapal-ir`): a function that reads a compiled program's graph and answers "which parts are independent, which waits for which, where are the prints, which parts can trap." Pure analysis + its own unit tests; changes no behavior anywhere.
2. **Scheduler** (`mapal-rt`): the thread pool + queue described in §4 — ~200 lines, no dependencies. Unit tests: every index covered exactly once, dependency order respected, trap flag picks the oracle's trap.
3. **Code generation** (`backend-llvm`): emit each path as its own small function plus the static wiring table; prints become the wait-then-check points. Re-record the golden output files (the expected-text snapshots) once, each diff hand-read.
4. **Prove nothing changed observably:** the existing full differential — every example and 320 generated programs, compiled at `-O0` and `-O2`, output byte-compared against the interpreter — must stay at zero divergences. New cases: a big map that genuinely runs on all cores; a program that traps mid-parallel (must exit 101 with the exact oracle stdout prefix); same program at 1 thread vs 12 threads vs run-twice — all byte-identical.
5. **Measure:** re-run the matmul benchmark CPU rows; target: flow-llvm f32 @512/1024 moves from the single-thread class into the same box's multicore class (chapel/C++ rows). Report in the standing per-session perf format.

Order: 1 → 2+3 (parallel lanes) → 4 → 5. codex codes; orchestrator reviews every diff line-by-line.

## 9. Honest expectations

- For matmul specifically, most of the wall-clock win comes from slicing the big maps across cores; path-overlap adds little there (both fills already saturate the pool). Path-level structure is what makes the language's idea real — it pays off on branchy graphs (independent pipelines, fanout blocks) and it is the SAME object cuda streams and Verilog regions will consume. The measured 60× gap should close to the multicore class either way.
- The scheduler is the riskiest piece (nested calls + help-first). Mitigation: the plan degenerates to sequential under `MAPAL_PAR=1` on every differential case — bugs surface as A/B diffs, not mysteries.

## 10. Open questions

- None blocking beyond the gate. (cuda-streams consumption of `path_plan` and the ADR-0028 tree-fold both stay queued as their own waves; `MAPAL_PAR`/`PAR_GRAIN` stay harness knobs, not language surface — same class as the deferred `time` builtin.)
