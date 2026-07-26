#!/bin/bash
# S36b per-language comparison, POST-FIX. One table: every shape, every language,
# on whichever machine it is run on.
#
# The Mapal legs are the already-built post-fix binaries (MAPAL_DIR); the C++,
# Rust and NumPy legs are built here from the same sources the published
# harnesses use. Every leg is compute-only and self-timed, and every cell reports
# BOTH min and median over N runs so no statistic is hidden.
#
#   MAPAL_DIR=<dir> PREFIX=<mac_|mapal_> SUFFIX=<_post> RUNS=30 bash compare_languages.sh
set -uo pipefail
ROOT="${ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MAPAL_DIR="${MAPAL_DIR:-$ROOT/target/tmp/i9}"
PREFIX="${PREFIX:-mac_}"
SUFFIX="${SUFFIX:-_post}"
RUNS="${RUNS:-30}"
TMP="${TMP:-$ROOT/target/tmp/langcmp}"
PYTHON="${PYTHON:-python3}"
N="${N:-1048576}"
SIDE="${SIDE:-1024}"
FIR_N="${FIR_N:-65536}"
CONV="${CONV:-512}"
MM="${MM:-1024}"
mkdir -p "$TMP"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

if [ "$(uname -s)" = Darwin ]; then CXX=clang++; NATIVE="-march=native"; else CXX=g++; NATIVE="-march=native"; fi

echo "== per-language comparison (post-fix) =="
uname -srm
$CXX --version | head -1
rustc --version
"$PYTHON" -c 'import numpy; print("numpy", numpy.__version__)'
echo "RUNS=$RUNS  ladder N=$N side=$SIDE  fir=$FIR_N conv2d=$CONV matmul=$MM"
echo

echo "-- build baselines"
$CXX -std=c++17 -O3 $NATIVE -ffp-contract=fast "$ROOT/benches/shapes/ladder2_baseline.cpp" -o "$TMP/ladder_cpp" -pthread
$CXX -std=c++17 -O3 $NATIVE -ffp-contract=fast "$ROOT/benches/shapes/shapes_baseline.cpp"  -o "$TMP/shapes_cpp" -pthread
rustc -O -C target-cpu=native "$ROOT/benches/shapes/ladder2_baseline.rs" -o "$TMP/ladder_rs" 2>/dev/null
rustc -O -C target-cpu=native "$ROOT/benches/shapes/shapes_baseline.rs"  -o "$TMP/shapes_rs"  2>/dev/null
$CXX -std=c++17 -O3 $NATIVE -ffp-contract=fast "$ROOT/benches/matmul/cpp_naive.cpp" -o "$TMP/mm_cpp_1t"
$CXX -std=c++17 -O3 $NATIVE -ffp-contract=fast "$ROOT/benches/matmul/cpp_mt.cpp"    -o "$TMP/mm_cpp_mt" -pthread
rustc -O -C target-cpu=native "$ROOT/benches/matmul/rust_naive.rs" -o "$TMP/mm_rs_1t" 2>/dev/null
rustc -O -C target-cpu=native "$ROOT/benches/matmul/rust_mt.rs"    -o "$TMP/mm_rs_mt" 2>/dev/null

# stat — reads `iter ms=` OR the matmul baselines' `... N=<n> <ms> ms ...`
stat_of() {
    awk '
        /iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0; next }
        match($0, / N=[0-9]+ [0-9.]+ ms/) { split($0, p, " "); for (i = 1; i <= NF; i++) if ($i == "ms") v[n++] = $(i-1) + 0 }
        END {
            if (n == 0) { print "n/a n/a"; exit }
            for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }; v[j+1] = x }
            med = (n % 2) ? v[(n-1)/2] : (v[n/2 - 1] + v[n/2]) / 2
            printf "%.4f %.4f\n", v[0], med
        }'
}

row() { # <shape> <leg> <command...>
    local shape="$1" leg="$2"; shift 2
    local out; out="$("$@" 2>/dev/null | stat_of)"
    printf '%-10s %-12s %10s %10s\n' "$shape" "$leg" ${out}
}
mapal_runs() { local bin="$1" par="$2" i; for ((i=0;i<RUNS;i++)); do MAPAL_PAR="$par" "$bin"; done; }

printf '%-10s %-12s %10s %10s\n' shape leg "min ms" "median"
printf '%-10s %-12s %10s %10s\n' ---------- ------------ ---------- ----------

for shape in saxpy reduce transpose gather; do
    size="$N"; [ "$shape" = transpose ] && size="$SIDE"
    bin="$MAPAL_DIR/${PREFIX}${shape}_${size}${SUFFIX}"
    row "$shape" mapal-1t  mapal_runs "$bin" 1
    row "$shape" mapal-par mapal_runs "$bin" par
    row "$shape" cpp-1t    "$TMP/ladder_cpp" "$shape" 1t "$RUNS" "$size"
    row "$shape" cpp-mt    "$TMP/ladder_cpp" "$shape" mt "$RUNS" "$size"
    row "$shape" rust-1t   "$TMP/ladder_rs"  "$shape" 1t "$RUNS" "$size"
    row "$shape" rust-mt   "$TMP/ladder_rs"  "$shape" mt "$RUNS" "$size"
    row "$shape" numpy     "$PYTHON" "$ROOT/benches/shapes/ladder2_numpy.py" "$shape" "$RUNS" "$size"
    echo
done

for shape in fir conv2d; do
    size="$FIR_N"; [ "$shape" = conv2d ] && size="$CONV"
    bin="$MAPAL_DIR/${PREFIX}${shape}_${size}${SUFFIX}"
    row "$shape" mapal-1t  mapal_runs "$bin" 1
    row "$shape" mapal-par mapal_runs "$bin" par
    row "$shape" cpp-1t    "$TMP/shapes_cpp" "$shape" 1t "$RUNS" "$size"
    row "$shape" cpp-mt    "$TMP/shapes_cpp" "$shape" mt "$RUNS" "$size"
    row "$shape" rust-1t   "$TMP/shapes_rs"  "$shape" 1t "$RUNS" "$size"
    row "$shape" rust-mt   "$TMP/shapes_rs"  "$shape" mt "$RUNS" "$size"
    row "$shape" numpy     "$PYTHON" "$ROOT/benches/shapes/shapes_numpy.py" "$shape" 1t "$RUNS" "$size"
    echo
done

MM_RUNS="${MM_RUNS:-5}"
bin="$MAPAL_DIR/${PREFIX}matmul${MM}_f32${SUFFIX}"
row "matmul$MM" mapal-1t  bash -c "for i in \$(seq $MM_RUNS); do MAPAL_PAR=1 '$bin'; done"
row "matmul$MM" mapal-par bash -c "for i in \$(seq $MM_RUNS); do MAPAL_PAR=par '$bin'; done"
row "matmul$MM" cpp-1t    "$TMP/mm_cpp_1t" "$MM" "$MM_RUNS" f32
row "matmul$MM" cpp-mt    "$TMP/mm_cpp_mt" "$MM" "$MM_RUNS" f32
row "matmul$MM" rust-1t   "$TMP/mm_rs_1t"  "$MM" "$MM_RUNS" f32
row "matmul$MM" rust-mt   "$TMP/mm_rs_mt"  "$MM" "$MM_RUNS" f32
row "matmul$MM" numpy-1t  env VECLIB_MAXIMUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1 "$PYTHON" "$ROOT/benches/matmul/numpy_bench.py" "$MM" "$MM_RUNS"
row "matmul$MM" numpy-mt  "$PYTHON" "$ROOT/benches/matmul/numpy_bench.py" "$MM" "$MM_RUNS"
echo "COMPARISON DONE"
