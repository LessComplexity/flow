# Plan — `time` builtin (kernel-scoped timing from inside the program)

**Status:** written pre-build, S29. Sapir: "instead of MAPAL_PERF stuff just add a
time function to the system to time the computation part, like any normal
language." Replaces the `--perf`/`MAPAL_PERF` emission hack for benchmark legs:
the program brackets its own compute region.

## Why (one paragraph)

`MAPAL_PERF` times all of `mapal_main` — so a bench shape whose program includes
data generation reports gen+kernel while cpp/rust/numpy time kernel-only (the
conv2d bracket problem, measured S28: gen ≈ 0.47 of 0.51 ms). Any normal
language solves this in-source: `t0 = now(); work(); print(now() - t0)`. Mapal
has no clock — add one. The effect must be **token-threaded** (like `print`):
a clock read reordered across the work it brackets is a wrong answer, so the
operation is effectful by construction.

## Categorical model

| Item | Kind | Model |
| --- | --- | --- |
| `time` | `Trn ⊸` (effectful) | `() → ℝ` — monotonic milliseconds (f64). The `⊸` marks the effect: it consumes/produces the IO token, so its position in the chain is semantic, never optimized away |
| IO token | `Dat` (ordering) | same token `print` threads — `time` composes with print's sequencing rules unchanged |
| `mapal_time_ms` | `Trm` (runtime seam) | the extern the llvm backend emits a call to; one `DataLoc` (the process clock) |

Surface: `() -> time` — a stage builtin (the wire is the Unit token of the
chain, exactly like a literal starts a chain); produces `f64` milliseconds from
a monotonic clock. Idiomatic bench use:

```
() -> time -> t0;
<img gen and kernel>
() -> time -> t1;
t1 - t0 -> elapsed -> println;
```

**Composition rules.**
1. `time` is effectful: never const-folded, CSE'd, reordered, or DCE'd (the
   token dependency enforces all four — no new machinery, it rides print's).
2. Same call, same process: monotonic non-decreasing (the runtime clock is
   `Instant`, not wall time-of-day).
3. Reserved stage name (like `print`/`iota`): user fns named `time` reject.
4. **(added in build, S29) A clock read fences the work written above it.**
   Every task all of whose morphisms originate before the read in the SOURCE
   must have completed before the clock is sampled. Rule 1 said "never
   reordered"; the token thread delivers that only against other *effects*, and
   pure work has no ordering relation to a clock read at all — so without this
   rule the orchestrator legally (and actually) runs the bracketed work after
   the closing read. Source position is the key precisely because the graph
   supplies no order; it is also exactly what a programmer means by putting two
   reads around some lines.
5. **(added in build, S29) A clock value never leaves the host spine.** `TimeMs`
   is the first spine op producing a value rather than only a token, and tasks
   are dispatched before the host writes it — so its whole consumer cone stays
   on the spine (§4.5 Law 1). Without this, `t1 - t0` runs in a task that races
   the host's write and reports a NEGATIVE elapsed (observed, S29).

## Work items (cross-crate)

1. **mapal-ir**: `Operation::TimeMs` (source = token, target = `(token, f64)`
   pair) + validate/typing rows + mermaid/debug printing.
2. **mapal-lower**: stage dispatch branch (mirror `emit_print`: consume token,
   `fb.time_ms(tok)` → record `(token, f64)`, rebind token); effects
   classification as an effect site (`mapal-lower/effects.rs`,
   `mapal-check/effects.rs`); reserved-name rule gains `time`.
3. **mapal-interp**: execute via `Instant` (f64 ms), thread the token.
4. **backend-llvm**: `mapal_time_ms() -> double` in `RT_DECLS` + emit the call
   where print ops are handled (token is ordering-only at emission).
5. **mapal-rt**: `pub extern "C" fn mapal_time_ms() -> f64` (same clock as
   `mapal_perf_begin/end`).
6. **mapal-rewrite**: no rule touches it (effectful); pin that the default
   pipeline leaves it intact.
7. Benches migrate to in-source brackets (`shapes/*.mapal` first: time the
   kernel map, exclude gen); `--perf`/`MAPAL_PERF` retires when the box/local
   drivers are converted (runner.py extraction moves to the printed `elapsed`
   line).
   **Done S29 for `benches/shapes/`:** the four perf-sized shapes open the
   bracket after generation and print `iter ms=<elapsed>` — the baselines' own
   format, so `shapes_ab.sh` extracts both with one rule and the `--perf` legs
   are gone (4 builds per shape → 2). `benches/matmul/` (tile_ab.sh, runner.py)
   is NOT migrated and still measures gen+kernel; fine for A/B against S28 on
   the same basis, wrong for cross-language verdicts.

## Tests

All landed S29 (14 tests). Beyond the list below, the two build-time rules (4
and 5) are pinned in `mapal-ir/tests/algos.rs` and `llvm/tests/golden_ll.rs`, both
mutation-verified — reverting either fix fails its test.

- syntax: `()` parses to `ExprKind::Unit` (it was the P0001 rejection), tree
  golden for the `() -> time -> t0` head.
- lower: bracket lowers clean and types f64; both reads on one token chain;
  L1009 (`fn time`), L1301 (`()` as a value), L1302 (`5 -> time`), L1605
  (`time` in a map body); mapal-check T0201 (`time` in a Plain fanout branch).
- ir: the source-order fence (t0 fences the generation above it, t1 that plus
  the kernel, neither fences work written below) and the host-cone rule (no task
  holds a morphism touching a clock value); `path_plan` determinism extended.
- interp: `t1 >= t0`, finite, and the bracketed work really ran.
- differential llvm: bracketed program at -O0/-O2 × MAPAL_PAR {default, 1} —
  byte-identical to the untimed twin, elapsed a finite f64 ≥ 0 (never a bound).
- golden_ll: extern declared, exactly two calls in chain order, `t0`'s wait list
  non-empty and `t1`'s strictly larger, `fsub` after both reads.

## Ceilings

- `time : () → ℕ` nanosecond integer twin (if f64 rounding ever matters for
  µs-scale kernels) — no demand yet.
- MAPAL_PERF removal from the emit example/runner (mechanical, after bench
  migration).
