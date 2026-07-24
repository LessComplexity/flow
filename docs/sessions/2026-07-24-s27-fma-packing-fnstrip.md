# 2026-07-24 — S27: FMA + rung 3 packing + fn-strip wiring + loop→map design

Orchestrator: Claude Fable (category-architect skill). Immutable log (ADR-0017).
Continuation of `2026-07-23-s26b-par-on-par-reframe.md` per `docs/next-session.md`
(Sapir chose "all in order": gap-closers → fn-strip → loop→map design → box).
Plans of record: `components/backend-llvm/plans/plan-s27-fma-packing.md` (SHIPPED S27)
+ `components/rewrite/plans/plan-loop-to-map-fold.md` (DESIGN, awaiting ratification).
Working protocol per Sapir's in-session directive: codex CLI implements (WP1/WP2/WP2b/WP3),
orchestrator designs, reviews every diff line-by-line, fixes/directs what falls short.

## 0. Continuation brief

Current state: **S27 closed except the box leg — BLOCKED on vast.ai balance 0**
(`vastai show user`; the standing pre-box check caught it). All three numpy-gap
closers shipped + locally measured (FMA product face 1.81×/1.73× vs S26 @1024 1t
f32/f64), fn-strip wired (map-body helper Calls tile now), loop→map/fold lifting
designed for ratification, harness extended to 4096 with fma legs, docs reconciled,
full workspace green (orchestrator-run). **All S27 work uncommitted — commits
pending Sapir's confirm.**
Next step: top up vast.ai → run the box (S28 agenda item 1, one command, prepped)
→ `s27.md` report. Then loop→map ratification.
Resume command/check: `docs/next-session.md` (S28 agenda); `git status`;
`vastai show user` (balance).

## 1. Work completed

- **FMA hypothesis verified in one golden** (pre-implementation): plain emitted IR +
  `-ffp-contract=fast` → **0** fused ops; same IR sed-flagged `contract` → **34
  `fmla.4s` + 2 `fmadd`, 0 unfused vector mul** — since LLVM 14 contraction gates on
  per-instruction IR flags; the driver flag never retrofits textual IR. The S26 box
  finding explained exactly.
- **WP1 (codex): FMA contraction, product face.** `EmitOpts::contract` (default OFF),
  tile-kernel chain emits `fmul contract`/`fadd contract` (float sites only, shared
  code path, both rung-1 and TileCtx trio paths), `--contract` CLI, tile_ab fma leg
  (numeric rel-tol + disasm asserts: fused present + zero unfused vector mul in fma;
  zero fused in conformance), contract-flag golden. **Orchestrator review fixes:**
  numeric compare denominator → `max(|expected|, 1)` (pure rel-error false-fails on
  near-cancellation cells); f64 tolerance 1e-12 → 1e-9 (K·ε at 4096); tolerance
  default inverted to the safe f32 bound (keyed on `widen_f64`, not `widen_f32`).
- **WP2 (codex): rung 3 packing + per-width TJ + k-unroll + prefetch.**
  `EmitOpts::packing` (default ON) + `--no-pack`; `tile_j_for` (f32→16, **f64→8**);
  packed j-tile-major b panel (`packed[jt·(K·TJ)+k·TJ+lane]`, 64-aligned, remainder
  lanes zero-padded never-read); parallel flavor: packed Split task → run-once
  wrapper (pack, then nested `flow_par_begin(1)` slice dispatch — happens-before by
  construction; verified sound: global pool, help-first `finish` (width-1 safe),
  tiled sites trap-free ⇒ nested `check_trap` vacuous); k-unroll ×2 in the TI-blocked
  main body (trailing single step; ascending per-cell k exact) + `llvm.prefetch` of
  the next packed line; f64 differential `_r6_c20_k5_f64` (TJ=8 main/remainder/
  unroll-tail); goldens re-pinned deliberately incl. new `tile_nest_shape_f64`.
- **WP2b (codex, orchestrator-found): the Seq-task packing hole.** Review traced:
  `Frame.packed` covered EVERY packing site but only Split tasks got the wrapper —
  a tiled site inside a LOOP (whole-loop Seq task; `tile_plan` has no loop filter)
  read an **uninitialized** frame buffer; b can be **loop-carried** there, so even
  hoisting would be wrong values. Fix: Frame fields for Split-task sites only
  (filter over `path_plan.tasks`); every other context packs inline at the site per
  iteration. Regression `differential_tiled_matmul_loop_carried_pack` (loop-carried
  b, multi-path entry, structural + oracle + FLOW_PAR=1 + untiled parity) —
  **pre-fix output demonstrably garbage** (`…/0/22` vs `4/64/22`).
- **WP3 (codex): fn-strip wired (Sapir directive).** `PassId::Inline` FIRST in the
  default `rewrite()` list; **new loop-bearing-callee guard** (orchestrator-specified:
  `!loop_structure(g).is_empty()` — inlining a loop-bodied callee would mint nested
  SCCs, a shape lower never produces and backend-llvm rejects; matmul4's both calls
  pinned staying — that unlock belongs to the loop→map plan); `INLINE_MAX_BODY`
  64→**256**; Calls inside Map/Fold body fns verified planned+stripped (sepia's 3
  `clamp` calls flatten). Pins: `loop_bearing_callee_stays_a_call`,
  `default_rewrite_inlines_call_inside_map_body`, `matmul4_loop_callees_stay_calls`,
  llvm `differential_tiled_matmul_via_helper_fn` — **a map body calling a helper fn
  now TILES** (the Call wall down). Fn-bearing rewrite goldens re-pinned (abs, calc,
  fanout, pipeline, sepia, seq_demo), each eyeballed via codex's per-snap report.
- **loop→map/fold lifting DESIGNED** (orchestrator):
  `components/rewrite/plans/plan-loop-to-map-fold.md` — R-LF (loop→fold: counter+acc
  carried, static K, pure advance ⇒ `(seed, Iota(K)) -> Fold`, byte-exact) + R-LM
  (loop→map: counter+output carried, single identity-index `Update`, c-free v-cone,
  n=T ⇒ `Iota(T) -> Map`, init dead by coverage); the existing fixpoint driver +
  wired Inline deliver the matmul4→cap chain with no new sequencing (lift innermost
  fold → inline the now-loop-free callee → lift the outer map → `tile_plan` fires);
  trap order preserved (ascending evaluation + S24 protocol); rejections recorded.
  **For Sapir's ratification; acceptance = the matmul4 non-tiling pin inverts.**
- **Harness (orchestrator):** regen.sh — `_fma.ll`/`_fma_perf.ll` twins for every cap
  stem + first-gen 2048/4096 `.ll`; runner.sh fma builds; runner.py — flow legs to
  4096, fma process/1t/compute legs, CPU baselines extended to 2048/4096 (1t naive =
  1 rep at 4096, ~8 min, labeled); `s27_box.sh` rewritten in the s26b trimmed
  CPU-only form (leg filter, per-face disasm checks, ulimit probe). Artifacts
  regenerated on the final emitter+pipeline; **`.cu` byte-stable (0 changed)** —
  cuda untouched by S27, no GPU re-verification owed.
- **Sapir in-session:** backend-genericity of k-panel blocking answered (legality
  generic in `tile_plan`; factors are `Loc`-specific placement facts; the
  backend-generic `block_plan` schedule query is the rule-of-three extraction when
  cuda consumes `tile_plan`) — recorded in the S28 agenda item 3.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Contraction scope | tile-kernel chain only, `EmitOpts::contract` default OFF | product/conformance face split (S24b GPU precedent, taken as ratified per "implement and test every one"); differential gate stays bit-exact |
| fma verification class | numeric rel-tol vs `max(\|e\|,1)` + disasm asserts, never byte | single rounding legitimately changes bits; near-cancellation cells must not false-fail |
| Packing default | ON (`packing: true`); `--no-pack` = A/B attribution only | packing is rung 3 of tiling, values byte-identical by construction |
| Parallel pack placement | wrapper task + nested global-pool session | happens-before all slices by construction; help-first finish ⇒ width-1 sound; trap-vacuous for tile sites |
| Seq/loop-site pack placement | inline at the site, per iteration | loop-carried b makes hoisting WRONG, not just wasteful; correctness rule, not a compromise |
| k-panel L2 blocking | deferred, gated on 4096 box data | acc spill/reload per panel only pays if the packed walk misses L2 — measure first (plan §ceilings) |
| Per-width TJ | f64→8, f32 stays 16 | f64 at TJ=16 = 32 acc regs = whole NEON file (the S26 f64 lag); packed layout follows |
| Inline position + cap | first in default list; cap 64→256 | flattening exposes cross-fn folds; 256 = headroom for real helpers, still recorded/tunable |
| **Loop-bearing callees never inline** | guard shipped (orchestrator-specified) | would mint nested SCCs (lower never produces; llvm rejects); the loop→map plan is that unlock — matmul4 pinned unchanged |
| loop→map | design-only this session, plan doc for ratification | per agenda ("design next", plan-gate §6.1); implementation is S28+ after Sapir's go |
| Box | NOT run — balance 0 | standing pre-box check; everything prepped so S28 opens with one command |
| numpy pairing flag | untouched, still Sapir's call | standing from S26b |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| FMA golden (matmul512_cap_f32.ll, local clang) | plain 0 fused · flagged 34 fmla.4s + 0 unfused | the per-instruction-flag hypothesis; the whole kernel contracts |
| `cargo test --workspace --release` (orchestrator-run) | exit 0, fmt clean — ir 176 · llvm 46 (22 diff + 24 golden) · rewrite 64 · rt 16 · others | full gate under WP1+2+2b+3 together |
| 1280-run testgen differential (-O0/-O2) | green (inside llvm 22) | R1 with Inline in the default pipeline + packing on |
| `differential_tiled_matmul_loop_carried_pack` | green post-fix; **pre-fix garbage** (`…/0/22` vs `4/64/22`) | the Seq-task hole was real and is closed |
| `differential_tiled_matmul_via_helper_fn` | green — zero Calls post-rewrite, tiled markers present | fn-strip → tile chain works end-to-end |
| `differential_tiled_matmul_r6_c20_k5_f64` | green — tiled==untiled==oracle + packed==nopack, -O0/-O2 | f64 TJ=8 main/remainder/unroll-tail paths |
| tile_ab 512 f32 (1t min-of-3) | tile 5.94 · nopack 5.81 · fma **3.00** · untile 79.0 | fma 1.98× vs tile at 512 |
| tile_ab 1024 f32 | tile 32.8 · nopack 38.1 · fma **19.3** · untile 836.6 | pack 1.16× + fma 1.70×; **1.81× total vs S26's 35.0** |
| tile_ab 1024 f64 | tile 64.0 · nopack 65.6 · fma **38.6** · untile 841.7 | TJ=8 + fma: **1.73× vs S26's ~66.7** |
| tile_ab disasm asserts (every run) | fma: fused + 0 unfused vector mul; conformance: 0 fused | both faces honest |
| `.cu` regen diff | 0 files changed | cuda byte-stable under S27; no GPU debt |
| `vastai show user` | balance **0** | box blocked; standing check held |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` @ `918b583` | **ALL S27 work uncommitted** (emitter+rewrite+tests+snapshots, 29 bench artifacts modified + ~36 new `_fma`/2048/4096 `.ll`, harness ×4, docs ×10, 2 plan docs, this log) — commits pending Sapir | `git status` |
| vast.ai | — | **balance 0; no instances** (`vastai show instances` empty — 45622441 gone since S26b) | `vastai show user` |
| box prep | `benches/matmul/s27_box.sh` | ready (trimmed CPU driver, fma legs, 4096, per-face disasm, ulimit probe) | S28 agenda item 1 |
| uncommitted (Sapir's own) | `examples/fib.flow` | whitespace reflow only, zero test references, inert | `git diff examples/fib.flow` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | commits for all S27 work | `git status` | Sapir confirm → commit (suggested: feat / bench / docs split) | committed |
| P0 | **box run + s27.md report** | `s27_box.sh`; S28 agenda 1 | top up vast.ai → one-command box → `results-s27.csv` → report | s27.md with 1t/par tables to 4096, fma rows labeled |
| P1 | loop→map plan ratification → implementation | `components/rewrite/plans/plan-loop-to-map-fold.md` | Sapir reads + ratifies (or amends) | matmul4 non-tiling pin inverts, byte-exact |
| P1 | cuda consumes `tile_plan` (+ `block_plan` extraction question) | S28 agenda 3 | plan doc when scheduled | smem-tiled cuda GEMM, differential-gated |
| P2 | k-panel blocking | plan-s27 §ceilings | read the 4096 box rows first | decision recorded either way |
| P2 | numpy pairing flag | s26b log; `numpy_bench.py` | Sapir's call | tables re-labeled or convention kept |
| P3 | heap lowering / `time` builtin / shapes legs / P7 | next-session §agenda 5–6 | standing | — |

## 6. Architecture / model changes

flow-ir: **zero change** (S27 cashes existing facts — `tile_plan`'s `b.ci==0`,
`path_plan`'s task kinds, `loop_structure`). backend-llvm: `EmitOpts` +2 morphisms
(`contract`, `packing`); the packed panel is a new emission-time `DataLoc` over b
(same `Dat`, layout functor — R1 absorbs it); the wrapper/slice task split refines
`TrnLoc` placement for packed Split sites. rewrite: the default-pipeline functor
gains Inline as its first arrow; the loop-bearing guard is a new composition rule
(policy) with its `Note:` in the module header. Model-level finding: `tile_plan`
admits loop-membered sites (no SCC filter) — the emitter now handles both placements
correctly; a recognizer-level filter was considered and rejected (loop-membered
sites still deserve tiling — they pack per iteration).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/backend-llvm/plans/plan-s27-fma-packing.md` | new (plan of record) → SHIPPED S27 + deviations (wrapper design; the WP2b hole; k-panels deferred; local numbers) |
| `components/rewrite/plans/plan-loop-to-map-fold.md` | new — DESIGN for Sapir's ratification |
| `components/backend-llvm/IMPLEMENTATION.md` | tile rows → S27 symbols/lines (`emit_tiled_map` :2387, `emit_tiled_map_blocked` :2711, `emit_tile_trio` :2997, `emit_tile_a_values` :3220, `emit_tile_lane_loop` :3246, `packed_buffer` :595, `allocate_frame_packs` :618, `emit_pack_copy` :645, `tile_j_for` :310, `packing_site` :317, `PackedBuffer`/`PackedField`, `emit_map` :3375); EmitOpts row; pins row |
| `components/backend-llvm/STATUS.md` | S27 entry prepended |
| `components/rewrite/STATUS.md` | S27 fn-strip entry + Move-1 policy paragraph revised (default-list, loop guard, cap 256, body-fn Calls strip) |
| `components/rewrite/IMPLEMENTATION.md` | header + `INLINE_MAX_BODY` + `analyze_inline` rows |
| `docs/STATUS.md` | header S27 block; session-table row 27; backend-llvm + rewrite component rows; test counts verified-not-guessed (46/64) |
| `docs/performance/matmul.md` | S27 index row (LOCAL ONLY, box pending) + CSV list |
| `docs/next-session.md` | rewritten to S28 (box = opener; ratification queue; gotchas incl. fma numeric-class rule) |
| `benches/matmul/{regen.sh,runner.sh,runner.py,s27_box.sh,tile_ab.sh}` | harness: fma twins, 4096 legs, box driver, A/B legs (tile_ab by codex + orchestrator tolerance fixes) |
| this log | new |

## 8. Files changed

Code: `crates/backends/llvm/src/{func.rs,lib.rs,module.rs}` ·
`crates/backends/llvm/examples/emit.rs` · `crates/flow-rewrite/src/{driver.rs,inline.rs}`.
Tests: `crates/backends/llvm/tests/{differential.rs,golden_ll.rs}` + 4 llvm snapshots
(2 new) · `crates/flow-rewrite/tests/` (+pins) + 12 rewrite snapshots re-pinned.
Bench: 29 `.ll` regenerated + ~36 new (`_fma` twins, 2048/4096) · `regen.sh` ·
`runner.sh` · `runner.py` · `tile_ab.sh` · `s27_box.sh`. Docs: as §7.
**Nothing committed — pending Sapir's confirm.**
