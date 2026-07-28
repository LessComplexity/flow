#!/bin/zsh
zmodload zsh/datetime
SCRATCH=$1
sweep() {
  local bin=$1
  for f in benches/shapes/*.mapal benches/matmul/*.mapal examples/*.mapal; do
    for a in "" "--rewrite" "--rewrite --contract"; do
      $bin "$f" - ${=a} > /dev/null 2>/dev/null
    done
  done
}
# warmup both
sweep $SCRATCH/emit_pre; sweep $SCRATCH/emit_post
for i in {1..51}; do
  t0=$EPOCHREALTIME; sweep $SCRATCH/emit_pre;  t1=$EPOCHREALTIME
  t2=$EPOCHREALTIME; sweep $SCRATCH/emit_post; t3=$EPOCHREALTIME
  echo "pre $((t1-t0)) post $((t3-t2))"
done
