// S42 closing probe: does k-loop unrolling help the PACKED kernel?
//
// WHY THIS EXISTS. mm4p.c refuted pipelining at 1.006x, but an adversarial audit
// of that probe held two objections that this file fixes:
//
//  1. mm4p.c's panel_rotate DID NOT SURVIVE COMPILATION. Source order was
//     "k+1 loads, then k fmopa"; LLVM emitted "k fmopa, then k+1 loads" at both
//     -O2 and -O3, putting every load immediately before its consumer across the
//     back edge. That arm measured base against base and is dropped here. Only
//     the unroll-x2 arm provably survives (8 loads above 8 fmopa), so only it is
//     tested. A transformation you cannot see in the assembly is not a variant.
//
//  2. mm4p.c read b UNPACKED, at stride N*4, re-streamed once per i0 block --
//     ~128 MB of b traffic per GEMM at N=1024 against a ~1.05 ms fmopa floor.
//     That loop is bandwidth-limited BY CONSTRUCTION, and a null from unrolling a
//     bandwidth-limited loop says nothing about the kernel the emitter actually
//     emits, which packs b (verified from the emitted call arguments:
//     bn=t, bj=t*k is the packed arm). So: same question, packed kernel.
//
// METHOD, per the project's standing rules (benches/sme/sme_ab.sh:8-17):
//   - 300 ms fmopa warmup before the first timer. The same binary measured the
//     roofline at 1.852 ms cold and 1.069 ms warm -- 1.73x on identical code.
//   - ALTERNATING runs, >=51 by default, medians reported alongside minima, and
//     an explicit distribution-overlap check. A sub-10% difference on an
//     unpinned Mac at these sizes is noise and is labelled as such.
//   - c is ZEROED before each timed region (outside it), so a variant that skips
//     panels leaves zeros and fails the gate instead of inheriting the previous
//     variant's correct output.
//   - An INDEPENDENT scalar reference checks a sample of cells, so the gate
//     proves the kernels compute A*B rather than merely agreeing with each other.
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

// packed b, 4 single-vector loads per k -- the form the emitter emits
__arm_new("za")
static void panel_base(const float *ap, const float *bp, float *c,
                       int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k++) {
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

// packed b, k unrolled x2, all 8 loads above all 8 fmopa -- the arm that survives
__arm_new("za")
static void panel_unroll2(const float *ap, const float *bp, float *c,
                          int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k += 2) {
        svfloat32_t an0 = svld1_f32(pg, &ap[k * 32]);
        svfloat32_t an1 = svld1_f32(pg, &ap[k * 32 + 16]);
        svfloat32_t bm0 = svld1_f32(pg, &bp[k * 32]);
        svfloat32_t bm1 = svld1_f32(pg, &bp[k * 32 + 16]);
        svfloat32_t cn0 = svld1_f32(pg, &ap[(k + 1) * 32]);
        svfloat32_t cn1 = svld1_f32(pg, &ap[(k + 1) * 32 + 16]);
        svfloat32_t dm0 = svld1_f32(pg, &bp[(k + 1) * 32]);
        svfloat32_t dm1 = svld1_f32(pg, &bp[(k + 1) * 32 + 16]);
        svmopa_za32_f32_m(0, pg, pg, an0, bm0);
        svmopa_za32_f32_m(1, pg, pg, an0, bm1);
        svmopa_za32_f32_m(2, pg, pg, an1, bm0);
        svmopa_za32_f32_m(3, pg, pg, an1, bm1);
        svmopa_za32_f32_m(0, pg, pg, cn0, dm0);
        svmopa_za32_f32_m(1, pg, pg, cn0, dm1);
        svmopa_za32_f32_m(2, pg, pg, cn1, dm0);
        svmopa_za32_f32_m(3, pg, pg, cn1, dm1);
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
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

typedef void (*panel_fn)(const float *, const float *, float *, int, int, int) __arm_streaming;

static void pack_a(const float *a, float *ap, int N, int i0) {
    for (int k = 0; k < N; k++)
        for (int i = 0; i < 16; i++) {
            ap[k * 32 + i]      = a[(i0 + i) * N + k];
            ap[k * 32 + 16 + i] = a[(i0 + 16 + i) * N + k];
        }
}

static void pack_b(const float *b, float *bp, int N) {
    for (int j0 = 0; j0 < N; j0 += 32) {
        float *dst = bp + (size_t)(j0 / 32) * N * 32;
        for (int k = 0; k < N; k++)
            memcpy(dst + (size_t)k * 32, b + (size_t)k * N + j0, 32 * sizeof(float));
    }
}

static double run(panel_fn panel, const float *a, const float *b, float *c,
                  float *ap, float *bp, int N) {
    double t0 = now_ms();
    pack_b(b, bp, N);
    for (int i0 = 0; i0 < N; i0 += 32) {
        pack_a(a, ap, N, i0);
        for (int j0 = 0; j0 < N; j0 += 32)
            panel(ap, bp + (size_t)(j0 / 32) * N * 32, c, N, i0, j0);
    }
    return now_ms() - t0;
}

// independent scalar reference for a sample of cells (fused, to match fmopa)
static int gate(const float *a, const float *b, const float *c, int N, const char *who) {
    for (int s = 0; s < 97; s++) {
        long i = (long)(s * 7919) % N, j = (long)(s * 6151 + 13) % N;
        float acc = 0.0f;
        for (int k = 0; k < N; k++) acc = fmaf(a[i * N + k], b[k * N + j], acc);
        float got = c[i * N + j];
        float tol = 1e-3f * (acc < 0 ? -acc : acc);
        float d = got - acc; if (d < 0) d = -d;
        if (d > tol) {
            fprintf(stderr, "%s GATE FAIL c[%ld][%ld] = %.4f, scalar ref %.4f\n", who, i, j, got, acc);
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
    int N = argc > 1 ? atoi(argv[1]) : 1024, RUNS = argc > 2 ? atoi(argv[2]) : 51;
    if (N % 32) { fprintf(stderr, "N must be a multiple of 32\n"); return 1; }
    double sink = 0;

    float *seed = aligned_alloc(64, 32 * 4);
    for (int i = 0; i < 32; i++) seed[i] = 1.0f + i * 0.01f;
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4);
    float *ap = aligned_alloc(64, (size_t)N*32*4), *bp = aligned_alloc(64, (size_t)N*N*4);
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }

    const char *names[2] = {"packed, base", "packed, k unrolled x2"};
    panel_fn fns[2] = {panel_base, panel_unroll2};

    // first-touch bp and c outside every timed region, for both variants equally
    memset(bp, 0, (size_t)N*N*4);
    memset(c, 0, (size_t)N*N*4);

    // warmup -- load-bearing, 1.73x cold-vs-warm on identical code
    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    // value gate, before any timing is read
    for (int v = 0; v < 2; v++) {
        memset(c, 0, (size_t)N*N*4);
        run(fns[v], a, b, c, ap, bp, N);
        if (!gate(a, b, c, N, names[v])) return 2;
    }
    printf("both variants gated against an independent scalar fmaf reference (97 cells)\n");

    long total = (long)N * N * N / 256;
    double *s0 = malloc(RUNS * sizeof(double)), *s1 = malloc(RUNS * sizeof(double));
    double roof = 0;
    for (int r = 0; r < RUNS; r++) {
        // alternating, and c zeroed before each so a skipped panel cannot hide
        memset(c, 0, (size_t)N*N*4);
        s0[r] = run(fns[0], a, b, c, ap, bp, N);
        memset(c, 0, (size_t)N*N*4);
        s1[r] = run(fns[1], a, b, c, ap, bp, N);
        double t0 = now_ms();
        sink += issue4(seed, total / 4);
        double gf = 512.0 * total / ((now_ms() - t0) * 1e6);
        if (gf > roof) roof = gf;
    }
    qsort(s0, RUNS, sizeof(double), cmp_d);
    qsort(s1, RUNS, sizeof(double), cmp_d);
    double m0 = s0[RUNS/2], m1 = s1[RUNS/2];
    double gflop = 2.0 * N * N * N / 1e6;

    printf("N=%d, %d ALTERNATING runs, 1 thread, 300 ms warmup, roofline interleaved\n", N, RUNS);
    printf("  %-24s %9s %9s %9s %11s %11s\n", "", "min", "median", "max", "GFLOP/s", "% roofline");
    for (int v = 0; v < 2; v++) {
        double *s = v ? s1 : s0, m = v ? m1 : m0;
        printf("  %-24s %9.4f %9.4f %9.4f %11.1f %10.0f%%\n", names[v],
               s[0], m, s[RUNS-1], gflop / m, 100.0 * (gflop / m) / roof);
    }
    printf("  fmopa roofline, 4 chains %9s %9s %9s %11.1f %10s\n", "-", "-", "-", roof, "100%");
    double ratio = m0 / m1;
    double pct = 100.0 * (ratio - 1.0);
    int overlap = s0[0] <= s1[RUNS-1] && s1[0] <= s0[RUNS-1];
    printf("  unroll x2 is worth %.3fx (%+.1f%%) on medians\n", ratio, pct);
    printf("  distributions %s\n", overlap ? "OVERLAP" : "are DISJOINT");
    printf("  VERDICT: %s\n",
           (pct > -10.0 && pct < 10.0)
             ? "sub-10% on an unpinned Mac -- NOISE, not a result (rule 6/11)"
             : (ratio > 1.0 ? "a real win, above the noise floor" : "a real LOSS, above the noise floor"));
    printf("  (c[0]=%.4f, sink=%.1f)\n", c[0], sink);
    return 0;
}
