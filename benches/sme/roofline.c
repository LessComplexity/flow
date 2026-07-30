// S42 re-diagnosis: k-loop pipelining bought 0.6% (see mm4p.c), so the gap to
// Accelerate is not scheduling of the fmopa stream. This asks where it IS, by
// measuring everything in ONE warmed process:
//
//   1. fmopa issue roofline, NO memory traffic, at 1 / 2 / 4 dependency chains.
//      Each ZA tile is one loop-carried chain, exactly as in the GEMM. The
//      sweep separates "the port is saturated" from "we ran out of chains".
//   2. the real 2x2 GEMM panel (mm4.c's kernel verbatim).
//   3. the A-panel transpose-gather alone -- it sits inside the timed region
//      of every SME probe here, and the Mapal rung pays it too.
//
// WARMUP IS LOAD-BEARING. The same binary measured this roofline at 1.852 ms
// cold and 1.069 ms warm -- a 1.73x swing on identical code. Anything timed on
// this part in the first ~100 ms of a process is measuring the clock ramp.
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// iters x CHAINS fmopa, zero loads. Returns za0[0][0] so nothing is dead.
#define ISSUE_ONLY(NAME, BODY)                                             \
    __arm_new("za")                                                        \
    static float NAME(const float *seed, long iters) __arm_streaming {     \
        svbool_t pg = svptrue_b32();                                       \
        svzero_za();                                                       \
        svfloat32_t z0 = svld1_f32(pg, seed);                              \
        svfloat32_t z1 = svld1_f32(pg, seed + 16);                         \
        for (long k = 0; k < iters; k++) { BODY }                          \
        svfloat32_t out = svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0);  \
        return svlasta_f32(svpfalse(), out);                               \
    }

ISSUE_ONLY(issue1, svmopa_za32_f32_m(0, pg, pg, z0, z0);)
ISSUE_ONLY(issue2, svmopa_za32_f32_m(0, pg, pg, z0, z0);
                   svmopa_za32_f32_m(1, pg, pg, z0, z1);)
ISSUE_ONLY(issue4, svmopa_za32_f32_m(0, pg, pg, z0, z0);
                   svmopa_za32_f32_m(1, pg, pg, z0, z1);
                   svmopa_za32_f32_m(2, pg, pg, z1, z0);
                   svmopa_za32_f32_m(3, pg, pg, z1, z1);)

typedef float (*issue_fn)(const float *, long) __arm_streaming;

// mm4.c's kernel, verbatim.
__arm_new("za")
static void mm_panel4(const float *ap0, const float *ap1, const float *b,
                      float *c, int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32_t zn0 = svld1_f32(pg, &ap0[k * 16]);
        svfloat32_t zn1 = svld1_f32(pg, &ap1[k * 16]);
        svfloat32_t zm0 = svld1_f32(pg, &b[k * N + j0]);
        svfloat32_t zm1 = svld1_f32(pg, &b[k * N + j0 + 16]);
        svmopa_za32_f32_m(0, pg, pg, zn0, zm0);
        svmopa_za32_f32_m(1, pg, pg, zn0, zm1);
        svmopa_za32_f32_m(2, pg, pg, zn1, zm0);
        svmopa_za32_f32_m(3, pg, pg, zn1, zm1);
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

static void pack_panel(const float *a, float *ap0, float *ap1, int N, int i0) {
    for (int k = 0; k < N; k++)
        for (int i = 0; i < 16; i++) {
            ap0[k * 16 + i] = a[(i0 + i) * N + k];
            ap1[k * 16 + i] = a[(i0 + 16 + i) * N + k];
        }
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 1024, REPS = argc > 2 ? atoi(argv[2]) : 9;
    double sink = 0;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4);
    float *ap0 = aligned_alloc(64, (size_t)N*16*4), *ap1 = aligned_alloc(64, (size_t)N*16*4);
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }

    // --- warmup: spin fmopa until the clock has ramped (see header note) ---
    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    long total = (long)N * N * N / 256;   // the GEMM's fmopa count at this N
    double gflop = 2.0 * N * N * N / 1e6; // per GEMM, for ms -> GFLOP/s

    printf("N=%d, best of %d, 1 thread, after 300 ms warmup\n", N, REPS);
    printf("  %-26s %10s %12s %10s\n", "", "ms", "GFLOP/s", "% roofline");

    issue_fn fns[3] = {issue1, issue2, issue4};
    int chains[3] = {1, 2, 4};
    double roof = 0;
    for (int v = 0; v < 3; v++) {
        double best = 1e18;
        for (int r = 0; r < REPS; r++) {
            double t0 = now_ms();
            sink += fns[v](seed, total / chains[v]);
            double dt = now_ms() - t0;
            if (dt < best) best = dt;
        }
        double gf = 512.0 * total / (best * 1e6);
        if (gf > roof) roof = gf;
        char label[40];
        snprintf(label, sizeof label, "fmopa only, %d chain%s", chains[v], chains[v] == 1 ? "" : "s");
        printf("  %-26s %10.4f %12.1f\n", label, best, gf);
    }

    double best = 1e18;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        for (int i0 = 0; i0 < N; i0 += 32) {
            pack_panel(a, ap0, ap1, N, i0);
            for (int j0 = 0; j0 < N; j0 += 32) mm_panel4(ap0, ap1, b, c, N, i0, j0);
        }
        double dt = now_ms() - t0;
        if (dt < best) best = dt;
    }
    printf("  %-26s %10.4f %12.1f %9.0f%%\n", "full GEMM (2x2 tiles)", best, gflop / best,
           100.0 * (gflop / best) / roof);

    double bestp = 1e18;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        for (int i0 = 0; i0 < N; i0 += 32) pack_panel(a, ap0, ap1, N, i0);
        double dt = now_ms() - t0;
        if (dt < bestp) bestp = dt;
    }
    printf("  %-26s %10.4f %11.1f%% of the GEMM\n", "  of which: A pack", bestp,
           100.0 * bestp / best);

    printf("  (c[0]=%.4f, sink=%.1f)\n", c[0], sink);
    return 0;
}
