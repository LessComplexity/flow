#!/bin/bash
# SME `nc` sweep (plan-s43-nc-blocking §6): the SAME source emitted once per
# B-panel block width, compiled identically, run round-robin.
#
# METHOD (the standing measurement rules):
#  - SWEEP, never one point (rule 4). `sme_kc` returned 512, four write-ups
#    concluded "KC loses", and the sweep showed 512 was two steps down a sharp
#    curve. The knee this level targets (8-12 MB, `benches/sme/loadlevel.c`) is
#    INSIDE the swept range by construction.
#  - VALUE IDENTITY FIRST, against the NEON leg, and the script refuses to print
#    a timing if any arm disagrees. Unlike the window instrument, a mismatch here
#    is a defect: `nc` does not split k, so every output block is still written
#    exactly once and the values are bit-identical by construction.
#  - ROUND-ROBIN with the order ROTATED per cycle, after a discarded warm-up
#    cycle, so the cold-clock ramp is paid symmetrically (rule 1: 1.73x
#    cold-vs-warm on identical code).
#  - ABSOLUTE MILLISECONDS, min/median/max, with an explicit overlap statement
#    against the nc-off arm (rule 6: under ~6% on this unpinned Mac is noise).
#  - A ZERO-EFFECT CONTROL ARM. `ctl` is compiled from the SAME `.ll` as `off`,
#    so the two binaries are byte-identical and `ctl` MUST track `off`. If it
#    does not, something on the swept axis is moving that is not the parameter —
#    clock drift, thermal ramp, another agent on the machine — and THE WHOLE RUN
#    IS VOID. This is the failure that put S42's 1864 GF/s L1 ceiling into the
#    record; a control costs one arm and catches it by construction.
#  - EVERY timed run goes through benches/perflock.sh. Invoke this script THROUGH
#    it: `benches/perflock.sh benches/sme/nc_sweep.sh ...`
#
# Usage: nc_sweep.sh <src.mapal> <runs> <nc list...>
#        MAPAL_PAR=1 for the one-thread leg; unset for the pool.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/nc_sweep"; mkdir -p "$TMP"
SRC="${1:?usage: nc_sweep.sh <src.mapal> <runs> <nc...>}"
RUNS="${2:?}"
shift 2
NCS=("$@")
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libmapal_rt.a"
EMIT="$ROOT/target/release/examples/emit"

# The M4 has SME but NOT full SVE; `armv9-a` implies +sve and the binary SIGILLs.
CFLAGS="-O2 -march=armv8-a+sme2"
NAME="$(basename "$SRC" .mapal)"

echo "== SME nc sweep: $NAME =="
echo "clang:   $(clang --version | head -1)"
echo "cflags:  $CFLAGS"
echo "threads: MAPAL_PAR=${MAPAL_PAR:-<pool default>}"
echo "runs:    $RUNS round-robin cycles (+1 discarded warm-up)"
echo "commit:  $(git -C "$ROOT" rev-parse --short HEAD)$(git -C "$ROOT" diff --quiet || echo ' +dirty')"

# --- arms: the NEON reference (value gate only), sme nc-off, sme nc=X
ARMS=(off ctl "${NCS[@]}")
build() { # <arm> ; "neon" | "off" | "ctl" | <nc>
  local arm=$1 ll="$TMP/$NAME.$1.ll" bin="$TMP/$NAME.$1"
  case "$arm" in
    neon)      "$EMIT" "$SRC" - --rewrite --contract > "$ll" ;;
    off|ctl)   "$EMIT" "$SRC" - --rewrite --contract --target=apple-m4-sme > "$ll" ;;
    *)         "$EMIT" "$SRC" - --rewrite --contract --target=apple-m4-sme --nc="$arm" > "$ll" ;;
  esac || { echo "  emit($arm) FAILED"; exit 1; }
  clang $CFLAGS "$ll" "$RT" -o "$bin" 2>/dev/null || { echo "  link($arm) FAILED"; exit 1; }
}
build neon
for a in "${ARMS[@]}"; do build "$a"; done
cmp -s "$TMP/$NAME.off.ll" "$TMP/$NAME.ctl.ll" \
  || { echo "  control is not byte-identical to off — the control is broken"; exit 1; }
# An `nc` the rung cannot honour emits `off`'s bytes by design (no ragged final
# block, no partial panel). Say so LOUDLY: an arm silently measuring the baseline
# reads as "nc had no effect at this width", which is a different claim entirely.
# (This caught `nc=768` at c=4096 — 4096 is not a multiple of 768.)
for a in "${NCS[@]}"; do
  cmp -s "$TMP/$NAME.off.ll" "$TMP/$NAME.$a.ll" \
    && echo "  !! nc=$a REJECTED by the legality gate — this arm IS the baseline, not a measurement"
done

# The transformation must be findable in the emission (rule 2). nc-off and every
# nc arm call the SAME kernel; what differs is the nest around it.
echo "  kernel bytes identical across arms: $(for a in "${ARMS[@]}"; do
  sed -n '/define internal void @mapal_sme_panel(/,/^}/p' "$TMP/$NAME.$a.ll" | shasum -a 256 | cut -d' ' -f1
done | sort -u | wc -l | tr -d ' ') distinct hash(es) — must be 1"

# ---- value identity, BEFORE any timing ----
VREF=$("$TMP/$NAME.neon" 2>/dev/null | grep -v '^iter ms=')
for a in "${ARMS[@]}"; do
  v=$("$TMP/$NAME.$a" 2>/dev/null | grep -v '^iter ms=')
  if [ "$v" != "$VREF" ]; then
    echo "  VALUE MISMATCH on arm '$a' — refusing to report timings."
    echo "    neon: $VREF"
    echo "    $a  : $v"
    exit 1
  fi
done
echo "  values identical to the NEON leg on every arm: $(echo $VREF)"

# ---- round-robin timed runs, order rotated per cycle ----
: > "$TMP/$NAME.series"
n=${#ARMS[@]}
for c in $(seq 0 "$RUNS"); do          # cycle 0 is the discarded warm-up
  for k in $(seq 0 $((n - 1))); do
    a=${ARMS[$(((k + c) % n))]}
    ms=$("$TMP/$NAME.$a" 2>/dev/null | sed -n 's/^iter ms=//p')
    [ "$c" -gt 0 ] && echo "$a $ms" >> "$TMP/$NAME.series"
  done
done

"$PYTHON" - "$TMP/$NAME.series" <<'PY'
import sys, statistics, collections
rows = [l.split() for l in open(sys.argv[1]) if len(l.split()) == 2]
by = collections.OrderedDict()
for a, ms in rows:
    by.setdefault(a, []).append(float(ms))
base = sorted(by["off"])
bmed = statistics.median(base)
print(f"  n={len(base)} per arm")
print(f"  {'arm':>8} {'min':>10} {'median':>10} {'max':>10}  {'vs off':>8}  distributions")
for a, v in by.items():
    v = sorted(v)
    med = statistics.median(v)
    # disjoint iff the two [min,max] intervals do not intersect
    dis = v[-1] < base[0] or base[-1] < v[0]
    tag = "-" if a == "off" else ("disjoint" if dis else "OVERLAP")
    print(f"  {a:>8} {v[0]:10.4f} {med:10.4f} {v[-1]:10.4f}  {bmed/med:7.3f}x  {tag}")
print("  (rule 6: under ~6% on this unpinned Mac is noise; byte-identical")
print("   binaries have measured -5.9%..+1.2% apart.)")
# The control is a byte-identical copy of `off`. Anything it "measures" is drift.
ctl = sorted(by["ctl"])
drift = abs(statistics.median(ctl) / bmed - 1.0) * 100
print(f"  CONTROL (byte-identical to off): {drift:.2f}% from off — "
      + ("run is VOID, drift landed on the swept axis" if drift >= 6.0
         else "within the noise floor, run stands"))
PY
