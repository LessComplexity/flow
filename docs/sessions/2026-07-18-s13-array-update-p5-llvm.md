# Session 13 — 2026-07-18 — ADR-0021 array update + P5 LLVM backend (M2)

Orchestrator: Claude (Fable 5) · design review: 38-agent workflow (8 Opus lenses × 2 Sonnet verifiers per finding) · implementation: sequenced-TDD Opus workflows with per-WP adversarial reviewers + fixers (two workflow crashes — an internet outage and two structured-output-cap failures — recovered by the orchestrator from cache/disk; no work lost) · orchestrator line-by-line review of every delegated diff (Sapir's standing mandate, reaffirmed this session). Immutable log (ADR-0017). Session scope set by Sapir: continue P5 + ratify the S12 stack + "I need the dynamic array access".

## 0. Continuation brief

Current state: **P5 complete, M2 reached, workspace 558 green** (199 syntax + 106 ir + 139 lower + 29 check + 44 interp + 27 rewrite + 13 backend-llvm + 1 flow-rt), fmt + clippy clean, all docs reconciled, committed (see §8). No live jobs/machines (vast.ai: no instances; all S13 workflows dead).
Next step: **P6 backend-cuda** — rent the vast.ai RTX 4090 box (memory: vast-ai-gpu-access), write backend-cuda DESIGN model-first (ADR-0020 contract; host-side prints per E2; the H↔D `Trm` makes the physical pair real), adversarial design review, then the implementation workflow. testgen + the differential harness pattern port directly.
Resume command/check: read `docs/next-session.md`, then `cargo test --workspace` (expect 558).

## 1. Work completed

- **Ratifications recorded (Sapir, S13):** ADR-0013 (+ new amendment: div-zero trap is integer-only, float ÷0 is IEEE — closes interp IN6, normative for all backends), ADR-0016 (guard-first), ADR-0020 (emission contract), rewrite RW2 (R1 ⊥-identified traps). Lower §16 OQ1–8 stay open (questions, not decisions — blanket ratification not applicable).
- **ADR-0021 (array element update), decided with Sapir + implemented pipeline-wide:** pure `Update : (Array{T,n} × I × T) → Array{T,n}` (fresh array, OOB ⇒ IndexOob, no token, fanout-legal; naive-copy semantics; dynamic sizes explicitly out — E3 scope) + surface rebind sugar `c[i] <- x;` (`BindStmt.index?` partial morphism; P0013/P0014/P0015). Lower wiring is **three explicit points** (emit→`rebind()` not `bind_new`; `collect_assigns_stmt`; `scan_stmt`/L1408) — the "inherited for free" claim was killed in design review. Rewrite: law L-a only (alias-channel); L-b/L-c → `reoperand` headroom (suggestions row). testgen: arrays + Update chains (trap_free in-bounds by construction; default sometimes OOB) + two-loops-per-fn (MAX_LOOPS=2).
- **P5 / M2:** `crates/flow-rt` (7 print externs + `flow_trap` exit-101; render-parity unit table vs `flow_interp::render`); `flow_ir::loop_plan` (**BL7** — the per-merge attribution predicate exported once from flow-ir; interp migrated onto it, S12 pins prove equivalence; backend consumes it); `crates/flow-backend-llvm` (full op-table emitter: wrapping ints no-`nsw`, Div/Mod split MIN/-1 guards (Div⇒MIN, Mod⇒0), type-directed index guards (u8 zext), strict select-Phi, token erasure arity-0/1/≥2 derived on demand, Update = guard+memcpy+GEP-store, guard-first loop CFG, `Unsupported` for multi-merge SCCs); differential harness (10 examples + 320-case closed testgen sweep, **raw and rewritten IR**, u8 ABI differential, native loop-driven matmul `8\n136\n`, traps exit-101, timeouts); sepia perf baseline recorded.
- **Design review before code (38 agents): 15/15 findings confirmed, all folded into ADR-0021/plan/DESIGN pre-implementation.** Headline kills: srem MIN/-1 must yield 0 (R1 break), u8 index sign-flip (3 independent lenses), rebind-inheritance falsehood, unimplementable rewrite laws, replay compile break, open-mode testgen unobservable, S12-P0-shape untested.
- **Post-hoc catches:** the u8 differential **failed clang immediately** — `zeroext` was emitted before the type in call args (invalid LLVM; both print sites; unexercisable by any prior test). Orchestrator's own review found and fixed **exit-only-payload double-emission** (`walk()` now skips by loop-plan ownership, not SCC incidence; a duplicated exit-arm `Print` would have silently broken R1; pinned by `exit_only_payload_emitted_once`). Reviewer catches during implementation: lower capture-check hole (indexed bind in map/fold bodies evaded L1108) and a blind flagship test (matmul seeded with the answer — reseeded with `b` so every write must land).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Dynamic array sizes (`[T]`) now vs later | kept OUT (ADR-0021 non-goal) | E3/ADR-0004 proof is fixed-size-scoped; Verilog E1 RAM sizing; `Update` doesn't need it — answered Sapir's question directly |
| Sugar form `c[i] <- x` (note) vs `x -> c[i]` | kept `c[i] <- x` | ratified note Option A; `<-` lexeme already exists (mut-init); `looks_like_bind` routes it with zero fork changes |
| Expression-position `update(...)` builtin | omitted | sugar-only per ratified note; addable later without a new op |
| Rewrite laws L-b/L-c this increment | deferred (headroom `reoperand` channel) | plan channels rewrite results, never operands — review-proven unimplementable as "equation rows" |
| `walk()` driver-ownership rule | plan-membership ∪ SCC-incidence | SCC incidence alone re-emits exit-only cone chains (double Print = R1 break) — orchestrator review |
| Fixer's rejection of loop-carried-Update differential | **overruled by orchestrator** | DESIGN-letter reading; the ADR's motivating program must run natively — added, green |
| Perf baseline N | capped at {16, 4096} (DESIGN said 262144) | Core has no array-fill ⇒ N-literal module; clang -O2 >25 min CPU at 262144 (observed); 4096 shows ~80× unambiguously; array-fill/heap lowering restores large N (suggestions) |
| Emitter trap blocks | per-site inline (DESIGN said shared trap_bb/fn) | as-built delta, semantics identical, recorded in DESIGN "(as-built S13)" |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` | **558 passed, 0 failed** (was 511 at S12) | both increments + no regressions |
| `cargo clippy --workspace --all-targets` / `cargo fmt` | 0 warnings / clean | gate |
| differential: 10 examples raw+rewritten | green (real clang compile+run) | the M2 line |
| differential: closed testgen sweep | 320 programs × raw+rewritten, green | random-program oracle equality incl. Update + multi-loop |
| differential: u8 index+print | green (after fixing the attr-order bug it exposed) | the class only a real toolchain catches |
| differential: loop-driven matmul (`c[t] <- v`) | native `8\n136\n`, raw+rewritten | ADR-0021's motivating program end-to-end |
| `exit_only_payload_emitted_once` | green | driver-ownership fix pinned |
| R1 property battery (Update-bearing pool) | green | rewrite soundness with the new op |
| `sepia_perf_baseline` (idle, arm64) | N=16: interp 1.53ms / -O0 4.60 / -O2 4.61 · N=4096: interp 387.36ms / -O0 4.84 / -O2 4.82 | native ~80× at N=4096 (spawn-dominated flat ~4.8ms); recorded in backend-llvm STATUS |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume | Stop / cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` | clean after commit (§8) | `git status` | none |
| machine/job | vast.ai | no instances (unchanged since S12) | `vastai show instances` | n/a — rent at P6 |
| toolchain | clang (homebrew llvm 22), verilator, icarus | present, exercised (clang) | `which clang verilator iverilog` | keep |
| workflows | all S13 workflows | dead/complete | n/a | none |
| artifact | golden `.ll` snapshots (13) | committed under `crates/flow-backend-llvm/tests/snapshots/` | `cargo insta test -p flow-backend-llvm` | keep |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 (S14) | P6 backend-cuda (M3) | `components/backend-cuda/STATUS.md` stub + ADR-0020 + memory `vast-ai-gpu-access` | DESIGN model-first (H↔D `Trm`, host-side prints per E2, nvcc toolchain seam) → adversarial review → implement; rent RTX 4090 via vastai for the differential leg | examples + testgen differential green on GPU (or honest skip-with-reason recorded); capability matrix cuda column filled |
| P1 | Sapir: lower §16 OQ1–8 | lower/DESIGN §16 | answer individually (blanket ratification didn't cover questions) | verdicts recorded |
| P2 | Emitter headroom | `components/backend-llvm/suggestions.md` (in-place Update; array-fill/heap lowering → perf N=262144; -O2 differential row; frem parity pin) | any session; all R1-safe | rows applied or re-parked |
| P2 | Rewrite headroom | `components/rewrite/suggestions.md` (#5 `reoperand` channel → L-b/L-c; #7 precise DCE) | any session | rows applied or re-parked |
| P3 | rewrite migration onto `flow_ir::loop_plan` | rewrite `plan.rs` still has its own `is_canonical`/`exit_of` (P5b migrated interp only) | mechanical swap; S12 pins prove equivalence | one predicate, three consumers |

## 6. Architecture / model changes

`Operation` set 29→30 (`Update`, ADR-0021 — realized-set delta, ADR-0018 class). `BindStmt` gains partial `index?` (consolidation §3 — extension, not a parallel statement kind). New components built: `backend-llvm` (modeled S12 → built/tested S13; its **physical pair is real** — clang toolchain + native process are genuine `Loc`s, the harness stdout/exit capture the `Trm`; architecture-map refreshed, §4.5 all-PASS stands) and `flow-rt` (owned by backend-llvm DESIGN §1 — the strategy-shape shared seam all backends will link). **BL7**: loop exit/body attribution is now one exported predicate (`flow_ir::loop_plan`) with two consumers (interp, backend-llvm); rewrite migration = open item P3. ADR-0013 amended (float ÷0 IEEE, integer-only trap — IN6 closed).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `decisions/ADR-0021-array-update.md` | new — decided with Sapir; review-hardened (3 wiring points, L-a scope) |
| `decisions/ADR-0013/0016/0020` + rewrite RW2 + interp IN6 | ratification statuses + the IN6 amendment |
| `components/ir/plans/plan-array-update.md` | new (model-first, review-corrected) + As-built (S13) section |
| `components/backend-llvm/{DESIGN,IMPLEMENTATION,STATUS,suggestions}.md` | DESIGN review fixes + 4 as-built deltas; IMPLEMENTATION rewritten stub→full functor map (incl. flow-rt symbols); STATUS stub→tested with real inventories + perf table; suggestions +4 rows |
| `components/{ir,syntax,lower,interp,rewrite}/{DESIGN,IMPLEMENTATION,STATUS}.md` | ADR-0021 + BL7 deltas, real per-crate counts (verified), test-name inventories |
| `components/rewrite/suggestions.md` | `reoperand` channel row (L-b/L-c headroom) |
| `docs/{STATUS,IMPLEMENTATION,architecture-map}.md` + `architecture/INDEX.md` | phase P5-complete/M2; backend-llvm row tested 13✅; capability matrix llvm column ✅ + new `array update` row; flow-rt + loop_plan cross-component rows; INDEX built |
| `docs/next-session.md` | rewritten for S14 (P6) |

## 8. Files changed

`crates/flow-rt/**` (new) · `crates/flow-backend-llvm/**` (new: 5 src + 3 test files + 13 snapshots) · `crates/flow-ir/{graph,builder,validate,mermaid,algo,lib}.rs` + tests (Update + `loop_plan`) · `crates/flow-syntax/{ast,parser}.rs` + tests (index bind) · `crates/flow-lower/{emit,typing}.rs` + tests (3 wiring points) · `crates/flow-interp/{eval,loops}.rs` + tests (Update arm, loop_plan migration, update/update_pipeline suites) · `crates/flow-rewrite/{equations,replay,graph_rewrites}.rs` + tests + testgen (L-a, arms, arrays/multi-loop) · `Cargo.toml` (workspace members) · docs per §7 · committed this session (see `git log`).
