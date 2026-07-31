#!/bin/bash
# Emit every source x N faces with a given emit binary; write one hash line per emission.
#
# Usage: benches/emit_sweep_ab.sh <emit-binary> <out.hashes> [extra emit flags...]
#
# WHY THIS IS DEFENSIVE. This script IS the byte-identity gate — the instrument that
# decides whether a compiler change moved an emitted byte. It used to be able to
# report a clean pass while measuring nothing, in two independent ways, and S43 hit
# the first one:
#
#   1. `#!/bin/zsh` plus `${=flags}` (zsh word-splitting). Run under bash it printed
#      `bad substitution` per line, passed NO flags, hashed the raw face 159 times,
#      and EXITED 0 — so a change gated behind `--rewrite`, `--contract` or
#      `--target=` was invisible and the diff came back empty. A fabricated pass on
#      exactly the gate you were trying to run.
#   2. `2>/dev/null` with no output check. A FAILING emission hashes empty input,
#      which is the same constant every time, so two broken runs "match".
#
# Both are now hard errors. A gate that cannot fail is not a gate.
set -u

BIN=${1:?usage: emit_sweep_ab.sh <emit-binary> <out.hashes> [extra flags...]}
OUT=${2:?usage: emit_sweep_ab.sh <emit-binary> <out.hashes> [extra flags...]}
shift 2
EXTRA="$*"

[ -x "$BIN" ] || { echo "emit_sweep_ab: '$BIN' is not executable" >&2; exit 2; }

# PREFLIGHT — the third silent-pass path, and it is the nastiest.
# `crates/backends/cuda/examples/emit.rs` and `crates/backends/llvm/examples/emit.rs`
# BOTH build to `target/release/examples/emit`; cargo warns ("consider changing
# their names to be unique") and whichever crate built last wins. Point this script
# at that path after a plain `cargo build --example emit` and you may get the CUDA
# emitter, which REJECTS `--contract` — so the `con` face fails on every source, and
# a stale-vs-fresh comparison then differs on ~all cells for a reason that has
# nothing to do with the change under test. Build with `-p mapal-backend-llvm` and
# verify the binary actually understands the flags before hashing anything.
for probe in "--rewrite" "--contract" $EXTRA; do
  case $probe in -*) ;; *) continue ;; esac
  if "$BIN" --help 2>&1 | grep -q -- "unknown flag" ||
     ! "$BIN" "$(ls benches/shapes/*.mapal | head -1)" - $probe >/dev/null 2>&1; then
    echo "emit_sweep_ab: '$BIN' does not accept $probe — wrong emit binary?" >&2
    echo "emit_sweep_ab: build it as: cargo build --release -p mapal-backend-llvm --example emit" >&2
    exit 2
  fi
done

# sha256 of the empty string — what a failed emission would otherwise hash to.
EMPTY=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649991b7852b855

: > "$OUT"
n=0; fail=0
for f in benches/shapes/*.mapal benches/matmul/*.mapal examples/*.mapal; do
  for a in "raw::" "rew::--rewrite" "con::--rewrite --contract"; do
    face=${a%%::*}; flags=${a#*::}
    # Unquoted on purpose so the flag string word-splits; the bash shebang above is
    # what makes that portable, and is why `${=flags}` must not come back.
    out=$("$BIN" "$f" - $flags $EXTRA 2>/dev/null)
    rc=$?
    h=$(printf '%s' "$out" | shasum -a 256 | cut -d' ' -f1)
    n=$((n + 1))
    if [ $rc -ne 0 ] || [ -z "$out" ] || [ "${h#"$EMPTY"}" != "$h" ]; then
      echo "${f}|${face}|EMIT-FAILED-rc${rc}" >> "$OUT"
      echo "emit_sweep_ab: FAILED ${f} (${face}) rc=${rc}$([ -z "$out" ] && echo ' empty output')" >&2
      fail=$((fail + 1))
      continue
    fi
    echo "${f}|${face}|${h}" >> "$OUT"
  done
done

echo "$n"
if [ "$fail" -gt 0 ]; then
  echo "emit_sweep_ab: $fail of $n emissions FAILED — this run is NOT a valid baseline" >&2
  exit 1
fi
