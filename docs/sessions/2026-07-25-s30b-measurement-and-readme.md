# 2026-07-25 — S30b: the full CPU comparison, the thread-count finding, and the README

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-25-s30-vector-accumulators.md`, which was written before this work happened.
Driven by three asks from Sapir in sequence: a full matmul comparison ("512 to 4096,
against all cpu formats like s28"), an explanation of why C++ beats us on conv2d, and an
open-source README plus a licence recommendation.

## 0. Continuation brief

Current state: **everything committed, gate green, tree clean.** The project has a README
and a LICENSE and is ready to open-source. The last measurement campaign produced a
finding that reframes the next session: **the default thread count is wrong for three of
our four benchmarks.**
Next step: S31 — deduce the thread count, and close the conv2d per-core gap. Both are
specified in `docs/next-session.md` under "S31 focus".
Resume command/check: `docs/next-session.md`; the numbers are in
`docs/performance/matmul/s29.md`.

## 1. Work completed

- **Migrated matmul off FLOW_PERF** — `gen_flow_capture.py` brackets the kernel with the
  `time` builtin and prints `iter ms=`; the eight 512–4096 × f32/f64 cap sources
  regenerated; new `benches/matmul/matmul_ab.sh` runs the whole CPU field. This completes
  plan-time-builtin item 7, which was queued.
- **Ran the full comparison** — 512→4096 × f32/f64 × {flow conf/fma × par/1t, cpp/rust
  1t/mt, numpy 1t/threaded}. Recorded in `s29.md`.
- **Answered the conv2d question with measurement, and was wrong once on the way** (§2).
- **Wrote README.md and LICENSE**, then had the README adversarially audited before
  publishing anything (§3).
- **Recorded the S31 direction** including Sapir's thread-deduction insight.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Migrate matmul to `time` before running the comparison | kept | A cross-language table on a basis where only our side includes data generation is the exact bug that broke conv2d's S28 verdict. Fixing the harness first was cheaper than explaining the caveat forever |
| First conv2d answer ("it is the thread count") | **retracted within the turn** | A C++ control at matched thread counts showed C++ ahead ~2× at *every* width. The thread count is real but second-order; the per-core kernel gap is first-order. The retraction is the record |
| Killing the 4096 f64 rust legs mid-sweep | kept, marked † | 193 s per run × min-of-3 to refine a number whose verdict ("~70× behind") does not move. The harness should scale its repeat count to cell cost — recorded, not fixed |
| Publish the README as first drafted | **rejected** | An adversarial audit found four framing problems that would not survive a hostile reader. Fixed before commit; details in §3 |
| Licence | **Apache-2.0 WITH LLVM exception** | Apache for the patent grant (a compiler project where adopters' lawyers will ask); the LLVM exception because `flow-rt` links into user binaries and without it §4 would impose attribution on *their* output. Same reasoning as LLVM and Swift |
| LICENSE text source | canonical local copy, never retyped | Taken from Homebrew's `llvm/LICENSE.TXT` with LLVM's own third-party and legacy-NCSA sections removed (verified absent). Reproducing licence text from memory is not acceptable |

## 3. The README audit — what was wrong before publishing

The first draft was arithmetically sound (every number traced to `s29.md`; all three
roofline percentages reproduced) and **badly framed**. Four findings would have been seized
on immediately:

1. **"beats C++/Rust by 10–190×"** — the baselines are deliberately naive triple loops
   striding down a column. The repo's own docs say "naive-class"; the README said
   "straightforward". Fixed: the baselines are described before the table, and the claim is
   scoped to what it demonstrates (automatic blocking works) rather than what it implies.
2. **Every benchmark number came from the `--contract` build, which is not bit-exact** —
   while bit-exactness was the headline property. Fixed: both faces named, and the default
   build's slower numbers printed (4096: 249 ms par / 2,249 ms 1t against 173 / 1,302).
3. **"checked on every commit"** — there is no CI. Fixed: says so, and dates the last CUDA
   hardware validation (July, three sessions of changes ago).
4. **The 10–190× line was contradicted by row four of its own table** (conv2d, where we
   lose 3.4×). Fixed: the loss is named in prose with its cause.

Also corrected: the NumPy column silently mixed 1-thread and threaded legs (the shape rows
were Flow on 14 cores against NumPy on one); the conv2d NumPy baseline is a Python loop
over nine array slices, not a kernel; the roofline denominator is an assumed pipe
count/clock, not a datasheet figure; `parallel fanout` was missing from the feature list of
a dataflow language; the quickstart omitted `cargo build -p flow-rt` and would fail on a
fresh clone.

## 4. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` | **72 suites, 0 failed** (after the bench-source regeneration; no code has changed since) | the tree is green as committed |
| README quickstart, run verbatim | interp → `f(10) = 25`; emit+clang+run → `5.375` | the published instructions work |
| Full matmul matrix, 512→4096 × f32/f64 | in `s29.md` | Flow beats naive C++/Rust 11×→192× f32 (margin grows with N); NumPy 4× ahead at 4096 f32 |
| Shapes vs NumPy | fir 1M 15.8× ahead, conv2d 1024 3.7× ahead | the NumPy deficit is one shape on one unit, not general |
| **Thread sweep** (conv2d/fir/matmul × 1,2,4,8,14) | conv2d best at **4** (0.218) and 2.1× worse at 14 (0.461); fir best at 8; matmul 1024 best at 8; matmul 4096 best at 14 | **the default width is wrong for three of four benchmarks** |
| C++ control at matched widths | cpp conv2d 1t 0.353 / 4t 0.125 / 8t 0.107 / 14t 0.153 vs flow 0.692 / 0.230 / 0.266 / 0.474 | C++ ~2× ahead at EVERY width — the conv2d gap is per-core, not scheduling. C++ also degrades at 14 (1.4×), so over-threading is partly the chip |
| conv2d hot loop (disasm) | 24 vector loads per 36 FMAs, FMA:mem 1.29 vs matmul 8.00 | the per-core cause: `TI=1`, every output row re-loads all three image rows |

## 5. Live handoff state

| Type | Handle / location | State | Inspect / cleanup |
| --- | --- | --- | --- |
| branch | `main` @ `4a16b51` | **clean — 0 dirty files**, 8 commits this session-chain | `git status --short` |
| worktrees | none | the S30 A/B worktree was removed at its close | `git worktree list` |
| processes | none | all sweeps finished or killed; no background jobs | `pgrep -fl matmul_ab` |
| artifacts | `target/tmp` (707 MB), session scratchpad (117 MB) | disposable — every number is in `s29.md` | `rm -rf target/tmp` |
| vast.ai | account | untouched all session; **0 instances** | `vastai show instances` |
| other session | its files are now committed (`02f3096`) at Sapir's request | — | — |

## 6. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **Deduce the thread count** | next-session "S31 focus" | width as a function of (element count, work/element, bytes/element) × (core count, P/E split); needs a work-per-element graph fact and the profile | conv2d/fir/matmul each run at their measured optimum without being told |
| P0 | **conv2d row blocking** | suggestions #11 | TI over output rows — six image rows serving four outputs instead of three serving one | conv2d 1t within ~1.2× of C++; FMA:mem well above 1.29 |
| P1 | `TargetProfile` | plan-s31-target-profiles.md | the named table; default profile must emit byte-identical text | the six literals stop existing |
| P1 | Box leg | suggestions #16 | zen3, `kc on/off × sizes × widths` | `kc_nest` default settled by the machine it was designed for |
| P2 | Uneven slicing for asymmetric cores | this log §4 | equal slices across 10 P + 4 E finish unequally; measure before designing | the 14-thread regression disappears |
| P2 | Harness repeat count | this log §2 | scale min-of-N to cell cost | no more 10-minute cells |
| P3 | Matrix units (SME2), then CUDA | next-session roadmap | after the above | — |

## 7. Architecture / model changes

None to the model. One direction ratified by Sapir and recorded: **the thread count is a
deducible quantity, not a constant.** It splits the same way everything else in this
project does — the graph supplies the geometry (how much work, how much data, what depends
on what), the machine supplies the constants (how many cores, how fast each) — so it
belongs with `TargetProfile` rather than as a runtime default. This subsumes backend-llvm
suggestion #15 (adaptive `GRAIN`), which is the same question about slice size rather than
worker count.

## 8. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/performance/matmul/s29.md` | the full 512→4096 × f32/f64 × all-CPU-legs matrix, with the reading, the NumPy caveats and the † single-run cells |
| `docs/performance/matmul.md` | S29 index row (earlier in the chain) |
| `docs/next-session.md` | S31 focus: both gaps located, the thread sweep table, the deduction design, and the roadmap to matrix units and CUDA |
| `README.md` | new — rewritten after the audit |
| `LICENSE` | new — Apache-2.0 with LLVM exception, canonical text |
| `benches/matmul/gen_flow_capture.py`, `matmul_ab.sh` | the `time` migration and the comparison harness |
| this log | new |

## 9. Files changed

Bench: `benches/matmul/gen_flow_capture.py`, the eight regenerated `matmul{512,1024,2048,4096}_cap{,_f32}.flow`,
new `benches/matmul/matmul_ab.sh`. Docs: `README.md`, `LICENSE`,
`docs/performance/matmul/s29.md`, `docs/next-session.md`, this log. No compiler code
changed in this part of the session — the last code commit is `669b907` (the vector
accumulators), and the gate has been green since.
