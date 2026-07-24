#!/bin/bash
# tile_ab.sh — repeatable local tile A/B (S26 WP4; the S25 A/B was ad-hoc).
#
# Usage: tile_ab.sh <file.flow> <label> [runs=3]
#
# Given a .flow file + label: emit tiled, --no-pack, --no-tile, and --contract
# .ll (plain and --perf variants) into target/tmp/tile_ab/<label>/ via the emit example with `-` +
# stdout redirect (the emit example's output-file naming keys on --perf only,
# so `-` avoids the `_perf.ll` collision), compile with
# `clang -O2 -march=native -ffp-contract=fast` against the flow-rt staticlib,
# run everything at FLOW_PAR=1, assert tiled stdout byte-equal to untiled,
# check contracted output numerically, and report min-of-N `FLOW_PERF total
# ms=` per side.
set -euo pipefail

FLOW_FILE="${1:?usage: tile_ab.sh <file.flow> <label> [runs]}"
LABEL="${2:?usage: tile_ab.sh <file.flow> <label> [runs]}"
RUNS="${3:-3}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
case "$FLOW_FILE" in /*) ;; *) FLOW_FILE="$PWD/$FLOW_FILE" ;; esac
[ -f "$FLOW_FILE" ] || { echo "no such file: $FLOW_FILE" >&2; exit 1; }

TMP="$ROOT/target/tmp/tile_ab/$LABEL"
mkdir -p "$TMP"
RT="$ROOT/target/release/libflow_rt.a"

# Big-N allocas live on the stack; macOS rejects `ulimit -s unlimited` (soft
# stays 8176K), so fall back to the hard limit — runner.py's proven recipe.
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true
export FLOW_PAR=1

echo "== tile A/B: $LABEL =="
echo "file:   $FLOW_FILE"
echo "clang:  $(clang --version | head -1)"
echo "tmp:    $TMP"

cargo build -q -p flow-rt --release --manifest-path "$ROOT/Cargo.toml"

emit() { # <out.ll> [flags...]
  local out="$1"; shift
  (cd "$ROOT" && cargo run -q --release -p flow-backend-llvm --example emit -- \
    "$FLOW_FILE" - --rewrite "$@") > "$out"
}

build() { # <ll> <bin>
  clang -O2 -march=native -ffp-contract=fast "$1" "$RT" -o "$2" -lpthread -ldl -lm
}

echo "-- emit + build"
emit "$TMP/tile.ll"
emit "$TMP/nopack.ll"        --no-pack
emit "$TMP/notile.ll"        --no-tile
emit "$TMP/fma.ll"           --contract
emit "$TMP/tile_perf.ll"     --perf
emit "$TMP/nopack_perf.ll"   --perf --no-pack
emit "$TMP/notile_perf.ll"   --perf --no-tile
emit "$TMP/fma_perf.ll"      --perf --contract
build "$TMP/tile.ll"        "$TMP/tile_bin"
build "$TMP/nopack.ll"      "$TMP/nopack_bin"
build "$TMP/notile.ll"      "$TMP/notile_bin"
build "$TMP/fma.ll"         "$TMP/fma_bin"
build "$TMP/tile_perf.ll"   "$TMP/tile_perf_bin"
build "$TMP/nopack_perf.ll" "$TMP/nopack_perf_bin"
build "$TMP/notile_perf.ll" "$TMP/notile_perf_bin"
build "$TMP/fma_perf.ll"    "$TMP/fma_perf_bin"

echo "-- correctness (stdout byte-equal, R1)"
"$TMP/tile_bin"   > "$TMP/tile.out"
"$TMP/nopack_bin" > "$TMP/nopack.out"
"$TMP/notile_bin" > "$TMP/notile.out"
if ! cmp -s "$TMP/tile.out" "$TMP/nopack.out"; then
  echo "FAIL: packed stdout != --no-pack stdout" >&2
  diff "$TMP/tile.out" "$TMP/nopack.out" >&2 || true
  exit 1
fi
if ! cmp -s "$TMP/tile.out" "$TMP/notile.out"; then
  echo "FAIL: tiled stdout != untiled stdout:" >&2
  diff "$TMP/tile.out" "$TMP/notile.out" >&2 || true
  exit 1
fi

# The check catches wrong results (index/reorder bugs produce garbage), not a
# tight rounding bound: error is measured against max(|expected|, 1) so a cell
# that nearly cancels (|sum| << the terms) can't false-fail on contraction
# noise. f64 gets 1e-9, not machine-eps class — K=4096 accumulation is ~K·eps
# before cancellation. Default is the loose f32 bound unless proven f64.
if grep -q 'widen_f64' "$FLOW_FILE"; then
  REL_TOL=1e-9
else
  REL_TOL=1e-4
fi

compare_numeric() { # <ref.out> <got.out>
  python3 - "$1" "$2" "$REL_TOL" <<'PY'
import math
import sys

ref = [float(value) for value in open(sys.argv[1]).read().split()]
got = [float(value) for value in open(sys.argv[2]).read().split()]
tol = float(sys.argv[3])
if len(ref) != len(got):
    raise SystemExit(f"FAIL: numeric output count differs: {len(ref)} != {len(got)}")
for i, (expected, actual) in enumerate(zip(ref, got), 1):
    rel = abs(actual - expected) / max(abs(expected), 1.0)
    if not math.isfinite(rel) or rel > tol:
        raise SystemExit(
            f"FAIL: value {i}: expected {expected}, got {actual}, rel-error {rel} > {tol}"
        )
PY
}

"$TMP/fma_bin" > "$TMP/fma.out"
compare_numeric "$TMP/tile.out" "$TMP/fma.out"

objdump -d "$TMP/tile_bin" > "$TMP/tile.disasm"
objdump -d "$TMP/fma_bin" > "$TMP/fma.disasm"
if grep -Eiq '(^|[^[:alnum:]_])(fmla|fmadd|vfmadd)' "$TMP/tile.disasm"; then
  echo "FAIL: plain tiled binary contains fused operations" >&2
  exit 1
fi
if ! grep -Eiq '(^|[^[:alnum:]_])(fmla|fmadd|vfmadd)' "$TMP/fma.disasm"; then
  echo "FAIL: fma binary contains no fused operations" >&2
  exit 1
fi
if grep -Eiq 'fmul\.4s|fmul\.2d|vmulps|vmulpd' "$TMP/fma.disasm"; then
  echo "FAIL: fma binary contains unfused vector multiply" >&2
  exit 1
fi

time_side() { # <bin> <ref.out> [numeric] -> prints "min r1 r2 ... rN"
  local bin="$1" ref="$2" numeric="${3:-}" ms all=""
  for _ in $(seq 1 "$RUNS"); do
    "$bin" > "$TMP/.run.out"
    grep -v '^FLOW_PERF' "$TMP/.run.out" > "$TMP/.run.stripped"
    if [ "$numeric" = numeric ]; then
      compare_numeric "$ref" "$TMP/.run.stripped"
    elif ! cmp -s "$TMP/.run.stripped" "$ref"; then
      echo "FAIL: $bin stdout (sans FLOW_PERF) != reference:" >&2
      diff "$ref" "$TMP/.run.stripped" >&2 || true
      exit 1
    fi
    ms="$(sed -n 's/^FLOW_PERF total ms=//p' "$TMP/.run.out" | head -1)"
    [ -n "$ms" ] || { echo "FAIL: no FLOW_PERF total in $bin output" >&2; exit 1; }
    all="$all $ms"
  done
  # shellcheck disable=SC2086
  echo "$(printf '%s\n' $all | sort -g | head -1)$all"
}

TILE_TIMES="$(time_side "$TMP/tile_perf_bin" "$TMP/tile.out")"
NOPACK_TIMES="$(time_side "$TMP/nopack_perf_bin" "$TMP/nopack.out")"
NOTILE_TIMES="$(time_side "$TMP/notile_perf_bin" "$TMP/notile.out")"
FMA_TIMES="$(time_side "$TMP/fma_perf_bin" "$TMP/tile.out" numeric)"
read -r TILE_MIN TILE_ALL <<< "$TILE_TIMES"
read -r NOPACK_MIN NOPACK_ALL <<< "$NOPACK_TIMES"
read -r NOTILE_MIN NOTILE_ALL <<< "$NOTILE_TIMES"
read -r FMA_MIN FMA_ALL <<< "$FMA_TIMES"

echo "-- results (FLOW_PERF total ms, min-of-$RUNS, FLOW_PAR=1)"
printf '%-10s %12s   runs:%s\n' "tile"   "$TILE_MIN"   "$(printf ' %s' $TILE_ALL)"
printf '%-10s %12s   runs:%s\n' "no-pack" "$NOPACK_MIN" "$(printf ' %s' $NOPACK_ALL)"
printf '%-10s %12s   runs:%s\n' "no-tile" "$NOTILE_MIN" "$(printf ' %s' $NOTILE_ALL)"
printf '%-10s %12s   runs:%s\n' "fma" "$FMA_MIN" "$(printf ' %s' $FMA_ALL)"
awk -v t="$TILE_MIN" -v n="$NOTILE_MIN" 'BEGIN {
  if (t+0 > 0) printf "speedup    %11.2fx   (tile vs no-tile)\n", n/t
}'
echo "stdout: tiled == untiled byte-equal (plain + $RUNS perf runs/side stripped) — OK"
echo "nopack: stdout byte-equal to tiled (plain + $RUNS perf runs stripped) — OK"
echo "fma:    rel-error <= $REL_TOL vs tiled; fused and no unfused vector mul — OK"
echo "out:    $(tr '\n' '/' < "$TMP/tile.out")"
