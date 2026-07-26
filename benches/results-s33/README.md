# S33 raw benchmark logs

Machine tags (S26 standing rule):

- `mac-matmul-ab.log` — Apple M4 Pro, 10 P + 4 E, NEON, clang 22.1.8, rustc 1.95.0,
  numpy 2.x on Accelerate (AMX). `benches/matmul/matmul_ab.sh "512 1024 2048 4096" 3`.
- `i9-suite.log` — Intel i9-14900F, 8 P + 16 E (32 threads), AVX2 (no AVX-512),
  gcc 15.2 / rustc 1.90, numpy 2.3.5 on scipy-openblas 0.3.30 `DYNAMIC_ARCH`.
  Flow legs cross-compiled on the Mac (`-march=raptorlake`) and linked with gcc.
  Reported as `min median`.
- `i9-redo.log` — the remaining i9 legs plus the OpenBLAS/Flow thread sweeps.
  Reported as `min median max`.

## Two contaminated runs are deliberately NOT included

An orphaned `mm_cpp_1t` process survived a parent-only `kill` (the ssh carrying the
child `pkill` died with exit 255 before running it) and burned one core at 95% through
two measurement rounds. Those results were discarded, not published — the error was
~20% on threaded cells (4096 `cpp-mt` read 38,633 ms contaminated vs 32,219 clean) and
2× on `numpy-1t` at 4096 (1584.79 vs 786.24). `i9-redo.log` brackets itself with live
process and loadavg checks so cleanliness is asserted in the output.

## Not run

i9 `cpp-1t` and `rust-1t` at 4096: each iteration exceeded 10 minutes, so 5 runs of
each was ~108 minutes for two cells the 512/1024/2048 rows already establish and that
the M4 log already carries. Skipped for time.

## Read the par cells as medians

A help-first race in `flow_par_wait` makes ~3-4% of threaded runs self-time far too
low (one live case in `i9-redo.log`: `FLOW_PAR=32` min 0.0001 ms, median 1.5259).
Minimum is the wrong statistic for any threaded cell until that is fixed.
See `docs/performance/matmul/s33.md` §5.
