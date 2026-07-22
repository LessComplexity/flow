# plan.md — Critique-mitigation pass (post-review, 2026-07-18)

Origin: 8-dimension honest review of Flow (syntax, spec, methodology, frontend, core,
backend, process, competition). Owner confirmed the action items match their own
thinking and authorized execution. This plan is the audit trail.

## Stage 1 — parallel mitigations (7 workers, non-overlapping file scopes)

| #   | Worker             | Scope (exclusive)                                                             | Deliverable                                                                                                                                                                                                                                             |
| --- | ------------------ | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | docs-hygiene       | `docs/spec/getting-started.md`, `docs/architecture/INDEX.md`                  | Fix the taught-but-rejected `seq` example (ADR-0019/LC-5 form); sweep the file for other taught-but-rejected syntax; repair stale INDEX metadata (S10 line, planned-row count)                                                                          |
| 2   | user-guide-badging | `docs/spec/user-guide.md`, `examples/vector.flow`                             | Badge every section/example as Core-compilable vs Core+1 (parses, named P-code rejection) vs aspirational; mark `vector.flow` with an explicit aspirational-dialect header                                                                              |
| 3   | related-work       | `VISION.md`, `docs/notes/related-work.md` (new)                               | Add citations (Compiling to Categories, DaCe, Vericert, Halide formal semantics, Futhark, SCADE/CompCert); correct the O4 "no incumbent makes this claim" cell; keep VISION's non-binding voice                                                         |
| 4   | as-implemented-doc | `docs/spec/flow-as-implemented.md` (new)                                      | One authoritative description of the language _as implemented_: authority order (spec v0.2 + E1–E5 + LC-1–5 + ADR-0001…0021), guard-first loops, IO token, arrays incl. `Update`, rejected constructs summary                                           |
| 5   | governance-adr     | `docs/decisions/ADR-0022-*.md` (new), `HANDOFF.md`                            | Record: as-implemented doc = operative language index; "frozen" retired in favor of explicit authority order; Level-B (FRAMEWORK) maintenance frozen until it finds its first bug; related-work correction. Patch HANDOFF's "frozen" phrasing minimally |
| 6   | o2-differential    | `crates/flow-backend-llvm/tests/**`, `docs/components/backend-llvm/STATUS.md` | Add the acknowledged-open `-O2` differential row to the harness; run it; fix small emission bugs with regression tests, or pin + document precisely if larger                                                                                           |
| 7   | scale-plan         | `docs/notes/array-scale-plan.md` (new)                                        | Convert the three recorded scale walls (alloca-resident arrays → 8 MB stack; literal-store explosion → clang compile time; naive-copy `Update` O(k·n)) into a designed plan with options, recommendation, milestone placement                           |

## Stage 2 — orchestrator verification

Verify each artifact exists, spot-check against the actual code/spec, summarize.

## Explicitly deferred (not this pass)

- Implementing heap lowering / last-use in-place `Update` (milestone-scale; item 7 produces the plan).
- External-user feedback loop and governance — needs humans, not agents.
- `emit.rs` refactor and lower-proptest strengthening (recorded as known risk; not blocking).

## Standing constraints for all workers

- No `git` commands; leave the working tree for the session process to commit.
- Do not edit `docs/STATUS.md` or `docs/sessions/` (session-log domain) — except during an explicit session-`end` reconcile.
- The compiler is ground truth; where docs and code disagree, the code wins and the doc gets corrected.

---

# Phase 2 — ADR candidates + P6 start (2026-07-18, owner-directed)

**STATUS: COMPLETE — M3 reached (Session 15, 2026-07-21).** Stage 1 landed (ADR-0023/24/25
candidates + backend-cuda DESIGN); Stage 2 landed (4-lens design review, 22/22 findings
fixed); Stage 3 reconcile done (S14). **Sapir's implementation gate opened 2026-07-20/21
("go for the cuda backend"); the S15 implementation workflow ran to green: crate built
(115 tests), a second 4-lens review found 2 blockers + 4 majors (all fixed pre-GPU), the
M3 sweep ran oracle-equal on a fresh vast.ai RTX 4090 (640 nvcc compile-and-runs, zero
divergences; box destroyed after, ≈$0.25). Full record: `docs/sessions/2026-07-21-s15-p6-cuda-m3.md`.**

Owner directives: "record all ADR candidates" (TT, dynamic sizing, templates —
from the roadmap discussion) and "Start P6" (CUDA backend, M3). Per HANDOFF §7.1.5 and
the category-architect build flow, P6 starts model-first: DESIGN → adversarial design
review → implementation gate (Sapir). The plan artifact for P6 is
`docs/components/backend-cuda/DESIGN.md` (their DESIGN convention), not a forked plan doc.

## Stage 1 — parallel authoring (4 workers, non-overlapping files)

| #   | Worker                     | File                                                              | Deliverable                                                                                                                                                                                         |
| --- | -------------------------- | ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | ADRCandidate_DynamicSizing | `docs/decisions/ADR-0023-dynamic-sized-arrays-candidate.md` (new) | Candidate ADR: unknown-size arrays, heap in flow-rt, E3 reopen, zip/enumerate length semantics                                                                                                      |
| 2   | ADRCandidate_Templates     | `docs/decisions/ADR-0024-templates-candidate.md` (new)            | Candidate ADR: T1 monomorphic type templates at lower (no IR change), T2 size-parametric `[A; N]` joined to ADR-0023 design                                                                         |
| 3   | ADRCandidate_TT            | `docs/decisions/ADR-0025-TT-backend-candidate.md` (new)           | Candidate ADR: post-M5 third backend, TT-Metalium emission, O5 proof-case, ttsim CI                                                                                                                 |
| 4   | CudaDESIGN_Author          | `docs/components/backend-cuda/DESIGN.md` (overwrite stub)         | Model-first P6 DESIGN: lead `## Categorical model (Dat + Trn)` — the ADR-0022 D2 sanctioned exception, first real `Loc`/`Trm` pair; host/device execution mapping; traps; harness recipe on vast.ai |

## Stage 2 — adversarial design review (stage-gate, after DESIGN lands)

4 lenses (plan-type, read-only): oracle fidelity · CUDA/hardware realism · contract
coherence (ADR-0020 duties + Level-B section quality) · harness practicality.
Then: fixer applies confirmed findings; review record → `docs/components/backend-cuda/reviews/`.

## Stage 3 — orchestrator line-by-line read → present DESIGN to Sapir (implementation

gate) → session `end` reconcile (S14 log, STATUS ledger + backend-cuda row, next-session).

## Deferred

P6 implementation proper (awaits gate); STATUS capability-matrix CUDA cells flip at
implementation; ADR-0023/24/25 remain candidates until Sapir decides.
