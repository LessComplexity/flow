# 2026-07-25 — S29: the KC verdict, the `time` builtin, heap lowering

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). The session opened
with `start` over `docs/next-session.md` + `sessions/2026-07-24-s28-shapes-ladder.md`, found the
previous session had been killed mid-build with **no final handoff and a tree that did not
compile**, and was directed by Sapir to run the whole S29 queue in one go.

## 0. Continuation brief

Current state: **S29 complete and committed; workspace green (72 suites); docs reconciled.**
Three builds landed — the KC k-panel nest (finished, measured, **gated OFF: a 3× loss locally**),
the `time` builtin end to end (three defects found and fixed while building it), and heap
lowering (**matmul2048 runs locally for the first time**) — plus the first kernel-only shape
numbers, which corrected an S28 claim. **Every number is local (M4 Pro); the box leg was not
run, and it is the leg that decides whether the KC nest lives.**
Next step: the box leg (S30 item 1) — `kc on/off × {1024,2048,4096} × {f32,f64}` on EPYC zen3.
Resume command/check: `docs/next-session.md`; then `docs/performance/matmul/s29.md`.

## 1. Work completed

- **`start` protocol.** FRAMEWORK + latest session log + `docs/STATUS.md` + both in-flight plan
  docs read. Flagged that S29 had no session log (killed before `end`) and recovered from git +
  tree facts instead.
- **Located the breakpoint** with a 5-agent read-only sweep over the mid-flight tree: the
  previous session had a complete, golden-pinned KC nest, then began an experiment it labelled
  `// EXP-A` (5 edits) and was interrupted inside it — a stray double blank line at
  `func.rs:2828`, an orphaned `jb0` parameter, `TILE_KC` left at the probe value 1023.
- **Unblocked the compile** — 5 unwired match arms (`Operation::TimeMs` in llvm/cuda×2/rewrite,
  `ExprKind::Unit` in lower + the syntax test tree printer).
- **Finished EXP-A rather than reverting it** (see §2): with the (jc, kc, ic) order the
  accumulator is one j-tile wide because partial sums park in `out`; dropped the dead `jb0`
  threading, restored `TILE_KC = 128`, re-pinned the golden and the five differential asserts.
- **Measured the KC nest** and gated it OFF behind `EmitOpts::kc_nest`.
- **Built the `time` builtin's missing half** (parser `()` → `ExprKind::Unit`, the wire-less
  `time` stage in lower, reserved name, both effect walks, typing) and **found three defects**
  (§2/§6), each fixed and mutation-verified.
- **Migrated `benches/shapes/`** to in-source brackets; `--perf` retired there.
- **Heap lowering** built by a delegated agent against the plan doc, then verified independently.
- **Reconciliation**: 6 parallel agents, one per component doc set; roll-ups by hand. One agent
  found a real bug in my own work (below) — it was fixed in-session and its docs corrected.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| EXP-A: finish vs revert to the `TI×NC` accumulator | **finished** | With `kc` outside `ib`, other i-blocks run between two panels of one block, so nothing survives in scratch — partial sums park in `out` and only the live j-tile's accumulators exist. `TI×NC` would be 31/32 dead space. The golden pinned the abandoned shape; the golden moved, not the code |
| KC nest after measuring a 3× loss | **kept, default OFF** (`EmitOpts::kc_nest`) | Shipping a default-on 3× regression is wrong; deleting a bit-exact, tested implementation of the BLAS order because one machine at one size dislikes it is premature. It was designed for box traffic and is unmeasured there |
| The clock fence's key: topo position vs source position | **source position** | Topo order legally schedules pure work on either side of a clock read (Kahn FIFO put the whole program before `t0`). Source order is the only key that makes `t1 - t0` mean "the work written between these lines" — which is the entire point of the builtin |
| `time`'s surface: `() -> time` (wire-less) | kept as planned | `()` had no other meaning; making it the chain head keeps `time` a stage like every other builtin instead of inventing a call syntax |
| A new L-code for `()` misuse | **rejected** | `()` in a value position IS a chain with no wire — L1301 already says exactly that; a wired `time` is L1302. 63 L-codes, S29 added none |
| cuda `TimeMs` | **`Unsupported` cell** | No device clock seam; a silent host-clock substitute would be a lie about where the time was taken |
| Heap lowering scope | **entry function only** | A big array in a Named fn or a Map/Fold body runs an unbounded number of times; arena placement without per-call free grows without bound, and per-call free needs last-use points the emitter does not compute. Recorded as BL9 |
| Committing the concurrent session's files | **excluded** | `VISION.md`, ADR-0033…0036, the thesis-review note and `docs/suggestions.md` belong to another session running on this repo; committing them would steal in-flight work |
| Box run | **not done** | Spinning up a paid cloud instance is an outward-facing, cost-incurring action and Sapir was not present to approve it. Recorded as S30 item 1 |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` | **72 suites ok, 0 failed** (run 4×: after the compile fix, after the KC finish, after the time builtin, after the effect-detector fix) | the whole tree, repeatedly |
| KC differential (5 cases, opt-in) | ok — K%KC≠0, C%NC≠0, rows%TI≠0, f64, FLOW_PAR splits | the nest is bit-exact vs untiled + the oracle at -O0/-O2 |
| `tile_ab.sh matmul1024_cap_f32`, FLOW_PAR=1, min-of-3 | **fma 59.82 (kc on) / 19.80 (kc off)**; tile 59.80/33.75; no-pack 40.79/36.25; no-tile 890/839 | the KC nest is a **3× loss**; the OFF column reproduces S28's 18.9, so the rest of the tree is intact |
| shapes A/B, kernel-only, `FIR_N=1048576 CONV_SIDE=1024` | fir fma-par **0.402** vs cpp-mt 1.462 / rust-mt 1.621 / numpy 6.368 / cpp-1t 11.395; conv2d fma-par **0.445** vs cpp-mt **0.133** / cpp-1t 0.256 / numpy 1.635 | fir wins every column (3.6× / 15.8× / 28×); **conv2d loses 3.4×** at 1024 |
| shapes A/B, kernel-only, 65536 / 512 | fir conf **0.052** vs cpp-mt 0.246; conv2d conf **0.083** vs cpp-mt 0.112 / cpp-1t 0.064 | conv2d wins at 512 and loses at 1024 — the scaling break is the finding |
| matmul2048_cap_f32, emitted + clang -O2 + run | `-1045` / `51275`, 0.43 s wall | heap lowering works; the same program SIGSEGV'd on the 64 MB macOS stack before |
| 14 new `time` tests + 2 heap + 6 KC | all green | incl. both `path_plan` clock rules, mutation-verified (reverting either fix fails its test) |
| pre-existing llvm goldens | **untouched** by the KC, heap and `time` work | byte-identity of unrelated emission preserved |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume | Stop / cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` | S29 in 3 commits (feat / bench / docs); another session's files left uncommitted on purpose | `git log --oneline -4`; `git status --short` | — |
| other session | same repo, concurrent | `VISION.md`, ADR-0033…0036, `docs/notes/2026-07-25-thesis-review.md`, `docs/suggestions.md` — **not mine, not committed** | `git status --short` | owner's call |
| vast.ai | account | untouched this session; **0 instances** as of S28 | `vastai show instances` | — |
| artifacts | `target/tmp/{tile_ab,shapes_ab}/`, scratchpad `.ll`/binaries | disposable | — | delete anytime |
| processes | none | — | — | — |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **Box leg — the KC question** | s29.md §1/§5; suggestions #16 | on-demand EPYC zen3: `kc on/off × {1024,2048,4096} × {f32,f64}`, `emit_with_opts` (the flag is API-level) | `kc_nest` default flipped with a number, or the parking-free variant taken, or the nest deleted |
| P1 | conv2d row blocking | suggestions #11/#12/#17 | TI over output rows (img-row reuse ×3), or im2col to reach the matmul ladder | conv2d 1024 ≥ cpp-mt |
| P1 | FLOW_PERF retirement, second half | plan-time-builtin item 7 | migrate `tile_ab.sh` / `runner.py` to the printed `iter ms=` | matmul legs are cross-language-comparable |
| P2 | effect-predicate refactor | lower suggestions #3 | one `stage_is_effect` helper for all four seams | a fifth effect builtin cannot miss a site |
| P2 | heap lowering, second half | backend-llvm BL9 | `flow_rt_free(ptr)` + `LastUsePlan` free points | a big array in a Named fn heap-lowers |
| P3 | standing | next-session §queue 6 | cuda consumes `tile_plan`; P7; ADR-0032 implementation; `exp` | per item |

## 6. Architecture / model changes

- **`Dat`/`Trn` (flow-ir):** `Operation::TimeMs : IoToken → (IoToken × f64)` — the first Core
  effect that produces a *value* beside the token. No new token-invariant clause was needed
  (I4/I4b/I5 key on `ty_contains_token`, and the pair is token-bearing).
- **`PathPlan` (deduced query) gained two clock rules**, both of which are placement facts, not
  machine facts (so ADR-0032's backend-genericity contract holds):
  - **the fence** — a `TimeMs` checkpoint waits for the completion of every task written entirely
    before it *in the source*. The graph supplies no order between pure work and a clock read, so
    without a key the orchestrator legally runs the bracketed work after the closing read.
  - **the host cone** — a clock read's consumer cone stays on the host spine. A task consuming it
    reads memory the host writes after dispatch: **FRAMEWORK §4.5 Law 1, a data teleport**, whose
    symptom was a negative elapsed. This is the framework's coherence law catching a real bug in
    the exact form it predicts.
- **`TrnLoc` (backend-llvm):** `emit_tile_packed_kc` is a new parallel realisation of the packed-
  site contract, selected by `EmitOpts::kc_nest` (default off) — the strategy shape (§4.4) used
  for a *measured* choice rather than a designed one.
- **`DataLoc` (backend-llvm + flow-rt):** entry-block blocks ≥ 256 KB move from the stack frame to
  a runtime arena — one `Dat`, a different location, which is the model's "one type, several
  `DataLoc`s" made literal.
- **Known divergence:** none outstanding. Two were found during reconciliation and closed in the
  same session (lower's 2-of-4 effect detectors; the plan doc's `TI×NC` accumulator and
  "stored at the last kc" rule, both amended to what the code actually became).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/components/backend-llvm/plans/plan-s29-openblas-levers.md` | acc `TI×NC` → `TI×TJ` + the parking row, composition rule 3, apack per (jb,kc,ib), the heap-lowering item as built, new Ceilings (parking-free order, prefetch clamp) |
| `docs/components/lower/plans/plan-time-builtin.md` | composition rules 4 (fence) + 5 (host cone), the built test list, the bench-migration state |
| `docs/components/{ir,lower,syntax,backend-llvm,backend-cuda,interp,check,rewrite}/` | DESIGN/IMPLEMENTATION/STATUS reconciled per component (6 parallel agents); lower + backend-llvm + backend-cuda suggestions updated |
| `docs/IMPLEMENTATION.md` | `PathPlan` row + the two clock rules; tile row + the KC gate; flow-rt row + clock/arena externs |
| `docs/STATUS.md` | S29 header; test counts corrected (syntax 201 · ir 184 · lower 161 · check 30 · interp 63 · llvm 64); `time` row in the capability matrix (cuda ✋) |
| `docs/performance/matmul/s29.md` | new — the KC verdict, both shape tables, the fence explanation, the S28 correction |
| `docs/performance/matmul.md` | S29 index row |
| `docs/components/backend-llvm/suggestions.md` | #14 CLOSED (the gen boundary — fixed by `time`, not by fusion); #16 (KC verdict + box leg) and #17 (conv2d scaling) added |
| `docs/components/lower/suggestions.md` | #3 — the effect-predicate fork; immediate half applied, structural half open |
| `docs/next-session.md` | rewritten for S30 |
| this log | new |

## 8. Files changed

Code: `crates/flow-syntax/src/parser.rs`, `crates/flow-lower/src/{emit,lib,effects,typing}.rs`,
`crates/flow-check/src/effects.rs`, `crates/flow-ir/src/algo.rs`,
`crates/flow-rewrite/src/replay.rs`, `crates/backends/cuda/src/{func,kernel}.rs`,
`crates/backends/llvm/src/{func,lib,module}.rs`, `crates/flow-rt/src/lib.rs`.
Tests: `crates/flow-syntax/tests/{golden_trees.rs,support/mod.rs}` + 1 new snapshot,
`crates/flow-lower/tests/{rejection,structural}.rs`, `crates/flow-check/tests/effects.rs`,
`crates/flow-ir/tests/algos.rs`, `crates/flow-ir/src/builder/tests.rs`,
`crates/flow-interp/tests/acceptance.rs`,
`crates/backends/llvm/tests/{differential,golden_ll}.rs` + the KC snapshot.
Bench: `benches/shapes/{fir_65536,fir_1048576,conv2d_512,conv2d_1024}.flow`, `shapes_ab.sh`.
Docs: as §7.
