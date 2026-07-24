# 2026-07-24 — S27b: loop→map/fold lifting + panel residence (Sapir's close-review directives)

Orchestrator: Claude Fable (category-architect skill). Immutable log (ADR-0017).
Same-day continuation of `2026-07-24-s27-fma-packing-fnstrip.md` (the S26b-precedent
letter suffix), driven by Sapir's S27 close review: (a) k-panel deferral challenged —
"K panels deferred? Why?"; (b) backend-genericity pressed a second time; (c)
**loop→map/fold plan RATIFIED**: "I want even loop naive implementations to enjoy the
perf boost from this structure automatically." Codex implemented (WP4, WP5);
orchestrator designed, reviewed line-by-line, adjudicated the one blocker.

## 0. Continuation brief

Current state: **S27b closed.** (1) Panel residence shipped (WP4): j-tile-outer /
i-block-inner for packed sites — the panel stays in the core's private L2 across the
thread's i-blocks (b-traffic ÷4 @1024, ÷16 @4096 per thread), zero acc spill, byte-exact
(cell visit order only). (2) **loop→map/fold lifting shipped (WP5)**: `PassId::LiftLoops`
after Inline; R-LF + R-LM at the ratified v1 boundary (K ≥ 1 amendment adjudicated
in-flight); **matmul4 loop form now lifts → inlines → tiles — the S26 non-tiling pin
inverted**; fir's loop form fold-lifts too (unplanned bonus). All loop-form bench
artifacts regenerated: matmul16..128 `.ll` now tile (the old N⁴-wall legs are dead).
Full workspace green (orchestrator-run, 72 suites, fmt clean). **Box still BLOCKED on
vast.ai balance 0. All work uncommitted — commits pending Sapir.**
Next step: S28 = box run (one command, prepped) → `s27.md` report; then cuda
`tile_plan` consumption (+ `block_plan` extraction — suggestions #10).
Resume command/check: `docs/next-session.md`; `git status`; `vastai show user`.

## 1. Work completed

- **WP4 (codex): panel-residence loop swap.** Packed 2-D sites emit j-tile OUTER /
  i-regions inner (`emit_tile_packed_j_outer` + `emit_tile_panel_base` — ONE panel-base
  computation per jt, structurally asserted); head/interior/tail row discipline
  preserved per jt; unpacked/1-D/`--no-pack` paths byte-identical (snapshots unmoved).
  Rationale (the deferral correction Sapir forced): packing made b-reads sequential but
  not smaller — every i-block re-streamed the whole packed b (N³/TI per thread = the
  measured flat par floor). Residence cuts per-thread b-traffic by i_blocks_per_thread
  with NO k split and NO acc spill. True KC-split stays the box-gated refinement.
  Local 1024 f32: tile 32.76 / fma 18.94 ms (flat vs pre-swap locally — M-series SLC
  already held b; zen2 private-L2 is the payoff, box pending).
- **WP5 (codex, one STOP round): loop→map/fold lifting.** First launch stopped
  correctly on a real plan hole: my K=0 edge claimed `Iota(0)` — Core has no empty
  arrays (`Ty::Array` ≥ 1, `iota` count ≥ 1). **Adjudicated: K ≥ 1 is a condition of
  both rules** (zero-trip loops stay loops — dead-code territory, not a lift arm; R-LM's
  T ≥ 1 falls out of n = T ∧ n ≥ 1); plan amended; relaunched. As shipped:
  `flow-rewrite/src/lift.rs` — `analyze_lift` consumes `loop_plan` verbatim (no SCC
  re-derivation); R-LF (exact (counter, acc) product, init 0/step +1/`Lt` guard vs
  const K ≥ 1, pure token-free cone, exit = acc → minted count + `Iota(K)` + captured
  Fold seeded by the acc init); R-LM (counter + `[E; n]`, exactly ONE identity-index
  `Update` — index checked by OBJECT identity, c-free v-cone, n = K, exit = c →
  `Iota(K)` + captured Map, init edge dead by coverage); `body_cone` clones pure
  invariant derivations into the body up to the parameter-projection capture boundary
  (keeps affine fields visible to `tile_plan` — the load-bearing subtlety);
  `covers_loop_body` audits every decide/advance morphism into cone-or-scaffolding
  (no silently dropped work, incl. trapping work — rejection-pinned).
- **The acceptance chain, live:** default-rewritten `examples/matmul4.flow` = 0 Calls,
  0 loop SCCs, Map-with-Fold body; llvm selects the packed align-64 tile path; stdout
  exactly `-275\n3748\n` at -O0/-O2, default env + FLOW_PAR=1. The WP3
  `matmul4_loop_callees_stay_calls` pin rewritten to its inversion. fir.flow's k-loop
  fold-lifts too (goldens re-pinned, eyeballed).
- **Bench artifacts regenerated on the final pipeline:** loop-form `matmul{16,32,64,128}`
  `.ll`/`.cu` now lift+tile (verified: `[64 x float]` acc + align-64 packed + prefetch
  in matmul64.ll); `matmul4.{ll,cu}` regenerated explicitly from `examples/` (stale —
  regen keys on bench-dir sources; orchestrator catch).
- **Docs:** suggestions #10 recorded (`block_plan` — the backend-generic blocking
  schedule query, Sapir's direction, gated on cuda as second consumer); rewrite
  STATUS/IMPLEMENTATION/DESIGN + plan doc As-built (codex-drafted, orchestrator
  label-fixed S28→S27b); next-session rewritten to S28.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| k-panel deferral | REVISED per Sapir: decompose — (a) panel residence NOW (no spill), (b) KC split box-gated | packing fixed access pattern, not volume; residence is most of the win at zero complexity cost |
| **K ≥ 1 condition (both lift rules)** | ratified in-flight (codex option 2) | Core has no empty arrays; zero-trip loops are dead code for const-fold/DCE, not a lift arm |
| Lift recognizer scope | v1 EXACT canonical shapes; every rejection pinned | oracle-equality bar; widening is future rungs, each its own proof |
| Identity-index check | ObjectId identity, not value equality | v1 discipline; value-equal-but-distinct index objects stay loops (recorded) |
| Invariant cloning boundary | parameter-projection = capture boundary | cloning through it would capture whole tuples and blind `tile_plan` to affine fields |
| Codex session log + "S28" labels | absorbed into this log; labels → S27b | one continuation, one authoritative handoff (ADR-0017); S26b letter precedent |
| `block_plan` | suggestions #10, gated on cuda consumption | rule of three; Sapir's direction committed, extraction at second consumer |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` (orchestrator-run, post-WP4+WP5+regen) | exit 0 — 72 suites ok, fmt clean | the whole S27+S27b stack coheres |
| rewrite crate | **68** green (lift positives + every rejection pin + inverted matmul4 pin + 6-pass property battery) | R-LF/R-LM at the v1 boundary |
| llvm crate | **47** green (23 differential incl. 1280-run with LiftFold/LiftMap testgen steps + matmul4 tiled acceptance · 24 golden) | lift → tile chain end-to-end, byte-exact |
| matmul4 acceptance | 0 Calls, 0 SCCs, Map{Fold}; tiled; `-275\n3748\n` exact at -O0/-O2 × {default, FLOW_PAR=1} | Sapir's unlock live |
| WP4 tile_ab 1024 f32 | tile 32.76 · fma 18.94 · no-pack 38.96 · no-tile 817.5 | swap holds locally (residence pays on zen2, not SLC) |
| matmul64.ll probes | 1× `[64 x float]` acc, align-64 packed, 2× prefetch | loop-form bench legs now tiled |
| WP5 first-launch STOP | codex reported the Iota(0) conflict instead of widening scope | the STOP-on-conflict instruction worked as designed |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` @ `918b583` | **ALL S27+S27b work uncommitted** — commits pending Sapir | `git status` |
| vast.ai | — | balance **0**; no instances | `vastai show user` |
| box prep | `benches/matmul/s27_box.sh` | ready — S28 opener | next-session agenda 1 |
| uncommitted (Sapir's own) | `examples/fib.flow` | whitespace reflow, inert | `git diff examples/fib.flow` |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | commits (S27 + S27b together) | `git status` | Sapir confirm → commit | committed |
| P0 | box run + s27.md report | `s27_box.sh` | top up → one command → report | tables to 4096, both faces |
| P1 | cuda `tile_plan` + `block_plan` extraction | suggestions #10; next-session agenda | plan doc | llvm+cuda consume one schedule query |
| P2 | KC-split decision | plan-s27 §ceilings | read 4096 box rows | recorded either way |
| P2 | numpy pairing | s26b flag | Sapir | tables labeled |
| P3 | lift v2 rungs (tuple accs, non-identity index, non-static bounds) | plan-loop-to-map-fold §Rejections | future | — |

## 6. Architecture / model changes

rewrite: `RewritePlan` gains the sixth channel `lift : merge → LiftSpec`; `PassId`
gains `LiftLoops`; the default functor is now `Inline → LiftLoops → ConstFold → Cse →
Dce → MapFusion` to fixpoint — **the fixpoint interleave is load-bearing** (matmul4
needs lift→inline→lift across rounds). The K ≥ 1 condition is a composition rule with
its Note in the plan. backend-llvm: packed-site nest order refined (jt-outer) — a
placement re-scheduling, no model change. flow-ir: untouched again (both features cash
existing deduced queries — `loop_plan`, `path_plan`, `tile_plan`).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/rewrite/plans/plan-loop-to-map-fold.md` | RATIFIED → SHIPPED S27b + K≥1 amendment + As-built |
| `components/rewrite/{STATUS,IMPLEMENTATION,DESIGN}.md` | lift rows/sections (codex-drafted, orchestrator-relabeled) |
| `components/rewrite/reviews/review-loop-to-map-fold.md` | codex's implementation review notes (kept) |
| `components/backend-llvm/plans/plan-s27-fma-packing.md` | deviation (3) revised: panel residence shipped, KC box-gated |
| `docs/suggestions.md` | #10 `block_plan` (Sapir direction, S27 close) |
| `docs/STATUS.md` | S27b additions (this close) |
| `docs/performance/matmul.md` | S27 row extended (residence + lift; loop-form legs tiled) |
| `docs/next-session.md` | → S28 (relabeled from codex's S29 draft) |
| this log | new (absorbs codex's deleted s28 draft log) |

## 8. Files changed

S27b increment: `crates/flow-rewrite/src/{lift.rs(new),plan.rs,replay.rs,driver.rs,inline.rs}` ·
`crates/flow-rewrite/tests/{lift.rs(new),inline.rs,property.rs,testgen/mod.rs}` + 4 golden
re-pins (fir/matmul4 mermaid+report) · `crates/backends/llvm/src/func.rs` (jt-outer) ·
`crates/backends/llvm/tests/{golden_ll.rs,differential.rs}` + tile snapshots re-pinned ·
loop-form bench artifacts `matmul{4,16,32,64,128}.{ll,cu}` regenerated · docs as §7.
**Nothing committed — pending Sapir's confirm.**
