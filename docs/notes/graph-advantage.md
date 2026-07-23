# The execution-graph advantage — Sapir's founding assumption, and its evidence ledger

Recorded 2026-07-23 (S24b), on Sapir's articulation: *"the execution graph — unlike an
AST — gives us the ability to do such optimizations out of the box using graph
analysis/reachability analysis and matching graph operations to instructions on
backends… this structure reuse allows us to, theoretically, do the BLAS mechanism out
of graph deduction automatically."*

## The claim, made precise

An AST is syntax: dataflow, aliasing, effects, and loop dependence must be
*reconstructed* by analysis, and in pointer languages those analyses are approximate
or undecidable (alias analysis; the polyhedral dependence industry). In Category-IR
the properties other compilers fight for are **invariants by construction**:

- every value has exactly one producer (edge-only dataflow, ADR-0013);
- arrays are whole values — there is no aliasing to analyze (`Update` is a morphism);
- effects form one linear token chain (E2/T0201) — purity is a signature fact;
- dependence IS the edge set — independence = no path, an exact reachability query.

So the *legality* of any transform — parallelize, vectorize, tile, fuse, reorder,
elide — is a cheap exact deduction, not a research problem. The deduced-query pattern
(`loop_plan`, `bounds_proof`, `last_use_plan`, `emission_plan`, `path_plan`) is this
claim operationalized; R1 + the oracle differential make every transform's
*correctness* mechanized rather than argued.

## Evidence ledger (shipped, measured — not theory)

| Deduction | Graph fact used | Measured result |
| --- | --- | --- |
| parallel orchestrator (S24) | no-path independence + token linearity | chapel-multicore parity @1024 f32 (184 vs 193 ms); 19× self-speedup |
| guard elision (S20) | index intervals over edges | hot map kernels emit zero bounds checks |
| in-place Update / copy elision (S20) | last-use + escape reachability | per-iteration 16 KB memcpy deleted; cuda back-edge frees |
| map∘map fusion, CSE, trap-conservative DCE (S12) | single-producer structural equality | R1-licensed rewrite suite, zero-divergence differential |
| minimal emission (S22) | value-lifetime classes | dissolved/inlined objects never materialize |

## Generalization: reuse is fanout

The BLAS mechanism's core — data reuse — is *visible* in the graph as fanout:
`a[i,k]` feeding N cells is N edges out of one node. The same signature appears in
stencils/FIR (neighboring windows share elements), convolutions, attention-shaped
patterns, scans, reduction trees (ADR-0028), and image pipelines. Iteration-space
tiling is therefore ONE rewrite (blocking over map/fold), written once, R1-checked
once, and *placed* per backend: CPU cache tiles, CUDA shared-memory tiles, FPGA line
buffers are the same transform at different `Loc`s (FRAMEWORK §4.2). The S25 ladder
(next-session item 2b): guards-off auto-SIMD → tiling as a rewrite → per-backend
vector emission only if measurement demands it.

## The honest comparison and the honest boundary

Array DSLs (Futhark, XLA, TVM, Accelerate) get this class of win from graph IRs —
they are the existence proof — but they are domain-restricted. General-purpose
languages get it unreliably or on trust: C/C++/Rust autovectorizers surrender to
aliasing; Chapel's `forall` and OpenMP pragmas are *programmer assertions* of
independence, not proofs. Flow's position is the combination no one hands out
together: a **general** language whose whole surface lowers to the analyzable graph
+ the multi-backend placement contract + oracle-differential-verified transforms.

Boundary: "out of the box" means legality is free and safety is mechanized; each
transform (tiling, region fusion) is still designed once. The last ~2–3× to BLAS
peak is register-blocked microkernels — possibly a per-backend seam forever. The
~50× between naive and near-BLAS is graph-deducible territory.
