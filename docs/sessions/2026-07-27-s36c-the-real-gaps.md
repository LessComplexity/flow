# 2026-07-27 — S36c: the real gaps, and two numbers that were wrong

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017). Continues
`2026-07-27-s36b-cross-machine-validation.md`, same day. Repository:
`github.com/LessComplexity/mapal`.

Driven by Sapir: *"what concepts keep us from beating the openBLAS implementation — same hardware,
so what is the difference? … what concepts are we missing to optimize the other types of tasks out
of the box and squeeze more from parallelism — saxpy looks almost 1:1 to matmul, zip is basically
map with extra steps and maps compose."*

## 0. Continuation brief

Current state: **two of S36b's published numbers were wrong and are corrected; the two largest
measured wins in the tree are now identified and neither is where we were looking.** The OpenBLAS
gap on identical hardware is **1.23×, not 1.85×** — everything published was Mapal's conformance
face against everyone else's fused one. The reduce "14× behind" was a semantics gap plus a
governor artifact; at one thread Mapal is the *fastest* of the three. The non-compute shapes are
not blocked by arithmetic intensity — they are blocked by two self-inflicted memory-layout facts
worth 2.3× and 3.1×, measured.
Next step: **P0 — emit the alias fact the backend already proves** (`plan-s37-scan-recurrence.md`
§7 names it; it needs its own plan).
Resume command/check: `benches/results-s36/LANGUAGES.md`, then this log §3.

## 1. Correction 1 — every published Mapal leg was the conformance face

`EmitOpts::contract` defaults `false` (`crates/backends/llvm/src/lib.rs:84`) and neither
`shapes_ab.sh` nor `ladder2_ab.sh` nor S36b's `compare_languages.sh` passes `--contract`. Verified
on the artifact: `objdump -d matmul1024_f32.o | grep -c vfmadd` = **0**, against 26 `vmulps` + 26
`vaddps`. Meanwhile every baseline gets FMA free — C++/Rust from `-ffp-contract=fast`, NumPy from
BLAS. **We published our bit-exact face against everyone else's fused one and called it a gap.**

Re-emitted with `--contract` (same object: 0 → **28** `vfmadd`) and re-measured under the same
protocol:

| machine | shape | conformance | FMA | best baseline | corrected gap |
| --- | --- | ---: | ---: | ---: | --- |
| i9 | matmul 1024 par | 3.2159 | **2.1464** | 1.7411 (OpenBLAS) | **1.23× behind** (was 1.85×) |
| i9 | matmul 1024 1t | 17.4225 | **14.8231** | 12.2196 | 1.21× behind (was 1.43×) |
| i9 | fir par | 0.0318 | **0.0286** | 0.2373 (C++ mt) | **8.3× ahead** |
| i9 | conv2d par | 0.0261 | **0.0357** | 0.1578 (Rust mt) | **4.4× ahead** |
| Mac | matmul 1024 par | 3.6504 | **2.2502** | 0.6869 (Accelerate) | 3.28× behind (was 5.31×) |

The Mac's remaining 3.3× is the AMX matrix unit, which is hardware we do not target — that column
is not a compiler comparison and should stop being read as one.

## 2. Correction 2 — the reduce row was a semantics gap, and a governor artifact

Two independent things, both measured this session on the box:

**The baselines compute a different function.** `ladder2_baseline.rs:run_reduce` splits into
`thread_width()` chunks and folds the partials — its f32 answer depends on the core count. NumPy's
`np.sum` is pairwise. Mapal's is a strict left fold. **At one thread, where all three compute the
same function, Mapal is the fastest: 0.3668 vs C++ 0.3821 vs Rust 0.3821.**

**The i9's small cells are frequency, not work.** `perf stat -e cpu_core/cycles/u,ref-cycles/u` over
35 runs across five affinity masks and five thread counts: the timed window costs a **constant
2.1 M core cycles** every time, `iter_ms × (cycles/ref-cycles)` = 1.05 ± 0.06 — wall time is just
`2.097e6 / f_core`. At a **fixed** 16 threads, widening only the affinity mask walks the median
0.397 → 0.399 → 0.401 → 0.525 → 1.436 → **2.006 ms** (masks `0,1` → `0-15`), because a pool spread
over more idle cores leaves them all at the ~1.07 GHz powersave floor. Sixteen threads crammed on
one core is the *fastest* configuration, because that core stays boosted.

Consequence for the protocol: **sub-5 ms cells on that box must be reported in cycles.** S36b's rule
("`MAPAL_PAR=1` cannot race, so any spread it shows is the machine") caught this for the 1t leg
only.

## 3. The two largest measured wins in the tree, and neither is algebra

Both are self-inflicted memory-layout facts; both were measured with A/B probes this session.

**(a) The `%Frame` struct destroys LLVM's alias analysis — worth 2.3× on saxpy 1t.**
`-Rpass-analysis=loop-vectorize` on the emitted saxpy says `loop not vectorized: unsafe dependent
memory operations` — twice. Isolated with a probe: an identical loop skeleton vectorises with two
`ptr` parameters and does *not* when the same arrays are fields of one frame struct, because LAA
cannot emit a runtime pointer check against one underlying object. **The backend already proves the
disjointness it is hiding** — `build_frame_layout` (`func.rs:1281`) gives every non-elided object
its own field and `update_aliases` records the only sharers exactly. There is no
memory-disjointness fact in the plan set (`TilePlan`/`EmissionPlan`/`LastUsePlan`/`BoundsProof`),
and zero grep hits for `!alias.scope`/`!noalias` anywhere in `crates/`. Measured: 0.5253 → **0.2262
ms**, identical output. Precedent already in the same file: clean functions get `noalias nocapture
readonly` on array *parameters* (`func.rs:1399`) — this is that move applied to frame fields. A
pragma does not substitute: `llvm.loop.parallel_accesses` on the probe left it scalar.

**(b) `iota` is an array instead of an index law — worth 3.1×.** `emit_iota` (`func.rs:6679`)
materialises a real `[N × i32]` and `emit_map` loads the element back out, so **every map over an
iota is an indirection** and LLVM reports `cannot identify array bounds`. `tile_iota_size`
(`algo.rs:997`) already proves the array is `[0..N)`, but it is private to `tile_site`. Replacing
the load with `trunc i64 %iv to i32` vectorises all four ladder loops. Measured: 0.3043 → **0.0972
ms**.

**Together: saxpy 1t 0.0972 vs C++ `-O3 -march=native` 0.0945 — parity — and par 0.0799 vs C++
0.1477.** The entire S35 "streaming kernels emit scalar loops" finding closes with **zero mapal-ir
changes** and nothing from the tile ladder.

## 4. A correction to S35's framing

S35's log says *"a plain `map` is not a site, and gets scalar code."* The same binary refutes it:
`_task4`, a plain map over a contiguous i32 array, is 4-wide NEON (`ldur q4`, `mla.4s`, `str q4`).
The shape-ladder doc's narrower wording — a plain map over a **zipped** array — is the correct one;
`docs/next-session.md` inherited the wrong one. **The right question is not which sites the ladder
recognises, it is which memory layouts the emitter forces on the machine.** Interleaved pair arrays
need `ld2/st2`; index arrays need indirect addressing. Both are ours.

## 5. Sapir's three intuitions, adjudicated

| Intuition | Verdict |
| --- | --- |
| "saxpy looks almost 1:1 to matmul" | **Right about the program, wrong about the compiler.** Same object in `path_plan` — both one 1048576-element `Split`. But what the ladder recognises is a **fold**: `tile_site` requires exactly one `Fold` that *is* the body output, a `Constant` seed, `Add`/`Mul` shape, `Index` into affine captures. saxpy fails four independently (captures 0, source is a `Zip` not an `Iota`, zero `Fold`s, zero `Index`es), and `TileSite` has no field that can represent "no reduction axis". There is no near-miss to widen — and it does not need one, per §3 |
| "zip is basically map with extra steps" | **Right in denotation, wrong about where the money is.** `Zip` is total, pure, token-free, so eliminating it cannot change the run's class — the rewrite is legal. `emit_zip` materialises the whole `[1048576 × {f32,f32}]`, 8 MB, so the timed window moves 20 MB where C++ moves 12; removing it is worth ~17%. But measured this session: **zip elimination alone does not unlock vectorisation** — the hand-written index form is still scalar because it now loads `i` out of the materialised iota. **Ordering is forced: (b) before zip elimination** |
| "maps compose" | **Already shipped, and it fires on nothing.** `analyze_map_fusion` (`crates/mapal-rewrite/src/functor_laws.rs:32`), body splice at `replay.rs:1323`, wired as `PassId::MapFusion` in the default order. Running the real `rewrite` over every ladder source: saxpy applies `[(ConstFold,2),(Dce,1)]`; transpose/gather/reduce apply `[]`. The law is right and implemented; the programs do not contain the pattern |

Zip elimination is **deforestation**, not naturality — `naturality.rs` law 1 moves maps *across* zip
and keeps the zip. Three blockers, none of them effects: the rewrite channels only rewrite a
morphism's *result*, never re-source a survivor's *operand* (the same limit that makes the `Update`
laws unimplementable); the rule introduces `Index`, which needs an in-bounds proof that lives in
mapal-ir and mapal-rewrite does not import it; and the crate has no cost model by design, while a
zipped array with ≥2 consumers makes this a recompute.

## 6. The OpenBLAS gap, decomposed

2·1024³ = 2.1475 GFLOP. Mapal 1t 123.3 GFLOP/s vs OpenBLAS 175.7 = **1.426× kernel**; scaling
5.418× vs 7.018× = **1.296×**; 1.426 × 1.296 = **1.847×** — the measured conformance-face par gap to
four decimals. **Kernel 58% / scaling 42%.** Of the kernel term, the FMA face is 1.185× of it
(§1 collapses that part), leaving ~1.19× of genuine micro-kernel deficit.

Named concepts behind that residue, with status:

| Concept | Status |
| --- | --- |
| Register-pressure accounting in **target** registers; (MR,NR) as a 2-D search | **never attempted.** `tile_i = vec_regs / (2·acc_vecs_per_row)` under `GENERIC` (a NEON model: 16 vec_bytes, 32 regs) yields 4×16, confirmed in the AVX2 disassembly — 8 ymm accumulators, no spills. The canonical 6×16 is **unreachable**: `tile_i` is a power-of-two quotient and the `2·` is a "spend at most half the vector file" policy while the standard kernel spends 75%. S26's TI=8 rejection swept TI with NR pinned, on NEON — a verdict about 8×16, not about the (MR,NR) product space |
| A-packing on the shipping path, and MC (the L2-resident A block) | **never attempted.** B is packed but as the whole K×N at once, not a kc×nc panel; **A is never packed** — 4 `vbroadcastss` streams 4 KB apart per micro-kernel. `MC` has no symbol, constant or flag anywhere; `profile.rs` has exactly one cache field (`l2_bytes`) |
| The BLAS 5-loop order (`kc_nest`) | **tried three times, lost three times** — S29 3.0× loss, S30 fixed the codegen and it still lost with a *growing* deficit (+5%@1024 → +14%@4096), S31 lost at the derived kc too. For a retry to differ it must ship **with MC** (all three added k-blocking while leaving `ic` unblocked) and be measured on the i9 (all three verdicts are Apple silicon) |
| A profile measured on the target | **never attempted.** `resolve()` returns `None` rather than falling back, so the i9 ran on `GENERIC`; the only x86 profile, `ZEN3`, is labelled in-source "UNTESTED — read off documentation, never measured" |
| Arithmetic-intensity / bandwidth cost model | **never attempted.** `TargetProfile` knows vector bytes, register count, L2 and a stack ceiling — no bytes-per-flop, no bandwidth, no L1, no L3. `tile_plan` is a pattern matcher with no cost model at all |

A caution the record needs: **FMA:load ratios published from NEON do not transfer.** S30's "FMA:mem
8.00" and S31's "0.80 → 1.20" are ARM, where `fmla v,v,v[i]` makes the A splat free. The same 4×16
on AVX2 needs 8 `vbroadcastss` per 2-k and lands at MAC:load 1.33, below the 6×16 reference's 1.5.

## 7. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| Publish the FMA face beside the conformance face | **kept** | Comparing our bit-exact face to everyone else's fused one is not a gap, it is a category error |
| Keep the conformance face as the default | **kept** | It is the bit-exact contract; `--contract` is a stated product face (ADR-0032). What changes is that the harness must say which one it measured |
| Report the reduce row as semantics | **kept** | Rust's answer depends on `thread_width()`; ours does not. That is a difference in what is computed |
| Retry `kc_nest` | **still rejected as-is** | Three measured losses. Only a retry that ships MC *and* runs on the i9 is a new experiment |
| Fix the fold before the layout facts | **rejected** | The fold is worth ~10% of one cell; the alias fact and the iota law are worth 2.3× and 3.1× across four shapes |
| A `Scan` primitive | **planned, not started** | `components/ir/plans/plan-s37-scan-recurrence.md` |

## 8. Live handoff state

| Type | Handle | State | Inspect |
| --- | --- | --- | --- |
| branch | `main` @ pushed | clean | `git status --short` |
| CI | last run on the S36b corrections | **success** (27m40s) | `gh run list --limit 3` |
| gh auth | `LessComplexity` active | switched in S36b | `gh auth status` |
| perf box | `<perf-box>` i9-14900F | idle; governor `powersave`, no passwordless sudo | `ssh … uptime` |
| box dirs | `~/s36bench` (conf), `~/s36bench_fma` (FMA), `~/s36bench_pre` (pre-fix), `~/mapalbench` (baseline sources) | left in place | `du -sh ~/s36bench*` |
| artifacts | `target/tmp/{i9,i9pre,fma}` | conformance, pre-fix and FMA binary sets | `ls target/tmp/fma` |

## 9. Open items

| Priority | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| **P0** | Emit the alias fact the backend already proves | §3(a) | Own plan first: a disjointness fact in the plan set → `!alias.scope`/`!noalias` on frame-field accesses | saxpy 1t ≤ 0.25 ms, gate green, output byte-identical |
| **P0** | `iota` as an index law, not an array | §3(b) | Own plan: `trunc i64 %iv to i32` at the use site when the source is a provable iota | saxpy 1t ≤ 0.10 ms; the four ladder loops vectorise |
| **P0** | Republish every `par` table | S36b | Now also: say which FACE each number is | no pre-S36 `par` cell and no unlabelled face is quoted |
| P1 | Harness must state the face | §1 | `shapes_ab.sh`/`ladder2_ab.sh`/`compare_languages.sh` emit both faces or name the one they emit | every published table says conf or FMA |
| P1 | i9 cells under 5 ms must be cycles | §2 | Add `perf stat` cycles to the box driver | the box driver reports cycles beside ms |
| P1 | ADR-0028 step 1 (integer tree reduce) | `plan-s37-scan-recurrence.md` §6 | Recognise `combine`/`unit` for the exact-op set | an i32 reduce splits, canonical under `MAPAL_PAR` |
| P1 | `reduce_1048576.mapal` cannot fire the path it claims | §5 | It says it tests ADR-0028 tree reduction and is f32, which D4 excludes; its data is bit-exact under any parenthesisation. Add an i32 twin and an f32 leg with real dynamic range | both twins exist |
| P2 | A target profile measured on the i9 | §6 | `ZEN3` is documentation, `resolve()` has no fallback | a raptorlake profile with measured constants |
| P2 | Zip elimination (deforestation) | §5 | After the iota law; needs an operand-rewriting channel and a bounds proof mapal-rewrite can see | saxpy emits no pair array |
| P2 | ADR-0038 empty-param calls; halve the differential cross product; `ladder2_ab.sh` has no correctness check | S35/S36b | — | — |

## 10. Method notes earned

1. **Check which face you measured before publishing a gap.** Two commands —
   `grep -c "fmul contract"` on the `.ll` and `objdump | grep -c vfmadd` on the `.o` — would have
   caught a 1.5× error before it reached a table.
2. **A cross-language cell must compare functions, not just times.** Two of the three reduce
   baselines reassociate; one of them is machine-dependent. "14× slower" was three different
   functions in one column.
3. **On a powersave box, report cycles.** One fixed 2.1 M-cycle kernel reads anywhere from 0.38 to
   2.01 ms depending on how widely the pool spread — at a fixed thread count.
4. **Ask what the compiler *hides* from LLVM, not only what it emits.** The two largest wins are
   facts the backend already proves and then discards: object disjointness and `iota`'s range.
5. **A shipped optimisation with no customers looks identical to a missing one from the outside.**
   Map fusion is implemented, wired, and fires on nothing in the ladder — only running the real
   rewrite over the real sources shows the difference.

## 11. Files changed

`benches/results-s36/{make_languages.py,LANGUAGES.md,lang_mac_fma.log,lang_i9_fma.log}` (the FMA
face and the two corrections), `docs/components/ir/plans/plan-s37-scan-recurrence.md` (new),
this log, `docs/STATUS.md`, `docs/next-session.md`.
