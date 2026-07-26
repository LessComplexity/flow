# The execution-graph advantage — Sapir's founding assumption, and its evidence ledger

Recorded 2026-07-23 (S24b), on Sapir's articulation: *"the execution graph — unlike an
AST — gives us the ability to do such optimizations out of the box using graph
analysis/reachability analysis and matching graph operations to instructions on
backends… this structure reuse allows us to, theoretically, do the BLAS mechanism out
of graph deduction automatically."*

## The claim, made precise

An AST is syntax: dataflow, aliasing, effects, and loop dependence must be
*reconstructed* by analysis, and in pointer languages those analyzes are approximate
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

## Is "the code IS the graph" true in the implementation? (Sapir's validation question, S24b)

**Yes — verified, and defended by two mechanisms.** The surface-to-IR mapping is
syntax-directed, never reconstructive: `->` is an edge; a binding names a node
output; `mut` rebind mints a new node; fanout blocks are literal out-edges; guards
are Phi arms; `loop` is a real cycle (inline SCC); bulk ops are single morphisms
with body sub-graphs; sequencing and effects are themselves edges (the world token —
`seq` has zero IR footprint beyond token order, ADR-0019). mapal-lower does real
work (naming, typing, widths, desugaring, 51 L-codes) but ALL of it is local —
no alias, dependence, or effect analysis exists anywhere in the pipeline, because
nothing structural is ever lost. The property survived contact by:
(1) **convergent refinement** — every syntax/graph collision was settled FOR the
graph (ADR-0013 edges-only; ADR-0019 seq → token discipline; ADR-0027 captures →
explicit broadcast edges; E1/ADR-0016 cycle semantics); and (2) **rejection over
analysis** — programs that would make the graph ambiguous are diagnosed away
(E2 effects-in-fanout, strings-as-data, dynamic sizes out of Core): the language's
restrictions are the price of the property. It is re-verified continuously:
`validate` seals the graph invariants (single producer, token linearity), the
oracle interpreter executes the graph itself, and every differential re-tests
"the graph means the code". Strongest single evidence: the S24 parallel
orchestrator needed only reachability over existing edges — had the assumption
diverged, `path_plan` would have required dependence analysis; it required none.

## The honest comparison and the honest boundary

Array DSLs (Futhark, XLA, TVM, Accelerate) get this class of win from graph IRs —
they are the existence proof — but they are domain-restricted. General-purpose
languages get it unreliably or on trust: C/C++/Rust autovectorizers surrender to
aliasing; Chapel's `forall` and OpenMP pragmas are *programmer assertions* of
independence, not proofs. Mapal's position is the combination no one hands out
together: a **general** language whose whole surface lowers to the analyzable graph
+ the multi-backend placement contract + oracle-differential-verified transforms.

Boundary: "out of the box" means legality is free and safety is mechanized; each
transform (tiling, region fusion) is still designed once. The last ~2–3× to BLAS
peak is register-blocked microkernels — possibly a per-backend seam forever. The
~50× between naive and near-BLAS is graph-deducible territory.
