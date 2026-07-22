# 2026-07-22 — S22: the minimal-emission wave + ADR-0031 (iota/fill pipeline surface)

Orchestrator: Claude Fable (category-architect skill). Immutable log (ADR-0017).
**Delegation reality:** the S21 codex split was attempted (T1 folder move + T2 `--rewrite` flags landed via codex, review-fixed) but codex went **network-dead twice** mid-session (worker process alive, ~0.1 s cputime after 10–25 min, zero edits) — WP-B, WP-C, and the ADR-0031 migration were implemented **inline by the orchestrator** (S21 WP5 precedent, Sapir's urgency directives standing). Six commits on main; Sapir directed the end protocol with the S23 mandate recorded in `next-session.md`.

## 0. Continuation brief

Current state: **S22 closed clean — six commits (`60e3aa1` · `2dc181b` · `644abb8` · `c9cceb8` · `4d8bb76` · `f3a95a1`), workspace 851 green (200 syntax · 153 ir · 155 lower · 29 check · 62 interp · 63 rewrite · 27 llvm + 1 ignored perf · 1 flow-rt · 161 cuda), fmt clean, tree committed.** The S21 mandate's core shipped: `flow_ir::emission_plan` (Dissolved/Inline/Named split rule) drives both cuda emitter lanes; the judged exhibit `d_fn3` collapsed from 23 locals + 15 assembles to one return expression; benches now emit through the optimizer (36 artifacts regenerated, −2843 net lines). ADR-0031 (Sapir, in-session): `iota(n)`/`fill(x,n)` call syntax is dead — `n -> iota` / `(x, n) -> fill`, the P0108 carve removed, IR byte-identity proven on the emitted artifacts.
Next step: **S23 per `docs/next-session.md`** — codex health-check, WP-D hoisting, WP-E llvm assessment, then the box leg and the FULL performance-comparison matrix (Sapir's run-until-done bar).
Resume command/check: read `docs/next-session.md`; `cargo test --workspace`.

## 1. Work completed

**P0 folder move (ADR-0030 §, codex T1 + orchestrator completion, `60e3aa1`):** `crates/flow-backend-{llvm,cuda,verilog}` → `crates/backends/{llvm,cuda,verilog}`, package names unchanged, history preserved (57 renames). As-built deviation recorded in the ADR: 13 depth-sensitive relative paths (2 `#[path]` testgen includes + 11 `CARGO_MANIFEST_DIR`-relative strings) needed one more `../` — codex's check-gate saw only the 2 compile-visible ones; the orchestrator sweep caught the 11 runtime ones. Living docs re-pathed (sessions/ untouched).

**WP-A `emission_plan` (codex, review-fixed, `2dc181b`):** per-object `EmissionClass{Dissolved, Inline, Named}` in `flow_ir::algo` — boundary/guard/fanout classification with transparent counting through dissolved products. **Orchestrator review caught a real R-NODUP bug:** a Proj-produced tuple could dissolve, silently dropping its consumers from the counts (`pair_slot_source` = None) — a shared computed field chain would classify Inline and duplicate textually. Fix: dissolution is Pair-built-only; two directed regressions. flow-ir 144 → 153.

**ADR-0031 (Sapir directive, orchestrator inline, `644abb8`):** Sapir saw `iota(262144)` in the bench source and rejected the ADR-0029 stage-2a call-expression carve on sight. New surface `n -> iota` / `(x, n) -> fill` on the `is_pure_builtin` stage path; the grammar lost its only call-expression production (P0108 uniform again, with an arrow-form teaching message); lower rides the EXISTING builder entries — `iota(count)` and **`fill_from(pair)`** (the S21 replay-faithfulness entry doubles as the surface spine). Static-n stays builder-owned (`NonStaticCount` → reworded L1612/L1613; oversize literals are width-owned L1202). New capability pinned: `4 -> n; n -> iota` is legal (a bound name IS the Constant object). typing threads the previous stage expr for size synthesis; `FnBuilder::ty_of` made pub. Migration: 14 bench `.flow` + both generators (rerun, byte-stable) + every embedded test source. **IR identity proven:** re-emitted `.cu`/`.ll` byte-identical to HEAD (0 diff lines); zero snapshot churn corpus-wide; oracle gates unchanged.

**WP-B DevEmit minimal emission (orchestrator inline, `c9cceb8`):** plan + expression memo inside the existing walk (op text-forms untouched — R-TEXT free). Dissolved products never materialize; Inline values nest parenthesized (`is_atomic_expr`); input param aliases `in` (the `o0 = in;` prelude class deleted — R-ONENAME). Emitter-side force-Named: Call targets (trap-check position) + product-typed Inline (local-name fallback). **Review catches during implementation:** capture reads through `pair_component` would duplicate Inline feeder expressions (d_fn4's `(o5/512)` twice — fixed: read through the Named operand product's field); Div/Mod fetched the target slot before the #13 check (panic on elided-guard Inline targets — fixed). d_fn3: 23 locals + 15 assembles → 4 names + 2 index temps + one return expression.

**WP-C FnEmit host lane (orchestrator inline, `4d8bb76`):** identical mechanism for host + `__host__ __device__` bodies. Host force-Named adds every bulk-op target (launch/readback lvalues). Scalar launch args inline (a fold seed constant rides the launch call). Phi strict-select discipline untouched. Exhibits: sepia channel rows one line each; fn1 (matmul v2 scalar body) one expression. Golden corpus re-pinned **+83/−432, every diff hand-read** — guards and selects verbatim.

**Item 1 — benches through the optimizer (codex T2 + orchestrator regen, `f3a95a1`):** both emit examples gained `--rewrite` (T2 also fixed the llvm example's silent-unknown-flag acceptance — codex, no fixes needed); new `benches/matmul/regen.sh`; all 36 checked-in artifacts regenerated in ONE churn (arrow sources + rewritten IR + minimal emission): +2591/−5434. Native gate: `matmul4_cap.ll` → clang -O2 → `-275`/`3748`.

**Also:** `PREVIEW.md` / `PREVIEW-matmul512.{cu,ll}` at repo root (untracked viewing aids for Sapir's nvim); plan-minimal-emission §3b/§3c as-built records; widen_f64 explainer given (ADR-0029 amendment recap); memory updated (codex health probe).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| ADR-0031: iota/fill surface | pipeline form `n -> iota` / `(x, n) -> fill`; carve removed | Sapir on sight: "this is flow" — builtins ride the arrow like print/zip/widen; count IS the input; grammar simplifies |
| ADR-0031 mechanism | lower reuses `iota`/`fill_from` builder entries; IR untouched | `fill_from` (S21 fixpoint fix) is exactly the arrow form's spine; zero backend churn, byte-identity provable |
| Oversize-count ownership | width system (L1202), not L1612/L1613 | an oversize literal never reaches the builtin — earlier, clearer diagnostic; IR twin still backstops |
| emission_plan owns `guarded?` | no backend parameter | bounds_proof + const-divisor + float-IEEE are all graph-visible; backends may only force-Named further, never inline a Named |
| Dissolution rule | Pair-built-only | Proj-produced tuples dissolving = silent count drop = textual duplication (R-NODUP break); caught in review, twice-pinned |
| WP-B shape | expression memo inside the existing walk, not a walk rewrite | op text-forms untouched ⇒ R-TEXT free; 150-line diff; exhibit-achieving |
| Capture reads | through the Named operand product's field | reading the feeder duplicates Inline expressions (the `(o5/512)` catch); per-iteration re-read dies at WP-D |
| Call targets / bulk targets / product-Inline | force-Named at the emitter | trap-check position is backend protocol the query can't know; launches need lvalues; braced literals deferred |
| Codex stalls (2×) | killed at ~0.1 s cputime; orchestrator inline | wall-clock + Sapir urgency; S21 WP5 precedent; health-probe rule recorded for S23 |
| T3 (first bench regen) | killed pre-completion | ADR-0031 was about to change every source — one churn instead of two |
| S22 close | end protocol now; WP-D/WP-E/box → S23 with a run-until-matrix bar | Sapir directive; S23 mandate recorded verbatim in next-session.md |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` at close | **851 green**, fmt clean | the whole S22 tree (841 + 9 WP-A + 1 ADR-0031 pin) |
| IR-identity witness (ADR-0031) | re-emitted `matmul4_cap.{cu,ll}` vs HEAD: **0 diff lines**; zero snapshot churn corpus-wide | surface change, same exec graph — Sapir's requirement, proven not asserted |
| Native oracle gate | `matmul4_cap.ll` → clang -O2 + libflow_rt.a → `-275`/`3748` (run twice: T2 acceptance + regen close) | full-pipeline emission executes correctly on this machine |
| cuda golden corpus re-pin | +83/−432, every diff hand-read | only ceremony died; guards/selects/trap protocol verbatim |
| Exhibit acceptance | d_fn3 = one return expression; d_fn4 wrap/unwrap + duplicate wrappers gone; each computation appears exactly once in matmul512_cap.cu | the S21 mandate's two laws, on the judged artifact |
| Hardware differential | **NOT RUN** (no box this session) | honest gap — S23's first gate |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | clean, committed through `f3a95a1` (+1 docs commit at close) | `git status` |
| vast.ai | `45510479` (Sapir's pytorch box) | unknown at close — **hands-off** | `vastai show instances` |
| artifacts | `benches/matmul/*.{ll,cu}` (36, rewritten-IR emission) · `regen.sh` | committed | `./benches/matmul/regen.sh` |
| untracked | `PREVIEW.md`, `PREVIEW-matmul512.{cu,ll}` (repo root) | viewing aids — regenerate or delete freely | — |
| background tasks | none (all codex tasks killed or completed; monitors closed) | — | — |

## 5. Open items (the S23 agenda — full detail in `docs/next-session.md`)

| Priority | Item | Next action | Done when |
| --- | --- | --- | --- |
| P0 | Codex health probe | trivial `codex exec` + cputime check before any WP dispatch | probe verdict recorded |
| P0 | WP-D hoisting (#16) | plan §4 rule via the same query; codex if alive, else inline | re-pins hand-read; 851+ green |
| P1 | WP-E llvm assessment | measure post-rewrite .ll; implement only if it pays this session | verdict row in backend-llvm/suggestions.md |
| P0 (done-bar) | Box leg + FULL comparison matrix | s21_box.sh; differential FIRST (S22 emitters hardware-unverified), then backup results.csv, full sweep N=4→512 all legs | `docs/performance/matmul.md` rewritten with the S23 matrix |
| P2 | region v2 · geometry · -fmad (Sapir) · arena v1.1 · tree-fold | as the numbers direct | — |

## 6. Architecture / model changes

- **`EmissionPlan` joins the deduced-query family** (`loop_plan`/`last_use_plan`/`bounds_proof`): the text-shape decision is a Cat-IR-level classification, consumed by both cuda lanes. Composition rule pinned: classification is FINAL at the query; backends may force-Named, never un-Name.
- **ADR-0031:** the surface grammar lost its only call-expression production; `iota`/`fill` are ordinary builtin stages; `fill_from` is the single Fill spine (surface + replay). Realized-op set, IR, oracle, backends: unchanged (proven).
- **R-NODUP invariant (new, load-bearing, twice-enforced):** every operation's text appears exactly once — dissolution requires Pair-built products (query side); capture reads go through Named product fields (emitter side). Any future emission change must preserve it.
- **ADR-0030 §Folder move executed** — backends group under `crates/backends/`; the ADR carries the as-built path-depth note.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/STATUS.md` | S22 close header; syntax/lower/ir component rows (S22 clauses, counts 200/155/153); ADR-0031 ledger row; ADR-0030 EXECUTED; ADR-0025 tt-path cell |
| `docs/decisions/ADR-0031-iota-fill-pipeline-surface.md` | NEW (accepted, Sapir S22) |
| `docs/decisions/ADR-0030-…` | §Folder move executed note + zero-source-edits deviation |
| `docs/components/backend-cuda/{STATUS,IMPLEMENTATION,suggestions}.md` | WP-B/WP-C waves; #15 discharged, #16 rescoped as WP-D; residual headroom |
| `docs/components/backend-cuda/plans/plan-minimal-emission.md` | NEW — model §0–§7 + §2 as-built (Pair-built rule, product-Inline) + §3b/§3c as-built |
| `docs/components/{ir,syntax,lower}/STATUS.md` + `ir/IMPLEMENTATION.md` | emission_plan; ADR-0031 waves; counts |
| `docs/IMPLEMENTATION.md` | EmissionPlan shared-object row; backend paths |
| `docs/suggestions.md` | cuda roll-up: #15 discharged (S22), remaining headroom rescoped |
| `docs/next-session.md` | rewritten for S23 (the mandate: codex-first split + run-until-matrix) |
| memory (`model-split-preference`) | codex health-probe recipe appended |
| this log | written at close |

## 8. Files changed

Six code commits (`60e3aa1`…`f3a95a1`): workspace manifests + 57 renames; `flow-ir` (algo.rs emission_plan + tests, builder ty_of pub, lib re-exports); `flow-syntax` (parser carve removal + tests); `flow-lower` (lib/emit/typing stage path + tests); `flow-interp`/`flow-rewrite` (test-source migrations); `flow-backend-cuda` (kernel.rs DevEmit + func.rs FnEmit reworks, pins, 14 snapshots); emit examples ×2; `benches/matmul` (14 .flow + 2 generators + regen.sh + 36 artifacts). Docs per §7 (+1 close commit).

**Gotchas recorded for S23 (tail of the standing list):** codex network-death signature = alive process, ~0.1 s cputime, zero edits — probe before trusting, kill at ~3 min flatline, inline fallback is precedented; S22 emitters are hardware-unverified until the S23 box differential; PREVIEW files are untracked aids; `emission_plan` classes are final — backends only force-Named.

**Next `start` path:** read this log → `docs/next-session.md` → codex health probe → WP-D → WP-E assessment → the box leg → the full matrix in `docs/performance/matmul.md`.
