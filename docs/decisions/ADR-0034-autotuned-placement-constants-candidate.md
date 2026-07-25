# ADR-0034: Placement constants are searched, not set — the autotuner over `tile_plan` (candidate)

Date: 2026-07-25 · Status: **candidate — NOT decided** · number provisional · changes nothing until accepted. Extends ADR-0032 D4 (backend config = performance tailors). Related: `docs/notes/tile-ladder-direction.md`, `docs/notes/2026-07-25-thesis-review.md`, suggestion #10 (`block_plan`).

## Context (what forced the decision)

The tile ladder splits cleanly into two things that have been treated as one:

| | What it is | Where it comes from | Portable? |
| --- | --- | --- | --- |
| **Geometry** | which reads broadcast vs stride, which axes split, the nest order, what is legally interleavable | **deduced** from the graph — exact, cheap, backend-independent (`tile_plan`, `TileRead.ksplit`, `path_plan`) | **yes** — this is the thesis, and it is proven (matmul → FIR → conv2d, one rule) |
| **Constants** | `TILE_J=16`, `TILE_I=4`, `TILE_KC=128`, `NC=TJ×32`, k-unroll ×2, prefetch distance, `GRAIN` | **hand-set literals** in `crates/backends/llvm/src/func.rs`, arrived at by manual sweep | **no** — they are facts about a cache hierarchy and a register file, not about the program |

ADR-0032 D4 already draws this line correctly ("tile factors … per-backend tuned
tables as defaults, overridable per project; placement knobs are value-invariant,
the differential gates remain valid under every config"). What it does not yet
say is **how the tables get their values**. Today: a human sweeps. S26's record
is explicit — *"TI sweep 2/4/8 → 4 (8 spills)"*, on one machine. S27's per-width
TJ (f32 16 / f64 8) is the same procedure repeated. S28's box run measured GRAIN
quantization by hand (fir 61T 0.526 → 16T 0.287 — 16 slices = 0.26 waves).

Three consequences of leaving it manual:

1. **The constants are wrong on every machine but the one swept.** The local sweep
   was an M4 Pro (NEON, 14 threads); the box is an EPYC 7B13 (zen3, AVX2, 61
   threads quota). Both run the same literals.
2. **It is the residual gap.** BLIS/OpenBLAS's margin over a competent generic
   tiler is mostly per-microarchitecture parameters plus per-ISA microkernels —
   the measured 10–14× at f64@1024 is not evidence that the geometry is wrong. It
   is largely evidence that the constants are untuned for the target.
3. **It scales badly by hand and perfectly by machine.** Each new backend, each
   new element width, each new arch multiplies the sweep. A search does not care.

Crucially, this does **not** weaken the genericity thesis — it completes it. The
*search procedure* is as backend-generic as the geometry: one tuner, driven by
the recorded `tile_plan`/`block_plan` facts, emitting a per-`Loc` constant table.
Deduce the shape once; measure the sizes per machine. That is the honest form of
"optimal out of the box on every backend."

## Decision (imperative, if accepted)

**D1 — Constants live in a per-target table, never in emitter source.** Every
placement knob currently a literal in `func.rs` moves behind a
`PlacementConfig` (loaded from a per-backend/per-arch table, overridable per
project per ADR-0032 D4). Emitters read the table; they do not embed numbers.
Value-invariance is the entry criterion: a knob that can change an output bit is
a precision contract (ADR-0032 D1) and is not admissible here.

**D2 — The search is one generic driver, not a per-backend script.** A tuner
walks the recorded geometry (which knobs exist for a site is a *fact of the
record*, not of the target), enumerates a bounded candidate set, compiles and
measures compute-only per the standing measurement rules, and writes the table.
The same driver serves llvm, cuda and verilog because it consumes the same
record; only the candidate ranges and the timing seam are per-`Loc`.

**D3 — Search is offline and cached; the compiler stays deterministic.** Tuning
is an explicit invocation (`flow tune`, CLI item), not a side effect of `flow
build`. A build with a given table is reproducible and byte-stable. A missing
table falls back to the shipped defaults — today's literals become the seed row,
not the mechanism.

**D4 — The oracle gate is unchanged and is what makes this safe.** Because knobs
are value-invariant by construction, every candidate in the search is checked by
the existing differential duty (ADR-0020) rather than trusted. A candidate that
changes stdout is a bug in the emitter, and the tuner is therefore also a
high-volume emitter fuzzer — the search space is exactly the space of emissions
that must agree.

**D5 — Machine facts are stamped, per the standing rule.** A table records the
machine it was searched on (Sapir's standing directive: comparisons same-machine,
specs stamped on every results CSV). A table from another uarch is usable and
labelled, never silently applied as if native.

**D6 — The honest claim, restated.** External and internal phrasing becomes:
*the geometry is deduced and portable; the constants are measured per target.*
"Optimal out of the box" means the shipped table is good and the tuner closes the
rest — not that a machine-independent constant exists.

## Consequences

- **Unlocks the parity claim** without per-target hand engineering, which is the
  form of the "portable performance graveyard" (VISION §5.1) that actually kills
  projects. Geometry deduced once + constants searched per target is the exit.
- **Costs a harness, not research.** `benches/matmul/tile_ab.sh` and
  `benches/shapes/shapes_ab.sh` already do candidate-vs-candidate measurement;
  the `time` builtin (S29, in flight) removes the FLOW_PERF measurement-boundary
  problem that made conv2d's par leg ambiguous.
- **Sequencing:** best landed *after* ADR-0033's second consumer, so the tuner is
  written against two `Loc`s and cannot accidentally be CPU-shaped — the same
  rule-of-three argument that gates `block_plan`.
- **Interacts with suggestion #10:** `block_plan` supplies the schedule tree the
  tuner parameterizes. Without it the tuner has to know the llvm nest; with it,
  it does not.

## Open questions

- **Q1** — Candidate-set generation: bounded grid from the record (register-file
  budget caps TI×TJ, cache sizes cap KC/NC), or a real search (simulated
  annealing / model-guided)? Start with the grid; the record already names the
  budgets.
- **Q2** — Table format and location: per-project toml (ADR-0032 D4's "toml or
  sibling"), or a shipped-defaults crate plus a project override? Does a table
  ship in the repo per known uarch?
- **Q3** — Does a cost model (analytic, from the record + `Loc` sizes) replace
  most of the search, with measurement only as the tie-break? That is the
  strongest form and the most work.
- **Q4** — Does `GRAIN` (task-slice width, `path_plan`) belong to the same table?
  S28 measured it quantizes badly at sub-ms N — it behaves like a placement knob.

## Spec impact

None (Level A untouched). On acceptance: ADR-0032 D4 gains a pointer here;
`tile-ladder-direction.md` gains the geometry/constants split as standing
language; the CLI's item list gains `flow tune`.
