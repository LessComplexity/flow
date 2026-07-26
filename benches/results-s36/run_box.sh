#!/bin/bash
# S36 validation on the i9 box. Cross-compiled Mapal objects are linked here with
# gcc (no clang on this machine); the C++/NumPy legs are built and run natively.
# Every sample is kept: the point is min AND median AND the sub-0.01 ms race
# counter, not a single statistic.
set -uo pipefail
DIR="${DIR:-$HOME/s36bench}"
cd "$DIR"
RUNS="${RUNS:-30}"
TAG="${TAG:-post}"
BASELINES="${BASELINES:-1}"
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true

echo "== S36 box run ($TAG, $DIR) =="
uname -srm; nproc; grep -m1 "model name" /proc/cpuinfo
gcc --version | head -1
uptime
echo "RUNS=$RUNS"

echo "-- link mapal legs"
for o in *.o; do
    gcc -O3 "$o" libmapal_rt.a -o "mapal_${o%.o}_$TAG" -lpthread -ldl -lm
done

if [ "$BASELINES" = 1 ] && [ ! -x ladder2_cpp ]; then
    echo "-- build baselines"
    g++ -std=c++17 -O3 -march=native -ffp-contract=fast ladder2_baseline.cpp -o ladder2_cpp -pthread
    g++ -std=c++17 -O3 -march=native -ffp-contract=fast shapes_baseline.cpp  -o shapes_cpp  -pthread
fi

# stat <label> — reads `iter ms=` on stdin, prints min/median/max/n/race.
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

runs_of() { local bin="$1" par="$2" i; for ((i=0;i<RUNS;i++)); do MAPAL_PAR="$par" "./$bin"; done; }

echo
echo "-- mapal legs (all samples kept)"
for bin in mapal_*_"$TAG"; do
    shape="${bin#mapal_}"; shape="${shape%_$TAG}"
    runs_of "$bin" 1   2>/dev/null | stats "$shape mapal-1t"
    runs_of "$bin" par 2>/dev/null | stats "$shape mapal-par"
done

if [ "$BASELINES" = 1 ]; then
    echo
    echo "-- baselines"
    for shape in saxpy reduce transpose gather; do
        size=1048576; [ "$shape" = transpose ] && size=1024
        ./ladder2_cpp "$shape" 1t "$RUNS" "$size" 2>/dev/null | stats "$shape cpp-1t"
        ./ladder2_cpp "$shape" mt "$RUNS" "$size" 2>/dev/null | stats "$shape cpp-mt"
        python3 ladder2_numpy.py "$shape" "$RUNS" "$size" 2>/dev/null | stats "$shape numpy"
    done
    for shape in fir conv2d; do
        size=65536; [ "$shape" = conv2d ] && size=512
        ./shapes_cpp "$shape" 1t "$RUNS" "$size" 2>/dev/null | stats "$shape cpp-1t"
        ./shapes_cpp "$shape" mt "$RUNS" "$size" 2>/dev/null | stats "$shape cpp-mt"
        python3 shapes_numpy.py "$shape" 1t "$RUNS" "$size" 2>/dev/null | stats "$shape numpy-1t"
    done
fi

echo
echo "-- cleanliness"
uptime
ps -eo pcpu,comm --sort=-pcpu | head -4
echo "BOX DONE ($TAG)"
