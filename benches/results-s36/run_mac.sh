#!/bin/bash
# S36 validation on the Mac, same protocol as run_box.sh: every sample kept,
# min AND median AND the sub-0.01 ms race counter per cell.
set -euo pipefail
R="/Volumes/LessComplex/Personal/Flow"
TMP="$R/target/tmp/i9"
RUNS="${RUNS:-30}"
TAG="${TAG:-post}"
RT="$R/target/release/libmapal_rt.a"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

echo "== S36 mac run ($TAG) =="
uname -srm; sysctl -n machdep.cpu.brand_string; clang --version | head -1
echo "RUNS=$RUNS"

echo "-- link mapal legs"
for ll in "$TMP"/*.ll; do
    base="$(basename "${ll%.ll}")"
    clang -O3 -march=native -ffp-contract=fast "$ll" "$RT" \
        -o "$TMP/mac_${base}_$TAG" -lpthread -ldl -lm 2>/dev/null
done

stats() {
    awk -v label="$1" '
        /iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0 }
        END {
            if (n == 0) { printf "%-26s %10s\n", label, "n/a"; exit }
            for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }
                v[j+1] = x }
            race = 0; for (i = 0; i < n; i++) if (v[i] < 0.01) race++
            med = (n % 2) ? v[(n-1)/2] : (v[n/2 - 1] + v[n/2]) / 2
            printf "%-26s min=%10.4f  median=%10.4f  max=%10.4f  n=%3d  sub0.01=%d\n", \
                label, v[0], med, v[n-1], n, race
        }'
}

runs_of() { local bin="$1" par="$2" i; for ((i=0;i<RUNS;i++)); do MAPAL_PAR="$par" "$bin"; done; }

echo
echo "-- mapal legs (all samples kept)"
for bin in "$TMP"/mac_*_"$TAG"; do
    name="$(basename "$bin")"; shape="${name#mac_}"; shape="${shape%_$TAG}"
    runs_of "$bin" 1   2>/dev/null | stats "$shape mapal-1t"
    runs_of "$bin" par 2>/dev/null | stats "$shape mapal-par"
done
echo "MAC DONE ($TAG)"
