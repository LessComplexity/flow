# Next Session (S30)

Written: 2026-07-25 · end of Session 29 · by: Claude (orchestrator; category-architect skill)
S29 CLOSED clean: workspace green, docs reconciled, nothing half-built.

## Where things stand (≤6 lines)

S28 is committed (`0e15bd0`/`aeed236`/`4bac6dd`). **S29 is complete in the working tree**: the
mid-flight KC nest finished + measured (**a 3× LOSS locally — shipped default-OFF behind
`EmitOpts::kc_nest`**), the `time` builtin end to end (with three defects found and fixed:
a clock read racing the tasks it brackets, a clock value consumed by a task, a loop-body read
hoisted out of the cycle), heap lowering (**matmul2048 runs locally for the first time**), and
the first honest kernel-only shape numbers (**fir wins every column; conv2d loses 3.4× at 1024**).
Everything is local-only — **the box leg was not run**.

## FIRST commands (resume checks, in order)

```sh
git log --oneline -4                      # S29 should be 3 commits: feat / bench / docs
git status --short                        # see "Concurrent session" below before judging dirt
cargo test --workspace --release 2>&1 | grep -c "test result: ok"   # expect 72, 0 failures
cat docs/performance/matmul/s29.md        # the KC verdict + the shape tables
```

## The S30 queue

1. **Promotable accumulators — this is the one that decides the KC question, and it
   gates the whole ladder.** S29 diagnosed the KC nest's 3× loss: it is NOT traffic
   (a `TILE_KC` sweep varies parking 4× and moves the clock 1.3%). `clang` register-
   promotes the `[64 x float]` accumulator alloca across the k loop in the jt-outer leg
   and fails to in the KC leg — **92 `str q…,[sp]` vs 0**, plus 8 runtime alias checks
   and a dead scalar fallback — so every FMA round-trips the stack (109 instructions per
   2-k body vs 51). Fix: emit the accumulator tile as SSA values or a fixed-width vector
   type instead of an alloca indexed by an induction variable, plus `noalias` on the
   emitted task's panel/frame pointers so LLVM stops versioning the loop. Evidence and
   the exact instruction sequences: `docs/performance/matmul/s29.md` §1, suggestions #16.
   Note the exact LLVM blocker is NOT pinned — a hand-applied `!noalias` on the panel
   pointer did not restore promotion; start by pinning it (`-Rpass-missed`, `opt` with
   AA remarks) rather than guessing.
2. **The box leg — now worth running on its merits, not as the tiebreak.** Every S29
   number is M4 Pro. Once the accumulator is promotable, re-measure the KC order on an
   on-demand EPYC zen3 (`kc on/off × {1024,2048,4096} × {f32,f64}`) — its own traversal
   costs are ~3% locally, so the box may well like it. Protocol in the S28 log §4:
   on-demand (no `--bid_price`), incremental log pulls, destroy after (~$0.45).
   **`kc_nest` is an API flag; the emit example now also takes `--kc`.**
3. **conv2d row blocking** (backend-llvm suggestions #11, now MEASURED not predicted): conv2d
   beats cpp-mt at 512 and loses 3.4× at 1024. TI over output rows (img-row reuse ×3), or #12
   (im2col) which reaches the whole matmul ladder instead of one rung.
4. **Finish the FLOW_PERF retirement.** `benches/shapes/` self-times; `benches/matmul/`
   (`tile_ab.sh`, `runner.py`) does not, so its totals still include data generation. Migrating
   it means the matmul legs become cross-language-comparable for the first time.
5. **The effect-predicate refactor** (lower suggestions #3): "is this stage an effect?" is asked
   at four independent sites. S29 taught all four about `time` after two of them silently
   hoisted a loop-body clock read; the structural fix is one `stage_is_effect` helper so a fifth
   effect builtin cannot miss a seam.
6. **Heap lowering, second half** (backend-llvm BL9): entry function only today. A big array
   local to a Named fn or a Map/Fold body still `alloca`s, so a matmul2048 written with its
   kernel in a helper fn still hits the wall. Needs `flow_rt_free(ptr)` + `LastUsePlan` free
   points.
7. **Standing:** cuda consumes `tile_plan` (incl. ksplit/window/KC in the design); P7; ADR rows;
   `exp`. ADR-0032 (precision contracts vs backend config) is accepted and unimplemented.

## Standing direction (Sapir — unchanged)

- **Compute-only legs; numpy in every verdict table; scale everything up** (fir 1M+, conv2d
  1024+, matmul 4096 minimum). State the basis once, no fairness narration.
- **Backend-genericity contract (ADR-0032):** a rung is either a generic graph fact in a flow-ir
  query or emitter-local cashing with zero flow-ir change. flow-ir never learns machine facts.
  Note S29 put two *scheduling* rules in flow-ir (the clock fence, the host cone) — those are
  graph facts (source order, placement legality), not machine facts, so the contract holds.
- **Type system = precision/format/reassociation contracts; backend config = performance
  tailors.** `EmitOpts::kc_nest` is the newest tailor and obeys the rule: bit-exact either way.

## Gotchas / warnings

- **Concurrent session.** Another session edited this repo during S29 and its work is UNCOMMITTED
  in the tree: `VISION.md`, `docs/decisions/ADR-0033…0036`, `docs/notes/2026-07-25-thesis-review.md`,
  `docs/suggestions.md`. S29's three commits deliberately exclude those paths. `git status` will
  look dirty — check whose work a file is before touching it.
- **`kc_nest` defaults OFF.** A measurement that forgets to opt in is measuring the S27b nest.
- **The KC verdict's REASON was corrected after the fact.** The first write-up blamed parking
  traffic; the control sweep refuted it. If you read an older doc claiming that, it is wrong —
  s29.md §1 carries the diagnosis.
- **`time` is source-order-sensitive by design.** `t1 - t0` measures the work *written* between
  the two reads. Moving a `() -> time` line changes what is measured — that is the semantic, not
  a bug. A clock read inside a loop body runs per iteration (pinned).
- **f64 prints via Rust `Display`**, so a small elapsed prints as a long plain decimal, never
  scientific. `shapes_ab.sh` parses `iter ms=` with `sort -g`, which handles it.
- vast.ai: read `credit` not `balance`; on-demand instances; pull logs incrementally; destroy after.
- Repo lives on `/Volumes` — after any path move, `cargo clean -p` the CARGO_MANIFEST_DIR-baking
  packages (flow-syntax, flow-check, flow-lower, flow-rewrite, flow-interp, flow-backend-cuda).
- The fma legs are numerically-equal-not-byte-equal BY DESIGN.
- GRAIN quantization at sub-ms N: FLOW_PAR > slice count loses — sweep/pin FLOW_PAR for small-N A/B.

## Live state at handoff

| Type | Handle | State |
| --- | --- | --- |
| branch | `main` | S29 committed in 3 commits (feat/bench/docs); the concurrent session's files left uncommitted |
| vast.ai | account | not touched this session; credit ~$14.5 as of S28, **0 instances** |
| artifacts | `target/tmp/` (tile_ab, shapes_ab), scratchpad `.ll`/binaries | disposable — every number is in `docs/performance/matmul/s29.md` |
| processes | none | — |
