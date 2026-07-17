# Component: ir

Status: tested
Last updated: 2026-07-18 · Session 12 (topo_order LoopEnter deferral — loop-invariant hoisting guarantee)
Spec references: category-ir.md §3 (IR data structures) + §5 (Graph representation) + CHANGES.md §1 (structural fixes: single-source/single-target morphisms, first-class Phi, loops as trace + LoopMerge, back-edges as real adjacency edges) + **ADR-0013 / ERRATA LC-4** (dataflow-is-edges realization). Supporting: architecture.md §3. Authoritative design: DESIGN.md (this folder) — 3-way adversarially reviewed, then implementation 2-way reviewed + soundness-attacked + fix round, Session 04.
Depends on: (none — defines its own `SourceLoc`, D8) Depended on by: lower, check, interp, rewrite, backend-llvm, backend-cuda, backend-verilog, cli

## What works

- Full Core graph IR per DESIGN §2–§15: slotmap arena (objects/morphisms/functions), SecondaryMap adjacency + owner maps, zero HashMap anywhere (I12 determinism).
- 28-variant Core `Operation` set (ADR-0013 + ADR-0018): per-slot `Pair{slot,arity}` product formation, `Proj`, arith/cmp/logic + `Neg`, `Phi`, `Call`, `Map`/`Fold` (bodies as non-first-class FuncDefs), `Index`, `Zip`/`Enumerate` (collection primitives, ADR-0018), `Print`, `LoopEnter`/`LoopBack`/`LoopExit`, `Output`.
- Builder with per-call typing (DESIGN §5.1 table), composite atomic primitives, typestate `LoopHandle`, `Dest`-mediated ret writes; `seal()` is the only producer of `CategoryIr` and re-checks everything global (I4/I4b tokens, I5 per-edge loop placement incl. carried-**state**-in-SCC, I6 acyclicity + StructNameConflict, I-RET, Str placement).
- `validate()`: independent re-derivation of every graph-shape clause (separate module, own helpers); module docs list the provenance clauses it cannot certify.
- Iterative Tarjan `sccs(f)`, Kahn `topo_order(f)` (LoopBack emitted-not-gating, header-first; **S12: LoopEnter deferred until no other morphism is ready** — every multi-hop loop-invariant precedes its loop header, a theorem the interp driver and straight-line backends rely on; regression `topo_orders_multi_hop_invariants_before_loop_enter`), `loop_structure(f)` — the backend-verilog capability predicate (single-loop accept vs multi-merge reject shapes tested).
- Deterministic Mermaid dump (§14 format: `f{i}o{j}` ids, quoted labels, single `-->` style, `"LoopBack ↩"` + `⟲` merge prefix) + `lint_mermaid` (label-stripping arrow scan).
- IO-as-linear-token: Print chains, loop-carried tokens with the structural loop-fork I4 exception (forward-cone classification), token-sink I4b.

## What does not / known issues

- Cross-builder id mixing is **UB with no defense** (DESIGN §10; pinned by `cross_builder_funcid_mixing_is_unsupported_ub`). The earlier "versioned-key defense" claim was disproven by review SND-2 — escalate to an ADR (builder nonce in id types) if a second constructing client ever appears. Flagged for Sapir.
- Well-formedness ≠ unique meaning: multiple unconditional full-value Return writers seal clean; exclusivity is flow-check/interp's obligation (DESIGN §17).
- Deliberately out (P4/later): JSON serialization (§5.3), mutation/removal API (CSE/DCE need it; additive rewrites fit v1), bifunctor-image tagging (§9.5 — recomputable from adjacency).
- `ValueTyMismatch` is declared but unreachable via the public API (constant() derives ty from value) — kept as defense for future direct-value APIs, documented by test.
- **Session 05 fix (lower design-review finding TY-1):** zero-field `Struct` tys sealed
  clean but failed `validate()` with `BadInEdges` (a 0-component `pack_struct` minted an
  in-edge-less Temporary), breaching the headline "seal Ok ⇒ validate empty" property.
  Fixed two-layer: I9 intake now rejects zero-field `Struct` (`NonCoreType`, mirroring
  Tuple ≥2 / Array ≥1) and `pack_struct(&[])` is `EmptyProduct`; +4 regression tests.

## Invariants enforced (and where in code)

DESIGN §9 ledger I1–I12 + I-RET + I4b + I9s. Mapping: I1/I11 type-level (`Morphism` fields); I2 `builder.rs` per-call dispatch + `validate.rs::edge_type_ok`; I3/I-RET builder atomic primitives + `check_i_ret` + validate; I4/I4b `builder.rs::check_token_linearity` (seal) + validate (shared predicate `ty::ty_contains_token`, loop-fork via forward-cone BFS); I5 `builder.rs::check_loops` + `validate.rs::check_loops` (both test the carried state's SCC membership — route-object membership was proven insufficient, review F2/SND-1); I6 seal acyclicity (iterative DFS) + owner checks; I7 `constant()` sole setter; I8 ret-write API + validate graph-shape form; I9/I9s intake on declared **and synthesized** tys + seal `check_str`; I10 iterative depth-guarded Ty walks (MAX_TY_DEPTH=64); I12 storage discipline + tested determinism.

## Test coverage (golden / property / differential / skipped+why)

102 tests green (`cargo test -p flow-ir`; +1 S12 `topo_orders_multi_hop_invariants_before_loop_enter`): 57 unit (rejection matrix — every reachable `IrError` variant driven + ty predicates; the §5.1 typing-table golden oracle `typing_table_golden::edge_type_ok_matches_design_5_1`, pinning validate's per-op typing judgment against the DESIGN §5.1 rows op-by-op — now incl. `Zip`/`Enumerate` positive+negative rows; the ADR-0018 `zip`/`enumerate` happy-path + rejection tests; and the enumerate-bound twin `enumerate_bound_twin::oversize_enumerate_edge_flagged`) · 18 builder_rejections integration (reviewer-named holes: Eq-on-Bool vs Lt-on-Bool, Str-in-product, SingletonTuple, RetNotProduct, LoopBackOutsideScc-with-real-guard, TokenInPhi nested, TokenNotEscaping, TokenDropped, StructNameConflict, oversize-array slots, cross-builder UB pin) · 14 golden Mermaid (the §16 graphs (a)–(i) incl. §4.5 two-route loop B=U and B≠U, fanout no-join vs join, print-inside-loop token-U, 3-way value-guard Phi chain; every snapshot hand-verified + linted; 2 lint-regression goldens) · 4 proptests (headline interleaved valid+invalid seal⇒validate-empty @256 — generator now also emits array-build/`Zip`/`Enumerate` steps so the new ops are covered by the headline property, positive generator @128, Str-bearing-ty property, determinism byte-identical dumps + identical topo/sccs) · 8 algos (SCC contents, cycle-breakers, topo ordering incl. body<LoopExit<consumers, nested multi-merge, 100k-chain no stack overflow — J1). Differential: n/a until interp (P3). Nothing skipped.

## Performance notes (numbers + bench name + date; regressions flagged)

`ir_scale` (criterion, 2026-06-12): build+seal / to_mermaid / sccs — chain 1k: 0.60ms / 0.63ms / 0.070ms · chain 10k: 5.9ms / 6.5ms / 0.70ms · chain 100k: 65ms / 69ms / 7.9ms · grid 100k: 31ms / 29ms / 5.0ms. Near-linear O(V+E); `to_mermaid` was O(n²) in draft and fixed to O(V+E) during implementation.

## Open questions (→ ADR candidates)

- Cross-builder id nonce (above) — ADR candidate if ever needed.
- Bifunctor-image tagging: revisit in rewrite's design (DESIGN §17).
- E3 frontier vs fanout-join boundary: if flow-check needs the block boundary as data, escalate (DESIGN §17).
- Trap semantics (div/mod-zero, OOB) revert to Kleisli(Result) typing with Core+1 coproducts (ADR-0013).
