# S40 — compile time: the regression the byte-identity proof could not see

Raw series: `benches/results-s40/` (51 alternating full sweeps per comparison, warmed, one
process launch per emission; machine in `machine.txt`). Harness:
`benches/results-s40/compile_ab.sh` — one sweep = 159 emission invocations (53 sources under
`benches/shapes/`, `benches/matmul/`, `examples/` × three faces: raw, `--rewrite`,
`--rewrite --contract`). PRE binary built from `8b40442`; POST from the S39+S40 working tree.
No pure-S39 build exists, so the delta is S39+S40 **combined**.

## Why this was measured at all

S40's runtime claim is structural — 103 of 104 emissions byte-identical to `8b40442`, so emitted
programs cannot run differently (measurement rules 9/10). Sapir asked what that proof does NOT
cover: the compiler's own runtime. It regressed, and the A/B found it in one pass:

| comparison | PRE median | POST median | Δ | separation |
| --- | --- | --- | --- | --- |
| before the Phi-free gates | 663.2 ms | 772.1 ms | **+108.9 ms (+16.4%)** | non-overlapping (PRE max 707.7 < POST min 751.7) |
| after the Phi-free gates | 651.0 ms | 662.3 ms | **+11.3 ms (+1.7%)** | overlapping |

## Cause and fix

`guard_plan` (S39, unit construction added S40) is recomputed per consumer per fixpoint round,
and until this measurement it built loop units, ran `bounds_proof` and the trap-capability
fixpoint even for **Phi-free functions** — which is most of every benchmark. DCE's verdict-cone,
forward and taint walks (S40 review find [5]) likewise ran on Phi-free graphs to compute nothing.

Two early-exits, both exact no-ops on results (byte-identity re-verified after: 103 identical,
`calc` only):

- `guard_plan`: no `Phi` in the function ⇒ return no sites before any construction
  (`mapal-ir/src/algo.rs`).
- `analyze_dce`: no `Phi` in the graph ⇒ skip the verdict-cone/halo/forward/taint walks
  (`mapal-rewrite/src/graph_rewrites.rs`).

## What the residual +1.7% is

Guard machinery on the functions that actually carry Phis (`calc`, `sepia`, `abs` bodies), per
pass per fixpoint round, ≈0.07 ms per emission on this Mac. Under the ~6% unpinned-Mac noise rule
it is not distinguishable from zero with this harness; the honest statement is "≤2%". If it ever
matters, the next rung is memoizing `guard_plan` across consumers within one pass round — today
it is recomputed by ConstFold, DCE, `path_plan`, and each backend ctx independently.

## Method note

**Byte-identity proves emitted-program runtime; it says nothing about the compiler's.** A
compile-time A/B costs one script and 90 seconds. S40 shipped a +16% compiler regression through
a green 1006-test gate, a 1,280-run differential, and a byte-identity sweep — none of which
measure the compiler. Added to the session rules as rule 12.
