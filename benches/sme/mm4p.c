// S42 step-0 probe: does software-pipelining the k loop help, given the M4 is
// out-of-order? mm4.c's emitted k loop is 4 loads then 4 fmopa, strictly
// batched per iteration -- LLVM does NOT pipeline it (verified in the Mapal
// emission too). But a batched *static* order is not a stall on an OoO core.
//
// Three kernels, same values, same harness:
//   base    -- mm4.c's loop verbatim (the control)
//   unroll2 -- k unrolled x2, both iterations' 8 loads hoisted above 8 fmopa
//   rotate  -- classic rotation: iteration k+1's loads issued before k's fmopa
//
// ponytail: probe first, emitter second. If none of these beats base, the 38%
// gap is elsewhere (fmopa issue rate / operand bandwidth) and the emitter
// change would be wasted work.
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

__arm_new("za")
static void panel_base(const float *ap0, const float *ap1, const float *b,
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

__arm_new("za")
static void panel_unroll2(const float *ap0, const float *ap1, const float *b,
                          float *c, int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k += 2) {
        svfloat32_t an0 = svld1_f32(pg, &ap0[k * 16]);
        svfloat32_t an1 = svld1_f32(pg, &ap1[k * 16]);
        svfloat32_t bm0 = svld1_f32(pg, &b[k * N + j0]);
        svfloat32_t bm1 = svld1_f32(pg, &b[k * N + j0 + 16]);
        svfloat32_t cn0 = svld1_f32(pg, &ap0[(k + 1) * 16]);
        svfloat32_t cn1 = svld1_f32(pg, &ap1[(k + 1) * 16]);
        svfloat32_t dm0 = svld1_f32(pg, &b[(k + 1) * N + j0]);
        svfloat32_t dm1 = svld1_f32(pg, &b[(k + 1) * N + j0 + 16]);
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
static void panel_rotate(const float *ap0, const float *ap1, const float *b,
                         float *c, int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    svfloat32_t zn0 = svld1_f32(pg, &ap0[0]);
    svfloat32_t zn1 = svld1_f32(pg, &ap1[0]);
    svfloat32_t zm0 = svld1_f32(pg, &b[j0]);
    svfloat32_t zm1 = svld1_f32(pg, &b[j0 + 16]);
    for (int k = 0; k < N - 1; k++) {
        svfloat32_t nn0 = svld1_f32(pg, &ap0[(k + 1) * 16]);
        svfloat32_t nn1 = svld1_f32(pg, &ap1[(k + 1) * 16]);
        svfloat32_t nm0 = svld1_f32(pg, &b[(k + 1) * N + j0]);
        svfloat32_t nm1 = svld1_f32(pg, &b[(k + 1) * N + j0 + 16]);
        svmopa_za32_f32_m(0, pg, pg, zn0, zm0);
        svmopa_za32_f32_m(1, pg, pg, zn0, zm1);
        svmopa_za32_f32_m(2, pg, pg, zn1, zm0);
        svmopa_za32_f32_m(3, pg, pg, zn1, zm1);
        zn0 = nn0; zn1 = nn1; zm0 = nm0; zm1 = nm1;
    }
    svmopa_za32_f32_m(0, pg, pg, zn0, zm0);
    svmopa_za32_f32_m(1, pg, pg, zn0, zm1);
    svmopa_za32_f32_m(2, pg, pg, zn1, zm0);
    svmopa_za32_f32_m(3, pg, pg, zn1, zm1);
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],         svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16],    svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}

// __arm_streaming is a TYPE attribute, so it must appear on the pointer type too
// or the indirect call will not get its smstart/smstop pair.
typedef void (*panel_fn)(const float *, const float *, const float *, float *, int, int, int)
    __arm_streaming;

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// One full GEMM with the given panel kernel; returns best-of-REPS ms.
static double run(panel_fn panel, const float *a, const float *b, float *c,
                  float *ap0, float *ap1, int N, int REPS) {
    double best = 1e18;
    for (int r = 0; r < REPS; r++) {
        double t0 = now_ms();
        for (int i0 = 0; i0 < N; i0 += 32) {
            for (int k = 0; k < N; k++)
                for (int i = 0; i < 16; i++) {
                    ap0[k * 16 + i] = a[(i0 + i) * N + k];
                    ap1[k * 16 + i] = a[(i0 + 16 + i) * N + k];
                }
            for (int j0 = 0; j0 < N; j0 += 32) panel(ap0, ap1, b, c, N, i0, j0);
        }
        double dt = now_ms() - t0;
        if (dt < best) best = dt;
    }
    return best;
}

int main(int argc, char **argv) {
    int N = argc > 1 ? atoi(argv[1]) : 1024, REPS = argc > 2 ? atoi(argv[2]) : 7;
    if (N % 32 != 0) { fprintf(stderr, "N must be a multiple of 32\n"); return 1; }
    float *a = aligned_alloc(64, (size_t)N*N*4), *b = aligned_alloc(64, (size_t)N*N*4);
    float *c = aligned_alloc(64, (size_t)N*N*4), *ref = aligned_alloc(64, (size_t)N*N*4);
    float *ap0 = aligned_alloc(64, (size_t)N*16*4), *ap1 = aligned_alloc(64, (size_t)N*16*4);
    for (long t = 0; t < (long)N*N; t++) {
        a[t] = (float)((t*7)%13)*0.01f + 1.0f;
        b[t] = (float)((t*5)%17)*0.01f + 1.0f;
    }

    const char *names[3] = {"base (mm4.c)", "unroll2", "rotate"};
    panel_fn fns[3] = {panel_base, panel_unroll2, panel_rotate};
    double ms[3];

    // interleave the three so drift/thermals hit all of them equally
    for (int r = 0; r < REPS; r++)
        for (int v = 0; v < 3; v++) {
            double t = run(fns[v], a, b, c, ap0, ap1, N, 1);
            if (r == 0) { ms[v] = t; if (v == 0) memcpy(ref, c, (size_t)N*N*4); }
            else if (t < ms[v]) ms[v] = t;
            if (v == 0 && r == 0) continue;
            // value gate: every variant must match base exactly
            for (long t2 = 0; t2 < (long)N*N; t2++)
                if (c[t2] != ref[t2]) {
                    fprintf(stderr, "%s MISMATCH at %ld: %.6f vs %.6f\n",
                            names[v], t2, c[t2], ref[t2]);
                    return 2;
                }
        }

    printf("N=%d, best of %d interleaved runs, 1 thread\n", N, REPS);
    for (int v = 0; v < 3; v++)
        printf("  %-14s %8.4f ms  %7.1f GFLOP/s  %5.3fx vs base\n",
               names[v], ms[v], 2.0*N*N*N/(ms[v]*1e6), ms[0]/ms[v]);
    printf("  values identical across all three (c[0]=%.4f)\n", ref[0]);
    return 0;
}
