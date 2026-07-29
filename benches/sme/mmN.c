#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

// Hand-written SME GEMM ceiling probe: C[NxN] = A[NxN] * B[NxN], f32, 1 thread.
// Tiles C into 16x16 ZA blocks; A is packed per i-panel so each fmopa operand
// is contiguous. This is the SAME rank-1-update structure the NEON micro-kernel
// already uses (S26 register blocking), 16x16 per issue instead of 4x16.
__arm_new("za")
static void mm_panel(const float *ap, const float *b, float *c, int N, int i0, int j0)
    __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32_t zn = svld1_f32(pg, &ap[k * 16]);
        svfloat32_t zm = svld1_f32(pg, &b[k * N + j0]);
        svmopa_za32_f32_m(0, pg, pg, zn, zm);
    }
    for (int i = 0; i < 16; i++) {
        svfloat32_t row = svread_hor_za32_f32_m(svundef_f32(), pg, 0, i);
        svst1_f32(pg, &c[(i0 + i) * N + j0], row);
    }
}

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 1024;
    int REPS = argc > 2 ? atoi(argv[2]) : 7;
    float *a = aligned_alloc(64, (size_t)N * N * sizeof(float));
    float *b = aligned_alloc(64, (size_t)N * N * sizeof(float));
    float *c = aligned_alloc(64, (size_t)N * N * sizeof(float));
    float *ap = aligned_alloc(64, (size_t)N * 16 * sizeof(float));
    for (long t = 0; t < (long)N * N; t++) { a[t] = (float)((t * 7) % 13) * 0.01f + 1.0f;
                                             b[t] = (float)((t * 5) % 17) * 0.01f + 1.0f; }
    double best = 1e18;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        for (int i0 = 0; i0 < N; i0 += 16) {
            for (int k = 0; k < N; k++)              // pack this i-panel of A
                for (int i = 0; i < 16; i++) ap[k * 16 + i] = a[(i0 + i) * N + k];
            for (int j0 = 0; j0 < N; j0 += 16) mm_panel(ap, b, c, N, i0, j0);
        }
        double dt = now_ms() - t0;
        if (dt < best) best = dt;
    }
    double gflops = 2.0 * N * N * N / (best * 1e6);
    printf("N=%-5d SME 1t: %8.4f ms   %7.1f GFLOP/s   (min of %d)   c[0]=%.4f\n",
           N, best, gflops, REPS, c[0]);
    return 0;
}
