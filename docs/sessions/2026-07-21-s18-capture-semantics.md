# Session 18 — 2026-07-21 — ADR-0027 capture semantics shipped + the re-benchmark

Orchestrator: Kimi Code · workers: implementation swarm (rewrite ∥ llvm ∥ cuda ∥ testgen) + 4-lens review panel + two fixer agents (one crashed mid-pass, its landed work verified and continued by a second) · skill: category-architect. Immutable log (ADR-0017). Session scope set by Sapir: "yes I ratify" (ADR-0027) + "benchmark against other languages after" + emitter-quality directives from reading `matmul_cap.cu`.

## 0. Continuation brief

Current state: **ADR-0027 shipped pipeline-wide; the one-kernel GEMM is real and measured; workspace 731 green; nothing live.** The matmul in capture form compiles to exactly one `__global__` and runs N=64 in **277 ms** (44× vs the S16 loop form's 12.16 s), N=128 in **278 ms** (359×), N=256 in **278 ms** — flat, startup-bound, faster than the naive-C binary's process wall (311–333 ms) on the same box. The review also found and fixed a **latent pre-existing `loop_plan` substrate bug** (computed exit payloads never evaluated — the S15 unexplained interp panic, now diagnosed and closed).
Next step: **region-v2 emission** (`components/backend-cuda/plans/plan-region-emission.md`: `inline` pass → `region_plan` in flow-ir → CUDA v2 with perf gates), or pull-forward emitter-quality rows #17/#18 (kernel shape dedup, arena allocation) — both now unblocked and prioritized.
Resume command/check: `cargo test --workspace` (expect 731); `cargo run -p flow-interp --example run -- benches/matmul/matmul4_cap.flow` (expect `-275\n3748`).

## 1. Work completed

- **ADR-0027 implemented pipeline-wide (ratified by Sapir this session):** `Operation::Map/Fold` gained `captures: u32` (source `(c₁…cₖ, [A;n])` / `(c₁…cₖ, Acc, [A;n])`); ir (typing, `map_captured`/`fold_captured`, validate, mermaid `+k caps` labels); lower (free-variable analysis with transitive nested-body descent + scope save/restore, capture wiring as broadcast edges, L1108 narrowed to mutation cases with a teaching diagnostic); interp (capture eval, read-at-position); rewrite (replay re-threads captures, fusion = identical capture sets only); llvm (body-call capture args); cuda (kernel/twin capture params); testgen (five capture shapes; 2048-draw stress = 1083 capture programs, zero failures).
- **4-lens review (oracle fidelity · lower · IR/rewrite · backend/F3): 16 findings, all adjudicated and fixed.** Two blockers: (a) **the `loop_plan` computed-exit-payload gap** — a merge-gated computation consumed only by the exit route was never scheduled (interp panic, llvm silent miscompile; *zero captures needed* — the S15 open item's root cause; fixed by widening the decide cone to the route feeders' transitive backward cone bounded by merge-reachability, in `flow_ir::algo`, every consumer inheriting); (b) capture resolution discarding the loop-poison bit (L1107 bypass → oracle panic). Majors: capture-walk scope leaks (Guard/Loop arms), rebind-of-captured-name misdiagnosed as L1104, stale L1108 tests, erased-capture hole (LLVM panic on `Unit` captures — builder now rejects), missing oracle pins. Verified fixes on the exact broken shapes (`t * 2 -> ret` → 6; exit-arm captured map → 13/23/33, both engines).
- **The re-benchmark (Sapir's ask):** capture-form matmul at N=16/64/128/256 on a fresh 4090 (≈$0.30, destroyed) — flat ~277 ms at every size, 44×/359×/~2800× vs the S16 loop form; faster at process scale than the naive-C and cuBLAS binaries on the same box; flow-llvm on the same source is *slower* than its old loop form at N=64 (2,076 ms — by-value aggregate copies, recorded llvm headroom). Full writeup in `docs/notes/bench-matmul.md` (S18 section).
- **Sapir's emitter-quality notes recorded** (`matmul_cap.cu` read-along): kernel shape dedup (#17), arena allocation (#18), guard elision, trap-param trimming, copy propagation — `components/backend-cuda/suggestions.md` #12–18 + region-plan §8 step 6 (dedup/arena pulled forward pre-region).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| IR representation of captures | **op field `captures: u32` + leading source-product components** (broadcast edges) | explicit in the graph (a map over a product element is unambiguous); DCE/CSE/region analysis see real edges; k=0 = every existing program unchanged |
| Capture order | **source-order of first use** (ratified ADR note) | a reviewer-found reversal (LIFO traversal) fixed; goldens re-pinned to the conformant order |
| Fusion with captures | **identical capture ObjectIds only** | union re-threading is sound machinery but unproven value; conservative = zero risk |
| Erased captures (`Unit`/`Str`/`IoToken`) | **rejected at the builder** (`ErasedCapture`) | validate-clean-but-LLVM-panics hole (unreachable from source; the public builder is a boundary) |
| loop_plan fix location | **flow-ir substrate** (the BL7 predicate), never per-consumer | one widening, all four consumers inherit — the whole point of BL7 |
| Token capture diagnostic | **`TokenInBody` → L1605 (user-facing)**, not L1901 Internal | a user error was reported as a compiler bug |
| Bench-box rustup failure (CN host, rust-lang route dead) | **cross-compile `libflow_rt.a` locally, ship it** | abort-and-record would burn the rental; the staticlib is dependency-free |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` | **731 passed, 0 failed** (was 673) | the whole arc + no regressions |
| The two previously-broken shapes | interp + llvm `-O2` both print 6 / 13·23·33 | the `loop_plan` widening is correct (guard-first intact, S12 invariants-before-header preserved by the merge-reachability bound) |
| testgen stress (2048 draws) | 1083 capture programs, 0 failures | generator + machinery at volume |
| llvm differential (agent-run) | capture programs at `-O0`/`-O2`, raw+rewritten, oracle-equal | backend parity |
| **S18 bench (4090)** | flow-cuda-cap 277/277/278/278 ms at N=16/64/128/256, outputs exact | the one-kernel wall is gone; startup is the next floor |
| flow-llvm (macOS) | 2.71 ms (N=16) / 2,076 ms (N=64) | by-value aggregate cost — recorded |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | **uncommitted** (S14–S18 work; Sapir owns commits) | `git status` |
| machine | vast.ai 45471663 (S18 bench box) | **destroyed** (≈ $0.30) | done |
| machine | vast.ai 45170851/52/45181070 | running — **Sapir-declared unrelated; do not use or destroy** | n/a |
| artifact | `benches/matmul/gen_flow_capture.py`, `matmul{4..256}_cap.flow`, `results.csv` | in-tree | `cargo run -p flow-backend-cuda --example emit -- benches/matmul/matmul64_cap.flow` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **Region-v2 emission** | `components/backend-cuda/plans/plan-region-emission.md` | `inline` pass → `regions.rs` → CUDA v2 + structural perf gates | matmul64 ≤ 0.3 s via regions; differential green |
| P1 | **Emitter-quality #17/#18 (pull forward)** | `components/backend-cuda/suggestions.md` | kernel shape dedup; arena allocation (one malloc/fn + offsets) | measured launch/malloc-count deltas in the perf gate |
| P1 | Kernel-time instrumentation in the perf gate | `docs/notes/bench-matmul.md` S18 | CUDA-event timing around launches (below the process-wall floor) | flow-kernel ms vs naive kernel ms at N≥256 |
| P1 | llvm by-value capture cost | llvm DESIGN/suggestions | by-reference capture passing for array captures | flow-llvm capture-matmul ≤ loop-form time at N=64 |
| P2 | `ExprKind::Call` unwalked in capture walk (latent, unreachable today) | `typing.rs` (fixer's note) | revisit if fn-in-expression-position ever becomes legal | pinned or closed |
| P2 | ADR-0024 decision; coproducts ADR on Sapir's word | `decisions/` | Sapir answers / orchestrator writes | status flips |
| P2 | `reduce` (canonical-tree) + `par` loop candidates | next-session 2b | orchestrator drafts on Sapir's word | two ADR candidates exist |
| P3 | Pre-existing: fanout as map-body tail fails L1201 (lower-lens note); per-iteration buffer leak (arena #18 covers); `ExprKind::Call` note | this log §6/§8 | any session | reconciled |

## 6. Architecture / model changes

`Operation::{Map, Fold}` gain `captures: u32` (ADR-0027 — a realized-set delta in the ADR-0018/0021 class). Body fns' input products gain leading capture components; capture edges are ordinary `Pair` edges (broadcast). `loop_plan`'s decide cone now includes the route feeders' transitive backward cone bounded by merge-reachability (the S15 substrate bug — loop attribution corrected once, everywhere). L1108 narrowed to mutation cases (write-to-captured-binding; the teaching diagnostic, D3). Erased-type captures rejected at the builder (`ErasedCapture`). Parallel-first package state: captures ✅ shipped · `reduce`/`par` candidates pending Sapir's word.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `decisions/ADR-0027-capture-semantics.md` | candidate → accepted (ratified; Q1–Q5 resolved in-file) |
| `components/ir/plans/plan-capture-semantics.md` | the implementation plan (written pre-code, model-first) |
| `docs/notes/bench-matmul.md` | S18 section (capture-form numbers, process-wall table, llvm contrast, emitter-quality links) |
| `components/backend-cuda/{STATUS,suggestions}.md` | perf notes S18; suggestions #12–18 (Sapir's emitter-quality set) |
| `components/backend-cuda/plans/plan-region-emission.md` | §8 step 6: emitter-quality follow-ons sequenced |
| `docs/{STATUS,next-session}.md` | S18 row + state |
| `docs/IMPLEMENTATION.md` | (entry points already carry the emit examples) |

## 8. Files changed

`crates/flow-ir/{graph,builder,validate,mermaid,algo}.rs` + tests · `crates/flow-lower/src/{typing,emit}.rs` + `tests/{captures,rejection}.rs` · `crates/flow-interp/{src/eval.rs,tests/captures.rs}` · `crates/flow-rewrite/{src/{replay,functor_laws,graph_rewrites,plan}.rs,tests/{captures,testgen_captures}.rs,tests/testgen/mod.rs}` · `crates/flow-backend-llvm/{src/func.rs,tests/*}` · `crates/flow-backend-cuda/{src/{kernel,func}.rs,tests/golden_cu.rs}` · `benches/matmul/*` · `examples/matmul4.flow` (S16) · docs per §7 · uncommitted (Sapir owns commits).

**Next `start` path:** read `sessions/2026-07-21-s18-capture-semantics.md` (this log) → `docs/next-session.md` → region-v2 or emitter-quality #17/#18 → `cargo test --workspace` (expect 731).
