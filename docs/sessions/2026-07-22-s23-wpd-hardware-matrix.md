# 2026-07-22 — S23: WP-D + hardware verification + the full performance matrix

Orchestrator: Claude Fable (category-architect skill). Immutable log (ADR-0017).
**Delegation reality:** the split WORKED this session — WP-D was coded by codex (`gpt-5.6-sol`, xhigh) and orchestrator-reviewed line-by-line. The S22 "network-death" was root-caused to a harness bug: `codex exec` blocks forever reading non-TTY stdin — **`</dev/null` on every dispatch** is the standing fix (memory + gotchas updated). Two dispatches were lost to it before the fix; the third ran clean.

## 0. Continuation brief

Current state: **S23 closed at the mandate's done-bar — five commits (`64f1f50` WP-D · `1daef83` WP-E verdict · `944350c` S21 CSV archive · `acdb319` Fold force-Named fix · `7b7680c` the S23 matrix + perf-doc rewrite), workspace 853 green (200 syntax · 153 ir · 155 lower · 29 check · 62 interp · 63 rewrite · 27 llvm + 1 ignored perf · 1 flow-rt · 163 cuda), fmt clean, tree committed, box destroyed.** The S21→S22 mandate ("run until the full performance-comparison matrix exists") is DISCHARGED: `docs/performance/matmul.md` carries the S23 matrix — 84 rows, one fresh 4090, all legs (flow-cuda loop/cap/kernel f64+f32 + FLOW_PERF, flow-llvm cap f64/f32 + loop→64, naive-CUDA, cuBLAS, numpy, rust, cpp f32/f64, chapel f32/f64), six-way output agreement at every shared N. Headline: **the GEMM kernel alone is 1.60× from naive-CUDA (N=512 f32)**; the remaining gap is exactly `-fmad=false` (Sapir's open call) + launch geometry.
Next step: **S24 per `docs/next-session.md`** — no mandate in flight; the numbers direct (fmad decision, geometry, region v2), Sapir sets the next mandate.
Resume command/check: read `docs/next-session.md`; `cargo test --workspace`.

## 1. Work completed

**Codex health + root cause (P0):** probe passed; both WP-D dispatches then flatlined with the S22 signature (~0.1 s cputime, zero edits) — output showed `Reading additional input from stdin...`: codex exec blocks on non-TTY stdin that never closes. `</dev/null` fixed it immediately. The S22 "network-death" was plausibly this same bug; codex is more reliable than the S22 log suggests. Memory (`model-split-preference`) updated.

**WP-D — loop-invariant hoisting (codex, orchestrator-reviewed, `64f1f50`):** `kernel.rs::assemble_body_arg` → `(pre, inloop, arg)` on per-part `varying: &[bool]`; the three looping sites (sequential fold `__global__` kernel, `DevEmit::emit_map` twin loop, `DevEmit::emit_fold` twin loop — the d_fn4 exhibit) hoist the `pair` decl + invariant assigns (captures, array-acc handle) above the `for`; scalar-acc/element assigns stay per-step; the non-loop map-kernel site byte-identical. **No query change** — invariance is structural to fold/map semantics (ADR-0027 immutable captures), recorded as the plan §4 as-built deviation. d_fn4: 6 assigns ×512/thread → decl+4 preloop, 2+call in-loop. 21 bench artifacts +105/−105 — **every changed line a hoist move (diff-census-verified)**; 2 example snapshots (sepia/vector_add, capture-free folds — decl-only moves), hand-read. Pin: `captured_twin_fold_hoists_invariant_fields`. Suggestion #16 discharged.

**WP-E — llvm assessment (orchestrator, `1daef83`):** measured, deferred. LLVM IR has no expression nesting — `emission_plan`'s Inline class collapses nothing; the analogous headroom is alloca→direct-SSA, and `opt -passes=sroa` alone takes the exhibit **539→181 lines / 75→5 allocas** — mechanically recovered inside clang `-O2`, where runtime is already optimal (S21). A Phi-discipline emitter rewrite does not pay while the matrix is the bar. Row #10 in backend-llvm/suggestions.md.

**The Fold force-Named fix (orchestrator, `acdb319`) — the box differential's catch:** the FIRST hardware run of the S22 emitters failed 27/640 testgen emissions (`neg operand` panic class at every scalar-operand load). Root cause via local repro + instrumentation: **an in-twin fold's scalar result classed Inline by the query (one pure, non-boundary consumer) — but a fold emits as a LOOP; no expression form exists; `store_obj` silently dropped the value.** Invisible locally (the differential skips without nvcc; the golden corpus only folds into `Output`, a boundary). Fix mirrors WP-C's host rule: `DevEmit::new` force-Names `Fold` targets (other bulk ops produce arrays, already blanket-Named). Pinned (`twin_fold_scalar_result_into_scalar_op_is_named`) + **`examples/emit_sweep.rs`**: the differential's deterministic 320-draw emission sweep runnable WITHOUT nvcc (640/640 post-fix) — the local blind spot is closed permanently. Plan §3b as-built item 6 records it.

**The box leg (the done-bar, `7b7680c` + `944350c`):** `results.csv` archived to `results-s21.csv` BEFORE the sweep (the S20-loss rule). Fresh 4090 rented (first offer stuck >15 min in `loading` — recycled per gotcha; two `success: False` phantom contracts destroyed). Preflight → rsync → differential FIRST: first attempt 13/2 with 3 "divergences" — **all 15 s run-timeouts under 48-way fan-out (CUDA context-init serialization), zero real; 15/15 green pinned to 16 cores.** Then builds (all cuda + llvm-cap legs; two loop-form `.ll` clang compiles stalled 25–30 min — znver3-specific, killed, runner.sh's skip-with-reason held; only casualty flow-llvm loop N=128) and the full sweep. Chapel's .deb deps hit port-80 apt blocking — https sources + install + append run (measurement-kind safe, CPU-only legs). 84 rows pulled, six-way verification, box destroyed (**≈$0.42 total incl. the recycled boot**). `docs/performance/matmul.md` rewritten (S23 main table + kernel decomposition + box-variance honesty note; S21 archived in-doc).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Codex stalls | root-caused: stdin wait, NOT network; `</dev/null` standing | `Reading additional input from stdin...` in the killed dispatch's output; third dispatch ran clean |
| WP-D mechanism | emitter-side structural invariance; NO emission_plan change | captures/array-acc invariant BY fold/map semantics — a query rule would model emitter text, not the graph; plan §4 as-built records the deviation |
| WP-D invariance definition | by runtime VALUE, not expression string | scalar acc's assignment text is constant yet must re-execute; array-acc handle is invariant |
| Fold fix location | backend force-Named (DevEmit), not the query | mirrors WP-C's host precedent; "backends may force-Named further" is the plan's law; llvm consumes no plan yet |
| WP-E | deferred with measurements | no expression nesting in LLVM IR; sroa already recovers the shuffle; -O2 runtime optimal (S21); matrix outranks |
| Differential timeouts | 16-core pinning, not a timeout raise | root cause is context-init serialization; pinning reproduces the S21-class box; timeout stays honest |
| flow-llvm loop N=128 leg | skipped this box (znver3 clang stall) | 25–30 min cc1 ×2 killed; S21's 63.09 ms stands as reference; known-degenerate leg |
| Chapel recovery | https apt + post-sweep append | port-80 egress blocked; CPU-only legs are measurement-kind safe to append |
| llvm-vs-C++ S21 reversal | recorded as box variance, not regression | the `.ll` is unchanged since S21; znver3 favors the C++ loop; "within ±35% of single-thread C++, box-dependent" is the honest statement |
| First stuck box (45539021) | recycled at 15 min `loading` | standing gotcha; S15 precedent |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` at close | **853 green** (851 + WP-D pin + fold pin), fmt clean | the whole S23 tree; includes the llvm 1280-run -O0/-O2 differential locally |
| `emit_sweep` (new, local, no nvcc) | pre-fix: 27/640 panics; post-fix: **640/640** | the fold bug + its fix, reproducible on this laptop |
| Remote cuda differential (4090) | first: 13/2 (3 timeout-"divergences") → **15/15 green** at `taskset -c 0-15` | S22 minimal-emission + WP-D emitters hardware-verified; timeouts were fan-out contention, zero real divergences |
| WP-D artifact census | 21 artifacts, +105/−105, all hoist moves | R-TEXT held exactly; nothing but the hoist changed |
| The S23 sweep | 84 rows, six-way `c[0]`/`c[N²−1]` agreement at every shared N (+ oracle pins at 4/16/32) | the matrix; `docs/performance/matmul.md` |
| Headline | GEMM kernel alone 0.125 ms vs naive-CUDA 0.078 at N=512 f32 = **1.60×** | the S20→S23 optimization program's current floor; gap = fmad + geometry exactly |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | clean, committed through `7b7680c` (+1 docs commit at close) | `git status` |
| vast.ai | S23 box `45539759` **DESTROYED**; stuck `45539021` + phantoms `45539701`/`45539723` destroyed | only Sapir's `45527066` remains (pytorch image) — **hands-off, never touched** | `vastai show instances` |
| artifacts | `benches/matmul/results.csv` (S23, 84 rows) · `results-s21.csv` (archive) · box logs in scratchpad (ephemeral) | committed | `git log --oneline -6` |
| untracked | `PREVIEW.md`, `PREVIEW-matmul512.{cu,ll}` (repo root) | refreshed to WP-D text — regenerate or delete freely | — |
| background tasks | none (all watchers closed; codex exec workers killed or completed) | — | — |

## 5. Open items (the S24 agenda — full detail in `docs/next-session.md`)

| Priority | Item | Next action | Done when |
| --- | --- | --- | --- |
| P0 (Sapir) | `-fmad` call | decide labeled non-oracle row | ledger row + bench leg if approved |
| P1 | Launch geometry (#5) | measure grid-stride/block variants on FLOW_PERF rows | perf-doc delta row |
| P1 | Region emission v2 | design the multi-merge-SCC oracle boundary (orchestrator lane) | plan v2 ready for review |
| P2 | arena v1.1 · tree-fold (ADR-0028) · 17b · llvm heap · `time` builtin · procedural sepia · P7 Verilog · ADR-0030 protocol | as directed | — |
| P2 | ADR-0029/0031 flow-as-implemented patch rows | on ledger close | spec index updated |

## 6. Architecture / model changes

- **`DevEmit.force_named` grows `Fold` targets** — composition-rule note: on the twin side, statement-form producers (loops) can never be Inline regardless of the query's consumer count; the query stays backend-agnostic, the backend force-Names (plan-minimal-emission §3b item 6). R-NODUP unaffected.
- **WP-D as-built = emitter-structural, query untouched** (plan §4 as-built): the `Invariant(subtree)` rule was never needed as graph analysis — fold/map node semantics carry it. General invariant-EXPRESSION hoisting out of quartet cones: recorded headroom, no corpus shape demands it.
- No IR, oracle, or Level-A change. ADR ledger untouched (no new ADRs).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/STATUS.md` | S23 close header; cuda component row (163, S23 clause); session-log S23 row |
| `docs/performance/matmul.md` | REWRITTEN — S23 matrix, ratios, decomposition, verification, method, box gotchas; S21 archived in-doc |
| `docs/components/backend-cuda/{STATUS,IMPLEMENTATION,suggestions}.md` | S23 test additions + hardware-verification block + S23 matrix perf note; DevEmit cell (Fold force-Named, WP-D split, emit_sweep); #16 discharged |
| `docs/components/backend-cuda/plans/plan-minimal-emission.md` | §4 as-built (WP-D, emitter-structural deviation) + §3b item 6 (Fold force-Named) |
| `docs/components/backend-llvm/suggestions.md` | row #10 — WP-E assessment, deferred with sroa numbers |
| `docs/next-session.md` | rewritten for S24 (numbers-directed agenda; codex stdin + box gotchas) |
| memory (`model-split-preference`) | stdin root cause + `</dev/null` rule |
| this log | written at close |

## 8. Files changed

Five code/docs commits pre-close (`64f1f50` kernel.rs + golden_cu.rs + 2 snapshots + 21 bench artifacts + plan/suggestions; `1daef83` llvm suggestions; `944350c` results-s21.csv; `acdb319` kernel.rs + golden_cu.rs + examples/emit_sweep.rs + plan; `7b7680c` results.csv + performance/matmul.md) + 1 docs close commit. PREVIEW aids refreshed (untracked).

**Gotchas recorded for S24 (tail of the standing list):** codex `</dev/null` ALWAYS; `taskset -c 0-15` for box differentials on big-vCPU hosts; run `emit_sweep` locally before any box leg; znver3 clang stall class; apt https on egress-filtered boxes; never co-locate a pkill pattern with its relaunch text in one ssh command.

**Next `start` path:** read this log → `docs/next-session.md` → Sapir's fmad/`time` calls or the geometry/region-v2 lanes → `cargo test --workspace`.
