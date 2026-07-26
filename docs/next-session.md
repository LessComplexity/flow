# Next Session (S37)

Written: 2026-07-27 · end of S36b · by: Claude (orchestrator; category-architect skill)
Session logs: `sessions/2026-07-27-s36b-cross-machine-validation.md` (the validation campaign) and
`sessions/2026-07-27-s36-clock-read-barrier.md` (the fix) — **read S36 §10 and S36b §11 before
republishing any `par` number.** Previous: S35
(`sessions/2026-07-26-s35-shape-ladder-and-the-ast-question.md`).

## Where things stand (≤6 lines)

**S33's last P0 is closed.** A clock read is no longer a bare host-spine checkpoint: `path_plan`
makes it a **pinned DAG node** with edges both ways, so the work written after a `time` read cannot
be dispatched until the read fires. fir 65 536 at `MAPAL_PAR=14`: **6/100 readings under 0.01 ms →
0/100**, values byte-identical, total wall time unchanged. It landed entirely in `mapal-ir` —
`crates/mapal-rt/` has a zero-line diff and the emitter needed no new code, because the pinned-task
machinery from S24 already carried it. **Validated A/B on seven shapes and two machines (M4 Pro and
the i9), 8,400 runs: pre-fix 11 of 21 shape-machine pairs reported speedups the hardware cannot deliver, post-fix
zero — `benches/results-s36/`.** Every `par` number published before S36 is biased fast; republishing
them is now P0. Gate: 972 passed; `17e10b3` pushed, CI green.

### Previously (S35)

Four non-compute shape classes (saxpy, reduce, transpose, gather) measured and published, losses
included; the finding is that a plain `map` is not a tile site, so streaming kernels emit **scalar
loops** — 3.7× behind naive C++ at one thread. Separately: the compiler **does** have an AST and the
README said it did not; corrected to "the syntax is a serialization of the execution graph."

## FIRST commands (resume checks, in order)

```sh
git log --oneline -4                  # HEAD is the S36b close; 896fb3c is the fix
git status --short                    # expect empty
gh run list --limit 3                 # expect success on main
cargo test --workspace --release --no-fail-fast 2>&1 | grep -E "FAILED|panicked|test result"
gh auth status                        # `LessComplexity` is now active (the account that can push)
git worktree list                     # expect TWO stale entries to clean up
```

**S36 + S36b are pushed.** `gh`'s active account is now `LessComplexity` (switched in S36b), which
is the one that can write to the repo — if a push is refused, check `gh auth status` first.

**The S36b pre-fix worktree is removed**; its emitted IR and binaries survive in
`target/tmp/i9pre/` if the A/B needs re-running. One stale worktree remains from S33
(`…/scratchpad/pre` @ `1daddaa`) and `git worktree prune` does not clear it — it is a live
directory under `/private/tmp/claude-501/…`, so it needs `git worktree remove --force <path>`.

## S37 focus

### 1. P0 — republish the `par` tables

The race is gone and the replacement numbers already exist. `benches/results-s36/` carries seven
shapes × two machines × pre/post at n=100, plus the published harnesses re-run post-fix
(`mac_shapes_ab.log`, `mac_ladder2_ab.log`). What remains is the edit: fold them into
`docs/performance/shape-ladder-v2.md` and the README, and make sure no pre-S36 `par` cell is still
quoted anywhere. Two consequences, in order:

- **Every published `par` cell is known to be biased fast.** The kernel used to get a head start
  before `t0`; it no longer does.
- **Re-confirm S32's scheduling verdict (1.41–1.43×)** under both min and median. It needs the
  pre-S32 A/B leg rebuilt — its own campaign, but no longer blocked on a bad statistic.

**Which statistic.** Not "min everywhere" again. S36 measured the residual min/median gap and it
tracks kernel size — 0.57 at fir 65 536, **0.84** at fir 1 048 576, zero sub-0.01 readings at
either. That is pool wake-up jitter. Min is trustworthy where the kernel dominates wake-up; small
cells stay on medians for a reason unrelated to the clock (plan-s33b §7).

**And check the bound, not just the threshold.** S36b's finding: the "under 0.01 ms" counter is
calibrated to fir and missed a 1494× reading on matmul. Any par cell whose apparent speedup
(`1t_median / par_min`) exceeds the machine's thread count is a defect, whatever its absolute
value — `benches/results-s36/run_pinned.sh` reports min/median per cell so this is checkable at a
glance (plan-s33b §8).

### 2. P1 — streaming and permutation kernels emit scalar loops

`shape-ladder-v2.md` §finding: the saxpy task has one `fmul` and **zero q-register loads**, while
the data-generation task in the same binary is full-width NEON. A plain `map` is not a tile site.
Decide whether a non-tile `map` gets a vectorization rung — **plan first**, per the build flow.
Done when saxpy 1t is within ~1.2× of naive C++.

### 3. One decision still waiting on Sapir

**Halve the per-push differential cross product.** 1,391 s of a ~1,475 s Ubuntu test step — 94%.
320 generated programs × {raw, rewritten} × {`-O0`, `-O2`} = 1,280 compile-and-runs per push.
Running raw@`-O0` and rewritten@`-O2` takes ~12 min off Ubuntu with the full cross product nightly.
It changes the README's published "1,280 comparisons per run" claim, so it needs a decision plus a
doc edit, not a silent change.

## Rules that bit S36 / S36b (S36 log §10, S36b log §11)

1. **A fixed threshold is a proxy; a physical bound is a test.** "Under 0.01 ms" missed a 1494×
   reading because it was calibrated on a shape 300× smaller. "Faster than the thread count allows"
   needs no calibration.
2. **Measure the control you already have.** `MAPAL_PAR=1` cannot race, so any spread it shows is
   the machine — that one comparison separated the i9's 5× powersave ramp from the defect.
3. **Build the "before" from a worktree, not from memory.** A pre leg compiled at the old commit in
   `/tmp/s36pre` makes the A/B paired and same-session.
4. **A benchmark's baseline must build on the measurement machine.** `ladder2_baseline.cpp` did not
   compile under gcc 16 — found only by running it there.
5. **Check the emitted artifact, not the intent, when labelling an A/B.**
   `grep -c 'call void @mapal_par_run_pinned'` is 0 pre and 2 post; the first attempt grepped a
   string that matched the *declaration* in both.
6. **Re-measure a baseline before fixing it.** S33 recorded 3–4/100; HEAD measured 6/100. The
   acceptance criterion is a delta, so the reference has to be current.
7. **A fix that raises the number can still be the fix.** Wall time unchanged + interval up 12% =
   the old measurement was excluding work.
8. **Check whether the machinery exists before designing a mechanism.** The emitter half of S36 was
   zero lines — S24's pinned tasks already did it.
9. **Do not promote a watermark wait to a completion wait without asking what it waits for.** The
   tempting reuse of the checkpoint wait list would have made `t0` wait for the kernel it opens.
10. Plus S35's six: never pipe a test summary through `head` (`grep -E "FAILED|panicked"` instead);
   `a && b; c` still runs `c`; a mechanical rename can invalidate a golden without touching its
   logic; answer architecture questions from the code; price a refactor before arguing about it;
   pre-register predictions.
11. Plus S34's eight and S33's eight — pin the CPU; `ref-cycles` not `cycles`; ratios inside one run;
   `cargo test` does not rebuild `libmapal_rt.a` (`cargo build -p mapal-rt --release` first);
   `calloc` is not a pre-fault; verify a kill; a test that passes wrongly is worse than none.

## Gotchas / warnings

- **`par` numbers published before S36 are stale in a specific direction: too fast.** Do not compare
  a post-S36 measurement against a pre-S36 published cell and call it a regression.
- **A clock read is now a DAG node, so it costs an ordering.** Work written after a `time` read
  cannot overlap the read. That is deliberate — the overlap is what corrupted the measurement — but
  it means `time` is not free to sprinkle: each read is a barrier in both directions for the tasks
  written around it. Total wall time was unchanged on fir; a program with genuinely independent work
  straddling a read would pay.
- **The extension is provisional** (ADR-0037 D4). `.mal` is the only other free short option; check
  Linguist *and* `$VIMRUNTIME/syntax/` before any replacement.
- **`docs/sessions/`, `ADR-0001…0036`, `docs/performance/` and recorded `results*.csv` still say
  "Flow" on purpose** (ADR-0037 D3). Do not "fix" them. Lowercase *flow* also remains the name of
  the language construct — `a -> b;` is a flow statement, and ADR-0005 holds verbatim.
- **Old bench CSVs use `flow-llvm-*` leg labels; the harness now writes `mapal-llvm-*`.**
- **`map(id) → id` refuses bodies containing `Widen`/`Iota`/`Fill`.** All three are legal to forward;
  admitting them is its own change with its own pins.
- **User-side leftovers outside the repo:** `~/.config/nvim/lua/plugins/flow.lua`,
  `~/Library/Fonts/FlowIcons.ttf`, and the old `flow-lang` VS Code extension.
- **The local directory is still `/Volumes/LessComplex/Personal/Flow`.** Renaming it means restarting
  the session there; git does not care.
- A stale S33 worktree (`…/scratchpad/pre` @ `1daddaa`) is still registered — `git worktree prune`.
- **The i9's governor is `powersave` (intel_pstate) and there is no passwordless sudo**, so unpinned
  numbers there carry a frequency ramp — visible as a 5× spread on the `MAPAL_PAR=1` leg, which
  cannot race. Pin with `taskset -c 0` (1t) and `-c 0-15` (par, the 8 P-cores; 16–31 are E-cores at
  4.3 GHz); `benches/results-s36/run_pinned.sh` is the driver.
- **The Arch i9 is the measurement machine**, key auth, `<perf-box>`. No clang there: cross-compile
  on the Mac (`-target x86_64-unknown-linux-gnu -march=raptorlake`) and link with gcc. `~/flowbench`
  and `~/flowbench_pre` are built and left in place under their old directory names.
- **`reside` pins pages to the touching thread's NUMA node.** Irrelevant single-socket, live on a
  dual-socket EPYC. A/B it or make `reside` lane-aware before any multi-socket run.
- **The scheduler advantage is heterogeneity tolerance, not a better scheduler.** On uniform cores
  OpenBLAS beats us — 41% on 8 P-cores. Do not restate it as a general claim.
- **VS Code extensions must be installed as a `.vsix`** — a copied or symlinked folder is ignored
  *silently*.
- Editor grammars have **opposite precedence** (Vim last-match-wins, TextMate earliest-match-wins).
  `sh editors/test.sh` after any edit.
- **Repo is public.** The perf box address is scrubbed to `<perf-box>`; keep it that way.
- `kc_nest` stays default-OFF; `oversub` is 1 for `Sliding` reads by design; the fma legs are
  numerically-equal-not-byte-equal BY DESIGN.

## Standing direction (Sapir — unchanged)

- **Compute-only legs; numpy in every verdict table; scale everything up.**
- **Parallel-first by construction.**
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a mapal-ir
  query or emitter-local cashing with zero mapal-ir change. mapal-ir never learns machine facts.
- **Type system = precision contracts; backend config = performance tailors.**
- **Compile time decides the SIZES, runtime decides the ASSIGNMENT.**
- **Nothing goes in the README that a default build does not deliver.**
- **Proof over suggestion:** a change arrives with the measurement of what it did, and names the
  published numbers it moves — the house rule in `CONTRIBUTING.md`.
