// How many SME matrix units does this part have?
//
// WHY IT MATTERS. The threaded SME curve saturates hard: 1 thread 789 GF/s,
// 4 -> 1993, 6 -> 2289, 10 -> 2515, 14 -> 2559 (N=4096, `docs/performance/`).
// Meanwhile the NEON leg scales 8.61x across the same cores (S41b). So the limit
// is a SHARED unit, not bandwidth, and the useful lane count for a matrix task is
// a machine fact the emitter should derive rather than inherit from
// `available_parallelism()`. That fact is the number of units, and it is not in
// any sysctl — `hw.optional.arm.FEAT_SME` says whether, never how many.
//
// METHOD. Each thread runs the `fmopa` issue loop with ZERO memory traffic:
// operands loaded once into two registers before the loop, four independent ZA
// chains, nothing else. No cache, no packing, no output. Aggregate throughput
// then measures exactly one thing — how many fmopa the machine can retire per
// second — so it rises linearly while units are free and goes FLAT at the unit
// count. The knee is the answer.
//
// STEERING. Thread affinity is advisory and largely ignored on Apple Silicon, so
// core placement is requested with QoS classes, which the scheduler does honour:
// USER_INTERACTIVE lands on P-cores, BACKGROUND on E-cores. `--qos` sweeps both,
// which also answers a second question: does the E-cluster have a matrix unit at
// all, or does an E-core running SME simply borrow a P-cluster's?
//
// The single-thread numbers validate the steering before any conclusion is drawn:
// if BACKGROUND does not measure slower per thread than USER_INTERACTIVE, the QoS
// request was not honoured and the placement half of this probe proves nothing.
#include <arm_sme.h>
#include <pthread.h>
#include <pthread/qos.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// 4 independent ZA chains, no loads in the loop. One iteration = 4 fmopa.
__arm_new("za")
static float issue4(const float *seed, long iters) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    svfloat32_t z0 = svld1_f32(pg, seed), z1 = svld1_f32(pg, seed + 16);
    for (long k = 0; k < iters; k++) {
        svmopa_za32_f32_m(0, pg, pg, z0, z0);
        svmopa_za32_f32_m(1, pg, pg, z0, z1);
        svmopa_za32_f32_m(2, pg, pg, z1, z0);
        svmopa_za32_f32_m(3, pg, pg, z1, z1);
    }
    return svlasta_f32(svpfalse(), svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0));
}

typedef struct {
    const float *seed;
    long iters;
    double sink;
    double ms;          // this thread's own elapsed time
} Arg;

static void *worker(void *p) {
    Arg *a = (Arg *)p;
    double t0 = now_ms();
    a->sink = issue4(a->seed, a->iters);
    a->ms = now_ms() - t0;
    return NULL;
}

// Aggregate GFLOP/s with `n` threads at the given QoS. Also reports the slowest
// thread, because a per-thread slowdown under contention is the signature of
// sharing a unit rather than of running out of cores.
static void measure(const float *seed, long iters, int n, qos_class_t qos,
                    double *gflops, double *worst_ms, double *best_ms) {
    pthread_t th[64];
    Arg args[64];
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    pthread_attr_set_qos_class_np(&attr, qos, 0);

    for (int i = 0; i < n; i++) {
        args[i] = (Arg){ .seed = seed, .iters = iters, .sink = 0, .ms = 0 };
    }
    double t0 = now_ms();
    for (int i = 0; i < n; i++) pthread_create(&th[i], &attr, worker, &args[i]);
    for (int i = 0; i < n; i++) pthread_join(th[i], NULL);
    double dt = now_ms() - t0;
    pthread_attr_destroy(&attr);

    double fmopa = (double)n * iters * 4.0;
    *gflops = 512.0 * fmopa / (dt * 1e6);
    *worst_ms = 0; *best_ms = 1e18;
    for (int i = 0; i < n; i++) {
        if (args[i].ms > *worst_ms) *worst_ms = args[i].ms;
        if (args[i].ms < *best_ms) *best_ms = args[i].ms;
    }
}

int main(int argc, char **argv) {
    long iters = argc > 1 ? atol(argv[1]) : 150000000L;  // ~150 ms per thread
    int reps = argc > 2 ? atoi(argv[2]) : 3;
    int maxt = argc > 3 ? atoi(argv[3]) : 14;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;

    // warm the clock (1.73x cold/warm on this part, measurement rule 14)
    double w0 = now_ms(), sink = 0;
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    struct { const char *name; qos_class_t q; int cap; } modes[] = {
        { "USER_INTERACTIVE (P-cores)", QOS_CLASS_USER_INTERACTIVE, maxt },
        { "BACKGROUND (E-cores)",       QOS_CLASS_BACKGROUND,       4 },
        { "DEFAULT (scheduler's choice)", QOS_CLASS_DEFAULT,        maxt },
    };

    for (unsigned m = 0; m < sizeof modes / sizeof *modes; m++) {
        printf("\n=== %s ===\n", modes[m].name);
        printf("%8s %12s %10s %12s %12s %10s\n",
               "threads", "GFLOP/s", "vs 1thr", "per-thread", "slowest ms", "fastest");
        double one = 0;
        for (int n = 1; n <= modes[m].cap; n++) {
            double best = 0, worst_ms = 0, best_ms = 0;
            for (int r = 0; r < reps; r++) {
                double g, w, b;
                measure(seed, iters, n, modes[m].q, &g, &w, &b);
                if (g > best) { best = g; worst_ms = w; best_ms = b; }
            }
            if (n == 1) one = best;
            printf("%8d %12.1f %10.2fx %12.1f %12.1f %10.1f\n",
                   n, best, best / one, best / n, worst_ms, best_ms);
        }
    }
    printf("\nReading it: aggregate GFLOP/s rises while units are free and goes FLAT\n"
           "at the unit count. `per-thread` falling while aggregate is flat means the\n"
           "threads are sharing a unit, not running out of cores.\n");
    printf("(sink %.1f)\n", sink);
    return 0;
}
