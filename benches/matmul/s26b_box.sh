#!/bin/bash
# S26b box driver (trimmed): Sapir's framing directive — par-on-par + 1t-on-1t.
# Runs ONLY the legs the two reframed s26.md tables need: flow cap f32/f64 par
# (wall + compute) + MAPAL_PAR=1, cpp naive+mt, rust naive+mt, chapel mc+1t,
# numpy threaded+1t. No cuda legs => no nvcc => any minimal ubuntu image works.
# Same protocol as s26_box.sh (rsync benches/matmul -> /root/bench, mapal-rt
# lib.rs -> /root/bench/mapal_rt.rs). Fixes the S26 gap: apt python3-pip BEFORE
# pip numpy (the cuda-devel image had no pip; s26_box.sh's pip line no-op'd).
set -e
export DEBIAN_FRONTEND=noninteractive
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
if ! clang --version 2>/dev/null | grep -qE "version (1[89]|[2-9][0-9])"; then
  apt-get update -qq && apt-get install -y -qq wget lsb-release software-properties-common gnupg >/dev/null
  wget -qO- https://apt.llvm.org/llvm.sh | bash -s -- 18 >/dev/null
  ln -sf /usr/bin/clang-18 /usr/local/bin/clang
  ln -sf /usr/bin/clang++-18 /usr/local/bin/clang++
fi
apt-get update -qq && apt-get install -y -qq python3-pip >/dev/null
python3 -c 'import numpy' 2>/dev/null || pip3 install -q numpy
clang --version | head -1
nproc; grep -m1 "model name" /proc/cpuinfo || true
cat /sys/fs/cgroup/cpu.max 2>/dev/null || cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us /sys/fs/cgroup/cpu/cpu.cfs_period_us 2>/dev/null || echo "no cgroup quota"
cd /root/bench
echo "== mapal-rt staticlib =="
rustc --edition 2024 --crate-type=staticlib -O mapal_rt.rs -o libmapal_rt.a
echo "== mapal-llvm builds (clang -O2 -march=native -ffp-contract=fast — the standing CPU recipe) =="
pids=()
for n in 16 64 128 256 512 1024; do
  for v in "cap" "cap_f32"; do
    clang -O2 -march=native -ffp-contract=fast "matmul${n}_${v}.ll" libmapal_rt.a -o "mm_ll_${v}_$n" -lpthread -ldl -lm &
    pids+=($!)
    clang -O2 -march=native -ffp-contract=fast "matmul${n}_${v}_perf.ll" libmapal_rt.a -o "mm_ll_perf_${v}_$n" -lpthread -ldl -lm &
    pids+=($!)
  done
done
for p in "${pids[@]}"; do wait "$p"; done
echo "== baseline builds (naive recipes: clang++ -O3 -march=native / rustc -O -C target-cpu=native) =="
clang++ -O3 -march=native cpp_naive.cpp -o cpp_naive
clang++ -O3 -march=native cpp_mt.cpp -o cpp_mt -lpthread
rustc -O -C target-cpu=native rust_naive.rs -o rust_naive
rustc -O -C target-cpu=native rust_mt.rs -o rust_mt
echo "== chapel (official 2.9.0 binary .deb, as runner.sh) =="
if ! command -v chpl >/dev/null; then
  wget -q https://github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb -O /tmp/chapel.deb \
    || curl -sL https://github.com/chapel-lang/chapel/releases/download/2.9.0/chapel-2.9.0-1.ubuntu22.amd64.deb -o /tmp/chapel.deb
  apt-get install -y /tmp/chapel.deb
fi
CHPL_TARGET_CPU=native chpl --fast chapel_matmul.chpl -o chapel_matmul
echo "== chapel-1t sanity (the env pin must slow the forall down, same c0) =="
./chapel_matmul --n=256 --iters=2 --width=f32
CHPL_RT_NUM_THREADS_PER_LOCALE=1 ./chapel_matmul --n=256 --iters=2 --width=f32
echo "== runs (leg filter — the two tables' legs only) =="
python3 runner.py \
  mapal-llvm-cap-f64 mapal-llvm-cap-f32 mapal-llvm-cap-compute-f64 mapal-llvm-cap-compute-f32 \
  mapal-llvm-cap-f64-1t mapal-llvm-cap-f32-1t \
  cpp-naive-f32 cpp-naive-f64 cpp-mt-f32 cpp-mt-f64 rust-naive rust-mt \
  chapel-f32 chapel-f64 chapel-1t-f32 chapel-1t-f64 numpy numpy-1t 2>&1 | tee /root/runner.log
echo "S26B BOX DONE"
