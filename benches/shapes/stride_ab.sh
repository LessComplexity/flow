#!/bin/bash
# S44 predictor test 2: does the L1 set-index collapse fire in the REAL EMITTED
# conv2d, on the row stride alone?
#
# Two Mapal sources that are identical in every respect except the image row
# stride — same output count (1022x1022), same 9 taps, same generation law:
#   conv2d_s1024.mapal  stride 1024 -> 4096 B tap-row step = 32 * 128 -> 4 sets
#   conv2d_s1026.mapal  stride 1026 -> 4104 B tap-row step, 128 does not divide
#                                   -> all 128 sets
# The predictor says arm A collapses and arm B does not. Written down first.
#
# GATE, before any timing: the two emitted `.ll` files must differ ONLY in the
# stride constants and the image extent. If they differ structurally, the two
# arms are not one experiment and the run is void.
#
# The arms are ALTERNATED inside one session (rule 14) and each cycle also runs a
# NULL CONTROL — the shipped `saxpy_1048576.mapal`, whose walk is sequential and
# which the predictor says cannot move. If the control moves, the run is VOID
# (rule 22).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/s44"
CYCLES="${CYCLES:-11}"
MAPAL_PAR="${MAPAL_PAR:-1}"
RT="$ROOT/target/release/libmapal_rt.a"
export MAPAL_PAR

mkdir -p "$TMP"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

emit() { (cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
            "$1" - --rewrite) > "$2"; }

echo "== conv2d row-stride A/B (MAPAL_PAR=$MAPAL_PAR, $CYCLES cycles) =="
for arm in s1024 s1026; do
    emit "$ROOT/benches/shapes/conv2d_$arm.mapal" "$TMP/conv2d_$arm.ll"
    clang -O2 -march=armv8-a+sme2 "$TMP/conv2d_$arm.ll" "$RT" \
        -o "$TMP/conv2d_$arm" -lpthread -ldl -lm 2>/dev/null
done
emit "$ROOT/benches/shapes/saxpy_1048576.mapal" "$TMP/saxpy_ctl.ll"
clang -O2 -march=armv8-a+sme2 "$TMP/saxpy_ctl.ll" "$RT" \
    -o "$TMP/saxpy_ctl" -lpthread -ldl -lm 2>/dev/null

# Structural gate. NOT a line count — the conv rung unrolls 9 taps at
# compile-time offsets, so ~330 lines legitimately carry a different literal.
# The real question is whether anything but a NUMBER moved: normalise every
# integer literal to `N` and require the two files to be identical. That admits
# exactly the intended difference (extent + stride constants) and rejects any
# structural change — a different instruction, a different unroll, a different
# rung firing on one arm and not the other.
raw=$(diff "$TMP/conv2d_s1024.ll" "$TMP/conv2d_s1026.ll" | grep -c '^[<>]' || true)
sed 's/[0-9][0-9]*/N/g' "$TMP/conv2d_s1024.ll" > "$TMP/a.norm"
sed 's/[0-9][0-9]*/N/g' "$TMP/conv2d_s1026.ll" > "$TMP/b.norm"
norm=$(diff "$TMP/a.norm" "$TMP/b.norm" | grep -c '^[<>]' || true)
echo "emitted .ll: $raw lines differ raw, $norm differ with integer literals normalised"
[ "$norm" -eq 0 ] || { echo "VOID: the arms differ structurally, not only in the stride" >&2; exit 1; }

stat_of() {
    awk '/iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0 }
         END { if (!n) { print "n/a"; exit }
               for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                   while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }; v[j+1] = x }
               printf "%.4f %.4f %.4f\n", v[0], v[int(n/2)], v[n-1] }'
}

: > "$TMP/a.log"; : > "$TMP/b.log"; : > "$TMP/c.log"
"$TMP/conv2d_s1024" > /dev/null   # warm the clock on real work
for ((c = 0; c < CYCLES; ++c)); do
    "$TMP/conv2d_s1024" | grep '^iter ms=' >> "$TMP/a.log"
    "$TMP/conv2d_s1026" | grep '^iter ms=' >> "$TMP/b.log"
    "$TMP/saxpy_ctl"    | grep '^iter ms=' >> "$TMP/c.log"
done

printf '%-22s %10s %10s %10s\n' arm min median max
printf '%-22s %10s %10s %10s\n' "conv2d stride 1024" $(stat_of < "$TMP/a.log")
printf '%-22s %10s %10s %10s\n' "conv2d stride 1026" $(stat_of < "$TMP/b.log")
printf '%-22s %10s %10s %10s\n' "saxpy (null control)" $(stat_of < "$TMP/c.log")
