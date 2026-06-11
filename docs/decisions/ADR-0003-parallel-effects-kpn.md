# ADR-0003: Effects are forbidden in parallel fanout; channels use Kahn process-network semantics (E2)

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

`user-guide.md` §5.4 (the parallelism decision table), row 4, reads: "Independent +
effectful → Executor decides (may parallelize with non-deterministic order)." This makes
the *meaning* of a program depend on the scheduler. That directly contradicts two of Flow's
load-bearing promises. First, "no data races by construction": if two effectful branches may
run in nondeterministic order, their observable interleaving is a race the language claims
not to have. Second, the functorial-correctness story: a backend is a functor that must
*preserve* a program's denotation, but if the denotation is defined as "whatever the executor
happened to do," there is nothing stable for the functor to preserve and correctness becomes
unstatable. Channels themselves are out of Flow-Core scope (HANDOFF §4.2), but the *rule*
governing effects must be fixed now so the effect checker is built correctly the first time.

## Decision (one paragraph, imperative)

Forbid effectful morphisms in parallel fanout entirely. Effects must either (a) be sequenced
explicitly with `seq`, or (b) communicate through **channels with Kahn process-network (KPN)
semantics** — blocking reads on unbounded FIFOs — under which determinism independent of
scheduling is a theorem (Kahn 1974). Build the effect checker now to enforce that no
effectful morphism appears as a branch of an implicit-join fanout; in Flow-Core the only
effect is `print`, so `print` is legal only in sequential context. Reserve for the later
streaming/FPGA subset the stronger synchronous-dataflow restrictions (Lee & Messerschmitt
1987) that yield static schedules and bounded buffers. Replace §5.4 row 4 with the
effects-not-permitted-in-parallel rule.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: programmers lose "fire-and-forget parallel side effects"; they must say `seq`
  (or, post-Core, use channels). This is intentional — observable meaning may never depend
  on scheduling (a sacred project invariant), so a flaky test is a semantics bug until proven
  otherwise.
- Implementation (now): `flow-check` runs an effect analysis that rejects any effectful node
  in a parallel-fanout branch with a clear diagnostic; `print` in fanout is an error pointing
  the user at `seq`. This is testable in Flow-Core today even though channels are not built.
- Implementation (later): channels, when they arrive post-M5, are blocking-read/unbounded-FIFO
  KPN nodes; the FPGA streaming path then narrows them to SDF for bounded buffers. The checker
  is shaped now so that addition is additive, not a rewrite.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/user-guide.md` §5.4 — decision-table row 4 replaced: effectful morphisms are not
permitted in parallel fanout; they sequence via `seq` or communicate via KPN channels. Marked
`> **Erratum E2 applied — see docs/spec/ERRATA.md and ADR-0003.**`. patched? yes — Session 01.
