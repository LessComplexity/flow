#!/bin/bash
# Local A/B for the FIR and conv2d Flow shapes plus C++, Rust, and NumPy baselines.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/shapes_ab"
RUNS="${RUNS:-3}"
PYTHON="${PYTHON:-python3}"
FLOW_PAR="${FLOW_PAR:-par}"
RT="$ROOT/target/release/libflow_rt.a"

case "$RUNS" in
    ""|*[!0-9]*) echo "RUNS must be an integer >= 3" >&2; exit 2 ;;
esac
[ "$RUNS" -ge 3 ] || { echo "RUNS must be >= 3" >&2; exit 2; }

mkdir -p "$TMP"
# Big generated arrays use stack allocations. macOS rejects "unlimited", so
# use the hard limit as tile_ab.sh does.
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true
export FLOW_PAR

echo "== shapes A/B =="
echo "clang:   $(clang --version | head -1)"
echo "rustc:   $(rustc --version)"
echo "python:  $("$PYTHON" --version 2>&1)"
echo "FLOW_PAR=$FLOW_PAR RUNS=$RUNS"
echo "tmp:     $TMP"

"$PYTHON" -c 'import numpy' >/dev/null
cargo build -q -p flow-rt --release --manifest-path "$ROOT/Cargo.toml"

echo "-- build baselines"
clang++ -std=c++17 -O3 -march=native -ffp-contract=fast \
    "$ROOT/benches/shapes/shapes_baseline.cpp" -o "$TMP/shapes_cpp" -pthread
rustc -O -C target-cpu=native \
    "$ROOT/benches/shapes/shapes_baseline.rs" -o "$TMP/shapes_rust"

emit() { # <source.flow> <out.ll> [flags...]
    local source="$1" out="$2"
    shift 2
    (cd "$ROOT" && cargo run -q --release -p flow-backend-llvm --example emit -- \
        "$source" - --rewrite "$@") > "$out"
}

build_flow() { # <in.ll> <out>
    clang -O3 -march=native -ffp-contract=fast "$1" "$RT" -o "$2" -lpthread -ldl -lm
}

echo "-- emit + build Flow legs"
for shape in fir conv2d; do
    source="$ROOT/benches/shapes/${shape}_$([ "$shape" = fir ] && echo 65536 || echo 512).flow"
    emit "$source" "$TMP/${shape}_conf.ll"
    emit "$source" "$TMP/${shape}_fma.ll" --contract
    emit "$source" "$TMP/${shape}_conf_perf.ll" --perf
    emit "$source" "$TMP/${shape}_fma_perf.ll" --perf --contract
    build_flow "$TMP/${shape}_conf.ll" "$TMP/${shape}_conf"
    build_flow "$TMP/${shape}_fma.ll" "$TMP/${shape}_fma"
    build_flow "$TMP/${shape}_conf_perf.ll" "$TMP/${shape}_conf_perf"
    build_flow "$TMP/${shape}_fma_perf.ll" "$TMP/${shape}_fma_perf"
done

compare_numeric() { # <reference> <actual>
    "$PYTHON" - "$1" "$2" <<'PY'
import math
import sys

ref = [float(value) for value in open(sys.argv[1]).read().split()]
got = [float(value) for value in open(sys.argv[2]).read().split()]
if len(ref) != len(got):
    raise SystemExit(f"FAIL: numeric output count differs: {len(ref)} != {len(got)}")
for index, (expected, actual) in enumerate(zip(ref, got), 1):
    rel = abs(actual - expected) / max(abs(expected), 1.0)
    if not math.isfinite(rel) or rel > 1e-4:
        raise SystemExit(
            f"FAIL: value {index}: expected {expected}, got {actual}, "
            f"rel-error {rel} > 0.0001"
        )
PY
}

check_exact() { # <reference> <actual> <label>
    if ! cmp -s "$1" "$2"; then
        echo "FAIL: $3 output differs from Flow conformance:" >&2
        diff "$1" "$2" >&2 || true
        exit 1
    fi
}

echo "-- verification references"
for shape in fir conv2d; do
    "$TMP/${shape}_conf" > "$TMP/${shape}_ref.out"
    "$TMP/${shape}_fma" > "$TMP/${shape}_fma.out"
    compare_numeric "$TMP/${shape}_ref.out" "$TMP/${shape}_fma.out"
done

time_flow() { # <shape> <conf|fma>
    local shape="$1" leg="$2" run out stripped ms count
    local values=()
    for ((run = 1; run <= RUNS; ++run)); do
        out="$TMP/${shape}_${leg}_perf.run"
        stripped="$TMP/${shape}_${leg}_perf.stripped"
        "$TMP/${shape}_${leg}_perf" > "$out"
        grep -v '^FLOW_PERF total ms=' "$out" > "$stripped"
        if [ "$leg" = fma ]; then
            compare_numeric "$TMP/${shape}_ref.out" "$stripped"
        else
            check_exact "$TMP/${shape}_ref.out" "$stripped" "$shape flow-$leg"
        fi
        count="$(grep -c '^FLOW_PERF total ms=' "$out" || true)"
        [ "$count" -eq 1 ] || { echo "FAIL: expected one FLOW_PERF line in $out" >&2; exit 1; }
        ms="$(sed -n 's/^FLOW_PERF total ms=//p' "$out")"
        values+=("$ms")
    done
    printf '%s\n' "${values[@]}" | sort -g | head -1
}

time_baseline() { # <shape> <leg> <command...>
    local shape="$1" leg="$2" out stripped count
    shift 2
    out="$TMP/${shape}_${leg}.run"
    stripped="$TMP/${shape}_${leg}.stripped"
    "$@" > "$out"
    grep -v '^iter ms=' "$out" > "$stripped"
    check_exact "$TMP/${shape}_ref.out" "$stripped" "$shape $leg"
    count="$(grep -c '^iter ms=' "$out" || true)"
    [ "$count" -eq "$RUNS" ] || {
        echo "FAIL: expected $RUNS iteration timings in $out, got $count" >&2
        exit 1
    }
    sed -n 's/^iter ms=//p' "$out" | sort -g | head -1
}

echo "-- timing"
rows=()
flow_mode="$FLOW_PAR"
[ "$flow_mode" = 1 ] && flow_mode=1t
for shape in fir conv2d; do
    rows+=("$shape|flow-conf-$flow_mode|$(time_flow "$shape" conf)")
    rows+=("$shape|flow-fma-$flow_mode|$(time_flow "$shape" fma)")
    rows+=("$shape|cpp-1t|$(time_baseline "$shape" cpp_1t "$TMP/shapes_cpp" "$shape" 1t "$RUNS")")
    rows+=("$shape|cpp-mt|$(time_baseline "$shape" cpp_mt "$TMP/shapes_cpp" "$shape" mt "$RUNS")")
    rows+=("$shape|rust-1t|$(time_baseline "$shape" rust_1t "$TMP/shapes_rust" "$shape" 1t "$RUNS")")
    rows+=("$shape|rust-mt|$(time_baseline "$shape" rust_mt "$TMP/shapes_rust" "$shape" mt "$RUNS")")
    rows+=("$shape|numpy-1t|$(time_baseline "$shape" numpy_1t env PYTHONDONTWRITEBYTECODE=1 \
        "$PYTHON" "$ROOT/benches/shapes/shapes_numpy.py" "$shape" --1t "$RUNS")")
done

echo
echo "-- results (min-of-$RUNS ms)"
printf '%-8s %-20s %12s\n' "shape" "leg" "min ms"
printf '%-8s %-20s %12s\n' "--------" "--------------------" "------------"
for row in "${rows[@]}"; do
    IFS='|' read -r shape leg ms <<< "$row"
    printf '%-8s %-20s %12s\n' "$shape" "$leg" "$ms"
done
echo "verification: baselines byte-equal; Flow FMA rel-error <= 1e-4 -- OK"
