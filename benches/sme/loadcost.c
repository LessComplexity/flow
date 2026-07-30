// Is the SME GEMM load-BOUND, or does something else happen at 4 loads?
//
// THE GAP. `fmopa` with zero memory traffic runs at ~2009 GFLOP/s on one unit
// (benches/sme/units.c, roofline.c). The real kernel, issuing the SAME four
// `fmopa` per iteration but feeding them from memory, runs at ~1043. Feeding the
// instructions costs half the throughput, and every SCHEDULING fix has been
// refuted: reordering loads 1.006x (mm4p.c), folding load instructions into
// multi-vector form 1.018x (mv.c), b layout 1.065x (bslice.c), k blocking
// net-negative threaded (func/sme.rs).
//
// So the question is what the load count itself costs. This holds the compute
// EXACTLY constant -- four independent `fmopa` into the four f32 ZA tiles, every
// iteration, in every variant -- and varies only how many of their operands come
// from memory rather than from registers loaded once before the loop.
//
//   0 loads   both operands preloaded (this is the roofline)
//   1 load    one A vector loaded
//   2 loads   one A, one B
//   3 loads   two A, one B
//   4 loads   two A, two B  <- the real 2x2 kernel
//
// READING IT. If throughput falls roughly linearly with the load count, the loop
// is load-bound and the 1-load-per-`fmopa` ratio IS the ceiling -- which would be
// architectural, not a scheduling failure, because f32 SME has exactly 4 ZA tiles
// and therefore cannot issue more MACs per load. If it is flat to 3 and then
// drops at 4, something else is happening and there is headroom to find.
//
// The buffer is L1-resident on purpose: this measures the LOAD PATH, not cache
// misses. A second pass with a buffer larger than L2 separates the two.
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// Every variant: 4 fmopa into za0..za3, `iters` times. Only the operand SOURCE
// differs. `mask` keeps the pointer inside the buffer without a branch.
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

int main(int argc, char **argv) {
    long iters = argc > 1 ? atol(argv[1]) : 50000000L;
    int reps = argc > 2 ? atoi(argv[2]) : 5;
    // two buffers: one inside L1D (128 KB here), one past L2, so a fall-off from
    // cache misses cannot be mistaken for a fall-off from load count
    long small = 32 * 1024 / 4;          // 32 KB, L1-resident
    long big = 64L * 1024 * 1024 / 4;    // 64 MB, past any cache
    float *sb = aligned_alloc(64, small * 4), *bb = aligned_alloc(64, big * 4);
    for (long i = 0; i < small; i++) sb[i] = 1.0f + (i % 7) * 0.01f;
    for (long i = 0; i < big; i++) bb[i] = 1.0f + (i % 7) * 0.01f;

    double sink = 0, w0 = now_ms();
    while (now_ms() - w0 < 300.0) sink += ld0(sb, small - 64, 200000);

    fn fns[5] = {ld0, ld1, ld2, ld3, ld4};
    const char *names[5] = {"0 loads (roofline)", "1 load", "2 loads", "3 loads",
                            "4 loads (real kernel)"};
    for (int pass = 0; pass < 2; pass++) {
        const float *buf = pass ? bb : sb;
        long mask = (pass ? big : small) - 64;
        printf("\n=== operands from %s ===\n", pass ? "a 64 MB buffer (past L2)"
                                                    : "a 32 KB buffer (L1-resident)");
        printf("  %-22s %10s %11s %9s %8s\n", "", "ms", "GFLOP/s", "% roofline", "vs prev");
        double roof = 0, prev = 0;
        for (int v = 0; v < 5; v++) {
            double best = 1e18;
            for (int r = 0; r < reps; r++) {
                double t0 = now_ms();
                sink += fns[v](buf, mask, iters);
                double d = now_ms() - t0;
                if (d < best) best = d;
            }
            // 4 fmopa per iteration, 512 flops each, in every variant
            double gf = 4.0 * 512.0 * iters / (best * 1e6);
            if (v == 0) roof = gf;
            printf("  %-22s %10.3f %11.1f %8.0f%% %8s\n", names[v], best, gf,
                   100.0 * gf / roof,
                   v ? ({ static char b[16]; snprintf(b, 16, "%.3fx", gf / prev); b; }) : "-");
            prev = gf;
        }
    }
    printf("\n  Same 4 fmopa in every row. If GFLOP/s falls ~linearly with the load\n"
           "  count, the loop is load-bound and 1 load per fmopa is the ceiling --\n"
           "  architectural, since f32 SME has exactly 4 ZA tiles.\n");
    printf("  (sink %.1f)\n", sink);
    return 0;
}
