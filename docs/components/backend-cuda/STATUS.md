# Component: backend-cuda

Status: not-started
Last updated: 2026-06-11 · Session 01
Spec references: architecture.md §4.2 (CUDA backend — `F_CUDA`) + category-ir.md §8 (backends as functors) + category-ir.md §8.2 (CUDA backend — kernel fusion = source-level map-fusion preserved by the functor). Supporting: category-ir.md §6.1.1 (List endofunctor / map fusion); HANDOFF §5 item 5 (CUDA `.cu` via nvcc when present).
Depends on: ir, lower, check, interp, rewrite Depended on by: cli

## What works

## What does not / known issues

Toolchain absent on this machine (nvcc not installed) — differential tests will skip-with-reason per HANDOFF §5 item 5.

## Invariants enforced (and where in code)

## Test coverage (golden / property / differential / skipped+why)

## Performance notes (numbers + bench name + date; regressions flagged)

## Open questions (→ ADR candidates)
