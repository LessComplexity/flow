# Next Session (S38)

Written: 2026-07-27 · end of S37 · by: Claude (orchestrator; category-architect skill)
Session log: `sessions/2026-07-27-s37-elem-plan-and-the-dead-array.md`.
Previous block: S36/S36b/S36c/S36d (`sessions/2026-07-27-s36*.md`).

## READ THIS FIRST

**The work is on a branch, `s37-elem-plan`, 7 commits, NOT pushed and NOT merged to `main`.**
Sapir's call whether to fast-forward or open a PR. `git log --oneline -8` on that branch.

**The gate is green except one pre-existing failure**, `open_inline` in `mapal-rewrite`. It fails on
`main` too — it is not from this work. Its seed is committed so it cannot pass on a lucky draw. It is
S38's first item.

## Where things stand (≤6 lines)

**`elem_plan` ships: the compiler now records what `out[i]` IS, as a deduced graph fact.** `iota`
becomes `trunc i64 %iv`, `zip` becomes two unit-stride loads plus `insertvalue`, `enumerate` falls out
as `Pair(Index, ·)` with no code of its own. The payoff was not the arithmetic — it was noticing that
once every consumer rebuilds the element, **the array is write-only**: saxpy's `zip` task wrote 8 MB
per run that nothing read, *inside* the `time` bracket. **saxpy 1t 0.4769 → 0.0981 ms (4.86×), par
0.1860 → 0.0833 ms (2.23×); matmul flat at 512/1024/2048/4096.** Separately, the per-push differential
sweep went **409 s → 15 s** with the cross product intact.

## FIRST commands

```sh
git branch --show-current                  # expect s37-elem-plan
git log --oneline -8                       # 7 commits since c5f48c9
git status --short                         # expect empty
cargo test -q -p mapal-rewrite --release --test inline   # expect open_inline FAILING (pre-existing)
cargo test --workspace --release --no-fail-fast 2>&1 | grep -E "FAILED|panicked|test result"
git worktree list                          # one stale entry from S33
```

## S38 focus

### 1. P0 — trap order is source order (approach A)

`components/ir/plans/plan-s38-trap-order-is-source-order.md`. **Already built, measured, and
deliberately reverted** — this is not a design task, it is a landing task.

`Inline` turns `Trapped(IndexOob)` into `Trapped(DivZero)`. `Index` and `Fold` are independent, the
graph orders them not at all, and `topo_order` breaks the tie on **object insertion order**, which
rewriting reshuffles. Approach A — tie-break on source position, plus testgen emitting real positions
instead of `{0,0}` at all 122 sites — makes the counterexample pass, keeps `inline` 15/15, and keeps
the **1,280-run differential 36/36 green**.

It was reverted for cost, not correctness: **19 goldens across three crates**, and it reorders
emission for programs that were never rewritten (a CUDA test on a *raw* graph had its arena offsets
move, which proves lowering does not create objects in source order).

- **Price A′ first** (plan §5.1): make lowering create objects in source order, so only *rewritten*
  programs move. Cheap to answer — compare `loc` order against insertion order per function.
- **Do not reach for approach B.** Selecting by source position in the interpreter alone is
  **unsound**: `record_trap` passes `task_site(m)`, the topo index, and the runtime CAS-mins on it,
  so the parallel backend is already record-and-select keyed on topo. Changing only the oracle makes
  the two disagree. This was proposed and withdrawn in S37; the reasoning is in the plan §3.1.
- Second obligation, not closed by A: **inlining must stamp spliced morphisms with the call-site
  position**, or a trap inside an inlined body can still move. The pinned counterexample has an empty
  helper so it does not exercise this. Needs its own counterexample.

### 2. P1 — the oracle clones captured arrays per fold step

`differential_tiled_matmul_kc_c540` takes **374 s of the LLVM differential suite's 395 s**, and it is
not compile (0.79 s for all four combos) and not execution (1.54 s). It is `eval.rs:288` —
`let mut v = caps.clone()` inside the per-step loop, deep-copying a 73,440-element captured array
293,760 times. Measured, fold steps held constant at 1,000:

| captured array | time |
| ---: | ---: |
| 1,000 | 0.034 s |
| 80,000 | 1.697 s |

80× the array, 50× the time — `O(fold_steps × captured_array_size)`. The fix is `Rc` + copy-on-write
on `RValue::Array` (46 sites, 40 in mapal-interp, 6 in mapal-rewrite). **Plan it first**, and when
you do: **the prompt must forbid agents writing into the repository** — in S37 the planning workflow
dropped five `zz_*.rs` probe files under `crates/*/tests/` (one of which did not compile, which would
have failed a gate for an unrelated reason) and then began migrating `RValue::Array` to `Rc` in the
live working tree. Killed and reverted; its ground phase had produced 85 grounded facts.

### 3. P2 — the headroom `elem_plan` left

- **Arrays with captured consumers are not elided.** The rule requires every out-edge to be a
  capture-free `Map`/`Fold`; a captured consumer reaches its array through a `Pair` product. fir and
  conv2d's `ts`/`kr` iotas are 4 MB each and still materialise.
- **Captured map bodies round-trip the argument through one reused `alloca`** (`body_call_arg`):
  gather's loop does two stores and a load into the same stack slot every element, which both
  serialises it and is why it reports `call instruction cannot be vectorized`.
- **`Apply` is legal but declined on CPU.** `ElemSrc::Apply` is recorded and gated (trap-free,
  loop-free, effect-free); `APPLY_INLINE = false` because enabling it cost 0.72× on saxpy —
  recompute loses to a load when the array is already materialised. A bandwidth-bound target should
  answer differently, which is the entire reason the decision lives in the backend.

### 4. P2 — republish the ladder with the new numbers

`docs/performance/shape-ladder-v2.md` and the README's shape rows. saxpy's cells moved 4.86× (1t) and
2.23× (par). Everything else is unchanged within noise.

## Things that are NOT open any more

- **The `%Frame` alias barrier is refuted, not deferred** (`b96a062`). S36c's 2.3× was a synthetic
  probe. Emitted code does not exhibit the problem: a struct-field control vectorises with no metadata
  at all, and across 61 tasks in 7 shapes exactly **one** reports `unsafe dependent memory operations`
  — saxpy's `Zip` task, whose output nothing reads. saxpy's timed loop already vectorises. Do not
  re-derive this from S36c; re-open only with a named shape whose *timed* loop reports the message.
- **"Halve the differential cross product" is unnecessary.** The 409 s was macOS code-signature
  validation on 1,280 freshly-linked binaries, in a system daemon outside the process tree — which is
  why fanning out made it slower. Batching 32 cases per binary got 27× with the full cross product
  and the README's "1,280 comparisons" claim intact.
- **CUDA does not mirror steps 2–3** (Sapir). Its version of this is smem staging and MMA into tensor
  cores, gated on the LLVM track landing complete. `elem_plan` is available to it and deliberately
  unconsumed; a backend that ignores the query is correct by construction.

## Measurement rules earned in S37 — read before quoting any number

1. **Interleave the two binaries in one pass, or do not report.** The same binary measured 0.5646 ms
   and 0.4731 ms twenty minutes apart. Three wrong conclusions in S37 traced to this, including a
   "1.50×" that became 1.00× and a conv2d "regression" that evaporated at 51 runs.
2. **≥50 alternating runs before claiming a sub-10% difference on a sub-millisecond cell.**
3. **Absolute ms on both sides, and name the baseline commit** (Sapir). A bare ratio is
   unfalsifiable and hides baseline drift.
4. **State which face.** S37 is all conformance — verified 0 `contract` flags, 0 `fmla`.
5. **A probe reproduces a pattern, not the compiler's output.** Two S36c probe numbers did not
   survive contact with emitted code.
6. Plus S36's ten and S35's six — a fixed threshold is a proxy, a physical bound is a test; measure
   the control you already have; check the emitted artifact, not the intent.

## Standing direction (Sapir — unchanged)

- **Compute-only legs; numpy in every verdict table; scale everything up.**
- **Parallel-first by construction.**
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a mapal-ir
  query or emitter-local cashing with zero mapal-ir change. mapal-ir never learns machine facts.
- **Three questions, three owners** (ratified S37): *is it legal* is mapal-ir's, machine-independent
  value semantics; *store or recompute* and *does it blow the cache/register budget* are the
  backend's, and they get a different answer per target. Two tables in two places, never one.
- **Query, not rewrite** (ratified S37): record that something *could* be skipped; never delete it.
  That is what leaves the decision with the backend and keeps deliberate materialisation expressible.
- **Type system = precision contracts; backend config = performance tailors.**
- **Compile time decides the SIZES, runtime decides the ASSIGNMENT.**
- **Nothing goes in the README that a default build does not deliver.**
- **Proof over suggestion** — a change arrives with the measurement of what it did, and names the
  published numbers it moves.
