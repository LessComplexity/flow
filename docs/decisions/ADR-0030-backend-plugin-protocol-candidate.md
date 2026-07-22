# ADR-0030: External-backend protocol + SDK (serialized IR, subprocess contract) — candidate

Date: 2026-07-22 (S21) · Status: **candidate — direction requested by Sapir; NOT scheduled** (future addition; the companion folder-structure move IS accepted, see §Folder move). Number provisional.

## Context

All backends today are in-tree Rust crates compiled into the workspace, each realizing
ADR-0020's contract `emit(&CategoryIr) -> Result<String, EmitError>`. Sapir wants third
parties to add backends **without recompiling the compiler**: a stable handoff a backend
can pick up (e.g. the code graph) plus an SDK/ecosystem story.

## Decision (recommended shape)

**D1 — The handoff artifact is the serialized `CategoryIr`.** One versioned bundle
(JSON first; binary later if measured): the sealed, validated graph (objects, edge-only
morphisms, fn table, types — the 33-variant realized op set), stamped with a schema
version. Produced by the CLI (`flow dump-ir --format json`; the CLI crate's existing
P-item). The schema is declared once and derives the serializer, the docs, and the
validator (FRAMEWORK §5: one boundary declaration, many derived artifacts).

**D2 — A backend is a subprocess, not a dynamic library.** Contract: read the bundle
(stdin or file), write target text + structured diagnostics (the `EmitError` classes,
incl. `Unsupported` = the honest ✋ cell) per a small versioned protocol;
`flow build --backend <exe>` resolves and spawns it (the ADR-0020 strategy 2-category
made runtime: the registry is CLI backend resolution). Rejected alternatives: dlopen/C
ABI (Rust has no stable ABI; freezing a C ABI buys speed this boundary does not need and
loses crash isolation) and WASM components (viable later refinement — sandboxed
in-process — not v1). In FRAMEWORK terms the boundary is a `Trm` between the compiler
`Loc` and the backend `Loc`, `carries` = the bundle; Law 2 gives well-typing for free.

**D3 — The bundle carries the deduced queries.** `loop_plan`, `bounds_proof`,
`last_use_plan` (and successors) are exported per fn inside the bundle. External
backends inherit the analyses that made the in-tree backends correct and fast
(guard-first CFG, guard elision, in-place/freeing legality) instead of re-deriving
them wrong. Deduce once, transmit — never recompute across the boundary (§5).

**D4 — The SDK is the conformance kit, and the conformance kit is the product.**
Three pieces: (a) the schema + a thin reader lib (serde types + validate); (b) the
example corpus + testgen generator; (c) the differential runner packaged to drive ANY
`--backend <exe>`: emit → toolchain compile → run → byte-compare against the interp
oracle (raw + rewritten, trap classes, exit-101/102 protocol — the exact M2/M3 duty).
Oracle behavior is the final arbiter (ADR-0022), so "conformant backend" is a checkable
claim: pass the same differential our in-tree backends pass.

**D5 — Versioning.** Schema version = spec version of the realized op set
(`flow-as-implemented` + ADR ledger); additive op growth (e.g. `Widen`, the 33rd
variant, S21) bumps the minor; a backend declares the versions it accepts; the CLI
refuses mismatches loudly. No silent downgrade.

## Folder move (accepted now, execution deferred to a commit boundary)

In-tree backends move `crates/flow-backend-{b}` → `crates/backends/{b}` with **package
names unchanged** (`flow-backend-{b}` — crates.io's namespace is flat, and unchanged
names mean zero source edits: only `workspace.members` + relative `path` deps move).
The directory structure then states the model fact (parallel realisations of one
contract, one 2-category) and scales with the growth axis (verilog P7, TT P8/ADR-0025,
ecosystem backends out-of-tree). Execute as its own atomic commit AFTER the S14–S21
tree lands — not on top of the uncommitted mountain.

## Consequences / open questions

- The CLI crate (not-started) is the natural owner of dump-ir, backend resolution, and
  the conformance runner — this ADR mostly SEQUENCES behind the CLI P-item.
- flow-rt linkage for external backends: publish `libflow_rt` (staticlib + the 7-extern
  contract + trap protocol) as part of the SDK, or let backends bring their own runtime
  honoring the render-parity contract — Q1 for the full ADR.
- Token/effect ops across the boundary: the bundle carries them like everything else;
  the E2 legality was already discharged upstream (check) — external backends trust the
  sealed graph exactly as in-tree ones do.
- Q2: does `rewrite()` run before the dump (ship optimized graphs) or is the pass list
  a dump flag? (Likely: flag, default = the standard pipeline.)
