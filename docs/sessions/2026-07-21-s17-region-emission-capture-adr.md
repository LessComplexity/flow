# Session 17 — 2026-07-21 — Design direction: parallel-first, region-based emission, ADR-0027

Orchestrator: Kimi Code · skill: category-architect. Immutable log (ADR-0017). Short design-direction session (same day as S16's benchmark, which it follows from). Scope set by three Sapir directives: (1) "optimization is key — performance tailored at ANY step"; (2) "the language has to be parallel first from the start"; (3) "not fusion — correct mapping" (+ the stripping insight: functions are a human construct; the optimizer's unit is the smallest primitive graph).

## 0. Continuation brief

Current state: **design direction recorded; no code changed; workspace 673 green; nothing live.** Two design artifacts written this session: `docs/components/backend-cuda/plans/plan-region-emission.md` (the backend v2 emission strategy: strip → partition → one kernel per region, with the matmul acceptance ladder 12.16 s → ~0.1 s → one kernel) and `docs/decisions/ADR-0027-capture-semantics-candidate.md` (map/fold bodies may read enclosing bindings — pure read captures as hidden extra parameters; L1108 narrows, E2 unchanged). Both await Sapir's read; ADR-0027 awaits his decision.
Next step: Sapir reviews ADR-0027 (and the parallel-first ADR package: canonical-tree `reduce`, `par` loops); then region-v2 implementation (inline pass → regions.rs → CUDA v2), or P7 Verilog with regions designed in.
Resume command/check: read `docs/next-session.md`; `cargo test --workspace` (expect 673).

## 1. Work completed

- **The honest reframing (recorded in docs/notes/bench-matmul.md):** category theory bought correctness *with structure* (the functor/R1 license to optimize safely, fusion for a narrow class, purity as the parallelism enabler) — not performance; the bench's costs are physical (launch/sync/PCIe), and only mapping strategy + expressibility close them. The CT payoff that *is* performance-relevant: E2's no-effects-in-fanout (kernels sound), associativity as the fold-parallelization gate, fusion laws.
- **Region-emission v2 plan** (`components/backend-cuda/plans/plan-region-emission.md`): three moves — (1) **strip**: an `inline` pass in flow-rewrite (graph substitution of calls, cost-model-bounded, R1-tested, inherited by all backends incl. the differential's rewritten leg); (2) **partition**: region formation on the primitive graph with explicit boundary rules (effects/token, host-consumed scalars cross once at region end, loop scopes, kept calls) and per-backend cost models (CUDA launch/sync prices vs Verilog stage sets); (3) **emit**: one kernel per region, traps re-grained per region launch, ownership/escape machinery unchanged. The matmul acceptance: N=64 goes 12.16 s → ~0.1–0.2 s (mapping alone, N² kernels) → naive-CUDA class (one kernel) with independent-iteration proof.
- **ADR-0027 candidate** (capture semantics): pure read captures in map/fold bodies, realized as hidden extra body-fn parameters (no new IR op; oracle parity by construction); L1108 narrows to mutation/effect cases; alternatives (cartesian ops, builtin zoo) weighed and rejected; Q1–Q5 for Sapir.
- **Directives recorded:** perf-per-step gate (bench note + next-session item 0), parallel-first package (next-session 2b: capture / canonical-tree `reduce` / `par` loops — all additive, oracle-pinned, byte-parity-preserving), region mapping as standing directive 0(b) + backend-cuda suggestions #0. P7 note: design regions in from the start (a feedforward pipeline is one streaming region), share the machinery, backport to CUDA.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Where stripping lives | **flow-rewrite** (a plan+replay `inline` pass) | R1-property-tested; every backend + the differential's rewritten leg inherits it; backends stay thin |
| Region formation | backend-independent analysis consuming a per-backend cost model | CUDA and Verilog partition with the same algorithm, different price vectors — build once |
| Capture realization | hidden extra body-fn params, **no new IR op** | smallest delta; interp parity by construction; kernel/twin arg machinery already exists |
| `fold` vs `reduce` | keep both (`reduce` = canonical-tree, oracle-pinned) | additive, non-breaking; float non-associativity stops blocking parallel reductions while byte-parity holds |
| v1 emitter | stays as reference until v2's differential matches on the full corpus | R1 guards the remap; retire-or-flag decided at the v2 gate |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace` | 673 green (S16 close; no code changed this session) | design-only session, no regressions |
| (S16, referenced) | flow-cuda matmul 12.16 s @64 vs naive-CUDA 0.0032 ms | the evidence the v2 plan is sized against |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | **uncommitted** (S14–S17 work; Sapir owns commits) | `git status` |
| machines | vast.ai | nothing session-provisioned (S15/S16 boxes destroyed); 45170851/52/45181070 running — **Sapir-declared unrelated; do not use or destroy** | `vastai show instances` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **Sapir: decide ADR-0027 (captures)** | `decisions/ADR-0027-capture-semantics-candidate.md` (Q1–Q5) | Sapir reads + decides/amends | status flips from candidate |
| P0 (S18) | Region v2 implementation (after 0027, or standalone) | `components/backend-cuda/plans/plan-region-emission.md` | `inline` pass → `regions.rs` → CUDA v2 + structural perf gates | matmul64 ≤ 0.3 s, then 1-kernel with `par`; differential green |
| P1 | Parallel-first package: canonical-tree `reduce` + `par` loop candidates | next-session 2b | orchestrator drafts on Sapir's word | two ADR candidates exist |
| P1 | P7 backend-verilog (M4) — regions designed in | `components/backend-verilog/` stub + the region plan §5 | model-first DESIGN | M4 line green on verilator |
| P2 | Perf-gate harness (structural launch/transfer counts as tests) | region plan §6 | formalize `benches/matmul/` + gates into the test suites | gates run in CI |
| P2 | ADR-0024 decision; coproducts ADR on Sapir's word | `decisions/` | Sapir answers / orchestrator writes | status flips |
| P3 | interp `read-before-write` panic on exit-only-payload shape (pre-existing) | `flow-interp/src/eval.rs:297` | reproduce; fix or document | pinned |

## 6. Architecture / model changes

None in code (design session). Recorded direction: the backend's emission unit moves from *morphism* to *region* (DESIGN v2 plan); functions become a presentation detail the `inline` pass strips before region formation; the capture question is promoted to ADR-0027 (language level). The R1/oracle architecture is what makes the remap safe — the differential is already green at v1 and is the acceptance test for v2.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/backend-cuda/plans/plan-region-emission.md` | new — the v2 emission strategy |
| `decisions/ADR-0027-capture-semantics-candidate.md` | new — capture semantics candidate (Q1–Q5) |
| `components/backend-cuda/suggestions.md` | #0 region-based emission (Sapir's mapping directive); #11 NVRTC/PTX |
| `docs/notes/bench-matmul.md` | perf-per-step directive; toolchain (PTX/NVRTC) note |
| `docs/suggestions.md` | roll-up row 8 (S16: 11 rows, capture headline) |
| `docs/next-session.md` | directives 0(a)/0(b), parallel-first package 2b, P7-regions note, S17 close |
| `docs/IMPLEMENTATION.md` | the two `emit` examples |

## 8. Files changed

Design docs only (§7) — no code. Uncommitted (Sapir owns commits).

**Next `start` path:** read `sessions/2026-07-21-s17-region-emission-capture-adr.md` (this log) → `docs/next-session.md` → ADR-0027 decision → region v2 or P7.
