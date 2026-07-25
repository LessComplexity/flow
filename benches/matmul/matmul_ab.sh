#!/bin/bash
# Full local matmul comparison: Flow against every CPU baseline, 512..4096, f32+f64.
#
# COMPUTE-ONLY on both sides. The Flow legs bracket their kernel map with the
# `time` builtin and print `iter ms=` (S30 — FLOW_PERF is retired here; it timed
# all of flow_main, data generation included, while every baseline times the
# kernel alone). Baselines print `<name> N=.. <ms> ms ..` and are given the same
# iteration count. min-of-RUNS per cell.
#
# Usage: matmul_ab.sh [sizes] [runs]     e.g. matmul_ab.sh "512 1024 2048 4096" 3
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/matmul_ab"; mkdir -p "$TMP"
SIZES="${1:-512 1024 2048 4096}"
RUNS="${2:-3}"
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libflow_rt.a"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

echo "== matmul A/B (compute-only, min-of-$RUNS) =="
echo "clang:  $(clang --version | head -1)"
echo "rustc:  $(rustc --version)"
echo "python: $("$PYTHON" --version 2>&1)"
echo "sizes:  $SIZES"

cargo build -q -p flow-rt --release --manifest-path "$ROOT/Cargo.toml"
clang++ -std=c++17 -O3 -march=native -ffp-contract=fast "$ROOT/benches/matmul/cpp_naive.cpp" -o "$TMP/cpp_1t"
clang++ -std=c++17 -O3 -march=native -ffp-contract=fast "$ROOT/benches/matmul/cpp_mt.cpp"    -o "$TMP/cpp_mt" -pthread
rustc -O -C target-cpu=native "$ROOT/benches/matmul/rust_naive.rs" -o "$TMP/rust_1t" 2>/dev/null
rustc -O -C target-cpu=native "$ROOT/benches/matmul/rust_mt.rs"    -o "$TMP/rust_mt" 2>/dev/null

emit() { ( cd "$ROOT" && cargo run -q --release -p flow-backend-llvm --example emit -- "$1" - --rewrite "${@:2}" ); }
flow_ms() { # <binary> ; min-of-RUNS of the program's own `iter ms=`
  local best="" v
  for _ in $(seq "$RUNS"); do
    v=$("$1" 2>/dev/null | sed -n 's/^iter ms=//p')
    [ -n "$v" ] && best=$("$PYTHON" -c "print(min($v,${best:-$v}))")
  done
  echo "${best:-FAIL}"
}
base_ms() { # <cmd...> ; the baselines print `<name> N=.. <ms> ms ..`
  local best="" v
  for _ in $(seq "$RUNS"); do
    v=$("$@" 2>/dev/null | sed -n 's/.* N=[0-9]* \([0-9.]*\) ms.*/\1/p' | head -1)
    [ -n "$v" ] && best=$("$PYTHON" -c "print(min($v,${best:-$v}))")
  done
  echo "${best:-FAIL}"
}

printf '\n%-6s %-5s %-16s %12s\n' size width leg "min ms"
printf '%-6s %-5s %-16s %12s\n' ------ ----- ---------------- ------------
for N in $SIZES; do
  for W in f32 f64; do
    SUF=""; [ "$W" = f32 ] && SUF="_f32"
    SRC="$ROOT/benches/matmul/matmul${N}_cap${SUF}.flow"
    [ -f "$SRC" ] || continue
    emit "$SRC"            > "$TMP/${N}${W}_conf.ll" || continue
    emit "$SRC" --contract > "$TMP/${N}${W}_fma.ll"  || continue
    for leg in conf fma; do
      clang -O2 -march=native -ffp-contract=fast "$TMP/${N}${W}_${leg}.ll" "$RT" \
        -o "$TMP/${N}${W}_${leg}" -lpthread -ldl -lm 2>/dev/null
    done
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "flow-conf-par" "$(flow_ms "$TMP/${N}${W}_conf")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "flow-fma-par"  "$(flow_ms "$TMP/${N}${W}_fma")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "flow-conf-1t"  "$(FLOW_PAR=1 flow_ms "$TMP/${N}${W}_conf")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "flow-fma-1t"   "$(FLOW_PAR=1 flow_ms "$TMP/${N}${W}_fma")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "cpp-1t"        "$(base_ms "$TMP/cpp_1t" "$N" 1 "$W")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "cpp-mt"        "$(base_ms "$TMP/cpp_mt" "$N" 1 "$W")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "rust-1t"       "$(base_ms "$TMP/rust_1t" "$N" 1 "$W")"
    printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "rust-mt"       "$(base_ms "$TMP/rust_mt" "$N" 1 "$W")"
    [ "$W" = f32 ] && printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "numpy-1t" \
      "$(base_ms env VECLIB_MAXIMUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 "$PYTHON" "$ROOT/benches/matmul/numpy_bench.py" "$N" 1)"
    [ "$W" = f32 ] && printf '%-6s %-5s %-16s %12s\n' "$N" "$W" "numpy-threaded" \
      "$(base_ms "$PYTHON" "$ROOT/benches/matmul/numpy_bench.py" "$N" 1)"
  done
done
