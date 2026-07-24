#!/bin/bash
# S27 box driver (trimmed, CPU-only): FMA contraction (product face) + BLAS
# rung 3 packing, measured at the S26c 4096-minimum sizes. Legs (compute-timed
# only — S28 rule): flow cap f32/f64 par+1t + their _fma twins, cpp naive+mt,
# rust naive+mt, chapel mc+1t, numpy threaded+1t. No cuda legs (=> any minimal
# ubuntu:22.04 image; the S27 .cu artifacts are byte-identical to S26's).
# Protocol as s26b_box.sh: rsync benches/matmul -> /root/bench, flow-rt
# lib.rs -> /root/bench/flow_rt.rs; runner.py stamps machine specs on the CSV.
# S27 vs s26b: fma builds (contract flags live IN the _fma.ll — same clang
# recipe), sizes to 4096 (ulimit probe below; runner wraps every flow run),
# disasm checks split by face (conformance: zero vfmadd; fma: zero vmulps).
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
echo "== S27 ulimit probe (4096 f64 ~700 MB stack: 3 arrays + packed panel) =="
ulimit -s unlimited && echo "ulimit -s unlimited: OK" || echo "ulimit -s unlimited: REFUSED — 4096 flow legs will show the wall (heap lowering is the recorded enabler)"
cd /root/bench
echo "== flow-rt staticlib =="
rustc --edition 2024 --crate-type=staticlib -O flow_rt.rs -o libflow_rt.a
echo "== flow-llvm builds (clang -O2 -march=native -ffp-contract=fast — conformance + fma faces, same flags; the flags differ IN the .ll) =="
pids=()
for n in 16 64 128 256 512 1024 2048 4096; do
  for v in "cap" "cap_f32"; do
    for stem in "matmul${n}_${v}:mm_ll_${v}_$n" \
                "matmul${n}_${v}_perf:mm_ll_perf_${v}_$n" \
                "matmul${n}_${v}_fma:mm_ll_fma_${v}_$n" \
                "matmul${n}_${v}_fma_perf:mm_ll_fma_perf_${v}_$n"; do
      src="${stem%%:*}.ll"; bin="${stem##*:}"
      if [ -f "$src" ] && [ ! -f "$bin" ]; then
        clang -O2 -march=native -ffp-contract=fast "$src" libflow_rt.a -o "$bin" -lpthread -ldl -lm &
        pids+=($!)
      fi
    done
  done
done
rc=0; for p in "${pids[@]}"; do wait "$p" || rc=1; done
[ "$rc" -eq 0 ] || echo "WARNING: some clang builds failed (legs skip-with-reason)"
echo "== baseline builds (standing recipes) =="
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
echo "== S27 disasm checks: conformance face must have ZERO fused ops; fma face all-fused, zero unfused vmulps =="
CONF_FUSED=$(objdump -d mm_ll_cap_f32_512 | grep -cE "vfmadd[0-9]*ps" || true)
FMA_FUSED=$(objdump -d mm_ll_fma_cap_f32_512 | grep -cE "vfmadd[0-9]*ps" || true)
FMA_UNFUSED=$(objdump -d mm_ll_fma_cap_f32_512 | grep -cE "vmulps" || true)
echo "conformance vfmadd=$CONF_FUSED (expect 0) | fma vfmadd=$FMA_FUSED (expect >0) | fma vmulps=$FMA_UNFUSED (expect 0)"
objdump -d mm_ll_fma_cap_f32_512 | grep -m1 -oE "%[yz]mm[0-9]+" | head -1 || true
echo "== runs (leg filter — CPU tables only) =="
python3 runner.py \
  flow-llvm-cap-compute-f64 flow-llvm-cap-compute-f32 \
  flow-llvm-cap-compute-f64-1t flow-llvm-cap-compute-f32-1t \
  flow-llvm-cap-fma-compute-f64 flow-llvm-cap-fma-compute-f32 \
  flow-llvm-cap-fma-compute-f64-1t flow-llvm-cap-fma-compute-f32-1t \
  cpp-naive-f32 cpp-naive-f64 cpp-mt-f32 cpp-mt-f64 rust-naive rust-mt \
  chapel-f32 chapel-f64 chapel-1t-f32 chapel-1t-f64 numpy numpy-1t 2>&1 | tee /root/runner.log
echo "S27 BOX DONE"
