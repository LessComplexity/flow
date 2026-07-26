#!/usr/bin/env bash
# Emit every bench artifact from its .mapal source through the optimizer, so a
# bench leg measures the FULL pipeline (S22 item 1).
#
# The artifacts are NOT checked in: they are 4.4 MB of compiler output derived
# from sources already in the tree. Run this before a bench sweep, or to inspect
# what the emitter produces.
#
# This used to decide what to regenerate by looking at which artifacts already
# existed on disk — which made the checked-in output its own worklist, and meant
# deleting it silently reduced the script to doing nothing. The faces are now
# derived from the source name:
#
#   every stem          .ll + .cu
#   *_cap / *_cap_f32   also _perf.ll, _fma.ll, _fma_perf.ll, _perf.cu
#
# ponytail: sequential; parallelize if the corpus outgrows a coffee break.
set -uo pipefail
cd "$(dirname "$0")/../.."

cargo build -q --release -p mapal-backend-llvm -p mapal-backend-cuda --examples

emit_ll() { cargo run -q --release -p mapal-backend-llvm --example emit -- "$@"; }
emit_cu() { cargo run -q --release -p mapal-backend-cuda --example emit -- "$@"; }

# The CUDA leg legitimately rejects some sources (a `time` builtin has no CUDA
# realization — it is a recorded ✋ cell, not a bug), so a refusal there must not
# abort the whole sweep. Report it and carry on.
skipped=()
try() { # <label> <command...>
  local label="$1"; shift
  if ! "$@" >/dev/null 2>&1; then
    skipped+=("$label")
  fi
}

for src in benches/matmul/*.mapal; do
  stem="${src%.mapal}"

  try "$stem.ll" emit_ll "$src" --rewrite
  try "$stem.cu" emit_cu "$src" --rewrite

  case "$stem" in
    *_cap | *_cap_f32)
      # The conformance artifact above stays contraction-free (it is the
      # differential face); the _fma pair is the bench/product face (plan-s27).
      emit_ll "$src" - --rewrite --perf >"${stem}_perf.ll" 2>/dev/null ||
        skipped+=("${stem}_perf.ll")
      emit_ll "$src" - --rewrite --contract >"${stem}_fma.ll" 2>/dev/null ||
        skipped+=("${stem}_fma.ll")
      emit_ll "$src" - --rewrite --contract --perf >"${stem}_fma_perf.ll" 2>/dev/null ||
        skipped+=("${stem}_fma_perf.ll")
      emit_cu "$src" - --rewrite --perf >"${stem}_perf.cu" 2>/dev/null ||
        skipped+=("${stem}_perf.cu")
      ;;
  esac

  echo "regen: $stem"
done

if [ ${#skipped[@]} -ne 0 ]; then
  echo
  echo "refused by a backend (${#skipped[@]}):"
  printf '  %s\n' "${skipped[@]}"
fi
echo "done."
