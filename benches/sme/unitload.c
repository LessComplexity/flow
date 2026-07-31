// What does a LOAD cost on a SHARED matrix unit? (S43)
//
// THE GAP. `units.c` measures the aggregate `fmopa` ceiling of this part at
// ~4100 GFLOP/s with ZERO memory traffic. The real threaded kernel reaches 2531.
// `loadcost.c` measured that adding 4 loads per 4 `fmopa` costs only 5% AT ONE
// THREAD when the operands are L1-resident — so at one thread the load path is
// nearly free. S43 then showed operand residency is worth <=5% THREADED too.
// Both instruments say "the bytes are not the problem". Something else is.
//
// THE HYPOTHESIS. `units.c` has no loads. `loadcost.c` has loads but only one
// thread, which is latency-bound at 4 chains (roofline.c §1) and therefore has
// spare issue slots to hide loads in. Neither probe can see the case that
// matters: SEVERAL threads sharing ONE unit, each also issuing loads. If the
// streaming-mode loads are retired by the same shared block as `fmopa` — Apple's
// SME is a per-cluster coprocessor, and in streaming mode the Z registers live
// there — then loads and `fmopa` COMPETE for the shared issue bandwidth, and the
// 4100 ceiling measured without loads is not the ceiling a 1-load-per-`fmopa`
// kernel can reach.
//
// THIS PROBE IS `units.c` x `loadcost.c`: N threads, each running the SAME four
// independent `fmopa` into the four f32 ZA tiles, with L of their operands
// coming from an L1-resident per-thread buffer instead of from registers loaded
// once. Sweeping (threads, loads) separates the two:
//
//   loads cost issue slots on the shared unit  =>  aggregate at L=4 falls FAR
//     below the L=0 aggregate once the unit is saturated (>= 4 threads), while
//     the 1-thread rows stay ~95% as loadcost.c measured.
//   loads are free on the shared unit          =>  aggregate is FLAT in L at
//     every thread count, and the threaded kernel's deficit is elsewhere.
//
// Buffers are PER THREAD (32 KB each) so residency is real per core and no
// sharing artifact can creep in. A second pass uses one shared 64 MB buffer to
// keep the memory-bound case in view, but the L1 pass is the one that answers
// the question — it holds bytes-from-cache constant and varies only issue.
//
// Build: clang -O2 -march=armv8-a+sme2 -o unitload unitload.c
//        (NEVER armv9-a: implies +sve, this part has SME without SVE, SIGILL)
#include <arm_sme.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// Every variant: 4 fmopa into za0..za3, `iters` times. Only the operand SOURCE
// differs. Bodies are byte-for-byte loadcost.c's, so the two probes are directly
// comparable and the 1-thread column of this one must reproduce that one.
#define LOADS(NAME, BODY)                                                       \
    __arm_new("za")                                                             \
    static float NAME(const float *buf, long mask, long iters) __arm_streaming { \
        svbool_t pg = svptrue_b32();                                            \
        svzero_za();                                                            \
        svfloat32_t z0 = svld1_f32(pg, buf);                                    \
        svfloat32_t z1 = svld1_f32(pg, buf + 16);                               \
        long off = 0;                                                           \
        for (long k = 0; k < iters; k++) {                                      \
            const float *p = buf + (off & mask);                                \
            BODY                                                                \
            off += 64;                                                          \
        }                                                                       \
        return svlasta_f32(svpfalse(), svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0)); \
    }

LOADS(ld0,
    svmopa_za32_f32_m(0, pg, pg, z0, z0);
    svmopa_za32_f32_m(1, pg, pg, z0, z1);
    svmopa_za32_f32_m(2, pg, pg, z1, z0);
    svmopa_za32_f32_m(3, pg, pg, z1, z1);
    (void)p;)

LOADS(ld1,
    svfloat32_t a0 = svld1_f32(pg, p);
    svmopa_za32_f32_m(0, pg, pg, a0, z0);
    svmopa_za32_f32_m(1, pg, pg, a0, z1);
    svmopa_za32_f32_m(2, pg, pg, z1, a0);
    svmopa_za32_f32_m(3, pg, pg, z1, z1);)

LOADS(ld2,
    svfloat32_t a0 = svld1_f32(pg, p);
    svfloat32_t b0 = svld1_f32(pg, p + 16);
    svmopa_za32_f32_m(0, pg, pg, a0, b0);
    svmopa_za32_f32_m(1, pg, pg, a0, z1);
    svmopa_za32_f32_m(2, pg, pg, z1, b0);
    svmopa_za32_f32_m(3, pg, pg, z1, z1);)

LOADS(ld3,
    svfloat32_t a0 = svld1_f32(pg, p);
    svfloat32_t a1 = svld1_f32(pg, p + 16);
    svfloat32_t b0 = svld1_f32(pg, p + 32);
    svmopa_za32_f32_m(0, pg, pg, a0, b0);
    svmopa_za32_f32_m(1, pg, pg, a0, z1);
    svmopa_za32_f32_m(2, pg, pg, a1, b0);
    svmopa_za32_f32_m(3, pg, pg, a1, z1);)

// The real emitted kernel: 2 A vectors, 2 B vectors, 4 fmopa (ti=tj=2, t=16).
LOADS(ld4,
    svfloat32_t a0 = svld1_f32(pg, p);
    svfloat32_t a1 = svld1_f32(pg, p + 16);
    svfloat32_t b0 = svld1_f32(pg, p + 32);
    svfloat32_t b1 = svld1_f32(pg, p + 48);
    svmopa_za32_f32_m(0, pg, pg, a0, b0);
    svmopa_za32_f32_m(1, pg, pg, a0, b1);
    svmopa_za32_f32_m(2, pg, pg, a1, b0);
    svmopa_za32_f32_m(3, pg, pg, a1, b1);)

typedef float (*fn)(const float *, long, long) __arm_streaming;

typedef struct {
    fn f;
    const float *buf;
    long mask;
    long iters;
    double sink;
    double ms;
} Arg;

static void *worker(void *p) {
    Arg *a = (Arg *)p;
    double t0 = now_ms();
    a->sink = a->f(a->buf, a->mask, a->iters);
    a->ms = now_ms() - t0;
    return NULL;
}

// Aggregate GFLOP/s with `n` threads. Wall clock over the whole join, exactly as
// units.c does it, so the two numbers are the same measurement.
static double measure(fn f, float **bufs, const float *shared, long mask, long iters,
                      int n, double *worst_ms, double *best_ms, double *sink) {
    pthread_t th[64];
    Arg args[64];
    for (int i = 0; i < n; i++) {
        args[i] = (Arg){ .f = f, .buf = shared ? shared : bufs[i], .mask = mask,
                         .iters = iters, .sink = 0, .ms = 0 };
    }
    double t0 = now_ms();
    for (int i = 0; i < n; i++) pthread_create(&th[i], NULL, worker, &args[i]);
    for (int i = 0; i < n; i++) pthread_join(th[i], NULL);
    double dt = now_ms() - t0;
    *worst_ms = 0; *best_ms = 1e18;
    for (int i = 0; i < n; i++) {
        if (args[i].ms > *worst_ms) *worst_ms = args[i].ms;
        if (args[i].ms < *best_ms) *best_ms = args[i].ms;
        *sink += args[i].sink;
    }
    // 4 fmopa per iteration, 512 flops each, in EVERY variant.
    return 4.0 * 512.0 * (double)n * (double)iters / (dt * 1e6);
}

// --- PROBE 3: the per-thread buffer-SIZE sweep, at the real kernel's 4 loads.
//
// Pass A above uses a 32 KB per-thread buffer, i.e. PURE L1 with zero L2 traffic.
// The real kernel never gets that: each `mapal_sme_panel` call sweeps ~1 MB (512 KB
// of `ap` plus two 256 KB packed-b panels) once, with no reuse inside the call, so
// EVERY load misses L1 and is served by the shared L2. `loadlevel.c` measured
// 249 GB/s from L2 to ONE thread and found L1-vs-L2 free; nothing has measured what
// 14 cores refilling L1 from the shared L2 get at once.
//
// So sweep the per-thread buffer size at loads=4, keeping the aggregate inside the
// 16 MB L2 for the small sizes and past it for the largest. `ld0` is carried as the
// control: it issues no loads, so its row must stay flat at ~4100 at every size, and
// any droop in it is thermal, not the effect.
static void size_sweep(long iters, int reps, int maxt) {
    // per-thread bytes; 14 x 1 MB = 14 MB still inside the 16 MB shared L2,
    // 14 x 4 MB = 56 MB well past it.
    // SIZE MUST NOT BE THE OUTER LOOP. A first attempt swept size-outer and the
    // `ld0` control -- which issues NO loads and therefore cannot depend on the
    // buffer at all -- tracked `ld4` down from 1996 to 1336 GF/s. That is global
    // clock drift landing squarely on the size axis, exactly the defect §7.1
    // records for `roofline.c`. The rep loop is outermost here, so every (size,
    // threads, variant) cell is revisited once per rep and drift spreads across
    // all of them instead of tilting one axis. Every buffer is allocated and
    // faulted in UP FRONT for the same reason.
    long sizes[] = {32L << 10, 128L << 10, 256L << 10, 512L << 10, 1L << 20, 4L << 20};
    int nsz = (int)(sizeof sizes / sizeof *sizes);
    int tcounts[] = {1, 2, 4, 8, 14};
    int ntc = (int)(sizeof tcounts / sizeof *tcounts);
    static float *bufs[6][64];
    double best[6][8][2];

    for (int s = 0; s < nsz; s++) {
        long words = sizes[s] / 4;
        for (int i = 0; i < maxt; i++) {
            bufs[s][i] = aligned_alloc(64, sizes[s]);
            for (long j = 0; j < words; j++) bufs[s][i][j] = 1.0f + (j % 7) * 0.01f;
        }
        for (int ti = 0; ti < ntc; ti++) best[s][ti][0] = best[s][ti][1] = 0;
    }

    double sink = 0;
    for (int r = 0; r < reps; r++) {
        for (int s = 0; s < nsz; s++) {
            long mask = sizes[s] / 4 - 64;
            for (int ti = 0; ti < ntc; ti++) {
                double w, b;
                double g4 = measure(ld4, bufs[s], NULL, mask, iters, tcounts[ti], &w, &b, &sink);
                double g0 = measure(ld0, bufs[s], NULL, mask, iters, tcounts[ti], &w, &b, &sink);
                if (g4 > best[s][ti][0]) best[s][ti][0] = g4;
                if (g0 > best[s][ti][1]) best[s][ti][1] = g0;
            }
        }
        fprintf(stderr, "  rep %d/%d done\n", r + 1, reps);
    }

    printf("\n=== 4 loads, PER-THREAD buffer of varying size (ld0 control in parens) ===\n");
    printf("%12s", "buffer");
    for (int i = 0; i < ntc; i++) printf("  %8dthr%7s", tcounts[i], "");
    printf("\n");
    for (int s = 0; s < nsz; s++) {
        printf("%9ld KB", sizes[s] >> 10);
        for (int ti = 0; ti < ntc; ti++)
            printf(" %11.1f (%5.0f)", best[s][ti][0], best[s][ti][1]);
        printf("\n");
    }
    for (int s = 0; s < nsz; s++)
        for (int i = 0; i < maxt; i++) free(bufs[s][i]);

    printf("\nReading it: ld0 has NO loads, so its control number CANNOT depend on the buffer\n"
           "size. If the ld0 column is not flat, the run is void -- that is drift, not an\n"
           "effect. If ld0 is flat and the 4-load number falls as the buffer passes the\n"
           "128 KB L1D while the aggregate is still inside the 16 MB L2, the binder is\n"
           "shared-L2 refill bandwidth at 14 cores -- the regime the real kernel runs in.\n");
    printf("(sink %.1f)\n", sink);
}

int main(int argc, char **argv) {
    long iters = argc > 1 ? atol(argv[1]) : 40000000L;
    int reps = argc > 2 ? atoi(argv[2]) : 3;
    int pass_big = argc > 3 ? atoi(argv[3]) : 1;
    int only_size = argc > 4 ? atoi(argv[4]) : 0;

    int tcounts[] = {1, 2, 3, 4, 6, 8, 10, 12, 14};
    int ntc = (int)(sizeof tcounts / sizeof *tcounts);
    int maxt = tcounts[ntc - 1];

    long small = 32 * 1024 / 4;           // 32 KB per thread, L1-resident
    long big = 64L * 1024 * 1024 / 4;     // 64 MB shared, past L2
    float *bufs[64];
    for (int i = 0; i < maxt; i++) {
        bufs[i] = aligned_alloc(64, small * 4);
        for (long j = 0; j < small; j++) bufs[i][j] = 1.0f + (j % 7) * 0.01f;
    }
    float *bb = NULL;
    if (pass_big) {
        bb = aligned_alloc(64, big * 4);
        for (long j = 0; j < big; j++) bb[j] = 1.0f + (j % 7) * 0.01f;
    }

    // Warm the clock: 1.73x cold/warm on identical code on this part (rule 14).
    double sink = 0, w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += ld0(bufs[0], small - 64, 200000);

    fn fns[5] = {ld0, ld1, ld2, ld3, ld4};
    const char *names[5] = {"0 loads", "1 load", "2 loads", "3 loads", "4 loads"};

    if (only_size) {
        for (int i = 0; i < maxt; i++) free(bufs[i]);
        if (bb) free(bb);
        size_sweep(iters, reps, maxt);
        return 0;
    }

    for (int pass = 0; pass < (pass_big ? 2 : 1); pass++) {
        const float *shared = pass ? bb : NULL;
        long mask = (pass ? big : small) - 64;
        printf("\n=== operands from %s ===\n",
               pass ? "one shared 64 MB buffer (past L2)"
                    : "a per-thread 32 KB buffer (L1-resident)");
        printf("%8s", "threads");
        for (int v = 0; v < 5; v++) printf(" %13s", names[v]);
        printf("   %10s %10s\n", "L4/L0", "L4 slow ms");

        for (int ti = 0; ti < ntc; ti++) {
            int n = tcounts[ti];
            double best[5] = {0, 0, 0, 0, 0};
            double slow4 = 0;
            // rep OUTER, variant INNER: the 5 variants alternate (rule 14).
            for (int r = 0; r < reps; r++) {
                for (int v = 0; v < 5; v++) {
                    double w, b;
                    double g = measure(fns[v], bufs, shared, mask, iters, n, &w, &b, &sink);
                    if (g > best[v]) { best[v] = g; if (v == 4) slow4 = w; }
                }
            }
            printf("%8d", n);
            for (int v = 0; v < 5; v++) printf(" %13.1f", best[v]);
            printf("   %9.3fx %10.1f\n", best[4] / best[0], slow4);
            fflush(stdout);
        }
    }

    printf("\nReading it: the L4/L0 column is what a load costs ON THE SHARED UNIT.\n"
           "If it stays ~0.95 at every thread count, loads never compete for issue and\n"
           "the threaded kernel's deficit is NOT the load stream. If it collapses as\n"
           "threads rise, the 4100 GFLOP/s no-load ceiling is not a 1-load-per-fmopa\n"
           "kernel's ceiling, and 4100 / (L4/L0) is.\n");
    printf("(sink %.1f)\n", sink);
    return 0;
}
