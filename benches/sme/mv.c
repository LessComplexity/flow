// S42: the load-instruction probe. Follows roofline.c, which found:
//   fmopa only, 4 chains  1.0690 ms  2008.9 GFLOP/s   <- the roofline
//   full GEMM (2x2)       2.0920 ms  1026.5 GFLOP/s   <- 51% of it
// The GEMM is almost exactly 2x the fmopa-only time for the same fmopa count,
// and mm4p.c already showed that REORDERING the loads buys 0.6%. So the cost is
// not load latency -- it is the four load instructions themselves competing
// with fmopa for issue. If that is right, SME2's contiguous multi-vector load
// (ld1w {z0.s,z1.s}) folding 4 load instructions into 2 should move the GEMM
// toward the roofline. If it does nothing, the limit is bytes, not slots.
//
// Both operand pairs are already adjacent in memory, so no new packing is
// needed beyond interleaving the two A row-blocks into ONE array:
//   A: ap[k*32 .. +15] and ap[k*32+16 .. +31]   (interleaved pack, below)
//   B: b[k*N+j0 .. +15] and b[k*N+j0+16 .. +31] (already contiguous in mm4.c)
//
// Every variant uses the SAME pack, so the only difference is the load
// instruction. Values are gated against the control.
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// control: 4 single-vector loads per k, 4 fmopa.
__arm_new("za")
static void panel_ld1(const float *ap, const float *b, float *c,
                      int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32_t an0 = svld1_f32(pg, &ap[k * 32]);
        svfloat32_t an1 = svld1_f32(pg, &ap[k * 32 + 16]);
        svfloat32_t bm0 = svld1_f32(pg, &b[k * N + j0]);
        svfloat32_t bm1 = svld1_f32(pg, &b[k * N + j0 + 16]);
        svmopa_za32_f32_m(0, pg, pg, an0, bm0);
        svmopa_za32_f32_m(1, pg, pg, an0, bm1);
        svmopa_za32_f32_m(2, pg, pg, an1, bm0);
        svmopa_za32_f32_m(3, pg, pg, an1, bm1);
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

// 2 two-vector loads per k, same 4 fmopa.
__arm_new("za")
static void panel_ld2(const float *ap, const float *b, float *c,
                      int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svcount_t pn = svptrue_c32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32x2_t av = svld1_f32_x2(pn, &ap[k * 32]);
        svfloat32x2_t bv = svld1_f32_x2(pn, &b[k * N + j0]);
        svfloat32_t an0 = svget2_f32(av, 0), an1 = svget2_f32(av, 1);
        svfloat32_t bm0 = svget2_f32(bv, 0), bm1 = svget2_f32(bv, 1);
        svmopa_za32_f32_m(0, pg, pg, an0, bm0);
        svmopa_za32_f32_m(1, pg, pg, an0, bm1);
        svmopa_za32_f32_m(2, pg, pg, an1, bm0);
        svmopa_za32_f32_m(3, pg, pg, an1, bm1);
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

// 2 two-vector loads, k unrolled x2 -- in case folding the loads exposes a
// latency the batched form then hits.
__arm_new("za")
static void panel_ld2u(const float *ap, const float *b, float *c,
                       int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svcount_t pn = svptrue_c32();
    svzero_za();
    for (int k = 0; k < N; k += 2) {
        svfloat32x2_t av = svld1_f32_x2(pn, &ap[k * 32]);
        svfloat32x2_t bv = svld1_f32_x2(pn, &b[k * N + j0]);
        svfloat32x2_t cv = svld1_f32_x2(pn, &ap[(k + 1) * 32]);
        svfloat32x2_t dv = svld1_f32_x2(pn, &b[(k + 1) * N + j0]);
        svmopa_za32_f32_m(0, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 0));
        svmopa_za32_f32_m(1, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 1));
        svmopa_za32_f32_m(2, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 0));
        svmopa_za32_f32_m(3, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 1));
        svmopa_za32_f32_m(0, pg, pg, svget2_f32(cv, 0), svget2_f32(dv, 0));
        svmopa_za32_f32_m(1, pg, pg, svget2_f32(cv, 0), svget2_f32(dv, 1));
        svmopa_za32_f32_m(2, pg, pg, svget2_f32(cv, 1), svget2_f32(dv, 0));
        svmopa_za32_f32_m(3, pg, pg, svget2_f32(cv, 1), svget2_f32(dv, 1));
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

// the fmopa-only roofline at 4 chains, for the % column
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

typedef void (*panel_fn)(const float *, const float *, float *, int, int, int) __arm_streaming;

// A pack, interleaved so the two row-blocks for one k are adjacent.
static void pack_panel(const float *a, float *ap, int N, int i0) {
    for (int k = 0; k < N; k++)
        for (int i = 0; i < 16; i++) {
            ap[k * 32 + i]      = a[(i0 + i) * N + k];
            ap[k * 32 + 16 + i] = a[(i0 + 16 + i) * N + k];
        }
}

static double run(panel_fn panel, const float *a, const float *b, float *c,
                 float *ap, int N) {
    double t0 = now_ms();
    for (int i0 = 0; i0 < N; i0 += 32) {
        pack_panel(a, ap, N, i0);
        for (int j0 = 0; j0 < N; j0 += 32) panel(ap, b, c, N, i0, j0);
    }
    return now_ms() - t0;
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 1024, REPS = argc > 2 ? atoi(argv[2]) : 9;
    if (N % 32) { fprintf(stderr, "N must be a multiple of 32\n"); return 1; }
    double sink = 0;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4), *ref = aligned_alloc(64, (size_t)N*N*4);
    float *ap = aligned_alloc(64, (size_t)N*32*4);
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }

    // warmup -- load-bearing, see roofline.c (1.73x swing cold vs warm)
    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    long total = (long)N * N * N / 256;
    double roof = 0;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        sink += issue4(seed, total / 4);
        double dt = now_ms() - t0;
        double gf = 512.0 * total / (dt * 1e6);
        if (gf > roof) roof = gf;
    }

    const char *names[3] = {"4x ld1w (control)", "2x ld1w x2", "2x ld1w x2, k unrolled x2"};
    panel_fn fns[3] = {panel_ld1, panel_ld2, panel_ld2u};
    double ms[3] = {1e18, 1e18, 1e18};

    // interleaved so drift hits all three equally; values gated against control
    for (int r = 0; r < REPS; r++)
        for (int v = 0; v < 3; v++) {
            double t = run(fns[v], a, b, c, ap, N);
            if (t < ms[v]) ms[v] = t;
            if (v == 0 && r == 0) { memcpy(ref, c, (size_t)N*N*4); continue; }
            for (long q = 0; q < (long)N*N; q++)
                if (c[q] != ref[q]) {
                    fprintf(stderr, "%s MISMATCH at %ld: %.6f vs %.6f\n",
                            names[v], q, c[q], ref[q]);
                    return 2;
                }
        }

    double gflop = 2.0 * N * N * N / 1e6;
    printf("N=%d, best of %d interleaved, 1 thread, after 300 ms warmup\n", N, REPS);
    printf("  %-28s %9s %11s %11s %9s\n", "", "ms", "GFLOP/s", "% roofline", "vs ctrl");
    printf("  %-28s %9.4f %11.1f %10s\n", "fmopa only, 4 chains",
           512.0 * total / (roof * 1e6), roof, "100%");
    for (int v = 0; v < 3; v++)
        printf("  %-28s %9.4f %11.1f %10.0f%% %8.3fx\n", names[v], ms[v], gflop / ms[v],
               100.0 * (gflop / ms[v]) / roof, ms[0] / ms[v]);
    printf("  values identical across all three (c[0]=%.4f, sink=%.1f)\n", ref[0], sink);
    return 0;
}
