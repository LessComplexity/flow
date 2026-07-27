# 2026-07-27 — S38: trap order is source order, and the trap that ate two lines

Orchestrator: Claude (category-architect skill). Immutable log (ADR-0017).
Driven by Sapir. Continues `2026-07-27-s37-elem-plan-and-the-dead-array.md` + `-s37b-*.md`.

## 0. Continuation brief

Current state: **plan-s38 SHIPPED. The gate is GREEN for the first time since S33** — 981 passed,
0 failed, LLVM differential 37/37 in 403.93 s at `-O0`/`-O2`, `cargo fmt` clean, zero pending
snapshots. `open_inline` passes; its pinned seed is retained.
Next step: **S39 — the GPU leg.** Sapir's call, taken this session on evidence: go to **LLVM NVPTX**
rather than growing the CUDA C emitter (§6).
Resume command/check: `bash -c 'cd /Volumes/LessComplex/Personal/Flow && cargo test --workspace --release'`

## 1. Work completed

**The fix (4a + 4b).** `crates/mapal-ir/src/algo.rs` — `topo_order`'s ready worklist became a
`BinaryHeap<Reverse<(loc.start, loc.end, insertion index)>>` instead of a FIFO `Vec` + cursor.
`crates/mapal-rewrite/tests/testgen/mod.rs` — `const L: SourceLoc = {0,0}` at 122 sites became a
per-`build` monotonic counter; without it every generated statement claims position 0 and the new key
degenerates straight back to insertion order, so the corpus would test nothing.

**Why A and not B** (unchanged from the plan, restated because it is the load-bearing reason): three
sites must agree on which trap is reported — the oracle (`interp/src/eval.rs:101` walks
`topo_order`), sequential LLVM emission, and the parallel runtime (`mapal_par_trap` CAS-mins on the
topo index). They agree today *because all three derive from `topo_order`*. Changing only the
interpreter creates a second key that must match every backend forever.

**Two test-only changes.** `backends/cuda/src/kernel.rs` — the arena assertion stopped pinning
`arena0 + 512ULL` (an ordering fact: `arena_plan` walks `topo_order` and assigns 256 B slots
first-wins, so offset order *is* topo order) and gained a pin on the element-offset copy instead;
disjointness and alignment are pinned where they belong, on the plan, by
`arena::tests::offsets_are_disjoint_aligned_and_topo_ordered`, which compares a **sorted** offset vec
and is therefore permutation-proof by construction. `backends/llvm/tests/differential.rs` — new test,
§4.

## 2. Decisions

| Decision | Verdict | Why |
| --- | --- | --- |
| A′ ("make lowering create objects in source order") | **REFUTED by measurement** | 62.2% of raw objects move (484/778, 14 examples, 36 fns); only 9/36 fns already ordered, all 3–9 objects. Systematic: `loc` is the *operator token*, not the sub-expression extent. Costs more than A, not less. Do not re-price. |
| Approach A | **kept, shipped** | one order in the system; oracle, emission and runtime CAS key all inherit it |
| `example_calc`'s behaviour change | **signed off (Sapir)** | prints preceding a trap now happen; the new order is source order, which is the point of the plan |
| Chase the saxpy +5.3% | **no (Sapir)** | expected from reordering; mechanism not isolated; the repo already has two P0s refuted after chasing unproven mechanisms |
| `%Frame` layout as the perf mechanism | **withdrawn — over-claimed** | correlates, but task-interleaving fits conv2d's 1t-faster/par-slower sign flip better. Not S36c's refuted alias-barrier claim (vector counts byte-identical), but not promoted either. |
| GPU: NVPTX vs CUDA C | **NVPTX (Sapir)** | §6 — the audit's blockers were unverified and failed the test |
| `INSTA_UPDATE=always` for the snapshot sweep | kept | multi-example tests abort at the first mismatch (1 snapshot per full gate run; `golden_examples` alone would need 8 more rounds). Old content is in `git HEAD`, nothing committed unreviewed, agents diff `git show HEAD:<path>`. |

## 3. Tests, checks, benchmarks

| Check | Result | What it proved |
| --- | --- | --- |
| `cargo test -p mapal-rewrite --test inline` | 15/15 | the pinned S37 counterexample passes; `Trapped(IndexOob)` survives inlining |
| `cargo test --workspace --release` (clean, no INSTA_UPDATE) | **981 passed, 0 failed** | gate green |
| LLVM differential | **37/37 in 403.93 s** | ran rather than skipped; every value byte-identical at `-O0` and `-O2` |
| `cargo fmt --check` | clean | (needed one `cargo fmt` after the 122-site substitution) |
| A′ pricing probe (throwaway, deleted) | 484/778 objects move | §2 |
| Effect-sequence sweep, 38 snapshots | 34 unchanged · 3 unobservable · 1 signed off | §5 |
| Tiling/guard/attribute/trap-count sweep, 38 snapshots | **0 of 38 changed** | no site became tiled or untiled; no guard, attribute or trap appeared or vanished |
| i9 PRE/POST A/B | 3 passes × 101 alternating | §7 |
| NVPTX capability probe | 4/4 | §6 |

## 4. The bug is wider than the plan described

plan-s38 §1 is about trap *kind*. The same root cause also let a trap **swallow output written before
it in source order**: `mapal_par_run_pinned` executes its task body **synchronously on the host
thread** (`mapal-rt/src/lib.rs:1080`), and the old order hoisted those runs ahead of the prints.

```
fn f(a: i32, b: i32) -> i32 { a / b -> ret; }
fn main() { 111 -> println; 222 -> println; (1, 0) -> f -> r; r -> println; }

PRE  (insertion-order tie-break):  mapal trap: div_zero                    exit=101
POST (source-position tie-break):  111 / 222 / mapal trap: div_zero        exit=101
```

**Both exit 101, which is exactly why the 1,280-run sweep could not see it.**
`differential.rs:216` maps `Outcome::Trapped(_)` to `(None, 101)` and the stdout assert is guarded by
`if let Some(want)` — trapping runs compare the exit code and discard stdout, at both opt levels.
That whole class had zero coverage. Now pinned by `differential_trap_preserves_preceding_output`,
**verified as a real negative control** (PRE fails it, POST passes).

**The test must pin a literal, and the reason is structural.** `interp::run` derives `output` from the
IoToken's accumulated log and only on `Done` (`mapal-interp/src/lib.rs:55`). Interpreted output is a
*value carried by the token*; on a trap the token never reaches the Return, so the log dies with the
aborted computation and `rr.output == ""`. Compiled output is a real side effect and survives. **The
two I/O models diverge exactly on the trap path**, so `expect_native`'s `None` is *forced*, not lazy —
and the first version of this test, which compared against the oracle, failed on its first run.

## 5. Golden churn: 38 snapshots, all adjudicated

**22 of the 38 were adjudicated by subagent panels** (round 1: 18 snapshots, 26 agents; round 2: 4
snapshots, 9 agents — each finding independently attacked by a refuter). **A third round covering the
remaining 16 was launched and did not complete** — it was stopped before producing a verdict, and its
partial agent output was not used. Those 16 are covered instead by the two mechanical sweeps below,
both run by the orchestrator over all 38: the effect-sequence comparison and the
tiling/guard/attribute/trap-count comparison, which found **0 of 38** changed. That is weaker evidence
than a panel and is recorded as such — **if any of the 16 is later found to be more than renumbering,
this is where the gap was.** The 16: `golden_ll__{example_sepia,example_fir,example_vector_add,
example_zip_demo,example_sum_to_n,example_pipeline,tile_nest_shape_f64}`,
`golden_cu__{example_sepia,example_sum_to_n,example_vector_add,example_zip_demo}`,
`golden__{matmul4_loop,sepia,vector_add,zip_demo,pipeline}_mermaid`. Re-run with
`Workflow({scriptPath: …/verify-s38-golden-round3-wf_1f377034-f4d.js, resumeFromRunId: "wf_1f377034-f4d"})`.

**37 ordering-only, 1 intended behaviour change.**

**The observable-effect axis is closed by proof, not inspection.** The ordered sequence of
print/trap/`run_pinned`/`par_check` calls is unchanged in 34. In 3 —
`capture_one_kernel_matmul`, `example_vector_add`, `example_zip_demo` — a PRINT crosses a
`mapal_par_check`, which is **provably a no-op there**: those modules contain zero `mapal_par_trap`
call sites, and the only production writer of `run.trap` is `mapal_par_trap` (init `0` at
`mapal-rt/src/lib.rs:586`, CAS at `:1000-1006`; the accesses at `:1271`/`:1278` are inside
`#[cfg(test)] mod tests`, which starts at `:1111`), so `check_trap`'s `if trap != 0` can never fire.
The shape of those reorderings is itself confirmation the fix works — batched checks became strictly
alternating, each print preceded by exactly the wait it needs:

```
example_vector_add HEAD:  check PRINT check check check PRINT check PRINT PRINT check PRINT PRINT
example_vector_add NEW :  check PRINT check PRINT check PRINT check PRINT check PRINT check PRINT
```

The 4th, `example_calc` (both backends), is the real one — §2, signed off.

## 6. The GPU decision: NVPTX, taken on a probe that refuted the audit

An 8-agent audit (4 parallel surveys + 3 independent design lenses + adjudication) recommended
**keeping the CUDA C emitter**. Sapir challenged it. Its central technical claims were flagged *by the
audit itself* as unverified. They were tested and **failed**:

| Question | Probe result (LLVM 22.1.8, `llc -march=nvptx64 -mcpu=sm_80`) |
| --- | --- |
| shared memory | `addrspace(3)` global → `.shared .align 4 .b8 tileA[1024]` + `st.shared.b32` ✅ |
| kernel marking | `ptx_kernel` calling convention → `.visible .entry tile_kernel(` ✅ |
| **tensor cores** | `llvm.nvvm.mma.m16n8k16.row.col.f32.f32` → **`mma.sync.aligned.m16n8k16.row.col.f32.f16.f16.f32`** ✅ — **804** mma/wmma intrinsics available, incl. MXFP block-scale (`e2m1`/`e4m3`/`e5m2`/`ue8m0`) |
| `<16 x float>` accumulator | 16 scalar `fma.rn.f32`, no `.v4` — the *correct* GPU lowering (per-thread register blocking, not SIMD lanes) |

Also corrected this session: the orchestrator's own framing that two emitters over one IR violate
FRAMEWORK §3/§5 was **wrong**. §4.2 says one `Trn` at two `Loc`s has two `TrnLoc` rows and "the two
**may be different code** … That is the strategy shape (§5)", and §5 lists backends among the
sanctioned pluggable variants. NVPTX violates nothing; the question was only sequencing.

**What survives from the audit and is worth keeping:**
- NVPTX does **not** build the smem rung for you — `grep '__shared__|__syncthreads|dim3'
  crates/backends/cuda/src/` returns **zero**, so the rung is greenfield in either language.
- Two real §5 duplications, both language-independent and untouched by any port: ~28 lines of the
  type-erasure remap are **character-identical** between `llvm/src/ty.rs` and `cuda/src/ty.rs` (the
  CUDA copy's own comment says "llvm rule, verbatim"), and the mapal-rt ABI is hand-declared twice
  (a `declare` block vs a 56-line C++ prelude).
- One genuine ADR-0032 leak: `llvm/src/func.rs:342 packing_site` is a CPU packed-**format** decision
  wearing a legality predicate's name (its own comment admits it). Rename `packed_layout_admits`,
  move next to `packed_type`/`packed_buffer`. Zero behaviour change.
- CUDA C **cannot** express ADR-0032 D1's per-region precision lattice — `-fmad` is one TU-wide nvcc
  flag. LLVM IR does it per instruction. That gap is why NVPTX was always going to be forced.

Sapir's framing, recorded: the NVPTX path may need graph facts handled differently, and that is the
point — it is ADR-0033 D4(b) ("which machine fact does the record not carry") answered rather than
asserted. First concrete instance already found: `<TJ x elem>` means SIMD lanes on CPU and per-thread
registers on GPU — one record field, two readings.

## 7. Perf: the pre-registration is refuted

i9-14900F, governor `performance` (5.5 GHz), pinned `taskset -c 0-15` (8 P-cores), PRE =
`main@d3ca82c`, both legs emitted through one pipeline in one pass, **alternated run-by-run**,
3 passes × 101 runs. **Values byte-identical on all 7 shapes in every pass.**

Medians (saxpy's and gather's *minima* are unusable — saxpy's min swung 0.40–0.63 ms between passes
while its median held to 4 significant figures):

| shape | conf p1 / p2 | FMA | par (conf p1/p2, FMA) |
| --- | --- | --- | --- |
| saxpy_1048576 | **+5.2% / +5.3%** | **+5.3%** | +6.4 / +1.1 / +1.5 |
| conv2d_1024 | −4.6% / −2.8% | **−8.2%** | **+4.1 / +7.2 / +3.0** |
| mm1024 | **+2.6% / +2.6%** | **+0.15%** | +0.9 / −0.1 / −1.4 |
| fir · reduce · transpose · gather | flat, inside ±1% | flat | flat |

**Emission order is performance-relevant.** Three findings worth keeping:
1. **saxpy 1t +5.3%, replicated three times** — the third pass is a byte-identical rebuild, so it
   doubles as the noise-floor control (reduce reproduced at 0.00%, transpose +0.1%, gather −0.9%).
2. **mm1024's regression is face-dependent** — +2.6% twice on conformance, flat with FMA. Running the
   FMA leg (Sapir's catch) prevented publishing a regression that only exists on one face.
3. **`--contract` is a no-op on 4 of 7 ladder shapes.** saxpy, reduce, transpose and gather emit
   **byte-identical IR in both faces**, because contraction flags are applied only in tile kernels and
   those four are not tile sites (S35). The README's two-face columns are, for those rows, one binary
   measured twice.

Mechanism **not isolated, deliberately**. Vector-instruction counts are byte-identical pre/post
(ymm/zmm 199/199 saxpy, 295/295 mm1024, 583/583 conv2d) so it is scheduling, not degraded codegen.
Two candidates remain live and were not separated: `%Frame` member order (which derives from graph
object order, which `replay.rs:1029` derives from `topo_order` — saxpy's two 4 MB arrays moved from
byte offset 16 to 8; mm1024's moved a full 4096 B page) and task interleaving / cache residency. The
latter fits conv2d's 1t-faster/par-slower sign flip; the former does not. **S36c's `%Frame` alias
barrier stays refuted** — this is a different claim and vectorization is unchanged.

## 8. Live handoff state

| Type | Handle | State | Inspect | Cleanup |
| --- | --- | --- | --- | --- |
| branch | `main` | committed, **not pushed** | `git log --oneline -3` | Sapir's call |
| perf box | `100.81.226.103` i9-14900F | idle, governor `performance` (persists until reboot) | `ssh … 'cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor'` | none |
| box dir | `~/s38bench` | **new** — 14 objects + 14 linked binaries (`_pre`/`_post`), `s38run.sh` | `ls ~/s38bench` | `rm -rf ~/s38bench` when done |
| box dir | `~/s38fma` | **new** — the FMA-face twin | `ls ~/s38fma` | `rm -rf ~/s38fma` |
| box dirs | `~/s37bench`, `~/s36bench*`, `~/mapalbench` | untouched | `du -sh ~/s3*` | — |
| worktree | `…/scratchpad/pre-s38` @ `d3ca82c` | the PRE compiler, still built | `git worktree list` | `git worktree remove` |
| worktrees | two stale, @ `6168863` and `1daddaa` | S33 debt, still listed | `git worktree list` | `git worktree prune` |

Re-run either perf leg: `ssh 100.81.226.103 'cd ~/s38bench && RUNS=101 ./s38run.sh'`.

## 9. Open items

| P | Item | Reference | Next action | Done when |
| --- | --- | --- | --- | --- |
| P0 | **GPU leg via NVPTX** | plan-s38 §6, ADR-0033 | write the plan; decide how graph facts are supplied to a GPU `Loc` | a recognized matmul site runs on the 4090 through PTX, differential bit-exact |
| P1 | Inlining must stamp spliced morphisms with the call-site position | plan-s38 §6.1 | needs its own counterexample: a trapping helper inlined into a caller with an earlier-positioned trap | counterexample passes |
| P1 | `SourceLoc` is now a semantic attribute | plan-s38 §6.2 | write the ADR | ADR merged |
| P1 | **`mapal_par_trap` is resumable, `mapal_trap` is not** | this log §5 | one test: a deferred-trapping op whose independent sibling stores into a frame slot that outlives the fn | test pins the ordering |
| P1 | Oracle cannot witness pre-trap output | this log §4 | decide whether the interp should record output as an effect rather than a token value | decided, or documented as permanent |
| P1 | Oracle clones captured arrays per fold step | S37 handoff | `Rc` + CoW on `RValue::Array`, 46 sites — **plan it first**, and forbid agents writing into the repo | differential suite < 60 s |
| P2 | `packing_site` → `packed_layout_admits` | this log §6 | rename + move next to `packed_type` | done; zero behaviour change |
| P2 | Two §5 duplications (erasure remap, mapal-rt ABI) | this log §6 | consolidate into `mapal-ir` / one declaration | one source each |
| P2 | `elem_plan` headroom | S37 handoff | captured consumers block elision; `body_call_arg` alloca round-trip | — |
| P2 | Republish `docs/performance/shape-ladder-v2.md` | S37 handoff | saxpy's cells moved 4.86× / 2.23× | — |
| P2 | M4 Pro table needs an idle Mac | S37 handoff | re-run baselines in the same pass | — |

## 10. Method notes earned

1. **A multi-example golden test aborts at the first mismatch**, so a serial accept loop reveals one
   snapshot per full gate run. `INSTA_UPDATE=always` + review from `git diff` gets the whole set in
   one pass without weakening the gate — git holds the originals.
2. **Check the oracle's model before asserting against it.** The pre-trap stdout test failed on its
   first run because interpreted output is a token *value* that dies with an abort, while compiled
   output is a side effect that survives.
3. **A verdict from a fleet of agents is a hypothesis, not a result.** The NVPTX audit's blockers were
   flagged unverified by the audit itself; a 15-minute `llc` probe refuted them. Sapir asked for the
   check.
4. **Ask which statistic is stable before quoting a delta.** saxpy's *min* moved +4%, +9% and −29%
   across three passes; its *median* moved +5.2%, +5.3%, +5.3%.
5. **Run both faces.** mm1024's +2.6% regression exists only with FMA off.
6. **State the mechanism only when it is isolated.** "%Frame layout is the cause" was withdrawn; two
   candidates remain and nothing run this session separates them.

## 11. Docs reconciled

| Doc | Change |
| --- | --- |
| `components/ir/DESIGN.md` | §13 `topo_order`: "ties broken by insertion order" → source position, with the reason and the observable consequence; deduced-morphism table row notes `SourceLoc` is now semantic |
| `components/ir/IMPLEMENTATION.md` | `topo_order` row carries the tie-break key |
| `components/ir/STATUS.md` | S38 header; P0 closed; new "what works" row |
| `components/ir/plans/plan-s38-*.md` | PLANNED → SHIPPED; A′ refuted with numbers; the wider bug, the golden verdict, the perf table; §6 gains a third obligation |
| `docs/STATUS.md` | S38 roll-up |
| `docs/next-session.md` | retargeted to S39 |
| this log | new |

## 12. Files changed

Code: `crates/mapal-ir/src/algo.rs` · `crates/mapal-rewrite/tests/testgen/mod.rs` ·
`crates/backends/cuda/src/kernel.rs` (test assertion) ·
`crates/backends/llvm/tests/differential.rs` (new test).
Snapshots: 38 regenerated (31 llvm/cuda golden, 7 mermaid).
Docs: as §11.
