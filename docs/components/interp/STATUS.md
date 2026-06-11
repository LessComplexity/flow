# Component: interp

Status: not-started
Last updated: 2026-06-11 · Session 01
Spec references: category-ir.md §11 (Implementation guide — graph walk in topological order, §11.1 evaluation/lowering sketch, §11.4 codegen-style traversal) + ADR-0002 (fueled evaluation: loop evaluation is partial and carries a fuel/step-limit; divergence is a defined outcome, not a hang — E1). Supporting: category-ir.md §5.2 (cycle structure / SCC ordering for loop regions).
Depends on: ir, lower, check Depended on by: rewrite, backend-llvm, backend-cuda, backend-verilog, cli

## What works

## What does not / known issues

## Invariants enforced (and where in code)

## Test coverage (golden / property / differential / skipped+why)

## Performance notes (numbers + bench name + date; regressions flagged)

## Open questions (→ ADR candidates)
