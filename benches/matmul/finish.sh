#!/bin/bash
# Post-cleanup finisher for the S20 sweep: sequential builds (no parallel OOM),
# then the full leg run on a quiet box. Idempotent — skips existing binaries.
cd /root/bench
. "$HOME/.cargo/env"
exec > finish.log 2>&1
set -x
for pair in "matmul128_cap.ll mm_ll_cap_128" "matmul128_cap_f32.ll mm_ll_cap_f32_128" "matmul256_cap.ll mm_ll_cap_256" "matmul256_cap_f32.ll mm_ll_cap_f32_256"; do
  set -- $pair
  if [ -f "$1" ] && [ ! -f "$2" ]; then
    clang -O2 -march=native "$1" libmapal_rt.a -o "$2" -lpthread -ldl -lm || echo "BUILD-FAILED $2"
  fi
done
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
