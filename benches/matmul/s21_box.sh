#!/bin/bash
# S21 box driver (RTX 4090, cuda:12.4.1-devel): ONE box discharges all three
# P0s — (1) remote cuda differential over the S21 shapes (iota/fill/widen +
# S20c trap-free kernels), (2) MAPAL_PERF re-measure, (3) the full v2
# procedural sweep incl. the N=512 legs and the previously BL1-walled llvm
# N>=128 legs. Repo expected at /root/flow (rsync'd from the workstation).
set -e
cd /root/flow
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
# clang >= 15 REQUIRED: the emitter's `.ll` uses opaque-pointer `ptr` syntax
# (LLVM 15 default); Ubuntu 22.04's bare `clang` is 14 and fails to parse —
# every llvm leg then skips silently (S21 gotcha, cost one sweep re-run).
if ! clang --version 2>/dev/null | grep -qE "version (1[5-9]|[2-9][0-9])"; then
  apt-get update -qq && apt-get install -y -qq clang-15 python3-pip >/dev/null
  ln -sf /usr/bin/clang-15 /usr/local/bin/clang
  ln -sf /usr/bin/clang++-15 /usr/local/bin/clang++
fi
python3 -c 'import numpy' 2>/dev/null || pip3 install -q numpy >/dev/null 2>&1 || true

echo "== S21 remote cuda differential (the M3 duty over the S21 emitter) =="
cargo test -p mapal-backend-cuda 2>&1 | tee /root/differential.log | grep -E "test result|FAILED|running" | tail -12

echo "== bench dir setup =="
mkdir -p /root/bench
cp -r benches/matmul/* /root/bench/
cp crates/mapal-rt/src/lib.rs /root/bench/mapal_rt.rs
cd /root/bench
echo "== builds (runner.sh: mapal-rt staticlib, cu/ll legs, baselines, chapel) =="
bash runner.sh 2>&1 | tee /root/runner_build.log | tail -25
echo "== sweep =="
python3 runner.py 2>&1 | tee /root/runner_run.log | tail -40
echo "S21 BOX DONE"
