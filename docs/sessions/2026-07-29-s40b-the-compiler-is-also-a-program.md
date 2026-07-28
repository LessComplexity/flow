# 2026-07-29 — S40b: the compiler is also a program

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017).
Driven by Sapir. Same-day continuation of `2026-07-28-s40-the-arm-owns-the-loop.md`,
opened by one question at close: *"did you do a before/after performance comparison on the same
code?? especially code that can get affected by this?"*

## 0. Continuation brief

Current state: **S40's compile-time regression is found, fixed, and measured — the emission sweep
is +1.7% vs `8b40442` (was +16.4%), with results exactly unchanged.** Gate re-verified GREEN:
1006 passed, 0 failed; `PROPTEST_CASES=1024` green; byte-identity re-proven (103/104 identical,
`calc` only); fmt clean. Work remains uncommitted on `main` @ `8b40442`, S39 + S40 + S40b
together.
Next step: unchanged — S41 per `docs/next-session.md`.
Resume command/check: `cargo test --workspace --release`.

## 1. The finding

S40's runtime claim was structural: 103/104 emissions byte-identical ⇒ emitted programs cannot
run differently. Sapir's question exposed the blind spot: **the byte-identity proof says nothing
about the compiler's own runtime — and the compiler is exactly the code S40 changed.**

Measured (51 alternating full sweeps, warmed, one process per emission; sweep = 159 emissions =
53 sources × 3 faces; PRE = `8b40442`, POST = working tree — S39+S40 combined, no pure-S39 build
exists to split):

| | PRE median | POST median | Δ |
| --- | --- | --- | --- |
| before fix | 663.2 ms | 772.1 ms | **+108.9 ms (+16.4%)** — distributions non-overlapping (PRE max 707.7 < POST min 751.7) |
| after fix | 651.0 ms | 662.3 ms | **+11.3 ms (+1.7%)** — overlapping |

Raw series: `benches/results-s40/`. Report: `performance/s40-compile-time.md`.

## 2. Cause and fix

`guard_plan` is recomputed per consumer per fixpoint round (ConstFold, DCE, `path_plan`, each
backend ctx), and it built loop units, ran `bounds_proof` and the trap-capability fixpoint even
for **Phi-free functions** — most of every benchmark. DCE's verdict-cone/halo/forward/taint
walks (review find [5]) likewise ran on Phi-free graphs to compute nothing.

Two early-exits, exact no-ops on results:

| Fix | Where |
| --- | --- |
| no `Phi` in the function ⇒ `guard_plan` returns no sites before any construction | `mapal-ir/src/algo.rs` |
| no `Phi` in the graph ⇒ DCE skips the verdict-cone machinery | `mapal-rewrite/src/graph_rewrites.rs` |

The residual +1.7% is guard machinery on the functions that actually carry Phis (`calc`,
`sepia`, `abs` bodies), ≈0.07 ms per emission — below the noise rule's bar with this harness.
Next rung if it ever matters: memoize `guard_plan` across consumers within a pass round.

## 3. Checks

| Check | Result |
| --- | --- |
| A/B emission vs `8b40442`, 3 faces | 103 identical, 1 differs (`calc` raw — S39's change) |
| `cargo test --workspace --release` | **1006 passed, 0 failed** |
| `PROPTEST_CASES=1024 … --test property` | green (3.89 s) |
| `cargo fmt --check` | clean |
| compile-time A/B, 51 alternating sweeps | the table above |

## 4. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/s40-compile-time.md` | new — the numbers, cause, fix, method note |
| `benches/results-s40/` | new — raw series + harness + machine |
| `docs/next-session.md` | measurement rule 12: byte-identity proves emitted-program runtime, never the compiler's — A/B compiler wall time whenever a deduced query grows or gains a consumer |
| `docs/STATUS.md`, `components/ir/STATUS.md` | the finding + fix folded into the S40 entries |
| this log | new |

## 5. Files changed

`crates/mapal-ir/src/algo.rs` (Phi-free early return in `guard_plan`) ·
`crates/mapal-rewrite/src/graph_rewrites.rs` (Phi-free skip of DCE's verdict-cone block).

## 6. Method note earned

**The compiler is also a program.** S40 shipped a +16.4% compiler-time regression through a green
1006-test gate, a 1,280-run differential, a 17-agent adversarial review, and a byte-identity
sweep — because every one of those instruments points at the compiler's OUTPUT. One question
from Sapir and a 90-second A/B script found it. Deduced queries that run per pass per fixpoint
round pay their cost on every function, including the ones they can say nothing about — gate
them on the shape they exist for.
