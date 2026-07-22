# ADR-0031: `iota`/`fill` ride the pipeline — the call-expression carve is removed

Date: 2026-07-22 (S22) · Status: **accepted — Sapir directive, S22 in-session** ("it should be `262144 -> iota`, not `iota(..)`"). Supersedes the *surface* half of ADR-0029 stage 2a; the IR half of ADR-0029 (ops, static-n, kernels) is untouched.

## Context

ADR-0029 stage 2a introduced `iota(n)` / `fill(x, n)` as **the only legal call
expressions** in the grammar (the P0108 carve, S20 — decided under the delegated
mandate). Sapir, reading `benches/matmul/matmul512_cap.flow` in S22, rejected the
form on sight: Flow is a dataflow language; every other builtin — `print`,
`println`, `zip`, `enumerate`, the `widen_*` family — is a **stage on the arrow
path**. Function-call syntax is alien to the surface (E4: a flow is a statement;
data enters stages via `->`). The carve also complicated the grammar for exactly
two names.

The arrow form was always expressible: the count IS the input.

## Decision

1. **Surface:** `n -> iota` (count flows in; `[i32; n]` flows out) and
   `(x, n) -> fill` (the standard tuple-input shape, like any 2-ary stage).
   `iota`/`fill` resolve by name on the builtin path (`is_pure_builtin` family,
   L1009 reserved), exactly like `zip`/`enumerate`.
2. **Grammar:** the call-expression production is REMOVED; **P0108 reverts to
   rejecting every call expression**, now with a teaching diagnostic for the two
   names: "write `n -> iota` / `(x, n) -> fill`".
3. **Static-n rule unchanged, enforced at the same depth:** the count wire must
   be a literal `Constant` (positive, ≤ i32::MAX). L1612/L1613 reword to the
   pipeline forms; the builder/validate twins (`NonStaticCount`,
   `IotaCountMismatch`) already enforce it below.
4. **Lower mechanism:** `iota` — the incoming wire object is passed directly to
   the existing `IrBuilder::iota(count, dest, loc)`. `fill` — the incoming pair
   is passed to the existing **`IrBuilder::fill_from(pair, dest, loc)`** (the
   S21 replay-faithfulness entry: consumes an existing tuple, mints nothing —
   the new surface and the replay path now share one spine). The value+count
   sugar `fill(x, n, …)` remains for tests/replay-independent construction.

## Consequences

- **Zero IR change.** `Operation::Iota`/`Fill`, validate, interp, rewrite,
  testgen, llvm/cuda emitters and their goldens are untouched — the same graphs
  come out of lower. Only syntax + lower + surface sources move.
- 14 `.flow` files (examples + benches) and the two bench generators
  (`gen_flow.py`, `gen_flow_capture.py`) migrate to the arrow form; syntax
  parse-tree goldens and lower snapshots re-pin; bench `.cu`/`.ll` artifacts
  regenerate afterwards (identical modulo the independent `--rewrite` change
  landing in the same session).
- The grammar loses its only expression-call production — a simplification;
  P0108's rejection surface is uniform again.
- `docs/STATUS.md` capability row stays (feature unchanged); ledger gains this
  row; ADR-0029's stage-2a bullet gains a supersession pointer.

## Spec impact

`flow-as-implemented.md`: the iota/fill row's surface column updates to the
pipeline forms on ledger close (same handling as ADR-0029's pending patch).
Level A untouched (realized-set surface delta, ADR-0018 class).
