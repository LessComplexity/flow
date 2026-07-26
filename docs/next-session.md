# Next Session (S34)

Written: 2026-07-26 · end of S33 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-26-s33-boundary-openblas-parity-open-source.md` — **read §5 and §7
before touching either P0.**

## Where things stand (≤6 lines)

S33 closed the S31/S32 P0 by **inverting** it: conv2d was never slow — Flow's timed window
included the output array's first-touch page-zeroing, which the C++ baseline pre-pays outside its
own timer. Shipped `reside`; conv2d is now **1.21× ahead** of naive C++ per core on both NEON and
AVX2. Second machine measured: on AVX2, where numpy is OpenBLAS rather than AMX, Flow's generated
GEMM reaches **parity** (1t a flat 1.20× behind, threaded ±10%). The repo is **public** with CI.
**Both CI and the local gate are red, correctly** — CI found a rewriter bug on its first run, and
pinning the seed makes it reproduce locally too. 4 of 9 `flow-rewrite` property tests fail: one bug,
four entry points, and the trigger is narrowed to **`MapFusion` in composition** (§1).

## FIRST commands (resume checks, in order)

```sh
git log --oneline -3                  # HEAD is the S33 close commit
git status --short                    # expect empty
gh run list --limit 3                 # expect RED on ubuntu-latest — that is P0 #1
cargo test -q -p flow-rewrite --release --test property   # P0 #1: expect 4 FAILED, ~0.4s
sh editors/test.sh                    # 61 assertions, expect green
ssh -o BatchMode=yes <perf-box> 'cat /proc/loadavg'   # the measurement machine, key auth
```

## S34 focus

### 1. P0 — the rewriter deletes a trap that must fire → **start at `MapFusion`**

```
prefix [Inline, LiftLoops, ConstFold, Cse, Dce, MapFusion]:
    Trapped(DivZero)  !≈  Done(Scalar(I32(3)))
```

The original traps on division by zero; after `rewrite()` it returns 3. That breaks the property
the whole project rests on. `dead_trapping_div_stays_trapped` still passes, so the generated shape
is one the hand-written guard does not cover.

**Already narrowed — do not re-derive it.** S33 added a pass-composition bisect to `check_open`,
because the original failure said only `full:`, which is a clue rather than an answer. It
establishes two things:

- **Every pass is individually trap-preserving** (the pre-existing per-pass loop proves that).
- **`Inline → LiftLoops → ConstFold → Cse → Dce` preserves the trap; adding `MapFusion` loses
  it.** Prefixes 1–5 pass; prefix 6 fails.

So it is an **interaction**, and **map fusion is the trigger**. Look at the fusion rule in
`crates/flow-rewrite/src/graph_rewrites.rs`, and ask what happens when the two maps being fused
have a body whose result is unused but whose evaluation traps — the earlier five passes are what
put the graph into that shape.

**Scope: 4 of 9 property tests fail** — `open_default`, `open_trap_free`, `closed_default`,
`closed_trap_free`. One bug, four entry points: proptest replays the persisted seed against every
property in the file, and each uses a different strategy.

```sh
cargo test -q -p flow-rewrite --release --test property        # all four, ~0.4s
```

The counterexample is **not minimal** (proptest hit its 192-iteration shrink limit). If the
`MapFusion` lead does not resolve it quickly, shrink harder:

```sh
PROPTEST_MAX_SHRINK_ITERS=1000000 cargo test -p flow-rewrite --release --test property open_default
```

**Do not un-pin the seed to get a green build.**

### 2. P0 — `flow_par_wait` lets workers run ahead of the clock

Workers do not stop at checkpoints, so a kernel can finish before the clock meant to bracket it is
read. 3–4% of threaded runs; one live case read **0.0001 ms** for a 1024² matmul. `FLOW_PAR=1` is
0/100, so every single-threaded number in the repo is sound.

**Do NOT retry the runtime-only dispatch ceiling.** It was built, measured and reverted;
`plan-s33b-clock-read-barrier.md` §4 records both reasons as do-not-retry. Short version: launch
must dispatch immediately, and **at launch the runtime does not know where the first checkpoint
is** — only the emitter does. Make the clock read a DAG node with edges both ways.

The sharp acceptance criterion: `watermark_wait_can_finish_before_task_completion` and
`wait_helps_while_the_background_worker_is_busy` must pass **unmodified** — they are the guard rail
the first attempt tripped.

### 3. Then the measurement debt this creates

- **Re-confirm S32's scheduling verdict** (1.41–1.43×) under a median. Every `par` minimum in
  S28–S32 is suspect.
- **Re-measure conv2d and fir through `shapes_ab.sh`.** The S33 figures are hand-linked; do not
  publish them as harness numbers.
- **matmul boundary immunity** is expected <1% but unverified at 4096.

## Rules that bit this session (log §7)

1. **Pin, always.** Two unpinned readings on the hybrid i9 produced confident wrong conclusions.
2. **`ref-cycles`, not `cycles`, separates frequency from time.**
3. **`cargo test` does not rebuild `target/release/libflow_rt.a`** — a stale staticlib presents
   exactly as a fix that does nothing. `cargo build -p flow-rt --release` before any hand-linked leg.
4. **`calloc` is not a pre-fault.** Page size is decided by alignment, not request size.
5. **Compare ratios within one run, never against a recorded baseline.** cpp-1t fir drifted 41%
   between sessions. I broke this rule once and the A/B refuted me.
6. **Verify a kill.** An orphan survived a parent-only `kill` and corrupted two measurement rounds
   (~20% on threaded cells). Assert live-process count and loadavg in the script's own output.
7. **A test that passes wrongly is worse than none.** Three happened this session, all caught by
   **negative control** — break the rule on purpose and check the test notices.
8. **min for 1t, median for par** — inverted from the old rule, because this race makes *fast*
   outliers. Reverts once P0 #2 is fixed.

## Gotchas / warnings

- **The Arch i9 is the measurement machine**, key auth, `<perf-box>`. No clang there:
  cross-compile `.ll` on the Mac (`-target x86_64-unknown-linux-gnu -march=raptorlake`) and link
  with gcc. `~/flowbench` (182 MB) and `~/flowbench_pre` (21 MB, the pre-`reside` runtime) are both
  built and left in place.
- **`reside` pins pages to the touching thread's NUMA node.** Irrelevant single-socket, live on a
  dual-socket EPYC. A/B it or make `reside` lane-aware before any multi-socket run.
- **The scheduler advantage is heterogeneity tolerance, not a better scheduler.** On uniform cores
  OpenBLAS beats us — 41% on 8 P-cores. Do not restate it as a general claim.
- **The i9 1t gap ran on the untuned `generic` profile** — some of the 20% is probably recoverable.
- Emitted `.ll`/`.cu` are no longer tracked; `benches/matmul/regen.sh` derives its own worklist and
  reports CUDA refusals (the `time` builtin has no CUDA seam) instead of aborting.
- **VS Code extensions must be installed as a `.vsix`** — a copied or symlinked folder is ignored
  *silently*, because VS Code reads `extensions.json` as its registry.
  `python3 editors/vscode/package-vsix.py`, then `--install-extension`.
- Editor grammars have **opposite precedence** (Vim last-match-wins, TextMate earliest-match-wins).
  A rule added to one goes at the other end of the other. `sh editors/test.sh` after any edit.
- **Repo is public.** The perf box address is scrubbed to `<perf-box>`; keep it that way.
- `kc_nest` stays default-OFF; `oversub` is 1 for `Sliding` reads by design; the fma legs are
  numerically-equal-not-byte-equal BY DESIGN.

## Standing direction (Sapir — unchanged)

- **Compute-only legs; numpy in every verdict table; scale everything up.**
- **Parallel-first by construction.**
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a flow-ir
  query or emitter-local cashing with zero flow-ir change. flow-ir never learns machine facts.
- **Type system = precision contracts; backend config = performance tailors.**
- **Compile time decides the SIZES, runtime decides the ASSIGNMENT.**
- **Nothing goes in the README that a default build does not deliver.**
