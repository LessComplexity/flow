#!/bin/bash
# S44: the move-panel traversal, measured inside the REAL emitted pipeline.
#
# `benches/shapes/tblock.c` priced this standalone (rule 3: a probe prices, it
# does not settle). This runs the same sweep through the emitter, so the
# instruction stream, the task slicing and the runtime are the shipped ones.
#
# GATE ORDER, and it is not negotiable: values first, timing second. Every arm's
# full stdout minus the `iter ms=` line must be byte-equal to the OFF arm's
# before a single number is read. The transform is a permutation of the loop
# counter, so anything but equality is a bug, not a tolerance.
#
# CONTROLS (rule 22):
#  * `B = W` is the IDENTITY permutation — same visit order as OFF, through the
#    same arithmetic. It prices the transform's own overhead, and OFF-vs-identity
#    is the arm that must NOT move.
#  * `saxpy` runs back-to-back inside every cycle. Its emission is byte-identical
#    under the flag (it is a captures=0 map whose n is not a multiple of W here),
#    so it cannot move; if it does, the machine did, and the run is VOID.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/s44"
CYCLES="${CYCLES:-11}"
SHAPE="${SHAPE:-transpose}"
SIDE="${SIDE:-1024}"
N="${N:-1048576}"
BLOCKS="${BLOCKS:-8 16 24 32 64 128}"
RT="$ROOT/target/release/libmapal_rt.a"
export MAPAL_PAR="${MAPAL_PAR:-1}"

mkdir -p "$TMP"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

case "$SHAPE" in
    transpose) SRC="$ROOT/benches/shapes/transpose_${SIDE}.mapal"; W="$SIDE" ;;
    gather)    SRC="$ROOT/benches/shapes/gather_${N}.mapal";       W="$SIDE" ;;
    *)         echo "unknown SHAPE=$SHAPE" >&2; exit 2 ;;
esac

emit() { # <out.ll> [flags...]
    local out="$1"; shift
    (cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
        "$SRC" - --rewrite "$@") > "$out"
}
build() { clang -O2 -march=armv8-a+sme2 "$1" "$RT" -o "$2" -lpthread -ldl -lm 2>/dev/null; }

echo "== move-panel A/B: $SHAPE side=$SIDE W=$W MAPAL_PAR=$MAPAL_PAR cycles=$CYCLES =="
arms=(off)
emit "$TMP/mp_off.ll"; build "$TMP/mp_off.ll" "$TMP/mp_off"
for b in $BLOCKS $W; do
    emit "$TMP/mp_$b.ll" "--move-panel=$W:$b"; build "$TMP/mp_$b.ll" "$TMP/mp_$b"
    arms+=("$b")
done
emit "$TMP/ctl.ll"   # the null control is a different program entirely
(cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
    "$ROOT/benches/shapes/saxpy_1048576.mapal" - --rewrite) > "$TMP/ctl.ll"
build "$TMP/ctl.ll" "$TMP/ctl"

# --- GATE 1: values, before any timing.
"$TMP/mp_off" | grep -v '^iter ms=' > "$TMP/mp_off.val"
for a in "${arms[@]:1}"; do
    "$TMP/mp_$a" | grep -v '^iter ms=' > "$TMP/mp_$a.val"
    cmp -s "$TMP/mp_off.val" "$TMP/mp_$a.val" || {
        echo "VALUE MISMATCH at B=$a:" >&2; diff "$TMP/mp_off.val" "$TMP/mp_$a.val" >&2; exit 1; }
done
echo "values: identical to OFF at every arm ($(tr '\n' ' ' < "$TMP/mp_off.val"))"

# --- GATE 2: the transform must be VISIBLE. An arm whose .ll equals OFF's is a
# declined gate being reported as a treatment — the failure mode that would make
# every "no effect" reading meaningless.
for a in "${arms[@]:1}"; do
    if cmp -s "$TMP/mp_off.ll" "$TMP/mp_$a.ll"; then
        echo "VOID: B=$a emitted the same text as OFF — the rung declined, it did not fire" >&2
        exit 1
    fi
done
echo "emission: every arm differs from OFF (the rung fired)"

stat_of() {
    awk '/iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0 }
         END { if (!n) { print "n/a n/a n/a"; exit }
               for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                   while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }; v[j+1] = x }
               printf "%.4f %.4f %.4f\n", v[0], v[int(n/2)], v[n-1] }'
}

for a in "${arms[@]}"; do : > "$TMP/t_$a.log"; done
: > "$TMP/t_ctl.log"
"$TMP/mp_off" > /dev/null   # warm the clock on real work
for ((c = 0; c < CYCLES; ++c)); do
    for a in "${arms[@]}"; do
        [ "$a" = off ] && bin="$TMP/mp_off" || bin="$TMP/mp_$a"
        "$bin" | grep '^iter ms=' >> "$TMP/t_$a.log"
    done
    "$TMP/ctl" | grep '^iter ms=' >> "$TMP/t_ctl.log"
done

printf '\n%-12s %10s %10s %10s\n' arm min median max
for a in "${arms[@]}"; do
    label="$a"; [ "$a" = "$W" ] && label="$a (identity)"
    printf '%-12s %10s %10s %10s\n' "$label" $(stat_of < "$TMP/t_$a.log")
done
printf '%-12s %10s %10s %10s\n' "saxpy null" $(stat_of < "$TMP/t_ctl.log")
