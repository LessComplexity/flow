# ADR-0014: FRAMEWORK.md is the categorical model layer for compiler-internal design (Level B), distinct from Flow-Cat (Level A)

Date: 2026-06-13 · Status: **accepted** — ratified by Sapir 2026-06-14 (Session 06); revisable by superseding ADR

## Context (what forced the decision; spec refs)

`FRAMEWORK.md` (added at repo root this session) is a portable, product-agnostic
method for modeling software as four atoms — `Dat` (data category / olog), `Trn`
(transformations as objects with `t_from`/`t_to` projections), `Loc` (physical
execution sites), `Trm` (typed transmissions) — with the **Consolidation
Principle** and its five-step reduction (§3) as the central move, and six
coherence laws (§4.5) as the review checklist (§8). The project already reasons
this way in places — the IR ledger's "deduce, don't store" (D3/D5: topo/SCC/loops
recomputed, no materialized `Trace`), the syntax/ir `SourceLoc` "one source of
truth, variation at one seam" decision (D8), `validate()` as an independent oracle
(§11), the per-`Operation` typing table as a single declared seam — but it does so
**without naming the method**, so the discipline is folklore, applied unevenly,
and re-litigated each session.

Two facts force a decision rather than ad-hoc adoption:

1. **There are two categories called "category," and they must never touch.**
   Flow programs *are* morphisms in **Flow-Cat** — the **object language**
   (Level A), already modeled in `docs/spec/category-ir.md` and frozen. The
   **compiler itself** — its own internal Rust data types and passes — is a
   *different* category (Level B): `flow-syntax`'s AST, `flow-ir`'s `CategoryIr`
   graph, `flow-lower`'s tables and passes. The crate names collide deliberately
   (`CategoryIr`, `Object`, `Morphism`, `Operation` echo the Level-A nouns), and
   the project has **already paid for that collision once** — errata **E5**
   (ADR-0006) renamed the surface keyword `category` → `type` precisely because the
   two senses of "category" were confusable. Adopting FRAMEWORK as the modeling
   method without an explicit firewall reopens that exact hazard.

2. **The physical pair is degenerate almost everywhere — and we must say so, not
   pretend otherwise.** The compiler is a single in-process pipe-and-filter
   pipeline (FRAMEWORK §7.1): lex → parse → lower → (validate) → backend. Every
   filter shares one process `Loc`; every pipe is same-location. Per §7.1's
   degenerate-case note, **`Loc`/`Trm` collapse** and the model reduces to `Dat` +
   `Alg`. Applying the *physical* pair richly inside the compiler would be
   ceremony with no content. `Loc`/`Trm` become genuinely real at exactly **one**
   seam: the **backend/runtime** (CPU/GPU/FPGA targets; host↔device transmission,
   e.g. `cudaMemcpy`, FPGA `stream_data`), where placement and transmission carry
   actual cost (cf. the backends-as-parallel-functors analysis).

## Decision (one paragraph, imperative)

Adopt `FRAMEWORK.md` as the **categorical model layer for compiler-internal
(Level B) design** — the method by which every `flow-*` crate's own data types and
passes are modeled, reviewed, and documented. Maintain a **strict two-level
firewall**: **Level A** is the object language (Flow programs as morphisms in
**Flow-Cat**), authoritative in `docs/spec/category-ir.md`, **frozen and not
restated by FRAMEWORK** — it appears inside Level B only as *data* (e.g.
`flow-ir`'s `CategoryIr` value is a Level-B `Dat` object that *represents* a
Level-A morphism, never a Level-A arrow itself); **Level B** is the compiler, and
**that** is what FRAMEWORK models. Apply the **logical pair `Dat` + `Trn`**
richly throughout the compiler; treat the **physical pair `Loc`/`Trm` as
degenerate** (FRAMEWORK §7.1) everywhere **except** the backend/runtime seam,
where it is invoked for real (target `Loc`s; host↔device `Trm`s). Make a
**"Categorical model (`Dat` + `Trn`)"** section the **required lead** of every
component `DESIGN.md` (after the one-line overview, before scope/tables — per
FRAMEWORK §2 "How to write a model section" and §6 rule 1 ("model first")): objects
(the crate's own types) and morphisms (their field/structural relations and
passes) come before prose. Add `docs/architecture/categorical-model.md` (the
binding statement of this layer: the firewall, the `Dat`/`Trn` vocabulary, the
`Loc`/`Trm` degeneracy scoping, and the required DESIGN lead-section template)
and `docs/architecture/INDEX.md` (the index of models FRAMEWORK §6 rule 4 requires).
Add a **FRAMEWORK §8 coherence line** to the doc-reconcile gate, so each session's
reconcile step checks the changed component's model section against the §8
checklist (modeling smells: no parallel object that is "an existing object plus
morphisms"; deduced-not-stored values justified; every diagram morphism in the
table and vice versa) alongside the existing ledger checks.

## Consequences (tradeoffs, implementation impact)

- **This ratifies and unifies; it does not introduce.** Honest note: the project
  already *practices* much of FRAMEWORK §5/§6 — "deduce, don't store" (ir D3/D5:
  topo/SCC/loop regions recomputed on demand, no stored `Trace`), "one source of
  truth, variation at one declared seam" (ir/syntax `SourceLoc` D8; the per-`Operation`
  typing table as the single typing seam), "isolate effects / define each boundary
  once" (renderer-free `Diagnostic`/`IrError`/`IrViolation`, presentation deferred
  to the CLI), and the independent-oracle pattern (`validate()` as a from-scratch
  re-derivation — flow-ir DESIGN §11 — two derivations of the same invariant, the §5
  "deduce, don't store" principle applied to correctness checks). This ADR
  **names** that practice and makes it the default, rather than inventing a new
  obligation. The cost of adoption is therefore mostly *documentation* (lead
  sections, two architecture docs, one reconcile line), not redesign.
- **The two-vocabulary hazard is real and is mitigated by the firewall.** Carrying
  two senses of "category" risks exactly the UX/comprehension tax FRAMEWORK §3
  warns about (the E5 collision in object form). The firewall is the mitigation:
  every Level-B model section states up front that its `CategoryIr`/`Object`/
  `Morphism`/`Operation` are the compiler's *Rust* `Dat`, not Flow-Cat arrows, and
  never re-describes Level A. Reviewers reject any DESIGN section that models a
  Flow program *as* a category at Level B (that is Level A's job) or that restates
  `category-ir.md`.
- **`Loc`/`Trm` degeneracy is stated, not fought.** Component DESIGNs for the
  frontend (syntax, ir, lower, check, interp) declare the physical pair degenerate
  and model in `Dat` + `Alg` only — this keeps those sections lean. The
  backend/runtime DESIGNs (llvm, cuda, verilog) are the *only* place `Loc`/`Trm`
  appear with content (target locations, host↔device transmissions); that is where
  Coherence Laws 1–2 do real work (no data teleport; typed crossing). A future
  backend ADR will own the strategy-2-category framing of the parallel target
  functors; this ADR only reserves the seam.
- **Every new/touched DESIGN.md gains a lead model section.** Existing component
  DESIGNs (syntax, ir, lower already written) are reconciled opportunistically —
  the model section is added when a crate is next edited, not in a big-bang
  rewrite (YAGNI, FRAMEWORK §5). The interp DESIGN (next session, P3) is the first
  to lead with the section from the start.
- **The reconcile gate grows one checkbox, not a new process.** The §8 coherence
  line rides the existing doc-reconcile step; it does not add a separate review
  pass. Verification fan-out stays cheap (FRAMEWORK §6 orchestration note).
- **Reversible.** This is methodology, not code or spec. A superseding ADR can
  retire or amend it with zero migration; nothing in the compiler depends on it at
  build time.

## Spec impact (exact files/sections to patch)

**Frozen Level-A spec: untouched.** This ADR is *methodology* for Level B (the
compiler's own design docs), and it explicitly does **not** touch Level A
(`docs/spec/category-ir.md`, `user-guide.md`, `ERRATA.md`), which remains the
frozen authority for Flow-Cat. No `category-ir.md` section, no ERRATA entry, no
LC-code — **patched: n/a** (no spec text changes).

**Methodology / process docs patched (not spec):**

- `docs/architecture/categorical-model.md` — **new** (the binding statement of this
  layer). patched: yes.
- `docs/architecture/INDEX.md` — **new** (the model index). patched: yes.
- `HANDOFF.md` §7.1.5 — every component `DESIGN.md` MUST lead with a
  `## Categorical model (Dat + Trn)` section. patched: yes.
- `HANDOFF.md` §7.2 step 7 — the doc-reconcile gate gains the FRAMEWORK §8 coherence
  line and the "update the model section + morphism table in the same change" rule.
  patched: yes.
- `HANDOFF.md` §2.1 / §2.2 — `FRAMEWORK.md` added to the corpus table and authority
  order (Level-B modeling; defers to accepted ADRs on spec-touching questions).
  patched: yes.

These are process docs (the development method), not the frozen Level-A specification;
all are reversible by a superseding ADR with zero build-time impact.
