#!/bin/bash
# tile_ab.sh — repeatable local tile A/B (S26 WP4; the S25 A/B was ad-hoc).
#
# Usage: tile_ab.sh <file.flow> <label> [runs=3]
#
# Given a .flow file + label: emit tiled and --no-tile .ll (plain and --perf
# variants) into target/tmp/tile_ab/<label>/ via the emit example with `-` +
# stdout redirect (the emit example's output-file naming keys on --perf only,
# so `-` avoids the `_perf.ll` collision), compile with
# `clang -O2 -march=native -ffp-contract=fast` against the flow-rt staticlib,
# run everything at FLOW_PAR=1, assert tiled stdout byte-equal to untiled
# (plain runs, and every perf run with FLOW_PERF lines stripped — hard fail
# otherwise, R1), and report min-of-N `FLOW_PERF total ms=` per side.
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
emit "$TMP/notile.ll"        --no-tile
emit "$TMP/tile_perf.ll"     --perf
emit "$TMP/notile_perf.ll"   --perf --no-tile
build "$TMP/tile.ll"        "$TMP/tile_bin"
build "$TMP/notile.ll"      "$TMP/notile_bin"
build "$TMP/tile_perf.ll"   "$TMP/tile_perf_bin"
build "$TMP/notile_perf.ll" "$TMP/notile_perf_bin"

echo "-- correctness (stdout byte-equal, R1)"
"$TMP/tile_bin"   > "$TMP/tile.out"
"$TMP/notile_bin" > "$TMP/notile.out"
if ! cmp -s "$TMP/tile.out" "$TMP/notile.out"; then
  echo "FAIL: tiled stdout != untiled stdout:" >&2
  diff "$TMP/tile.out" "$TMP/notile.out" >&2 || true
  exit 1
fi

time_side() { # <bin> <ref.out> -> prints "min r1 r2 ... rN"
  local bin="$1" ref="$2" ms all=""
  for _ in $(seq 1 "$RUNS"); do
    "$bin" > "$TMP/.run.out"
    grep -v '^FLOW_PERF' "$TMP/.run.out" > "$TMP/.run.stripped"
    if ! cmp -s "$TMP/.run.stripped" "$ref"; then
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

read -r TILE_MIN TILE_ALL <<< "$(time_side "$TMP/tile_perf_bin" "$TMP/tile.out")"
read -r NOTILE_MIN NOTILE_ALL <<< "$(time_side "$TMP/notile_perf_bin" "$TMP/notile.out")"

echo "-- results (FLOW_PERF total ms, min-of-$RUNS, FLOW_PAR=1)"
printf '%-10s %12s   runs:%s\n' "tile"   "$TILE_MIN"   "$(printf ' %s' $TILE_ALL)"
printf '%-10s %12s   runs:%s\n' "no-tile" "$NOTILE_MIN" "$(printf ' %s' $NOTILE_ALL)"
awk -v t="$TILE_MIN" -v n="$NOTILE_MIN" 'BEGIN {
  if (t+0 > 0) printf "speedup    %11.2fx   (tile vs no-tile)\n", n/t
}'
echo "stdout: tiled == untiled byte-equal (plain + $RUNS perf runs/side stripped) — OK"
echo "out:    $(tr '\n' '/' < "$TMP/tile.out")"
