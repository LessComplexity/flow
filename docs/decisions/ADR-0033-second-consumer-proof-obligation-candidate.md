# ADR-0033: The backend-genericity proof obligation — CUDA consumes `tile_plan` before further CPU rungs (candidate)

Date: 2026-07-25 · Status: **candidate — NOT decided.** Sequencing is **decided by Sapir (2026-07-25): "cuda work will start right after we manage to take full advantage of this on cpu, later move on to gpu"** — the obligation below is written to that order; what is undecided is the interim guard (D3) and the discharge bar (D4/D5). Number provisional. Related: ADR-0032 (genericity contract), ADR-0034 (placement constants), suggestion #10 (`block_plan`, gated on this event), `docs/notes/tile-ladder-direction.md`, `docs/notes/graph-advantage.md`, `docs/notes/2026-07-25-thesis-review.md`.

## Context (what forced the decision)

The founding bet is not "Flow is fast on CPU." It is that **one geometric
deduction on the graph pays out on every backend** — `tile-ladder-direction.md`,
verbatim intent: *"cuBLAS/cuDNN-class performance out of the box, from naive
intuitive source, on every backend… the graph analysis deduces what the libraries
hand-encode."* ADR-0032 fixes the contract that makes this checkable: rungs land
as (a) generic graph facts in flow-ir queries or (b) emitter-local cashing with
zero flow-ir change; *"CUDA/Verilog consume the SAME record."*

The generality half is **proven**. The recognizer holds no matrix concept — one
affine-address rule (lane-coefficient 0 = broadcast, 1 = unit stride, else
refuse) matched matmul, FIR and attention; S28 carried the ladder to a FIR 1-D
window rung and a conv2d micro-kernel, both winning their tables, one with **zero
flow-ir change**. Shape-genericity is not a hope; it is a measured result.

The **backend** half has zero measurement. Verified 2026-07-25:

```
tile_plan consumers:  crates/backends/llvm/src/lib.rs, crates/backends/llvm/src/func.rs
crates/backends/cuda/src/:  no hits
```

`tile_plan` landed at S25. The last CUDA session was S23. Every rung since —
register blocking (S26), FMA contraction + packing + panel residence (S27/S27b),
k-split + window + conv micro-kernels (S28), the KC nest (S29, in flight) — has
had exactly one consumer. Suggestion #10 already names the consequence and gates
`block_plan` extraction on "when cuda consumes `tile_plan`" (rule of three), but
records it as a *trigger to wait for*, not as an *obligation to discharge*. Six
sessions have passed the trigger by.

Two risks compound while the second consumer is absent:

1. **Unfalsifiable thesis.** "This power transfers to all backends" is the
   load-bearing claim of the entire project, and it is currently architectural
   assertion. One CUDA leg converts it into a measured fact — or finds the seam
   where it fails, at rung 3 rather than rung 8.
2. **Silent CPU capture of flow-ir.** With one consumer, nothing tests ADR-0032's
   contract. A cache-hierarchy assumption can migrate into a "generic" query and
   no gate fires. The contract is only enforced by a second, structurally
   different `Loc`.

## Decision (imperative, if accepted)

**D1 — The trigger is CPU saturation, per Sapir's sequencing.** The CUDA leg
starts when the CPU ladder is declared done — full advantage taken on one
backend first, then the port. The obligation is therefore *not* a gate on the
next CPU rung; it is the **named exit condition of the CPU phase**, so that the
phase ends on a thesis test rather than trailing off. "Saturated" needs a written
bar (Q1) — otherwise the phase has no end and the second consumer keeps
receding, which is the failure mode this ADR exists to prevent.

**D2 — The interim guard: a paper second-consumer check per rung.** Because the
CUDA leg is deliberately deferred, each CPU rung landing in the meantime records
**in its plan doc, before it ships**, three lines: (a) which `tile_plan`/record
fields it consumes; (b) its CUDA realization named against the record (smem
staging = pack `DataLoc`; lane blocks = warp shape; k-panel = the same axis
split) — or the explicit admission that it has none; (c) any machine fact it
needed that the record does not carry. Cost: minutes, not a session. Value: it
catches ADR-0032 drift — a cache-hierarchy assumption migrating into a "generic"
query — at the rung that introduces it, instead of at the port. A rung whose (b)
is "none" is admissible and is marked **`cpu-local`** in the ladder; honest, not
forbidden. This is the whole mitigation for deferring the real leg, and it is why
deferring is affordable.

**D3 — Minimum viable leg, when it fires.** Shared-memory tiling on one
already-recognized matmul site: the pack `DataLoc` at the smem location
(`tile-ladder-direction.md` §per-backend), backend-owned tile factors
(warp-shaped, per ADR-0032 D4 / ADR-0034 D1), the existing differential duty
(ADR-0020) as the correctness gate. `mma` is **not** in scope for the discharge —
it is the precision-face rung (ADR-0032 D1/D3, Ampere+ tf32, product face only)
and follows once the memory rung is measured.

**D4 — What the leg reports.** Three answers, recorded in the session log
whatever they are: (a) which `TileRead` fields the smem emitter actually
consumed; (b) which facts it needed and had to re-derive locally — **each one is
an ADR-0032 violation and a `tile_plan` gap**; (c) the measured delta vs the
untiled CUDA path at one size, compute-only. A *negative* result (the record is
insufficient for smem) is a successful discharge: it names the missing fact.

**D5 — `block_plan` stays gated, and D3 discharges the gate.** Suggestion #10's
extraction trigger fires on the CUDA leg, not before — the second consumer is
what tells us which parts of the llvm nest are schedule (generic) and which are
`Loc` constants (backend). Extracting first is the premature abstraction
FRAMEWORK §5 forbids. Corollary: until then, the llvm nest is allowed to stay
hand-rolled; that is not debt, it is the deliberate wait for the third case.

## Consequences

- **Sequencing agreed:** CPU first. The genericity claim stays architectural
  assertion for the duration of the CPU phase, which is an accepted, bounded
  exposure — bounded *by D2*, which is the only thing making the deferral cheap.
  Without D2 the exposure is unbounded and grows per rung.
- **Cost of the leg itself:** one focused session on surveyed ground —
  `tile-ladder-direction.md` specifies the mapping; the recognizer, the record,
  the CUDA emitter and the 4090 harness all exist. Not research.
- **Falsifiability is still the point.** If the record turns out CPU-shaped, D2
  is what makes the discovery cheap: the offending rung is already named in a
  plan doc rather than excavated from `func.rs`.
- **Interacts with ADR-0030** (external backends): a second in-tree consumer is
  the dry run for third parties consuming the same deduced queries (its D3
  exports them across the boundary).
- **Interacts with ADR-0034:** the tuner is best written after two `Loc`s exist,
  for the same rule-of-three reason — so the CPU-saturation bar (Q1) should be
  read as "geometry rungs done and constants tuned *for this machine*," not "the
  OpenBLAS number is matched everywhere."

## Open questions

- **Q1 (the important one)** — What is the written bar for "full advantage taken
  on CPU"? Candidates: OpenBLAS parity within X× at N≥1024 f32/f64; or the
  geometry ladder complete (SIMD → TI blocking → packing → KC panels → tuned
  constants) regardless of the residual number; or a fixed session budget. Without
  one, D1 has no trigger.
- **Q2** — Does the discharge need the `f32` conformance face only, or both faces?
  (ADR-0032 D1 suggests conformance-only for the memory rung, since smem staging
  does not reassociate.)
- **Q3** — Is Verilog a *third* consumer obligation at P7, or does the
  systolic-array mapping (`tile-ladder-direction.md` §FPGA) count as sufficiently
  different that the contract is proven at two?
- **Q4** — Should D2's `cpu-local` mark be a field in the ladder doc, or a lint on
  plan docs?

## Spec impact

None (Level A untouched; this is realization-layer scheduling). On acceptance:
`docs/notes/tile-ladder-direction.md` gains the obligation line, suggestion #10's
Status cell cites this ADR, and the next-session build queue is reordered.
