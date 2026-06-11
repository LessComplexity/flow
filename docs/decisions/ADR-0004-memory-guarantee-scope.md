# ADR-0004: The zero-annotation memory guarantee is scoped to the first-order non-cyclic core (E3)

Date: 2026-06-11 · Status: accepted

## Context (what forced the decision; spec refs)

`user-guide.md` §6.5 claims, for the whole language, that the compiler guarantees no
use-after-free, no double-free, no leaks, and no data races on heap data — "with zero
annotations" — by inferring lifetimes from the graph's last-use frontier. That promise is
not defensible at general-purpose scope. Whole-program region/lifetime inference over
closures, channels, and cyclic structures is historically treacherous (cf. the Tofte–Talpin
region pathologies), and the spec already concedes the hard case: §6.5 notes that cyclic
data structures need an explicit annotation switching them to reference-counting. A guarantee
that silently excludes its own hardest case is not a guarantee; it is an aspiration stated as
a theorem. The claim must be scoped to where it is actually true.

## Decision (one paragraph, imperative)

State the memory-safety guarantee as **proven for the first-order, non-cyclic dataflow core**
— which contains Flow-Core entirely — and **open for the full language**. Amend
`user-guide.md` §6.5 to draw that boundary explicitly: within the first-order non-cyclic core
the four properties (no use-after-free, no double-free, no leak within a function body, no
heap data race) hold with zero annotations by last-use-frontier analysis; outside it
(closures, channels, cyclic structures), safety is an open problem and cyclic types continue
to require the explicit reference-counting annotation. Build the Flow-Core lifetime engine to
be *exactly right* on its domain rather than approximately right everywhere.

## Consequences (tradeoffs, implementation impact)

- Tradeoff: Flow can no longer advertise unconditional whole-language memory safety; the
  honest claim is scoped. This is a strictly better position — a small exact guarantee beats
  a large unprovable one.
- Implementation benefit: because Flow-Core is first-order, non-cyclic, and uses only
  fixed-size data, the lifetime engine is simple and decidable — stack/static allocation for
  fixed-size values, last-use-frontier free insertion for arrays — and can be verified exactly
  against the interpreter's allocation/free trace.
- The out-of-scope cases (closures, channels, cycles) are all already post-Core (HANDOFF §4.2),
  so nothing in the M5 path depends on the unproven region.
- Reopening the full-language guarantee later is itself ADR-gated; it does not happen by drift.

## Spec impact (exact files/sections to patch; patched? yes — Session 01)

`docs/spec/user-guide.md` §6.5 — guarantee rescoped to the first-order non-cyclic core
(contains Flow-Core); full-language case marked open; cyclic-type refcount annotation
retained. Marked `> **Erratum E3 applied — see docs/spec/ERRATA.md and ADR-0004.**`.
patched? yes — Session 01.
