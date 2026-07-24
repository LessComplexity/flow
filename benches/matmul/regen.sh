#!/usr/bin/env bash
# Regenerate every checked-in emitted artifact from its .flow source through
# the optimizer (S22 item 1: bench legs measure the FULL pipeline). For each
# stem: <stem>.ll and <stem>.cu via `emit -- <src> --rewrite`, plus
# <stem>_perf.cu (`--rewrite --perf`) where one is checked in, plus matching
# <stem>_perf.ll for cap sources that have both an LLVM and perf-CUDA artifact.
# ponytail: sequential; parallelize if the corpus outgrows a coffee break.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build -q --release -p flow-backend-llvm -p flow-backend-cuda --examples

# S27: llvm artifacts at 2048/4096 (the S26c 4096-minimum directive) — first
# generation; afterwards the keyed loop below maintains them like every stem.
for n in 2048 4096; do
  for v in cap cap_f32; do
    src="benches/matmul/matmul${n}_${v}.flow"
    stem="${src%.flow}"
    if [ -f "$src" ] && [ ! -f "$stem.ll" ]; then
      cargo run -q --release -p flow-backend-llvm --example emit -- "$src" - --rewrite > "$stem.ll"
      cargo run -q --release -p flow-backend-llvm --example emit -- "$src" - --rewrite --perf > "${stem}_perf.ll"
    fi
  done
done

for f in benches/matmul/*.flow; do
  stem="${f%.flow}"
  if [ -f "$stem.ll" ]; then
    cargo run -q --release -p flow-backend-llvm --example emit -- "$f" --rewrite
  fi
  # S27: product-face FMA twins (--contract) for every cap stem with an .ll —
  # the conformance artifacts above stay contraction-free (the differential
  # face); _fma.ll/_fma_perf.ll are the bench/product face (plan-s27).
  case "$stem" in
    *_cap|*_cap_f32)
      if [ -f "$stem.ll" ]; then
        cargo run -q --release -p flow-backend-llvm --example emit -- "$f" - --rewrite --contract > "${stem}_fma.ll"
        cargo run -q --release -p flow-backend-llvm --example emit -- "$f" - --rewrite --contract --perf > "${stem}_fma_perf.ll"
      fi
      ;;
  esac
  if [ -f "$stem.ll" ] && [ -f "${stem}_perf.cu" ]; then
    cargo run -q --release -p flow-backend-llvm --example emit -- "$f" --rewrite --perf
  fi
  if [ -f "$stem.cu" ]; then
    cargo run -q --release -p flow-backend-cuda --example emit -- "$f" --rewrite
  fi
  if [ -f "${stem}_perf.cu" ]; then
    cargo run -q --release -p flow-backend-cuda --example emit -- "$f" --rewrite --perf - >"${stem}_perf.cu"
  fi
  echo "regen: $stem"
done
echo "done."
