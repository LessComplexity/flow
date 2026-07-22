#!/bin/bash
# S20 sweep finisher v2: the N=256 .ll legs are recorded as BL1-walled (the
# literal-store modules: 23 MB, OOM-killed at 500 GB, ~hours per build) — the
# ADR-0029 procedural artifacts are the fix. This script: waits for any
# in-flight clang (the cap_128 build finishing standalone), builds
# cap_f32_128 + the baselines, runs the full leg matrix, marks .done.
cd /root/bench
. "$HOME/.cargo/env"
exec > finish2.log 2>&1
set -x
# Wait for the standalone cap_128 build (survives the killed finish.sh).
while pgrep -x clang > /dev/null; do sleep 30; done
if [ -f matmul128_cap_f32.ll ] && [ ! -f mm_ll_cap_f32_128 ]; then
  clang -O2 -march=native matmul128_cap_f32.ll libflow_rt.a -o mm_ll_cap_f32_128 -lpthread -ldl -lm || echo "BUILD-FAILED mm_ll_cap_f32_128"
fi
[ -x naive_cuda ] || nvcc -O3 -arch=sm_89 naive_cuda.cu -o naive_cuda || echo "BUILD-FAILED naive_cuda"
[ -x cublas_gemm ] || nvcc -O3 -arch=sm_89 cublas_gemm.cu -lcublas -o cublas_gemm || echo "BUILD-FAILED cublas_gemm"
if [ ! -x cpp_naive ]; then
  if command -v clang++ >/dev/null; then clang++ -O3 -march=native cpp_naive.cpp -o cpp_naive || echo "BUILD-FAILED cpp_naive";
  else g++ -O3 -march=native cpp_naive.cpp -o cpp_naive || echo "BUILD-FAILED cpp_naive"; fi
fi
[ -x rust_naive ] || rustc -O -C target-cpu=native rust_naive.rs -o rust_naive || echo "BUILD-FAILED rust_naive"
[ -x chapel_matmul ] || CHPL_TARGET_CPU=native chpl --fast chapel_matmul.chpl -o chapel_matmul || echo "BUILD-FAILED chapel_matmul"
set +x
echo "== builds done; running legs =="
python3 runner.py
echo "== ALL DONE =="
touch /root/bench/.done
