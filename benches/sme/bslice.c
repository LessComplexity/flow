// S42: why does KC blocking win 1.448x standalone and LOSE 0.87x in the emitter?
//
// Emission was verified correct line by line (ap alloca 64 KB = panel_rows*kc,
// pack bound 512, a's k coord = k0+pk, k0 loop outside i0, A pack hoisted out of
// j, b offset k0*pack_w, bj = whole-panel 65536, K arg 512, first block stores).
// Work counts match too: 268M fmopa = N^3/256, A-pack elements = rows*k. So the
// nest does the right work in the right order and the loss is memory behaviour.
//
// THE ONE STRUCTURAL DIFFERENCE between kc.c and the emitter is the B LAYOUT:
//
//   kc.c      re-packs B per k block into [panel32][kc][32]. One panel call
//             reads ONE contiguous 64 KB run; the 2x2 block's two column halves
//             are adjacent (offset 16 inside the same 32-float row).
//
//   emitter   slices the NEON packing rung's whole-k buffer, [panel16][k][16].
//             A panel call's two column blocks are two SEPARATE 32 KB runs
//             bj = 16*k = 65536 floats = 256 KB apart, and each is a kc-deep
//             slice out of the middle of a 256 KB panel.
//
// This probe changes ONLY that. Same kernel body, same kc, same k0-outer order,
// same A pack, same accumulate read-out. If `emitter layout` lands near the
// emitter's 219 ms while `kc.c layout` stays near 125 ms, the B slicing is the
// cause and a kc-deep B pack is the fix.
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

// `bn` = distance between consecutive k rows, `bj` = distance between the 2x2
// block's two column halves. Both in elements. That is exactly the emitted
// kernel's parameterisation (`i64 %bn, i64 %bj`), so one body serves both
// layouts and the ONLY thing under test is what the caller passes.
// F is a macro-time constant (1 or 0), so the branch folds away per variant.
#define WRITE_TILE(F, Q, DST)                                                    \
    do {                                                                         \
        float *dst_ = (DST);                                                     \
        svfloat32_t za_ = svread_hor_za32_f32_m(svundef_f32(), pg, Q, i);        \
        svst1_f32(pg, dst_,                                                      \
                  F ? za_ : svadd_f32_m(pg, svld1_f32(pg, dst_), za_));          \
    } while (0)

#define PANEL(NAME, FIRST)                                                       \
    __arm_new("za")                                                              \
    static void NAME(const float *ap, const float *b, float *c,                  \
                     int N, int KC, long bn, long bj, int i0, int j0)             \
        __arm_streaming {                                                        \
        svbool_t pg = svptrue_b32();                                             \
        svzero_za();                                                             \
        for (int k = 0; k < KC; k++) {                                           \
            svfloat32_t an0 = svld1_f32(pg, &ap[k * 32]);                        \
            svfloat32_t an1 = svld1_f32(pg, &ap[k * 32 + 16]);                   \
            svfloat32_t bm0 = svld1_f32(pg, &b[(long)k * bn]);                   \
            svfloat32_t bm1 = svld1_f32(pg, &b[(long)k * bn + bj]);              \
            svmopa_za32_f32_m(0, pg, pg, an0, bm0);                              \
            svmopa_za32_f32_m(1, pg, pg, an0, bm1);                              \
            svmopa_za32_f32_m(2, pg, pg, an1, bm0);                              \
            svmopa_za32_f32_m(3, pg, pg, an1, bm1);                              \
        }                                                                        \
        for (int i = 0; i < 16; i++) {                                           \
            /* the ZA tile index must be a literal, so this is unrolled */       \
            WRITE_TILE(FIRST, 0, &c[(long)(i0 + i) * N + j0]);                   \
            WRITE_TILE(FIRST, 1, &c[(long)(i0 + i) * N + j0 + 16]);              \
            WRITE_TILE(FIRST, 2, &c[(long)(i0 + 16 + i) * N + j0]);              \
            WRITE_TILE(FIRST, 3, &c[(long)(i0 + 16 + i) * N + j0 + 16]);         \
        }                                                                        \
    }

PANEL(panel_store, 1)
PANEL(panel_acc, 0)

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

static void pack_a(const float *a, float *ap, int N, int i0, int k0, int KC) {
    for (int kk = 0; kk < KC; kk++)
        for (int i = 0; i < 16; i++) {
            ap[kk * 32 + i]      = a[(long)(i0 + i) * N + k0 + kk];
            ap[kk * 32 + 16 + i] = a[(long)(i0 + 16 + i) * N + k0 + kk];
        }
}

// kc.c's layout: [panel32][KC][32], re-packed for every k block. Both column
// halves of a 2x2 block are adjacent, so bn = 32 and bj = 16.
static void pack_b_block(const float *b, float *bp, int N, int k0, int KC) {
    for (int j0 = 0; j0 < N; j0 += 32) {
        float *dst = bp + (size_t)(j0 / 32) * KC * 32;
        for (int kk = 0; kk < KC; kk++)
            memcpy(dst + (size_t)kk * 32, b + (size_t)(k0 + kk) * N + j0, 32 * sizeof(float));
    }
}

// The emitter's layout: the NEON packing rung's [panel16][k][16], whole k axis,
// packed ONCE. bn = 16 and bj = 16*N — two runs, 256 KB apart at N=4096.
static void pack_b_wholek(const float *b, float *bp, int N) {
    for (int p = 0; p < N / 16; p++) {
        float *dst = bp + (size_t)p * N * 16;
        for (int k = 0; k < N; k++)
            memcpy(dst + (size_t)k * 16, b + (size_t)k * N + p * 16, 16 * sizeof(float));
    }
}

static int gate(const float *a, const float *b, const float *c, int N, const char *who) {
    for (int s = 0; s < 61; s++) {
        long i = (long)(s * 7919) % N, j = (long)(s * 6151 + 13) % N;
        float acc = 0.0f;
        for (int k = 0; k < N; k++) acc = fmaf(a[i * (long)N + k], b[(long)k * N + j], acc);
        float got = c[i * (long)N + j], d = got - acc;
        if (d < 0) d = -d;
        if (d > 1e-3f * fabsf(acc)) {
            fprintf(stderr, "%s GATE FAIL c[%ld][%ld]=%.4f vs ref %.4f\n", who, i, j, got, acc);
            return 0;
        }
    }
    return 1;
}

static int cmp_d(const void *x, const void *y) {
    double a = *(const double *)x, b = *(const double *)y;
    return a < b ? -1 : a > b ? 1 : 0;
}

// variant 0 = kc.c layout (re-pack per block); variant 1 = emitter layout
static double run(int variant, const float *a, const float *b, float *c,
                  float *ap, float *bp, int N, int KC) {
    double t0 = now_ms();
    if (variant == 1) pack_b_wholek(b, bp, N);   // once per GEMM, as the emitter
    for (int k0 = 0; k0 < N; k0 += KC) {
        if (variant == 0) pack_b_block(b, bp, N, k0, KC);  // once per k block
        for (int i0 = 0; i0 < N; i0 += 32) {
            pack_a(a, ap, N, i0, k0, KC);
            for (int j0 = 0; j0 < N; j0 += 32) {
                const float *bpanel;
                long bn, bj;
                if (variant == 0) {
                    bpanel = bp + (size_t)(j0 / 32) * KC * 32;
                    bn = 32; bj = 16;
                } else {
                    bpanel = bp + (size_t)j0 * N + (size_t)k0 * 16;
                    bn = 16; bj = (long)16 * N;
                }
                if (k0 == 0) panel_store(ap, bpanel, c, N, KC, bn, bj, i0, j0);
                else         panel_acc  (ap, bpanel, c, N, KC, bn, bj, i0, j0);
            }
        }
    }
    return now_ms() - t0;
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 4096;
    int KC = argc > 2 ? atoi(argv[2]) : 512;
    int RUNS = argc > 3 ? atoi(argv[3]) : 7;
    if (N % 32 || N % KC) { fprintf(stderr, "need N%%32==0 and N%%KC==0\n"); return 1; }
    double sink = 0;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4);
    float *ap = aligned_alloc(64, (size_t)KC*32*4), *bp = aligned_alloc(64, (size_t)N*N*4);
    if (!a || !b || !c || !ap || !bp) { fprintf(stderr, "alloc failed\n"); return 1; }
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }
    memset(bp, 0, (size_t)N*N*4);
    memset(c, 0, (size_t)N*N*4);

    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    const char *names[2] = {"kc.c layout (repack/blk)", "EMITTER layout (slice)"};
    for (int v = 0; v < 2; v++) {
        memset(c, 0, (size_t)N*N*4);
        run(v, a, b, c, ap, bp, N, KC);
        if (!gate(a, b, c, N, names[v])) return 2;
    }
    printf("both layouts gated against an independent scalar fmaf reference\n");

    long total = (long)N * N * N / 256;
    double *s[2] = {malloc(RUNS * sizeof(double)), malloc(RUNS * sizeof(double))};
    double roof = 0;
    for (int r = 0; r < RUNS; r++) {
        for (int v = 0; v < 2; v++) {
            memset(c, 0, (size_t)N*N*4);
            s[v][r] = run(v, a, b, c, ap, bp, N, KC);
        }
        double t0 = now_ms();
        sink += issue4(seed, total / 4);
        double gf = 512.0 * total / ((now_ms() - t0) * 1e6);
        if (gf > roof) roof = gf;
    }

    double gflop = 2.0 * N * N * N / 1e6;
    printf("N=%d KC=%d, %d ALTERNATING runs, 1 thread, 300 ms warmup\n", N, KC, RUNS);
    printf("  %-26s %9s %9s %11s %11s\n", "", "min", "median", "GFLOP/s", "% roofline");
    for (int v = 0; v < 2; v++) {
        qsort(s[v], RUNS, sizeof(double), cmp_d);
        double m = s[v][RUNS/2];
        printf("  %-26s %9.3f %9.3f %11.1f %10.0f%%\n", names[v], s[v][0], m,
               gflop / m, 100.0 * (gflop / m) / roof);
    }
    printf("  %-26s %9s %9s %11.1f %10s\n", "fmopa roofline", "-", "-", roof, "100%");
    printf("  slicing costs %.3fx  (emitter median / kc.c median)\n",
           s[1][RUNS/2] / s[0][RUNS/2]);
    printf("  for reference: the real emitter measured 219.53 ms at this N and KC\n");
    printf("  (c[0]=%.4f, sink=%.1f)\n", c[0], sink);
    return 0;
}
