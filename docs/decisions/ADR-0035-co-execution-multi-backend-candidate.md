# ADR-0035: Co-execution — one source, several backends at once; `Trm` as typed cross-backend transmission (candidate)

Date: 2026-07-25 · Status: **candidate — direction stated by Sapir (2026-07-25); NOT scheduled** · number provisional · changes nothing until accepted. Related: ADR-0014 (FRAMEWORK Level-B, `Loc`/`Trm`), ADR-0030 (external-backend protocol), ADR-0025 (TT backend), ADR-0023 (dynamic arrays / allocator amendment), VISION §3 (O8), `docs/notes/2026-07-25-thesis-review.md`.

## Context (what forced the decision)

Sapir, 2026-07-25: *"later we can compile a source into multiple backends at the
same time to facilitate communication on the same machine (e.g. cpu-cuda-fpga)
and backend specific overrides for portability (stdin, trap, etc…)."*

This is a **stronger claim than the one VISION currently sells.** VISION §3's
options are all forms of *portability* — the same source runs on target A, or on
target B (O1/O5/O7). Portability is contested ground: DaCe, XLA/PJRT, SYCL,
Mojo/MAX are all there. **Co-execution** — one program whose parts are placed on
*several* targets simultaneously, with the transmissions between them typed and
cost-visible in the same IR — is a different claim, and it is the one Flow is
structurally built for and others are not:

- `Loc`/`Trm` are already first-class in the design method (FRAMEWORK §0/§4.2),
  not bolted on. ADR-0014 records that the backend/runtime seam *is* the one real
  `Loc`/`Trm` pair; co-execution is that seam used in earnest instead of once at
  the boundary.
- Placement is already a deduced query, not an annotation: `path_plan` cuts the
  graph into independent tasks (S24); `tile_plan` records geometry per site.
  Choosing a `Loc` per task is the same kind of decision, one level up.
- Effects are a linear token chain, so cross-`Loc` ordering has a carrier that
  already exists and is already enforced.
- The IR has no aliasing, so "what must be transmitted when this task moves" is
  an exact reachability answer, not an alias analysis.

The heterogeneous-machine problem (CPU + GPU + FPGA in one box, moving data
between them correctly and not too often) is real, unsolved for most people, and
is exactly where a dataflow IR with explicit placement should win. Today the
framework models it and nothing exercises it: every compiled program is
single-`Loc` (plus the host↔device split inside the CUDA backend, which is
hand-written per backend rather than deduced).

**Honest status:** this is a post-M5 direction, not a next increment. It is
recorded now because (a) it changes what VISION's north star should say, and
(b) two nearer decisions — the backend protocol (ADR-0030) and the host-effect
seam (D4 below) — are cheaper if made with it in view.

## Decision (recommended shape, if accepted)

**D1 — Placement is a deduced-then-chosen query, not an annotation.** A
`place_plan` (successor to `path_plan`) assigns each task a `Loc` from the set of
backends the build targets. Deduction supplies the *legal* placements and the
*costs*: which tasks are bulk-parallel (GPU-shaped), which are order-pinned
sequential chains (CPU-shaped), which are streaming/feedforward (FPGA-shaped —
the recognizer already distinguishes these, `tile-ladder-direction.md` §FPGA);
what each placement would cost in transmission is the edge cut. The *choice* is
policy (heuristic, cost model, or user override), never re-derivation. The `@`
hardware annotations reserved at P0102/P0111 (`executor` declarations) are the
override surface if one is wanted — they are already parsed and rejected, so the
syntax is held.

**D2 — A transmission is a first-class morphism with a cost, not an implicit
copy.** A cut edge between two `Loc`s lowers to an explicit `Trm` node carrying
what moves and where. Consequences: host↔device copies become *visible in the
graph* and therefore optimizable by the same machinery as everything else (fuse
to avoid a round-trip; keep a value resident — the `Loc`-level twin of S27b's
panel residence); the existing CUDA backend's hand-written host/device split
becomes an instance of the general mechanism rather than a bespoke one.
FRAMEWORK Law 2 gives well-typing of the transmission for free.

**D3 — The build artifact is multi-target.** `flow build --target cpu,cuda`
produces one program with several emitted texts plus the orchestration that runs
them together — not N independent binaries. This is where ADR-0030's protocol
and this ADR meet: an external backend must be able to be *one participant* in a
co-executed program, so the protocol's bundle needs a placement/transmission
section, and that is much cheaper to design now than to retrofit.

**D4 — Backend-specific overrides are the portability seam (`stdin`, traps, and
the rest of the host surface).** A backend declares which host services it
provides; the language declares the effect, the backend provides the realization
or declines it (the ADR-0032 D3 capability-matrix pattern, applied to effects
instead of precision). Two immediate items, both real today:

- **`stdin` / input effects reopen the token model.** `Ty::IoToken` is currently
  output-only: `Print : (IoToken × P) → IoToken` (ADR-0013). An input effect is
  `Read : IoToken → (IoToken × A)` — the token machinery handles it, but the
  *shape* is new (a token-carrying value production), and E2/E3 want re-reading
  against it. **This is an ADR, not "just implementation"** — small, but it must
  be written before someone adds a builtin.
- **Traps across `Loc`s.** The trap protocol is currently per-backend (llvm
  exit-101; CUDA exit-102 with a device flag checked after every launch;
  speculate-and-order for the parallel orchestrator, S24). Co-execution needs one
  protocol *across* backends — a trap raised on the FPGA leg must stop the CPU leg
  with the same class and the same ordering guarantee. The S24 speculate-and-order
  design is the precedent and probably generalizes.

**D5 — What this does not change.** Single-target builds stay exactly what they
are; co-execution is additive. The oracle relation is unchanged: a co-executed
program must still produce byte-equal stdout against the interpreter (R1,
ADR-0020) — which is a *strong* statement, because it means the placement is
provably output-irrelevant, and it is the natural extension of S24's "byte-equal
at any thread count" to "byte-equal at any placement." **That property is the
actual product.**

## Consequences

- **Repositions the north star.** VISION §7's O1 ("universal layer, earned never
  assaulted") is a crowded destination. O8 co-execution is a destination with
  fewer occupants and a structural reason Flow gets there first. Recorded as a
  VISION addition in the same change.
- **Raises the value of `Loc`/`Trm` from methodology to product.** FRAMEWORK's
  physical pair has so far been documentation discipline; this is the increment
  where it earns its place, which also answers the standing ADR-0022 question of
  whether Level-B is carrying weight.
- **Post-M5, and gated on real prerequisites:** modules (multi-file programs),
  dynamic arrays (ADR-0023 — cross-`Loc` buffers are not fixed-size literals),
  the second-consumer discharge (ADR-0033 — co-execution is meaningless if the
  second backend cannot consume the deduced queries at all), and a CLI that can
  express multi-target builds.
- **Risk — scope.** This is the largest thing in the ADR set. It is recorded as a
  direction so that nearer decisions do not foreclose it; it is explicitly not a
  licence to start.

## Open questions

- **Q1** — Is the placement choice automatic (cost model) or declared (`@`
  annotations / `executor` decls, P0102/P0111) in v1? Recommendation: declared
  first, deduced later — the legality query is the hard part and it is shared.
- **Q2** — What is the runtime? `flow-rt` currently owns a work-stealing CPU
  pool; co-execution needs a cross-`Loc` scheduler. Extension or new component?
- **Q3** — Does the input-effect ADR (D4) land earlier, standalone, since `stdin`
  is also what "generic language" needs regardless of co-execution? (Likely yes.)
- **Q4** — Does the byte-equal-under-any-placement gate survive contact with
  reduced-precision targets, or does it become the ADR-0032 two-face split at the
  `Loc` level (conformance placement vs product placement)?
- **Q5** — Does this subsume or conflict with ADR-0025 (TT backend as the O5
  proof case)? A TT leg co-executing with a CPU leg is a stronger demo than a TT
  leg alone.

## Spec impact

None yet (candidate). On acceptance: VISION §3/§7 gain O8 as the north star
(done ahead of acceptance as a recorded *option*, per VISION's non-binding
charter); FRAMEWORK §4.2 gains the co-execution instantiation; ADR-0030's bundle
schema gains a placement section; the input-effect ADR (D4) is spun out
separately.
