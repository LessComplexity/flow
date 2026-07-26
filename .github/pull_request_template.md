<!--
Three questions decide a PR here: what the model says, what the evidence shows, and which
existing numbers move. Delete sections that genuinely do not apply — but say "n/a", do not
just remove them silently.
-->

## What this changes

<!-- One paragraph. What it does, and why it exists. -->

## The model

<!--
For anything beyond a bug fix or a typo (FRAMEWORK.md §6.1 — model before code):
- which Dat / Trn / Loc / Trm this touches,
- new or changed morphisms, with signatures (`f : A → B`) and whether they are stored or deduced,
- the plan or ADR this implements: docs/components/<c>/plans/plan-<slug>.md, or docs/decisions/ADR-NNNN.
-->

## Proof

<!--
Correctness: which suite covers it, and how you know the test can actually fail.
Performance: machine, method, both legs, the cells where it loses too.
Neutral change? Say so, with the measurement that shows it.
-->

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | |
| `cargo test --workspace --release` | |
| LLVM differential (did **not** skip) | |
| `sh editors/test.sh` (if `editors/` touched) | |

> `flow-rewrite`'s property suite replays pinned counterexamples from
> `property.proptest-regressions`. Never delete a seed to get a green run — if a pinned seed
> fails, that *is* the result.

## Numbers this affects

<!--
Which published figures does this change or invalidate — README rows, docs/performance/ tables?
Re-measure them here, or state plainly that they are now stale. "None" is a valid answer.
-->

## Checklist

- [ ] Docs reconciled **in this PR**, not later — morphism table, `IMPLEMENTATION.md` `file:symbol`
      rows, `STATUS.md` (FRAMEWORK §6.3).
- [ ] New tests were **negative-controlled**: I broke the behaviour on purpose and watched the
      test fail. (A test that passes wrongly is worse than none.)
- [ ] Ran the §4.5 coherence checklist (FRAMEWORK §8) — no data read where nothing put it, every
      cross-location dependency mediated by a transmission.
- [ ] Observable behaviour is unchanged where it should be: traps still trap, divergence still
      diverges, output byte-identical at `-O0` and `-O2`, one thread and many.
- [ ] Any documented rule this violates is either fixed or annotated with an explicit `Note:`
      exception.
- [ ] Unrelated changes (reformatting, drive-by cleanups) are not in this diff.
