# Session 20 — 2026-07-22 — Optimization marathon: emitter-quality wave, arenas, by-ref, bounds proofs, iota/fill stage 1

Orchestrator: Kimi Code · skill: category-architect. Immutable log (ADR-0017). **This log was written post-hoc from the session export (`kimi-export-session_-20260722-024239.md`, 49 turns, 599 tool calls) — the session ended on a completed background-suite notification before its own `end` ran; the reconcile was performed in the following session.** Scope: Sapir's marathon mandate — "implement every optimization possible… match or beat the native backend/language… use vast.ai on demand, no approval needed for ADR suggestions" — executed as: emitter-quality wave (#12/#13/#14/#17), smart arenas v1.0 (#18), kernel-time instrumentation (#19a), llvm by-ref captures/args (#6/#8), fn attributes (#7), last-use query + Update elision (#2), flow-rewrite `inline` pass, `bounds_proof` + capture-range flow (S20a/b/c), ADR-0028/0029 decided, iota/fill stage 1 + surface, and a full vast.ai benchmark sweep.

## 0. Continuation brief

Current state: **all S20 code landed locally; llvm golden_ll 2 stale insta snapshots re-pinned during this reconcile (the session's only red); workspace gate at reconcile: **820 green, 0 failed** (200 syntax · 142 ir · 151 lower · 29 check · 61 interp · 59 rewrite · 22 llvm · 1 flow-rt · 153 cuda + 2 flow-cli bins — see §3).** Sweep results landed (`benches/matmul/results.csv` 76 rows; `docs/performance/matmul.md` rewritten); box A destroyed at reconcile (its results were already local). flow-cuda capture matmul: ~0.2 ms kernel time at every N (163 GFLOP/s @ N=256), 40–321× over its own loop form; llvm matmul64_cap 2,076 ms → 29 ms wall (capture) / 9 ms (loop).
Next step: **S21 per `docs/next-session.md`** — box re-run for post-S20c FLOW_PERF numbers (trap-free kernel), then the queued waves: ADR-0029 stage 2+ (cuda iota/fill kernels, procedural generators — the BL1 fix), region_plan step 2, arena v1.1 (#18a), tree-fold (ADR-0028).
Resume command/check: `cargo test --workspace`; read `docs/next-session.md`.

## 1. Work completed

**Plans (model-first, §6.1):**

- `docs/components/backend-cuda/plans/plan-smart-arenas.md` — capacity deduced from the execution graph, one arena per fn zone, var = `base + offset` @ 256 B, `ARENA_MAX_BYTES = 4 GiB` emit-time guard, `d_trap` carve-out, `malloc_buffer` stays the single choke point; v1.0 = FnScope zones only (LoopCone = v1.1 debt).
- `docs/components/ir/plans/plan-last-use.md` — one deduced query `last_use : IR × FuncId → LastUsePlan` (death/escapes/carried_by/dead_after), three consumers: cuda back-edge freeing, llvm Update memcpy elision, arena v1.1 coloring.

**CUDA backend (flow-backend-cuda: 119 → 153 tests):**

- **#17 kernel shape dedup** — `kernel.rs:emit_kernel_set`/`KernelSet`, key = emitted text minus name; vector_add 5→4, zip_demo 8→5, matmul 6→4 kernels.
- **#12 dead-host-twin elimination** — `kernel.rs:host_reachable`/`dead_host_twin`; matmul host static defs 3→1.
- **#13 constant-divisor guard elision** — `kernel.rs:const_int_operand`; extended in S20c to a TrapCaps credit (`v != 0 && v != -1` ⇒ Div/Mod not trap-capable).
- **#14 trap-param trimming** — `kernel.rs:TrapCaps`; refined S20a/b (bounds proofs) and S20c (capture-range flow): matmul64_cap map kernel has **no trap param, 0 bounds guards**, trap checks 3→1 → 0 on the one-kernel golden.
- **#18 smart arenas v1.0** — new `src/arena.rs` (`arena_plan`), `kernel.rs:abi_sizeof/abi_alignof/align_up/buffer_bytes_of`, `func.rs:FnEmit::arena` + `alloc_buffer` offset carve, per-struct `static_assert`; gates: one-kernel matmul 8→1 mallocs (2048 B zone), vector_add 7→1, fir 4→3, micro_loop_update 3→2 (CI pin `arena_gates_plan_section_7`).
- **#19a kernel-time instrumentation** — `lib.rs:EmitOpts{perf_timing}` + `emit_with_opts` (default emission byte-identical), per-site `cudaEvent_t fev{i}` pairs, `FLOW_PERF launch=…/total ms=` lines, `examples/emit.rs --perf`.
- **#2 (cuda half) in-place Update + back-edge freeing** — `func.rs:FnEmit::last_use`, `update_site` in-place path, `in_place_update` + `merge_family_dead`/`owned_loop_init`/`fresh_owned_buffer`, `emit_back_edge_frees`; `kernel.rs:DevEmit::in_place`; matmul .cu byte-identical (borrowed-init veto keeps full copy — intended).
- **S20a/b guard elision + TrapCaps proofs** — `DevEmit.proof` per-fn; `TrapCaps.proofs: SecondaryMap<FuncId, BoundsProof>`; Index ⇒ `!proof.proven(m)`.

**LLVM backend (flow-backend-llvm: 18 → 22 tests):**

- **#6 by-reference array captures** — `ty.rs:lower_body_input_ty`, `FnEmit::{byref,ptr_resident}`, `component_ptr`, `body_call_arg`, `array_operand_ptr`, `load_whole` deep-copy on escape. matmul64_cap **2.08 s → 0.01 s** (~200×; the 17 GB memcpy wall gone).
- **#7 trap-aware fn attributes** — `func.rs:FnAttrs` fixpoints; refined with bounds proofs (map/fold bodies now `readonly nounwind willreturn`).
- **#8/BL5 by-ref array call args** — `ty.rs:lower_named_input_ty`; loop-form matmul64 **0.33 s → 0.01 s**; 13 snapshots re-pinned.
- **#2 (llvm half) last-use Update memcpy elision** — `FnEmit::lup: LastUsePlan`, `elided_updates`, legality `update_in_place_source` (func.rs:977); explicit vetoes ptr-resident/by-ref/Constant; `llvm.memcpy` 2 → 1 (declare only) in the matmul4-class pin.
- **S20b** — proven-Index guard elision (`emit_index`/`guard_index` func.rs:608-619) + FnAttrs refinement.

**flow-ir (118 → 142):**

- **W3b last-use query** — `algo.rs:LastUsePlan` (:56) + `CategoryIr::last_use_plan` (:636); composed from `topo_order` + `loop_plan`, never re-derived; deterministic, total on sealed fns.
- **W6 `bounds_proof` query** — `algo.rs:CategoryIr::bounds_proof(f) -> BoundsProof`, unsigned interval lattice `Rng{Int(u64,u64), EnumIdx(u64)}`, negative-capable shapes bail to unknown; **S20c** restructure → recursive `bounds_proof_inner(f, visited)` with capture-range seeding (a capture at slot j rides the site's slot-j feeder through the enclosing site-owner's analysis) — the unlock that proved the matmul fold body's captured `a[i*4+k]`/`b[k*4+j]` reads in-bounds.
- **ADR-0029 IR** — `Operation::Iota`/`Fill` (graph.rs), validate rules + `IotaCountMismatch`, builder `iota(count)`/`fill(x, count)` + `IrError::NonStaticCount`, mermaid labels. "32-variant Core Operation set."

**flow-rewrite (45 → 59):** **W3a `inline` pass** (region-emission Move 1) — `src/inline.rs:analyze_inline` (`INLINE_MAX_BODY = 64`, untuned), `RewritePlan::inline` channel + `PassId::Inline` (deliberately NOT in default `rewrite()`), `replay.rs:inline_call` + `RetDest{Own,Fresh}`; PROPTEST_CASES=512 green; matmul4 strips to one primitive graph. Replay arms for Iota/Fill.

**flow-syntax (200) / flow-lower (151) / flow-interp (61):** iota/fill surface — P0108 carve (`iota(...)`/`fill(...)` are the only legal call expressions), L1612 IotaArgs / L1613 FillArgs, `typing.rs` `ExprKind::Call` WTy::Array synthesis, `emit.rs:emit_iota/emit_fill` + `static_count_arg`; interp oracle arms + `tests/iota_fill.rs` (3 contracts + 2 e2e). llvm `emit_iota`/`emit_fill` (i64-indexed store loops); cuda honest `EmitError::Unsupported` stubs (5th Unsupported cell; TrapCaps classifies both trap-free).

**Benches & sweep:** `benches/matmul/cpp_naive.cpp` (f32+f64), `chapel_matmul.chpl`, f32 capture variants, runner legs (cpp-naive, flow-cuda-cap-f32, flow-llvm-cap-f32), clang `-march=native` skew fix, `finish.sh`/`finish2.sh`/`finish3.sh` rescue scripts; 34 emitted artifacts checked in; `results.csv` backed up to `results-pre-s20.csv` (45 rows) then rewritten (76 rows).

**ADRs decided under Sapir's delegated mandate:**

- **ADR-0028** tree-reduction of exact-op folds (wrapping Add/Mul, Min/Max, And/Or/Xor, int/Bool); f32/f64 folds stay sequential-pinned.
- **ADR-0029** array-construction builtins (`iota(n)`, `fill(x,n)`, `widen`) — **STAGE 1 SHIPPED**; motivated by measured 23 MB literal `.ll` modules / 27 GB clang RSS / OOM kills (the BL1 wall).

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Marathon scope | Sapir verbatim: "implement every optimization possible… match or be more performant than the same one in its native backend/language… use vast ai — create an instance on demand and destroy when no longer needed… for the adr suggestions no need for my approval, let the best option take the case" | S20 governing mandate |
| Arena design | capacity deduced from graph; one arena per fn zone; var = base+offset @ 256 B; 4 GiB emit-time guard; LoopCone sites stay per-buffer (v1.1 debt) | Sapir's S19 spec; honest compile-time error over runtime surprise |
| Chapel baseline | CPU `forall` idiom; `chapel-gpu` leg future (`CHPL_LOCALE_MODEL=gpu`) | Chapel added as "almost a direct competitor" |
| BL1 wall | N=256 llvm legs dropped: literal-store `.ll` (23 MB) OOMs clang -O2 at 500 GB+; 5.7 MB module = 27 GB RSS / 1.5 h+ CPU | recorded honestly; ADR-0029 procedural arrays is the fix (procedural matmul256 ≈ 40 lines) |
| Guard elision safety contract | only `bounds_proof`-proven guards removed; unproven byte-identical; no fast-math, no f64 reordering; trap kind+1 / exit-101/102 protocols unchanged | trap-free-by-proof is exact — a willreturn violation would be UB |
| Update in proofs | stays conservative in TrapCaps/FnAttrs this wave | deliberate; nested-product carried states fall back conservatively |
| Agent orchestration | fresh coder agents stalled 4× on read-heavy core work (agents 13/14/15/16 — exhaustive-match blast radius of a new `Operation`); orchestrator implements flow-ir/syntax/lower directly; well-scoped additive agent tasks (8/10/17/18) succeed | recorded pattern |
| `INLINE_MAX_BODY = 64` | untuned start value | perf harness owns tuning |
| Box A destroy | results.csv already local; box idle at reconcile; destroyed per "destroy when no longer needed" | $0.3633/hr stopped at age 7.2 h |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| flow-ir | 118 → 127 (last-use) → 134 (iota/fill) → 141 (bounds_proof) → **142 green** | queries + ADR-0029 IR |
| flow-backend-cuda | 119 → 129 → 144 → 151 → **153 green** (115 lib · 13 differential · 4 gate · 21 golden) | emitter wave + arenas + proofs |
| flow-backend-llvm | 18 → 19 → 21 → **22 green** (incl. 14 golden after reconcile re-pin) | by-ref + elision + attrs |
| flow-syntax / flow-lower / flow-rewrite / flow-interp | **200 / 151 / 59 / 61 green** | iota/fill surface; inline pass |
| llvm differential | 320 testgen × raw/rewritten × -O0/-O2, zero divergence (×2 waves; ~320 s each) | oracle equality under all elisions |
| cuda remote differential (box B, then box A) | 144 → **151 green, 640 compile-and-runs, zero divergences** | W3 shapes on hardware |
| FLOW_PERF smoke (box B) | vector_add `total ms=0.2040`, RUN_EXIT=0 | #19a instrumentation works on hardware |
| Local oracle pins | matmul4 `-275/3748`, matmul16 `1815/6944`, matmul64 `1047/2107` at -O0 AND -O2 | semantic safety of the whole wave |
| **Sweep (box A, RTX 4090, nvcc 12.4.131)** | all PASS → `docs/performance/matmul.md` | see below |
| flow-cuda cap kernel (FLOW_PERF compute, f64/f32) | **0.197/0.200 (N=16) · 0.193/0.195 (N=64) · 0.206/0.223 (N=128) · 0.205/0.210 ms (N=256) = 163 GFLOP/s** | startup-bound walls gone from the headline; 40× (N=64) / 321× (N=128) vs own loop form |
| flow-cuda cap wall | 313–322 ms at every N (startup ~270 ms) | process-wall rows labeled by kind |
| flow-llvm wall (znver2, N=64) | loop 9.17 ms (was 316), capture 29.17 ms (was 2,076) vs C++ 0.254 ms | ~200×/70× improvement; remaining gap = next waves |
| Baselines N=64 compute | naive CUDA 0.0032 · cuBLAS 0.0136 · chapel 0.0650 · cpp 0.2541 · rust 0.1911 · numpy 0.0106 ms | flow kernel 0.193 ms — between cpp/rust and numpy; cuBLAS peak 58.8 TF/s @ N=4096 |
| llvm golden_ll at session end | **2 FAILED** (stale insta snapshots post-S20c) | **fixed at reconcile: diffs verified as intended S20c shape (Div/Mod guard elision + `readonly nounwind willreturn`), snapshots accepted, 14/14 green** |

## 4. Live handoff state

| Type | Handle / location | State | Inspect / resume |
| --- | --- | --- | --- |
| branch | `main` | **uncommitted** (S14–S20 work; Sapir owns commits) | `git status` |
| vast.ai | box A `45490972` | **destroyed at reconcile** (idle, results local) | — |
| vast.ai | `45510479` (pytorch image, appeared 2026-07-22 ~02:40) | **unknown provenance — do NOT use or destroy** (S13/S19 precedent: Sapir's boxes are hands-off) | `vastai show instances` |
| vast.ai | `45485120`, `45481091` | Sapir's / pre-session — hands-off | — |
| local artifacts | `benches/matmul/results.csv` (76 rows) + `results-pre-s20.csv` (45 legacy rows) | landed | `docs/performance/matmul.md` |
| local artifacts | `benches/matmul/finish{,2,3}.sh` | sweep rescue scripts (box-side; reusable) | — |
| background tasks | none | all S20 agents/tasks accounted for | — |

## 5. Open items (the S21 agenda)

| Priority | Item | Next action | Done when |
| --- | --- | --- | --- |
| P0 | **Post-S20c box re-run** — FLOW_PERF re-measure of the now-trap-free kernel + re-sweep | fresh vast.ai 4090 (~$0.30; gotcha: fresh instances can be key-denied — wait 2–5 min then recycle); rsync `benches/matmul/` → `/root/bench`, run runner/finish3 + cuda remote differential over S20c shapes | new kernel ms in `docs/performance/matmul.md`; differential green on hardware |
| P0 | **ADR-0029 stage 2+ (the BL1 fix)** | cuda iota/fill kernels (5th Unsupported cell); generators → procedural v2 artifacts; then llvm N=128/256 legs become buildable | procedural matmul256 built + benched both backends |
| P0 | **ADR-0029 stage 2b: `widen`** | own round; IR-op decision to record (likely `Operation::Widen` — another exhaustive-match wave: interp + both backends + replay) | decided + implemented |
| P1 | **Arena v1.1 (#18a)** | last-use interference coloring on `death` intervals; loop-cone zones for the still-allocating carried class (map-in-loop per-iteration malloc traffic) | malloc counts drop in `arena_gates_plan_section_7` pins |
| P1 | **Region_plan step 2** | after `inline` (shipped); nested-loop boundary known: loop-bearing callee inlined into loop body yields multi-merge SCC the interp oracle can't evaluate — blocks matmul region acceptance | recorded in plan |
| P1 | **Tree-fold wave (ADR-0028)** | canonical-tree re-pin deferred from S20 | exact-op folds tree-reduced; f32/f64 pinned |
| P2 | llvm -O0 residual per-iteration aggregate copies (merge re-projection, back-route Pair, back-edge store) | loop-driver/arena territory | -O0 matmul64 closes on -O2 |
| P2 | `Update` proof elision; last-use testgen totality row; llvm `Zip` capture peek | as numbers direct | discharged suggestions |
| P2 | `time` builtin (language-level) | Sapir's call (S19) | decision recorded |
| P3 | P7 Verilog; chapel-gpu leg; `INLINE_MAX_BODY` tuning | standing | — |

## 6. Architecture / model changes

- **flow-ir gains two deduced queries** (FRAMEWORK §5 deduce-don't-store): `last_use_plan` and `bounds_proof` (+ S20c capture-range recursion) — both composed from `topo_order`/`loop_plan`, total on sealed fns, conservative on non-canonical shapes. Consumers: llvm/cuda guard elision, attrs/TrapCaps refinement, Update elision, (planned) arena coloring.
- **Core `Operation` set grows to 32 variants** (`Iota`, `Fill`; ADR-0029 stage 1) — exhaustive arms in interp, both backends, rewrite replay landed in the same change (§6.3).
- **CUDA memory model is now arena-based** (one `cudaMalloc` per zone; per-var `base + offset`; capacity a deduced morphism of the execution graph) with recorded v1.1 debt for loop-cone sites.
- **Emission has an options seam**: `emit_with_opts(EmitOpts{perf_timing})`; default byte-identical.
- ADRs 0028/0029 decided; `docs/notes/related-work.md` §8 Bend/HVM added (research leg of the mandate).

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `docs/STATUS.md` | S20 header + component rows (done in-session; close state finalized at reconcile) |
| `docs/components/backend-cuda/{STATUS,IMPLEMENTATION,DESIGN,suggestions}.md` + `plans/plan-smart-arenas.md` | S20 wave as-built; #2/#12/#13/#14/#17/#18/#19a discharged; #18a v1.1 scope; deviations recorded |
| `docs/components/backend-llvm/{STATUS,IMPLEMENTATION,suggestions}.md` | #2/#6/#7/#8/#9 discharged |
| `docs/components/ir/{STATUS,IMPLEMENTATION}.md` + `plans/plan-last-use.md` | last-use + bounds_proof shipped; 134→142 |
| `docs/components/{syntax,lower,interp,rewrite}/{STATUS,IMPLEMENTATION}.md` | iota/fill surface; inline pass |
| `docs/decisions/ADR-0028`, `ADR-0029` | decided; ADR-0029 Status → STAGE 1 SHIPPED with deviations (i)-(iii) |
| `docs/performance/{README,matmul,arena}.md` | S20 sweep matrix; arena v1.0 structural contract |
| `docs/suggestions.md`, `docs/IMPLEMENTATION.md` | roll-ups |
| `docs/notes/related-work.md` | §8 Bend/HVM |
| `docs/next-session.md` | rewritten for S21 at reconcile |
| `docs/sessions/2026-07-22-s20-optimization-marathon.md` | this log (written post-hoc from the export) |

## 8. Files changed

Code: `flow-ir` (algo.rs last_use/bounds_proof, graph.rs Iota/Fill, builder.rs, validate.rs, mermaid.rs, lib.rs exports), `flow-backend-cuda` (new arena.rs; func.rs/kernel.rs/loops.rs/module.rs/ty.rs/lib.rs; new examples/; tests/), `flow-backend-llvm` (func.rs/ty.rs/lib.rs/module.rs; new examples/; snapshots re-pinned ×2 at reconcile), `flow-rewrite` (new inline.rs, plan/replay/driver arms), `flow-lower` (diag/typing/emit iota-fill), `flow-syntax` (P0108 carve), `flow-interp` (eval arms). Benches: 34 emitted artifacts + runners + results.csv. Docs: §7. All uncommitted (Sapir owns commits).

**Gotchas carried into S21 (new this session, on top of the standing S08–S19 list):** BL1 wall — never queue large literal `.ll` clang builds (27 GB RSS / 1.5 h+ per 5.7 MB module); rsync excludes must be anchored (`--exclude '/benches/'`); `pkill -f runner.sh` over ssh kills your own session — use `"runner[.]sh"`; `vastai destroy instance` prompts — `echo y |`; fresh vast.ai instances may never receive the ssh key (recycle, don't fight); `label` is a Chapel reserved word; interp eval match is exhaustive — a new `Operation` needs interp + both backends + replay arms in the same change; `bounds_proof` is per-fn — call it on the site's owner fn; #13 credit rule `v != 0 && v != -1` (INT_MIN % -1 is UB too); `grep -c` exit-1-on-zero is harmless; `cargo test … | tail` hides cargo's exit code; `d_trap` excluded from arena accounting; `dbg_bounds.rs` was a temp debug file — must not reappear.

**Next `start` path:** read `sessions/2026-07-22-s20-optimization-marathon.md` (this log) → `docs/next-session.md` → post-S20c box re-run P0 → `cargo test --workspace`.
