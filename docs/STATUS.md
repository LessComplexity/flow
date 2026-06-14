# Flow — Global Status

Last updated: 2026-06-14 · Session 07
Current phase: **P3 underway** — interp DESIGN written + review-hardened; `flow-interp` implementation next (→ M1). Current milestone: M1 — sepia, abs, sum_to_n, pipeline, fanout run correctly on CPU via the interpreter (oracle established)

## Components

| Component      | Status      | Tests | One-line state                                              | Docs                                          |
| -------------- | ----------- | ----- | ---------------------------------------------------------- | --------------------------------------------- |
| syntax         | tested      | 174 ✅ | P1 complete: lexer + parser (ADR-0005/0009/0010/0011/0012); golden trees for all 6 examples, zero diags; P-code out-of-Core rejection; bench recorded. | [status](components/syntax/STATUS.md)         |
| ir             | tested      | 92 ✅ | Core graph IR per ADR-0013/LC-4: edge-only dataflow, sealed builder + independent validate, inline-trace loops, IO token, SCC/topo, linted Mermaid; bench recorded. S05: empty-struct hole fixed. S07: `Print{newline}`/`println` (ADR-0015, +1). | [status](components/ir/STATUS.md)             |
| lower          | tested      | 100 ✅ | P2 second half complete: full Core surface lowers to sealed validate-empty IR; all 6 examples golden (+countdown/effectful-call); 46 L-codes with rejection matrix; literal-width unification; token laws; bench recorded. | [status](components/lower/STATUS.md)          |
| check          | not-started | —     | Type / effect / lifetime checks for Core; not begun.       | [status](components/check/STATUS.md)          |
| interp         | design      | —     | Fueled reference interpreter (the oracle): DESIGN written + adversarially review-hardened (S07; 6 blockers fixed); implementation next. | [status](components/interp/STATUS.md)         |
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
| ADR-0010 | Guard arrows are single lexemes — adjacency + statement-initial context gate; `-7->x;` is a guard arm, write `-7 -> x;` | accepted — flagged to Sapir (next-session.md), revisable by superseding ADR | n/a (spec silent; no text patched) |
| ADR-0011 | Flow-Core loop labels are `loop` only; statement-initial `Ident {` disambiguation | accepted — amended by ADR-0012 (scan demoted to hint; `Ident {` always a struct literal) | n/a |
| ADR-0012 | Labeled blocks `:label { … }` / jumps `-> :label;` (prefix sigil both ends); enclosing-targets-only (E1/Verilog reducibility); `loop` keyword unchanged; labels stay Core+1 (P0110) | accepted — decided with Sapir, Session 03 | yes (LC-3: user-guide §3.5/§8.5 patched) |
| ADR-0013 | IR realization: all dataflow is edges (per-slot Pair, constants-as-objects); Core op set (+Neg/Index/Map/Fold/Print/loop edges/Output, −Identity/Const/Trace); loops as inline SCC-visible cycles; IO as linear world token (signature synthesis, token sink, token-in⇒token-out) | accepted — autonomous Session 04, **flagged for Sapir review**, revisable | yes (LC-4: category-ir §4.1/§5.3 marked, §3.3 pointer note) |
| ADR-0014 | FRAMEWORK.md adopted as the Level-B categorical model layer for compiler-internal design (firewalled from Flow-Cat / Level A); mandatory `## Categorical model (Dat + Trn)` DESIGN lead-section + FRAMEWORK §8 reconcile-gate line; new `docs/architecture/{categorical-model,INDEX}.md` | accepted — ratified by Sapir 2026-06-14 (Session 06) | n/a (methodology; Level A untouched — HANDOFF §7.1.5/§7.2 patched, not spec) |
| ADR-0015 | Split the print effect: `print` raw, `println` appends `\n`; one IR op `Print { newline }`; Core effect surface `{print}` → `{print, println}` | accepted — decided with Sapir (Session 07) | n/a (scope/methodology; HANDOFF §4.1 patched; Level-A spec untouched) |

## Session log (newest first)

| NN | date       | focus        | outcome                       |
| -- | ---------- | ------------ | ----------------------------- |
| 07 | 2026-06-14 | interp DESIGN + println split | **`flow-interp` DESIGN written + adversarially review-hardened** (6-dimension review, 22 confirmed findings incl. **6 blockers** — headline: the SCC loop-driver read the exit payload from an out-of-SCC route object → would have miscompiled every loop example; fixed via an incident-SCC body partition). Leads with its ADR-0014 categorical-model section; INDEX `interp`→modeled. **ADR-0015 (print/println split) decided + implemented**: `Operation::Print{newline}` + `println` builder (ir, +1 test); `is_print_builtin` helper routes the 9 effect/typing/emit sites that special-cased `print` (lower — one of them had regressed); examples use `println` for line output + `print` for pipeline's label; `dump_ir` example added (file → Category-IR Mermaid). Workspace **366 green** (174 syntax + 92 ir + 100 lower; fmt+clippy clean); 13 snapshots regenerated + hand-verified Print→Println-only. Interpreter still unimplemented — next session. |
| 06 | 2026-06-13 | FRAMEWORK / categorical model layer (Level B) | **ADR-0014 accepted** (ratified by Sapir 2026-06-14): FRAMEWORK.md adopted as the compiler-internal modeling method under a strict two-level firewall (Flow-Cat = Level A, frozen, untouched; the compiler itself = Level B). New `docs/architecture/categorical-model.md` (incl. §7 reduction audit — 11/12 findings survived adversarial verification; one firm ADR candidate: the backend strategy-2-category / `TargetText` contract) + `INDEX.md` (10 component rows: syntax/ir/lower modeled, 7 planned). `HANDOFF.md` §7.1.5/§7.2 amended (mandatory `## Categorical model (Dat + Trn)` DESIGN lead-section + FRAMEWORK §8 coherence line on the reconcile gate); syntax/ir/lower DESIGNs gained firewalled §0 model sections (point to the cross-cutting doc, no duplication). Built by a 6-phase dynamic workflow (73 agents: map → reduction-audit → synthesize → author → adversarial coherence review → critic); Fable-5 outage mid-run repaired by Opus. **Methodology only — no spec/code/test change; workspace still 365 green.** |
| 05 | 2026-06-12 | P2 flow-lower | flow-lower complete — **P2 done** (workspace 365 tests green: 174 syntax + 91 ir + 100 lower). Binding lower/DESIGN.md written (passes A–E, literal-width unification, derives-from-merge tags, 46-code L1xxx catalogue, LD1–LD24 ledger) + 3-way Fable adversarial design review with per-finding Sonnet verification (38 confirmed findings applied; killed three would-be miscompiles: Phi-arm mut leaks, the 55→66 snapshot bug, Block-tail routing guards). Implementation by Opus agents + 2 impl reviews + Fable soundness attack (19 attack findings; all real ones fixed with named regressions — headline ATK-02: effectful-*call* loops now carry the token). flow-ir empty-struct seal/validate hole (TY-1) fixed (+4 tests). All 8 golden snaps hand-read against §9 shape contracts (incl. orchestrator re-read after the head-naming fix). `lower_scale` bench recorded. Open: DESIGN §16 OQ1–OQ8 for Sapir. |
| 04 | 2026-06-12 | P2 flow-ir   | flow-ir complete (87 tests green; workspace 32 targets all ok): ADR-0013 + ERRATA LC-4 (spec's Pair-metadata/rhs_const dataflow conflict resolved → edges-only), DESIGN.md written + 3-way adversarial design review (26 findings applied — incl. the three token laws and exit-value pin, `sum_to_n` exits 55), implementation by Opus workflow agents + 2-way impl review + soundness attack (3 real breaches found: Str-param seal/validate gap, I5 route-vs-state SCC hole, u64→u32 arity truncation) + fix round with regressions. `ir_scale` bench recorded (100k morphisms: build+seal 65ms). lower/DESIGN §0.1 seeded with 5 pinned lowering obligations. Cross-builder id mixing pinned as UB — flagged to Sapir (nonce ADR if ever needed). |
| 03 | 2026-06-12 | P1 parser    | flow-syntax parser complete — **P1 done** (174 tests green incl. the ADR-0012 amendment): two-tier grammar (ADR-0005), thin spanned parse tree, P0001–P0012 + P0101–P0116 diagnostics with recovery, ADR-0011 (loop labels / `Ident {` scan), golden parse trees for all 6 examples (zero diags, independently re-derived), full_surface precise rejection, criterion lex+parse bench (1–7.5 µs/example). Design 3-way + impl 2-way adversarially reviewed; totality stack-overflow defect found & fixed pre-merge. Post-review with Sapir: ADR-0012 labeled-block sigil (`:label`) decided + implemented (LC-3 spec patch); lower/DESIGN.md seeded with the parse-tree-obligations extract. |
| 02 | 2026-06-11 | P1 lexer     | flow-syntax lexer complete (74 tests green): full-surface token set, ADR-0010 guard lexing, L0001–L0008 diagnostics, golden snapshots for all 6 examples + C8 fixture, proptest totality. Design + implementation each adversarially review-verified. |
| 01 | 2026-06-11 | M0 bootstrap | M0 complete — skeleton green, E1–E5 applied + ERRATA, ADR-0001…0007, docs system, 6 examples. |
