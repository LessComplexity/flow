# 2026-07-26 — S33: the conv2d gap was a measurement boundary, OpenBLAS parity on AVX2, and going public

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-26-s31-s32-deduced-blocking-and-scheduling.md`. **18 commits, `1daddaa..a6aa0da`**,
pushed to a now-public repo.

Driven by Sapir, in order: *"conv2d gap (P0)"* → *"rerun the full bench suite"* → *"run the
numbers on the arch machine too"* → *"I'm curious about the 1t numpy to 1t flow gap"* → *"create
a repo on my account, public"* → *"README has too much jargon"* → *"logo on .flow files"* →
*"broaden the tests"*.

## 0. Continuation brief

Current state: **everything committed and pushed; both machines idle; `main` clean at
`a6aa0da`.** The S31/S32 P0 is **closed and inverted** — conv2d was never slow. Two new P0s were
found, one of them by CI on its first run.
Next step: **the rewriter deletes a trap that must fire** (§5). Sapir approved going after it and
then deferred it twice; it is pinned as a proptest seed so CI is red until fixed.
Resume command/check: `docs/next-session.md`, then
`cargo test -q -p flow-rewrite --release --test property open_default` — reproduces in ~0.2 s.

## 1. The headline: conv2d's "1.55× per-core gap" was never a kernel defect

`flow_rt_alloc` returned a reserved address range, not memory. A large `alloc` is served by
`mmap`, which delivers **no** physical pages. The emitter allocates in the entry-block prologue
but tasks store later, so the first store to each page trapped into the kernel (2 MiB of zeroing
apiece under THP) **inside whatever `() -> time` region wrote first**.

conv2d exposed it because **nothing writes its output array until the convolution itself does**.
The C++ baseline never pays it inside its window: `std::vector<float> out(n)` value-initialises
above `run_iters`.

Proven **before** the fix existed, by exact differenced counters (i9, warm, pinned; kernel
isolated by differencing a gen-only build):

| leg | cycles | **ref-cycles** | instructions | IPC |
| --- | ---: | ---: | ---: | ---: |
| flow `task7` | 905,100 | **299,221** | 2,034,188 | **2.25** |
| cpp `conv_range` | 1,072,928 | **382,489** | 1,914,675 | **1.78** |

`ref-cycles` is fixed-rate, so frequency-invariant. Method validated against C++: 382,489 →
**0.191 ms** vs its self-timed 0.194, i.e. its window is 98% kernel. Flow's kernel: **0.150 ms**
against a **0.258 ms** window. The fix landed the window at **0.144 ms** — two independent
methods 4% apart.

Shipped `reside` (one byte per 4 KiB after each `alloc`; **not** a memset — the fault's own
zeroing IS the initialisation, and a memset would double traffic, ~15 ms wasted on a 64 MB
matmul frame). **No emitter change, no flow-ir change, no `.ll` moved.**

**It is not a speedup.** Total process wall time 1.485 → 1.498 ms (+0.9%). It moves a real cost
out of a window that was never meant to contain it.

**The recorded "IPC 3.11 vs 1.57" was process-level**, contaminated by Flow's gen legs (IPC
0.86–1.04) whose instruction counts differ ~7× from C++'s. Never quote process IPC for a kernel.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Build a repeat-loop bench (S31/S32's P0 prerequisite) | **discarded as unnecessary** | Both kernels are distinct symbols, so per-symbol `perf` attribution reads counters out of a diluted process. Exact differencing against a gen-only build is better still |
| Boundary: pre-fault Flow vs make baselines allocate inside | **pre-fault Flow** (Sapir) | Measures the kernel, which is what the shapes benches claim to measure |
| `reside` as memset (what `std::vector` does) | **rejected** | The fault already zeroes the page; memset zeroes it twice |
| Runtime-only dispatch ceiling for the `flow_par_wait` race | **built, measured, REVERTED** | Did not fix it (5/100 vs 4/100), and closing the launch window broke two tests that are right. Only the emitter knows where the first checkpoint is — §5 |
| Quote min for par legs | **rejected, rule inverted** | This race makes FAST outliers; min is maximally vulnerable and worsens with N. **min for 1t, median for par** |
| i9 4096 naive-1t cells | **not run** | >10 min/iteration; ~108 min for two cells the smaller sizes settle. Skipped for time, recorded as such |
| Purge `.ll`/`.cu` from git history | **not done** | Needs a force-push rewriting published history; offered, not taken |
| VS Code file icon theme | **rejected** | Icon themes are all-or-nothing; would blank every other file type. Used `contributes.languages[].icon` |
| Patch a Nerd Font with our glyph | **rejected** | Redistributes someone else's font under their licence. Shipped our own single-glyph font instead |
| Label scope `entity.name.label` | **rejected after measuring the theme** | Default themes style it `#C8C8C8` against `#CCCCCC` foreground — invisible. Now `keyword.control.label.flow` |

## 3. The AMX question, settled by cross-machine measurement

Sapir's hypothesis: the M4's numpy win is the AMX coprocessor. **Confirmed.** Same Flow, same
numpy source, only silicon differs — 1024² f32:

| | flow-1t vs numpy-1t | flow-par vs numpy-threaded | numpy backend |
| --- | ---: | ---: | --- |
| M4 Pro | numpy **13.5×** ahead | numpy **3.3×** ahead | Accelerate → **AMX** |
| i9-14900F | numpy **1.21×** ahead | **tie** (1.526 / 1.506) | OpenBLAS 0.3.30 → AVX2 |

**Parity, not victory**, and recorded in full rather than by its best cell. 1t is a flat **1.20×
behind** at 1024/2048/4096 (146 vs 174 GFLOP/s, both size-invariant ⟹ a steady micro-kernel
deficit, not a blocking failure). Threaded is within **±10%**: ahead 1.08× at 2048, behind 1.24×
at 512 and 1.06× at 4096. On the **untuned `generic`** profile.

Confound ruled out: OpenBLAS default == `OPENBLAS_NUM_THREADS=32` (1.5036 vs 1.5055).

### The scheduler claim was tested and REFUTED as stated

Flow scales 9.2–9.8× vs OpenBLAS's 7.1–8.1×, which invites "our scheduler is better". Controlled
test — same box, same binaries, **8 threads in every cell**, only CPU uniformity varying:

| 8 threads on… | flow | numpy | verdict |
| --- | ---: | ---: | --- |
| 8 E-cores (uniform) | 5.894 | 5.591 | numpy 5% ahead |
| 8 P-cores (uniform) | 2.438 | 1.724 | **numpy 41% ahead** |
| 4 P + 4 E (mixed) | 3.384 | 5.573 | **flow 1.65× ahead** |

**On uniform cores OpenBLAS wins. Flow wins only on mixed cores.** Mechanism visible in numpy's
own column: 8E **5.591** → 4P+4E **5.573**, so four 35%-faster cores bought OpenBLAS *nothing*
(static partitioning waits on the slowest thread) while Flow went 5.894 → 3.384. So it is
**heterogeneity tolerance, not a better scheduler** — real (consumer CPUs are hybrid) but not a
claim about a homogeneous server.

## 4. Every number, and what the fix was worth

Alternating same-session A/B (PRE/POST from the same `.ll`, linked against `HEAD~1` and `HEAD`,
alternated per iteration so drift cannot bias one leg), min-of-9, `FLOW_PAR=1`:

| bench | M4 PRE→POST | M4 | i9 PRE→POST | i9 |
| --- | --- | ---: | --- | ---: |
| conv2d 512 | 0.1079 → 0.0571 | **1.89×** | 0.0370 → 0.0372 | **1.00×** |
| conv2d 1024 | 0.4389 → 0.2553 | **1.72×** | 0.2169 → 0.1610 | **1.35×** |
| fir 65 536 | 0.1323 → 0.1127 | 1.17× | 0.1151 → 0.0849 | 1.36× |
| fir 1M | 1.7627 → 1.5767 | 1.12× | 1.4537 → 1.3516 | 1.08× |
| matmul 512/1024/2048/4096 | — | 1.09/1.02× | — | 1.12/1.04/1.01/**1.003×** |

The effect scales as *output ÷ kernel length*; matmul's four sizes trace it cleanly to nothing.
**Platform-dependent**: conv2d 512 gains 1.89× on macOS's 16 KiB pages and **exactly nothing** on
Linux, where a 1 MB output hides inside a 2 MiB huge page its neighbours already faulted.

Full suites, both machines, raw logs: `docs/performance/matmul/s33.md`, `benches/results-s33/`.

## 5. ⚠ Two P0s, both open

### 5a. The rewriter deletes a trap that must fire — **found by CI on its first run**

```
open_default: before Trapped(DivZero)  !≈  after Done(Scalar(I32(3)))
```

The original program traps on division by zero; after `rewrite()` it returns 3. That breaks the
property the project rests on. The suite already guards it by hand
(`dead_trapping_div_stays_trapped`); the generator found a shape the guard misses.

- **Pre-existing** — reproduces identically at `1daddaa`, this session's start. Never *drawn*
  locally, which is the argument for CI.
- **Pinned** as a proptest regression seed, so CI stays red until fixed rather than passing on a
  lucky seed.
- **Counterexample is NOT minimal** — proptest hit its 192-iteration shrink limit. First step is
  re-shrinking with a raised `PROPTEST_MAX_SHRINK_ITERS` to find which pass drops the trap.
- Repro: `cargo test -q -p flow-rewrite --release --test property open_default` (~0.2 s).

### 5b. `flow_par_wait` lets workers run ahead of the clock

Workers do not stop at checkpoints: once a wait's condition is satisfied the DAG unlocks the next
tasks and they begin **while the host is still between the wait and its next instruction**, so a
kernel can finish before the clock meant to bracket it is read. 3–4% of threaded runs; one live
case self-timed a 1024² matmul at **0.0001 ms**. `FLOW_PAR=1`: 0/100.

**A runtime-only fix was built and reverted.** Two measured reasons, both in
`plan-s33b-clock-read-barrier.md` §4 as do-not-retry: it did not fix the race (5/100 vs 4/100,
because the window between `flow_par_launch` and the first wait is wide enough to lose in), and
closing that window broke `watermark_wait_can_finish_before_task_completion` and
`wait_helps_while_the_background_worker_is_busy` — which are right, because **launch must dispatch
immediately and at launch the runtime does not know where the first checkpoint is.** Only the
emitter does; once it supplies the ordering, a DAG edge beats a second mechanism shadowing the DAG.

Consequence: **every `par` minimum in S28–S32 is suspect**, and the S32 scheduling verdict
(1.41–1.43×) needs re-confirming under a median.

## 6. Going public, and the editor work

Repo: **https://github.com/LessComplexity/flow** — public, `main`, full history, 12 topics.
Scrubbed first: the perf box's Tailscale address/username, and `/Users/lesscomplex/...` from
`editors/nvim/README.md`, whose setup instructions pointed users at a path on one machine. Secret
sweep clean. Noted in-commit that this edited two files under `docs/sessions/`, which ADR-0017
makes immutable — deliberate, privacy over immutability.

**CI added** (`.github/workflows/ci.yml`): fmt + full suite on Linux and macOS. Deliberately
cannot pass vacuously — the LLVM differential *skips itself* without clang, so clang is installed
and a follow-up step fails the run if the suite reports a skip.

**Artifacts untracked**: 112 emitted `.ll`/`.cu` (4.4 MB, ~40% of tracked content). `regen.sh` had
to be rewritten with them: every branch was `if [ -f "$stem.ll" ]`, so the checked-in output was
its own worklist and deleting it would have silently reduced the script to doing nothing. Closed
two recorded open items in passing — the "72 stale `.ll`" and "regen.sh exits 1 on the CUDA leg"
had one cause, S30b's `time` migration against a backend with no clock seam.

**README** rewritten results-forward (390 → ~300 lines, every number kept), with *The idea*
restored at Sapir's request and category theory given its own section, argued from where it is
load-bearing: `ci == 0` and `ci == cq` are one predicate at q=0/q=1, so the conv2d rung fell out
of the model (~60 lines, −25% at 1t); and the coherence laws are what located this session's
measurement bug, as an undeclared transmission.

**Editors.** Logo as SVG (teal `#14B8A6`) + **our own single-glyph font** `FlowIcons.ttf` at
U+F8F0, because the Rust/C++ marks in a file tree are font glyphs, not images. Full TextMate
grammar for VS Code. Test coverage **15 → 61 assertions** across both editors with cross-editor
consistency checks. Five editor bugs, all found by Sapir's eyes first:

| bug | cause |
| --- | --- |
| icon never appeared | required a manual `setup()`, and `ft = "flow"` never loads while you look at a file tree |
| fallback glyph blank | a literal private-use char did not survive being written to the file |
| `iota` mis-painted | builtin list predated 8 builtins; a missing name falls through and is *mislabelled*, not merely uncoloured |
| `print`/`println`/`ret` mis-scoped in VS Code | TextMate picks the **earliest** match; `(?<=->)\s*(ident)` starts at the whitespace, one column before the keyword rule |
| labels invisible in VS Code | `entity.name.label` is themed `#C8C8C8` against a `#CCCCCC` foreground |

VS Code install also needed correcting twice: a relative `ln -s` dangles, and **current VS Code
ignores unregistered folders entirely** — it reads `extensions.json`. Fixed with
`package-vsix.py`, which builds a `.vsix` without npm.

## 7. Method notes earned the hard way

1. **Pin, always.** Two unpinned readings on the hybrid i9 produced confident wrong conclusions: a
   "2.2× g++-vs-clang spread" that vanished under `taskset`, and "2.37× cold / 1.12× warm" that
   min-of-40 refuted (1.56× / 1.33×).
2. **`ref-cycles` separates frequency from time.** `cycles` cannot tell a slower clock from more
   work.
3. **`cargo test` does not rebuild `target/release/libflow_rt.a`.** The first M4 measurement of the
   fix linked the stale runtime and read "no change at all". A stale staticlib presents exactly as
   a fix that does nothing.
4. **`calloc` is not a pre-fault.** An early probe using it appeared to refute the whole diagnosis:
   both arms faulted inside the window, and glibc's region was not 2 MiB-aligned so it took ~1024
   small faults instead of 3 huge ones. **Page size is decided by alignment, not request size.**
5. **Baselines drift across sessions; ratios within one run do not.** cpp-1t fir 65 536 read 1.3024
   here against S28's 0.9239 — a 41% swing in the *baseline*. I violated this once mid-session
   (claiming fir was a "2.07× correction" against a recorded number) and the A/B refuted it.
6. **Verify a kill.** An orphaned `mm_cpp_1t` survived a parent-only `kill` because the ssh
   carrying the child `pkill` returned 255 and I read it as a blip. It burned a core at 95% through
   two measurement rounds (~20% error on threaded cells, 2× on `numpy-1t` at 4096). Scripts now
   assert live-process count and loadavg in their own output.
7. **A test that passes wrongly is worse than none.** Three times this session: the TextMate driver
   ignored TextMate's earliest-match rule; it dropped begin/end regions so a string assertion
   passed by accident; and three guard assertions could not fail because their scopes are also
   produced by the plain number/boolean/type rules. All found by **negative control** — break the
   rule on purpose, check the test notices.
8. **Check a scope against the theme, not only the grammar.** A scope can be correct, matched, and
   invisible.

## 8. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ `a6aa0da` | **clean, pushed**, 0 ahead/behind | `git status --short` |
| remote | `github.com/LessComplexity/flow` | **public**, 12 topics, CI wired | `gh repo view LessComplexity/flow` |
| CI | 4 runs queued/running | **RED and correctly so** — §5a is pinned | `gh run list` |
| arch box | `<perf-box>` | **idle**, load 0.02 | `ssh -o BatchMode=yes …` |
| box dirs | `~/flowbench` 182 MB, `~/flowbench_pre` 21 MB | left in place, both runtimes built | `du -sh ~/flowbench*` |
| vast.ai | account | **0 instances** | `vastai show instances` |
| local procs | none | two orphaned waiter shells killed at close | `pgrep -f matmul_ab` |
| worktree | `scratchpad/pre` @ `1daddaa` | pre-`reside` runtime, kept for A/B | `git worktree list` |
| user config | `~/.config/{nvim/lua/plugins/flow.lua,kitty/kitty.conf}` | edited, `.bak-flow` backups | — |
| user fonts | `~/Library/Fonts/FlowIcons.ttf` | installed | `:FlowIcon` |
| extensions | VS Code + Cursor | `flow-lang.flow-lang` v0.1.0 | `code --list-extensions \| grep flow` |
| proptest seed | `crates/flow-rewrite/tests/property.proptest-regressions` | **committed**, keeps CI red | — |

## 9. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | Rewriter deletes a trap | §5a | Re-shrink with `PROPTEST_MAX_SHRINK_ITERS=100000`, find the pass | `open_default` green with the seed retained |
| **P0** | `flow_par_wait` clock barrier | `plan-s33b-clock-read-barrier.md` | Make the clock read a DAG node; **do not retry the runtime ceiling** (§4 there) | fir 65 536 `FLOW_PAR=14`: 0/100 under 0.01 ms |
| P1 | Re-confirm S32 scheduling | §5b | Re-measure 1.41–1.43× under a median | verdict restated on a sound statistic |
| P1 | Re-measure conv2d/fir through the harness | s33.md §8 | `shapes_ab.sh`; do not publish the hand-linked figures | README rows re-derived from a harness run |
| P1 | matmul boundary immunity | s33.md | Expected <1%, **unverified** at 4096 | measured or the claim dropped |
| P2 | The 1.20× 1t kernel gap vs OpenBLAS | §3 | A real Raptor Lake profile; finish the packing ladder | 146 GFLOP/s moves toward 174 |
| P2 | NUMA first-touch in `reside` | `plan-s33-timed-window-boundary.md` §5 | A/B a parallel leg on a multi-socket host, or make `reside` lane-aware | measured on dual-socket |
| P2 | Fold tap 0 into `fmul` | `emit_conv_block_tile` | 16 of 274 instructions (~6%) | `movi` count drops |
| P2 | GitHub licence not detected | — | LLVM exception makes it non-verbatim Apache-2.0; cosmetic | badge appears, or accepted as-is |
| P3 | Purge `.ll`/`.cu` from git history | §6 | `filter-repo` + force-push; **needs Sapir** | `.git` under 20 MB |

## 10. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/conv2d-per-core-gap.md` | OPEN → **RESOLVED**, full evidence chain |
| `performance/matmul/s33.md` | new; both machines, the AMX result, the scheduler refutation, the race |
| `performance/matmul.md`, `performance/README.md` | S33 rows; conv2d line flipped |
| `components/backend-llvm/plans/plan-s33-timed-window-boundary.md` | new; SHIPPED + as-built, incl. a withdrawn acceptance criterion |
| `components/backend-llvm/plans/plan-s33b-clock-read-barrier.md` | new; **OPEN**, one approach reverted with reasons |
| `docs/STATUS.md` | S33 header |
| `README.md` | rewritten; Tests/CI rows now say red, and why |
| `editors/{nvim,vscode}/README.md`, `assets/font/README.md` | new/rewritten |
| memory `conv2d-gap-is-architecture-independent` | inverted — it asserted the opposite |

## 11. Files changed

Code: `crates/flow-rt/src/lib.rs` (`reside`). New: `.github/workflows/ci.yml`,
`assets/{logo*.svg,font/*}`, `editors/vscode/*`, `editors/nvim/{plugin,lua,test}/*`,
`benches/results-s33/*`, `crates/flow-rewrite/tests/property.proptest-regressions`. Removed: 112
emitted `.ll`/`.cu`. Rewritten: `benches/matmul/regen.sh`, `README.md`.

Gate: `cargo test --workspace --release` **72 suites green on macOS**; `fmt` clean; editor suites
61 assertions green. **CI red on ubuntu-latest for §5a, deliberately.**
