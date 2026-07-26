#!/bin/bash
# Local A/B for the shape-ladder-v2 classes (docs/performance/shape-ladder-v2.md):
# saxpy (streaming), reduce (reduction), transpose (data movement), gather
# (irregular reads). Separate from shapes_ab.sh on purpose — that harness is the
# published fir/conv2d path and is not touched here.
#
# Every leg is COMPUTE-ONLY and self-timed: the Mapal shapes bracket their kernel
# with `() -> time`, the C++ and NumPy legs time the same region, and all data
# generation happens outside it.
#
# Statistic: min for 1t, median for par (the `mapal_par_wait` race makes fast
# outliers, so a minimum is the vulnerable one — S33 §5b).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/ladder2"
RUNS="${RUNS:-9}"
PYTHON="${PYTHON:-python3}"
N="${N:-1048576}"
SIDE="${SIDE:-1024}"
RT="$ROOT/target/release/libmapal_rt.a"

case "$RUNS" in ""|*[!0-9]*) echo "RUNS must be an integer >= 3" >&2; exit 2 ;; esac
[ "$RUNS" -ge 3 ] || { echo "RUNS must be >= 3" >&2; exit 2; }

mkdir -p "$TMP"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

echo "== shape ladder v2 A/B =="
echo "clang:  $(clang --version | head -1)"
echo "python: $("$PYTHON" --version 2>&1)"
echo "N=$N SIDE=$SIDE RUNS=$RUNS"
uname -sm

"$PYTHON" -c 'import numpy' >/dev/null
cargo build -q -p mapal-rt --release --manifest-path "$ROOT/Cargo.toml"

echo "-- build baselines"
clang++ -std=c++17 -O3 -march=native -ffp-contract=fast \
    "$ROOT/benches/shapes/ladder2_baseline.cpp" -o "$TMP/ladder2_cpp" -pthread

emit() { (cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
            "$1" - --rewrite) > "$2"; }

echo "-- emit + build Mapal legs"
# macOS ships bash 3.2, which has no associative arrays — keep this POSIX-plain.
src_of() {
    case "$1" in
        transpose) echo "$ROOT/benches/shapes/transpose_${SIDE}.mapal" ;;
        *)         echo "$ROOT/benches/shapes/$1_${N}.mapal" ;;
    esac
}
for shape in saxpy reduce transpose gather; do
    src="$(src_of "$shape")"
    [ -f "$src" ] || { echo "missing $src" >&2; exit 2; }
    emit "$src" "$TMP/$shape.ll"
    clang -O3 -march=native -ffp-contract=fast "$TMP/$shape.ll" "$RT" \
        -o "$TMP/mapal_$shape" -lpthread -ldl -lm
done

# stat <min|median> — reads `iter ms=` lines on stdin.
# awk, not python: `python3 - <<HEREDOC` would make the heredoc stdin and eat the
# piped samples, which is exactly the bug this replaced.
stat_of() {
    awk -v mode="$1" '
        /iter ms=/ { split($0, part, "iter ms="); v[n++] = part[2] + 0 }
        END {
            if (n == 0) { print "n/a"; exit }
            if (mode == "min") {
                m = v[0]; for (i = 1; i < n; i++) if (v[i] < m) m = v[i]
                printf "%.4f\n", m; exit
            }
            for (i = 1; i < n; i++) {
                x = v[i]; j = i - 1
                while (j >= 0 && v[j] > x) { v[j + 1] = v[j]; j-- }
                v[j + 1] = x
            }
            if (n % 2) printf "%.4f\n", v[(n - 1) / 2]
            else printf "%.4f\n", (v[n / 2 - 1] + v[n / 2]) / 2
        }'
}

run_cell() { # <label> <command...>
    local label="$1"; shift
    local mode="min"
    case "$label" in *-par) mode="median" ;; esac
    local out; out="$("$@" 2>/dev/null)"
    printf '%-10s %-14s %10s\n' "$SHAPE" "$label" "$(printf '%s\n' "$out" | stat_of "$mode")"
}

printf '\n%-10s %-14s %10s\n' shape leg "ms"
printf '%-10s %-14s %10s\n' ---------- -------------- ----------
for SHAPE in saxpy reduce transpose gather; do
    size="$N"; [ "$SHAPE" = transpose ] && size="$SIDE"
    # Mapal: the binary self-times RUNS times only if re-run, so loop it.
    mapal_runs() { local i; for ((i=0;i<RUNS;i++)); do MAPAL_PAR="$1" "$TMP/mapal_$SHAPE"; done; }
    run_cell "mapal-1t"  mapal_runs 1
    run_cell "mapal-par" mapal_runs par
    run_cell "cpp-1t"   "$TMP/ladder2_cpp" "$SHAPE" 1t "$RUNS" "$size"
    run_cell "cpp-mt"   "$TMP/ladder2_cpp" "$SHAPE" mt "$RUNS" "$size"
    run_cell "numpy"    "$PYTHON" "$ROOT/benches/shapes/ladder2_numpy.py" "$SHAPE" "$RUNS" "$size"
    echo
done
