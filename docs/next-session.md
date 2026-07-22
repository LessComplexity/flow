# Next Session (S24)

Written: 2026-07-22 · close of Session 23 · by: Claude Fable (orchestrator; category-architect skill)

## Where things stand (≤5 lines)

**S23 closed: the S21→S22 mandate ran to its done-bar.** WP-D hoisting shipped (`64f1f50`, codex), WP-E assessed-and-deferred with measurements (`1daef83`), the in-twin Fold force-Named fix landed after the FIRST hardware run of the S22 emitters caught it (`acdb319`), and the **full S23 performance matrix exists** (`7b7680c`): 84 rows, one fresh 4090, every mandated leg, six-way output agreement, `docs/performance/matmul.md` rewritten. Workspace **853 green**, fmt clean, tree committed, box destroyed (≈$0.42). `PREVIEW{.md,-matmul512.cu,-matmul512.ll}` at repo root refreshed to the WP-D text (untracked viewing aids).

## Test state

`cargo test --workspace`: **853 green** (200 syntax · 153 ir · 155 lower · 29 check · 62 interp · 63 rewrite · 27 llvm + 1 ignored perf · 1 flow-rt · 163 cuda). The llvm 1280-run differential (-O0/-O2) runs INSIDE the local suite (clang present). The cuda remote differential: **15/15 green on the S23 box** over the S22+WP-D emitters. New local gate: `cargo run -q --release -p flow-backend-cuda --example emit_sweep` — the deterministic 320-draw EMISSION sweep without nvcc (640/640; run it before any future box leg).

## The numbers (what S24's work rides on — full table in `docs/performance/matmul.md`)

- **GEMM kernel at naive-CUDA PARITY from N=1024 f32** (0.788 vs 0.785 ms @1024 = 1.00×; 1.06× @2048) — the S21 mandate's "equivalent or better than naive CUDA" is MET at saturation sizes. The 1.6× @512 is small-N overhead (`-fmad` + geometry show only there).
- The remaining GPU distance is cuBLAS's algorithm class (24.6× @4096 f32) — tiling/shared-memory/tensor cores, not codegen.
- Scale legs exist to 4096 (cuda) / 1024 (llvm, stack-bound — heap lowering unlocks 2048+).
- cap wall flat ~270 ms at every N (context startup); loop form Θ(N³) launches unchanged (region v2's target).
- flow-llvm cap f32 = single-thread C++ parity; f64 0.86× (S21's 1.30× reversed — znver3 box variance, recorded; compare within one box only).

## The S24 agenda (Sapir directives at S23 close + the numbers)

1. **flow-llvm parallel orchestrator (Sapir, S23 close — "it should be parallel first, this is the idea"):** the CPU backend is single-thread; design a threading orchestrator so map/fold bulk sites fan across cores (the 60× chapel-48-core gap at N=512 is this). Design sketch to start from: the cuda backend's bulk-site machinery is the template — a bulk op site becomes `flow_par_for(n, body, ctx)` in flow-rt (pthreads in the dependency-free staticlib) instead of a kernel launch; E2/purity already legalizes the fan-out (the same proof that legalizes kernels); first-trap-wins via a shared flag (the cuda trap-protocol shape); float folds stay sequential-pinned unless ADR-0028's tree class applies. Model-first plan before code (§6.1); orchestrator's design lane, codex codes.
2. **chapel-gpu leg (Sapir, S23 close — "compare cpu to cpu and gpu to gpu"):** the .deb is CPU-locale only; the GPU leg needs a `CHPL_LOCALE_MODEL=gpu` source build with CUDA on the box (budget ~20-30 min box time for the chapel build; cache the tarball). Lands in the s24 report's GPU table against flow-cuda.
3. **`-fmad` decision (Sapir):** a labeled `-fmad=true` non-oracle perf row would close a chunk of the 1.6×. Standing since S21.
4. **Launch geometry (#5):** grid-stride + block-size tuning — the other half of the 1.6×; measure-first on the existing FLOW_PERF rows.
5. **Region emission v2 (S17 directive):** the loop form's Θ(N³)-launches fix; the multi-merge-SCC oracle boundary is the design blocker — orchestrator's lane, plan exists (`plan-region-emission.md`).
6. **P2 standing:** arena v1.1 (18a), tree-fold (ADR-0028), 17b dedup key, llvm heap lowering, `time` builtin (Sapir), procedural sepia, P7 Verilog, ADR-0030 protocol behind the CLI crate.
7. **Docs debt:** ADR-0029/0031 `flow-as-implemented` patch rows (standing "on ledger close").

**Perf-report format is now a standing rule (S23, Sapir):** per-session files `docs/performance/matmul/sNN.md` + thin index; compute-only tables grouped GPU-vs-GPU / CPU-vs-CPU; wall tables separate; ratios ONLY vs flow with the numbers visible; no box-ID/cost noise in perf docs. Memory: `perf-report-format`.

## Open questions for Sapir

- `-fmad=false` is oracle-pinned; allow a labeled `-fmad=true` non-oracle perf row? (now measured: it + geometry are the whole remaining kernel gap)
- `time` builtin: language feature or harness-only? (standing since S19)
- ADR-0023/0024/0025 in-file questions; lower §16 OQ1–OQ8 (standing).

## Gotchas / warnings

- **CODEX: always `codex exec "..." </dev/null` (S23 root cause).** The S22 "network-dead" signature (~0.1 s cputime, zero edits) was codex BLOCKING ON STDIN in non-TTY shells — `</dev/null` fixed it on the spot, twice. The health-probe + mtime-watch rules still stand as backstop. Memory updated (`model-split-preference`).
- **Box differential on big-vCPU boxes: pin to ~16 cores** (`taskset -c 0-15 cargo test -p flow-backend-cuda`). 48-way fan-out serializes CUDA context init and starves the 15 s run timeout — the S23 first attempt's 3 "divergences" were ALL this, zero real.
- **Run `emit_sweep` locally before trusting emitters** — emission panics hide from the local suite (differential skips without nvcc); this example is the no-GPU gate that would have caught the S22 fold bug.
- **znver3 + clang 15 `-O2 -march=native` stalls >25 min** on loop-form `.ll` at N≥128 — box-dependent; kill + skip-with-reason (runner.sh already handles a dead clang).
- **vast.ai box egress may block port-80 apt mirrors** — `sed -i 's|http://|https://|g' /etc/apt/sources.list` before installs (chapel's dep chain hit this).
- **ssh remote kills: never put the kill pattern and the relaunch text in one command** — `pkill -f` matches the ssh shell's OWN cmdline (use `[s]21`-style bracket guards AND split kill/launch into separate ssh calls; two S23 relaunches were lost to this).
- **All S08–S22 gotchas stand** (results.csv backup before sweeps — done this session into `results-s21.csv`; vast.ai `loading` >15 min ⇒ recycle — exercised; box-side nohup survives local crashes).
- Replay-faithfulness invariant (S21) stands; `emission_plan` classes final at the query — backends only force-Named (now incl. `Fold` targets on the twin side).

## Commands (currently working)

```sh
cargo test --workspace                          # 853 green (~6 min, incl. llvm -O0/-O2 differential)
cargo run -q --release -p flow-backend-cuda --example emit_sweep   # 640/640 emission sweep, no nvcc
cargo run -q -p flow-interp --example run -- benches/matmul/matmul4_cap.flow   # -275\n3748
./benches/matmul/regen.sh                       # re-emit all bench artifacts (--rewrite)
cargo run -q --release -p flow-backend-cuda --example emit -- benches/matmul/matmul512_cap.flow --rewrite -   # the exhibit
# box driver: benches/matmul/s21_box.sh (differential inside; pin tests to 16 cores on big boxes)
# perf home: docs/performance/matmul.md · raw: benches/matmul/results.csv (S23, 84 rows) · archives: results-s21.csv, results-pre-s20.csv
```
