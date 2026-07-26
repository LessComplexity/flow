# Mapal — Model Index

Last updated: 2026-07-18 · Session 13
Authority: ADR-0014 (FRAMEWORK adoption) · FRAMEWORK.md §6 (process), §2 (model-section shape)

The index of categorical models for the Mapal compiler, per FRAMEWORK §6 ("Add it to
the index of models"). Two kinds of model live here:

- **Cross-cutting** — `categorical-model.md`: the compiler-wide `Dat`/`Trn` picture
  (the two-level firewall, the degenerate `Loc`/`Trm` pair, the cross-component
  morphisms — SourceLoc duality, the Diagnostic seam, the type-resolution functor).
- **Per-component** — each component DESIGN.md leads with a `## Categorical model
  (Dat + Trn)` section (FRAMEWORK §2 / HANDOFF §7.1.5). This table is the authoritative
  list of where each lives; a component without a row here is **not yet modeled**.

Every modeled component is Level B (the compiler's own data types + passes). The
object-language Mapal-Cat (Level A) is specified separately in `docs/spec/category-ir.md`
and is **not** re-modeled here (errata E5 firewall).

## The pipeline, and what each stage owns

Written down because a reader asked whether Mapal has an AST, two answers came back, and only
the code settled it. It does, and the honest framing is: **the syntax is a serialization of the
execution graph — the parser's tree is the deserializer's scratch space, and the graph is the
program.**

| Stage | Signature | Owns | Erases |
| --- | --- | --- | --- |
| `lex` + `parse` | `parse(&str) -> ParseOutput { program: Program, diagnostics }` | tokens, spans, **error recovery** (`Item::Error`, `StmtKind::Error`, 32 P-codes) | nothing — it is the only stage that can hold a broken program |
| `lower` | `lower(&str, &Program) -> Result<CategoryIr, Vec<Diagnostic>>` | name resolution, declaration order (it walks `program.items` **three times**), desugaring, edge construction | `seq` (ADR-0019: *no IR footprint*), surface-form differences (ADR-0031: arrow and call forms collapse to one op), `c[i] <- x` ⇒ `Update`, error nodes (L1000 rejects them) |
| `check` | `check(&str, &Program, &CategoryIr)` | Return exclusivity (T0101), effect legality (T0201) | — reads **both**, because `Fanout`/`SeqBlock` scope is exactly what lowering erased |
| everything after | `CategoryIr -> CategoryIr`, `CategoryIr -> String` | **all** optimization and every deduced query — `path_plan`, `tile_plan`, `TileRead`, `bounds_proof` | — the tree is never consulted again |

**Lowering is deliberately not injective**, so the tree and the graph are *not* isomorphic: the
graph is a quotient of the tree. That is the point — optimization is uniform because syntax is
normalized away — and it is also why `check` still needs the tree for the one property the graph
no longer carries.

**Cost of the tree, measured** (`cargo run --release -p mapal-backend-llvm --example
stage_timing -- <file>`, conv2d_1024, min of 20):

| stage | time | share of a real compile |
| --- | --- | --- |
| lex + parse (the tree is built here) | **4.4 µs** | 0.003% |
| lower (tree → graph) | 78.8 µs | 0.06% |
| rewrite | 291 µs | 0.21% |
| emit | 773 µs | 0.55% |
| `clang -O3` on the emitted IR | **140 ms** | 99.2% |

So "skip the tree and parse straight to the graph" saves microseconds against a compile that is
99% LLVM, and costs error recovery, declaration-order freedom, and check T0201. Reopening that
question needs an ADR whose acceptance criteria are those three, not a preference.

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
| `rewrite` | Plan+replay rewriter: layers 3–4 (const fold, DCE, CSE) + layer 1 (map fusion) over sealed `CategoryIr`; R1 oracle-equality contract | [`components/rewrite/DESIGN.md`](../components/rewrite/DESIGN.md) | modeled |
| `backend-llvm` | `F_LLVM` piecewise emitter: alloca-slot scheme, ADR-0016 loop CFG, `mapal-rt` seam (ADR-0020, DESIGN §1 — the runtime crate is owned here, not a modeled component of its own), L1 oracle parity | [`components/backend-llvm/DESIGN.md`](../components/backend-llvm/DESIGN.md) | modeled · built/tested (S13) |
| `backend-cuda` | `F_CUDA` (the one place `Loc`/`Trm` are real — host↔device; model paid in its DESIGN per ADR-0022 D2's backend-seam practice) | [`components/backend-cuda/DESIGN.md`](../components/backend-cuda/DESIGN.md) | modeled · built/tested (S15, M3) |
| `backend-verilog` | `F_Verilog : Mapal-Cat → Clocked-Cat` (E1 done-protocol) | `components/backend-verilog/DESIGN.md` | planned |
| `cli` | `mapal build\|run\|dump-ir\|test`; the lone renderer of structured diagnostics | `components/cli/DESIGN.md` | planned |

The two `planned` rows have a DESIGN.md but no `## Categorical model (Dat + Trn)`
section yet; modeling each is part of its component increment (HANDOFF §7.2 step 2).
Add the model **and** flip its status here in the same change.
