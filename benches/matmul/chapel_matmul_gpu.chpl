// Naive GEMM in Chapel on the GPU locale — the direct-competitor GPU leg:
// the same per-cell i/j/k math as chapel_matmul.chpl (and the Mapal cell fn),
// with data and the forall on here.gpus[0] so the loop compiles to a GPU
// kernel. Requires a gpu-enabled chpl (CHPL_LOCALE_MODEL=gpu, CHPL_GPU=nvidia
// source build — the binary .deb is CPU-locale only).
// Build: chpl --fast chapel_matmul_gpu.chpl -o chapel_matmul_gpu
// Usage: chapel_matmul_gpu [--n=512] [--iters=50] [--width=f32|f64]
// Timing: stopwatch around the forall (gpu foralls synchronize on exit) —
// per-iteration compute, init/warmup excluded, best-of-iters: the same
// convention as every other baseline leg.
use Time;
use IO.FormattedIO;

config const n = 512;
config const iters = 50;
config const width = "f32";

proc run(type T, leg: string) {
    var ms: real;
    var c0, clast: real;
    on here.gpus[0] {
        const D = {0..<n*n};
        var a, b, c: [D] T;
        forall t in D {
            a[t] = (((t * 7 + 13) % 101) - 50): T;
            b[t] = (((t * 7 + 57) % 101) - 50): T;
        }
        // warmup: kernel/module load out of the timed region
        forall t in D {
            const i = t / n, j = t % n;
            var acc: T = 0;
            for k in 0..<n do acc += a[i*n+k] * b[k*n+j];
            c[t] = acc;
        }
        var sw: stopwatch;
        var best = 1.0e300;
        for 1..iters {
            sw.restart();
            forall t in D {
                const i = t / n, j = t % n;
                var acc: T = 0;
                for k in 0..<n do acc += a[i*n+k] * b[k*n+j];
                c[t] = acc;
            }
            best = min(best, sw.elapsed());
        }
        ms = best * 1e3;
        c0 = c[0]: real;
        clast = c[n*n-1]: real;
    }
    try! writef("%s N=%i %.4dr ms %.1dr GFLOP/s c0=%.1dr clast=%.1dr\n",
                leg, n, ms, 2.0 * ((n: real) ** 3) / (ms * 1e6), c0, clast);
}

if width == "f64" then
    run(real(64), "chapel-gpu-f64");
else
    run(real(32), "chapel-gpu-f32");
