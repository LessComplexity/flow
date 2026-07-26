#!/bin/bash
# Pinned box A/B: P-cores only (CPUs 0-15 = 8 P-cores x2 threads; 16-31 are E-cores
# at 4.3 GHz). The governor is powersave and cannot be changed without root, so the
# ramp is a recorded caveat, not a corrected one.
set -uo pipefail
DIR="${DIR:-$HOME/s36bench}"; cd "$DIR"
RUNS="${RUNS:-100}"; TAG="${TAG:-post}"
ulimit -s unlimited 2>/dev/null || true
stats() {
    awk -v label="$1" '
        /iter ms=/ { split($0, p, "iter ms="); v[n++] = p[2] + 0 }
        END { if (n == 0) { printf "%-28s %s\n", label, "n/a"; exit }
            for (i = 1; i < n; i++) { x = v[i]; j = i - 1
                while (j >= 0 && v[j] > x) { v[j+1] = v[j]; j-- }; v[j+1] = x }
            race = 0; for (i = 0; i < n; i++) if (v[i] < 0.01) race++
            med = (n % 2) ? v[(n-1)/2] : (v[n/2 - 1] + v[n/2]) / 2
            printf "%-28s min=%9.4f  median=%9.4f  max=%9.4f  min/med=%5.3f  n=%3d  sub0.01=%d\n", \
                label, v[0], med, v[n-1], (med > 0 ? v[0]/med : 0), n, race }'
}
echo "== pinned box run ($TAG, P-cores 0-15) =="; uptime
for bin in mapal_*_"$TAG"; do
    shape="${bin#mapal_}"; shape="${shape%_$TAG}"
    for i in $(seq 1 "$RUNS"); do MAPAL_PAR=1 taskset -c 0 "./$bin"; done 2>/dev/null | stats "$shape 1t@P-core"
    for i in $(seq 1 "$RUNS"); do MAPAL_PAR=par taskset -c 0-15 "./$bin"; done 2>/dev/null | stats "$shape par@8P"
done
uptime; echo "PINNED DONE ($TAG)"
