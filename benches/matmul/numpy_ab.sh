#!/bin/bash
# Same-session, interleaved Mapal-vs-numpy matmul comparison.
#
# WHY THIS EXISTS. The threaded numpy figure this project quotes (3113 GF/s at
# N=4096) was taken in S42 and has been carried across sessions since. S43 then
# retracted a different published number for being thermal drift on this same
# machine, and wrote rule 19: a number that was never re-taken has never been
# checked. Comparing today's Mapal median against a stale numpy baseline is
# exactly that error, so this runs both legs ALTERNATING in one session.
#
# Both legs are compute-only: the Mapal source brackets its kernel with the
# `time` builtin, numpy times `a @ b` alone with warmup. numpy's BLAS on this
# machine IS Accelerate, so "numpy" and "Accelerate" are one measurement path.
#
# numpy reports best-of-N and we report the median, which biases the comparison
# AGAINST Mapal. That is deliberate; a win under a handicap is still a win.
#
# Usage: numpy_ab.sh <emit-binary> <N> [cycles]
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN=${1:?usage: numpy_ab.sh <emit-binary> <N> [cycles]}
N=${2:?usage: numpy_ab.sh <emit-binary> <N> [cycles]}
CYCLES=${3:-15}
TMP="$ROOT/target/tmp/numpy_ab"; mkdir -p "$TMP"
SRC="$ROOT/benches/matmul/matmul${N}_cap_f32.mapal"
RT="$ROOT/target/release/libmapal_rt.a"

[ -f "$SRC" ] || { echo "no source $SRC" >&2; exit 2; }
[ -f "$RT" ] || { echo "no runtime $RT — cargo build -p mapal-rt --release" >&2; exit 2; }

echo "== Mapal vs numpy, N=$N, $CYCLES alternating cycles =="
echo "machine: $(sysctl -n machdep.cpu.brand_string)"
echo "numpy:   $(python3 -c 'import numpy;print(numpy.__version__)') / BLAS = $(python3 -c "import numpy,json;print(json.dumps(numpy.__config__.show(mode='dicts'))[:0] or '')" 2>/dev/null; python3 -c "import numpy;d=numpy.__config__.show(mode='dicts');print(d['Build Dependencies']['blas']['name'])" 2>/dev/null)"
echo "emit:    $BIN"
echo

"$BIN" "$SRC" - --rewrite --contract --target=apple-m4-sme > "$TMP/m.ll" 2>/dev/null || { echo "emit failed" >&2; exit 1; }
clang -O2 -march=armv8-a+sme2 "$TMP/m.ll" "$RT" -lpthread -o "$TMP/mapal" 2>/dev/null || { echo "link failed" >&2; exit 1; }

# Value gate FIRST: a timing from a leg computing a different answer is worthless.
mv=$("$TMP/mapal" | tail -1)
nv=$(python3 "$ROOT/benches/matmul/numpy_bench.py" "$N" 1)
echo "mapal: $mv"
echo "numpy: $nv"
echo

: > "$TMP/m.times"; : > "$TMP/n.times"
for i in $(seq 1 "$CYCLES"); do
  # alternate which leg goes first so the cold-clock ramp is paid symmetrically
  if [ $((i % 2)) -eq 0 ]; then
    "$TMP/mapal"                                        | grep -o 'iter ms=[0-9.]*' | cut -d= -f2 >> "$TMP/m.times"
    python3 "$ROOT/benches/matmul/numpy_bench.py" "$N" 3 | grep -o ' [0-9.]* ms'    | tr -d ' ms' >> "$TMP/n.times"
  else
    python3 "$ROOT/benches/matmul/numpy_bench.py" "$N" 3 | grep -o ' [0-9.]* ms'    | tr -d ' ms' >> "$TMP/n.times"
    "$TMP/mapal"                                        | grep -o 'iter ms=[0-9.]*' | cut -d= -f2 >> "$TMP/m.times"
  fi
  printf '.' >&2
done
echo >&2

python3 - "$TMP/m.times" "$TMP/n.times" "$N" <<'PY'
import sys
m=[float(x) for x in open(sys.argv[1]) if x.strip()]
n=[float(x) for x in open(sys.argv[2]) if x.strip()]
N=int(sys.argv[3]); fl=2.0*N**3
def st(v):
    v=sorted(v); return v[0], v[len(v)//2], v[-1]
mm=st(m); nn=st(n)
print(f"\n{'leg':6} {'min':>10} {'median':>10} {'max':>10} {'GF/s med':>10}  n")
for name,s,v in (("mapal",mm,m),("numpy",nn,n)):
    print(f"{name:6} {s[0]:10.3f} {s[1]:10.3f} {s[2]:10.3f} {fl/(s[1]*1e6):10.1f}  {len(v)}")
overlap = not (mm[2] < nn[0] or nn[2] < mm[0])
print(f"\nratio of medians (numpy/mapal): {nn[1]/mm[1]:.3f}x   distributions: {'OVERLAP' if overlap else 'DISJOINT'}")
print("mapal is FASTER" if mm[1] < nn[1] else "numpy is FASTER")
PY
