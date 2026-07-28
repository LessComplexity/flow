# plan-s39: an arm that isn't taken shouldn't run

Status: **SHIPPED WITH ONE KNOWN GAP** (2026-07-28) — gating is not stable across `LiftLoops`; see §11 "Still owed" and the session log §4a. Gate 992/2. · planned 2026-07-27, reframed and rewritten 2026-07-28 against measured evidence
Component: `ir` (the condition) · `interp` · `backend-llvm` · `backend-cuda`
Follows: `plan-s38-trap-order-is-source-order.md`
Answers: **ADR-0026 Q8** for guards in value position.

Everything below was **run**, not read. Commands are in §8.

---

## 1. The bug

```
(1 > 0) -> { -true-> 42; -false-> 7 / 0; } -> println
```

Traps. Both arms run, so the divide by zero happens even though the condition picked `42`.

`examples/calc.mapal:12-16` documents this as intended. `docs/spec/mapal-as-implemented.md:77` states
it as the language's semantics. Sapir's direction:

> *"this is a flow with a condition, the condition should execute and thus determine if the path is
> to be taken or not."*
> *"not all branches should compute if a branch is not gonna be taken. they should be
> rendered/compiled yes, but not computed at the same time if not taken."*
> *"it's not about `select` — it is about **dataflow, if the flow condition allows it**."*

---

## 2. It's worse than one trap — measured

Put a whole `map` in the arm that isn't taken, and make the map body divide by zero:

```
4 -> iota -> src;
(1 > 0) -> {
    -true->  99;
    -false-> { src -> map { x -> x / 0 } -> bad; bad[0] }
} -> println;
```

Interpreter: **traps**. So the entire map ran.

Now emit LLVM with the rewriter on. The rewriter folds `1 > 0`, deletes the `select`, and reduces the
answer to a constant:

```llvm
store i32 99, ptr %o6
```

And in the same function, it still emits this:

```llvm
call @mapal_par_task(h, 0, ..., @task0, ...)
call @mapal_par_task(h, 1, ..., @task1, ...)   ; the map that divides by zero
call @mapal_par_task(h, 2, ..., @task2, ...)
call @mapal_par_dep(h, 0, 1)
call @mapal_par_dep(h, 1, 2)
```

**The compiler proves the answer is 99, then dispatches a parallel job whose only effect is to trap.**

Why it survives: DCE pins anything that can trap, even when dead (`graph_rewrites.rs:59-65`, R4).
That rule is correct *given today's semantics* — under today's rules the trap is supposed to fire.
Change the semantics and the same rule stops pinning it.

So the trap is not the whole problem. Work in an untaken arm runs, whether or not it traps, and even
after the compiler has proved the arm unreachable.

---

## 3. The rule

> **An arm's work runs only if the condition picks that arm.**

That's the whole change. Not "branch instead of select" — that's a codegen detail. The condition is
part of the flow: data reaches the arm's work only when the condition allows it, and work with no
data doesn't run.

**Compiled ≠ run.** Both arms' code stays in the binary. Only running is conditional.

### Where the current rule came from

`docs/spec/category-ir.md` §4.4 justifies always computing both arms because *"both datapaths exist"*
in hardware and it *"matches the branchless-by-default bias for GPU codegen"*. Both true — and both
are statements about **how to emit** a guard on a particular machine. They got written down as what
the guard **means**. That's the whole mistake: one target's best codegen became the language rule.

### Mapal already does this, one place

`ir/DESIGN.md` §380 — a loop's `LoopBack` and `LoopExit` hang off one shared `Bool` and *"fire
mutually exclusively"*. `validate()` checks it from the graph. That is a condition deciding which
flow carries, already in the IR, already enforced.

| Guard writes | Arms route to | Work skipped when not taken? |
| --- | --- | --- |
| `-true-> { … -> loop; }` (`fib`, `fir`, `sum_to_n`, `matmul4_loop`) | `LoopBack` / `LoopExit` | **yes** |
| `-true-> 42; -false-> 7/0` (`calc`, `abs`, `sepia`) | `Phi` | **no** |

Same syntax, same `Bool`, two different behaviours depending on where the arm points. This plan
deletes that difference. It isn't a new feature — it's removing an exception.

---

## 4. Where the condition goes — measured, two shapes

I emitted both shapes and looked at which task holds what.

**Arm holds bulk work (a `map`)** — it's already its own task:

```
task1 = the map          task2 = the select        dep(1, 2)
```

`mapal-rt` already runs a task only when its deps are met (`mapal_par_dep`, `mapal_par_launch`).
`Task` (`mapal-ir/src/algo.rs:83`) already carries `deps`, `rank`, `trap_min`, `pinned`.

→ **Add one field: which `Bool` enables this task, and at which polarity.** A disabled task never
becomes ready and never runs. No new runtime concept.

**Arm holds scalar work** (`7 / 0`) — one task, `sdiv` and `select` both inside it:

```llvm
%t20 = sdiv i32 %t13, %t15
%t30 = select i1 %t29, i32 %t25, i32 %t27
```

→ No task boundary to hang a condition on, so the backend skips those instructions instead. On CPU
that's a branch. On a SIMD lane it's a mask. On a GPU it's divergence. All three are the same rule at
different widths, which is why this is not a `select`-vs-branch question.

**Both cases need the same fact from the IR:** which work belongs only to which arm.

---

## 5. What "belongs only to this arm" means, and what it costs per example

Take the arm's value. Remove its edge into the guard. Whatever DCE would now delete is that arm's
work. Nothing new to write — `analyze_dce` (`graph_rewrites.rs:33`) already does this walk.

Two facts fall out, both free:

- **The two arms never share work.** Anything both arms use survives removing either edge, so it
  belongs to neither and stays unconditional.
- **The condition's own work is never inside an arm**, same argument. So the condition is always
  computable first, which is what the interpreter needs.

Checked against the real examples, by emitting them:

| Example | Arms | Arm-only work | Changes? |
| --- | --- | --- | --- |
| `sepia.mapal` | `hi` · `lo` · `v` — all parameters | **none** | **no** — nothing to skip. Emits 6 `fcmp`+`select` inside `fn1`, marked `readonly nounwind willreturn`, and stays that way |
| `abs.mapal` | `x` · `x * -1` | one `Mul`, can't trap | **no** — one safe op; skipping and computing are the same work |
| `calc.mapal` | `a+b` `a-b` `a*b` **`a/b`** **`a%b`** | two divides that **can trap** | **yes** |

That's structural, not a guess: sepia's arms are function parameters, so there is literally no work to
skip. `calc` is the only example in the tree that changes.

---

## 6. Build order

1. **`ir`** — record, per guard: the condition object, and each arm's own work + whether it can trap
   (reuse `path_local_trap_capable`, `algo.rs:2897`). Add the enabling condition to `Task`.
2. **`interp`** — evaluate the condition, run only the picked arm's work, then select. **First**, because
   it's the oracle everything else is compared against.
3. **`backend-llvm`** — bulk arm: don't dispatch the task. Scalar arm: skip the instructions. Arm with
   no work, or one safe op: unchanged `select`.
4. **`backend-cuda`** — same, host and kernel forms. Delete the deliberate strictness in
   `func.rs:810` / `kernel.rs:2275` (both comment that they suppressed the correct C++ ternary
   *because* it would skip the untaken arm's trap).
5. **`calc.mapal`** — rewrite the header comment. It currently teaches the bug.
6. **`docs/spec/category-ir.md` §4.4** and **`mapal-as-implemented.md:77`** — fix the rule.

The rewriter needs no new pass, but `eval ∘ rewrite = eval` has to be re-checked per pass: `Inline`
can move work into an arm, `CSE` can move it out. Targeted tests, not an assertion.

---

## 7. Tests

| # | Test | Passes when |
| --- | --- | --- |
| T1 | `(1>0) -> { -true-> 42; -false-> 7/0 }` | prints `42`, exit 0 — interp, llvm `-O0`, llvm `-O2`, cuda |
| T2 | same with `map { x -> x/0 }` in the untaken arm | prints `99`; and `task1` is **not dispatched** (grep the emitted IR) |
| T3 | negative control | T1 and T2 both fail at `main@8b40442`, both pass after. Recorded PRE and POST |
| T4 | `calc(0, 20, 0)` | prints `20 + 0 = 20`, exit 0 |
| T5 | `abs` and `sepia` emitted IR | **byte-identical** to today. §5 says they must be |
| T6 | goldens | only `example_calc` moves, both backends. Any other movement gets explained before the snapshot is accepted |
| T7 | `cargo test --workspace --release` | 981+ pass, 0 fail; LLVM differential runs, not skipped |
| T8 | `testgen` | generates guards with trapping arms. **Zero coverage today** — same hole shape S38 found on stdout |
| T9 | rewriter | R1 battery green, plus one case per pass that moves work across an arm boundary |
| T10 | perf | 7-shape ladder, i9, interleaved, both faces, ≥50 runs, medians. Expect flat — no ladder shape has a trapping arm. Non-flat is a finding |

---

## 8. Evidence — rerun any of this

```sh
S=<scratch>
printf 'fn main() {\n  (1 > 0) -> { -true-> 42; -false-> 7 / 0; } -> println;\n}\n' > $S/g1.mapal
cargo run -q -p mapal-interp --example run -- $S/g1.mapal          # traps
cargo run -q -p mapal-backend-llvm --example emit -- $S/g1.mapal - # 1 select, sdiv above it, 1 task

# map in the untaken arm, body divides by zero
cargo run -q -p mapal-interp --example run -- $S/g3.mapal                    # traps => the map ran
cargo run -q -p mapal-backend-llvm --example emit -- $S/g3.mapal - --rewrite # store i32 99, and 3 tasks

cargo run -q -p mapal-backend-llvm --example emit -- examples/sepia.mapal - --rewrite   # 6 fcmp+select in fn1
```

Sources kept in the S39 scratch dir; §7 T1/T2 promote them into the tree as real tests.

---

## 9. Open

- **Q1.** Does the condition live as a **derived query** (like `path_plan`, `tile_plan`, `elem_plan`)
  or as **structure in the graph** (like the loop fork, which `validate()` enforces)? Structure means
  a malformed guard is rejected at the boundary; a query means a backend that ignores it is silently
  wrong. §3's argument — that this is the loop fork's own shape — points at structure. **Sapir's
  call**, and it's the only real design question left.
- **Q2.** Where's the cut for "small enough that skipping isn't worth it"? `abs` is one `Mul`.
  Measure `abs` and `sepia`, set the cut from what they need, nothing wider.
- **Q3.** Own ADR, or amend ADR-0026 Q8? Lean: own ADR — ADR-0026 stays a candidate.

## 10. Not doing

- Effects in arms. L1404/L1405/L1406/L1408 unchanged.
- `Ty::Sum`, user enums, `Option`/`Result`. That's ADR-0026, untouched. This plan makes it **smaller**
  — Q8 said the IR had no non-strict machinery; after this it does.
- Renaming the construct to `select`. Refused by Sapir.
- Changing trap order. S38's rule stands.
- Removing an arm's code from the binary. Both arms compile; only running is conditional.


---

## 11. What shipped, and the three defects the plan did not predict

`guard_plan` landed as a deduced query (Q1 answered: **query**, not graph
structure — smallest diff, and every consumer is written in this same change).

**Ownership is CONSUMER CLOSURE, not liveness.** §5 proposed "the arm's work is
what DCE would delete if the arm edge were removed." That is unsound here and
the 1,280-run differential caught it (testgen case #94, `read before write`):
nothing deletes dead code before execution — the interpreter walks every
morphism in topo order — so a *dead* consumer still reads its operand. A `Proj`
feeding both an arm and a dead `Neg` passed the liveness test, got gated, and
the dead `Neg` then read an object the unchosen arm never wrote. The rule is now:
a morphism is arm-owned iff **every** consumer of its target is arm-owned.

Two follow-on defects, both found by running rather than reading:

1. **Subtraction breaks closure.** Stripping a nested site's work from the
   enclosing arm can orphan a morphism that joined only because a sibling
   guard's edge was owned at walk time. Fixed with a re-closure pass to a
   fixpoint; anything dropped runs unconditionally, which is always safe.
2. **Flags must be TRANSITIVE, in two places.** After subtraction an enclosing
   arm's direct list no longer holds the nested trap, so `can_trap`/`heavy`
   under-reported and the outer arms of `calc`'s right-folded 5-arm match went
   ungated — `calc(0, 20, 0)` still trapped. The same omission in ConstFold's
   losing-arm drop made the *rewritten* build trap while the oracle returned 20:
   an R1 divergence, caught by running both. Both now walk nested sites.

**The cost cut (§4.2's degenerate row) is real and was measured, not guessed.**
An arm's own-list is never empty — it always ends with its boundary `Pair` edge
— so the first implementation branched even for `-true-> x`, and in `sepia` that
branch landed inside a per-element map body where it would cost the loop its
vectorization. `GuardSite::gated()` now requires an arm that **can trap**
(legality) or is **heavy** — holds a bulk op or a call (cost). Two arms of
scalar arithmetic stay ungated.

### Measured results

| Check | Result |
| --- | --- |
| LLVM emission A/B, PRE `8b40442` vs POST, every bench shape + matmul + example × {raw, `--rewrite`, `--rewrite --contract`} | **103 byte-identical, 1 changed** (`examples/calc.mapal` raw) |
| Linked binary, same input filename, `clang -O2` | **byte-identical** |
| New emit failures | **0** (all 55 skips fail identically on both sides) |
| Snapshots moved | `golden_ll__example_calc`, `golden_cu__example_calc` — the two the plan predicted |
| testgen guard census, before | 320 programs, 82 sites, **0 trapping arms** — the class had zero differential coverage, which is why the bug reached production |
| testgen guard census, after `Step::PhiTrapArm` | 139 sites, **60 trapping arms**, 62 gated |
| Force-gate all sites, outcome digest vs normal | **identical** — gating changes no value |

Runtime timings were taken (51 alternating runs, six shapes) and are **not**
evidence of anything: the two binaries are byte-identical, yet medians spread
−5.9%…+1.2%. That is this Mac's noise floor at sub-millisecond sizes — the
strongest possible form of S38's measurement rule 6.

### Still owed

- **P0 — gating is not stable across `LiftLoops`.** `guard_arm`'s v1 refusal (skip the site when arm
  work touches a loop SCC) makes the *semantics* depend on whether a pass ran: raw refuses and runs
  strict, rewritten gates, so `Trapped(IndexOob) !≈ Done`. The gated side is correct. Deleting the
  refusal is not the fix — loops break outright (`route object built before read`). Arm-owned work has
  to be fired through the loop driver. IR-only: L1406 keeps `-> loop` out of a Phi arm, so no surface
  program is affected. Three proptest seeds pinned. Full account: session log §4a.

- **Hardware verification of the CUDA change.** None exists — no local GPU; the
  emitted C++ was syntax-checked only. This is the one real CUDA gap.
- *(retracted)* An earlier version of this section said the CUDA **device**
  form was not gated. It is. Map/fold bodies are emitted as
  `__host__ __device__` fns by `func.rs`, which gates them, so
  `map { x -> (x > 100) -> { -true-> x / 0; -false-> x } }` emits a real
  `if`/`else` on the device — verified by emitting it. `kernel.rs` does keep a
  strict-select arm, but it is **measured unreachable**: probed with a panic
  across 106 emissions (every example, bench shape and matmul, raw and
  `--rewrite`) plus the 163-test CUDA suite, it fired **zero** times. Left
  strict because there is no program to verify a change against.
- **No hardware verification of the CUDA change at all** (no local GPU). The
  emitted C++ was syntax-checked only.
- `mapal-rt` gained nothing: a gated bulk op is folded into its `Phi`'s
  sequential task rather than dispatched with an enable predicate
  (`ponytail:` marker in `path_plan`). Per-task predicates are the upgrade if a
  real program ever guards a big map.
