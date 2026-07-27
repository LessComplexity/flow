# 2026-07-27 — S36d: the front page, the editor tooling, and why we don't fuse

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Closes the S36 block —
continues `2026-07-27-s36c-the-real-gaps.md`. Repository: `github.com/LessComplexity/mapal`.

Driven by Sapir: *"Don't forget to change stale readme numbers too if necessary"* → *"I don't need
per fix post fix. Only current, front facing numbers. Pre or post fixes belong in status/session
logs not readme"* → *"because of the name change flow to mapal, syntax highlighting and icons are
broken on vscode; on nvim the icons are broken"* → *"by default compilers do the fma and we don't,
why? Maybe this should be the default? Or should/can it be deduced at mapal ir?"*

## 0. Continuation brief

Current state: **the front page carries current numbers only, the editor tooling works again, and
the FMA question has an answer with a plan attached.** Gate green. Everything pushed.
Next step: **P0 — the two layout facts** (`%Frame` alias barrier, `iota` as an index law), each
needing its own plan; they are worth 2.3× and 3.1× and put saxpy at parity with `clang -O3`.
Resume command/check: `docs/next-session.md`, then `git log --oneline -5`.

## 1. The README now states what the compiler does today

Sapir's rule, restated: *pre/post-fix belongs in status and session logs, not the front page.*
Removed the fix narration, the validation-campaign summary and every "pre-S36" caveat. What
replaced them is one session's measurements on one machine, both build faces:

| N | conformance | FMA | C++ naive-mt | Rust naive-mt | NumPy 1t | NumPy mt |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1024 | 3.65 ms | **2.25 ms** | 125 | 117 | 1.29 | 0.69 |
| 4096 | 245 ms | **155 ms** | 33,439 | 33,574 | 90.5 | 44.3 |

55× the naive baseline at 1024², 216× at 4096². The equal-hardware section states the current
pinned-i9 comparison (1.21× behind at one thread, 1.23× threaded) and drops the older
whole-machine "tie", which was measured with the clock race live.

**The 4096 row is a real measurement now.** It had been inherited: no `matmul4096_cap_f32.mapal`
existed (only stale `.ll`), and `gen_flow.py` emits the *untimed* loop form, so the row could not be
reproduced. Wrote the cap-form twin of the 1024 source, with the same `time` bracket; the row is now
regenerable. Its baselines were re-measured too — C++ 33,439 and Rust 33,574 confirm the old
figures, NumPy came in at 90.5/44.3 against the recorded 84.6/44.1.

"What is next" item 5 struck through: it still described the pool race as open.

## 2. The editor tooling — three breakages, all rename leftovers

The repo had been renamed; the **built artifacts and the user-side config** had not.

| symptom | cause | fix |
| --- | --- | --- |
| VS Code / Cursor: no highlighting | `package.json` declared `mapal-lang` with id `mapal`, but the only built artifact was `flow-lang-0.1.0.vsix`, and that is what was installed in both editors | rebuilt `mapal-lang-0.1.0.vsix`; uninstalled `flow-lang` and installed it in both. `code` is not on PATH — used the app-bundled binary |
| nvim: no icon | `~/.config/nvim/lua/plugins/flow.lua` did `require("flow.icon")`; the module moved to `lua/mapal/icon.lua`, so the require failed and the icon never registered | replaced with `mapal.lua` (`require("mapal.icon")`, `name = "mapal.nvim"`); deleted `flow.lua` and its `.bak-flow` |
| both: blank box instead of the mark | only `FlowIcons.ttf` was ever installed | built `MapalIcons.ttf`, installed to `~/Library/Fonts`, removed the old one, updated `symbol_map U+F8F0` in `kitty.conf` |

The rebuilt font reports family **`MapalIcons`** internally — which closes the ADR-0037 open item
asking the font to report Mapal. It needed a rebuild, not a fix.

Also dropped the stale `flow-lang-0.1.0.vsix` from the tree: one artifact per extension.
`sh editors/test.sh` green — 61 scope assertions across both grammars.

**One thing to watch.** A format-on-save fired while the extension was reinstalling and added blank
lines to `benches/shapes/conv2d_1024.mapal` and `examples/matmul128.mapal`, which were swept into
the chore commit. Reverted in `8cbbc74`. Benchmark and example sources must not drift as a side
effect of tooling work — `git status` before `git add -A`, every time.

## 3. Why we don't fuse by default — and the premise was half wrong

**Checked rather than assumed, and it corrected the README again.** The page claimed the C++ *and
Rust* baselines get FMA from `-ffp-contract=fast`:

| baseline | fuses? | evidence |
| --- | --- | --- |
| C++ | **yes** | 2 FMA per matmul object; C/C++ contract by default, and we also pass the flag |
| Rust | **no** | **0** FMA; `rustc -O -C target-cpu=native` never contracts without an explicit `mul_add` |
| NumPy | yes | hand-written BLAS kernels, not a compiler default |

So Mapal's `exact` default puts it exactly where Rust is. C and C++ are the permissive ones.

**Can it be deduced in mapal-ir? No, and that is the point.** Fusing changes the value; nothing in
the dataflow graph says whether the program wants one rounding or two. It is a **permission**, on
the same ADR-0032 D1 lattice as reassociation (`exact | contract | tf32-class`), and unrepresentable
today for the same reason: `Ty` has no contract dimension. **One type change answers both
questions** — this one and the fold's.

**Why the default cannot simply be flipped.** Contraction is a *flag*: `EmitOpts::contract` attaches
a `contract` fast-math flag and **LLVM** decides which pairs fuse. `graph.rs` has no `Fma`;
`mapal-interp` has no `mul_add`. The oracle therefore cannot predict the fusion set, so the
differential suite's byte-for-byte `assert_eq!` — 1,280 compile-and-runs per push — would have to
weaken to a tolerance. That trades the project's strongest correctness property for 1.62×.

**The fix is the move this codebase already makes elsewhere: put the decision in the graph.**
`Operation::Fma` in Core, emitted by `lower` when the type's contract permits; interp evaluates it
with `f32::mul_add`, whose IEEE single rounding is bit-identical to the instruction, so byte-equality
with the oracle *survives* a fused build. Then the default can be `contract` with `exact` reachable
per type. Written up in `components/ir/plans/plan-s37-scan-recurrence.md` §8 and as ir suggestion #3.

## 4. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| README shows current numbers only | **kept** (Sapir's rule) | The front page is what the compiler does today; how it got there is what session logs are for |
| Keep both build faces in every README table | **kept** | Not history — a current product fact. The default emits zero FMA and one baseline family fuses while another does not; one column would mislead in one direction or the other |
| Re-measure matmul 4096 rather than flag it | **kept** | A number that cannot be regenerated is not a published result. Writing the missing source was cheaper than the caveat |
| Flip `--contract` to default now | **rejected** | It would weaken the differential oracle from byte-equality to a tolerance. Revisit after `Fma` is an op |
| `Fma` as a Core op | **proposed** | plan-s37 §8 + ir suggestion #3. The same `Ty` contract dimension unlocks fold reassociation |
| Delete the stale `flow-lang` vsix | **kept** | Two artifacts for one extension is how the wrong one gets installed |

## 5. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ pushed | clean | `git status --short` |
| gate | full workspace | green at close | `cargo test --workspace --release --no-fail-fast` |
| CI | latest push | running at close | `gh run list --limit 3` |
| gh auth | `LessComplexity` active | can push | `gh auth status` |
| editors | VS Code + Cursor: `mapal-lang.mapal-lang-0.1.0` | installed, **needs an editor restart** | `ls ~/.vscode/extensions ~/.cursor/extensions` |
| fonts | `~/Library/Fonts/MapalIcons.ttf` | installed, `FlowIcons.ttf` removed; **kitty needs a restart** | `ls ~/Library/Fonts \| grep Icons` |
| nvim | `~/.config/nvim/lua/plugins/mapal.lua` | replaces `flow.lua` | `ls ~/.config/nvim/lua/plugins` |
| perf box | `<perf-box>` i9-14900F | idle; governor `powersave`, no passwordless sudo | `ssh … uptime` |
| box dirs | `~/s36bench`, `~/s36bench_fma`, `~/s36bench_pre`, `~/mapalbench` | left in place | `du -sh ~/s36bench*` |
| worktree | `…/scratchpad/pre` @ `1daddaa` | stale since S33; `prune` will not clear it (live directory) | `git worktree remove --force <path>` |

## 6. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | The `%Frame` alias barrier | S36c §3(a) | Own plan: a disjointness fact in the plan set → `!alias.scope`/`!noalias` on frame-field accesses | saxpy 1t ≤ 0.25 ms, gate green, output byte-identical |
| **P0** | `iota` as an index law | S36c §3(b) | Own plan: `trunc i64 %iv to i32` at the use site when the source is a provable iota | saxpy 1t ≤ 0.10 ms; the four ladder loops vectorise |
| P1 | `Operation::Fma`, then flip the default | plan-s37 §8, ir suggestion #3 | Core op + lower + interp `mul_add`; `Ty` gains the ADR-0032 D1 contract | a fused build is byte-equal to the oracle |
| P1 | ADR-0028 step 1 — integer tree reduce | plan-s37 §6 | Recognise `combine`/`unit` over the exact-op set | an i32 reduce splits, canonical under `MAPAL_PAR` |
| P1 | Re-confirm S32's scheduling verdict | plan-s33b §7 check 6 | Rebuild the pre-S32 leg; the statistic is sound now | verdict restated |
| P1 | Ladder rows 5–9; DRAM-sized variants | `shape-ladder-v2.md` | scan, histogram, mandelbrot, binary search, bitonic sort | measured and published |
| P2 | Harness must state which face it emits | S36c §1 | `shapes_ab.sh` / `ladder2_ab.sh` / `compare_languages.sh` | every table says conf or FMA |
| P2 | i9 cells under 5 ms in cycles | S36c §2 | add `perf stat` to the box driver | driver reports cycles beside ms |
| P2 | `reduce_1048576.mapal` cannot fire the path it claims | S36c §5 | add an i32 twin and an f32 leg with real dynamic range | both twins exist |
| P2 | `ladder2_ab.sh` has no correctness check | S36b | add the cross-leg comparison `shapes_ab.sh` has | fails loudly on a wrong answer |
| P2 | ADR-0038 empty-param calls; halve the differential cross product | S35 §8, S34 §3 | Sapir's call on the second | — |
| P3 | One stale worktree; local dir still named `Flow` | §5 | `git worktree remove --force` | one worktree listed |

## 7. Method notes earned

1. **`git status` before `git add -A`.** An editor formatter touched two benchmark sources while an
   extension was reinstalling, and they rode into an unrelated commit.
2. **Check the premise of a question before answering it.** "Compilers do FMA by default" is true
   for C/C++ and false for Rust — and our own Rust baselines were the evidence, sitting unexamined
   in the same directory as the claim they contradicted.
3. **A published number that cannot be regenerated is not a result.** The matmul 4096 row had no
   source behind it; writing the source was cheaper than writing the caveat.
4. **Rebuild the artifact before debugging the tool.** All three editor breakages were stale build
   outputs, and the font's "wrong internal name" open item was satisfied by simply rebuilding.
5. **A permission expressed as a backend flag is untestable; expressed as an op it is not.** That is
   the whole reason `--contract` cannot be the default today, and it generalises to reassociation.

## 8. Files changed

`README.md` (current-only numbers, both faces, per-language FMA accuracy),
`benches/matmul/matmul4096_cap_f32.mapal` (new — the missing timed source),
`editors/vscode/mapal-lang-0.1.0.vsix` (new build), `assets/font/MapalIcons.ttf` (rebuilt),
`editors/vscode/flow-lang-0.1.0.vsix` (deleted),
`docs/components/ir/plans/plan-s37-scan-recurrence.md` §8 (FMA as an op),
`docs/components/ir/suggestions.md` (#3), this log, `docs/STATUS.md`, `docs/next-session.md`.
User-side, outside the repo: `~/.config/nvim/lua/plugins/mapal.lua`, `~/.config/kitty/kitty.conf`,
`~/Library/Fonts/MapalIcons.ttf`, and the extension in both editors.
