#!/bin/bash
# Box-side runner for the matmul benchmark (RTX 4090, cuda:12.4.1-devel).
set -e
cd /root/bench
. "$HOME/.cargo/env"
echo "== toolchain =="
nvcc --version | tail -1
rustc --version
nvidia-smi -L
echo "== flow-rt staticlib (rustc direct, dependency-free) =="
rustc --edition 2024 --crate-type=staticlib -O flow_rt.rs -o libflow_rt.a
echo "== flow-cuda builds (pinned recipe, DESIGN §4/§6) =="
for n in 4 16 32 64 128; do
  if [ -f "matmul$n.cu" ] && [ ! -f "mm_cu_$n" ]; then
    nvcc -std=c++17 -fmad=true -arch=sm_89 "matmul$n.cu" libflow_rt.a -o "mm_cu_$n" -lpthread -ldl -lm
    echo "built mm_cu_$n"
  fi
done
echo "== flow-cuda-cap builds (pinned recipe, same as the loop legs) =="
for n in 16 64 128 256 512 1024 2048 4096; do
  for v in "cap" "cap_f32" "cap_perf" "cap_f32_perf"; do
    if [ -f "matmul${n}_${v}.cu" ] && [ ! -f "mm_cu_${v}_$n" ]; then
      nvcc -std=c++17 -fmad=true -arch=sm_89 "matmul${n}_${v}.cu" libflow_rt.a -o "mm_cu_${v}_$n" -lpthread -ldl -lm
      echo "built mm_cu_${v}_$n"
    fi
  done
done
echo "== flow-llvm builds (clang -O2 -march=native -ffp-contract=fast: the S24b fmad decision, CPU face — bench is perf mode; the differential gate stays contraction-off) =="
if command -v clang >/dev/null; then
  # Parallel + incremental (S21: the v2 procedural modules are ~21 KB — each
  # clang build is sub-second after WP3b; the loop stays for rerun-resume).
  pids=()
  for n in 4 16 32 64 128; do
    if [ -f "matmul$n.ll" ] && [ ! -f "mm_ll_$n" ]; then
      clang -O2 -march=native -ffp-contract=fast "matmul$n.ll" libflow_rt.a -o "mm_ll_$n" -lpthread -ldl -lm && echo "built mm_ll_$n" &
      pids+=($!)
    fi
  done
  for n in 16 64 128 256 512 1024 2048 4096; do
    for v in "cap" "cap_f32"; do
      if [ -f "matmul${n}_${v}.ll" ] && [ ! -f "mm_ll_${v}_$n" ]; then
        clang -O2 -march=native -ffp-contract=fast "matmul${n}_${v}.ll" libflow_rt.a -o "mm_ll_${v}_$n" -lpthread -ldl -lm && echo "built mm_ll_${v}_$n" &
        pids+=($!)
      fi
      if [ -f "matmul${n}_${v}_perf.ll" ] && [ ! -f "mm_ll_perf_${v}_$n" ]; then
        clang -O2 -march=native -ffp-contract=fast "matmul${n}_${v}_perf.ll" libflow_rt.a -o "mm_ll_perf_${v}_$n" -lpthread -ldl -lm && echo "built mm_ll_perf_${v}_$n" &
        pids+=($!)
      fi
    done
  done
  rc=0
  for p in "${pids[@]}"; do wait "$p" || rc=1; done
  if [ "$rc" -ne 0 ]; then echo "WARNING: one or more clang builds failed (affected legs skip-with-reason)"; fi
else
  echo "clang not on box — skipping all flow-llvm legs (run them locally instead)"
fi
echo "== baseline builds =="
nvcc -O3 -arch=sm_89 naive_cuda.cu -o naive_cuda
nvcc -O3 -arch=sm_89 cublas_gemm.cu -lcublas -o cublas_gemm
rustc -O -C target-cpu=native rust_naive.rs -o rust_naive
rustc -O -C target-cpu=native rust_mt.rs -o rust_mt
if command -v clang++ >/dev/null; then
  clang++ -O3 -march=native cpp_naive.cpp -o cpp_naive
  clang++ -O3 -march=native cpp_mt.cpp -o cpp_mt -lpthread
else
  echo "clang++ not on box — falling back to g++ for cpp_naive"
  g++ -O3 -march=native cpp_naive.cpp -o cpp_naive
  g++ -O3 -march=native cpp_mt.cpp -o cpp_mt -lpthread
fi
echo "== chapel (binary .deb; the .chpl compile-check happens here, box-side) =="
# Official Chapel 2.9.0 binary package for Ubuntu 22.04 x86_64 (the documented
# package-manager install — chapel-lang.org/docs/usingchapel/QUICKSTART.html):
#   wget https://github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb
#   apt-get install ./chapel-2.9.0-1.ubuntu22.amd64.deb
# Single-locale CPU leg: the package defaults are what we want (CHPL_COMM=none,
# qthreads, flat locale model). Source-build fallback if the .deb ever fails:
#   tar xzf chapel-2.9.0.tar.gz && cd chapel-2.9.0
#   source util/setchplenv.bash   # the PREFERRED config (NOT util/quickstart/
#   make -j                       #  — quickstart is the low-performance build)
if ! command -v chpl >/dev/null; then
  apt-get update -qq || true
  wget -q https://github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb -O /tmp/chapel.deb \
    || curl -sL https://github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb -o /tmp/chapel.deb
  apt-get install -y /tmp/chapel.deb
fi
chpl --version | head -2
# CHPL_TARGET_CPU=native is already the linux64 default under --fast; set it
# explicitly for recorded parity with rust_naive's -C target-cpu=native.
if [ ! -f chapel_matmul ]; then CHPL_TARGET_CPU=native chpl --fast chapel_matmul.chpl -o chapel_matmul; fi
echo "== numpy =="
pip install -q numpy 2>&1 | tail -1 || true
python3 -c "import numpy; numpy.show_config()" | head -3 || true
echo "== runs =="
python3 runner.py
