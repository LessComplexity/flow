# 2026-07-23 — S25b: close addendum — report format directive + the tile-ladder direction

Orchestrator: Claude Fable. Immutable log (ADR-0017). Post-close continuation of S25
(main log: `2026-07-23-s25-tile-emission.md`), driven live by Sapir's review.

## 0. Continuation brief

Current state: **three commits after the S25 close** — `f546f29` (ratio tables
rewritten to matchup/conditions/verdict rows), `a1393ff` (`docs/notes/
tile-ladder-direction.md` + next-session pointer), plus this log. No code changes;
docs + memory only. Workspace state unchanged from the S25 close (904 green).
Next step: S26 per `docs/next-session.md` (rung 2: TI register blocking + the
fixed-width split), under the standing direction note.
Resume command/check: `docs/notes/tile-ladder-direction.md`; `git log --oneline -4`.

## 1. Work completed

- **Perf-report format directive (Sapir, standing):** the `0.34× — 3.0× faster`
  ratio-then-restatement convention is dead — tables are now
  `| Matchup | Conditions | flow | them | Verdict |`, one plain-direction verdict
  per row, N/width/threads/what-is-timed spelled out per row. Both s25.md ratio
  tables rewritten (`f546f29`); rule added to the perf-report-format memory so
  future reports are born in this shape.
- **The tile-ladder direction recorded** (`a1393ff`, from the close Q&A):
  cuBLAS/cuDNN-class out of the box per backend as the standing target; the
  general detector story (seq/par free by node meaning; the only analysis is
  affine address regularity + lane-stride classification); reuse-is-fanout →
  register blocking reads off already-recorded zero coefficients; per-backend
  tile widths (16 is a v1 constant, not doctrine); cuda mma path with the
  fmad-class parity decision flagged for Sapir; conv2d derived-var extension +
  conv→matmul rewrite; FPGA/ASIC framing (a recognized site is a systolic-array
  spec — P7 inherits the record).
- **PREVIEW files deleted** (Sapir decision — the S25 log's stray-file flag resolved).
- Closing assessment delivered (Sapir's question): edge judged real and structural
  (three cashed bets: rewrites, threads, SIMD — same naive source), correctness
  contract as the second moat, language-surface gaps and shape-family boundary
  named honestly, beachhead recommendation (kernel/heterogeneous-target niche first).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Ratio-table format | matchup/conditions/verdict, one number, plain direction — standing | Sapir: ranges/reciprocals unreadable ("wtf is 0.34x - 3.0x"); memory rule 6 |
| Strategic target | out-of-the-box BLAS/DNN-class per backend, incl. FPGA/ASIC detection | Sapir directive at close; recorded in the direction note, S26+ inherits |
| PREVIEW-matmul512.{cu,ll}, PREVIEW.md | deleted | Sapir decision; stale S23 demo copies |

## 3. Tests, checks, benchmarks

None run — docs/memory-only continuation; the S25 close state (904 green) stands.

## 4. Live handoff state

| Type | Handle / location | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` | committed through this log | `git status` |
| vast.ai | **Unknown instance cycling: `45610428` (at S25 close) is gone; `45622441` running now** — neither created by any Flow session; consistent with Sapir's own parallel activity (`ae0ef76` fib commit + live `examples/fib.flow` edit); hands-off | `vastai show instances` |
| uncommitted | `examples/fib.flow` modified — Sapir's own edit, deliberately left untouched | Sapir's | `git diff examples/fib.flow` |

## 5. Open items

Unchanged from the S25 log except: PREVIEW strays CLOSED (deleted); the vast.ai
unknown-instance flag updated to `45622441`. The S26 agenda + standing direction
live in `docs/next-session.md`.

## 6. Architecture / model changes

None. The direction note is vision/methodology documentation (graph-advantage.md
class), not model change.

## 7. Docs reconciled

`docs/performance/matmul/s25.md` (tables) · `docs/notes/tile-ladder-direction.md`
(new) · `docs/next-session.md` (standing-direction section + post-close commit
pointers) · memory `perf-report-format` (rule 6) · this log.

## 8. Files changed

`docs/performance/matmul/s25.md` · `docs/notes/tile-ladder-direction.md` ·
`docs/next-session.md` · deleted `PREVIEW-matmul512.cu`/`PREVIEW-matmul512.ll`/
`PREVIEW.md` · this log.
