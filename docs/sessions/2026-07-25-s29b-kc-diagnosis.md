# 2026-07-25 — S29b: the KC regression, actually diagnosed

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Addendum to
`2026-07-25-s29-kc-verdict-time-builtin.md`, opened by Sapir's question: *"Why was KC
k-panels worse? Did you disasm/check the ll file to diagnose?"*

**The honest answer to the question as asked: no, I had not.** The S29 verdict was a
correct measurement (3× slower) attached to an *assumed* mechanism (parking traffic).
This session tested the assumption and it was wrong.

## 0. Continuation brief

Current state: the KC regression is diagnosed with evidence at the machine-code level.
The number is unchanged; the reason in every doc has been corrected, and the S30 queue
is reordered — the follow-up is a codegen fix, not a box run and not a re-tune.
Next step: S30 item 1 — make the accumulator tile promotable, starting by pinning the
exact LLVM blocker (not yet known).
Resume command/check: `docs/performance/matmul/s29.md` §1.

## 1. What was done

- Added `--kc` to the llvm `emit` example (the nest is a default-off API flag; a box run
  or any A/B needs a way in from the command line).
- Froze two artifacts — `matmul1024_cap_f32`, `--rewrite --contract`, KC on and off — as
  `.ll` and as compiled binaries, and reproduced the gap on them: **56.3 ms vs 21.3 ms**.
- Ran two controlled sweeps, each varying ONE term of the hypothesis.
- Disassembled both binaries and compared `_task6_slice` instruction for instruction.
- Fanned out three independent analysis lenses (IR anatomy, arm64 disasm, adversarial
  counter-hypotheses) over the frozen artifacts, each briefed to REFUTE the standing
  hypothesis rather than confirm it. All three refuted it and converged on the same
  mechanism; every load-bearing claim was then re-verified by hand.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Accept the parking explanation | **rejected — refuted by control** | Parking traffic scales with panel count; a `TILE_KC` sweep varies it 4× and moves the clock 1.3% |
| Blame the tile constants | rejected | Every `TILE_KC` ∈ {128,256,512} and every `NC` spills identically |
| Blame register pressure | rejected | The KC leg touches FEWER distinct vector registers (25 vs 27) and leaves `v10`–`v15` idle |
| Blame vectorization loss | rejected | 16 `fmla.4s` per k-step in both legs; no narrowing anywhere |
| Where the S30 effort goes | **codegen fix, not the box** | The traversal's own costs total ~3%; the emitted accumulator form costs 2.2× |
| Correct the docs in place vs write only an addendum | **both** | ADR-0017: the session log is immutable, so the living docs (s29.md, plan, suggestions, both STATUS headers, next-session) carry the correction and this log records how it was found |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `TILE_KC` sweep (128 / 256 / 512 → 8 / 4 / 2 panels) | **56.37 / 55.40 / 55.09 ms** | Parking traffic is not the cost — 4× less of it buys 1.3% |
| `NC` sweep (256 / 512 / 1024 → 4 / 2 / 1 jb blocks) | 56.98 / 56.43 / **39.11 ms** | The jb blocking costs ~17 ms (preheaders + parks), but one block is still 1.8× off the baseline |
| `NC`=1024 + `TILE_KC`=512 (minimum blocking the nest allows) | 32.89 ms | Even the most degenerate KC configuration cannot reach 21.3 |
| `str q…,[sp]` in `_task6_slice` | **KC 92 · baseline 0** | The accumulators live in stack memory in one leg and in registers in the other |
| `ldr q…,[sp]` | KC 56 · baseline 0 | Same |
| `ccmp` (runtime alias checks) | KC 8 · baseline 0 | LLVM could not disambiguate the accumulator scratch from the packed panel and versioned the loop |
| scalar `fmadd` | KC 32 · baseline 2 | The versioned fallback is a fully scalar kernel (dead at runtime) |
| micro-kernel size per 2-k body | KC 109 · baseline 51 instructions | 2.14× the instructions for identical work — the measured ratio is 2.64× |
| `-Rpass-missed=licm` | 48 vs 32 remarks, same class | Inconclusive — the exact blocker is NOT pinned |
| hand-applied `!noalias` on the panel pointer | still 92 stores | The naive alias annotation is not the fix |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | S29b committed on top of S29's three commits | `git log --oneline -5` |
| artifacts | `scratchpad/kcdiag/{kc_on,kc_off,sweep_*,nc_*}.ll` + binaries | disposable — all numbers are in s29.md §1 | — |
| repo constants | `TILE_KC` = 128, `NC` multiplier = 32 | restored after both sweeps; `git diff` on `func.rs` clean | `grep -n "^const TILE_KC" crates/backends/llvm/src/func.rs` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | Promotable accumulators | suggestions #16; s29.md §1 | Pin the LLVM blocker first (`opt` AA remarks), then emit the acc tile as SSA/fixed-width vector + `noalias` on the task's pointers | `str q…,[sp]` = 0 in the KC leg's hot loop |
| P1 | Re-measure the KC order after the fix | next-session item 2 | local A/B, then the box | the order judged on its traversal, not its codegen |
| P2 | The same promotion check on the OTHER rungs | — | grep the window1d/conv kernels for the same load-modify-store shape | no rung spills its accumulators |

## 6. Architecture / model changes

None to the model. One methodological fact worth keeping: **a placement's cost can be
dominated by the emitted form rather than by the traversal it implements.** The nest was
evaluated as a `TrnLoc` (a different traversal over the same iteration space) and
priced with a traffic model — but what the machine paid for was how the accumulator
`DataLoc` materialised (stack vs register file), which the traversal only influenced
indirectly by changing the accumulator's live range. Suggestions #16 records this.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/performance/matmul/s29.md` | §1 rewritten: the refuting sweep table, the disasm table, the promotion diagnosis, the corrected disposition |
| `docs/components/backend-llvm/suggestions.md` | #16 rewritten — the proposal is now the promotable-accumulator fix, with the evidence and the not-yet-pinned blocker |
| `docs/components/backend-llvm/plans/plan-s29-openblas-levers.md` | "Measured outcome" note before the composition rules: the traffic model is sound and irrelevant |
| `docs/components/backend-llvm/STATUS.md`, `docs/STATUS.md` | the "priced the A re-read and not the parking" sentence replaced by the diagnosis in both headers |
| `docs/next-session.md` | S30 queue reordered — promotable accumulators is item 1, the box leg item 2 and no longer the tiebreak; gotcha added that the first written reason was wrong |
| this log | new |

## 8. Files changed

Code: `crates/backends/llvm/examples/emit.rs` (`--kc` flag). Docs: as §7.
No change to any emitted-code path — `TILE_KC` and the `NC` multiplier were restored
after each sweep and `func.rs` is byte-identical to the S29 commit.
