#!/usr/bin/env bash
# Regenerate every checked-in emitted artifact from its .flow source through
# the optimizer (S22 item 1: bench legs measure the FULL pipeline). For each
# stem: <stem>.ll and <stem>.cu via `emit -- <src> --rewrite`, plus
# <stem>_perf.cu (`--rewrite --perf`) where one is checked in. Only artifact
# kinds that already exist regenerate — no new files are invented.
# ponytail: sequential; parallelize if the corpus outgrows a coffee break.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo build -q --release -p flow-backend-llvm -p flow-backend-cuda --examples

for f in benches/matmul/*.flow; do
  stem="${f%.flow}"
  if [ -f "$stem.ll" ]; then
    cargo run -q --release -p flow-backend-llvm --example emit -- "$f" --rewrite
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
