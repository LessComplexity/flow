# 2026-07-26 — S35: the shape ladder beyond compute, and "is there an AST?"

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-26-s34-mapal-rename-and-trap-deleting-rewrite.md`. Repository:
`github.com/LessComplexity/mapal`.

Driven by Sapir: *"the README needs to surface current results, not session conclusions"* →
*"add more different algorithm types … I wanna see generalizations (maybe not only compute?)"* →
*"someone said I DO create an AST — is there an AST?"* → *"isn't it better to skip the AST"* →
*"fix the build and close the session"*.

## 0. Continuation brief

Current state: **gate green — 971 passed, 0 failed, fmt clean, pushed at `26f0350`.** CI was red
for part of the session on a snapshot problem the rename caused; that is fixed and verified. The
shape ladder gained four non-compute classes with published numbers, and the README's false "no
AST" claim is corrected.
Next step: **P0 — the `mapal_par_wait` clock race** (unchanged from S34;
`plan-s33b-clock-read-barrier.md`, and §4 there says which approach not to retry).
Resume command/check: `docs/next-session.md`, then `cargo test --workspace --release`.

## 1. The README is now current results, not history

Sapir's rule: *"they belong to session logs/conclusions, not the readme — readme needs to surface
current results."* Removed the conv2d five-session post-mortem, the cache-blocking narrative, the
"Worth keeping on the page, because…" preamble, and the "honest limit"/"proving ground"/"not
something to brew install" framings. Numbers, caveats and links all stayed.

Also this session: American spelling across 52 living docs, and the README rewritten in the
project's own voice (mechanism first, `->` connectors, claims flat, no second person).

Two overreaches of the spelling pass, both caught and reverted: it "corrected" GitHub's own
conclusion value (`cancelled`, which GitHub spells with two l's) in a doc that tells the next
session what string to look for, and it changed "e-mail" inside the Contributor Covenant, which
`CODE_OF_CONDUCT.md` claims to reproduce unmodified. The rest of that file was diffed against the
canonical 2.1 text sentence by sentence.

## 2. Shape ladder v2 — four classes that are not compute-bound

`docs/performance/shape-ladder-v2.md`, written before any benchmark existed, with predictions
recorded so results could contradict them. Every published number came from matmul/fir/conv2d,
which are one class: dense, compute-bound, affine reads, perfectly parallel.

Added: **saxpy** (streaming/bandwidth), **reduce** (fold, no output array), **transpose** (data
movement, zero arithmetic), **gather** (data-dependent reads). Each has a Mapal shape, an
interpreter-sized oracle sibling, a C++ baseline threaded identically, and a NumPy leg. All three
legs print identical values before any timing is taken.

M4 Pro, RUNS=9, compute-only, min for 1t / median for par:

| shape | class | mapal-1t | mapal-par | cpp-1t | cpp-mt | numpy |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| saxpy 1M | streaming | 0.551 | **0.172** | 0.148 | 0.127 | 0.162 |
| reduce 1M | reduction | 0.620 | 0.584 | 0.508 | 0.892 | 0.101 |
| transpose 1024² | data movement | 1.008 | **0.229** | 0.761 | 0.251 | 0.779 |
| gather 1M | irregular reads | 0.559 | **0.140** | 0.476 | 0.154 | 2.043 |

### The finding: streaming kernels emit scalar loops

saxpy at one thread is **3.7× behind naive C++**, far outside the other rows. Two candidates,
both tested rather than argued:

1. `zip` materializing a pair array — real, but only ~17% (0.436 vs 0.522 when indexing both
   arrays from the lane id instead).
2. **The kernel is not vectorized.** The emitted saxpy task has one `fmul` and **zero q-register
   loads or stores** — a scalar loop — while the data-generation task in the same binary is
   full-width NEON.

So the vectorization matmul/fir/conv2d enjoy comes from the tile ladder recognizing those sites.
**A plain `map` is not a site, and gets scalar code.** That is a rung, not a defect, but it is the
first measured evidence that the deduction is narrower than the headline shapes suggest.

Two of four pre-registered predictions were wrong, both by underestimating the threaded path, and
the one I was most confident about (saxpy ties) was the worst miss.

## 3. "Is there an AST?" — yes, and the README was lying about it

Sapir had been told twice that source maps directly to the execution graph. A reader of the code
said otherwise. The code settles it: `crates/mapal-syntax/src/ast.rs` is a 384-line recursive tree
(`Program → Item → Block → Stmt → Chain → Stage → Expr`), `parse()` returns it, `lower()` consumes
it, and `check()` reads **both** it and the graph.

The README and the repo description both claimed the compiler translates source *"instead of a
traditional AST"* — falsifiable by opening one file, which is the worst property a claim can have
in a repo whose culture is proof over assertion. Corrected to the framing Sapir chose:

> The syntax is a serialization of the execution graph. The AST is the deserializer's scratch
> space; the graph is the program.

plus the sharper differentiator: optimization happens on the graph, **dataflow-first**, not on the
control-flow-first IRs (GIMPLE, MIR, LLVM IR) that mainstream compilers optimize on. Having an AST
is not unusual — GCC, Clang, Rust, Swift, GHC and Go all have one, and all optimize a lowered IR.

### Why not parse straight to the graph — answered with numbers

| stage | conv2d_1024, min of 20 | share of a real compile |
| --- | ---: | ---: |
| lex + parse (the tree is built here) | **4.4 µs** | 0.003% |
| lower (tree → graph) | 78.8 µs | 0.06% |
| rewrite | 291 µs | 0.21% |
| emit | 773 µs | 0.55% |
| `clang -O3` on the emitted IR | **140 ms** | 99.2% |

Fusing the front-end stages saves microseconds against a compile that is 99% LLVM, and costs three
things the graph is designed not to do: error recovery (`Item::Error`, 32 P-codes — a sealed,
validated graph cannot hold a broken program), declaration-order freedom (`lower` walks
`program.items` three times), and check T0201 (`Fanout`/`SeqBlock` scope, which ADR-0019
deliberately gives *no IR footprint*).

On isomorphism: lowering is **not injective** by design — `seq` leaves no footprint, surface forms
collapse onto one op (ADR-0031), `c[i] <- x` becomes `Update`, error nodes are rejected outright.
The graph is a **quotient** of the tree, not an isomorphic copy. That is the point, and it is why
`check` still needs the tree.

New tool: `cargo run --release -p mapal-backend-llvm --example stage_timing -- <file>`.

## 4. CI went red, and why the local gate missed it

Ten insta snapshots in `mapal-syntax` record byte offsets over their fixtures. The S34 rename
rewrote those fixtures' header comments (`.flow` → `.mapal`, `Flow-Core` → `Mapal-Core`), which
shifts every offset — and the same text pass rewrote strings *inside* the snapshots, which cannot
recompute the recorded numbers. The goldens described a file that no longer existed:

```
stored   332..333 At ‹@›   ->  fixture bytes 332..333 = b'\n'
actual   334..335 At ‹@›   ->  fixture bytes 334..335 = b'@'
```

Regenerated, then **verified rather than accepted**: for all ten, the first token's recorded range
is decoded out of the source file and compared with the literal the snapshot claims. 0 mismatches.

**How it reached main:** the local gate run was piped through `head -50`, and with ~50 result lines
the failure fell below the cut. The truncation hid exactly what the run existed to find. Also in
this session: a `cargo fmt --check` failure I read past because `&&` short-circuited while the
commit ran anyway (`00c0b3e`, fixed in `0880f06`).

## 5. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Post-mortems in the README | **removed** | README surfaces current results; history lives in session logs |
| Spelling | **American**, living docs only | Matches the project description; immutable set untouched |
| Four new shape classes | **shipped and published, losses included** | The claim is about generalization, so the shapes that do not win are the informative ones |
| Publish the scalar-kernel finding | **yes, with the disassembly** | It bounds the claim honestly and names the next rung |
| "instead of a traditional AST" | **retracted** | False. Disproved by opening one file |
| Parse straight to the graph | **rejected for now, with acceptance criteria** | 0.003% of compile time to gain; costs recovery, declaration order and T0201. Reopening needs an ADR with those three as the bar |
| Snapshot regeneration | **verified against source bytes, not accepted** | A regenerated golden that nobody checked is not a golden |

## 6. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ `26f0350` | pushed, clean | `git status --short` |
| gate | full suite | **971 passed, 0 failed**, fmt clean | `cargo test --workspace --release` |
| CI | latest run on `26f0350` | started at close, not yet observed | `gh run list --limit 3` |
| gh auth | `sapiritur` active; repo is LessComplexity's | pushes need the other token | `gh auth status` |
| new tools | `stage_timing` example, `benches/shapes/ladder2_ab.sh` | committed | `bash benches/shapes/ladder2_ab.sh` |
| local dir | `/Volumes/LessComplex/Personal/Flow` | still the old name | — |
| worktree | `…/scratchpad/pre` @ `1daddaa` | stale, still registered | `git worktree list` |

## 7. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | `mapal_par_wait` clock race | `plan-s33b-clock-read-barrier.md` | Make the clock read a DAG node; do **not** retry the runtime ceiling | fir 65 536 `MAPAL_PAR=14`: 0/100 under 0.01 ms |
| P1 | Streaming/permutation kernels emit scalar loops | `shape-ladder-v2.md` §finding | Decide whether a non-tile `map` gets a vectorization rung; plan first | saxpy 1t within ~1.2× of naive C++ |
| P1 | Ladder rows 5–9 | `shape-ladder-v2.md` | scan, histogram, mandelbrot (verify loop-in-map first), binary search, bitonic sort | measured and published, losses included |
| P1 | Ladder shapes are cache-resident | `shape-ladder-v2.md` caveats | 64 MB variants before any claim about irregular access at scale | DRAM-sized cells published |
| P1 | Re-confirm S32 scheduling under a median | S33 log | — | verdict restated on a sound statistic |
| P2 | **Empty-param calls should not need `()`** — Sapir's note | this log §8 | Becomes **ADR-0038**: is `-> time -> t;` or `time -> t;` legal, and does it collide with a bare name stage (W25) or with `Unit` as a value? | ADR written, decided |
| P2 | Halve the per-push differential cross product | S34 log §3 | Sapir's call; changes a published coverage claim | Ubuntu under ~13 min |
| P2 | Admit `Widen`/`Iota`/`Fill` to `is_pure` | rewrite STATUS | own change, own pins | `map(id)` forwards them, traps preserved |
| P2 | `MapalIcons.ttf` internal family name | ADR-0037 | regenerate via `build_mapal_icons.py` | font reports Mapal |
| P3 | Local directory still named `Flow`; stale worktree; user-side nvim/font/VS Code names | S34 log §7 | — | — |

## 8. Sapir's note, recorded verbatim for ADR-0038

> "a function with empty params should be able to be called natively without inputs. now we have
> `() -> time -> time_measure;` instead: `-> time -> time_measure;` or just `time -> time_measure;`
> — because no incoming variables either way."

Today `()` is `ExprKind::Unit`, and `ast.rs` calls it "the only legal empty-paren form; its one
sanctioned use is the `() -> time` chain head" (`lower/src/emit.rs` special-cases it). The design
questions an ADR has to answer: does a bare `time -> t;` collide with a plain name stage (W25: an
un-sigiled `-> search;` is a name stage, never a call), does a leading `-> time` chain head parse
unambiguously, and does the change generalize to every zero-parameter function or stay a builtin
affordance. Not started — a note, not a decision.

## 9. Method notes earned

1. **Never pipe a test summary through `head`.** ~50 result lines, failure below the cut, red CI.
   `grep -E "FAILED|panicked"` instead.
2. **`a && b; c` still runs `c`.** A `cargo fmt --check` failure short-circuited the echo and the
   commit ran anyway.
3. **A mechanical rename can invalidate a golden without touching its logic.** Text substitution
   inside a snapshot cannot recompute offsets recorded in it. After any rename, regenerate
   snapshots *and verify them against their sources*.
4. **Answer architecture questions from the code.** Two prior answers about the AST were wrong;
   `ast.rs` settled it in one grep.
5. **Price a refactor before arguing about it.** "Skip the AST to save compile time" died on a
   4.4 µs measurement against 140 ms of clang.
6. **Pre-register predictions.** Two of four were wrong, and knowing which is worth more than the
   numbers alone.

## 10. Docs reconciled

| Doc | Change |
| --- | --- |
| `README.md` | post-mortems removed; American spelling; project voice; AST claim corrected; ladder rows added |
| `docs/performance/shape-ladder-v2.md` | **new** — plan, predictions, results, the scalar-kernel finding, caveats |
| `docs/architecture/INDEX.md` | **new section** — the pipeline, what each stage owns, what lowering erases, measured stage costs |
| `docs/decisions/ADR-0037` | extension corrected to `.mapal`, D4 marks it provisional |
| GitHub repo description | rewritten to the serialization framing |
| `docs/next-session.md`, `docs/STATUS.md` | S35 handoff |

## 11. Files changed

New: `benches/shapes/{saxpy,reduce,transpose,gather}_*.mapal` (+ oracle siblings),
`ladder2_baseline.cpp`, `ladder2_numpy.py`, `ladder2_ab.sh`,
`crates/backends/llvm/examples/stage_timing.rs`, `docs/performance/shape-ladder-v2.md`.
Changed: `crates/mapal-rt/src/lib.rs` and `mapal-cli/src/main.rs` (user-visible `flow trap:` /
`flow:` strings the rename missed), `crates/backends/cuda/tests/differential.rs` (the stderr pin
that moved with them), 10 regenerated syntax snapshots, README, both backend DESIGN docs.

Gate at close: **971 passed, 0 failed**, `cargo fmt` clean.
