# Component: backend-verilog

Status: not-started
Last updated: 2026-06-11 · Session 01
Spec references: category-ir.md §8.3 (Verilog backend — `F_Verilog`, Clocked-Cat) as patched by E1 (guarded trace + register feedback) + the done-signal protocol (`valid_in / busy / done / result` handshake; ADR-0002). Supporting: architecture.md §4.3 (Verilog backend); CHANGES.md §1.6 (Verilog aligns with the loop trace); HANDOFF §4.3 (standing restriction: feedforward pipelines + single-loop FSM only; everything else rejected-with-error).
Depends on: ir, lower, check, interp, rewrite Depended on by: cli

## What works

## What does not / known issues

Toolchain absent on this machine (verilator not installed) — differential tests will skip-with-reason per HANDOFF §5 item 5.

## Invariants enforced (and where in code)

## Test coverage (golden / property / differential / skipped+why)

## Performance notes (numbers + bench name + date; regressions flagged)

## Open questions (→ ADR candidates)
