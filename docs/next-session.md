# Next Session (S23)

Written: 2026-07-22 · close of Session 22 · by: Claude Fable (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S22 closed: the minimal-emission wave shipped + ADR-0031.** Six commits on main (folder move ADR-0030 · `emission_plan` WP-A · ADR-0031 `n -> iota`/`(x, n) -> fill` surface, zero IR change proven · WP-B device twins · WP-C host lane · benches through the optimizer, 36 artifacts −2843 lines). The judged exhibit `d_fn3` is one return expression; sepia channels are one line each; `regen.sh` regenerates all bench artifacts with `--rewrite`. Workspace **851 green**, fmt clean, tree fully committed. `PREVIEW{.md,-matmul512.cu,-matmul512.ll}` at repo root are untracked viewing aids — regenerate or delete freely.

## Test state

`cargo test --workspace`: **851 green** (200 syntax · 153 ir · 155 lower · 29 check · 62 interp · 63 rewrite · 27 llvm + 1 ignored perf · 1 flow-rt · 161 cuda; flow-cli = 2 bins). Native oracle gate verified at close: `matmul4_cap.ll` → clang -O2 + libflow_rt.a → `-275`/`3748`. No hardware leg this session — **S22's emitter changes are differential-verified locally only; the box sweep is S23's first-class duty.**

## THE S23 MANDATE (Sapir, S22 close, verbatim intent)

1. **Delegation split:** codex CLI (`gpt-5.6-sol`, xhigh) does coding/debugging; Fable does review, fixes, changes, testing, analysis, planning, orchestration, docs. **Health-check codex FIRST** (see Gotchas — it was network-dead all of S22's second half; the split only binds when codex actually works; the S22 precedent for inline fallback stands).
2. **Run until full performance-comparison results exist.** S23 does not stop at code: it ends with the complete measured matrix — flow-cuda (raw + cap, f64/f32, process-wall + FLOW_PERF kernel time) and flow-llvm (cap f64/f32) vs naive-CUDA, cuBLAS, numpy, rust, cpp, chapel, at N=4→512 — on a fresh box, `docs/performance/matmul.md` rewritten, `results.csv` backed up BEFORE the sweep. That is the session's done-bar. ("Do all the optimizations to completion, until generated code is optimal and equivalent or better than naive CUDA too" — the S21 mandate — remains the standing frame.)

## The S23 agenda (ordered; the done-bar is the full matrix)

1. **WP-D — loop-invariant hoisting (#16, plan-minimal-emission §4):** invariant capture loads + `pair.fK = oN.fK` assemblies hoist out of per-thread/twin fold loops (d_fn4's 512×/thread re-reads). Same query, one extra rule. Golden re-pins hand-read; differential-gated.
2. **WP-E — llvm assessment:** measure the post-rewrite `.ll` for the exhibit set; the expected win is the alloca/load/store shuffle (direct SSA values), NOT -O2 runtime (S21 proved -O2 output already beats single-thread C++ at 256/512). Record either way in backend-llvm/suggestions.md; implement only if the assessment says it pays THIS session — the matrix outranks it.
3. **The box leg (the done-bar):** fresh vast.ai 4090 via `benches/matmul/s21_box.sh` (clang≥15 + numpy guards built in). (a) full cuda differential (10 examples + 320 testgen, raw+rewritten — S22's emitter rework MUST prove itself on hardware); (b) llvm 1280-run differential at -O0/-O2 if not run locally first; (c) `results.csv` BACKUP, then the full sweep N=4→512 all legs incl. FLOW_PERF kernel-time rows; (d) `docs/performance/matmul.md` rewritten with the S23 matrix (S22 archived in-doc convention). Destroy the box; hands-off `45510479` (Sapir's pytorch box) if still alive.
4. **If the matrix shows the gap:** the remaining named items ride the numbers — region emission v2 (S17 directive; the multi-merge-SCC oracle boundary is the design blocker, orchestrator's lane), launch geometry, `-fmad` (Sapir's open call below), arena v1.1 (18a), tree-fold (ADR-0028).
5. **P2 standing:** 17b dedup key; llvm heap lowering; `time` builtin (Sapir); procedural sepia; chapel-gpu; P7 Verilog; ADR-0030 protocol behind the CLI crate.

## Open questions for Sapir

- `-fmad=false` is oracle-pinned; allow a labeled `-fmad=true` non-oracle perf row? (closes a chunk of the kernel-time gap)
- `time` builtin: language feature or harness-only? (standing since S19)
- ADR-0023/0024/0025 in-file questions; lower §16 OQ1–OQ8 (standing).

## Gotchas / warnings

- **CODEX HEALTH CHECK FIRST (new, S22):** `codex exec` went network-dead twice — process alive, **~0.1 s cputime after 10+ minutes, zero edits**. Probe before relying on it: dispatch a trivial task (or check `ps -eo pid,etime,cputime -C codex`); if cputime stays <2 s past ~3 min, it's wedged — kill and either re-dispatch once or implement inline (S21 WP5 + S22 WP-B/C precedent). Check file mtimes, not process liveness.
- **All S08–S22 gotchas stand.** Key for S23's box: clang ≥ 15 (llvm legs skip SILENTLY on 14; `s21_box.sh` guards); **back up `results.csv` before any sweep** (S20's raw CSV was lost this way); vast.ai `loading` >15 min ⇒ recycle; box-side nohup survives local crashes (recover, don't re-run).
- **S22 emitter changes are hardware-unverified.** The cuda differential on the box is the FIRST S23 action gate — before any perf number is trusted.
- Replay-faithfulness invariant (S21) stands: new minting ops need `*_from` replay entries + testgen draws in the same change.
- `emission_plan` classification is final at the query; backends may only FORCE-NAME further (Call targets, bulk targets, product-typed Inline — see plan §3b/§3c), never inline what the query named.

## Commands (currently working)

```sh
cargo test --workspace                          # 851 green (~6 min)
cargo run -q -p flow-interp --example run -- benches/matmul/matmul4_cap.flow   # -275\n3748
./benches/matmul/regen.sh                       # re-emit all bench artifacts (--rewrite)
cargo run -q --release -p flow-backend-cuda --example emit -- benches/matmul/matmul512_cap.flow --rewrite -   # the exhibit
# box driver: benches/matmul/s21_box.sh (rsync → /root/flow, run; clang≥15 + numpy guards)
# perf home: docs/performance/matmul.md · raw: benches/matmul/results.csv (86 rows, S21 — BACK UP before sweeping)
```
