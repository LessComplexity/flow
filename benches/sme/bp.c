// S42, third probe. What is left after two refutations:
//   mm4p.c    reordering / software-pipelining the k loop   -> 1.006x  (nothing)
//   mv.c      folding 4 load instructions into 2 (ld1w x2)  -> 1.018x  (nothing)
//   roofline.c fmopa-only ceiling at 4 chains               -> 2008.9 GFLOP/s
//              the real GEMM                                -> 1071.6 (53%)
//
// Load ORDER does not matter and load INSTRUCTION COUNT does not matter, so the
// deficit is bytes and where they come from. Count them: B is read as
// b[k*N + j0], 128 contiguous bytes at stride N*4, and the whole 128 KB column
// block is re-walked for every one of the 32 i0 panels -> ~128 MB of B traffic
// per GEMM at N=1024, against a 4 MB array. Every one of those is an L2 hit at
// best; A, packed, is only 128 KB.
//
// So: pack B ONCE per GEMM into panel-major order, so each panel's B is 128 KB
// read straight through instead of strided. Same bytes, sequential. This is the
// hypothesis that survives, and it is also exactly what the NEON leg already
// does (rung 3, emit_tile_packed_j_outer).
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// B strided out of the original array: the control (mv.c's winner, ld1w x2).
__arm_new("za")
static void panel_bstrided(const float *ap, const float *b, float *c,
                           int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svcount_t pn = svptrue_c32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32x2_t av = svld1_f32_x2(pn, &ap[k * 32]);
        svfloat32x2_t bv = svld1_f32_x2(pn, &b[k * N + j0]);
        svmopa_za32_f32_m(0, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 0));
        svmopa_za32_f32_m(1, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 1));
        svmopa_za32_f32_m(2, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 0));
        svmopa_za32_f32_m(3, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 1));
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

// B pre-packed panel-major: bp is this panel's 128 KB, read straight through.
__arm_new("za")
static void panel_bpacked(const float *ap, const float *bp, float *c,
                          int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svcount_t pn = svptrue_c32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32x2_t av = svld1_f32_x2(pn, &ap[k * 32]);
        svfloat32x2_t bv = svld1_f32_x2(pn, &bp[k * 32]);
        svmopa_za32_f32_m(0, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 0));
        svmopa_za32_f32_m(1, pg, pg, svget2_f32(av, 0), svget2_f32(bv, 1));
        svmopa_za32_f32_m(2, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 0));
        svmopa_za32_f32_m(3, pg, pg, svget2_f32(av, 1), svget2_f32(bv, 1));
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

static void pack_a(const float *a, float *ap, int N, int i0) {
    for (int k = 0; k < N; k++)
        for (int i = 0; i < 16; i++) {
            ap[k * 32 + i]      = a[(i0 + i) * N + k];
            ap[k * 32 + 16 + i] = a[(i0 + 16 + i) * N + k];
        }
}

// panel-major: panel p (= j0/32) occupies bp[p*N*32 ..], k-major inside
static void pack_b(const float *b, float *bp, int N) {
    for (int j0 = 0; j0 < N; j0 += 32) {
        float *dst = bp + (size_t)(j0 / 32) * N * 32;
        for (int k = 0; k < N; k++)
            memcpy(dst + (size_t)k * 32, b + (size_t)k * N + j0, 32 * sizeof(float));
    }
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
    float *bp = aligned_alloc(64, (size_t)N*N*4);
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }

    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += issue4(seed, 200000);

    long total = (long)N * N * N / 256;
    double roof = 0;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        sink += issue4(seed, total / 4);
        double gf = 512.0 * total / ((now_ms() - t0) * 1e6);
        if (gf > roof) roof = gf;
    }

    double ms_str = 1e18, ms_pk = 1e18, ms_bpack = 1e18;
    for (int r = 0; r < REPS; r++) {
        // control: B strided
        double t0 = now_ms();
        for (int i0 = 0; i0 < N; i0 += 32) {
            pack_a(a, ap, N, i0);
            for (int j0 = 0; j0 < N; j0 += 32) panel_bstrided(ap, b, c, N, i0, j0);
        }
        double dt = now_ms() - t0;
        if (dt < ms_str) ms_str = dt;
        if (r == 0) memcpy(ref, c, (size_t)N*N*4);

        // B packed once per GEMM, then panel-major reads
        t0 = now_ms();
        pack_b(b, bp, N);
        double tpk = now_ms() - t0;
        for (int i0 = 0; i0 < N; i0 += 32) {
            pack_a(a, ap, N, i0);
            for (int j0 = 0; j0 < N; j0 += 32)
                panel_bpacked(ap, bp + (size_t)(j0 / 32) * N * 32, c, N, i0, j0);
        }
        dt = now_ms() - t0;
        if (dt < ms_bpack) ms_bpack = dt;
        if (tpk < ms_pk) ms_pk = tpk;

        for (long q = 0; q < (long)N*N; q++)
            if (c[q] != ref[q]) {
                fprintf(stderr, "B-packed MISMATCH at %ld: %.6f vs %.6f\n", q, c[q], ref[q]);
                return 2;
            }
    }

    double gflop = 2.0 * N * N * N / 1e6;
    printf("N=%d, best of %d interleaved, 1 thread, after 300 ms warmup\n", N, REPS);
    printf("  %-30s %9s %11s %11s %9s\n", "", "ms", "GFLOP/s", "% roofline", "vs ctrl");
    printf("  %-30s %9.4f %11.1f %10s\n", "fmopa only, 4 chains",
           512.0 * total / (roof * 1e6), roof, "100%");
    printf("  %-30s %9.4f %11.1f %10.0f%% %8.3fx\n", "B strided (control)", ms_str,
           gflop / ms_str, 100.0 * (gflop / ms_str) / roof, 1.0);
    printf("  %-30s %9.4f %11.1f %10.0f%% %8.3fx\n", "B packed panel-major", ms_bpack,
           gflop / ms_bpack, 100.0 * (gflop / ms_bpack) / roof, ms_str / ms_bpack);
    printf("  %-30s %9.4f %11.1f%% of the packed run\n", "  of which: the B pack itself",
           ms_pk, 100.0 * ms_pk / ms_bpack);
    printf("  values identical (c[0]=%.4f, sink=%.1f)\n", ref[0], sink);
    return 0;
}
