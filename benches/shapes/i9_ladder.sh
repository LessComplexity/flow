#!/bin/bash
# S45: the whole shape ladder plus the move-panel sweep, on a remote x86-64 box.
#
# WHY THIS EXISTS AND THE OTHER HARNESSES DO NOT COVER IT. `ladder2_ab.sh`,
# `movepanel_ab.sh` and `transpose_vs_baselines.sh` all shell out to a LOCAL
# `clang`, and `movepanel_ab.sh` hardcodes `-march=armv8-a+sme2`. The i9 box has
# gcc but NO clang. So none of them can run there, and the numbers in
# `benches/results-s44/i9-ladder.md` are not reproducible without this.
#
# The split: EMIT and .ll -> .o happen HERE (needs clang + the Mapal emitter);
# only the LINK and the RUN happen on the box (needs gcc + the machine). Data
# generation stays outside every timed region on every leg, as everywhere else.
#
# Usage: benches/shapes/i9_ladder.sh <host>            # full run
#        CYCLES=25 benches/shapes/i9_ladder.sh <host>
set -euo pipefail

HOST="${1:?usage: i9_ladder.sh <host>}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
STAGE="$ROOT/target/tmp/i9"
REMOTE="${REMOTE:-mapal-s45}"     # NOT mapal-s42/s44 — one directory per session
CYCLES="${CYCLES:-25}"
EMIT="$ROOT/target/release/examples/emit"
# -march=raptorlake mirrors what the native g++ leg gets from -march=native, so
# the Mapal and C++ columns are compiled at the same level.
CFLAGS="-O3 -target x86_64-unknown-linux-gnu -march=raptorlake -ffp-contract=fast"
# S45: the i9's machine facts come from a NAMED profile, because the emission
# happens HERE (the box has no clang) and nothing about that box is detectable
# from this one. That is the cross-compilation case named profiles exist for.
TARGET="${TARGET:-raptorlake}"
BLOCKS="${BLOCKS:-2 4 8 16 32 64 128 256 512 1024}"

# There are TWO examples named `emit` (cuda and llvm) colliding on
# target/release/examples/emit, and the cuda one silently rejects --contract.
# Build the right one and prove it understands the flags before trusting it.
cargo build -q --release -p mapal-backend-llvm --example emit --manifest-path "$ROOT/Cargo.toml"
cargo build -q -p mapal-rt --release --target x86_64-unknown-linux-gnu --manifest-path "$ROOT/Cargo.toml"
for probe in --contract "--move-panel=off" "--move-panel=1024:16" "--target=$TARGET"; do
    "$EMIT" "$ROOT/benches/shapes/transpose_1024.mapal" - --rewrite "$probe" >/dev/null 2>&1 ||
        { echo "i9_ladder: wrong emit binary — it rejects $probe" >&2; exit 2; }
done

rm -rf "$STAGE"; mkdir -p "$STAGE/ll"
cd "$ROOT"

emit_cc() { # <name> <src> [flags...]
    local name="$1" src="$2"; shift 2
    "$EMIT" "$src" - --rewrite "--target=$TARGET" "$@" > "$STAGE/ll/$name.ll"
    [ -s "$STAGE/ll/$name.ll" ] || { echo "EMPTY EMISSION: $name" >&2; exit 1; }
    clang $CFLAGS -c "$STAGE/ll/$name.ll" -o "$STAGE/$name.o" 2>/dev/null
}

echo "-- emit + cross-compile the ladder (conf = FMA off, fma = --contract)"
for spec in fir:fir_1048576 conv2d:conv2d_1024 saxpy:saxpy_1048576 \
            reduce:reduce_1048576 transpose:transpose_1024 gather:gather_1048576; do
    name=${spec%%:*}; src="$ROOT/benches/shapes/${spec#*:}.mapal"
    emit_cc "${name}_conf" "$src"
    emit_cc "${name}_fma"  "$src" --contract
done

echo "-- emit + cross-compile the move-panel sweep"
# `move_panel_index` DECLINES unless `w % b == 0 && rows % b == 0`, so with
# w=rows=side only the divisors are treatments. A declined arm emits the same
# text as OFF; timing it as a variant would report "no effect" for a change that
# never fired, so it is a hard error here rather than a silent row.
for side in 512 1024 2048; do
    src="$ROOT/benches/shapes/transpose_${side}.mapal"
    [ -f "$src" ] || continue
    emit_cc "t${side}_off" "$src" --move-panel=off
    # The DEDUCED arm — no block supplied. Whether it fires is the result.
    emit_cc "t${side}_deduce" "$src"
    if cmp -s "$STAGE/ll/t${side}_off.ll" "$STAGE/ll/t${side}_deduce.ll"; then
        echo "   side=$side: deduction DECLINED (byte-identical to OFF)"
    else
        echo "   side=$side: deduction FIRED with B=$(grep -m1 'urem i64' "$STAGE/ll/t${side}_deduce.ll" | sed 's/.*, //')"
    fi
    for b in $BLOCKS; do
        [ $((side % b)) -eq 0 ] || continue
        emit_cc "t${side}_$b" "$src" "--move-panel=$side:$b"
        if cmp -s "$STAGE/ll/t${side}_off.ll" "$STAGE/ll/t${side}_$b.ll"; then
            echo "VOID: side=$side B=$b emitted the same text as OFF — the rung declined" >&2
            exit 1
        fi
    done
done
echo "   emission gate: every arm differs from OFF"

cp "$ROOT/benches/shapes/ladder2_baseline.cpp" "$ROOT/benches/shapes/shapes_baseline.cpp" \
   "$ROOT/benches/shapes/ladder2_numpy.py" "$ROOT/benches/shapes/shapes_numpy.py" "$STAGE/"
cp "$ROOT/target/x86_64-unknown-linux-gnu/release/libmapal_rt.a" "$STAGE/"

echo "-- ship to $HOST:~/$REMOTE"
ssh "$HOST" "rm -rf ~/$REMOTE && mkdir -p ~/$REMOTE/log"
scp -q "$STAGE"/*.o "$STAGE"/*.a "$STAGE"/*.cpp "$STAGE"/*.py "$HOST:~/$REMOTE/"

ssh "$HOST" "CYCLES=$CYCLES REMOTE=$REMOTE bash -s" <<'REMOTE_SCRIPT'
set -u
cd ~/"$REMOTE" || exit 2
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true
mkdir -p bin log; rm -f log/[LSC]_*

for o in *.o; do
    gcc -O3 "$o" libmapal_rt.a -lpthread -ldl -lm -o "bin/${o%.o}" || exit 1
done
g++ -std=c++17 -O3 -march=native -ffp-contract=fast ladder2_baseline.cpp -o bin/ladder2_cpp -pthread || exit 1
g++ -std=c++17 -O3 -march=native -ffp-contract=fast shapes_baseline.cpp  -o bin/shapes_cpp  -pthread || exit 1

# PINNING, and it is reported rather than assumed. cpu4 is a 5500 MHz P-core, not
# cpu0/cpu2 (the 5800 MHz favoured cores, where the boost lottery and interrupts
# live). available_parallelism() honours affinity, so the taskset sets the pool.
P1="taskset -c 4"; PA="taskset -c 0-31"
NP="env OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 python3"
strip_ms() { grep -v '^iter ms=' ; }

echo "== VALUE GATE — nothing is timed until every leg agrees =="
gate=0
for shape in fir conv2d saxpy reduce transpose gather; do
    case $shape in
        fir)       sz=1048576; cpp=shapes_cpp;  py=shapes_numpy.py;  pa=(--1t) ;;
        conv2d)    sz=1024;    cpp=shapes_cpp;  py=shapes_numpy.py;  pa=(--1t) ;;
        transpose) sz=1024;    cpp=ladder2_cpp; py=ladder2_numpy.py; pa=() ;;
        *)         sz=1048576; cpp=ladder2_cpp; py=ladder2_numpy.py; pa=() ;;
    esac
    $P1 env MAPAL_PAR=1 "bin/${shape}_conf" | strip_ms > "log/$shape.ref"
    for leg in "$PA env MAPAL_PAR=32 bin/${shape}_conf" \
               "$P1 env THREADS=1 bin/$cpp $shape 1t 1 $sz" \
               "$PA env THREADS=32 bin/$cpp $shape mt 1 $sz" \
               "$P1 $NP $py $shape ${pa+${pa[*]}} 1 $sz"; do
        $leg | strip_ms > log/.g || true
        cmp -s "log/$shape.ref" log/.g || { echo "VALUE MISMATCH $shape: $leg"; gate=1; }
    done
    # The FMA arm is contraction, so compare with tolerance, not bytes.
    $P1 env MAPAL_PAR=1 "bin/${shape}_fma" | strip_ms > log/.g
    python3 -c '
import sys
a=[float(x) for x in open(sys.argv[1]).read().split()]
b=[float(x) for x in open(sys.argv[2]).read().split()]
assert len(a)==len(b), "count"
for x,y in zip(a,b): assert abs(y-x)/max(abs(x),1.0) <= 1e-4, (x,y)
' "log/$shape.ref" log/.g || { echo "FMA DRIFT $shape"; gate=1; }
    printf '%-10s values OK ref=[%s]\n' "$shape" "$(tr '\n' ' ' < "log/$shape.ref")"
done
[ $gate -eq 0 ] || { echo "GATE FAILED — no timing"; exit 1; }

# Every run is wrapped in perf, OUTSIDE the self-timed kernel region so it cannot
# perturb the ms. S37b: this box holds ~constant cycles while wall time swings on
# the boost clock, and no_turbo needs root, so the clock is recorded per run
# instead of pinned. Raptor Lake exposes cpu_core AND cpu_atom cycle PMUs and perf
# MULTIPLEXES them: a P-core-pinned task still gets a cpu_atom row enabled ~4% of
# the time, then scaled up 25x into a five-figure lie. Only count a PMU actually
# enabled for most of the run ($5 = enabled %).
run() {
    local name="$1"; shift
    local pf="log/.perf.$$" ms ghz
    ms=$(perf stat -e task-clock,cycles -x, -o "$pf" -- "$@" 2>/dev/null | sed -n 's/^iter ms=//p')
    [ -n "$ms" ] || { echo "NO TIMING: $*" >&2; return 1; }
    ghz=$(awk -F, '$3 ~ /task-clock/ { tc = $1+0 }
                   $3 ~ /cycles/ && $5+0 >= 40 { cy += $1+0 }
                   END { if (tc > 0) printf "%.3f", cy/(tc*1e6); else printf "0" }' "$pf")
    printf '%s %s\n' "$ms" "$ghz" >> "log/$name"
}

echo "== timing: $CYCLES interleaved cycles =="
for w in 1 2 3; do $PA env MAPAL_PAR=32 bin/transpose_conf >/dev/null 2>&1; done
for ((c = 0; c < CYCLES; ++c)); do
    for shape in fir conv2d saxpy reduce transpose gather; do
        case $shape in
            fir)       sz=1048576; cpp=shapes_cpp;  py=shapes_numpy.py;  pa=(--1t) ;;
            conv2d)    sz=1024;    cpp=shapes_cpp;  py=shapes_numpy.py;  pa=(--1t) ;;
            transpose) sz=1024;    cpp=ladder2_cpp; py=ladder2_numpy.py; pa=() ;;
            *)         sz=1048576; cpp=ladder2_cpp; py=ladder2_numpy.py; pa=() ;;
        esac
        run "L_${shape}_mapal-conf-1t"  $P1 env MAPAL_PAR=1  "bin/${shape}_conf"
        run "L_${shape}_mapal-fma-1t"   $P1 env MAPAL_PAR=1  "bin/${shape}_fma"
        run "L_${shape}_mapal-conf-par" $PA env MAPAL_PAR=32 "bin/${shape}_conf"
        run "L_${shape}_mapal-fma-par"  $PA env MAPAL_PAR=32 "bin/${shape}_fma"
        run "L_${shape}_cpp-1t"         $P1 env THREADS=1  "bin/$cpp" "$shape" 1t 1 "$sz"
        run "L_${shape}_cpp-mt"         $PA env THREADS=32 "bin/$cpp" "$shape" mt 1 "$sz"
        run "L_${shape}_numpy-1t"       $P1 $NP "$py" "$shape" ${pa+"${pa[@]}"} 1 "$sz"
    done
    # The sweep. Controls (rule 22): B=side is the IDENTITY permutation through
    # the same arithmetic and must NOT move; saxpy is a null arm the flag never
    # touches, so if it moves the machine did and the run is VOID.
    for side in 512 1024 2048; do
        [ -x "bin/t${side}_off" ] || continue
        run "C_${side}_off_1t"  $P1 env MAPAL_PAR=1  "bin/t${side}_off"
        run "C_${side}_off_par" $PA env MAPAL_PAR=32 "bin/t${side}_off"
        run "C_${side}_deduce_1t"  $P1 env MAPAL_PAR=1  "bin/t${side}_deduce"
        run "C_${side}_deduce_par" $PA env MAPAL_PAR=32 "bin/t${side}_deduce"
        for b in $(ls bin | sed -n "s/^t${side}_\([0-9]*\)$/\1/p" | sort -n); do
            run "C_${side}_${b}_1t"  $P1 env MAPAL_PAR=1  "bin/t${side}_$b"
            run "C_${side}_${b}_par" $PA env MAPAL_PAR=32 "bin/t${side}_$b"
        done
    done
    run "S_ctl_1t"  $P1 env MAPAL_PAR=1  bin/saxpy_conf
    run "S_ctl_par" $PA env MAPAL_PAR=32 bin/saxpy_conf
done
rm -f log/.perf.* log/.g
echo DONE
REMOTE_SCRIPT

echo "-- pull samples"
# tar over ssh, NOT `scp -r`. There are ~110 sample files of a few hundred bytes
# each, and scp pays a round trip per file: measured at ~10 files/minute over this
# link, i.e. ten minutes to move 40 KB. One stream moves the lot in under a second.
rm -rf "$STAGE/log"
ssh "$HOST" "tar cf - -C ~/$REMOTE log" | tar xf - -C "$STAGE"
echo "samples in $STAGE/log — one file per leg, lines are '<ms> <GHz>'"
