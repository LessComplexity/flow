#!/bin/bash
# S24b mini-sweep: the fmad=true measurement (GPU only, ~10 min).
# Builds the FLOW_PERF kernel binaries with the NEW default (-fmad=true),
# plus naive-cuda f32/f64 (f64 column exists since S24 close) and cuBLAS for
# same-box ratios. Expects /root/bench with: matmul{512,1024,2048,4096}_cap{,_f32}_perf.cu,
# naive_cuda.cu, cublas_gemm.cu, flow_rt.rs, this script.
set -e
cd /root/bench
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
nvidia-smi -L
rustc --edition 2024 --crate-type=staticlib -O flow_rt.rs -o libflow_rt.a
for n in 512 1024 2048 4096; do
  for v in cap_perf cap_f32_perf; do
    nvcc -std=c++17 -fmad=true -arch=sm_89 "matmul${n}_${v}.cu" libflow_rt.a -o "mm_${v}_$n" -lpthread -ldl -lm
  done
done
nvcc -O3 -arch=sm_89 naive_cuda.cu -o naive_cuda
nvcc -O3 -arch=sm_89 cublas_gemm.cu -lcublas -o cublas_gemm
echo "== flow kernels, -fmad=true (3 runs each; FLOW_PERF lines are the data) =="
for n in 512 1024 2048 4096; do
  for v in cap_perf cap_f32_perf; do
    for i in 1 2 3; do echo "RUN $v $n #$i"; "./mm_${v}_$n"; done
  done
done
echo "== baselines =="
for spec in "512 50" "1024 20" "2048 5" "4096 3"; do
  set -- $spec
  ./naive_cuda "$1" "$2"
  ./naive_cuda "$1" "$2" f64
  ./cublas_gemm "$1" "$2"
done
echo "S24B BOX DONE"
