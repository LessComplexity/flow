#!/bin/bash
# SME A/B: the same Mapal source emitted twice — once on the NEON path, once with the
# SME realization selected — compiled identically, run alternating.
#
# METHOD (the standing measurement rules):
#  - Both legs are the CONTRACT / FMA face. `fmopa` fuses, so SME is a contract-face
#    realization (ADR-0032 D1/D3); comparing it against a conformance build would repeat
#    S36c's exact error. `--contract` is passed to BOTH sides.
#  - VALUE IDENTITY IS CHECKED FIRST and the script exits non-zero if the two legs
#    disagree. A timing number from a leg that computes a different answer is worthless.
#  - Compute-only: the sources bracket their kernel with the `time` builtin and print
#    `iter ms=`. Nothing here times data generation.
#  - ALTERNATING runs, medians reported alongside minima. Per rule 6/11 a sub-10%
#    difference on an unpinned Mac at these sizes is noise; this leg expects a multiple,
#    not a percentage, and prints both so that stays visible.
#  - Absolute milliseconds, never ratios alone, with the baseline named.
#
# Usage: benches/sme/sme_ab.sh [sources] [runs]
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/sme_ab"; mkdir -p "$TMP"
SRCS="${1:-benches/matmul/matmul512_cap_f32.mapal benches/matmul/matmul1024_cap_f32.mapal}"
RUNS="${2:-51}"
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libmapal_rt.a"

# The M4 has SME but NOT full SVE; `armv9-a` implies +sve and the binary SIGILLs.
# See benches/sme/README.md. armv8-a+sme2 is the verified configuration.
CFLAGS="-O2 -march=armv8-a+sme2"

echo "== SME A/B =="
echo "clang:   $(clang --version | head -1)"
echo "cflags:  $CFLAGS"
echo "machine: $(sysctl -n machdep.cpu.brand_string), SVL=$( [ -x "$TMP/svl" ] && "$TMP/svl" | head -1 || echo '?')"
echo "runs:    $RUNS alternating"
echo "baseline commit: $(git -C "$ROOT" rev-parse --short HEAD)$(git -C "$ROOT" diff --quiet || echo ' +dirty')"
echo

cargo build -q -p mapal-rt --release --manifest-path "$ROOT/Cargo.toml" || exit 1

emit() { # <src> <out.ll> [extra flags...]
  ( cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
      "$1" - --rewrite --contract "${@:3}" ) > "$2"
}

for SRC in $SRCS; do
  NAME="$(basename "$SRC" .mapal)"
  echo "--- $NAME ---"

  emit "$SRC" "$TMP/$NAME.neon.ll"                              || { echo "  emit(neon) FAILED"; continue; }
  emit "$SRC" "$TMP/$NAME.sme.ll" --target=apple-m4-sme         || { echo "  emit(sme) FAILED";  continue; }

  if cmp -s "$TMP/$NAME.neon.ll" "$TMP/$NAME.sme.ll"; then
    echo "  !! the two emissions are BYTE-IDENTICAL — the SME path did not fire."
    echo "     (not a measurement; fix selection before reading any number below)"
  fi
  echo "  sme site fired: $(grep -c 'sme.mopa' "$TMP/$NAME.sme.ll") mopa call(s), \
$(grep -c 'aarch64_pstate_sm_body' "$TMP/$NAME.sme.ll") streaming kernel(s)"

  clang $CFLAGS "$TMP/$NAME.neon.ll" "$RT" -o "$TMP/$NAME.neon" 2>/dev/null || { echo "  link(neon) FAILED"; continue; }
  clang $CFLAGS "$TMP/$NAME.sme.ll"  "$RT" -o "$TMP/$NAME.sme"  2>/dev/null || { echo "  link(sme) FAILED";  continue; }

  # ---- value identity, BEFORE any timing ----
  VN=$("$TMP/$NAME.neon" 2>/dev/null | grep -v '^iter ms=')
  VS=$("$TMP/$NAME.sme"  2>/dev/null | grep -v '^iter ms=')
  if [ "$VN" != "$VS" ]; then
    echo "  VALUE MISMATCH — refusing to report timings."
    echo "    neon: $VN"
    echo "    sme : $VS"
    continue
  fi
  echo "  values identical: $VN"

  # ---- alternating timed runs ----
  : > "$TMP/$NAME.series"
  for _ in $(seq "$RUNS"); do
    n=$("$TMP/$NAME.neon" 2>/dev/null | sed -n 's/^iter ms=//p')
    s=$("$TMP/$NAME.sme"  2>/dev/null | sed -n 's/^iter ms=//p')
    echo "$n $s" >> "$TMP/$NAME.series"
  done

  "$PYTHON" - "$TMP/$NAME.series" "$NAME" <<'PY'
import sys, statistics
rows = [l.split() for l in open(sys.argv[1]) if len(l.split()) == 2]
neon = sorted(float(a) for a, _ in rows)
sme  = sorted(float(b) for _, b in rows)
if not neon: print("  no samples"); raise SystemExit
med = lambda v: statistics.median(v)
print(f"  n={len(rows)}")
print(f"  neon  min {neon[0]:9.4f} ms   median {med(neon):9.4f} ms   max {neon[-1]:9.4f} ms")
print(f"  sme   min {sme[0]:9.4f} ms   median {med(sme):9.4f} ms   max {sme[-1]:9.4f} ms")
print(f"  median speedup: {med(neon)/med(sme):.2f}x        min-on-min: {neon[0]/sme[0]:.2f}x")
print(f"  distributions {'OVERLAP (treat with suspicion)' if neon[0] <= sme[-1] else 'are disjoint'}")
PY
  echo
done

echo "Compare against docs/performance/matmul/s33.md:150-158 (M4 Pro f32):"
echo "  1024: flow-fma-1t 17.5449 ms | numpy-1t 1.2977 ms | numpy-thr 0.6757 ms"
echo "  512 : flow-fma-1t  2.1766 ms | numpy-1t 0.1600 ms | numpy-thr 0.1075 ms"
echo "NOTE: the numpy legs above are threaded/Accelerate; the Mapal legs here are 1t."
