#!/bin/bash
# S20 sweep finisher v3: NO more .ll builds (the literal-store modules are
# BL1-walled: 27 GB RSS / 1.5h+ CPU per 5.7 MB module; the 23 MB N=256 ones
# OOM at 500 GB — recorded, the ADR-0029 procedural artifacts are the fix).
# Baselines + the full leg run on a quiet box, then .done.
cd /root/bench
. "$HOME/.cargo/env"
exec > finish3.log 2>&1
set -x
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
