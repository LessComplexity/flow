#!/bin/bash
# S26 box driver: BLAS rung 2 (TI register blocking + fixed-TJ split) — same-box sweep.
# Same protocol as s25_box.sh (rsync benches/matmul -> /root/bench, flow-rt lib.rs
# -> /root/bench/flow_rt.rs; runner.sh builds, runner.py runs + results.csv).
# S26 changes vs s25_box.sh:
#   1. clang-18 via llvm.sh instead of apt clang-15 (standing gotcha: apt clang-15
#      leaves the tile nest fully scalar; clang version is result-changing).
#   2. machine specs (utc/cpu/threads/quota/RAM/clang) stamped atop results.csv
#      by runner.py itself (S26 rule: same-machine comparisons, specs on record).
#   3. disasm check re-pointed at the rung-2 kernel: expect ymm vfmadd in the
#      fixed-TJ main body (xmm/scalar = a finding, not a pass).
set -e
export DEBIAN_FRONTEND=noninteractive
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
if ! clang --version 2>/dev/null | grep -qE "version (1[89]|[2-9][0-9])"; then
  apt-get update -qq && apt-get install -y -qq wget lsb-release software-properties-common gnupg python3-pip >/dev/null
  wget -qO- https://apt.llvm.org/llvm.sh | bash -s -- 18 >/dev/null
  ln -sf /usr/bin/clang-18 /usr/local/bin/clang
  ln -sf /usr/bin/clang++-18 /usr/local/bin/clang++
fi
clang --version | head -1
python3 -c 'import numpy' 2>/dev/null || pip3 install -q numpy >/dev/null 2>&1 || true
nproc; grep -m1 "model name" /proc/cpuinfo || true
cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo "no cgroup v2 cpu.max"
cd /root/bench
bash runner.sh 2>&1 | tee /root/runner.log | tail -60
echo "== S26 disasm check (rung-2 tiled f32 512 kernel) =="
objdump -d mm_ll_cap_f32_512 2>/dev/null | grep -oE "vfmadd[0-9]*ps|vmulps|vaddps" | sort | uniq -c | head -5
objdump -d mm_ll_cap_f32_512 2>/dev/null | grep -m1 -oE "%[yz]mm[0-9]+" | head -1
echo "S26 BOX DONE"
