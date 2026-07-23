#!/bin/bash
# S25 box driver: the tile-emission CPU leg — full same-box sweep, tiled default.
# Same protocol as s24_box.sh (rsync benches/matmul -> /root/bench, flow-rt lib.rs
# -> /root/bench/flow_rt.rs); runner.sh now also builds the mm_ll_perf_* compute
# binaries and runner.py adds the flow-llvm-cap-compute-{f64,f32} legs.
# S25 addition: x86 disasm check — does the tiled kernel carry vfmadd (fused) or
# split vmul/vadd (the arm64 observation), and is it 256/512-bit?
set -e
export DEBIAN_FRONTEND=noninteractive
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
if ! clang --version 2>/dev/null | grep -qE "version (1[5-9]|[2-9][0-9])"; then
  apt-get update -qq && apt-get install -y -qq clang-15 python3-pip >/dev/null
  ln -sf /usr/bin/clang-15 /usr/local/bin/clang
  ln -sf /usr/bin/clang++-15 /usr/local/bin/clang++
fi
python3 -c 'import numpy' 2>/dev/null || pip3 install -q numpy >/dev/null 2>&1 || true
nproc; grep -m1 "model name" /proc/cpuinfo || true
cat /sys/fs/cgroup/cpu.max 2>/dev/null || echo "no cgroup v2 cpu.max"
cd /root/bench
bash runner.sh 2>&1 | tee /root/runner.log | tail -60
echo "== S25 disasm check (tiled f32 512 kernel) =="
objdump -d mm_ll_cap_f32_512 2>/dev/null | grep -oE "vfmadd[0-9]*ps|vmulps|vaddps" | sort | uniq -c | head -5
objdump -d mm_ll_cap_f32_512 2>/dev/null | grep -m1 -oE "%[yz]mm[0-9]+" | head -1
echo "S25 BOX DONE"
