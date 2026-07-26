# 2026-07-26 — S34: the rewriter's trap deletion closed, and the project becomes **Mapal**

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-26-s33-boundary-openblas-parity-open-source.md`. **10 commits, `061366e..HEAD`**, pushed
to a repository that was renamed mid-session: **`github.com/LessComplexity/mapal`**.

Driven by Sapir, in order: *"start"* → *"community files: conduct, contributing, security,
templates"* → *"ok commit / push it"* → *"badges like Hyprland"* → *"CI is very slow"* → *"I need
name suggestions"* (eight rounds) → *"Let's go with Mapal"* → *"rename the project completely"* →
*"end and reconcile"*.

## 0. Continuation brief

Current state: **the gate is green for the first time since CI was added, and the project is
named Mapal.** S33's top P0 is closed at the root. The second P0 (`mapal_par_wait` lets workers
run ahead of the clock) is untouched and is now the highest-priority item. Everything is
committed and pushed except a one-line logo geometry fix carried in this session's close commit.
Next step: **P0 — make the clock read a DAG node** (`plan-s33b-clock-read-barrier.md`; do *not*
retry the runtime-only ceiling, §4 there records both reasons).
Resume command/check: `docs/next-session.md`, then
`cargo test -q -p mapal-rewrite --release --test property` (11 green, ~0.3 s) to confirm the tree
is sound before touching anything.

## 1. The P0, closed — and it was not map fusion

S33 left the bisect pointing at `MapFusion`. That was true but misleading: the deleter is the
**`map(id) → id`** arm, not fusion proper.

Re-shrunk with `PROPTEST_MAX_SHRINK_ITERS=200000` — which took **0.43 s** and produced a
three-step program, retiring S33's "counterexample is not minimal" note:

```
map body = [ Bin{op:3 → Div, a:0, b:0},        // elem / elem — traps at elem 0
             PackProj{a:0, b:12, snd:false} ]   // proj₀(pack(elem, y)) = elem
main     = [ ConstI32(0), Iota, MapArr{arr:0, body:0} ]
```

The body is **the identity plus a dead trapping `Div`**. `ConstFold` collapses `proj₀ ∘ pack` to
the parameter, so the Return writer becomes `Output(param)`; DCE correctly keeps the dead `Div`
because impure dead cones stay live (R4); and then `is_identity_body` — which looked at *only
that writer* — declared the body the identity and aliased the entire `Map` away, trap included.
Prefixes 1–5 pass because they are the **enabler**, not the culprit.

The law is `List(id_A) = id_{List A}`, and its precondition is an equality of **morphisms**. A
body that returns its parameter *and* traps denotes `id ∘ trap : A ⇀ B`, a partial morphism, so
the law does not apply. The guard now quantifies over the body's whole morphism set through
`graph_rewrites::is_pure`, made `pub(crate)`: DCE's "may this dead cone go?" and the identity
law's "does this body denote `id`?" are the same question, and two lists would drift.

**Fusion proper was never wrong** — it inlines both bodies verbatim into the synthesized `h`, so
trapping ops in `f` survive.

Negative-controlled: with the guard reverted, the new hand-written case **and all four property
entry points** fail. That is also the proof that the four failures were one bug.

Recorded cost: `map(id) → id` now refuses a body containing `Widen`, `Iota` or `Fill`. All three
are total and legal to forward; they are simply outside `is_pure`, which is DCE's allow-list.

## 2. The rename — Flow → **Mapal**, `.flow` → `.mapal` (ADR-0037)

Forced by evidence gathered against the registries, not by taste:

| Evidence | State |
| --- | --- |
| `flowc` on crates.io | **taken** — *"A compiler for 'flow' programs"*, 92,059 downloads, `andrewdavidmackenzie/flow`: a Rust-implemented **dataflow language** |
| `flow` on crates.io | taken — a realtime log analyzer |
| flow.org / flow-lang.com | Meta's Flow, a JS type checker, owns the search |

**~150 names checked across eight families** (water, category theory, Hebrew, literary, metal,
biology, arrow/graph, LessComplexity-derived) against crates.io, npm, PyPI, GitHub and RDAP. The
durable finding: **crates.io is exhausted for English dictionary words**; availability survives
only in other languages, precise technical terms, and invented compounds.

מפל (*mapal*) = waterfall. It keeps the lineage that six of Sapir's own suggestions kept
reaching for, it **contains `map`** (the central operator — nothing else in the search had both a
lineage meaning and a core operator in its Latin letters), and it has exactly one pronunciation,
which eliminated most of the field.

**Homset was chosen and then reversed.** `Hom(A,B)` is the set of arrows `A → B`, so it names the
guarantee the differential proves — 1,280 arrows, one semantic arrow. Reversed when Sapir said
*"lineage matters more"*: it abandons the lineage and signals "category theory required". The
working tree was ~80% renamed to `homset` at that point; retargeting cost one variable.

### The extension took two attempts, and a test caught the first

`.flow → .mp` looked obvious. **`.mp` is MetaPost.** `editors/test.sh` failed 29 highlighting
assertions with METAFONT's `mfNumeric` groups leaking in, because every Vim install ships
`syntax/mp.vim` and wins the filetype race; GitHub's Linguist would have mislabelled all 49
source files identically. Nobody would have thought to test "does our chosen extension already
belong to someone" — the editor suite did it for free.

Alternatives then checked against Linguist's `languages.yml` rather than guessed:

| ext | Owner | Verdict |
| --- | --- | --- |
| `.mapal` | unclaimed in Linguist and Vim | **chosen** |
| `.mal` | unclaimed | viable shorter option; *mal* = "bad" in ES/DE/FR |
| `.mp` | MetaPost, in practice | rejected |
| `.mpl` | **JetBrains MPS** | rejected |
| `.ml` | **OCaml *and* Standard ML** | rejected — collides inside this project's own audience |
| `.map` | universally source maps / linker maps | rejected on ambiguity |

Sapir at close: *"Mapal is definitive. The extension name might change."* Recorded as ADR-0037
**D4**, with the change priced at one scripted pass (~49 files, ~300 references, ftdetect,
`package.json`, fixtures, re-run the gate).

### What was renamed, and what deliberately was not

| Surface | Count |
| --- | --- |
| crates + module references | 8 crates, 691 references |
| runtime symbol occurrences | 1,409 (`mapal_main`, `mapal_par_*`, `mapal_rt_*`, `mapal_perf_*`, `mapal_time_ms`) |
| environment variables | 202 (`MAPAL_*`) |
| source files | 49 → `.mapal` (303 reference rewrites) |
| living docs + editor files | 323 |
| TextMate scope names | 83 (`source.mapal`) |

Untouched by **ADR-0037 D3**: `docs/sessions/**`, `ADR-0001…0036`, `docs/performance/**` and
every recorded `results*.csv`. A measurement's provenance includes the name of the binary that
produced it, so old CSVs keep `flow-llvm-*` legs while the harness now writes `mapal-llvm-*`.
**Lowercase *flow* survives as the language construct** — `a -> b -> c;` is still a flow
statement, ADR-0005 holds verbatim, and `flow_soup`/`flow_piece` (proptest generators for flow
statements) kept their names.

Also renamed outward: the GitHub repository (`LessComplexity/flow` → `LessComplexity/mapal`, via
the API; GitHub redirects the old URL), the git remote, and the wordmark — whose viewBox had to
grow `213 → 242` because `mapal` is one glyph wider than `flow` and was being clipped. Verified
by rendering the SVG and looking at it.

## 3. CI — 43 → ~35 min of machine time, and one structural defect removed

Measured first: **1,879 s of the 1,934 s Ubuntu test step was the LLVM differential — 97%.**
Every other suite in the workspace is noise, so the levers are exactly three: fewer cases,
cheaper per case, fewer platforms.

**The workflow was paying for the most expensive suite twice.** `Test (release)` passed on Ubuntu
in 30m 48s, and then "Assert the differential was not skipped" re-ran the same 1,280-case suite
with `--nocapture` to grep for "skip" — heading straight for the 60-minute job timeout, which it
hit. Replaced with `FLOW_REQUIRE_CLANG` (now `MAPAL_REQUIRE_CLANG`), which turns skip-with-reason
into a hard failure inside `clang()` — the single point all nine skip sites route through — so
the *first* run is its own proof. Verified three ways: clang present + variable set compiles;
clang unreachable + variable set fails with the message; clang unreachable without the variable
still skips, so nobody without clang is blocked.

**Then `lld`.** Per-case split, min of 3 on one representative module: compile+link 0.080 s,
compile alone 0.020 s, **link alone 0.050 s — ~60% of a case**, and Linux defaults to GNU `ld`.
Installed `lld` and passed `-fuse-ld` through `MAPAL_LD` at the single point where the suite
spawns clang.

| | before | after |
| --- | --- | --- |
| LLVM differential (Ubuntu) | 1,879 s | **1,391 s (−26%)** |
| Ubuntu job | 32m 44s | **24m 34s** |
| macOS job | 10m 08s | 10m 37s (keeps `ld64`, by design) |

Prediction was ~30% from the 60% link share; measured 26%. Model sound rather than lucky.

Also learned: the Linux runner has **4 cores** (not 2), clang 18.1.3, LLD 18.1.3 — now printed by
the workflow so nobody guesses again. At 4-way fan-out, 1,280 cases × 0.08 s is ~26 s of ideal
work per core against 1,391 s actual, so **process spawn and I/O dominate, not compilation** —
which means the remaining lever with real teeth is halving the per-push cross product
(raw@`-O0`, rewritten@`-O2` instead of all four combinations, ~12 min more off Ubuntu). Not done:
it changes a published coverage claim and is Sapir's call.

## 4. Community files, and the ADR index that did not exist

Public-repo files, written in the project's own voice: `CONTRIBUTING.md` (the proof rule, the
FRAMEWORK workflow, the open-ADR pickup table, the eight measurement rules, negative control as a
requirement), `CODE_OF_CONDUCT.md` (Contributor Covenant 2.1 under a preamble: disagreement is
settled by measurement; being right is not a licence to be unpleasant), `SECURITY.md` (scope drawn
where it actually is for a compiler — an unsound `bounds_proof` is a security bug, compiling
hostile source is not), three issue forms and a PR template.

Sapir then asked where ADR-0037 was, which exposed two real gaps: **"ADR" was never expanded
anywhere in the repo** (36 files, hundreds of references), and **`docs/decisions/` had no index**
— the only listing was the *Errata/ADR ledger* buried at the bottom of `docs/STATUS.md`. Fixed:
`docs/decisions/README.md` now carries the acronym, the authority order (ADR-0022 D1), numbering
and status vocabulary, all 37 decisions one line each, the E1–E5 mapping, and how to write one.
Verified mechanically that every ADR is linked and every link resolves.

## 5. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Root cause of the trap deletion | **`map(id) → id`, not fusion** | The guard read the Return writer only; fusion inlines bodies verbatim and was never wrong |
| Where the purity predicate lives | **one `pub(crate) is_pure`, shared** | DCE and the identity law ask the same question; two lists drift, and a drift here deletes observable behaviour |
| Admit `Widen`/`Iota`/`Fill` to `is_pure` | **deferred** | Total and legal, but relaxing a predicate two passes share belongs in its own change with its own pins |
| Prove clang non-skip | **inside the suite, via env var** | Deletes a second full differential run that hit the job timeout |
| `lld` on Linux | **shipped, measured −26%** | Linking is ~60% of a case and Linux's default is the slow one |
| Halve the per-push differential cross product | **not done — Sapir's call** | ~12 min more, but it changes the published "1,280 comparisons per run" claim |
| `cancel-in-progress` concurrency | **kept, with the cost named** | Saves runner minutes on superseded pushes; it also cancelled this session's own `lld` measurement |
| Project name | **Mapal** (ADR-0037) | Lineage + contains `map` + one pronunciation; `flowc` is someone else's compiler |
| Homset | **chosen, then reversed** | Names the guarantee, but abandons the lineage and gates on category theory |
| Lessplex / Leplex / Lesplex | **rejected** | Cleanest namespace found (free `.com` *and* GitHub handle) but names the org, not the language |
| Floph / Flaph / Flaf | **rejected on phonetics** | `fl-` + short vowel + soft `f` is English's "unsteady or failed" cluster, and **FLOP** is this project's headline unit |
| BAPIR (acronym) | **rejected as a name, kept as a tagline** | Names the implementation; acronyms do not survive speech — LLVM retired its own expansion |
| Extension | **`.mapal`, provisional (D4)** | `.mp` is MetaPost; `.mpl` is MPS; `.ml` is OCaml/SML; `.map` is source maps |
| Licence badge | **static, not shields' dynamic one** | GitHub reports NOASSERTION for the LLVM exception, so the dynamic badge renders "not identifiable by github" |
| Rewriting history for the rename | **refused** | Sessions, ADRs and measurement records keep the old name; D3 says why |

## 6. Checks

| Check | Result | What it proved |
| --- | --- | --- |
| `PROPTEST_MAX_SHRINK_ITERS=200000 … open_default` | 3-step counterexample in **0.43 s** | S33's shrink limit, not an irreducible case |
| `cargo test -p mapal-rewrite --test property` | **11 green** (9 + 2 new), 0.20 s, seed retained | The P0 is fixed without un-pinning |
| Guard reverted (negative control) | **5 failures** — new test + all four entry points | The four were one bug, and the new pins can fail |
| `cargo test --workspace --release` | **green**, differential 36/36 in **465 s** | Behaviour unchanged across the rename |
| `sh editors/test.sh` | 29 failures → **all green** (61 assertions) | Caught `.mp`/MetaPost; then confirmed `.mapal` |
| `cargo fmt --all --check` | clean (after re-running `fmt`; renames change line lengths) | |
| CI run 30201636070 | **success** — first green run since CI was added | Ubuntu 32m 44s, macOS 10m 08s |
| CI run 30205798573 | in flight at close | |
| Badge endpoints against `/mapal` | `CI - passing`, `rust: 95.1%`, `issues: 0 open` | The renamed repo serves them |
| Wordmark render | full word, no clipping at viewBox 242 | The geometry fix, verified by eye |

## 7. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ close commit | pushed, 0 ahead | `git status --short` |
| remote | **`github.com/LessComplexity/mapal`** | renamed via API; old URL redirects | `gh repo view LessComplexity/mapal` |
| CI | run 30205798573 | in flight at close | `gh run list --limit 3` |
| gh auth | two accounts; **`sapiritur` is active**, repo belongs to `LessComplexity` | pushes 403 unless the right token is used | `gh auth status` |
| worktree | `…/fe60a79a-…/scratchpad/pre` @ `1daddaa` | **stale** — S33's pre-`reside` A/B tree, in a dead session's temp dir | `git worktree list` |
| arch box | `<perf-box>` | untouched this session; idle as of S33 | `ssh -o BatchMode=yes … 'cat /proc/loadavg'` |
| vast.ai | account | 0 instances (S33) | `vastai show instances` |
| proptest seed | `crates/mapal-rewrite/tests/property.proptest-regressions` | committed, **now passing** | — |
| local dir | `/Volumes/LessComplex/Personal/Flow` | **still the old name** | — |
| user nvim | `~/.config/nvim/lua/plugins/flow.lua` (+ `.bak-flow`) | stale: old path, `ft = "flow"` | — |
| user font | `~/Library/Fonts/FlowIcons.ttf` | stale name; repo ships `MapalIcons.ttf` whose *internal* family is still FlowIcons | `:MapalIcon` |
| VS Code ext | old `flow-lang.flow-lang` presumably installed | `code` CLI not on PATH here, unverified | `code --list-extensions \| grep -i mapal` |

## 8. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | `mapal_par_wait` lets workers run ahead of the clock | `plan-s33b-clock-read-barrier.md` | Make the clock read a DAG node; **do not** retry the runtime ceiling (§4) | fir 65 536 `MAPAL_PAR=14`: 0/100 under 0.01 ms, with `watermark_wait_can_finish_before_task_completion` and `wait_helps_while_the_background_worker_is_busy` passing **unmodified** |
| P1 | Re-confirm S32's scheduling verdict under a median | s33 log §5b | Re-measure 1.41–1.43× | verdict restated on a sound statistic |
| P1 | Re-measure conv2d/fir through `shapes_ab.sh` | s33.md §8 | Do not publish the hand-linked figures | README rows re-derived from a harness run |
| P1 | matmul boundary immunity at 4096 | s33.md | Expected <1%, unverified | measured or the claim dropped |
| P1 | Halve the per-push differential cross product | this log §3 | Sapir decides; update the README's "1,280 comparisons" claim with it | Ubuntu under ~13 min, coverage restated honestly |
| P2 | The 1.20× 1t kernel gap vs OpenBLAS | s33 log §3 | Raptor Lake profile; finish the packing ladder | 146 GFLOP/s moves toward 174 |
| P2 | NUMA first-touch in `reside` | `plan-s33-timed-window-boundary.md` §5 | A/B on a multi-socket host, or make `reside` lane-aware | measured on dual-socket |
| P2 | Fold tap 0 into `fmul` | `emit_conv_block_tile` | 16 of 274 instructions (~6%) | `movi` count drops |
| P2 | Admit `Widen`/`Iota`/`Fill` to `is_pure` | rewrite STATUS "S34 headroom" | Own change, own pins | `map(id)` forwards those bodies again, traps still preserved |
| P2 | `MapalIcons.ttf` internal family name | ADR-0037 acceptance | Regenerate via `assets/font/build_mapal_icons.py` | font reports Mapal, not Flow |
| P2 | GitHub licence not detected | badge decision, this log §5 | LLVM exception ⇒ NOASSERTION; cosmetic | dynamic badge usable, or accepted as-is |
| P3 | Local directory still named `Flow` | this log §7 | `mv …/Flow …/Mapal`, restart the session there | paths match the project name |
| P3 | User-side editor/font/extension names | this log §7 | Update nvim config `ft = "mapal"`, reinstall the `.vsix`, reinstall the font | `:MapalIcon` reports the Mapal glyph |
| P3 | Stale S33 worktree | this log §7 | `git worktree prune` (its temp dir is from a dead session) | `git worktree list` shows one entry |
| P3 | Purge `.ll`/`.cu` from git history | s33 log §6 | `filter-repo` + force-push; **needs Sapir** | `.git` under 20 MB |

## 9. Method notes earned the hard way

1. **A pass that iterates `git ls-files` silently skips untracked files.** This bit twice: the
   Homset ADR draft was never renamed because it was never `git add`ed, and then ADR-0037 itself
   kept saying `.mp` after the extension changed — *the document that decides the extension* was
   the one file the extension pass could not see. Stage new files before running a sweep over
   them, or iterate the filesystem.
2. **`\bflow_` misses `perf_timing_flow_main`.** An underscore is a word character, so there is
   no boundary before `flow` in a snake_case suffix. Word-boundary anchors are the wrong tool for
   identifier renames; enumerate the real symbol list and grep for what is left.
3. **`gh run watch … | tail` swallows the exit status.** `$?` is `tail`'s. Twice a run reported
   "exit 0" while the actual conclusion was `cancelled` — a green claim would have been wrong.
   Always re-read the conclusion with `gh run view --json conclusion`.
4. **My own concurrency rule cancelled my own measurement.** `cancel-in-progress: true` is right
   for superseded pushes, but it means pushing during a run destroys whatever that run was
   measuring. Do not push on top of a CI change you are A/B-ing.
5. **Choosing a file extension is a namespace decision.** Free on crates.io says nothing about
   free as an extension. Check `$VIMRUNTIME/syntax/` *and* Linguist's `languages.yml` before
   committing to one — `.mp`, `.mpl`, `.ml` and `.map` are all taken by something.
6. **A test in an unrelated area caught a naming decision.** The editor suite's 29 failures were
   the only signal that `.mp` was MetaPost. Broad, cheap assertions pay off in places nobody
   predicted.
7. **Mechanical renames produce formatting churn.** Identifier lengths change, so rustfmt wants
   to re-wrap; run `cargo fmt --all` after a rename pass or the `--check` gate fails for a reason
   unrelated to the change.
8. **Renaming text is not renaming a logo.** `flow` → `mapal` is one glyph wider; the wordmark's
   viewBox clipped it silently. Render the asset and look at it.

## 10. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/decisions/ADR-0037-project-name-mapal.md` | **new** — the name decision, ~150 rejected alternatives with evidence, D3 immutability, D4 provisional extension |
| `docs/decisions/README.md` | **new** — the ADR index that did not exist; acronym expanded, authority order, numbering, all 37 logged |
| `docs/components/rewrite/plans/plan-s34-identity-map-trap.md` | **new** — SHIPPED, with as-built acceptance and the recorded cost |
| `docs/components/rewrite/{STATUS,IMPLEMENTATION}.md` | P0 root cause, the shared `is_pure` note, 68 → 70 tests, S34 headroom |
| `docs/STATUS.md` | S34 header: P0 closed, gate green, the rename with counts and the extension detour |
| `README.md` | badges (Hyprland idiom, reference-style), "what is next" item 4 struck through with the real explanation, contributing pointer, renamed throughout |
| `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `.github/**` | **new** — community files, issue forms, PR template |
| `.github/workflows/ci.yml` | single-run clang proof, `lld`, core reporting, concurrency group, timeout 75 |
| `assets/logo-wordmark.svg` | text `flow` → `mapal`, viewBox 213 → 242 so it is not clipped |
| `docs/next-session.md` | rewritten for S35 |

## 11. Files changed

10 commits. Code: `crates/mapal-rewrite/src/{functor_laws,graph_rewrites}.rs` (the P0 guard),
`crates/backends/llvm/tests/differential.rs` (`MAPAL_REQUIRE_CLANG`, `MAPAL_LD`),
`crates/mapal-rewrite/tests/property.rs` (+2 pins). Renames: 8 crate directories, 49 source
files, 6 editor files, 2 spec files, 1 snapshot, the font and its build script. Rewritten: 323
living docs and editor files. New: 7 community/ADR/plan files.

Gate at close: **green** — `cargo test --workspace --release`, differential 36/36 in 465 s,
`cargo fmt` clean, 61 editor assertions across both editors.
