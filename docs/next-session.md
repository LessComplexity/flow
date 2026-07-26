# Next Session (S37)

Written: 2026-07-27 · end of S36 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-27-s36-clock-read-barrier.md` — **read §10 (method notes) and §3
(the interval went UP) before republishing any `par` number.** Previous: S35
(`sessions/2026-07-26-s35-shape-ladder-and-the-ast-question.md`).

## Where things stand (≤6 lines)

**S33's last P0 is closed.** A clock read is no longer a bare host-spine checkpoint: `path_plan`
makes it a **pinned DAG node** with edges both ways, so the work written after a `time` read cannot
be dispatched until the read fires. fir 65 536 at `MAPAL_PAR=14`: **6/100 readings under 0.01 ms →
0/100**, values byte-identical, total wall time unchanged. It landed entirely in `mapal-ir` —
`crates/mapal-rt/` has a zero-line diff and the emitter needed no new code, because the pinned-task
machinery from S24 already carried it. **The self-timed interval RISES ~12%: every published `par`
number from S28–S35 was under-reported, and re-measuring them is now P1.** Gate: 972 passed.

### Previously (S35)

Four non-compute shape classes (saxpy, reduce, transpose, gather) measured and published, losses
included; the finding is that a plain `map` is not a tile site, so streaming kernels emit **scalar
loops** — 3.7× behind naive C++ at one thread. Separately: the compiler **does** have an AST and the
README said it did not; corrected to "the syntax is a serialization of the execution graph."

## FIRST commands (resume checks, in order)

```sh
git status --short                    # S36's work may still be UNCOMMITTED — see below
git log --oneline -3
gh run list --limit 3
cargo test --workspace --release --no-fail-fast 2>&1 | grep -E "FAILED|panicked|test result"
gh auth status                        # NOTE: `sapiritur` is active; the repo is LessComplexity's
```

**If `git status` is dirty, S36 ended before committing.** The change is five files:
`crates/mapal-ir/src/algo.rs`, `crates/mapal-ir/tests/algos.rs`,
`crates/backends/llvm/tests/golden_ll.rs`, plus the docs listed in the S36 log §9. The gate was
green at 972 passed before the session closed.

**Pushing needs the right account.** `gh` has two logins and the active one cannot write to
`LessComplexity/mapal`. Either `gh auth switch --user LessComplexity`, or push with an explicit
token for that account.

## S37 focus

### 1. The measurement debt S36 unblocks — and enlarged

The race is gone, so `par` statistics are trustworthy again. Two consequences, in order:

- **Every published `par` cell is now known to be under-reported.** The kernel used to get a head
  start before `t0`; it no longer does. Re-run the ladder through `benches/shapes/shapes_ab.sh` and
  `ladder2_ab.sh` before any of those numbers is quoted again.
- **Re-confirm S32's scheduling verdict (1.41–1.43×)** under both min and median. It needs the
  pre-S32 A/B leg rebuilt — its own campaign, but no longer blocked on a bad statistic.

**Which statistic.** Not "min everywhere" again. S36 measured the residual min/median gap and it
tracks kernel size — 0.57 at fir 65 536, **0.84** at fir 1 048 576, zero sub-0.01 readings at
either. That is pool wake-up jitter. Min is trustworthy where the kernel dominates wake-up; small
cells stay on medians for a reason unrelated to the clock (plan-s33b §7).

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

## Rules that bit S36 (log §10)

1. **Re-measure a baseline before fixing it.** S33 recorded 3–4/100; HEAD measured 6/100. The
   acceptance criterion is a delta, so the reference has to be current.
2. **A fix that raises the number can still be the fix.** Wall time unchanged + interval up 12% =
   the old measurement was excluding work.
3. **Check whether the machinery exists before designing a mechanism.** The emitter half of S36 was
   zero lines — S24's pinned tasks already did it.
4. **Do not promote a watermark wait to a completion wait without asking what it waits for.** The
   tempting reuse of the checkpoint wait list would have made `t0` wait for the kernel it opens.
5. Plus S35's six: never pipe a test summary through `head` (`grep -E "FAILED|panicked"` instead);
   `a && b; c` still runs `c`; a mechanical rename can invalidate a golden without touching its
   logic; answer architecture questions from the code; price a refactor before arguing about it;
   pre-register predictions.
6. Plus S34's eight and S33's eight — pin the CPU; `ref-cycles` not `cycles`; ratios inside one run;
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
