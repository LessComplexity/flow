#!/bin/bash
# S44: does the move-panel rung move the README's shape-ladder boundary?
#
# The README quotes transpose as one of exactly two shapes that "still go to
# C++ — the boundary of the claim", on a threaded row of Mapal 0.290 / C++ 0.26 /
# NumPy 0.83. Those are CARRIED numbers. Rule 19: a number that was never
# re-taken has never been checked. So every leg here — Mapal OFF, Mapal ON, C++
# 1t, C++ mt, NumPy — runs ALTERNATING IN ONE SESSION on the same machine, with
# values gated first.
#
# The C++ and NumPy legs come from `ladder2_baseline.cpp` / `ladder2_numpy.py`,
# i.e. the same code the published row was taken from, so this is a re-take and
# not a new benchmark.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/s44"
CYCLES="${CYCLES:-13}"
SIDE="${SIDE:-1024}"
B="${B:-16}"
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libmapal_rt.a"
SRC="$ROOT/benches/shapes/transpose_${SIDE}.mapal"

mkdir -p "$TMP"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

emit() { local out="$1"; shift
    (cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
        "$SRC" - --rewrite "$@") > "$out"; }
build() { clang -O2 -march=armv8-a+sme2 "$1" "$RT" -o "$2" -lpthread -ldl -lm 2>/dev/null; }

echo "== transpose ${SIDE}: move-panel vs the ladder baselines (B=$B, $CYCLES cycles) =="
emit "$TMP/vb_off.ll";                    build "$TMP/vb_off.ll" "$TMP/vb_off"
emit "$TMP/vb_on.ll" "--move-panel=$SIDE:$B"; build "$TMP/vb_on.ll" "$TMP/vb_on"
clang++ -std=c++17 -O3 -march=native -ffp-contract=fast \
    "$ROOT/benches/shapes/ladder2_baseline.cpp" -o "$TMP/vb_cpp" -pthread

# --- values first, always.
MAPAL_PAR=1 "$TMP/vb_off" | grep -v '^iter ms=' > "$TMP/vb.val"
for leg in "1 vb_on" "par vb_off" "par vb_on"; do
    set -- $leg
    MAPAL_PAR="$1" "$TMP/$2" | grep -v '^iter ms=' > "$TMP/vb.got"
    cmp -s "$TMP/vb.val" "$TMP/vb.got" || {
        echo "VALUE MISMATCH ($1 $2)" >&2; diff "$TMP/vb.val" "$TMP/vb.got" >&2; exit 1; }
done
echo "values: identical across OFF/ON at 1 thread and threaded ($(tr '\n' ' ' < "$TMP/vb.val"))"

stat_of() {
    awk '/iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0 }
         END { if (!n) { print "n/a n/a n/a"; exit }
               for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                   while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }; v[j+1] = x }
               printf "%.4f %.4f %.4f\n", v[0], v[int(n/2)], v[n-1] }'
}

legs=(off-1t on-1t off-par on-par cpp-1t cpp-mt numpy)
for l in "${legs[@]}"; do : > "$TMP/vb_$l.log"; done
MAPAL_PAR=par "$TMP/vb_on" > /dev/null    # warm

for ((c = 0; c < CYCLES; ++c)); do
    MAPAL_PAR=1   "$TMP/vb_off" | grep '^iter ms=' >> "$TMP/vb_off-1t.log"
    MAPAL_PAR=1   "$TMP/vb_on"  | grep '^iter ms=' >> "$TMP/vb_on-1t.log"
    MAPAL_PAR=par "$TMP/vb_off" | grep '^iter ms=' >> "$TMP/vb_off-par.log"
    MAPAL_PAR=par "$TMP/vb_on"  | grep '^iter ms=' >> "$TMP/vb_on-par.log"
    "$TMP/vb_cpp" transpose 1t 1 "$SIDE" | grep '^iter ms=' >> "$TMP/vb_cpp-1t.log"
    "$TMP/vb_cpp" transpose mt 1 "$SIDE" | grep '^iter ms=' >> "$TMP/vb_cpp-mt.log"
    "$PYTHON" "$ROOT/benches/shapes/ladder2_numpy.py" transpose 1 "$SIDE" \
        | grep '^iter ms=' >> "$TMP/vb_numpy.log"
done

printf '\n%-12s %10s %10s %10s\n' leg min median max
for l in "${legs[@]}"; do
    printf '%-12s %10s %10s %10s\n' "$l" $(stat_of < "$TMP/vb_$l.log")
done
