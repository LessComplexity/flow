# Review — flow-check increment 1 (Session 10)

Change under review: the flow-check crate (DESIGN.md → 4 src modules + 3 test files,
25 tests) + doc reconciliation. Process: plan (7-reader fan-out) → DESIGN → 4-lens
adversarial design review (2 blockers, 5 majors, 6 minors — all applied pre-code) →
Opus implementer TDD-to-green → 3 adversarial impl reviewers (1 gating finding, fixed;
1 refuted) → fixer → orchestrator line-by-line read → workspace 448 green, fmt+clippy clean.

## §4.5 coherence checklist

- [x] **1. Placement honesty** — degenerate physical pair (single process); every input
  (`source`, `Program`, `CategoryIr`) is materialised in-process at the one `Loc`. No teleport.
- [x] **2. Transmission well-typing** — no `Trm` exists (no boundary crossed); the pipe-weld
  data are typed function parameters. Vacuously holds.
- [x] **3. Placement totality** — the one component's placements are the two passes in one
  crate; all projections defined (architecture-map.md row updated).
- [x] **4. Dependency mediation** — depends-on: check → ir, syntax (lib), → lower (dev-only).
  Same-process calls; no cross-location reach. Notably check does NOT depend on lower's
  internals — the one shared predicate (`is_print_builtin`) is a recorded copy (suggestions #1).
- [x] **5. Composition soundness** — roll-ups (docs/STATUS.md, docs/IMPLEMENTATION.md,
  architecture-map.md) deduced from this component's files, not forked.
- [x] **6. `runsAt` is a relation** — nothing assumes single placement; trivially holds.

## §8 modeling smells

- [x] No new object that is an existing object + morphisms: `check` complements `validate`
  (zero overlapping rules — validate = graph-shape, check = disclaimed semantic layer);
  `EffectSig`/`WriterSet` are deduced fibres, not stored twins.
- [x] Deduced stays deduced: `effectful?` read off lowered signatures (never recomputed,
  never cached across calls); writers read off `in_edges`.
- [x] Diagram ↔ morphism table parity: verified after review fixes (parity note added for
  the Trn rows, per interp/ir precedent).
- [x] Firewall holds: `Program`/`CategoryIr` held as data only; no Level-A restatement.

## Findings adjudicated this increment

| Finding | Verdict |
| --- | --- |
| Design B1: `check` needs `source: &str` (Name = span) | fixed pre-code (CK1) |
| Design B2: §0 typing claim contradicted lower §12 | fixed — supersession stated both sides |
| Design M1: FanoutKind is the discriminator (`seq` = same node kind) | fixed pre-code (§4 rewritten context-sensitive) |
| Design M2–M5: four citation errors | fixed |
| Impl major: cross-pass order never exercised | fixed — composed fixture, fails-on-reorder verified |
| Impl major: leftover `zz_probe.rs` | **refuted** — file never existed in tree (verified: find + git log) |
| Impl minors: T0201 span pins, token-bearing loop case, `pub use diag` leak | all fixed |

## Residue

OQ-C1 (nested-seq pin) flagged to Sapir; suggestions #1 (`is_print_builtin` seam) parked;
E3 vacuity + reopen trigger recorded (DESIGN §5); no bench by CK8.
