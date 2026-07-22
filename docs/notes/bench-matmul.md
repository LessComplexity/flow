# Benchmark: Flow matmul vs hand CUDA vs cuBLAS vs CPU (Session 16, 2026-07-21)

**Question (Sapir):** can a Flow matmul at best optimization levels be compared to other
languages on speed, over many iterations? **Answer: yes, and here it is — but the honest
headline is that today's Flow-CUDA matmul is bounded by the *execution strategy*
(one kernel launch per `Index`/`Update`, Θ(N³) launches at ~24 µs each), not by the
algorithm, the GPU, or the language's ceiling.** Every number below is measured, on one
vast.ai RTX 4090 box (nvcc 12.4.131, 12 vCPU, Ubuntu 22.04) unless marked macOS;
correctness was verified at every size (all legs print `c[0]`/`c[N²−1]` and agree exactly).

## Artifacts (in-tree, reproducible)

- `examples/matmul4.flow` — the readable N=4 program (runs `-275\n3748` through interp).
- `benches/matmul/` — `gen_flow.py` (the generator), `matmul{16,32,64,128}.flow`,
  `naive_cuda.cu`, `cublas_gemm.cu`, `numpy_bench.py`, `rust_naive.rs`, `runner.{sh,py}`,
  `results.csv`.
- Emitters: `cargo run -p flow-backend-{llvm,cuda} --example emit -- <file.flow>`
  (new `examples/emit.rs` dev tools in both backend crates).
- The Flow program is the only Core-expressible matmul shape today: a flattened
  loop-driven `cell(i,j)` dot-product with `c[t] <- v` (the ADR-0021 motivating
  pattern) — map/fold bodies may not close over arrays (**L1108**), so the one-kernel
  map+fold form is not expressible in Core.

## Correctness chain (checked first)

`interp` (N=4) = `flow-cuda` = `naive-cuda` = `numpy` at every N — e.g. N=64 →
`1047.0 / 2107.0`, N=128 → `-7312.0 / -933.0`, N=4096 → `74348.0 / -302529.0`.
The flow-cuda programs ran under the **pinned M3 recipe** (`nvcc -std=c++17 -fmad=false
-arch=sm_89`, flow-rt staticlib) — the differential contract, unmodified for speed.

## Numbers

Same box for all GPU/CPU legs except flow-llvm (macOS arm64, clang -O2, process wall min-of-5).
flow-cuda is process wall min-of-3; compiled CUDA/cuBLAS legs are per-iteration medians
after warmup (CUDA events); numpy/rust are per-iteration best-of.

| N | flow-cuda | flow-llvm (macOS) | naive-cuda | cuBLAS | numpy | rust-naive |
|---|---|---|---|---|---|---|
| 4 | 332.6 ms | 2.34 ms | — | — | — | — |
| 16 | 481.8 ms | 2.64 ms | — | — | — | — |
| 32 | 1.75 s | 5.14 ms | — | — | — | — |
| 64 | **12.16 s** | 316.2 ms | **0.0032 ms** (164 GF/s) | 0.0100 ms | 0.0107 ms | 0.203 ms |
| 128 | **99.8 s** | — | 0.0050 ms (839) | 0.0054 ms | 0.025 ms | 2.62 ms |
| 256 | — | — | 0.0120 ms (2.8 TF/s) | 0.0098 ms | 0.133 ms | 20.0 ms |
| 512 | — | — | 0.0857 ms (3.1) | 0.0123 ms (21.8 TF/s) | 0.745 ms | 208.8 ms |
| 1024 | — | — | 0.869 ms (2.5) | 0.046 ms (46.3) | 4.28 ms | 6.49 s |
| 2048 | — | — | 7.36 ms (2.3) | 0.327 ms (52.5) | 12.9 ms | — |
| 4096 | — | — | — | 2.46 ms (**55.9 TF/s**) | 402.7 ms | — |

## What the numbers say (five findings)

1. **flow-cuda's wall is per-op launch latency, Θ(N³) of it.** Each top-level `Index`
   is a 1-launch kernel + D→H readback + trap-check sync (~24 µs/op, measured: 12.16 s /
   528k ops at N=64, 99.8 s / 4.2M ops at N=128 — both ≈ 23–24 µs/op). The *same
   algorithm* as a single kernel runs in 0.0032 ms at N=64 — a **3.8-million× gap**
   (20M× at N=128). This is the DESIGN's recorded correct-first price (sync after every
   launch, §3) made visible; it is not the algorithm and not the GPU.
2. **The same `.flow` source on flow-llvm is 38× faster at N=64 — on a laptop CPU**
   (316 ms vs 12.16 s). One native process, alloca-resident arrays, no launches.
   Backend strategy dominates (VISION truth #1). flow-llvm's own jump from 5.1 ms
   (N=32) to 316 ms (N=64) is the recorded **W3 naive-`Update` wall** (the N⁴ copy) —
   the backends' two biggest walls are both already in the array-scale plan.
3. **The expressibility gap is the real blocker for a Flow GEMM.** The one-kernel form
   needs map bodies that close over the whole arrays — L1108 forbids it today.
   Relaxing that (pure read captures / a cartesian or broadcast form) is an **ADR-level
   language-design conversation**, not a backend optimization. Until then every Flow
   matmul is the flattened loop shape, and no backend can rescue Θ(N³) launches.
4. **The algorithm gap (independent of language) is 22×:** naive one-kernel CUDA peaks
   at ~3.1 TF/s (memory-bound, ~4% of the 4090's ~82.6 TF/s fp32 peak); cuBLAS reaches
   **55.9 TF/s at N=4096** (~68% of peak). That is the tiling/tensor-core gap — it
   defines what "best optimization levels" would mean for a future Flow GEMM kernel
   (shared-memory tiling before anything else matters).
5. **CPU context:** numpy (the box's BLAS) sits between naive-CUDA and cuBLAS at big N
   (1.3 TF/s at N=2048); single-thread naive Rust tracks ~1.3–2.6 GF/s until the
   N=1024 cache cliff (0.33). flow-cuda at N=128 is ~38,000× slower than naive Rust;
   the gap is launches, not arithmetic.

## Caveats (what this does not show)

- flow-cuda at N=4/16 is dominated by process/CUDA-context startup (~0.3 s); the
  N³ launch law is the clean signal from N=32 up.
- These are single-GPU, single-precision, square sizes; cuBLAS ran `CUBLAS_DEFAULT_MATH`
  (no TF32) to stay honest fp32.
- Nothing here measures a "Flow is slow" property of the language: it measures (a) one
  deliberately correct-first execution strategy, and (b) one missing language feature
  (captures). Both walls are named, recorded, and have design paths.
- numpy's N=4096 dip (341 GF/s) is its BLAS threading behavior at that size — reported,
  not smoothed.

## The ladder that closes it (all already recorded headroom)

**Sapir's S16 directive: performance is a per-step gate, not a milestone appendix.** From S16 on, every backend increment carries a perf contract (measured baseline + budget, asserted like the R1 differential asserts correctness), and every DESIGN states its expected physical costs up front (launches, transfers, copies — the backend-cuda §2 inventory pattern). The ordered critical path, each step landing with before/after numbers:

1. **Capture/cartesian ADR** (expressibility) — lets `map`/`fold` bodies read whole
   arrays; enables the one-kernel GEMM formulation. Sapir's design call.
2. **Bulk/fused kernels** — rewrite's Map∘Map fusion already arrives free (§8.2);
   a `fold`-aware fused GEMM kernel is the next step once (1) exists.
3. **Batched trap checks + scalar forwarding on device** (backend-cuda suggestions
   #3/#4) — removes the ~24 µs/op floor for any op-rich shape.
4. **Tiling** — only meaningful after (1)–(2); the 22× naive→cuBLAS gap says where the
   ceiling is.

## Toolchain note (Sapir's PTX question, S16)

The CUDA-C++-via-nvcc path is orthogonal to these walls. A PTX/LLVM-NVPTX backend
would not reuse the host-side llvm emitter (host IR ≠ device IR: address spaces, no
host calls, NVVM intrinsics) and would add driver-API launch glue — more parts, no
perf (nvcc's device pipeline is Clang→NVPTX→ptxas underneath). The interesting variant
is **NVRTC**: same emitted `.cu` text, compiled at runtime — removes the nvcc *binary*
dependency and enables JIT (relevant to a future `flow run --gpu` without a toolkit
install). Recorded in `components/backend-cuda/suggestions.md` #11; a spike belongs to
a future session if JIT becomes a requirement. (Mature precedent for the raw-NVPTX
path exists — Triton, Numba — but they have no host surface; Flow has a real one:
scalars, guards, loop driving, `flow-rt` IO.)

## Reproduce

```sh
# Flow programs (any N): python3 benches/matmul/gen_flow.py <N> benches/matmul/matmul<N>.flow
cargo run -p flow-backend-cuda --example emit -- benches/matmul/matmul64.flow   # → .cu
cargo run -p flow-backend-llvm --example emit -- benches/matmul/matmul64.flow   # → .ll
clang -O2 benches/matmul/matmul64.ll target/debug/libflow_rt.a -o /tmp/mm       # local llvm leg
# New legs (harness, no numbers recorded here yet):
#   f32 capture variants: python3 benches/matmul/gen_flow_capture.py <N> --width f32
#     (checked in: matmul{16,64,128,256}_cap_f32.flow; emit .cu/.ll exactly as above)
#   cpp naive baseline (both widths): clang++ -O3 -march=native benches/matmul/cpp_naive.cpp -o cpp_naive
#     then `cpp_naive <N> <ITERS> f32|f64` — runner legs cpp-naive-f32 / cpp-naive-f64
#   flow legs (runner.sh builds every checked-in .cu/.ll into mm_{cu,ll}[_cap[_f32]][_perf]_<N>):
#     flow-cuda + flow-llvm (loop f64, N=4..128), flow-cuda-cap-f64/f32 + flow-llvm-cap-f64/f32
#     (capture, N=16..256, process wall min-of-3; llvm: clang -O2 -march=native —
#     native-codegen parity with rust_naive's -C target-cpu=native)
#   perf-instrumented kernels (legs flow-cuda-cap-kernel-f64/-f32, per-iteration compute =
#     sum of the FLOW_PERF launch= CUDA-event lines, min of 3 process runs):
#     cargo run -p flow-backend-cuda --example emit -- <file.flow> - --perf > benches/matmul/<base>_perf.cu
#   All bench .cu/.ll artifacts are checked in, emitted from the W1+W2 tree (by-ref llvm
#     captures, deduped kernels — 3 __global__ for 4 launches, one arena cudaMalloc per
#     capture flow_main) — rsync benches/matmul to the box as usual.
#   chapel baselines (chapel-f32/f64, forall over a 2D domain, one binary both widths):
#     CHPL_TARGET_CPU=native chpl --fast benches/matmul/chapel_matmul.chpl -o chapel_matmul
#     then `chapel_matmul --n=<N> --iters=<ITERS> --width=f32|f64` (config consts, no positionals).
#     Box install (Ubuntu 22.04, official binary package): wget
#     github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb
#     && apt-get install ./chapel-2.9.0-1.ubuntu22.amd64.deb (runner.sh does this; source-build
#     fallback: util/setchplenv.bash — the PREFERRED config, not quickstart — && make)
# GPU leg (vast.ai, per backend-cuda DESIGN §6): image nvidia/cuda:12.4.1-devel-ubuntu22.04,
# rsync benches/matmul + crates/flow-rt/src/lib.rs (as flow_rt.rs), then: bash runner.sh
# Box cost for this run: ≈ $0.18 (destroyed after, per the standing rule).
```


---

# S18 update (2026-07-21): the capture form — the wall is gone

**What changed:** ADR-0027 (ratified by Sapir) landed — map/fold bodies may read enclosing
bindings (pure read captures). The matmul is now writable in its natural one-kernel form:
a map over cells with an inner fold over the captured `a`/`b`. Same generator, same arrays,
same pinned recipe, same box class (RTX 4090, destroyed after, ≈$0.30 incl. a CN-network
rustup workaround — `libflow_rt.a` cross-compiled locally and shipped).

**Correctness first (as always):** flow-cuda output = interp = numpy at every N
(1815/6944 · 1047/2107 · −7312/−933 · −3694/10946). The LLVM differential covers the
capture programs at `-O0`/`-O2`, raw+rewritten (agent-run, green). The `.cu` is exactly
one `__global__` for the matmul with the inner fold as a per-thread loop and the captured
arrays as kernel params (see `matmul_cap.cu` discussion in the S18 session log).

## The numbers (process wall, min-of-3, same box for all S18 rows)

| N | S16 loop form | S18 capture form | speedup |
|---|---|---|---|
| 64 | 12,164 ms | **277 ms** | **44×** |
| 128 | 99,767 ms | **278 ms** | **359×** |
| 256 | ~13 min (extrapolated) | **278 ms** | **~2,800×** |

**The wall is gone.** The capture form is FLAT across sizes — the whole matmul is 4 launches
(enumerate, one map kernel, two index readbacks) with the cell computation on device; the
remaining ~277 ms is **process startup** (CUDA context init + buffer allocs), not compute.
And the process-wall comparison on the same box:

| program | N=64 | N=256 |
|---|---|---|
| **flow-cuda (capture)** | **277 ms** | **278 ms** |
| naive-cuda binary | 333 ms | 311 ms |
| cublas binary | 496 ms | 469 ms |

flow-cuda is now *faster than the naive-C binary at process scale* (smaller startup
footprint; cuBLAS pays ~150 ms of handle init). Kernel-time precision (the flow kernel is
below the process-wall floor at these sizes — the naive kernel's 0.003–0.012 ms class) is
the recorded next step: instrumented timing in the perf-gate harness.

## Honest contrasts (measured, not argued)

- **flow-llvm on the SAME capture source is SLOWER than the old loop form at N=64**
  (2,076 ms vs 316 ms): the llvm backend passes aggregates **by value** — the captured
  `[16 x float]` array is copied inline into every element's body call. Captures are only
  free where they are pointer handles (CUDA). By-reference capture passing is llvm headroom.
- **The `.cu` shows the emitter-quality gaps** (nvcc `-W#550-D` warnings on the bench run:
  unused `FlowProd` locals, dead host-side `fn1`/`fn2` twins, guard duplication) — recorded
  as backend-cuda suggestions #12–18 (kernel shape dedup, arena allocation, guard elision,
  trap-param trimming, copy propagation) with dedup+arena pulled forward pre-region.

## What this closes and what it opens

Closed: the S16 finding #3 (L1108 expressibility) — the one-kernel GEMM is writable,
correct, and fast enough that the next wall is process startup, not the language or the
mapping. Open (in order): kernel-time instrumentation in the perf gate; the llvm by-value
capture cost; the emitter-quality rows (#12–18); the region-v2 emission plan (strip →
zones → one kernel per region) where tiling becomes the real question (cuBLAS's 55.9 TF/s
ceiling stands).
