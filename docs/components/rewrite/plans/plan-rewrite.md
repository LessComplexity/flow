# Plan: flow-rewrite (P4) — layers 3–4 + map fusion, property-tested vs the oracle

Written: 2026-07-17 · Session 12 · Status: approved (Sapir pre-authorized S12 "complete all"; DESIGN.md is the model)
Model: [`../DESIGN.md`](../DESIGN.md) — read it first; this plan only sequences the build.

## Definition of done (HANDOFF §8 P4)

Constant folding, DCE, CSE (layers 3–4) + map fusion (layer 1); every pass property-tested: random Core program × random inputs → interpreter-equal before/after (per DESIGN R1). Workspace green, fmt+clippy clean, bench recorded, docs reconciled, INDEX row `modeled`.

## Work packages (sequenced; TDD each; adversarial review after)

- **WP1 — crate scaffold + plan.rs + replay.rs + identity anchor.** Deps wired; `RewritePlan`; the §1.1 replayer complete (incl. loop quartet, Return writes, bodies); `identity.rs` green: `replay(ir, ∅)` on all 11 examples → validate-empty + interp byte-equal + lint-clean. *This WP is the risk concentrate — everything else is plans.*
- **WP2 — equations.rs (layer 3)** per DESIGN §2 + micro-goldens (incl. float non-rewrites, no-fold-on-trap).
- **WP3 — graph_rewrites.rs (layer 4)** per DESIGN §3 + micro-goldens (trap-conservative DCE incl. dead-trap-kept; CSE exclusions; fn-level DCE).
- **WP4 — functor_laws.rs (layer 1)** per DESIGN §4 + naturality.rs table (data only) + fusion micros.
- **WP5 — driver.rs + testgen + property.rs**: fixpoint + report; §6 generator (both strategies, both modes); §8.2 headline property + §8.5 adversarial cases; golden.rs example snapshots.
- **WP6 — bench + docs**: `rewrite_scale`; IMPLEMENTATION.md (functor table), STATUS.md, INDEX flip, global STATUS row; As-built section below.

## Test matrix pointer

DESIGN §8 items 1–6 are the acceptance list; every DESIGN §10 ledger row (RW1–RW8) has at least one named regression.

## As-built deltas (S12, reconciled into DESIGN same change)

1. **WP3's agent died mid-run (API 529)**; WP5 detected the hole and implemented `equations.rs` + `graph_rewrites.rs` in its own scope. All WPs landed; sequencing note only.
2. **CSE constant dedup omitted** — contradicted P1 (Constants unkeyable; replayer rebuilds them unconditionally). DESIGN §3.2/RW6 amended; headroom §11.
3. **DCE conservatively pure-only** — `Div/Mod/Index/Call/Map/Fold` results are unconditional keep-roots; the refined removability rows are recorded headroom (§11). Strict-superset-conservative, R1-safe.
4. **`RewriteResult` derives `Debug` only** (`CategoryIr` is not `Clone`/`PartialEq`); `RewriteReport` carries `Clone/Debug/PartialEq`; `applied` is cumulative per pass.
5. **Fixer (adversarial review, 16 findings)** caught a real miscompile: proj∘pack/index-of-const forwarding aliased **token-typed** components → `TokenNotLinear` panic at replay. Fixed at the shared `forward()` seam + regression.
6. **Orchestrator review (line-by-line)** then found + fixed: exit attribution used reachability (a second sequential loop's exit mis-attributed to the first merge → needless whole-graph skip) — now route-feeder-in-SCC (interp's rule), two-loop fns rewritable (`identity.rs::two_sequential_loops_rewrite_not_skipped`); and the **matching interp P0** (`derive_plan` attributed over the per-fn SCC *union* → assert-panic on two sequential loops, legal Core) — fixed per-merge, pinned in interp `loop_invariants.rs`.
7. `plan.rs::is_consistent`/`scc_membership` ungated from `#[cfg(debug_assertions)]` (release builds — benches — must compile the `debug_assert!` expression).
8. Bench numbers recorded in STATUS (chain 1k/10k/100k: 720µs / 7.75ms / 87ms; grid CSE: 257µs / 2.3ms / 23ms — ~linear).

All six WPs complete; workspace 511 green; DoD (HANDOFF §8 P4) met.
