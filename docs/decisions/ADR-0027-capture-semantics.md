# ADR-0027: Capture semantics — map/fold bodies may read enclosing bindings

Date: 2026-07-21 · Status: **accepted — ratified by Sapir 2026-07-21 (S17)**, as proposed (D1–D3 + D2b; Q1–Q5 resolved per the Ratification note)
Motivation: `docs/notes/bench-matmul.md` finding #3 (the one-kernel GEMM is **unwritable** in Core today) · Sapir's parallel-first directive (S16) · companion to `components/backend-cuda/plans/plan-region-emission.md` (the strip→partition→emit work is downstream of this).

## Context (what forced the question)

Since M0, map/fold/zip inline bodies are **not closures**: lower rejects any reference to an enclosing local inside a body with **L1108** ("blocks are not closures" — `flow-lower/src/typing.rs:642,1236`; a body sees only its parameters). The rule kept bodies trivially pure and made kernel synthesis simple — and it is the precise reason the natural data-parallel matmul cannot be written:

```
// The shape the bench proved unreachable (P0117-free, valid Core except L1108):
(a, b) -> {
  enumerate -> map { (t, _) ->
      i = t / N; j = t % N;
      krange -> fold { (acc, k) -> acc + a[i*N+k] * b[k*N+j] }   // reads a, b, i, j — L1108 today
  }
}
```

Every alternative the current language offers is worse: the flattened sequential loop with per-element `Index`/`Update` (the S16 bench — Θ(N³) kernel launches at ~24 µs each), or N² explicit index reads with no fold. The S16 evidence: the *same algorithm* as one kernel is 3.8M× faster at N=64. Meanwhile purity — the property L1108 was protecting — is independently guaranteed by **E2** (no effects in fanout bodies, L1605 token-freedom), not by the no-capture rule: a body that *reads* an enclosing binding is still pure.

## Decision (proposed; awaits Sapir)

**D1 — Map/fold/enumerate-body blocks may read enclosing bindings (pure read captures).** Mutation of a captured binding stays rejected; effects stay rejected (E2/L1605 unchanged); the captured set is the body's free variables, computed by lower at lowering time (compile-time, deterministic).

**D2 — Realization: graph linking (broadcast edges), no new IR op.** A body that captures `x₁…xₖ` is lowered as a body fn whose input product is `(captures…, original-input)` — in graph terms, the enclosing producers gain **edges into every body instance**: a capture is a *broadcast edge* (one producer, N consumers), the same legal fanout the IR already trusts everywhere. Interp evaluates with the enclosing values substituted (byte-parity by construction); backends thread the captured handles/scalars as extra kernel/twin arguments (the parameter machinery already exists).

**D2b — Legality is a graph property, not a syntax rule (Sapir, S17).** Whether a capture is permitted is *deduced from the elaborated graph*: a capture is legal iff **no data path leads from the fanout's output back into the captured value's binding** (the fanout must never influence what it reads — plain reachability), and a captured name is never a **rebind target** inside the body. Because values are immutable and `Update` produces a fresh array, rebinding edges are the only mutation surface — so this check is complete, and a violation is reported *as the path* (the read edge + the write edge, with spans): "the fanout would read what it produces." The same graph view demotes op-primitivity for analysis: `map`/`zip`/`enumerate` are parameterized fanout (one body subgraph quantified over n — analyses descend uniformly, they do not elaborate to n nodes), while `fold` is a dependence chain (or a tree, if associative — the `reduce` candidate's question); parallel-vs-series is *read off* the graph, not decreed.

**D3 — L1108 narrows, and the diagnostic must teach.** The code remains for the genuinely illegal cases (capturing for *mutation*, any effect — already E2) — and its message must name the variable, state why (per-element parallel instances; shared reads must be captures), show the legal form, and, for the mutation case, print the offending graph path (D2b). "Blocks are not closures" fails this bar (Sapir, S17: "a shitty not understandable error").

## Semantics notes (the rules a ratified version must keep)

- **Read-at-position:** a captured binding denotes its SSA value at the map/fold site's position in the enclosing dataflow (a loop-mut variable captured inside a loop body sees the current iteration's value — identical to passing it explicitly; deterministic, no aliasing questions because values are immutable and arrays copy-on-`Update`).
- **Purity preserved:** a capturing body is still token-free, so kernels remain effect-free by construction (E2's "no effects in kernels" corollary survives).
- **Oracle parity:** the oracle substitutes the same enclosing values — there is no new evaluation rule, only new parameter passing; the differential contract (R1, raw+rewritten) is untouched.
- **Determinism:** the capture set is a pure function of the parse tree; lowering order of the hidden components is source-order of first use.

## Alternatives weighed

- **Explicit cartesian/broadcast ops** (`cartesian : ([A;n],[B;m]) → [(A,B); n·m]` + a broadcast lift): no closures, but new Core ops, new surface forms, and the body still needs the *whole arrays* — the cartesian materializes n·m pairs, a worse memory story than a captured read. Rejected as the primary mechanism; may still be worth its own candidate later for genuinely cartesian workloads.
- **Status quo / bigger primitive set** (a `dot`/`gemm` builtin per shape): a builtin zoo — exactly the premature-consolidation failure mode; the language should say the general thing (captures), not name each kernel.

## Consequences if accepted

- **Unlocks:** the one-kernel GEMM (the S16 acceptance example), stencil pipelines (sepia with neighbour reads — the thesis artifact's natural form), broadcast patterns, body-local reuse of enclosing constants.
- **Per-component impact:** syntax (none — the grammar already parses the references; only the diagnostic narrows) · lower (free-variable analysis, hidden-parameter wiring, L1108 narrowed) · ir (none — no new op kind) · interp (body calls pass the extended input) · check (E2 walk unchanged) · rewrite (capture-aware CSE/DCE; the `inline` pass from the region plan interacts) · backends (kernel/twin signatures gain captured args — mechanical) · docs (user-guide §5, flow-as-implemented.md, the L-code catalogue).
- **Differential duty:** capture-heavy testgen cases (capturing map/fold bodies over arrays, incl. loop-mut capture-at-position) added to the random-program generator.

## Non-goals

- Mutable captures (write to an enclosing local from a body) — rejected by construction (breaks purity/dataflow; not needed by any motivating example).
- Parallel reduction semantics (the canonical-tree `reduce`) — separate candidate.
- The `par` loop annotation / independence proof — separate candidate.
- Closures as first-class values / partial application — out of Core scope (E3-class).

## Open questions (→ Sapir)

- Q1: read-only captures ratified as above, or an even narrower first step (captures of *array* bindings only, scalars already fine)?
- Q2: implicit free-variable capture (proposed) vs an explicit `capture (a, b) in map { … }` annotation (more visible, more syntax)?
- Q3: nesting — bodies capturing through multiple enclosing body levels (map in a map body's fold)? Allowed transitively (proposed) or depth-1 only?
- Q4: interaction with ADR-0024 templates (captured type variables at Pass 0) — any conflict?
- Q5: should the capture set surface in diagnostics/IR dumps (visible hidden params) or stay implicit?

## Spec impact (on acceptance)

`flow-as-implemented.md` (capture rule + examples), user-guide §5 (map/fold bodies), the L-code catalogue (L1108 narrowed), a living-correction entry; the S16 bench note's finding #3 closed by implementation + differential.


## Ratification note (2026-07-21, Sapir — S17)

Ratified as proposed ("yes I ratify"), with the S17 discussion folded in beforehand: D2 realized as **broadcast edges** (graph linking), D2b (**legality is a graph property** — no path from a fanout's output back into a captured binding; no captured rebind targets; violations reported as the path), and the D3 diagnostics bar (name the variable, show the legal form, print the path — "blocks are not closures" fails it). Open questions resolved per the proposal: **Q1** read-only captures (mutation/effects stay rejected); **Q2** implicit free-variable capture (no annotation syntax); **Q3** transitive (bodies may capture through enclosing body levels); **Q4** no conflict with ADR-0024 recorded (revisit at templates' design); **Q5** the capture set is implicit in source but **visible where it matters** — named in diagnostics and shown in IR dumps (no new surface syntax). Implementation lands pipeline-wide as one increment: ir (Map/Fold op typing + builder + validate) → lower (free-variable analysis, hidden-parameter wiring, L1108 narrowed + the new message) → interp (extended body calls) → both backends (thread captured args) → testgen (capture-heavy programs) → differential + docs (Spec impact list). Plan: `components/ir/plans/plan-capture-semantics.md`.
