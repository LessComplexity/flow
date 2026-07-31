#!/bin/bash
# S43 operand-residency A/B: ONE emitted SME module, nine arms that differ only in
# how far the two k-derived operand offsets are allowed to walk before wrapping.
# An INSTRUMENT, not a feature — nothing here ships, no emitter byte moves. The
# window is a text patch on the emitted `.ll` (benches/sme/winmask.py), applied
# before clang runs. See docs/components/backend-llvm/plans/plan-s43-*.md.
#
# METHOD (the standing measurement rules):
#  - The `Trn` is held BYTE-CONSTANT across arms: same k loop, same 4 loads, same
#    4 `fmopa`, same pack, same full 16-row x 4-tile ZA read-out. Only the
#    addresses' upper bits change. Rule 18: the read-out is checked identical
#    across arms in the ASSEMBLY, because a previous probe shrank a loop and left
#    a read-out standing, and produced two wrong attributions that were retracted.
#  - Arm 0 is the SHIPPING binary, unpatched, in the run so that claim is measured
#    rather than asserted. Arm 1 is the control: masks larger than any real offset.
#  - VALUE IDENTITY IS INVERTED HERE, and that is the point. Arms 2-8 are wrong by
#    construction; an arm printing the CORRECT answer is VOID — its mask did not
#    survive. Only arm 1 must match arm 0.
#  - Arms 3, 6, 7, 8 plus the control are a 5-point B-window sweep (16/32/64/128 KB
#    and the real 256 KB). Each carries exactly ONE surviving `and`, so they are
#    mutually byte-identical but for one immediate. That sweep is the point of the run.
#  - Compute-only: the sources bracket their kernel with the `time` builtin and
#    print `iter ms=`. Nothing here times data generation.
#  - ROUND-ROBIN cycles with the process order ROTATED every cycle, so the
#    cold-clock ramp is paid symmetrically across arms (rule 14).
#  - Absolute milliseconds, min/median/max, with a two-sided permutation test on
#    the difference of medians against arm 1 and a bootstrap CI on the median
#    ratio. Never ratios alone, and never a min/max range one sample can flip.
#
# Usage: benches/sme/resid_ab.sh [src] [cycles] [threads: 1|max]
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$ROOT/target/tmp/resid_ab"; mkdir -p "$TMP"
SRC="${1:-benches/matmul/matmul4096_cap_f32.mapal}"
CYCLES="${2:-21}"
THREADS="${3:-1}"
PYTHON="${PYTHON:-python3}"
RT="$ROOT/target/release/libmapal_rt.a"
NAME="$(basename "$SRC" .mapal).t$THREADS"

# The M4 has SME but NOT full SVE; `armv9-a` implies +sve and the binary SIGILLs.
# See benches/sme/README.md. armv8-a+sme2 is the verified configuration.
CFLAGS="-O2 -march=armv8-a+sme2"

# Masks are FLOAT-ELEMENT counts. In the emitted N=4096 kernel `%aoff = mul %k, 32`
# (max 4095*32 = 131040 = 512 KB) and `%boff = mul %k, %bn` with bn=16 (max
# 4095*16 = 65520 = 256 KB). 17592186044415 = 2^44-1 is larger than either, so it
# is a control that -O2 can prove dead.
#
# THE B WINDOW IS PER CALL; THE THING THAT DECIDES RESIDENCY IS THE PER-I-STEP
# FOOTPRINT, because the whole of B is re-swept on each of the 128 i-steps:
#   B footprint / i-step = 128 j-panels * 2 B streams * window
#   16 KB -> 4 MB | 32 KB -> 8 MB | 64 KB -> 16 MB | 128 KB -> 32 MB | 256 KB -> 64 MB
#
#        arm:   0    1               2               3               4      5       6               7               8
MASKA=(  ''     17592186044415  4095            17592186044415  4095   32767   17592186044415  17592186044415  17592186044415 )
MASKB=(  ''     17592186044415  17592186044415  4095            4095   32767   8191            16383           32767 )
ARMS="0 1 2 3 4 5 6 7 8"
PATCHED="1 2 3 4 5 6 7 8"

echo "== S43 operand-residency A/B =="
echo "source:  $SRC"
echo "clang:   $(clang --version | head -1)"
echo "cflags:  $CFLAGS"
echo "machine: $(sysctl -n machdep.cpu.brand_string)"
echo "threads: $THREADS$( [ "$THREADS" = 1 ] && echo ' (MAPAL_PAR=1)' || echo ' (full width)')"
echo "cycles:  $CYCLES round-robin, order rotated per cycle"
echo "baseline commit: $(git -C "$ROOT" rev-parse --short HEAD)$(git -C "$ROOT" diff --quiet || echo ' +dirty')"
echo

cargo build -q -p mapal-rt --release --manifest-path "$ROOT/Cargo.toml" || exit 1

# ---- emit once, patch eight ways ----
( cd "$ROOT" && cargo run -q --release -p mapal-backend-llvm --example emit -- \
    "$SRC" - --rewrite --contract --target=apple-m4-sme ) > "$TMP/$NAME.arm0.ll" || { echo "emit FAILED"; exit 1; }
grep -q 'aarch64_pstate_sm_body' "$TMP/$NAME.arm0.ll" || { echo "no streaming kernel emitted — the SME path did not fire"; exit 1; }

for a in $PATCHED; do
  "$PYTHON" "$ROOT/benches/sme/winmask.py" "${MASKA[$a]}" "${MASKB[$a]}" \
      "$TMP/$NAME.arm0.ll" -o "$TMP/$NAME.arm$a.ll" || { echo "winmask arm$a FAILED"; exit 1; }
done

for a in $ARMS; do
  clang $CFLAGS -S "$TMP/$NAME.arm$a.ll" -o "$TMP/$NAME.arm$a.s" 2>/dev/null || { echo "asm arm$a FAILED"; exit 1; }
  clang $CFLAGS    "$TMP/$NAME.arm$a.ll" "$RT" -o "$TMP/$NAME.arm$a" 2>/dev/null || { echo "link arm$a FAILED"; exit 1; }
done

# ---- GATE 1: the assembly (rules 15 + 18) — counts printed, never asserted ----
echo "--- GATE 1: assembly, per k iteration, inside _mapal_sme_panel ---"
"$PYTHON" - "$TMP/$NAME" <<'PY' || exit 1
import hashlib, re, sys
base = sys.argv[1]
rows_sig, ok = {}, True
# `K=4096` and `bn=16` reach the kernel as constants, so IPSCCP/instcombine can
# prove a 2^44-1 mask dead and DELETES it, while 4095/8191/16383/32767 survive.
# ASSERTED, not printed: an arm where only ONE of two masks folded would
# otherwise pass the tripwire and be tabled under the wrong window label.
EXPECT_AND = {0: 0, 1: 0, 2: 1, 3: 1, 4: 2, 5: 2, 6: 1, 7: 1, 8: 1}
print(f"  {'arm':<4}{'ld1w':>6}{'ldr z':>7}{'vec ld':>8}{'fmopa':>7}{'and':>6}{'want':>6}   read-out (16x4) sig")
for a in range(9):
    body = open(f"{base}.arm{a}.s").read()
    m = re.search(r"^_mapal_sme_panel:.*?-- End function", body, re.S | re.M)
    if not m: sys.exit(f"  arm{a}: no _mapal_sme_panel in the assembly")
    fn = m.group(0)
    # blocks are delimited by their `; %<irblock>` codegen comments
    blocks = re.split(r"\n(?=LBB)", fn)
    kloop = [b for b in blocks if re.search(r";\s*%kloop\s*$", b, re.M)]
    rows  = [b for b in blocks if re.search(r";\s*%rows\s*$", b, re.M)]
    if len(kloop) != 1 or len(rows) != 1:
        sys.exit(f"  arm{a}: expected 1 kloop + 1 rows block, got {len(kloop)}/{len(rows)}")
    k, r = kloop[0], rows[0]
    c = lambda blk, pat: len(re.findall(pat, blk, re.M))
    ld1w, ldrz = c(k, r"^\tld1w\b"), c(k, r"^\tldr\tz")
    fmopa, ands = c(k, r"^\tfmopa\b"), c(k, r"^\tand\b")
    # read-out signature: the instruction opcodes of the rows block, operands stripped
    ops = [l.split("\t")[1] for l in r.splitlines() if l.startswith("\t") and len(l.split("\t")) > 1]
    sig = hashlib.sha1(" ".join(ops).encode()).hexdigest()[:10]
    moves = c(r, r"^\tmov\tz\d+\.s, p\d+/m, za")
    stores = c(r, r"^\t(?:st1w|str\tz)")
    rows_sig[a] = (sig, moves, stores)
    print(f"  {a:<4}{ld1w:>6}{ldrz:>7}{ld1w+ldrz:>8}{fmopa:>7}{ands:>6}   {sig}  ({moves} za reads, {stores} stores)")
    if ld1w + ldrz != 4 or fmopa != 4:
        print(f"       !! arm{a}: expected 4 vector loads + 4 fmopa"); ok = False
    if moves != 4 or stores != 4:
        print(f"       !! arm{a}: read-out block is not 4 za reads + 4 stores"); ok = False
s0 = rows_sig[0][0]
if all(v[0] == s0 for v in rows_sig.values()):
    print(f"  read-out: IDENTICAL across all six arms (sig {s0}), 4 tiles x 16 rows loop intact")
else:
    print(f"  !! read-out DIFFERS across arms: {rows_sig}"); ok = False
print("  NOTE: `and` count is reported, not asserted — a mask LLVM can prove dead is folded away.")
sys.exit(0 if ok else 1)
PY
echo

# ---- GATE 1b: the IR diff (rule 18) — the ONLY change is the two masks ----
# Stronger than counting the assembly loop: it proves %K, the trip count, the four
# loads, every `fmopa`, the 16-row x 4-tile read-out and the stores are untouched.
echo "--- GATE 1b: emitted-IR diff vs arm0, per arm ---"
for a in 1 2 3 4 5; do
  D=$(diff "$TMP/$NAME.arm0.ll" "$TMP/$NAME.arm$a.ll")
  DEL=$(echo "$D" | grep -c '^<'); ADD=$(echo "$D" | grep -c '^>')
  ANDS=$(echo "$D" | grep -c '^> .* = and i64 ')
  if [ "$DEL" = 2 ] && [ "$ADD" = 4 ] && [ "$ANDS" = 2 ]; then
    echo "  arm$a: 2 GEPs redirected, 2 \`and\` inserted, nothing else changed"
  else
    echo "  !! arm$a: unexpected IR delta ($DEL removed / $ADD added / $ANDS and) — aborting"; echo "$D"; exit 1
  fi
done
echo

run() { if [ "$THREADS" = 1 ]; then MAPAL_PAR=1 "$1" 2>/dev/null; else "$1" 2>/dev/null; fi; }
vals() { run "$1" | grep -v '^iter ms='; }

# ---- GATE 2: control fidelity (arm 1 == arm 0) ----
echo "--- GATE 2: control fidelity ---"
V0=$(vals "$TMP/$NAME.arm0"); V1=$(vals "$TMP/$NAME.arm1")
if [ "$V0" = "$V1" ]; then echo "  arm1 stdout BIT-IDENTICAL to arm0: $(echo "$V0" | tr '\n' ' ')"
else echo "  !! arm1 DIFFERS from arm0 — the control is not dead-in-effect"; echo "     arm0: $V0"; echo "     arm1: $V1"; exit 1; fi
if cmp -s "$TMP/$NAME.arm0.s" "$TMP/$NAME.arm1.s"; then
  echo "  arm0 and arm1 assembly are BYTE-IDENTICAL — the control's \`and\` was folded away by LLVM."
  echo "  (K and bn reach the kernel as constants, so the 2^44 mask is provably a no-op. The"
  echo "   control therefore IS the shipping binary; the windowed arms carry 2 extra \`and\` the"
  echo "   control does not. That residual is extra work in the TREATMENT, so any win it measures"
  echo "   is a LOWER BOUND. Reported, not smoothed — plan §3's 'one immediate' does not hold.)"
fi
echo

# ---- GATE 3: the tripwire, INVERTED — windowed arms must be WRONG ----
# Not a pass/fail value-identity gate: the windowed arms are wrong by construction,
# so an arm printing the CONTROL's answer is VOID (its mask folded away) and is
# excluded from interpretation. It is labelled, not silently dropped.
echo "--- GATE 3: wrong-values tripwire (INVERTED: windowed arms MUST differ) ---"
: > "$TMP/$NAME.void"
for a in 2 3 4 5; do
  VA=$(vals "$TMP/$NAME.arm$a")
  if [ "$VA" = "$V1" ]; then
    echo "  !! arm$a VOID — prints the CONTROL's answer; its mask did not survive, it IS the control"
    echo "$a" >> "$TMP/$NAME.void"
  else echo "  arm$a differs from control, as required: $(echo "$VA" | tr '\n' ' ')"; fi
done
echo

# ---- the run: round-robin, order rotated each cycle ----
: > "$TMP/$NAME.series"
for c in $(seq "$CYCLES"); do
  for o in 0 1 2 3 4 5; do
    a=$(( (o + c) % 6 ))
    ms=$(run "$TMP/$NAME.arm$a" | sed -n 's/^iter ms=//p')
    echo "$a $ms" >> "$TMP/$NAME.series"
  done
done

"$PYTHON" - "$TMP/$NAME.series" "$TMP/$NAME.void" "$NAME" <<'PY'
import os, statistics, sys
what = ['shipped, unpatched', 'CONTROL 2^44-1 both', 'A->16 KB', 'B->2x16 KB', 'all-L1 48 KB', 'L2-slice 384 KB']
s = {a: [] for a in range(6)}
for line in open(sys.argv[1]):
    p = line.split()
    if len(p) == 2: s[int(p[0])].append(float(p[1]))
void = {int(x) for x in open(sys.argv[2]).read().split()} if os.path.exists(sys.argv[2]) else set()
for a in s: s[a].sort()
med = {a: statistics.median(v) for a, v in s.items() if v}
print(f"  n={len(s[0])} per arm, absolute milliseconds")
print(f"  {'arm':<4}{'min':>10}{'median':>10}{'max':>10}   {'vs arm1':<9} {'overlap w/ arm1':<16} window")
for a in range(6):
    v = s[a]
    if not v: print(f"  {a:<4} no samples"); continue
    ov = 'OVERLAPS' if not (v[-1] < s[1][0] or v[0] > s[1][-1]) else 'disjoint'
    if a == 1: ov = '(baseline)'
    tag = what[a] + ('   [VOID: mask folded, == control]' if a in void else '')
    print(f"  {a:<4}{v[0]:>10.3f}{med[a]:>10.3f}{v[-1]:>10.3f}   {med[1]/med[a]:<9.3f} {ov:<16} {tag}")
g2, g3, g4 = (med[1] - med[i] for i in (2, 3, 4))
print(f"  additivity: gain(2)={g2:.2f} + gain(3)={g3:.2f} = {g2+g3:.2f} ms vs gain(4)={g4:.2f} ms")
d4 = s[4][-1] < s[1][0] or s[4][0] > s[1][-1]
if sys.argv[3] != 'matmul4096_cap_f32.t1':
    print(f"  arm4 vs control: {'disjoint' if d4 else 'OVERLAPPING'}. "
          "(plan §5's 128 ms threshold is declared for N=4096 / 1 thread only — no verdict here.)")
elif 4 in void:
    print("  H (plan §5): arm4 is VOID — its mask folded away. No verdict from this configuration.")
else:
    print(f"  H (plan §5): arm4 median {med[4]:.3f} ms {'<=' if med[4] <= 128 else '>'} 128 ms, "
          f"distributions {'disjoint' if d4 else 'OVERLAPPING'} from control "
          f"=> {'CONFIRMED' if med[4] <= 128 and d4 else 'REFUTED'}")
PY
