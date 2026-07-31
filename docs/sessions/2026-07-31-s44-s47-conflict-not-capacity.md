# 2026-07-31 — S44–S47: conflict, not capacity

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Driven by Sapir.
Continues `2026-07-31-s43-the-pack-was-serial.md`. Four sessions in one sitting, logged together
because S45–S47 are each a direct consequence of the one before.
Governing record: **`docs/performance/s44-conflict-not-capacity.md`**.

## 0. Continuation brief

Current state: **the compiler now detects the machine by default and deduces a cache-conflict fix
that turned transpose from the slowest shape into a win on two different processors.** S44 was asked
to build "the L1 micro-panel"; it measured that L1 *capacity* was never the variable — the winning
block is 4 KB, 1/32 of L1D — and that **set-index conflict** was, a variable S43 had deliberately
engineered out of its instruments (`tlbreach.c` used odd strides "to avoid the classic trap"). S45
deleted the hand-typed flag and made the decision deduce itself. S46 made detection the default and
caught that detection read caches but not the vector ISA. S47 tried to derive the optimal block size,
**failed and said so**, and found a live 1.75× regression while failing.

Everything is merged and pushed. `main` @ `0f40ce0`, clean, gate **1047 passed / 0 failed**.

Next step: **the single-thread matmul gap** — numpy is still ~2× ahead there, unchanged from S43.

Resume command/check: `cargo test --workspace --release`, then read
`docs/performance/s44-conflict-not-capacity.md`.

## 1. Work completed

**S44 — the mechanism.** Transpose moved 8 MB in 1.12 ms = **7.5 GB/s** on a machine whose DRAM
floor is 95. Not bandwidth. A 1024-wide f32 row is 4096 B; on a 128-set L1D with 128 B lines that
advances 32 sets per read, so the walk lands on **4 sets** — 32 usable lines against 1024 needed.
Built the rung as a **permutation of the loop counter, not a blocked nest**: `perm` is a bijection of
`[0,n)`, so slices partition the counter and their images partition the outputs — every element
visited once, values bit-identical, and `%lo`/`%hi` need no head/tail arms. 60 lines.

**S45 — the flag deleted.** `--move-panel=W:B` was a machine constant typed by a human, applied
globally to every eligible map. Replaced by a deduction: `mapal-ir` gained a **fold-less move-site
record** (`tile_site` hard-requires a fold, so a transpose was never recognised); `TargetProfile`
gained cache line size, associativity and `l2_cores`; the backend computes the decision.

**S46 — detect by default.** `EmitOpts::default().target` went `"generic"` → `"native"`. Then the i9
run caught that `detect()` read caches but left `vec_bytes`/`vec_regs` at NEON's values on an AVX2
part — **+8.8% on conv2d** before it was read.

**S47 — the block size, and a failure worth having.** Swept every legal divisor at 4 sides × 2 thread
counts × 2 machines. **`B` is not derivable**, and the leading hypothesis was refuted. Found and
fixed a 1.75× regression at width 1536.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| "L1 micro-panel" as capacity blocking | **abandoned** | the winning block is 1/32 of L1D; capacity was never the variable |
| a blocked traversal nest | **rejected** | a counter permutation is a bijection, so slices still partition the outputs — 60 lines instead of 300, no head/tail arms |
| padding the array stride instead | **rejected** | the compiler owns base and extent; **the user owns stride**, so padding is a reindexing that belongs in `mapal-ir`, not the allocator |
| `--move-panel=W:B` as a shipped flag | **deleted** | one hand-typed width applied to every eligible map; measurably harmful where pressure is low |
| default target `generic` | **replaced by `native`** | `generic` carries `l1d: None`, so a default build declined every cache-aware rung |
| `vec_bytes`/`vec_regs` "arrive with the target features" | **overturned** | true while `native` was opt-in; as the default it tiled an AVX2 box to NEON |
| deriving `B` from an L1 budget | **kept, ceiling documented** | four replacements priced, all regress the M4 4–15% |
| memory-level parallelism as the resource setting the optimum | **REFUTED** | at the i9 optimum `fb_full` is 3.0% of cycles and MLP is the *lowest* of any arm |
| firing at non-power-of-two widths | **rejected** | measured 1.75× loss; the permutation's div cost is derivable, its benefit is not |
| a per-profile table of block sizes | **rejected** | the flag again with extra steps |

## 3. Tests, checks, benchmarks

| Check | Result |
| --- | --- |
| transpose 1024, M4, 1t | 0.8996 → **0.5700 ms**, 1.578× (medians; tails overlap) |
| …threaded | 0.2890 → **0.1450 ms**, **1.993×, disjoint** |
| transpose 1024, i9, 1t | 2.4154 → **1.0862 ms**, **−55%, disjoint**; 9.427 → 3.313 Mcyc |
| deduction fires/declines vs measurement | **6 of 6 correct** on two machines, including the i9-512 case that loses |
| `B` derived vs optimum | M4 **exact** at 1024 and 2048; i9 **1.15–1.27× short** at every cell |
| width 1536 after the fix | 0.572× → **1.007×** (1t), 0.708× → **0.986×** (par) |
| pressure sweep, M4 | 0.5 → 1.00× · 2 → 2.00× · 8 → 2.09× · 32 → 2.71× · 128 → 3.19× |
| `l2_miss` ordering, i9, 8 arms | **no inversion** — L2 residency orders the arms, MLP does not |
| non-pow2 instruction cost, i9 | 185.4 M at B=16/32 vs 223.2 M at B=12/24 |
| byte-identity, `generic` | **0 of 159 moved** |
| …`--target=native` | **exactly 6** — transpose_1024/2048, all three faces |
| …rule-23 injection | **67 cells move** — the gate is not blind |
| ladder, M4 threaded | saxpy **0.085** · reduce 0.564 · transpose **0.162** · gather 0.167 |
| ladder, i9 threaded | fir **0.187** · conv2d **0.084** · saxpy **0.130** · transpose **0.176** · gather **0.203** |
| `cargo test --workspace --release` | **1047 passed / 0 failed** (1037 → 1047) |
| `cargo fmt --all --check` | clean |

## 4. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` @ `0f40ce0` | **pushed, in sync, clean** — only `oainotes.md` untracked | `git status -sb` | — |
| worktree ×9 | `.claude/worktrees/agent-*` | **all merged or probe-only**; every source copied to `main` | `git worktree list` | discardable |
| worktree ×3 | `…-Personal-**Flow**/…` | prunable, pre-rename paths | `git worktree list` | `git worktree prune` |
| machine | Arch box `100.81.226.103` | up, i9-14900F, **has cargo/rustc 1.90** (found S46 — emission can happen on the box, which detection requires) | `ssh … nproc` | owned box |
| artifact | box `~/mapal-s42,44,45,46,47` | **1.02 GB total** (107 + 204 + 174 + 234 + 304 MB) | `ssh … 'du -sh ~/mapal-s4*'` | **delete — nothing depends on them** |
| file | `oainotes.md` | untracked, deliberately uncommitted | — | Sapir's call |

**Nothing is running.** No background job, no server, no port. Measurement mutex free.

## 5. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | **single-thread matmul is ~2× behind numpy** | `s43-…` §4b/§4e | needs a reuse-structure change; every blocking knob is swept and refuted | 1t GF/s moves off ~800 |
| P1 | `B` leaves 1.15–1.27× on the i9 | `s47` §4, `move_block` doc | not derivable from readable facts — revisit only if the benefit becomes predictable | derived == optimum, or closed as unreachable |
| P1 | width 1536 declines rather than winning | `move_block` doc | the i9 forgoes 5.7%/4.2% there; needs the same unreadable quantity | a rule that fires safely at non-pow2 widths |
| P1 | decide `nc` blocking's fate | worktree `a03f9b2318` | built, swept, gates green, ships OFF | merged as a documented lever, or discarded |
| P1 | `examples/vector.mapal` does not parse | `emit_sweep_ab.sh` | 3 of 159 gate cells have always failed | it parses, or leaves the sweep |
| P1 | delete or justify `kc_nest` | `lib.rs::EmitOpts` | unchanged since S42 | gone, or has a written reason |
| P1 | executing SME value check in `cargo test` | `benches/sme/README.md` | unchanged since S42 | the suite runs an SME binary |
| P2 | box scratch, **1.02 GB** across five dirs | §4 | delete | gone |
| P2 | 9 agent worktrees + 3 pre-rename | §4 | `git worktree prune` after removing the agent ones | only `main` listed |
| P2 | f16/bf16 rung (2× MAC density) | S42 §5e | plan first | `svmopa_za32_f16_m` emitted |

## 6. Architecture / model changes

**`mapal-ir` gained one record, and it is geometry only.** `algo.rs:MoveSite`/`MovePlan`/`move_plan`/
`move_site`/`move_affine` — a **fold-less move site**, a map whose read address is affine in
`(t ÷ C, t % C)`. `tile_site` hard-requires a fold in the map body, so a transpose (a pure
permutation, no reduction) was invisible to every existing record; this is a new arm on the family
`tile_split`/`TileAffine` already implements one level up. **ADR-0032 verified by grep**: the whole
`mapal-ir` diff has one machine-vocabulary hit and it is the sentence stating the boundary.
`MoveSite` carries width, rows, cq, cr, elem, len.

**`TargetProfile` gained the facts a cache decision needs**: `L1d` (line, sets, `ways()`),
`l2_cores`/`l2_per_core`, and Linux `detect()`. `l2_cores` fixed a **live defect** — `l2_bytes` was
documented per-core while `apple-m` recorded a *shared* 16 MB across 5 cores, and that 4.8× is the
whole answer at M4-1024.

**The `Trn`/`Loc` reading.** The transpose's cost was never in the transformation; it was in the
*order* its placement visited memory. Two placements of one `Trn` over one `Dat` differ only in
traversal order and differ 2.7× in time — which is FRAMEWORK §4.2's "the two may be different code
realising the same signature", with the cache as the `Loc` that distinguishes them.

## 7. Docs reconciled

| Doc | Change |
| --- | --- |
| `performance/s44-conflict-not-capacity.md` | **new** — the mechanism, the predictor and its two refutations, measurement rule 24 |
| `components/backend-llvm/plans/plan-s44-l1-micro-panel.md` | **new** — written pre-build |
| `components/backend-llvm/plans/plan-s45-deduced-move-panel.md` | **new** — written pre-build, §10 reconciles |
| `benches/results-s4{4,5,6,7}/**` | **new** — every number appended as it landed |
| `benches/emit_sweep_ab.sh` | **third** silent-pass path closed: preflights that the binary accepts its flags |
| `README.md` | Results section cut to tables + links; shape rows re-measured; detect-by-default |
| `docs/next-session.md` | rewritten for S48 |
| this log | new |

## 8. Files changed

`crates/mapal-ir/src/{algo,lib}.rs` · `crates/backends/llvm/src/{profile,lib}.rs` ·
`crates/backends/llvm/src/func/{bulk,core,drive,mod,window}.rs` ·
`crates/backends/llvm/{examples/emit.rs,tests/{move_panel,golden_ll,differential}.rs}` ·
`benches/shapes/{tblock.c,stride_ab.sh,movepanel_ab.sh,transpose_vs_baselines.sh,i9_ladder.sh,blocksweep_i9.sh,transpose_{512,1536,2048}.mapal,conv2d_s{1024,1026}.mapal}` ·
`benches/emit_sweep_ab.sh` · docs as §7.

## 9. Method notes earned

> **24. Classify what an optimization removes, and its thread-count behaviour follows.** A **serial
> fraction** (the parallel B pack) does nothing at 1 thread and 1.381× threaded. A **shared
> bottleneck** (`kc`, residency, `nc`) is big at 1 thread and gone threaded. A **per-core resource**
> (this rung) *grows* with cores: 1.578× → 1.993×. Predict the shape before measuring it.
> **Note (S46): the third case holds only while the per-core resource still binds** — on the i9 the
> same rung shrank 2.646× → 1.547× across 32 cores because the fixed arm hits a shared ceiling the
> slow arm never reaches.

- **An instrument that excludes a variable cannot measure it.** `tlbreach.c` used odd strides *on
  purpose* to avoid conflict misses. That was correct for its question and it hid the answer to the
  next one for a whole session. When a design says "to avoid X", X is a hypothesis nobody has priced.
- **Kill your own predictor.** v1 (count reachable sets) predicted a conv2d cliff at width 1024;
  measured 1.018%, refuted. v2 (`lines_live / (sets × ways)`) survived four predictive probes and
  then failed *quantitatively* — it predicts sign and ordering, never magnitude, saturating by
  pressure ≈ 2. Both failures came from tests designed to break it.
- **A rule fitted to its own counterexample is retrodiction.** Both predictors explained the data
  they were derived from. Only the pre-registered probes (odd side, conv2d-1024, side 128, side 544)
  made either of them worth anything.
- **Refusing to derive is a result.** `B` is not derivable: the i9 wants a block 4–8× larger while
  *every* readable fact is larger on the M4. Documenting that ceiling beat shipping a per-machine
  table, which is the deleted flag wearing a derivation's clothes.
- **A default that loses is worse than a default that under-performs.** Width 1536 fired and lost
  1.75×; forgoing the i9's 5.7% there to stop giving away 43% is the right trade, and both numbers
  belong where the rule lives.
- **Making something the default changes what its documentation means.** `vec_bytes` was documented
  as arriving with the target features, which was true while `native` was opt-in and false the moment
  it became the default.
