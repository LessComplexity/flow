# 2026-07-28 — S39: guards gate the flow, and five defects that only running found

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017).
Driven by Sapir. Continues `2026-07-27-s38-trap-order-is-source-order.md`.

## 0. Continuation brief

Current state: **plan-s39 SHIPPED, and the gate is RED — 992 passed, 2 failed.** Both failures are one
defect, mine, characterised in §4a below and pinned as proptest seeds. It does not affect any program
Mapal's surface syntax can express; it affects IR that `testgen` builds directly. Read §4a before
anything else.

 An arm that is not taken no longer runs — on the interpreter, the
LLVM backend, and the CUDA **host** emitter. `guard_plan` is a new deduced query in `mapal-ir`.
Emission is unchanged almost everywhere and that is proven, not estimated: **103 of 104 A/B emissions
byte-identical** against `8b40442` (= `24f52c9` + a rename-only commit; the compiler is unchanged
between them); the one change is `examples/calc.mapal`.
Next step: **fix §4a** — it is the only thing standing between this and a green gate.
Resume command/check: `bash -c 'cd /Volumes/LessComplex/Personal/Flow && cargo test --workspace --release'`

## 1. The bug, and what it really was

```
(1 > 0) -> { -true-> 42; -false-> 7 / 0; } -> println     → mapal trap: div_zero   exit=101
```

`examples/calc.mapal`'s header taught this as intended; `mapal-as-implemented.md:77` stated it as the
language's semantics. Both were wrong, and the reason is precise.

`category-ir.md` §4.4 justified computing both arms because *"both datapaths exist"* in hardware and
it *"matches the branchless-by-default bias for GPU codegen"*. Both statements are true. Both are
about **how to emit a guard on a particular machine**. They were written down as what a guard
**means** — FRAMEWORK §4.2's exact error, a `TrnLoc` promoted to a `Trn`.

Two distinctions the strict reading collapsed:

- **Pure is not total.** The arm restrictions (L1404/L1405/L1406/L1408) buy purity. `Div`, `Mod`,
  `Index` and `Update` are pure **partial** morphisms. Evaluating both arms implements the copair
  `[f, g]` only where *both* are defined — a strictly smaller morphism than the guard denotes. The
  wrong morphism, not a different schedule of the right one.
- **Compiled is not computed.** Both arms' code stays in the binary. Only running is conditional.

**Mapal already gated one placement and not the other.** I4's structural loop fork hangs `LoopBack`
and `LoopExit` off one shared `Bool` that *"fire mutually exclusively"*, checked by `validate()` from
the graph alone. So `-true-> { … -> loop; }` gated and `-true-> 42` did not: same syntax, same `Bool`,
two semantics decided by where the arm points. FRAMEWORK §3's warning sign. This change **removes an
exception**; it does not add a feature. It needs no `Ty::Sum` — ADR-0026 stays a candidate, and is
now *smaller*, because its Q8 said the IR had no non-strict machinery and now it does.

## 2. The trap was the loud instance, not the class

Put a whole `map` in the untaken arm and make its body divide by zero — it runs. Worse, with the
rewriter on:

```llvm
store i32 99, ptr %o6                              ; the compiler PROVED the answer
call @mapal_par_task(h, 1, ..., @task1, ...)       ; and still dispatched the divide-by-zero map
call @mapal_par_dep(h, 0, 1)
```

ConstFold folds `1 > 0`, deletes the `select`, reduces the program to a constant — and DCE then pins
the dead trapping cone (R4, `graph_rewrites.rs:59-65`), which is *correct under the old semantics*.
130 lines of IR before, 47 after.

## 3. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Frame the fix as "branch instead of select" | **withdrawn (Sapir)** | wrong altitude for a dataflow-first compiler. `select`/branch/lane-mask/warp-divergence all *realize* the gate; the meaning is that an arm's work runs only if the flow condition allows it |
| `guard_plan` as a deduced query vs graph structure | **query (Sapir)** | smallest diff, no `validate()` change, every consumer written in this same change. Promote later if an external consumer ever appears |
| Defer arm traps instead of gating | **rejected** | the arm still fires; leaves the whole wasted-work half untouched |
| `Ty::Sum` / honest coproducts | **out of scope** | ADR-0026's own sizing is "the biggest Core change since ADR-0013". Building it to stop an untaken `7 / 0` is backwards |
| Gate every site | **rejected on measurement** | an arm's own-list is never empty (boundary `Pair` edge), so this branched even for `-true-> x`; in `sepia` that lands inside a per-element map body and costs the loop its vectorization |
| `gated()` = can_trap ∨ heavy | **kept** | legality then cost. Can-trap MUST gate (no cost argument buys it); heavy = bulk op or call; two scalar arms left alone |
| Separate acceptance item for "total but expensive" | **deleted (Sapir)** | one rule, two sizes — a `Map` in an untaken arm doesn't fire for the same reason `7/0` doesn't |
| Verify the S39 CUDA change on a 4090 | **not doing (Sapir, at close)** | *"we are going to transition from cuda to nvptx anyways"* — spending a hardware run on an emitter being replaced is wasted. The change is correct by inspection and the CUDA goldens moved only for `calc`; it carries no hardware evidence and that is now accepted, not owed |

## 4. Five defects the plan did not predict — all found by running, none by reading

**(1) Ownership is CONSUMER CLOSURE, not liveness.** The plan proposed "the arm's work is what DCE
would delete if the arm edge were removed." The 1,280-run differential refuted it — testgen case #94,
`read before write`:

```
* Proj{2}      23v1 -> 27v1     <- marked arm-owned by the liveness rule
  Neg          27v1 -> 28v1     <- NOT owned, and 28v1 is DEAD
* Pair{slot 0} 27v1 -> 33v1
```

Nothing deletes dead code before execution — the interpreter walks every morphism in topo order — so
the dead `Neg` still ran and read an object the unchosen arm never wrote. Rule now: a morphism is
arm-owned iff **every** consumer of its target is arm-owned. Simpler, and needs no liveness at all.

**(2) Subtraction breaks closure.** Stripping a nested site's work from the enclosing arm can orphan a
morphism that joined only because a *sibling* guard's edge was owned at walk time. Re-closed to a
fixpoint; anything dropped runs unconditionally, which is always safe.

**(3) Flags must be TRANSITIVE, in two places.** After subtraction the enclosing arm's direct list no
longer holds the nested trap. `calc`'s 5-arm match right-folds to nested Phis with `a % b` innermost,
so the outer arms reported `can_trap=false`, went ungated, and `calc(0, 20, 0)` still trapped. The
same omission in ConstFold's losing-arm drop made the **rewritten** build trap while the oracle
returned 20 — an R1 divergence, visible only by running both.

**(4) ConstFold must drop the guard's TRIPLE, not just the losing arm.** Dropping the arm value while
leaving the triple alive made replay rebuild the losing boundary `Pair` edge against a dropped feeder
(`replay.rs:989`, "feeder is not mapped"). With the `Phi` aliased to the winner, nothing reads the
triple — drop it and both `Pair` edges go with it.

**(5) Guard-arm work is NOT exempt from DCE's R4 trap-pinning.** An intermediate version exempted it,
reasoning that a gated trap only fires when its arm is picked. Wrong: **the interpreter walks a dead
`Phi` too**, so it still fires the chosen arm and still traps — DCE turned `Trapped(DivZero)` into
`Done(0)` (property `open_default`). The exemption existed to handle an orphan left behind when
ConstFold aliased a `Phi` away; that is now handled where it belongs, in ConstFold's own plan (defect
4), and R4 is untouched.

## 4a. THE OPEN DEFECT — gating is not stable across `LiftLoops`

**Symptom.** `LiftLoops: Trapped(IndexOob) !≈ Done(I32(1))`. The raw graph traps; the rewritten one
does not. That breaks the rewriter's one rule, `eval ∘ rewrite = eval`.

**Cause, exactly.** `guard_plan`'s `guard_arm` **refuses** to gate a site when any candidate morphism
is incident to a loop SCC (`algo.rs`, "v1 refusal"). `LiftLoops` turns a loop into a `Map`/`Fold`, so
the SCC disappears:

| | loop present? | site gated? | trapping `Index` in the arm |
| --- | --- | --- | --- |
| raw | yes | **refused** → strict | runs → `Trapped(IndexOob)` |
| after `LiftLoops` | no | **gated** | not selected → `Done` |

So the *meaning* of the program depends on whether a pass ran. Any shape-based refusal has this
property, because rewriting changes shape — this is not a coding slip, it is the v1 refusal being the
wrong mechanism.

**Which side is right:** the gated one. Under the new semantics an untaken arm does not run, so the
RAW evaluation is the wrong one — the raw graph should gate and refuses to.

**The refusal is load-bearing and cannot simply be deleted.** Tried it: consumer closure alone does
not exclude enough, and loops break outright — `internal error: route object built before read`,
every loop property test. So the fix is not "remove the refusal".

**Fix direction.** Gating must work *through* the loop driver rather than around it: an arm-owned
morphism that the driver owns should be fired by the driver under the guard's condition, instead of
the site being refused. That is real work in `mapal-interp/src/loops.rs` plus the two backends' loop
emitters, and it is the S40 P0.

**Blast radius is IR-only.** Surface Mapal cannot build this: L1406 rejects `-> loop` inside a Phi
arm, and `lower` never places loop machinery in a guard arm. `testgen` builds IR directly and does.
Every example, bench shape and matmul is unaffected — the 1,280-run differential passes and all
goldens are green.

**Pinned seeds** (`crates/mapal-rewrite/tests/property.proptest-regressions`), all
`PhiTrapArm` + a loop step:

```
3f922cd8…  LiftMap + Index + PhiTrapArm          closed_default
64b6805c…  LiftFold + LiftMap + Index + PhiTrapArm   open_default
10b11e2a…  Loop + Loop + PhiTrapArm              (trap_free)
```

They cannot pass on a lucky draw. Two earlier seeds (`905bea30…`, `f4329a14…`) are the DCE defects
§4(5) and §4a's sibling, both fixed and still pinned.

**Honest note on how this was found.** The default 256-case property run was green three times. It
took `PROPTEST_CASES=1024` to surface it, and that run was made only because this change had already
produced five defects. Raising the case count on a suspicious change is worth the 90 seconds.

## 5. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `guard_plan` unit tests (`tests/algos.rs`) | 4/4 | trapping arm owned; shared value owns nothing but edges; nested sites partition; topo order + edge last |
| `mapal-interp/tests/guards.rs` | 8/8 | T1/T2 + PRE/POST negative controls + both polarities + nested + `calc` both ways |
| `mapal-rewrite/tests/guard_ownership.rs` | 139 sites | consumer closure, arm disjointness, condition-not-owned, topo order — over the closed testgen corpus |
| Force-gate every site, outcome digest vs normal, 320 programs | **identical** | gating changes no value |
| **A/B emission**, PRE `8b40442` vs POST, all benches + examples × {raw, `--rewrite`, `--rewrite --contract`} | **103 identical, 1 changed** | only `examples/calc.mapal` (raw) |
| Linked binary, same input filename, `clang -O2` | **byte-identical** | identical IR ⇒ identical machine code |
| New emit failures | **0** | all 55 skips fail identically on both sides |
| testgen guard census, before | 320 programs, 82 sites, **0 trapping arms** | the class had ZERO differential coverage — why the bug reached production |
| testgen census after `Step::PhiTrapArm` | 139 sites, **60 trapping**, 62 gated | hole closed |
| `calc(0, 20, 0)` | `20`, exit 0 | interp, llvm raw, llvm `--rewrite` all agree |
| `calc.mapal` output | unchanged | values identical to HEAD, oracle agrees |
| **`cargo test --workspace --release`** | **992 passed, 2 failed** | the two are §4a, seeds pinned. LLVM differential RAN (411.94 s) and passed; 1,280-run sweep ok; all goldens ok; 0 pending snapshots; `cargo fmt --check` clean |

**Perf: no change, and it is proven structurally rather than measured.** Full report:
`docs/performance/s39-guards-gate-the-flow.md`; raw series in `benches/results-s39/` (51 runs per shape
per side, plus `machine.txt`). Every bench shape and every
matmul emits the same bytes. Runtime medians were taken anyway (51 alternating runs, 6 shapes) and are
recorded as a *noise-floor control, not a result*:

| shape | PRE ms | POST ms | Δ |
| --- | --- | --- | --- |
| saxpy_1048576 | 0.1238 | 0.1165 | −5.89% |
| conv2d_512 | 0.0785 | 0.0758 | −3.34% |
| transpose_1024 | 0.3724 | 0.3622 | −2.73% |
| gather_1048576 | 0.1950 | 0.1973 | +1.18% |

**The two binaries are byte-identical.** So that entire spread, including a −5.9% that would read as a
win, is this Mac's noise floor at sub-millisecond sizes. S38's measurement rule 6 in its strongest
form: **anything under ~6% on an unpinned Mac at these sizes is nothing.**

## 6. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `8b40442` | **uncommitted** — 22 modified, 6 new (incl. `benches/results-s39/`) | `git status --short` | Sapir's call |
| worktree | `…/scratchpad/s39/pre` @ `8b40442` | **removed at close** | `git worktree list` | done |
| worktrees | three stale, @ `d3ca82c`, `6168863`, `1daddaa` | S33/S38 debt, still listed — **not mine, left alone** | `git worktree list` | `git worktree prune` |
| scratch | `…/scratchpad/s39/` | probe sources, both emit binaries, `ab/`, gate logs — **session-scoped, will vanish**. The timing series were copied into `benches/results-s39/` before close | `ls …/s39` | none needed |
| untracked | `oainotes.md` | the external review that produced the guard P0 | `head oainotes.md` | Sapir's call |

Re-run the A/B: build `--example emit -p mapal-backend-llvm` in both trees, **copy each binary out
before building the other** — `mapal-backend-cuda` ships an example with the same name and they
collide at `target/release/examples/emit`. That mistake produced a first A/B run reading "everything
changed"; the fix is disambiguated binaries.

## 7. Open items

| P | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| — | ~~CUDA device guards are strict~~ **RETRACTED, measured** | `backends/cuda/src/kernel.rs` | none — map/fold bodies are `__host__ __device__` fns emitted by `func.rs`, which gates them; a guard in a `map` emits a real `if`/`else` on the device. `kernel.rs`'s arm fired **0 times** across 106 emissions + the 163-test suite when probed with a panic | already correct |
| — | ~~Hardware-verify the CUDA change~~ **CLOSED, won't do (Sapir)** | this log §3 | none — the CUDA emitter is being replaced by NVPTX | n/a |
| **P0** | **Gating is not stable across `LiftLoops`** | this log §4a | fire arm-owned work through the loop driver instead of refusing the site | the three pinned seeds pass and `PROPTEST_CASES=1024` is green |
| P0 | **GPU leg via NVPTX** | S38 §6, ADR-0033 | **now the only GPU item.** Write the plan; decide how graph facts reach a GPU `Loc`. `guard_plan` supplies one more: a gate has four realizations and warp divergence is one | a matmul site runs on the 4090 through PTX, bit-exact |
| P1 | `guard_plan` as graph structure rather than a query | plan-s39 §9 Q1 | revisit if a consumer outside this change appears | decided |
| P1 | Per-task enable predicates in `mapal-rt` | `ponytail:` marker in `path_plan` | a gated bulk op currently folds into its Phi's **sequential** task | a guarded big map dispatches in parallel |
| P1 | ADR for "guards gate the flow" | plan-s39 §9 Q3 | write it; amend ADR-0026 Q8 to point at it | ADR merged |
| P1 | Beat OpenBLAS at ONE thread | S33 | unchanged | 1t parity on the i9 |
| P1 | Inlining must stamp spliced morphisms with the call-site position | plan-s38 §6.1 | unchanged | counterexample passes |
| P2 | Oracle clones captured arrays per fold step | S37 | `Rc` + CoW, 46 sites — **plan it first**, forbid agents writing into the repo | differential < 60 s |

## 8. Architecture / model changes

New deduced query `guard_plan : CategoryIr × FuncId → GuardSite*` (`mapal-ir/src/algo.rs`), beside
`path_plan`/`tile_plan`/`elem_plan`/`loop_plan`/`bounds_proof`/`last_use_plan`/`emission_plan`. No new
`Operation`, no new `Ty` variant, no `validate()` change.

`Task` gained no field: a gated bulk op folds into its Phi's `Seq` task instead, because a `Split`
site is dispatched at launch, before the condition's value exists.

Coherence: no law is violated. The gate is a placement fact (which `TrnLoc`s fire), and the four
realizations — unready task, branch, lane mask, warp divergence — are §4.2's fibre over one `Trn`,
used rather than collapsed.

## 9. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/spec/category-ir.md` §4.4 | rewritten — the two realization clauses that licensed the bug are named as the error |
| `docs/spec/mapal-as-implemented.md` §2.4 | "both arms always compute" → the gated rule, with pure≠total spelled out |
| `docs/components/ir/DESIGN.md` | `guard_plan` row in the deduced-query table + the §13 prose entry with all three earned rules |
| `docs/components/ir/IMPLEMENTATION.md` | `guard_plan`/`GuardSite`/`GuardArm` functor row + the test row |
| `docs/components/ir/STATUS.md` | S39 header |
| `docs/STATUS.md` | S39 roll-up; ir row; capability matrix gains the CUDA-device footnote |
| `docs/components/ir/plans/plan-s39-*.md` | PLANNED → SHIPPED, plus §11 recording the three defects and what was measured |
| `examples/calc.mapal` | header rewrote itself from teaching the bug to teaching the rule |
| this log | new |

## 10. Files changed

Code: `mapal-ir/src/algo.rs` (guard_plan, GuardSite/GuardArm, path_plan gating) · `mapal-ir/src/lib.rs`
(exports) · `mapal-interp/src/eval.rs` + `loops.rs` · `backends/llvm/src/func.rs` ·
`backends/cuda/src/func.rs` + `kernel.rs` (comment) · `mapal-rewrite/src/equations.rs` (transitive
losing-arm drop) + `graph_rewrites.rs` (R4 exemption).
Tests: `mapal-ir/tests/algos.rs` (+4) · `mapal-interp/tests/guards.rs` (new, 8) ·
`mapal-rewrite/tests/guard_ownership.rs` (new) · `mapal-rewrite/tests/testgen/mod.rs`
(`Step::PhiTrapArm`).
Snapshots: `golden_ll__example_calc`, `golden_cu__example_calc`.
Docs + example: as §9.

## 11. Method notes earned

1. **Read the emitter, don't reason about it.** An hour of writing model tables produced a plan whose
   central definition was unsound. Four commands (emit raw, emit `--rewrite`, grep the task table,
   grep the goldens) produced the finding that reframed the whole change.
2. **A/B the emitted artifact before benchmarking.** Byte-identical IR is a stronger perf statement
   than any number of runs, and it costs one script.
3. **Watch for colliding example binaries.** Two crates shipping `--example emit` overwrite each other
   in `target/release/examples/`; the first A/B run compared an LLVM emitter against a CUDA one and
   reported that everything had changed.
4. **A byte-identical binary is the only honest noise floor.** −5.9% between the same bytes.
5. **Census the corpus before trusting the corpus.** "The differential covers it" was false: 0 of 82
   guard sites had a trapping arm. Measure coverage, don't assume it.
6. **`rm -rf` on a directory you created inside a tracked tree.** `crates/*/examples/` already existed;
   removing it deleted `run.rs` and `dump_demo.rs`. `git status` caught it. Check before removing.
