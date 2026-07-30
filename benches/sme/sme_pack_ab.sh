#!/bin/bash
# One-off S42 diagnostic: the SME rung WITH its existing B pack vs WITHOUT it.
# Same source, same target, same face, same flags -- the ONLY difference is
# --no-pack, which flips emit_tiled_map_sme from the Some(packed) arm to None.
# Verified from the emitted call: default (bn=16, bj=t*k=16384) is packed,
# --no-pack (bn=b.ck=1024, bj=t=16) is not.
#
# This exists because bp.c measured B packing worth 1.885x at 2048 in a HAND
# kernel, and the emitter already does it -- so this checks the emitter gets the
# same win, i.e. that bp.c's control was the hand kernel's gap, not Mapal's.
#
# Method follows benches/sme/sme_ab.sh: contract face on both legs, value
# identity checked BEFORE any timing, alternating runs, medians and minima,
# absolute ms with the baseline commit named. MAPAL_PAR=1 for one thread.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/sme_pack_ab"; mkdir -p "$TMP"
SRCS="${1:-benches/matmul/matmul512_cap_f32.mapal benches/matmul/matmul1024_cap_f32.mapal benches/matmul/matmul2048_cap_f32.mapal}"
RUNS="${2:-21}"
CFLAGS="-O2 -march=armv8-a+sme2"
RT="$ROOT/target/release/libmapal_rt.a"
export MAPAL_PAR=1

echo "== SME: B packed vs B unpacked, 1 thread =="
echo "cflags: $CFLAGS"
echo "baseline commit: $(git -C "$ROOT" rev-parse --short HEAD)$(git -C "$ROOT" diff --quiet || echo ' +dirty')"
echo "runs:   $RUNS alternating, MAPAL_PAR=1"
echo

cargo build -q -p mapal-rt --release --manifest-path "$ROOT/Cargo.toml" || exit 1

emit() { ( cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
  "$1" - --rewrite --contract --target=apple-m4-sme "${@:3}" ) > "$2"; }

for SRC in $SRCS; do
  NAME="$(basename "$SRC" .mapal)"
  echo "--- $NAME ---"
  emit "$SRC" "$TMP/$NAME.pk.ll"              || { echo "  emit(packed) FAILED"; continue; }
  emit "$SRC" "$TMP/$NAME.np.ll" --no-pack    || { echo "  emit(nopack) FAILED"; continue; }

  echo "  packed call: $(grep -oE 'mapal_sme_panel\(.*' "$TMP/$NAME.pk.ll" | head -1)"
  echo "  nopack call: $(grep -oE 'mapal_sme_panel\(.*' "$TMP/$NAME.np.ll" | head -1)"
  [ "$(grep -c 'sme.mopa' "$TMP/$NAME.pk.ll")" -gt 0 ] || { echo "  !! SME did not fire (packed)"; continue; }
  [ "$(grep -c 'sme.mopa' "$TMP/$NAME.np.ll")" -gt 0 ] || { echo "  !! SME did not fire (nopack)"; continue; }

  clang $CFLAGS "$TMP/$NAME.pk.ll" "$RT" -o "$TMP/$NAME.pk" 2>/dev/null || { echo "  link(pk) FAILED"; continue; }
  clang $CFLAGS "$TMP/$NAME.np.ll" "$RT" -o "$TMP/$NAME.np" 2>/dev/null || { echo "  link(np) FAILED"; continue; }

  VP=$("$TMP/$NAME.pk" 2>/dev/null | grep -v '^iter ms=')
  VN=$("$TMP/$NAME.np" 2>/dev/null | grep -v '^iter ms=')
  if [ "$VP" != "$VN" ]; then
    echo "  VALUE MISMATCH -- refusing to report timings."; echo "    pk: $VP"; echo "    np: $VN"; continue
  fi
  echo "  values identical: $VP"

  : > "$TMP/$NAME.series"
  for _ in $(seq "$RUNS"); do
    p=$("$TMP/$NAME.pk" 2>/dev/null | sed -n 's/^iter ms=//p')
    n=$("$TMP/$NAME.np" 2>/dev/null | sed -n 's/^iter ms=//p')
    echo "$p $n" >> "$TMP/$NAME.series"
  done

  python3 - "$TMP/$NAME.series" "$NAME" <<'PY'
import sys, statistics, re
rows = [l.split() for l in open(sys.argv[1]) if len(l.split()) == 2]
try:
    pk = sorted(float(a) for a, _ in rows); np_ = sorted(float(b) for _, b in rows)
except ValueError:
    print("  unparsable samples"); raise SystemExit
if not pk: print("  no samples"); raise SystemExit
N = int(re.search(r'matmul(\d+)', sys.argv[2]).group(1))
gf = lambda ms: 2.0*N*N*N/(ms*1e6)
med = statistics.median
print(f"  n={len(rows)}  N={N}")
print(f"  B packed    min {pk[0]:9.4f}  median {med(pk):9.4f} ms   {gf(med(pk)):7.1f} GFLOP/s")
print(f"  B unpacked  min {np_[0]:9.4f}  median {med(np_):9.4f} ms   {gf(med(np_)):7.1f} GFLOP/s")
print(f"  packing is worth {med(np_)/med(pk):.3f}x (median)")
print(f"  distributions {'OVERLAP -- treat with suspicion' if pk[-1] >= np_[0] else 'are disjoint'}")
PY
  echo
done
