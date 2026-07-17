# Component: backend-cuda

Status: not-started
Last updated: 2026-07-17 · Session 11
Spec references: architecture.md §4.2 (CUDA backend — `F_CUDA`) + category-ir.md §8 (backends as functors) + category-ir.md §8.2 (CUDA backend — kernel fusion = source-level map-fusion preserved by the functor). Supporting: category-ir.md §6.1.1 (List endofunctor / map fusion); HANDOFF §5 item 5 (CUDA `.cu` via nvcc when present).
Depends on: ir, lower, check, interp, rewrite Depended on by: cli

## What works

## What does not / known issues

Toolchain absent on this machine (nvcc not installed) — differential tests will skip-with-reason per HANDOFF §5 item 5.

**GPU access decided (Sapir, 2026-07-17 / S11):** rent an NVIDIA box (e.g. RTX 4090) via the **vast.ai CLI** for P6 — differential tests run for real on the rented instance instead of skipping forever locally. CUDA (P6/M3) is a stated priority. Arrange the instance + a run recipe ~when P5 closes.

## Invariants enforced (and where in code)

## Test coverage (golden / property / differential / skipped+why)

## Performance notes (numbers + bench name + date; regressions flagged)

## Open questions (→ ADR candidates)
