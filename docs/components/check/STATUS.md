# Component: check

Status: tested
Last updated: 2026-07-17 · ADR-0019 WP3
Spec references: user-guide.md §5 (E2, ADR-0003), §6 (E3, ADR-0004), §7 (error values); category-ir.md §2.6 (Kleisli effects) + §10 (lifetime); architecture.md §2.2.4 (type checker) + §2.2.5 (lifetime/escape).
Depends on: ir, syntax (lower as dev-dep for fixtures) Depended on by: interp (IN3, by assumption), rewrite, backends, cli

## What works

- `check(source, program, ir) -> Vec<Diagnostic>` — the two owed passes, **28 tests green**:
  - **T0101 Return exclusivity** (DESIGN §3): >1 full-value writer per Return rejected (strict CK3); discharges interp IN3 / ir §17.
  - **T0201 E2 effect legality** (DESIGN §4): effectful morphism inside a parallel `StageKind::Fanout` branch rejected, message names `seq` (ADR-0003 mandate); **node-kind-keyed** sticky context (ADR-0019) — a `Fanout` opens the context unconditionally, a `SeqBlock` recurses sticky (inner `seq` does not legalize, CK5 now a theorem); scope-aware call resolution; `effectful?` deduced from lowered token signatures.
- All nine in-Core examples (incl. **calc**, first time through the pipeline — clean) pass with zero diagnostics; cross-pass order + determinism pinned by fixture.
- Typing obligation discharged **by construction** at the validate boundary (DESIGN §0; lower §12 amended in step).

## What does not / known issues

- Nothing red. E2 nested-`seq` reading (CK5) is now a **theorem** (ADR-0019 made `seq` its own node) — OQ-C1 closed; a pure `seq` in a branch is clean by construction.
- Nothing calls check in a driver pipeline yet (cli is P-later) — OQ-C4; tests are the only callers.

## Invariants enforced (and where in code)

- C-check-1 boundary: `lib.rs:check` debug_assert (validate-clean input).
- C-check-2 discharge: `exclusivity.rs` — |W| ≤ 1 ⇒ IN3 sound.
- C-check-3/4: `effects.rs` — deduced `effectful?`, sticky Plain context.
- C-check-5 determinism: no HashMap anywhere; order pinned by `cross_pass_order_is_exclusivity_then_effects`.

## Test coverage (golden / property / differential / skipped+why)

- 28 tests: 11 acceptance (9 examples, determinism, cross-pass order) · 6 exclusivity (hand-built IrBuilder multi-writer shapes, slot form, loops incl. token-bearing) · 11 effects (all DESIGN §7.3 cases incl. node-kind seq discrimination, both fanout/seq nesting directions, loop/rebind statement forms in a seq body, pure-seq-in-branch clean, and T0201 span pins).
- E3: **zero tests by design** — vacuous for Core (DESIGN §5 proof: no heap ops, no ref types; violating graph unconstructible). Reopen trigger pinned.
- No proptest yet: input domain is lower-clean programs; the acceptance set + hand-built IR covers every constructible rejection shape. Revisit if rewrite adds IR producers.

## Performance notes (numbers + bench name + date; regressions flagged)

- No bench (CK8): two O(V+E) folds over graphs that build in µs–ms; nothing perf-relevant. Add only if a profile says otherwise.

## Open questions (→ ADR candidates)

- OQ-C1: **CLOSED** by ADR-0019 — `seq` is its own node, so nested-`seq`-in-fanout legality falls out of node-kind discrimination (effectful seq → T0201, pure seq → clean); no pin.
- OQ-C2: T0101 same-SCC-exit-cone loosening — owned by the future multi-route-loop ADR (lower OQ7).
- OQ-C3: E3 reopen — owned by the first heap-op ADR (frontier + escape pass + T03xx).
- OQ-C4: pipeline enforcement (`build = parse ; lower ; check`) — cli DESIGN pins it when written.
