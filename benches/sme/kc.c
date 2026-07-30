// S42, fifth probe: size the 4096 knee. This is the term that actually matters.
//
// WHERE THIS COMES FROM. pipe2.c, packed b, 1 thread, vs a ~2007 GFLOP/s fmopa
// ceiling:  1024 -> 1089 (54%)   2048 -> 1074 (53%)   4096 -> 756 (38%).
// B packing is already in (both here and in the emitter), unrolling the k loop is
// null at all three sizes, and 4096 still loses a third of its throughput. So the
// remaining term is the working set: at k=4096 one panel touches
//   ap  = ti*t * k * 4 = 32 * 4096 * 4 = 512 KB
//   bp  = tj*t * k * 4 = 32 * 4096 * 4 = 512 KB
// = 1 MB streamed per panel call, which no per-core cache holds. That is KC
// blocking, and it is on the S42 P1 list. This probe sizes the prize.
//
// THE COST KC HAS TO PAY, which next-session.md §3 flagged: the panel kernel
// STORES its ZA tiles rather than accumulating into c (hence the seed==0
// precondition). Blocking k means partial sums must live in c across k-blocks, so
// every block after the first is a read-modify-write of the output block --
// `read.horiz` out, add, store back -- which is more expensive than spilling
// vector registers. Whether blocking wins is therefore a real crossover, not a
// given. Both kernels are here so the crossover is measured, not assumed.
//
// CAVEAT, and it is Sapir's, recorded because it bounds what this file can say:
// a standalone C probe does NOT settle what an optimization is worth inside the
// real pipeline. It has none of the other optimizations to compose with, and it
// says nothing about threaded scaling. Read every number here as a floor on a
// standalone kernel, and settle the verdict in the emitter, threaded, at scale.
#include <arm_sme.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// first k-block: store ZA over c (what the emitter's kernel does today)
__arm_new("za")
static void panel_store(const float *ap, const float *bp, float *c,
                        int N, int KC, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < KC; k++) {
        svfloat32_t an0 = svld1_f32(pg, &ap[k * 32]);
        svfloat32_t an1 = svld1_f32(pg, &ap[k * 32 + 16]);
        svfloat32_t bm0 = svld1_f32(pg, &bp[k * 32]);
        svfloat32_t bm1 = svld1_f32(pg, &bp[k * 32 + 16]);
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

// later k-blocks: read-modify-write c. This is the price of blocking.
__arm_new("za")
static void panel_acc(const float *ap, const float *bp, float *c,
                      int N, int KC, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < KC; k++) {
        svfloat32_t an0 = svld1_f32(pg, &ap[k * 32]);
        svfloat32_t an1 = svld1_f32(pg, &ap[k * 32 + 16]);
        svfloat32_t bm0 = svld1_f32(pg, &bp[k * 32]);
        svfloat32_t bm1 = svld1_f32(pg, &bp[k * 32 + 16]);
        svmopa_za32_f32_m(0, pg, pg, an0, bm0);
        svmopa_za32_f32_m(1, pg, pg, an0, bm1);
        svmopa_za32_f32_m(2, pg, pg, an1, bm0);
        svmopa_za32_f32_m(3, pg, pg, an1, bm1);
    }
    for (int i = 0; i < 16; i++) {
        float *p0 = &c[(i0+i)*N + j0],      *p1 = &c[(i0+i)*N + j0 + 16];
        float *p2 = &c[(i0+16+i)*N + j0],   *p3 = &c[(i0+16+i)*N + j0 + 16];
        svst1_f32(pg, p0, svadd_f32_m(pg, svld1_f32(pg, p0), svread_hor_za32_f32_m(svundef_f32(), pg, 0, i)));
        svst1_f32(pg, p1, svadd_f32_m(pg, svld1_f32(pg, p1), svread_hor_za32_f32_m(svundef_f32(), pg, 1, i)));
        svst1_f32(pg, p2, svadd_f32_m(pg, svld1_f32(pg, p2), svread_hor_za32_f32_m(svundef_f32(), pg, 2, i)));
        svst1_f32(pg, p3, svadd_f32_m(pg, svld1_f32(pg, p3), svread_hor_za32_f32_m(svundef_f32(), pg, 3, i)));
    }
}

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

// A panel for k in [k0, k0+KC): ap[kk*32 + i], two row-blocks interleaved
static void pack_a(const float *a, float *ap, int N, int i0, int k0, int KC) {
    for (int kk = 0; kk < KC; kk++)
        for (int i = 0; i < 16; i++) {
            ap[kk * 32 + i]      = a[(i0 + i) * N + k0 + kk];
            ap[kk * 32 + 16 + i] = a[(i0 + 16 + i) * N + k0 + kk];
        }
}

// B panel-major for k in [k0, k0+KC): panel p at bp[p*KC*32], k-major inside
static void pack_b(const float *b, float *bp, int N, int k0, int KC) {
    for (int j0 = 0; j0 < N; j0 += 32) {
        float *dst = bp + (size_t)(j0 / 32) * KC * 32;
        for (int kk = 0; kk < KC; kk++)
            memcpy(dst + (size_t)kk * 32, b + (size_t)(k0 + kk) * N + j0, 32 * sizeof(float));
    }
}

static double run(const float *a, const float *b, float *c, float *ap, float *bp,
                  int N, int KC) {
    double t0 = now_ms();
    for (int k0 = 0; k0 < N; k0 += KC) {
        pack_b(b, bp, N, k0, KC);
        for (int i0 = 0; i0 < N; i0 += 32) {
            pack_a(a, ap, N, i0, k0, KC);
            for (int j0 = 0; j0 < N; j0 += 32) {
                const float *bpanel = bp + (size_t)(j0 / 32) * KC * 32;
                if (k0 == 0) panel_store(ap, bpanel, c, N, KC, i0, j0);
                else         panel_acc  (ap, bpanel, c, N, KC, i0, j0);
            }
        }
    }
    return now_ms() - t0;
}

static int gate(const float *a, const float *b, const float *c, int N, int KC) {
    for (int s = 0; s < 97; s++) {
        long i = (long)(s * 7919) % N, j = (long)(s * 6151 + 13) % N;
        float acc = 0.0f;
        for (int k = 0; k < N; k++) acc = fmaf(a[i * N + k], b[k * N + j], acc);
        float got = c[i * N + j], d = got - acc;
        if (d < 0) d = -d;
        if (d > 1e-3f * fabsf(acc)) {
            fprintf(stderr, "KC=%d GATE FAIL c[%ld][%ld] = %.4f, scalar ref %.4f\n", KC, i, j, got, acc);
            return 0;
        }
    }
    return 1;
}

static int cmp_d(const void *x, const void *y) {
    double a = *(const double *)x, b = *(const double *)y;
    return a < b ? -1 : a > b ? 1 : 0;
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 4096, RUNS = argc > 2 ? atoi(argv[2]) : 11;
    if (N % 32) { fprintf(stderr, "N must be a multiple of 32\n"); return 1; }
    double sink = 0;

    int KCS[5], nkc = 0;
    for (int kc = 256; kc <= N; kc *= 2) if (N % kc == 0 && nkc < 5) KCS[nkc++] = kc;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4);
    float *ap = aligned_alloc(64, (size_t)N*32*4), *bp = aligned_alloc(64, (size_t)N*N*4);
    if (!a || !b || !c || !ap || !bp) { fprintf(stderr, "alloc failed\n"); return 1; }
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }
    memset(bp, 0, (size_t)N*N*4);
    memset(c, 0, (size_t)N*N*4);

    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    // gate every KC before any timing is read
    for (int v = 0; v < nkc; v++) {
        memset(c, 0, (size_t)N*N*4);
        run(a, b, c, ap, bp, N, KCS[v]);
        if (!gate(a, b, c, N, KCS[v])) return 2;
    }
    printf("all %d KC values gated against an independent scalar fmaf reference (97 cells)\n", nkc);

    long total = (long)N * N * N / 256;
    double *s[5];
    for (int v = 0; v < nkc; v++) s[v] = malloc(RUNS * sizeof(double));
    double roof = 0;
    for (int r = 0; r < RUNS; r++) {
        for (int v = 0; v < nkc; v++) {          // alternating, drift hits all KCs equally
            memset(c, 0, (size_t)N*N*4);
            s[v][r] = run(a, b, c, ap, bp, N, KCS[v]);
        }
        double t0 = now_ms();
        sink += issue4(seed, total / 4);
        double gf = 512.0 * total / ((now_ms() - t0) * 1e6);
        if (gf > roof) roof = gf;
    }

    double gflop = 2.0 * N * N * N / 1e6;
    printf("N=%d, %d ALTERNATING runs, 1 thread, 300 ms warmup, roofline interleaved\n", N, RUNS);
    printf("  panel working set = 2 * 32 * KC * 4 B\n");
    printf("  %-10s %9s %9s %9s %11s %11s %11s\n",
           "KC", "min", "median", "max", "GFLOP/s", "% roofline", "working set");
    double best = 0; int bestkc = 0;
    for (int v = 0; v < nkc; v++) {
        qsort(s[v], RUNS, sizeof(double), cmp_d);
        double m = s[v][RUNS/2], gf = gflop / m;
        if (gf > best) { best = gf; bestkc = KCS[v]; }
        char ws[24]; snprintf(ws, sizeof ws, "%ld KB", (long)(2L*32*KCS[v]*4/1024));
        printf("  %-10d %9.3f %9.3f %9.3f %11.1f %10.0f%% %11s%s\n",
               KCS[v], s[v][0], m, s[v][RUNS-1], gf, 100.0 * gf / roof, ws,
               KCS[v] == N ? "   <- no blocking" : "");
    }
    printf("  fmopa roofline, 4 chains %31.1f %10s\n", roof, "100%");
    double unblocked = gflop / s[nkc-1][RUNS/2];
    printf("  best KC = %d at %.1f GFLOP/s -> blocking is worth %.3fx over unblocked\n",
           bestkc, best, best / unblocked);
    int overlap = s[nkc-1][0] <= s[0][RUNS-1] && s[0][0] <= s[nkc-1][RUNS-1];
    printf("  best vs unblocked distributions %s\n", overlap ? "OVERLAP" : "are DISJOINT");
    printf("  (c[0]=%.4f, sink=%.1f)\n", c[0], sink);
    return 0;
}
