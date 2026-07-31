#!/bin/bash
# S47: optimum(machine, side) for the move-panel block B, on the i9 box.
#
# WHY NOT `i9_ladder.sh`. That script measures the whole shape ladder AND a
# sweep at three sides; the ladder legs (numpy, g++, fir/conv2d/…) are ~80% of
# its run time and none of them move with B. This is the sweep alone, at four
# sides including a non-power-of-two one, over EVERY legal divisor — which is
# what "two points fit any curve" demands and what the S45 record does not have.
#
# The split is `i9_ladder.sh`'s: EMIT and .ll -> .o here (needs clang + the Mapal
# emitter), LINK and RUN on the box (gcc + the machine).
#
# PINNING, reported rather than assumed:
#   1t  -> taskset -c 4        one P-core, not cpu0/cpu2 (the favoured-core
#                              boost lottery lives there)
#   par -> taskset -c 0-15     the EIGHT P-cores (16 SMT threads). NOT 0-31:
#                              cpu16-31 are E-cores with a 32K/8-way L1D, and
#                              mixing two L1 geometries into one argmin is how a
#                              block-size optimum becomes unattributable.
# CONTROLS (rule 22): B = side is the IDENTITY permutation and must not move;
# `saxpy` is a null arm the flag never touches, so if it moves the machine did.
#
# Usage: benches/shapes/blocksweep_i9.sh <host>
set -euo pipefail

HOST="${1:?usage: blocksweep_i9.sh <host>}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
STAGE="$ROOT/target/tmp/s47"
REMOTE="${REMOTE:-mapal-s47}"
CYCLES="${CYCLES:-15}"
EMIT="$ROOT/target/release/examples/emit"
CFLAGS="-O3 -target x86_64-unknown-linux-gnu -march=raptorlake -ffp-contract=fast"
TARGET="${TARGET:-raptorlake}"
SIDES="${SIDES:-512 1024 1536 2048}"

# Legal B = divisors of gcd(w, rows) = divisors of `side`; below 2 the
# permutation is the identity and above `side` it does not tile. The 1536 list is
# the point of that side: 3, 6, 12, 24, 48, 96, 192, 384 are legal there and
# nowhere else, so a power-of-two-shaped optimum is falsifiable.
blocks_for() {
    case "$1" in
        512)  echo "2 4 8 16 32 64 128 256 512" ;;
        1024) echo "2 4 8 16 32 64 128 256 512 1024" ;;
        1536) echo "2 3 4 6 8 12 16 24 32 48 64 96 128 192 256 384 1536" ;;
        2048) echo "2 4 8 16 32 64 128 256 512 1024 2048" ;;
        *)    echo "2 4 8 16 32 64 128 256" ;;
    esac
}

cargo build -q --release -p mapal-backend-llvm --example emit --manifest-path "$ROOT/Cargo.toml"
cargo build -q -p mapal-rt --release --target x86_64-unknown-linux-gnu --manifest-path "$ROOT/Cargo.toml"
for probe in --contract "--move-panel=off" "--move-panel=1024:16" "--target=$TARGET"; do
    "$EMIT" "$ROOT/benches/shapes/transpose_1024.mapal" - --rewrite "$probe" >/dev/null 2>&1 ||
        { echo "blocksweep_i9: wrong emit binary — it rejects $probe" >&2; exit 2; }
done

rm -rf "$STAGE"; mkdir -p "$STAGE/ll"
cd "$ROOT"

emit_cc() { # <name> <src> [flags...]
    local name="$1" src="$2"; shift 2
    "$EMIT" "$src" - --rewrite "--target=$TARGET" "$@" > "$STAGE/ll/$name.ll"
    [ -s "$STAGE/ll/$name.ll" ] || { echo "EMPTY EMISSION: $name" >&2; exit 1; }
    clang $CFLAGS -c "$STAGE/ll/$name.ll" -o "$STAGE/$name.o" 2>/dev/null
}

echo "-- emit + cross-compile (target=$TARGET, sides: $SIDES)"
for side in $SIDES; do
    src="$ROOT/benches/shapes/transpose_${side}.mapal"
    [ -f "$src" ] || { echo "   side=$side: no shape, skipped"; continue; }
    emit_cc "t${side}_off" "$src" --move-panel=off
    emit_cc "t${side}_deduce" "$src"
    if cmp -s "$STAGE/ll/t${side}_off.ll" "$STAGE/ll/t${side}_deduce.ll"; then
        echo "   side=$side: deduction DECLINED (byte-identical to OFF)"
    else
        echo "   side=$side: deduction FIRED with B=$(grep -m1 'urem i64' "$STAGE/ll/t${side}_deduce.ll" | sed 's/.*, //')"
    fi
    for b in $(blocks_for "$side"); do
        emit_cc "t${side}_$b" "$src" "--move-panel=$side:$b"
        cmp -s "$STAGE/ll/t${side}_off.ll" "$STAGE/ll/t${side}_$b.ll" &&
            { echo "VOID: side=$side B=$b emitted the same text as OFF" >&2; exit 1; }
    done
done
emit_cc "ctl" "$ROOT/benches/shapes/saxpy_1048576.mapal"
echo "   emission gate: every B arm differs from OFF"

cp "$ROOT/target/x86_64-unknown-linux-gnu/release/libmapal_rt.a" "$STAGE/"

echo "-- ship to $HOST:~/$REMOTE"
ssh "$HOST" "rm -rf ~/$REMOTE && mkdir -p ~/$REMOTE/log"
tar cf - -C "$STAGE" --exclude ll . | ssh "$HOST" "tar xf - -C ~/$REMOTE"

ssh "$HOST" "CYCLES=$CYCLES REMOTE=$REMOTE SIDES='$SIDES' bash -s" <<'REMOTE_SCRIPT'
set -u
cd ~/"$REMOTE" || exit 2
ulimit -s unlimited 2>/dev/null || ulimit -s hard 2>/dev/null || true
mkdir -p bin log; rm -f log/*

for o in *.o; do
    gcc -O3 "$o" libmapal_rt.a -lpthread -ldl -lm -o "bin/${o%.o}" || exit 1
done

P1="taskset -c 4"; PA="taskset -c 0-15"; PAR=16
strip_ms() { grep -v '^iter ms='; }

echo "== VALUE GATE — nothing is timed until every arm agrees with OFF =="
gate=0
for side in $SIDES; do
    [ -x "bin/t${side}_off" ] || continue
    $P1 env MAPAL_PAR=1 "bin/t${side}_off" | strip_ms > "log/$side.ref"
    for f in bin/t${side}_*; do
        a=$(basename "$f"); [ "$a" = "t${side}_off" ] && continue
        $P1 env MAPAL_PAR=1 "$f" | strip_ms > log/.g
        cmp -s "log/$side.ref" log/.g || { echo "VALUE MISMATCH $a"; gate=1; }
        $PA env MAPAL_PAR=$PAR "$f" | strip_ms > log/.g
        cmp -s "log/$side.ref" log/.g || { echo "VALUE MISMATCH $a (par)"; gate=1; }
    done
    printf 'side=%-5s values identical across every arm, 1t and par  ref=[%s]\n' \
        "$side" "$(tr '\n' ' ' < "log/$side.ref")"
done
[ $gate -eq 0 ] || { echo "GATE FAILED — no timing"; exit 1; }

# ms AND cycles. This box holds ~constant cycles while wall time swings with the
# boost clock, and no_turbo needs root. Raptor Lake exposes cpu_core AND cpu_atom
# cycle PMUs and perf MULTIPLEXES them: only count a PMU enabled for most of the
# run ($5 = enabled %), or a 4%-enabled cpu_atom row gets scaled 25x into a lie.
run() {
    local name="$1"; shift
    local pf="log/.perf.$$" ms cy
    ms=$(perf stat -e cycles -x, -o "$pf" -- "$@" 2>/dev/null | sed -n 's/^iter ms=//p')
    [ -n "$ms" ] || { echo "NO TIMING: $*" >&2; return 1; }
    cy=$(awk -F, '$3 ~ /cycles/ && $5+0 >= 40 { c += $1+0 } END { printf "%.0f", c }' "$pf")
    printf '%s %s\n' "$ms" "$cy" >> "log/$name"
}

echo "== timing: $CYCLES interleaved cycles; 1t=$P1  par=$PA MAPAL_PAR=$PAR =="
for w in 1 2 3; do $PA env MAPAL_PAR=$PAR bin/t1024_off >/dev/null 2>&1; done
for ((c = 0; c < CYCLES; ++c)); do
    for side in $SIDES; do
        [ -x "bin/t${side}_off" ] || continue
        for f in bin/t${side}_off bin/t${side}_deduce $(ls bin | sed -n "s/^t${side}_\([0-9]*\)$/bin\/t${side}_\1/p" | sort -t_ -k2 -n); do
            a=$(basename "$f")
            run "${a}_1t"  $P1 env MAPAL_PAR=1    "$f"
            run "${a}_par" $PA env MAPAL_PAR=$PAR "$f"
        done
    done
    run "ctl_1t"  $P1 env MAPAL_PAR=1    bin/ctl
    run "ctl_par" $PA env MAPAL_PAR=$PAR bin/ctl
done
rm -f log/.perf.* log/.g
echo DONE
REMOTE_SCRIPT

echo "-- pull samples"
rm -rf "$STAGE/log"
ssh "$HOST" "tar cf - -C ~/$REMOTE log" | tar xf - -C "$STAGE"
echo "samples in $STAGE/log — one file per arm, lines are '<ms> <cycles>'"
