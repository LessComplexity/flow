#!/bin/zsh
# Emit every source x 3 faces with a given emit binary; write one hash line per emission.
BIN=$1; OUT=$2
: > $OUT
for f in benches/shapes/*.mapal benches/matmul/*.mapal examples/*.mapal; do
  for a in "raw::" "rew::--rewrite" "con::--rewrite --contract"; do
    face=${a%%::*}; flags=${a#*::}
    h=$($BIN "$f" - ${=flags} 2>/dev/null | shasum -a 256 | cut -d' ' -f1)
    rc=$?
    echo "${f}|${face}|${h}" >> $OUT
  done
done
wc -l < $OUT
