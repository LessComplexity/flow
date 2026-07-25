# Next Session (S31)

Written: 2026-07-25 · end of Session 30 · by: Claude (orchestrator; category-architect skill)
S29 + S29b + S30 CLOSED clean: workspace green, docs reconciled, nothing half-built.

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
git log --oneline -6                      # S29 feat/bench/docs, S29b diagnosis, S30 accumulators
git status --short                        # see "Concurrent session" below before judging dirt
cargo test --workspace --release 2>&1 | grep -c "test result: ok"   # expect 72, 0 failures
cat docs/performance/matmul/s29.md        # the KC verdict, the shape tables, and the S30 addendum
```

## The S30 queue

0. **`TargetProfile` — architecture selection instead of hardcoded constants**
   (`plans/plan-s31-target-profiles.md`, written pre-build; Sapir's directive). Six machine
   facts are literals in the emitter today (`tile_j_for`, `TILE_I`, `TILE_KC`, `tile_nc_for`,
   `HEAP_MIN_BYTES`, plus `GRAIN` in flow-rt). The plan replaces them with one named profile
   table (`generic`/`apple-m`/`zen3`/`native`, selected by name — `native` probes the host only
   when asked) and derives the constants from it. The key property: `tile_kc = (l2_bytes/2) /
   (nc × sizeof(elem))` reproduces today's 128 on a 512 KB L2 and yields `kc ≥ K` on this
   machine's 16 MB — so **the KC nest disables itself by derivation** instead of by a
   default-off flag. Implements the unbuilt half of ADR-0032 D4; prerequisite for ADR-0034.
   Rule 1 is the safety property: the default profile must emit byte-identical text.
1. **The box leg — now a FAIR test of the (jc, kc, ic) order.** S30 landed the promotable
   accumulators (item 1 of the old queue — done): tile accumulators are `<TJ x elem>` SSA
   values carried by `phi`, the KC leg's stack spills went 92 → 0 and its hot loop is now
   instruction-for-instruction the baseline's. KC-on 1024 f32 59.9 → 21.7 ms, 4096 4097 →
   1564. **But the traversal still loses on M4 Pro at every size, and the deficit GROWS
   with N (+5% @1024 → +14% @4096)** — the opposite of the traffic prediction, because a
   16 MB shared L2 absorbs the A re-reads the nest exists to remove. So the question is
   now purely about a machine with a small per-core L2: run `kc on/off × {1024,2048,4096}
   × {f32,f64}` on an on-demand EPYC zen3 and settle `kc_nest`'s default with a number
   from the machine it was designed for. Protocol in the S28 log §4: on-demand (no
   `--bid_price`), incremental log pulls, destroy after (~$0.45). The emit example takes
   `--kc`; the API flag is `EmitOpts::kc_nest`.
2. **conv2d row blocking** (suggestions #11) — the mechanism is now measured, not guessed:
   conv2d's hot loop is **operand**-bound, not accumulator-bound (24 vector loads per 36
   FMAs, FMA:mem 1.29 against matmul's 8.00), because TI=1 re-loads all three image rows
   per output row and re-seeds the accumulator in-loop. TI over output rows makes six
   image rows serve four output rows. This is the measured cause of conv2d winning at 512
   and losing 3.4× at 1024.
4. ~~**Finish the FLOW_PERF retirement.**~~ **DONE (S30)** — `gen_flow_capture.py` brackets the
   kernel and the new `benches/matmul/matmul_ab.sh` runs the full CPU comparison off `iter ms=`.
   `runner.py` and `tile_ab.sh` still use FLOW_PERF and are now the legacy path; retire or
   migrate them when next touched.
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
- **Remainder/boundary tiles and both TI=1 rungs still keep the memory accumulator form.** Only
  the constant-width main tile is phi-carried. fir/conv2d emissions are byte-identical to S29 —
  they were never victims (verified, not assumed).
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
