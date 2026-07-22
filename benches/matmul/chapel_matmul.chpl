// Naive GEMM in Chapel — the parallel-language baseline: the same i/j/k math
// per cell as the Flow program's cell fn and rust_naive.rs, with the (i,j)
// space expressed as a 2D domain and Chapel's headline `forall` data
// parallelism (tasks default to the physical core count). One binary covers
// both widths: real(32)/real(64) via a generic proc selected by `--width=`.
// Build: CHPL_TARGET_CPU=native chpl --fast chapel_matmul.chpl -o chapel_matmul
//   (--fast is Chapel's documented performance idiom — CPU specialization is
//    `native` by default on linux64, and bounds checks are omitted)
// Usage: chapel_matmul [--n=512] [--iters=50] [--width=f32|f64]
use Time;
use IO.FormattedIO;

config const n = 512;
config const iters = 50;
config const width = "f32";

proc run(type T, leg: string) {
    const D = {0..<n, 0..<n};
    var a, b, c: [D] T;
    forall (i, j) in D {
        const t = i * n + j;
        a[i, j] = (((t * 7 + 13) % 101) - 50): T;
        b[i, j] = (((t * 7 + 57) % 101) - 50): T;
    }
    var sw: stopwatch;
    var best = 1.0e300;
    for 1..iters {
        sw.restart();
        forall (i, j) in D {
            var acc: T = 0;
            for k in 0..<n do
                acc += a[i, k] * b[k, j];
            c[i, j] = acc;
        }
        best = min(best, sw.elapsed());
    }
    const ms = best * 1e3;
    try! writef("%s N=%i %.4dr ms %.1dr GFLOP/s c0=%.1dr clast=%.1dr\n",
                leg, n, ms, 2.0 * (n ** 3) / (ms * 1e6), c[0, 0], c[n - 1, n - 1]);
}

if width == "f64" then
    run(real(64), "chapel-f64");
else
    run(real(32), "chapel-f32");
