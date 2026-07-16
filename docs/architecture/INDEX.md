# Flow — Model Index

Last updated: 2026-07-16 · Session 10
Authority: ADR-0014 (FRAMEWORK adoption) · FRAMEWORK.md §6 (process), §2 (model-section shape)

The index of categorical models for the Flow compiler, per FRAMEWORK §6 ("Add it to
the index of models"). Two kinds of model live here:

- **Cross-cutting** — `categorical-model.md`: the compiler-wide `Dat`/`Trn` picture
  (the two-level firewall, the degenerate `Loc`/`Trm` pair, the cross-component
  morphisms — SourceLoc duality, the Diagnostic seam, the type-resolution functor).
- **Per-component** — each component DESIGN.md leads with a `## Categorical model
  (Dat + Trn)` section (FRAMEWORK §2 / HANDOFF §7.1.5). This table is the authoritative
  list of where each lives; a component without a row here is **not yet modeled**.

Every modeled component is Level B (the compiler's own data types + passes). The
object-language Flow-Cat (Level A) is specified separately in `docs/spec/category-ir.md`
and is **not** re-modeled here (errata E5 firewall).

## Models

| Model | Scope | Location | Status |
| --- | --- | --- | --- |
| Whole-system §4 map | Four atoms at a glance, component table, placements, coherence checklist (ADR-0017) | [`../architecture-map.md`](../architecture-map.md) | modeled |
| Cross-cutting `Dat`/`Trn` | Whole compiler — firewall, degenerate `Loc`/`Trm`, boundary morphisms | [`categorical-model.md`](./categorical-model.md) | modeled |
| `syntax` | Lexer + parser: source model, token/AST olog, the `lex`/`parse` passes | [`components/syntax/DESIGN.md`](../components/syntax/DESIGN.md) | modeled |
| `ir` | Sealed Core graph IR: `CategoryIr`/`Object`/`Morphism` olog, builder + validate passes | [`components/ir/DESIGN.md`](../components/ir/DESIGN.md) | modeled |
| `lower` | `lower` functor: surface tree ⇀ sealed IR, passes A–E, the type-resolution + signature-synthesis morphisms | [`components/lower/DESIGN.md`](../components/lower/DESIGN.md) | modeled |
| `check` | E2 effect legality (tree×graph) + Return exclusivity; typing discharged at boundary; E3 vacuity proof | [`components/check/DESIGN.md`](../components/check/DESIGN.md) | modeled |
| `interp` | Fueled reference interpreter (the oracle): the `RValue` domain, the `eval` `Trn`, the SCC loop driver, token-as-Writer | [`components/interp/DESIGN.md`](../components/interp/DESIGN.md) | modeled |
| `rewrite` | Layer 1–4 rewrite passes over `CategoryIr` | `components/rewrite/DESIGN.md` | planned |
| `backend-llvm` | `F_LLVM : Flow-Cat → LLVM-Cat` | `components/backend-llvm/DESIGN.md` | planned |
| `backend-cuda` | `F_CUDA` (the one place `Loc`/`Trm` are real — host↔device) | `components/backend-cuda/DESIGN.md` | planned |
| `backend-verilog` | `F_Verilog : Flow-Cat → Clocked-Cat` (E1 done-protocol) | `components/backend-verilog/DESIGN.md` | planned |
| `cli` | `flow build\|run\|dump-ir\|test`; the lone renderer of structured diagnostics | `components/cli/DESIGN.md` | planned |

The six `planned` rows have a DESIGN.md but no `## Categorical model (Dat + Trn)`
section yet; modeling each is part of its component increment (HANDOFF §7.2 step 2).
Add the model **and** flip its status here in the same change.
