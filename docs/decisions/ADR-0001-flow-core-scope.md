# ADR-0001: Flow-Core (v0.3 subset) is the frozen implementation scope through M5

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

The v0.2 corpus describes a full general-purpose dataflow language: coproducts and
`Option`/`Result`, the `?` operator, recursion, closures as values, channels, executor
definitions, hardware annotations, modules, strings as data, and dynamic collections.
Attempting to implement all of it at once is the project's single biggest risk —
another year of breadth with no working artifact. The thesis artifact (M5) only needs
one source file to run identically on CPU, GPU, and FPGA simulation. The implementation
therefore needs a frozen, minimal subset that is large enough to express `sepia.flow`
and friends and small enough to finish, with everything else rejected loudly rather than
half-supported. HANDOFF §4 defines that subset (Flow-Core v0.3); this ADR records it as
the binding scope so no session quietly widens it.

## Decision (one paragraph, imperative)

Implement exactly Flow-Core v0.3 as defined in HANDOFF §4: in scope (§4.1) are the scalar
types `i32 i64 u8 f32 f64 bool`, tuples, named product types (`type` after E5), fixed-size
arrays `[T; N]`, string literals only as `print` arguments; arithmetic/comparison/boolean
operators, member access, indexing (bounds-checked), construction and literals; `->`/`<-`
flow statements and pipelines; non-recursive functions with an acyclic call graph; pure
guard-block conditionals lowered to Phi; labeled loops with scalar/tuple carried state
lowered to Trace under E1; pure parallel fanout with implicit join and `seq` for ordering;
`print` as the only effect, sequential-context-only per E2; and `map`/`fold` over fixed
arrays with inline non-first-class block bodies. Everything in §4.2 — dynamic arrays/slices,
strings as data, coproducts/`Option`/`Result`/`?`, recursion, closures, channels, executors,
hardware annotations, modules, and any `category`/`type` declaration beyond product types —
is out of scope and MUST be rejected with a clear diagnostic, never silently accepted; any
scope change requires a superseding ADR.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: Flow-Core deliberately omits the coproduct machinery that is central to the
  categorical story; that capability returns first in Core+1 (after M2), so the categorical
  exposition is temporarily ahead of what the compiler accepts.
- Implementation: every front-end stage gets a "reject-with-reason" path for out-of-scope
  constructs; the diagnostic must name the construct and that it is post-Core, not emit a
  generic parse error. This is testable and keeps scope honest.
- The acyclic-call-graph and no-recursion restriction lets the interpreter and lifetime
  engine stay first-order and exactly correct (see ADR-0004); recursion is a CPU-only Core+1
  feature.
- The backend capability matrix (§4.3, maintained in global STATUS.md) carries one of
  `supported / rejected-with-error / planned` per feature×backend; the Verilog column is
  restricted to feedforward pipelines + single-loop FSMs and must reject the rest cleanly.

## Spec impact (exact files/sections to patch; patched? n/a)

No spec file is patched: Flow-Core is an implementation scope decision layered on the v0.2
corpus, not a correction to it. The binding definition is HANDOFF §4.1/§4.2 and the
capability-matrix policy §4.3. patched? n/a.
