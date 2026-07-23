# 2026-07-23 — S24: the parallel orchestrator — graph paths across cores, measured

Orchestrator: Claude Fable. Immutable log (ADR-0017). Implementation by codex CLI (gpt-5.6-sol, xhigh) per the restored delegation split; every diff orchestrator-reviewed line-by-line.

## 0. Continuation brief

Current state: **parallel orchestrator v1 shipped, hardware-measured, session closed.** Three commits: `36b6d39` (feature: path_plan + scheduler + parallel flow_main), `c98d657` (bench: parallel .ll artifacts + -1t legs + s24_box.sh), + this close-out (results.csv S24, perf report, docs). Workspace green (`cargo test --workspace --release`: llvm 15 differential + 19 golden, ir 165, flow-rt 12, everything else unchanged-green). **The S23 chapel-multicore gap is closed: N=1024 f32 flow-llvm 184.0 ms vs chapel 192.7 ms — flow ahead — on one box; 19.1× over the same binary at FLOW_PAR=1** (`docs/performance/matmul/s24.md`). No live boxes from this session.
Next step: S25 per `docs/next-session.md` — pool-floor knobs (item 1) are the cheap follow-through; fmad decision has its full price on the table.
Resume command/check: `docs/performance/matmul/s24.md`; `git log --oneline -4`; `cargo test -p flow-backend-llvm` (~8 min).

## 1. Work completed

- **Design (Sapir-ratified in-session, two amendments):** `plans/plan-parallel-orchestrator.md` v2 — Sapir's correction reframed v1 (per-site loops) to the language's own idea: *the execution graph's paths are the concurrency; a deduced query hands them to every backend; each backend maps them to its platform's layout* (CPU threads now; cuda streams and Verilog regions consume the same query later). Scheduler amendment at ratification: static critical-path schedule seeds the workers, stealing is the backstop.
- **`flow_ir::path_plan`** (codex round 1 + fix round): the backend-independent task DAG — Split/Seq tasks, dataflow deps, saturating critical-path ranks, transitive trap sites (`fn_trap_capabilities` fixpoint over Call/Map/Fold closures, attributed at the referencing site's topo), threshold checkpoints (`WaitEntry`), pinning, token chain + effectful loops excluded to the host spine. 165 ir tests.
- **flow-rt scheduler** (codex round + v3 alignment round + one hand-fix): global pool, per-worker deques seeded by rank, work stealing, help-first waits, packed `(task<<32)|threshold` wait entries against per-task watermarks, CAS-min trap flag, `FLOW_PAR` env, `GRAIN=4096`, `flow_par_run_pinned`, std-only. 12 tests incl. randomized-DAG stress.
- **backend-llvm parallel `flow_main`** (codex round + two hand-fixes): one `%Frame` struct (update-elision aliases share fields), one `@task{i}(lo, hi, frame)` per task (Split = the same bulk loop over `%lo..%hi`; Seq = chains/folds/pure loops verbatim), static `begin/task/pin/dep/launch` table, checkpoint `wait+check` at computed injection points, `finish` before the epilogue. Single-path fns and every non-entry fn emit byte-identical sequential text (v1 scope: flow_main only — recorded ceiling). 19 golden + 15 differential.
- **The v3 trap protocol ("speculate-and-order"), derived mid-session** when adversarial review killed the ratified draft's abandon-and-skip semantics (deadlock + stdout-prefix corners): task guards record `(site topo, kind)` and continue with a dummy zero (branch-around keeps `sdiv`/loads off the bad path); min-recorded-topo = the exact oracle trap (everything before the true first trap computes on real values); the host polls the flag at checkpoints and is the only trap deliverer. Workers never exit/park/unwind. Plan §2 rule 3 rewritten; soundness argument rests on the S12 invariants-before-header theorem.
- **Box leg (fresh 4090 / EPYC 9B14 host, ≈45 min, ≈$0.25, destroyed):** full same-box sweep, all legs; results.csv 104 rows (S23 raw → `results-s23.csv`); `docs/performance/matmul/s24.md` + index row in the standing format.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Unit of parallelism | graph paths from a deduced query (path_plan), NOT per-bulk-site loops | Sapir: "the orchestrator should be GIVEN the parallelizable parts from the execution graph… each path its own concurrent way per backend" |
| Scheduler | static rank seed + work-stealing backstop | Sapir at ratification: "we know the dispatching pattern — schedule even smarter"; stealing insures runtime variance only |
| Trap protocol | v3 speculate-and-order (record + dummy + host-fired) | ratified draft (abandon/skip) had provable deadlock + print-order corners; v3 is airtight via the topo-deferral theorem; recorded as plan deviation |
| Pinning | registration-time (`flow_par_pin`), never queued | review find: run-time pinning raced the pool — a worker could execute host-flavor code |
| Fold | sequential in-task, all types | order-observable; ADR-0028 tree class is its own toolchain-wide wave |
| v1 parallelism scope | `flow_main` only | no nested runs/reentrancy; named callees keep sequential emission; recorded ceiling with lift paths |
| Bench single-thread baseline | same binaries at `FLOW_PAR=1` (-1t legs) | S23 fairness lesson: the comparison lives inside one table, one box, one binary |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` | all green (llvm suite 433 s) | nothing regressed toolchain-wide under the emitter restructure |
| `differential_parallel_trap_order` (-O0/-O2) | stdout prefix byte-exact + exit 101 | R-PAR under a real mid-parallel trap — the protocol's whole point, live |
| `differential_parallel_env_matrix` / `run_twice` / `bign` | byte-equal across FLOW_PAR=1/8/unset, run-twice, 65536-wide split | schedule invisibility |
| `parallel_effectful_loop_waits_before_entry` (golden) | wait+check precede the loop CFG | the review-find fix is structurally pinned |
| Box sweep, every flow row `out=` field | == interp oracle | field confirmation on a 384-thread host |
| **N=1024 f32 same-box** | **flow 184.0 ms vs chapel 192.7 (flow ahead); 1t 3,505; cpp 3,334** | **the S23 60× multicore gap is closed; 19.1× self-speedup** |
| N=512 f32 | flow 26.9 wall vs chapel 5.27 compute (≈3× floor-adjusted) | remaining small-N distance = the ≈11 ms pool-spawn floor (384 threads), not compute |
| GPU continuity (same box) | kernel f32@4096 1.05× vs naive-CUDA | S23 saturation story stands; @1024 cell is box-variant (noted in report) |

## 4. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | committed through this log's commit | `git status` |
| vast.ai | S24 box `45599634` DESTROYED (≈$0.25). **Two foreign instances running at close: `45550035`-successor `45591095` + `45602038` — NOT created this session, hands-off, flagged to Sapir** | `vastai show instances` |
| artifacts | `results.csv` 104 rows (S24) · `results-s23.csv` archive · 17 parallel-form `.ll` committed · `s24_box.sh` | — | `docs/performance/matmul/s24.md` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P1 | pool floor knobs (quota-width spawn; llvm compute timer) | next-session.md item 1; `flow-rt::configured_threads` | cgroup-aware width or runner `FLOW_PAR`; FLOW_PERF-twin for llvm | next box: N=16 flow row ≤ ~2 ms; compute column real, not estimated |
| P1 | fmad decision | next-session.md item 2 | Sapir yes/no on labeled non-oracle row | row lands or question closed |
| P2 | cuda streams consume path_plan | plan §3 | next cuda wave design note | a-fill ∥ b-fill overlap on GPU |
| P2 | v1 ceilings (callee parallelism, dual-flavor guards, tree-fold, catch_unwind removal) | next-session.md item 6 | lift on demand | per item |
| P3 | foreign instances 45591095/45602038 | §4 | Sapir confirms ownership | confirmed or destroyed |

## 6. Architecture / model changes

New objects/morphisms (all grounded, plan §2): `PathPlan/Task/Checkpoint/WaitEntry` (Dat, deduced — `flow-ir/src/algo.rs`), `flow_par_*` (Trm/Trn seam — `flow-rt/src/lib.rs`), `%Frame`/`@task{i}`/wait globals (functor image — `backends/llvm/src/func.rs`). Placement story: one element-body Trn, up to T `TrnLoc`s — §4.5 law 6 realized. Model/code divergences: none known; plan deviations recorded in its Status line. Coherence checklist run at plan v2 + revisited at v3 (rule 3 rewrite).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/backend-llvm/plans/plan-parallel-orchestrator.md` | v2 ratified → v3 protocol → SHIPPED v1 + deviations |
| `components/backend-llvm/{STATUS,IMPLEMENTATION}.md` | S24 sections: parallel form, guard flavor, scheduler seam, new tests |
| `components/ir/STATUS.md` · `docs/STATUS.md` (ir + backend-llvm rows) | path_plan query; S24 rows |
| `docs/IMPLEMENTATION.md` | flow-rt seam row + scheduler |
| `docs/performance/matmul/s24.md` + `matmul.md` index | the S24 measured story |
| `docs/next-session.md` | rewritten for S25 |
| memory | none added (no new cross-project fact; box-nproc gotcha recorded here + next-session) |

## 8. Files changed

`crates/flow-ir/{src/algo.rs,src/lib.rs,tests/algos.rs}` · `crates/flow-rt/src/lib.rs` · `crates/backends/llvm/{src/*,tests/*,tests/snapshots/*}` · `benches/matmul/{*.ll ×17,runner.py,s24_box.sh,results.csv,results-s23.csv}` · `docs/{STATUS,IMPLEMENTATION,next-session}.md` · `docs/components/{backend-llvm,ir}/*` · `docs/performance/matmul{.md,/s24.md}` · this log.
