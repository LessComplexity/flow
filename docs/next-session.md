# Next Session (S41)

Written: 2026-07-28 · end of S40 · by: Claude (orchestrator; category-architect skill)
Session logs: `sessions/2026-07-28-s40-the-arm-owns-the-loop.md` +
`sessions/2026-07-29-s40b-the-compiler-is-also-a-program.md` (the compile-time A/B — read both).
Previous: S39 (`sessions/2026-07-28-s39-guards-gate-the-flow.md`).

## READ THIS FIRST

**THE GATE IS GREEN — 1006 passed, 0 failed — fmt clean.** S39's §4a P0 is closed: gating is stable across
`LiftLoops` (loops join guard arms as atomic units through their `LoopEnter` handle), and a
17-agent adversarial review of the first build found seven real defects that were all fixed in the
same session — including a pre-existing S39-class instability in DCE the review predicted and the
1024-case hammer then reproduced independently. Full account: `components/ir/plans/
plan-s40-the-arm-owns-the-loop.md` §6a (found while building) + §6b (the review round).

**Work is uncommitted on `main` @ `8b40442`, S39 + S40 together.** Sapir's call on committing.

**A/B emission vs `8b40442`: 103 byte-identical, 1 differs (`examples/calc.mapal` raw — S39's own
signed-off change), 0 new emit failures.** S40 moved ZERO surface emissions; no perf run owed
(measurement rules 9/10). This was re-proven AFTER the review fixes — two intermediate designs
moved `sepia`/`matmul4_loop` and were caught by exactly this check. Closed to the machine-code
level for the loop-exclusive scripts (Sapir's question): `fib`, `sum_to_n`, `matmul4_loop` —
`clang -O2` objects byte-identical PRE vs POST on raw and `--rewrite`; the 55 A/B skips are the
`--rewrite --contract` face rejected identically on both sides, no loop file among them.

## FIRST commands

```sh
git status --short                    # S39+S40, uncommitted
cargo test --workspace --release      # GREEN
PROPTEST_CASES=1024 cargo test -p mapal-rewrite --test property  # the hammer that found [5]
git worktree list                     # stale entries — prune (incl. s40 scratchpad if left)
```

## S41 opens on

### 1. The verdict-stability class — is DCE really the only pass that can flip a gate?

Review find [5] generalized: **a guard verdict is a function of consumer sets, and any pass that
changes consumer sets can flip a site between strict and gated across `eval ∘ rewrite = eval`.**
DCE was convicted (dead sibling reader dropped → trap suppressed) and fixed (verdict-cone tainted
dead-sink pin, `graph_rewrites.rs`). The evidence that the other five passes are safe is
empirical, not structural: the 1024-case hammer with `Step::PhiTrapArm` exercises all six and only
DCE fell; CSE never keys trap-capable ops (`is_pure`), aliasing roughly preserves
read-multiplicity, Inline duplicates rather than deletes. Worth making structural: an invariant
test over the testgen corpus — for each pass, compare `guard_plan`'s gated-site verdicts before
and after (match sites through the replay mapping) and assert no flip in either direction. That
would have caught [5] without needing a trap to fire.

### 2. P0 — the GPU leg via NVPTX (unchanged from S38/S39)

Sapir's call, taken on the probe that refuted the 8-agent audit. `guard_plan` supplies one more
input: a gate has four realizations and warp divergence is one. The CUDA emitter keeps strict
semantics for loop-touching sites (deliberate, S40) — NVPTX inherits gating from scratch.

### 3. P1 — unchanged queue

- **Per-task enable predicates in `mapal-rt`** (`ponytail:` marker in `path_plan`): a gated bulk
  op folds into its Phi's sequential task; upgrade is one field on `Task` + one arg on
  `mapal_par_dep`.
- **ADR for "guards gate the flow"**, amending ADR-0026 Q8; now also owes the unit rule.
- Beat OpenBLAS at ONE thread (flat 1.20× behind, size-invariant, untuned `generic`).
- Hardware-specific units (AMX / tensor cores as per-`Loc` capability).
- Inlining must stamp spliced morphisms with the call-site position (plan-s38 §6.1).
- Oracle clones captured arrays per fold step (plan first; forbid agents writing into the repo).
- `guard_plan` as graph structure rather than a query (revisit only on an external consumer).

### 4. Coverage debts recorded in S40

- testgen builds only topology (a) (loop-inside-arm); topology (b) (arm-inside-loop-body) rests on
  two hand-built tests (`algos.rs`, `guards.rs`). A `Step` that puts a `PhiTrapArm` inside a loop
  body would close it.
- No test pins that an untaken arm's EFFECT (print) is suppressed — arms cannot carry tokens
  today (L1404-8), so this is vacuous until they can; recorded so the day they can, the test
  exists first.

## Measurement rules (S37's six + S38's four + S39's three + S40's one)

See `sessions/2026-07-28-s39-guards-gate-the-flow.md` and prior. S40 re-confirmed rule 9 twice:
the A/B emission check caught two review-fix designs moving surface programs (`sepia`,
`matmul4_loop`) that every unit test passed.
12. **Byte-identity proves emitted-program runtime; it says nothing about the COMPILER's.**
    S40 shipped a +16.4% compiler-time regression (Sapir's catch) through a green 1006-test gate
    and a byte-identity sweep; a 51-run alternating compile-time A/B found it in one pass and two
    Phi-free early-exits cut it to +1.7%. Report: `performance/s40-compile-time.md`, raw series
    `benches/results-s40/`. A/B the compiler's wall time whenever a deduced query grows or gains
    a consumer.

## Method notes that cost time in S40

- **Pinning a dead object in a rewrite plan does not keep it** — replay materializes only READ
  objects. To preserve a dead cone, pin its SINK; the cone follows backward.
- **Dead-sink ownership in `guard_arm` is a trap**: a dead cone can read BOTH arms' values, and
  gating it into one arm reads the other's un-fired value. The ownership invariants' disjointness
  assert is what caught it — invariant suites earn their keep on designs, not just regressions.
- **Verify each fix against the A/B emission sweep before calling it done** — two designs were
  semantically fine and still wrong (surface movement).
- Two crates ship `--example emit` and collide in `target/release/examples/` (S39 note, still
  true): copy each binary out before building the other.

## Standing direction (Sapir — unchanged)

- Compute-only legs; numpy in every verdict table; scale everything up.
- Parallel-first by construction.
- Backend-genericity contract (ADR-0032): mapal-ir never learns machine facts.
- Query, not rewrite: record that something *could* be skipped; never delete it.
- Compile time decides the SIZES, runtime decides the ASSIGNMENT.
- Nothing goes in the README that a default build does not deliver.
- Proof over suggestion — a change arrives with the measurement of what it did.
- Speak simply, base claims on empirical results.
