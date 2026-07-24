# ADR-0032: Precision contracts in the type system, machine tailoring in backend config

Date: 2026-07-24 · Status: **accepted (Sapir, S29 — "my instinct led to that: type system — precision, format, reassociation; backend config — performance tailors, machine tailoring")**. Queue: after the S29 OpenBLAS levers. Related: ADR-0029 (widen family), ADR-0021 (Widen op), the S24b fmad two-face precedent, `docs/notes/tile-ladder-direction.md` (mma arch-precision).

## Motivation

The mma rung forces the question: tf32 exists only on Ampere+, f64 DMMA only on
datacenter cards, and cuBLAS/cuDNN default to tf32 for f32 work — so "which
precision does this program compute in" is a per-backend, per-arch question with
semantic consequences. Today the two-face split lives as emitter flags
(`--contract`, `-fmad`): out-of-band state that changes the compiled function.
That breaks the porting promise — **provably the same function, plug-play
backends** — and makes oracle gates ambiguous ("which function are we testing?").
Meanwhile tile factors (TJ/TI/KC/NC), grain sizes, arena thresholds are pure
placement: they never change an output bit, yet users will want to tune them per
project and per backend.

## The dividing rule

**Anything that can change an output bit belongs to the language. Anything that
only changes how fast we get there belongs to backend config.**

- **Type system — precision, format, reassociation.** Semantic contracts travel
  with the code; every backend honors them or declines them, same source
  everywhere.
- **Backend config — performance tailors, machine tailoring.** Value-invariant
  by construction (bit-identical output regardless of setting, provable); tuned
  defaults, user-overridable per project.

## Decision

**D1 — Precision-contract lattice in the type system.** Types/regions carry a
numeric contract: `exact` (default — today's conformance face; bit-exact to the
interp oracle), `contract` (single-rounding class — today's fma product face),
`tf32-class` (reduced-precision inputs ≤10-bit mantissa, f32-class accumulate —
the mma class). Default `exact`: naive code is portable bit-exactly with zero
annotations.

**D2 — Format conversions as explicit ops.** Conversion into reduced formats is
an explicit morphism in the graph (the `widen_*` family's sibling — e.g. a
`to_tf32`-class op): visible, checkable, portable. A region whose operands are
tf32-converted is *declared*; a backend emitting mma there honors the graph,
never sneaks a flag past it.

**D3 — Backend capability matrix.** Each backend/arch reports which contracts
it can honor (4090: `contract` ✓, `tf32` ✓, f64-DMMA ✗; A100: all ✓). A
lowered-precision realisation (mma, fma contraction) fires exactly when the
region's contract admits it AND the target supports it — otherwise the exact
fallback (SIMT/plain-FMA path), same source, no code change. The backend can
honor the declared contract or fall back; **it never violates it.**

**D4 — Backend config for placement knobs.** Tile factors (TJ/TI/KC/NC), grain
sizes, arena thresholds, prefetch distance: per-backend tuned tables as
defaults ("best performance out of the box"), overridable per project (toml or
sibling). Placement knobs are value-invariant; the differential gates remain
valid under every config.

> **How the tables get their values → ADR-0034 (candidate, 2026-07-25).** D4 fixes
> *where* constants live; it does not say how they are chosen. Today they are
> literals in `backends/llvm/src/func.rs` from manual sweeps on one machine
> (S26: "TI sweep 2/4/8 → 4"), applied unchanged on every target. ADR-0034
> proposes they be **searched** — one generic tuner over the recorded geometry,
> per-`Loc` table, offline and cached so builds stay deterministic. The
> value-invariance D4 establishes is exactly what makes the search safe: every
> candidate is checked by the existing differential duty rather than trusted.

**D5 — Forbidden: precision via ambient config.** No compiler flag, env var, or
toml key may change rounding semantics. (The existing `--contract` emitter flag
is the interim form of D1's `contract` class and gets absorbed into it.)

## Semantics notes

- The contract is **part of the function**: the differential/oracle gates stay
  meaningful per class (conformance face bit-exact; product faces rel-tol
  gated, the S24b/S27 pattern).
- Reassociation permission rides the same lattice (`exact` forbids it;
  `contract`/`tf32-class` admit the k-panel/window orders those classes
  already use — today's rungs never reassociate the per-cell chain, so they
  stay `exact`-compatible).
- Verilog/P7: contracts map to fixed-point/pipelined realisations or decline —
  the capability matrix is the only backend-specific surface.
- `time` builtin (S29, in flight) is orthogonal: measurement, not semantics.
