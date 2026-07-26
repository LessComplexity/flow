# 2026-07-24 — S28: shapes ladder (FIR window rung + conv2d k-split) + the S27 box debt

Orchestrator: Kimi (category-architect skill). Immutable log (ADR-0017). Session opened
with `start` over `2026-07-24-s27c-*.md` + `docs/next-session.md`; Sapir's mid-start
correction: "vast should be available, it has auto re-new, the '0' is the mistake of the
agent not real." Two threads ran in parallel: the S28 focus build (fir/conv2d) and the
S27 box matrix.

## 0. Continuation brief

Current state: **the S28 focus is shipped and measured, the box debt is paid, everything
reconciled; commits pending Sapir.** flow-ir records conv2d's `(k÷3,k%3)` site
(`TileRead.ksplit` — the map-body derived-var move one level down); the llvm backend
gained two emitter-local rungs: the FIR 1-D window rung (the rung-2 dual) and the conv2d
unrolled micro-kernel (zero div/mod in the nest). **fir: both tables WON local AND box.
conv2d: kernel 3× over cpp-mt; box par table WON at the leg level; the M4 par leg is OPEN
on the gen measurement boundary** (the FLOW_PERF bracket spans the untiled img-gen map the
baselines exclude — the S28 recorded finding, suggestion #14). Box (on-demand 45712913,
destroyed ≈$0.45): the full S27 matrix + S27/S28 shapes — disasm gates pass (S26 vfmadd
finding CLOSED: conf 0 / fma 128 vfmadd 0 unfused), 2048/4096 flow rows clean, OpenBLAS
frontier measured (threaded 9.7×/5.9× @1024/@4096 f32; numpy-1t 2.7× ahead of fma-1t wall),
GRAIN quantization measured (fir 61T 0.526 → 16T 0.287). Gate: full workspace 69/69 green;
matmul .ll artifacts byte-identical. Next step: S29 agenda 1 (the gen-boundary fix —
Sapir chooses fusion vs kernel-scoped bracket vs bench restructure), then the OpenBLAS
levers (KC k-panel / a-panel packing) + heap lowering.
Resume command/check: `docs/next-session.md`; `docs/performance/matmul/s27.md` (box section).

## 1. Work completed

- `start` protocol: latest log + STATUS + next-session read; live state rehydrated
  (`git status` clean — S27 committed; vast.ai inspected).
- **vast.ai misread corrected (Sapir):** `vastai show user --raw` → `credit: 15.41` +
  autobill ($5→$20); `balance: 0` is NOT a block signal. Found instance 45692618 (EPYC
  7713 zen3, 64-core, $0.4056/hr) RUNNING at 0% util — the S27 handoff's "no instances"
  was wrong too. Rule recorded in next-session gotchas.
- **S27 box matrix launched, lost, relaunched:** 45692618 VANISHED mid-run (~1h in —
  instance gone from the account, SSH refused; cause unknown, Sapir asked whether it was
  him). Relaunched on-demand as 45712913 (EPYC 7B13, $0.16/hr) with incremental log pulls;
  ran to completion: full matmul matrix + S27-baseline shapes, then the S28-code shapes
  re-run + a FLOW_PAR=16 diagnostic. Destroyed after (≈$0.45 total this instance).
- **Plan first (§6.1):** `docs/components/backend-llvm/plans/plan-s28-shapes-ladder.md`
  (categorical model, composition rules 1–5, done-whens, ceilings) after two parallel
  read-only explorations (recognizer map; emitter map).
- **A1 — flow-ir k-split record (orchestrator-built):** `TileKSplit{div,cq,cr}` +
  `TileRead.ksplit?` (§3 consolidation: partial morphism, not a new site type);
  `tile_fold_shape` binds the fold-body `Div`/`Mod` pair on the counted element (slot
  `fold_captures+1`, shared literal, `depth % div == 0`) as `kq`/`kr` walker axes via the
  EXISTING `tile_split` helper; `tile_affine` widened to the private checked `TileAffine`
  (6 coefficients); rule 1 (mixed raw/derived k refuses) in `index_parts`; unused pair ⇒
  `ksplit: None` (matmul/fir records bit-identical). conv2d_16 records
  `{ci:18, clane:1, ck:0, ksplit:Some{div:3,cq:18,cr:1}}`.
- **A2 — emitter guard (same change):** `emit_map` site filter + `packing_site` excludes
  ksplit — a recorded k-split site could NEVER hit the affine tile path (would emit wrong
  addresses); conv2d emission byte-stable (tiled == --no-tile verified).
- **B — FIR 1-D window rung (agent-2, orchestrator-reviewed):** `window1d_site`
  (`rows == 1 && b.ck == 1 && b.ksplit.is_none()`) → `emit_tiled_map_blocked_1d` +
  `emit_tile_window_block`/`emit_tile_window_step`: full TI·TJ blocks unmasked, ONE scalar
  `a` load per k shared across TI=4 subrows, constant-TJ main, ×2 k-unroll (K even),
  remainder = TI=1 j-split discipline; non-window 1-D byte-stable (negative control).
- **A3 — conv2d micro-kernel (agent-2 resumed, orchestrator-reviewed):** `conv_site` gate,
  A2 filter retargeted (untiled fallback ONLY for non-conv ksplit), `emit_tiled_map_conv`
  + `emit_tile_conv_tile` + `ConvTileCtx`: rung-1 row idiom, fully unrolled (kq,kr) tap
  nest (k-ascending = R1), compile-time tap offsets, ZERO sdiv/srem; TI=1 (row blocking a
  recorded ceiling).
- **Gate v1 failure diagnosed as environmental, not code:** flow-syntax example tests
  ENOENT — the repo MOVED (`/path/to/...` → `/path/to/...`) and stale
  fingerprints baked `env!("CARGO_MANIFEST_DIR")` with the old path. Fix: `cargo clean -p`
  of the six path-baking packages. Gate v2: 69/69 green. Gotcha recorded.
- **Measurements (local M4 Pro, clean idle, min-of-3; box EPYC 7B13):** see §3.
- **Reconciliation:** ir + backend-llvm component docs (two parallel agents — both found
  ADR-0017's "DESIGN.md IS the ARCHITECTURE doc" rule and applied it), `deduced-queries.md`
  tile walkthrough (+k-split, stale line-ref), root `IMPLEMENTATION.md` tile row,
  `docs/STATUS.md` (header + ir/llvm rows + session-table row), perf report (S28 shapes
  section + box section), perf index (S28 row), `next-session.md` rewritten for S29.
  Reverted an agent's 94-line prettier churn on `categorical-model.md` (formatting-only,
  unrequested); corrected one agent's 52→53 llvm test count.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Box #1 vanish → relaunch on-demand + incremental pulls | kept | instance loss costs ~$1 + an hour; on-demand can't be preempted; pulls make any loss recoverable |
| `ksplit` as `Option<TileKSplit>` on `TileRead` | kept (§3) | the SAME read object + one partial morphism; doubles as the emitter-gate discriminator; `None` = bit-identical pre-S28 records |
| ksplit sites never touch the affine tile path | kept (rule 3) | the affine emitter hardcodes lane coeff 1 and ignores ksplit — silently wrong addresses; guard landed WITH the record |
| conv emission TI=1 | kept (v1) | row blocking is the next ceiling (#11, img-row reuse ×3); the micro-kernel already beats cpp-mt's kernel |
| FIR rung zero flow-ir change | kept (rung doctrine) | emitter-local predicate cashing recorded facts — the S26/S27 pattern |
| `cargo clean -p` of six path-baking packages (not full clean) | kept | targeted rebuild; gate v2 green |
| Box destroyed after use (project norm) | kept — Sapir confirm invited | ≈$0.45 total; next-session asks about box #1's vanish |
| prettier churn on `categorical-model.md` (agent side-effect) | reverted | formatting-only, unrequested, out of scope |
| `deduced-queries.md` + ir STATUS test headline (153→180) | fixed in-session | stale living docs found during reconciliation |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test --workspace --release` (v2, post-clean) | **69/69 suites ok** | the whole tree incl. S28 rungs |
| `cargo test -p flow-ir --release --test algos` | 62/62 (incl. `tile_conv2d_site_recognized` full-site assert + 3 refusal pins) | the k-split record + rules 1–2 |
| conv2d_16 emit tiled vs `--no-tile` (A2 state) | byte-identical | the guard routes k-split to untiled |
| matmul256_cap.ll re-emit vs checked-in | byte-identical | no affine-path drift through A1/B/A3 |
| llvm differential | 28/28 (fir remainder + fir split ×FLOW_PAR; conv2d 16/20/92, mid-tile GRAIN splits, -O0/-O2, interp oracle byte-equal) | bit-exactness of both rungs |
| golden_ll | 25/25 (1-D snap re-pinned deliberately + structural; conv pin: 0 sdiv/srem, 9 taps) | emission shapes pinned |
| Box disasm gates | conf 0 vfmadd; fma 128 vfmadd / 0 unfused vmulps | S26 finding CLOSED on zen3 |
| **Local shapes (min-of-3, clean idle M4 Pro)** | fir fma-par **0.2133** vs cpp-mt 0.2395 / rust-mt 0.3017 / numpy 0.3932; fir fma-1t **0.2156** vs cpp-1t 0.9239 / rust-1t 0.8461; conv2d fma-par 0.5109 vs cpp-mt 0.133 (gen ≈0.47 of it) | **fir both tables WON**; conv kernel ≈0.04 = 3× cpp-mt (decomposition measured) |
| **Box shapes S27→S28** | fir par 0.316→0.526(61T)/**0.287(16T)**; fir 1t 0.925→**0.786** vs cpp-1t 2.78; conv2d par 0.752→**0.742** vs cpp-mt 2.32 | fir box tables WON; **conv2d box par table WON at leg level**; GRAIN quantization measured (16 slices = 0.26 waves @61T → #15) |
| Box matmul (compute, f32) | flow conf/fma @1024 13.10/18.15 vs cpp-mt 122.6 / rust-mt 79.2 / chapel 97.0; @4096 275.5/283.5 vs 9305/7737/9576; **numpy threaded 1.87/48.1 @1024/@4096 ahead 9.7×/5.9×; numpy-1t 20.5 @1024 = 2.7× over fma-1t wall** | flow ahead of every language baseline; OpenBLAS is the measured frontier (agenda-2) |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume | Stop / cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` | **all S28 work uncommitted** — pending Sapir | `git status`; suggested split: feat(ir+llvm rungs) / bench(results-s27.csv) / docs(reconcile+plan+logs) | — |
| vast.ai | account | credit ~$14.5 post-session, autobill on; **0 instances** | `vastai show user --raw`; `vastai show instances` | — |
| box | 45712913 | **destroyed** (≈$0.45) | — | done |
| box | 45692618 | vanished mid-run (cause unknown — Sapir asked) | — | — |
| artifacts | `benches/matmul/results-s27.csv` | the box matrix, spec-stamped, committed-candidate | `head -3` (spec block) | keep |
| artifacts | `target/tmp/s28/` | box logs (driver, runner, shapes_s28), local shapes finals, A/B .ll | disposable (CSVs/docs hold the numbers) | delete anytime |
| data | box results in docs | `docs/performance/matmul/s27.md` box section + perf index row | — | — |

## 5. Open items

| Priority | Item | Doc/code reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **Gen measurement boundary** (conv2d M4 leg) | backend-llvm suggestions #14; s27.md box section | Sapir picks: gen-map fusion/inlining vs kernel-scoped `FLOW_PERF` bracket vs bench restructure | conv2d_512 flow-par leg ≥ cpp-mt on M4 (box already won) |
| P1 | OpenBLAS levers | next-session agenda 2 | KC k-panel split (4096 data), a-panel packing, f64 TJ/unroll, prefetch; heap lowering rides here | numpy-1t gap @1024 < 2×; threaded gap shrinking |
| P1 | Commits | this log §4 | Sapir confirms; split feat/bench/docs | `git status` clean |
| P2 | Conv TI rows + im2col | suggestions #11/#12 | TI over output rows (img-row reuse ×3, `b.ci == cq`); im2col as `emit_pack_copy` sibling → rungs 2+3 | conv2d kernel further ahead; measured |
| P2 | GRAIN slicing policy | suggestions #15 (measured) | slice-count-aware grain at sub-ms N (flow-rt) | fir box 61T ≈ 16T number |
| P3 | Box #1 vanish explanation | next-session open Q | Sapir answers (him vs vast) | answer recorded |
| P3 | standing | next-session agenda 5/6 | cuda consumes tile_plan (incl. ksplit/window in the design); `time` builtin; P7; ADR rows; loop-lift v2; `exp` | per item |

## 6. Architecture / model changes

- **`Dat` (flow-ir):** `TileRead` gains partial morphism `ksplit? : TileRead → TileKSplit`
  ({div, cq, cr}); a read's address is now `base + ci·i + clane·lane + ck·k + cq·(k÷div)
  + cr·(k%div)`. §3 consolidation applied — no new site type.
- **`Trn` (recognizer):** the map-body derived-var split (`tile_split`) now has its
  fold-body analog inside `tile_fold_shape`; the walker (`tile_affine`) carries six
  checked coefficients (`TileAffine`).
- **`TrnLoc` (strategies, §4.4 — backend-llvm):** two new parallel realisations over the
  same site contract, selected by record facts: `window1d_site` →
  `emit_tiled_map_blocked_1d` (the rung-2 DUAL — shared-vs-varying roles swapped);
  `conv_site` → `emit_tiled_map_conv` (the k-split constant-folded).
- **Composition rules (plan-s28):** (1) `ksplit ⇒ ck == 0`; (2) `depth % div == 0`;
  (3) ksplit site ⇒ conv branch or untiled fallback, never the affine tile path;
  (4) per-cell chain k-ascending in every new branch; (5) no masked dead lanes;
  runtime-`tj` only on remainder tiles.
- **Coherence (§4.5):** no new placements outside the single-process `Loc`; the rungs
  change emission shape only — Law 1/4 unaffected. Known divergence: none.
- **Recorded findings (new model facts):** the gen measurement boundary (the legs' `Trm`
  boundaries differ from the baselines' — placement honesty at the benchmark level) and
  the GRAIN quantization (a placement-count effect: 16 slices over 61 threads).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/components/backend-llvm/plans/plan-s28-shapes-ladder.md` | new (the model-first plan) |
| `docs/components/ir/IMPLEMENTATION.md` | tile row + k-split composition-rules row + TileAffine note |
| `docs/components/ir/STATUS.md` | S28 header; test headline 153→180 corrected |
| `docs/components/ir/suggestions.md` | #2 general-ksplit ceiling (no measured demand) |
| `docs/components/backend-llvm/DESIGN.md` | new Tile-ladder subsection (rungs 1–3 context + B/A3 rows + rules 3–5) — ADR-0017: DESIGN.md IS the architecture doc |
| `docs/components/backend-llvm/IMPLEMENTATION.md` | tile row retitled + S28 symbols (line-verified) + pin rows |
| `docs/components/backend-llvm/STATUS.md` | S28 header + gen-boundary known-issue; count fixed to 53 |
| `docs/components/backend-llvm/suggestions.md` | #11 conv TI · #12 im2col · #13 window rotation · #14 gen boundary · #15 GRAIN |
| `docs/architecture/deduced-queries.md` | tile walkthrough: k-split paragraph, record, mermaid node, dispatch note, stale line-ref |
| `docs/IMPLEMENTATION.md` | tile row: S27 rung 3 + S28 ksplit/consumers |
| `docs/STATUS.md` | S28 header + ir/llvm rows + session-table row 28 |
| `docs/performance/matmul/s27.md` | S28 shapes section + box section (matrix, shapes S27→S28, gates, findings) |
| `docs/performance/matmul.md` | S28 index row; stale "balance" notes corrected |
| `docs/next-session.md` | rewritten for S29 (agenda, open questions, gotchas incl. vast + repo-move + GRAIN) |
| this log | new |

## 8. Files changed

Code: `crates/flow-ir/src/algo.rs` (TileKSplit/TileRead.ksplit/TileAffine/recognizer),
`crates/flow-ir/src/lib.rs` (export), `crates/flow-ir/tests/algos.rs` (conv2d fixture + 4
tests + 2 literals), `crates/backends/llvm/src/func.rs` (guard, window1d rung, conv rung),
`crates/backends/llvm/tests/{differential.rs,golden_ll.rs}` + `golden_ll__tile_nest_shape_1d.snap`
(re-pinned) + `golden_ll__tile_nest_shape_conv.snap` (new). Bench: `benches/matmul/results-s27.csv`
(new, box). Docs: as §7. **Nothing committed — pending Sapir's confirm.**
