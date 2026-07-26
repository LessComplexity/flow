# Next Session (S35)

Written: 2026-07-26 · end of S34 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-26-s34-mapal-rename-and-trap-deleting-rewrite.md` — **read §9
(method notes) before any rename-shaped or measurement-shaped work.**

## Where things stand (≤6 lines)

**The project is now `Mapal`; source files are `.mapal` (ADR-0037).** S33's top P0 is closed at the
root: `map(id) → id` judged a map body by its Return writer alone, so a body that returns its
parameter *and* computes a dead trapping `Div` read as the identity and the whole `Map` was aliased
away. The guard now quantifies over the body's entire morphism set via the crate-shared `is_pure`.
**The gate is green** — full suite, differential 36/36, fmt, 61 editor assertions — and CI went
green for the first time since it was added, at ~35 min of machine time (was 43). The remaining P0
is the `mapal_par_wait` clock race: measurement-only, but it flatters us.

## FIRST commands (resume checks, in order)

```sh
git log --oneline -3                  # HEAD is the S34 close commit
git status --short                    # expect empty
gh run list --limit 3                 # expect success on main
cargo test -q -p mapal-rewrite --release --test property   # 11 green, ~0.3 s
sh editors/test.sh                    # 61 assertions, expect green
gh auth status                        # NOTE: `sapiritur` is active; the repo is LessComplexity's
```

**Pushing needs the right account.** `gh` has two logins and the active one cannot write to
`LessComplexity/mapal`. Either `gh auth switch --user LessComplexity`, or push with an explicit
token for that account.

## S35 focus

### 1. P0 — `mapal_par_wait` lets workers run ahead of the clock

Workers do not stop at checkpoints, so a kernel can finish before the clock meant to bracket it is
read. 3–4% of threaded runs; one live case read **0.0001 ms** for a 1024² matmul. `MAPAL_PAR=1` is
0/100, so every single-threaded number in the repo is sound.

**Do NOT retry the runtime-only dispatch ceiling.** It was built, measured and reverted;
`components/backend-llvm/plans/plan-s33b-clock-read-barrier.md` §4 records both reasons as
do-not-retry. Short version: launch must dispatch immediately, and **at launch the runtime does
not know where the first checkpoint is** — only the emitter does. Make the clock read a DAG node
with edges both ways.

Sharp acceptance criterion: `watermark_wait_can_finish_before_task_completion` and
`wait_helps_while_the_background_worker_is_busy` must pass **unmodified** — they are the guard rail
the first attempt tripped.

### 2. The measurement debt this unblocks

- **Re-confirm S32's scheduling verdict** (1.41–1.43×) under a median. Every `par` minimum in
  S28–S32 is suspect.
- **Re-measure conv2d and fir through `shapes_ab.sh`.** The S33 figures are hand-linked; do not
  publish them as harness numbers.
- **matmul boundary immunity** is expected <1% but unverified at 4096.
- Once the race is fixed, the statistic rule reverts: **min for both 1t and par**.

### 3. One decision waiting on Sapir

**Halve the per-push differential cross product.** That suite is 1,391 s of a ~1,475 s Ubuntu test
step — 94%. It runs 320 generated programs × {raw, rewritten} × {`-O0`, `-O2`} = 1,280
compile-and-runs *per push*. Running raw@`-O0` and rewritten@`-O2` (both forms and both levels
still exercised every run, just not all four combinations) takes ~12 min more off Ubuntu, with the
full cross product on a nightly schedule. It changes the README's published "1,280 comparisons per
run" claim, so it needs a decision plus a doc edit, not a silent change.

## Rules that bit S34 (log §9)

1. **A sweep over `git ls-files` cannot see untracked files.** It skipped the Homset draft, then
   skipped ADR-0037's own extension references — the document deciding the extension was the one
   file the extension pass could not touch. Stage first, or walk the filesystem.
2. **`\bflow_` misses `perf_timing_flow_main`** — `_` is a word character. Word boundaries are the
   wrong tool for snake_case identifier renames; enumerate symbols and grep for leftovers.
3. **`gh run watch … | tail` returns `tail`'s exit code.** Two runs looked green and were
   `cancelled`. Re-read with `gh run view --json conclusion`.
4. **Do not push on top of a CI change you are measuring** — `cancel-in-progress` kills the run
   that was producing your number.
5. **A file extension is a namespace.** Check `$VIMRUNTIME/syntax/` and Linguist's
   `languages.yml`: `.mp` is MetaPost, `.mpl` is JetBrains MPS, `.ml` is OCaml/SML, `.map` is
   source maps. Only `.mapal` and `.mal` were free.
6. **Run `cargo fmt --all` after any mechanical rename** — identifier lengths change and rustfmt
   re-wraps, failing `--check` for a reason unrelated to the change.
7. **Renaming text is not renaming a logo.** The wordmark clipped `mapal` silently; render the
   asset and look at it.
8. Plus S33's still-standing eight: pin the CPU; `ref-cycles` not `cycles`; ratios inside one run;
   `cargo test` does not rebuild `libmapal_rt.a` (`cargo build -p mapal-rt --release` first);
   `calloc` is not a pre-fault; verify a kill; a test that passes wrongly is worse than none; min
   for 1t, median for par.

## Gotchas / warnings

- **The extension is provisional** (ADR-0037 D4). `.mal` is the only other free short option; any
  replacement must be checked against Linguist *and* `$VIMRUNTIME/syntax/` first. Cost of
  changing: one scripted pass, ~49 files and ~300 references, then re-run the gate.
- **`docs/sessions/`, `ADR-0001…0036`, `docs/performance/` and recorded `results*.csv` still say
  "Flow" on purpose** (ADR-0037 D3). Do not "fix" them. Lowercase *flow* also remains the name of
  the language construct — `a -> b;` is a flow statement, and ADR-0005 holds verbatim.
- **Old bench CSVs use `flow-llvm-*` leg labels; the harness now writes `mapal-llvm-*`.** Two
  vocabularies in the results tree, deliberately.
- **`map(id) → id` now refuses bodies containing `Widen`/`Iota`/`Fill`.** All three are legal to
  forward; admitting them is its own change with its own pins (rewrite STATUS, "S34 headroom").
- **User-side leftovers outside the repo:** `~/.config/nvim/lua/plugins/flow.lua` (stale path and
  `ft = "flow"`, plus a `.bak-flow`), `~/Library/Fonts/FlowIcons.ttf`, and the old `flow-lang` VS
  Code extension. Rebuild with `python3 editors/vscode/package-vsix.py`.
- **The local directory is still `/Volumes/LessComplex/Personal/Flow`.** Renaming it means
  restarting the session there; git does not care.
- A stale S33 worktree (`…/scratchpad/pre` @ `1daddaa`) is still registered — `git worktree prune`.
- **The Arch i9 is the measurement machine**, key auth, `<perf-box>`. No clang there:
  cross-compile on the Mac (`-target x86_64-unknown-linux-gnu -march=raptorlake`) and link with
  gcc. `~/flowbench` (182 MB) and `~/flowbench_pre` (21 MB, pre-`reside`) are built and left in
  place — note they keep their old directory names.
- **`reside` pins pages to the touching thread's NUMA node.** Irrelevant single-socket, live on a
  dual-socket EPYC. A/B it or make `reside` lane-aware before any multi-socket run.
- **The scheduler advantage is heterogeneity tolerance, not a better scheduler.** On uniform cores
  OpenBLAS beats us — 41% on 8 P-cores. Do not restate it as a general claim.
- **The i9 1t gap ran on the untuned `generic` profile** — some of the 20% is probably recoverable.
- **VS Code extensions must be installed as a `.vsix`** — a copied or symlinked folder is ignored
  *silently*, because VS Code reads `extensions.json` as its registry.
- Editor grammars have **opposite precedence** (Vim last-match-wins, TextMate earliest-match-wins).
  A rule added to one goes at the other end of the other. `sh editors/test.sh` after any edit.
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
  published numbers it moves — now written into `CONTRIBUTING.md` as the house rule.
