# Next Session (S22)

Written: 2026-07-22 · close of Session 21 · by: Claude Fable (orchestrator; category-architect skill; coding delegated to codex gpt-5.6-sol per Sapir's S21 split)

## Where things stand (≤5 lines)

**S21 closed: ADR-0029 stage 2 fully shipped** — cuda iota/fill kernels (5th `Unsupported` cell discharged), `Operation::Widen` + `widen_i64/f32/f64` across the whole pipeline, procedural v2 bench sources (3.8 MB → 72 KB), llvm WP3b (no first-class aggregate array moves — matmul256 clang -O2: OOM-kill → 0.08 s/57 MB). **All three P0s discharged on one box**: remote cuda differential 15/15 on a 4090 (its rewritten iota/fill leg exposed a rewrite-fixpoint P0 — replay's `Fill` re-minted its internal tuple, resurrecting CSE'd duplicates; fixed via the `fill_from` replay entry + testgen Iota/Fill draws), FLOW_PERF re-measure, full sweep N=4→512 all legs. Headlines: **flow-llvm cap f64 beats single-thread C++ at N=256/512**; **flow-cuda kernel f32 812 GFLOP/s @512** (4× from naive CUDA). Perf table: `docs/performance/matmul.md` (S20 archived in-doc). Tree uncommitted (S14–S21; Sapir owns commits).

## Test state

`cargo test --workspace`: **841 green** (200 syntax · 144 ir · 154 lower · 29 check · 62 interp · 63 rewrite · 27 llvm+1 ignored perf · 1 flow-rt · 161 cuda; flow-cli = 2 bins, no tests) at close; fmt clean. Hardware: cuda differential 15/15 (640+ runs) on the S21 emitter; llvm 1,280-run differential green at -O0/-O2 post-WP3b.

## THE S22 MANDATE (Sapir, verbatim intent, S21 close)

**"Do all the optimizations to completion, until generated code is optimal and equivalent or better than naive CUDA too."** Cross-backend — the minimal-emission principle is a Cat-IR-level trait, not a cuda patch:

- **One name per value, ever.** A value gets a name only where its chain genuinely splits (>1 consumer) — and even then ONE name that consumers REFERENCE; never a per-consumer re-wrap (the `o6=(t,512)`/`o8=(t,512)` duplicate-wrapper offense in matmul512_cap.cu `d_fn4`), never a name-copy.
- **Chains emit as chains.** Straight-line graph paths compose into expressions (`Add → Mul → pair …`), no hanger local per edge. The execution graph doesn't carry named points; the text should read as the operations directly.
- Judged against the artifact: `benches/matmul/matmul512_cap.cu` `d_fn3`/`d_fn4` are the before-exhibits.

## The S22 agenda (ordered)

0. **Folder move (P0, accepted — ADR-0030 §Folder move):** AFTER Sapir commits the S14–S21 tree — `crates/flow-backend-{b}` → `crates/backends/{b}`, names unchanged, one atomic commit.
1. **Rewrite-before-emit for benches** (`--rewrite` on the emit examples; bench legs measure the FULL pipeline): existing CSE already merges duplicate wrapper products — near-zero code, immediate.
2. **The minimal-emission functor (the mandate's core; cuda seq llvm):** materialize iff fanout > 1 ∨ consumed-whole (call arg / capture struct / escape / effect boundary); single-consumer values inline into consumer expressions; consumers reference the one name, no re-packing (suggestions #15 done as the split rule, not text cleanup) + **#16** loop-invariant hoisting. Differential-gated at every step (raw+rewritten, both opt levels, box leg).
3. **Region emission v2 (S17 directive):** fold/map bodies fuse into their loops — kills the per-iteration struct+call ceremony (`d_fn3(pair)` 512×/thread). The multi-merge-SCC oracle boundary must be solved here (blocks matmul region acceptance).
4. **The last measured gaps to ≥ naive CUDA:** `-fmad` decision (Sapir — oracle-pinned today; labeled non-oracle row?), launch geometry, module-load constant out of the kernel-time sum; arena v1.1 (18a) + tree-fold (ADR-0028) ride along where the numbers direct.
5. **P2 standing:** 17b dedup key; llvm heap lowering (8 MB stack face); `time` builtin (Sapir); procedural sepia; chapel-gpu; P7 Verilog; ADR-0030 protocol behind the CLI crate.

## Open questions for Sapir

- Commit the S14–S21 tree (the folder move waits on it).
- `time` builtin: language feature or harness-only? (standing since S19)
- `-fmad=false` is oracle-pinned; a `-fmad=true` non-oracle perf leg would close much of the 4× kernel gap — allow as a labeled extra row?
- ADR-0023/0024/0025/0026 in-file questions (standing). Lower §16 OQ1–OQ8 (standing).

## Gotchas / warnings

- **All S08–S21 gotchas stand.** New in S21: **box `clang` must be ≥ 15** (Ubuntu 22.04 apt default is 14 — predates opaque-`ptr`; every llvm leg then skips SILENTLY; `s21_box.sh` now guards this); the S20 raw results.csv was overwritten by the runner before archiving (S21 numbers are `results.csv`, S20's survive only in the matmul.md archive + session logs — **back up results.csv before any sweep**); codex CLI can stall silently on long tasks (1 h, zero edits — kill + re-dispatch or implement inline; check file mtimes not just process liveness); a killed local session does NOT kill box-side nohup work (recover, don't re-run); `vastai` fresh instances can sit in `loading` 15+ min — recycle (S15 precedent).
- **vast.ai hands-off instance:** `45510479` (Sapir's pytorch box) was still running at S21 close — do not use or destroy. S21's own boxes (45516002 flaky, 45516809 work) both destroyed.
- **Replay-faithfulness invariant (new, load-bearing):** replay must emit structure from EXISTING remapped objects, never via sugar builders that mint (the S21 fixpoint class). Any new op with internal minting needs a `*_from` replay entry + a testgen draw in the same change.

## Commands (currently working)

```sh
cargo test --workspace                                        # 841 green (~6 min; llvm sweep ~330 s)
cargo test -p flow-syntax -p flow-ir -p flow-lower -p flow-check -p flow-interp -p flow-rewrite   # fast six
cargo run -p flow-interp --example run -- benches/matmul/matmul4_cap.flow    # -275\n3748
cargo run -p flow-backend-cuda --example emit -- benches/matmul/matmul64_cap.flow -- --perf  # FLOW_PERF variant
# box driver: benches/matmul/s21_box.sh (rsync repo → /root/flow, run) — clang≥15 + numpy guards included
# perf: docs/performance/matmul.md · raw: benches/matmul/results.csv (86 rows, S21)
```
