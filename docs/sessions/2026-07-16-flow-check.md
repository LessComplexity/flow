# 2026-07-16 - flow-check (Session 10)

## 0. Continuation brief

Current state: **P3 complete.** `flow-check` designed + built: crate `crates/flow-check`
(4 src modules, 3 test files, 25 tests), workspace **448 green** (178 syntax · 101 ir ·
112 lower · 32 interp · 25 check), fmt + clippy clean. Working tree **dirty by design** —
the whole increment is uncommitted (commit is next-session item 1, Sapir's call on split).
Next step: commit, then **P4 rewrites** (unblocked: oracle + checks both exist).
Resume command/check: read `docs/next-session.md`, then `git status && cargo test --workspace`.

## 1. Work completed

- `docs/components/check/plans/plan-check.md` — model-first plan from a 7-reader Opus
  fan-out (wf_51a1970c); user chose Proceed at the plan gate.
- `docs/components/check/DESIGN.md` — full design (categorical model lead section,
  T-code catalogue, CK1–CK8 ledger, E3 vacuity proof); INDEX row `planned → modeled`.
- 4-lens adversarial design review (wf_62d83a85) — 2 blockers + 5 majors + 6 minors,
  all applied **pre-code** (see §2).
- `crates/flow-check` — implementation workflow (wf_6058337a): Opus implementer
  TDD-to-green → 3 adversarial reviewers → fixer; then orchestrator line-by-line read.
- Docs reconciled bottom-up (§7). New OQ-C1 flagged to Sapir.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| `check(source, program, ir)` — 3 params (CK1) | kept | E2 graph-invisible (lower token-threads `Plain`/`Seq` identically); `Name` = span, text needs `source`; alternatives (IR fanout annotation / E2-in-lower / tree effect inference) each violate a standing pin |
| Typing pass | **none — discharged by construction** | builder I2 + `validate::edge_type_ok` certify §5.1 independently; residual empty; lower §12's stale "re-walks the sealed graph" clause amended in step (design-review blocker B2) |
| Exclusivity rule (CK3) | strict `\|W\|>1 ⇒ T0101`, no loop-exit exception | no constructible legitimate multi-writer shape through M5 (L1405/L1409 upstream); loosening additive when multi-route-loop ADR lands (OQ-C2) |
| `effectful?` (CK4) | deduced from token-in-lowered-signature | signature synthesis already computed the effect closure; recompute = second source of truth |
| Nested `seq` in fanout branch (CK5) | does NOT legalize — T0201 stands | branch composite stays effectful under Kleisli composition; siblings still race; → OQ-C1 for Sapir, one-line ADR to loosen |
| E3 (CK6) | zero code + documented vacuity proof + reopen trigger | no heap ops (ADR-0013) + no ref types ⇒ violating graph unconstructible; a scope-guard pass would assert an unfalsifiable property |
| Boundary (CK2) | `debug_assert!(validate(ir).is_empty())`, no dirty-input T-code | validate is the designed debug-assert hook (ir §11); interp precedent |
| Bench (CK8) | none | two O(V+E) folds; STATUS records rationale |
| Impl-review finding "leftover zz_probe.rs" | **refuted** | file never existed (find + git log verified by fixer) |
| Plan's D-B loop-exit-cone exception | discarded for strict CK3 | simpler sound rule; the exception's only client is a rejected-upstream shape |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` | **448 passed, 0 failed** (verified by orchestrator directly) | whole system green with check live |
| `cargo test -p flow-check` | 25 (acceptance 11 · effects 8 · exclusivity 6) | DESIGN §7 plan fully realised |
| `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D warnings` | clean | style/lint gate |
| cross-pass-order fixture | `["T0101","T0201","T0201"]` exact; **fails when passes reordered** (verified empirically) | C-check-5 order pin is real, not vacuous |
| calc.flow `parse → lower → check` | zero diagnostics | calc fully in-Core — S10's read-phase discovery resolved green (first-ever pipeline pass) |
| shadow-an-effectful-fn case | rejected upstream **L1105 FunctionAsValue** (test documents) | check's scope-aware resolution is defense-in-depth; clean-shadow path unreachable via standard pipeline |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume | Stop / cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `536470b` | **dirty — S10 increment uncommitted** (docs + crates/flow-check + Cargo.lock; `git status -s` for the list) | `git status && cargo test --workspace` | commit = next-session item 1 (Sapir's split call) |
| process/jobs | — | none (3 workflows completed: wf_51a1970c readers, wf_62d83a85 design review, wf_6058337a implement) | — | none |

No remote machines, ports, or artifacts outside the repo.

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | Commit Session 10 | `git status -s` | Sapir (or next session on Sapir's word) commits | tree clean |
| P1 | P4 rewrites | HANDOFF §8 P4; `docs/next-session.md` item 2 | write `components/rewrite/DESIGN.md` model-first; flip INDEX row | property-tested passes, interpreter-equal |
| P1 | OQ-C1 nested-seq-in-fanout (new) | check/DESIGN §10 | Sapir confirms CK5 or supersedes by one-line ADR | pin ratified or ADR lands |
| P1 | RATIFY ADR-0016; ADR-0013 review | ledgers | Sapir | flags cleared |
| P2 | IN6 float ÷0 amendment | interp §14 | one-line ADR-0013 amendment | normative across backends |
| P3 | suggestions: `is_print_builtin` single seam | check/suggestions.md #1 | fold into any future flow-syntax/ir touch | one definition, two importers |

## 6. Architecture / model changes

`check` modeled + built (INDEX `planned → modeled`; architecture-map rows flipped):
two `Trn` (exclusivity, effects) over `Dat` {Src, Program, CategoryIr, Diagnostic*, TCode}
with deduced `effectful?`/`writers` fibres; physical pair degenerate. Cross-component:
lower §12's typing deferral superseded (discharge-by-construction, both docs agree);
the `lower/check` partial-functor domain (categorical-model §7.3) is now fully realised.
All six §4.5 laws PASS (`components/check/reviews/review-check.md`). No new `Loc`/`Trm`.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/check/DESIGN.md` | created from stub (model-first, pre-code) + design-review fixes + built-reality edits (name-keyed map, calc resolved, L1105 shadow note, cross-pass fixture note) |
| `components/check/{IMPLEMENTATION,STATUS,suggestions}.md` | functor table (file:symbol, all rows built); status `not-started → tested (25)`; 2 CT suggestions |
| `components/check/plans/plan-check.md` + `reviews/review-check.md` | plan closed accepted-&-built with deltas; §4.5 review written |
| `components/lower/DESIGN.md` §12 | typing-deferral clause amended (supersession, dangling §11.2 pointer removed) |
| `docs/architecture/INDEX.md` | check row `planned → modeled`; counts updated |
| `docs/{STATUS,IMPLEMENTATION,architecture-map}.md` | phase line P3 COMPLETE; check rows tested/built; session-log row 10 |
| `docs/next-session.md` | overwritten — points at commit + P4 rewrites |

## 8. Files changed

Uncommitted: `crates/flow-check/{Cargo.toml, src/{lib,diag,exclusivity,effects}.rs, tests/{acceptance,effects,exclusivity}.rs}`, `Cargo.lock`, and the docs in §7. `git status -s` is authoritative.
