#!/bin/bash
# S43 parallel B pack (plan-s43-parallel-bpack §6): the SAME source emitted by the
# base-commit emitter and by the changed emitter, compiled identically, run
# round-robin.
#
# METHOD (the standing measurement rules):
#  - A/B ACROSS TWO EMITTERS. This change ships no lever (plan §3.3) — the pack is
#    parallel or it is not — so the `off` arm is a binary emitted by `emit.before`,
#    stashed from the base commit. The before binary IS the off lever.
#  - SWEEP, never one point (rule 4). What IS free is the pack task's `oversub`,
#    the one number the emitter picks. The `on-oN` arms are the `on` emission with
#    exactly ONE immediate substituted — the `i32 <oversub>` field of the pack's
#    `mapal_par_task` registration. The script ASSERTS exactly one changed line per
#    swept arm and fails otherwise (resid_ab.sh's §6 lesson: assert the patch,
#    never merely print it).
#  - VALUE IDENTITY FIRST, and no timing is printed if any arm disagrees with
#    `off`. The pack writes the same bytes from disjoint sources in a different
#    interleaving, all of it before any reader launches; a mismatch is a defect.
#  - ROUND-ROBIN, order ROTATED per cycle, after a discarded warm-up cycle, so the
#    cold-clock ramp is paid symmetrically (rule 1).
#  - ABSOLUTE MILLISECONDS, min/median/max, explicit overlap statement vs `off`.
#  - A ZERO-EFFECT CONTROL ARM (rule 22). `ctl` is linked from `off`'s own `.ll`,
#    so the two binaries are byte-identical and `ctl` MUST track `off`. If it does
#    not, something on the swept axis is moving that is not the parameter and THE
#    WHOLE RUN IS VOID.
#  - THE GATE THAT WOULD MISLEAD: `packing_site` is geometry, not ISA, so this
#    change moves bytes on the NEON leg too. A byte-identical off/on emission here
#    means the packed branch never fired — the script treats that as FAILURE, not
#    as a clean control.
#  - EVERY timed run goes through benches/perflock.sh. Invoke this script THROUGH
#    it: `benches/perflock.sh benches/sme/bpack_sweep.sh ...`
#
# Usage: bpack_sweep.sh <src.mapal> <cycles> <sme|neon> [oversub list...]
#        MAPAL_PAR=1 for the one-thread leg; unset for the pool.
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/bpack"; mkdir -p "$TMP"
SRC="${1:?usage: bpack_sweep.sh <src.mapal> <cycles> <sme|neon> [oversub...]}"
RUNS="${2:?}"
LEG="${3:?}"
shift 3
SUBS=("$@")
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libmapal_rt.a"
BEFORE="$TMP/emit.before"
AFTER="$TMP/emit.after"

# The M4 has SME but NOT full SVE; `armv9-a` implies +sve and the binary SIGILLs.
CFLAGS="-O2 -march=armv8-a+sme2"
NAME="$(basename "$SRC" .mapal).$LEG"
case "$LEG" in
  sme)  TFLAG="--target=apple-m4-sme" ;;
  neon) TFLAG="" ;;
  *)    echo "leg must be sme or neon"; exit 2 ;;
esac

echo "== S43 parallel B pack: $NAME =="
echo "clang:   $(clang --version | head -1)"
echo "cflags:  $CFLAGS"
echo "leg:     $LEG ${TFLAG:-<default target>}"
echo "threads: MAPAL_PAR=${MAPAL_PAR:-<pool default>}"
echo "cycles:  $RUNS round-robin (+1 discarded warm-up)"
echo "commit:  $(git -C "$ROOT" rev-parse --short HEAD)$(git -C "$ROOT" diff --quiet || echo ' +dirty')"

[ -x "$BEFORE" ] || { echo "  missing $BEFORE — stash the base-commit emitter first"; exit 1; }
[ -x "$AFTER"  ] || { echo "  missing $AFTER";  exit 1; }

ARMS=(off ctl on)
for s in "${SUBS[@]}"; do ARMS+=("on-o$s"); done

for arm in "${ARMS[@]}"; do
  ll="$TMP/$NAME.$arm.ll"
  case "$arm" in
    off|ctl) "$BEFORE" "$SRC" - --rewrite --contract $TFLAG > "$ll" ;;
    on)      "$AFTER"  "$SRC" - --rewrite --contract $TFLAG > "$ll" ;;
    on-o*)
      s=${arm#on-o}
      sed -E "s/(ptr @task[0-9]+_pack, i64 [0-9]+, i32 [0-9]+, i64 [0-9]+, i32 )[0-9]+/\1$s/" \
        "$TMP/$NAME.on.ll" > "$ll"
      # Assert the patch: exactly one line differs from `on`, and it is the pack
      # registration. An arm that silently equals `on` (or differs in two places)
      # is not the measurement it claims to be.
      d=$(diff "$TMP/$NAME.on.ll" "$ll" | grep -c '^<')
      if [ "$d" -eq 0 ]; then
        echo "  (oversub=$s is the emitter's shipped default — same bytes as 'on')"
      elif [ "$d" -ne 1 ]; then
        echo "  PATCH ASSERTION FAILED on arm '$arm': $d lines changed, expected 0 or 1"; exit 1
      fi
      ;;
  esac || { echo "  emit($arm) FAILED"; exit 1; }
  clang $CFLAGS "$ll" "$RT" -o "$TMP/$NAME.$arm" 2>/dev/null \
    || { echo "  link($arm) FAILED"; exit 1; }
done

cmp -s "$TMP/$NAME.off.ll" "$TMP/$NAME.ctl.ll" \
  || { echo "  control is not byte-identical to off — the control is broken"; exit 1; }
cmp -s "$TMP/$NAME.off.ll" "$TMP/$NAME.on.ll" \
  && { echo "  !! off and on are BYTE-IDENTICAL — the packed branch did not fire on this leg."
       echo "     (not a measurement; fix selection before reading any number below)"; exit 1; }

echo "  pack dispatch: $(grep -c '_pack, i64' "$TMP/$NAME.on.ll") in 'on', \
$(grep -c '_pack, i64' "$TMP/$NAME.off.ll") in 'off' (must be 0)"
echo "  pack registration: $(grep -o 'ptr @task[0-9]*_pack, i64 [0-9]*, i32 [0-9]*, i64 [0-9]*, i32 [0-9]*' "$TMP/$NAME.on.ll" | head -1)"
# Rule 15/18: the transformation must be findable, and the kernel must NOT be.
echo "  slice fn identical off/on: $(for a in off on; do
  sed -n '/define internal void @task[0-9]*_slice(/,/^}/p' "$TMP/$NAME.$a.ll" | shasum -a 256 | cut -d' ' -f1
done | sort -u | wc -l | tr -d ' ') distinct hash(es) — must be 1"

# ---- value identity, BEFORE any timing ----
VREF=$("$TMP/$NAME.off" 2>/dev/null | grep -v '^iter ms=')
for a in "${ARMS[@]}"; do
  v=$("$TMP/$NAME.$a" 2>/dev/null | grep -v '^iter ms=')
  if [ "$v" != "$VREF" ]; then
    echo "  VALUE MISMATCH on arm '$a' — refusing to report timings."
    echo "    off: $VREF"
    echo "    $a : $v"
    exit 1
  fi
done
echo "  values identical to 'off' on every arm: $(echo $VREF)"

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
    dis = v[-1] < base[0] or base[-1] < v[0]
    tag = "-" if a == "off" else ("disjoint" if dis else "OVERLAP")
    print(f"  {a:>8} {v[0]:10.4f} {med:10.4f} {v[-1]:10.4f}  {bmed/med:7.3f}x  {tag}")
print("  (rule 6: under ~6% on this unpinned Mac is noise; byte-identical")
print("   binaries have measured -5.9%..+1.2% apart.)")
ctl = sorted(by["ctl"])
drift = abs(statistics.median(ctl) / bmed - 1.0) * 100
spread = (ctl[-1] / ctl[0] - 1.0) * 100
print(f"  CONTROL (byte-identical to off): {drift:.2f}% from off, own spread {spread:.1f}% — "
      + ("RUN IS VOID, drift landed on the swept axis" if drift >= 6.0
         else "within the noise floor, run stands"))
PY
