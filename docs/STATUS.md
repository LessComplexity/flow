# Flow — Global Status

Last updated: 2026-06-11 · Session 01
Current phase: P1 — Frontend Current milestone: M1 — sepia, abs, sum_to_n, pipeline, fanout run correctly on CPU via the interpreter (oracle established)

## Components

| Component      | Status      | Tests | One-line state                                              | Docs                                          |
| -------------- | ----------- | ----- | ---------------------------------------------------------- | --------------------------------------------- |
| syntax         | not-started | —     | Lexer + recursive-descent parser for Flow-Core; next: lexer (P1).  | [status](components/syntax/STATUS.md)         |
| ir             | not-started | —     | Graph IR with builder-enforced invariants; not begun.      | [status](components/ir/STATUS.md)             |
| lower          | not-started | —     | Parse tree → IR per category-ir §4; not begun.             | [status](components/lower/STATUS.md)          |
| check          | not-started | —     | Type / effect / lifetime checks for Core; not begun.       | [status](components/check/STATUS.md)          |
| interp         | not-started | —     | Fueled reference interpreter (the oracle); not begun.      | [status](components/interp/STATUS.md)         |
| rewrite        | not-started | —     | Layer 1–4 rewrite passes + property harness; not begun.    | [status](components/rewrite/STATUS.md)        |
| backend-llvm   | not-started | —     | Textual LLVM IR → clang; not begun.                        | [status](components/backend-llvm/STATUS.md)   |
| backend-cuda   | not-started | —     | CUDA .cu for map-kernels via nvcc; not begun.              | [status](components/backend-cuda/STATUS.md)   |
| backend-verilog| not-started | —     | Feedforward + single-loop FSM Verilog (E1); not begun.     | [status](components/backend-verilog/STATUS.md)|
| cli            | not-started | —     | `flow build\|run\|dump-ir\|test`; not begun.               | [status](components/cli/STATUS.md)            |

Status vocabulary: not-started · design · building · tested · stable · blocked

## Backend capability matrix

| Feature                              | interp  | llvm    | cuda    | verilog |
| ------------------------------------ | ------- | ------- | ------- | ------- |
| pipelines / operator-shorthand       | planned | planned | planned | planned |
| functions                            | planned | planned | planned | planned |
| guards → Phi                         | planned | planned | planned | planned |
| loops / trace                        | planned | planned | planned | planned |
| parallel fanout (pure)               | planned | planned | planned | planned |
| seq + print (IO)                     | planned | planned | planned | planned |
| tuples / named types / fixed arrays  | planned | planned | planned | planned |
| map / fold inline-block              | planned | planned | planned | planned |

Legend: ✅ supported · ✋ rejected-with-error · planned

**Standing Verilog restriction (HANDOFF §4.3):** the Verilog backend supports only feedforward pipelines + single-loop FSMs (with the E1 done protocol). Everything else is rejected-with-error when implemented.

## Blockers

None.

## Errata/ADR ledger

| ID       | Title                                          | Status                      | Applied to spec? |
| -------- | ---------------------------------------------- | --------------------------- | ---------------- |
| E1       | Flow-Cat cannot be both total and traced-cartesian (loops are partial / guarded trace + done protocol) | accepted (ADR-0002) | yes |
| E2       | Parallel effects rule — no effects in parallel fanout; seq or KPN channels | accepted (ADR-0003) | yes |
| E3       | Memory-model guarantee scoped to first-order non-cyclic core | accepted (ADR-0004) | yes |
| E4       | Operator-precedence example fixed; a flow is a statement, not a value | accepted (ADR-0005) | yes |
| E5       | Rename surface keyword `category` → `type`     | accepted (ADR-0006) — veto window closed 2026-06-11, no veto; rename final | yes |
| ADR-0001 | Flow-Core scope                                | accepted                    | n/a              |
| ADR-0007 | Tech stack                                     | accepted                    | n/a              |
| ADR-0008 | Editor tooling & LSP plan                      | accepted                    | n/a              |
| ADR-0009 | Collection-operator syntax — postfix inline block; input tuple ↔ block params positionally (`(init, array) -> fold { acc, item -> ... }`) | accepted | yes (LC-2) |

## Session log (newest first)

| NN | date       | focus        | outcome                       |
| -- | ---------- | ------------ | ----------------------------- |
| 01 | 2026-06-11 | M0 bootstrap | M0 complete — skeleton green, E1–E5 applied + ERRATA, ADR-0001…0007, docs system, 6 examples. |
