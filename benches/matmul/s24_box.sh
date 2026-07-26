#!/bin/bash
# S24 box driver: the parallel-orchestrator CPU leg — full same-box sweep.
# mapal-llvm legs now run the mapal-rt scheduler (MAPAL_PAR unset = all cores);
# the -1t rows are the same binaries pinned to one thread (runner.py S24 rows).
# No cargo, no differential: emitters were hardware-verified S23 and the .cu
# text is byte-identical this wave; .ll artifacts are pre-emitted (regen.sh)
# and rsync'd in. Expects /root/bench populated from the workstation:
#   rsync benches/matmul/ -> /root/bench/
#   rsync crates/mapal-rt/src/lib.rs -> /root/bench/mapal_rt.rs
set -e
export DEBIAN_FRONTEND=noninteractive
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustc >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
  . "$HOME/.cargo/env"
fi
# clang >= 15 REQUIRED (opaque-pointer `ptr` syntax; S21 gotcha).
if ! clang --version 2>/dev/null | grep -qE "version (1[5-9]|[2-9][0-9])"; then
  apt-get update -qq && apt-get install -y -qq clang-15 python3-pip >/dev/null
  ln -sf /usr/bin/clang-15 /usr/local/bin/clang
  ln -sf /usr/bin/clang++-15 /usr/local/bin/clang++
fi
python3 -c 'import numpy' 2>/dev/null || pip3 install -q numpy >/dev/null 2>&1 || true
nproc; grep -m1 "model name" /proc/cpuinfo || true
cd /root/bench
bash runner.sh 2>&1 | tee /root/runner.log | tail -60
echo "S24 BOX DONE"
