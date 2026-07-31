// Where in the cache hierarchy does an SME operand stop being free?
//
// THE DEFECT THIS FIXES. `loadcost.c` holds the compute exactly constant (four
// independent `fmopa` into the four f32 ZA tiles, every iteration) and varies
// only how many operands come from memory. It has exactly TWO buffer sizes,
// 32 KB and 64 MB, so it prices **L1 against DRAM** and nothing in between. The
// project has been quoting its 2.4x as if it priced L1 against L2. It does not.
// This sweeps the buffer across the whole hierarchy so every level is measured
// rather than interpolated (measurement rule 17: sweep the parameter).
//
// This part (Apple M4 Pro, P core): L1D 128 KB, L1I 192 KB, L2 16 MB shared by
// 5 P cores => a ~3.2 MB per-core slice. Line size 128 B. The sweep straddles
// all three so the cliff can be attributed rather than assumed.
//
// TWO VARIANTS, same 4 fmopa and the same 256 B of fresh operand line per
// iteration, so they are directly comparable to each other and to `loadcost.c`:
//
//   (a) `loadcost.c`'s 4-load row, unchanged in shape: ONE contiguous stream,
//       four 64 B loads at p+0/+64/+128/+192, advancing 256 B per iteration.
//       Its 32 KB and 64 MB points are the validity check -- they must land on
//       the published 1864.2 and 760.8 GFLOP/s, or the published table is what
//       is wrong.
//
//   (b) the shape `mapal_sme_panel` actually emits (crates/backends/llvm/src/
//       module.rs, ti=tj=2, t=16, verified by reading it):
//         A: `%apk = %ap + k*ti*t` with loads at `%apk + r*t` for r in 0..2
//            => ONE stream, two 64 B loads 64 B apart, advancing 128 B per k.
//         B: `%bk = %b + k*%bn` with loads at `%bk` and `%bk + %bj`
//            => TWO streams, one 64 B load each, each advancing `%bn` per k,
//            displaced by `%bj`. Against the packed-B panel the NEON rung
//            already builds, `%bn` is `t` = 64 B and `%bj` is a whole packed
//            panel `t*k` -- 256 KB at k=4096. So: 64 B/iter each, 256 KB apart.
//       Total 128 + 64 + 64 = 256 B of fresh operand line per k iteration.
//
// FOOTPRINT. One 64 MB buffer; a size S means only its first S bytes are ever
// touched, so S *is* the working set. Variant (a) runs S/256 iterations per
// pass and covers all of S. Variant (b) runs S/128 per pass: A covers all of S,
// B0 covers [0, S/2), B1 covers [disp, disp + S/2). `disp` is 256 KB wherever
// that fits (S >= 512 KB) and S/2 below it -- the largest displacement that
// still keeps the touched set exactly S. Reported per row, not assumed.
//
// RULE 14 (warm the clock, interleave). The same binary has measured 1.852 ms
// cold and 1.069 ms warm. Warmup runs to a wall-clock budget before anything is
// timed, and the rep loop is the OUTER loop -- every (size, variant) cell is
// visited once per rep -- so a drift affects all cells alike instead of tilting
// the sweep. The first-rep/last-rep spread on a fixed reference cell is printed
// so an inadequate warmup is visible rather than assumed away.
//
// RULE 15 (find it in the assembly). `clang -O2 -march=armv8-a+sme2 -S` and
// count: both inner loops must show exactly 4 `fmopa` and 4 `ld1w`, unrolled by
// nothing, with the buffer size arriving as a register rather than folded in.
//
// BUILD -- NEVER armv9-a+sme2, it implies +sve and this part SIGILLs (README):
//   clang -O2 -march=armv8-a+sme2 -o loadlevel loadlevel.c
//   ./loadlevel 60000000 15 1000
//
// ---------------------------------------------------------------------------
// WHAT IT MEASURED (two independent runs, 60M iters/cell, 15 interleaved reps,
// 1 s warmup, drift 1.004x and 1.009x -- medians, GFLOP/s):
//
//   32K..8M   both variants ~1990, i.e. the ZERO-LOAD ROOFLINE, dead flat
//   12M       a 1750/1402   b 1882/1806      <- knee, the noisiest point
//   16M       a  959/978    b 1723/1742
//   24M       a  771/778    b 1401/1297
//   64M       a  752/751    b  833/829
//
// **THERE IS NO L1-VS-L2 COST.** Nothing happens at 128 KB (L1D) and nothing
// happens at 3.2 MB (the per-core L2 slice). At an 8 MB working set every
// operand line is an L1 miss served by L2 -- the stream has no reuse inside a
// pass and 8 MB cannot sit in a 128 KB L1D -- and it runs at the same ~1990
// GFLOP/s as the 32 KB buffer that never misses. The single cliff is at 8-12 MB
// and it tracks the 16 MB SHARED L2, arriving early because one thread does not
// get all 16 MB. The floor from 24 MB on is ~95 GB/s: single-core DRAM
// bandwidth, not a cache level.
//
// The kernel needs 249 GB/s of operand bandwidth to hold roofline. L1 and L2
// both supply it; DRAM supplies ~95 GB/s and throughput lands on that ratio.
// "Operand cache residency" is therefore a DRAM-bandwidth wall, not a cache
// level -- L2 is as good as L1 here.
//
// VALIDITY: 64 MB reproduces `loadcost.c`'s published 760.8 (764.9 / 771.0,
// +0.5% / +1.3%). 32 KB DOES NOT: 2001.0 / 1997.4 vs a published 1864.2, +7.2%.
// Re-running `loadcost.c`'s OWN binary today gives 2004.2 / 2000.4 / 1996.1 on
// that cell and a dead-flat L1 row, so the published 32 KB row -- the whole
// 1956.7 -> 1864.2 decline -- is a drifting machine, not a load-count effect.
// ---------------------------------------------------------------------------
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// Both variants: 4 fmopa into za0..za3 per inner iteration, `n` inner
// iterations per pass, `reps` passes. Reading ZA back at the end keeps the
// whole fmopa chain -- and therefore every load feeding it -- live.
#define TAIL return svlasta_f32(svpfalse(), svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0));

// (a) loadcost.c's ld4: one contiguous stream, 256 B per iteration.
__arm_new("za")
static float va(const float *buf, long n, long reps, long disp) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    (void)disp;
    for (long r = 0; r < reps; r++) {
        const float *p = buf;
        for (long i = 0; i < n; i++) {
            svfloat32_t a0 = svld1_f32(pg, p);
            svfloat32_t a1 = svld1_f32(pg, p + 16);
            svfloat32_t b0 = svld1_f32(pg, p + 32);
            svfloat32_t b1 = svld1_f32(pg, p + 48);
            svmopa_za32_f32_m(0, pg, pg, a0, b0);
            svmopa_za32_f32_m(1, pg, pg, a0, b1);
            svmopa_za32_f32_m(2, pg, pg, a1, b0);
            svmopa_za32_f32_m(3, pg, pg, a1, b1);
            p += 64;
        }
    }
    TAIL
}

// (b) mapal_sme_panel's shape: A 128 B/iter (2 loads, 64 B apart), two B
// streams 64 B/iter, `disp` floats apart.
__arm_new("za")
static float vb(const float *buf, long n, long reps, long disp) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (long r = 0; r < reps; r++) {
        const float *pa = buf, *pb = buf;
        for (long i = 0; i < n; i++) {
            svfloat32_t a0 = svld1_f32(pg, pa);
            svfloat32_t a1 = svld1_f32(pg, pa + 16);
            svfloat32_t b0 = svld1_f32(pg, pb);
            svfloat32_t b1 = svld1_f32(pg, pb + disp);
            svmopa_za32_f32_m(0, pg, pg, a0, b0);
            svmopa_za32_f32_m(1, pg, pg, a0, b1);
            svmopa_za32_f32_m(2, pg, pg, a1, b0);
            svmopa_za32_f32_m(3, pg, pg, a1, b1);
            pa += 32;
            pb += 16;
        }
    }
    TAIL
}

typedef float (*fn)(const float *, long, long, long) __arm_streaming;

static int cmpd(const void *x, const void *y) {
    double a = *(const double *)x, b = *(const double *)y;
    return (a > b) - (a < b);
}

// bytes-per-iteration is 256 in BOTH variants, which is what makes their
// GFLOP/s and GB/s columns comparable.
#define ITER_BYTES 256.0
#define ITER_FLOPS (4.0 * 512.0)

// The first thirteen are the specified sweep; 12/24/32/48 MB were added after
// the first run came back FLAT all the way to 8 MB, to bracket the one cliff
// that turned out to exist instead of guessing at it from two points.
static const long SIZES[] = {
    32L << 10,  64L << 10, 128L << 10, 192L << 10, 256L << 10, 512L << 10,
     1L << 20,   2L << 20,   3L << 20,   4L << 20,   8L << 20,  12L << 20,
    16L << 20,  24L << 20,  32L << 20,  48L << 20,  64L << 20,
};
#define NSZ ((int)(sizeof SIZES / sizeof *SIZES))

static void human(long b, char *out) {
    if (b >= 1 << 20) snprintf(out, 8, "%ldM", b >> 20);
    else snprintf(out, 8, "%ldK", b >> 10);
}

int main(int argc, char **argv) {
    long iters = argc > 1 ? atol(argv[1]) : 30000000L;
    int reps = argc > 2 ? atoi(argv[2]) : 7;
    double warm_ms = argc > 3 ? atof(argv[3]) : 500.0;

    long big = SIZES[NSZ - 1];
    // page-aligned so the 256 KB / S/2 displacements land on deterministic set
    // indices instead of a malloc-dependent offset
    float *buf = aligned_alloc(16384, big);
    if (!buf) { fprintf(stderr, "alloc failed\n"); return 1; }
    for (long i = 0; i < big / 4; i++) buf[i] = 1.0f + (i % 7) * 0.01f;

    fn fns[2] = {va, vb};
    const char *vn[2] = {"a", "b"};
    // inner-loop trip count per pass: (a) 256 B/iter covers S, (b) 128 B/iter
    // for the A stream covers S
    long n[NSZ][2], pass[NSZ][2], disp[NSZ];
    for (int s = 0; s < NSZ; s++) {
        n[s][0] = SIZES[s] / 256;
        n[s][1] = SIZES[s] / 128;
        long d = SIZES[s] / 2 < (256L << 10) ? SIZES[s] / 2 : (256L << 10);
        disp[s] = d / 4;   // floats
        for (int v = 0; v < 2; v++) pass[s][v] = (iters + n[s][v] - 1) / n[s][v];
    }

    double sink = 0, w0 = now_ms();
    while (now_ms() - w0 < warm_ms) sink += va(buf, 4096, 32, 0);

    static double t[NSZ][2][64];
    for (int r = 0; r < reps; r++)
        for (int s = 0; s < NSZ; s++)
            for (int v = 0; v < 2; v++) {
                double t0 = now_ms();
                sink += fns[v](buf, n[s][v], pass[s][v], disp[s]);
                t[s][v][r] = now_ms() - t0;
            }

    printf("SME operand-residency sweep -- 4 fmopa and 256 B of fresh operand\n"
           "line per iteration in EVERY row; only the working set changes.\n"
           "%ld iterations/cell (nominal), %d interleaved reps, %.0f ms warmup.\n"
           "L1D 128 KB | per-core L2 slice ~3.2 MB | shared L2 16 MB\n\n",
           iters, reps, warm_ms);
    printf("  %6s %3s %9s %9s %9s %9s %9s %9s %8s %8s\n", "size", "var",
           "ms min", "ms med", "ms max", "GF/s max", "GF/s med", "GF/s min",
           "GB/s med", "disp");
    for (int s = 0; s < NSZ; s++) {
        for (int v = 0; v < 2; v++) {
            double c[64];
            for (int r = 0; r < reps; r++) c[r] = t[s][v][r];
            qsort(c, reps, sizeof *c, cmpd);
            double lo = c[0], md = c[reps / 2], hi = c[reps - 1];
            double it = (double)n[s][v] * pass[s][v];
            char sz[8], dz[8];
            human(SIZES[s], sz);
            human(v ? disp[s] * 4 : 0, dz);
            printf("  %6s %3s %9.3f %9.3f %9.3f %9.1f %9.1f %9.1f %8.1f %8s\n",
                   v ? "" : sz, vn[v], lo, md, hi,
                   ITER_FLOPS * it / (lo * 1e6), ITER_FLOPS * it / (md * 1e6),
                   ITER_FLOPS * it / (hi * 1e6), ITER_BYTES * it / (md * 1e6),
                   v ? dz : "-");
        }
    }

    // Drift check: the reference cell's first rep against its last. If warmup
    // were inadequate these diverge and every number above is suspect.
    printf("\n  drift (32K/a): rep0 %.3f ms, rep%d %.3f ms  (%.3fx)\n",
           t[0][0][0], reps - 1, t[0][0][reps - 1],
           t[0][0][0] / t[0][0][reps - 1]);

    // Validity: variant (a) at 32 KB and 64 MB IS loadcost.c's 4-load row.
    // These must reproduce, or loadcost.c's published table is the finding.
    double g32 = ITER_FLOPS * n[0][0] * pass[0][0] / (t[0][0][0] * 1e6);
    {
        double c[64];
        for (int r = 0; r < reps; r++) c[r] = t[0][0][r];
        qsort(c, reps, sizeof *c, cmpd);
        g32 = ITER_FLOPS * (double)n[0][0] * pass[0][0] / (c[0] * 1e6);
        for (int r = 0; r < reps; r++) c[r] = t[NSZ - 1][0][r];
        qsort(c, reps, sizeof *c, cmpd);
        double g64 = ITER_FLOPS * (double)n[NSZ - 1][0] * pass[NSZ - 1][0] / (c[0] * 1e6);
        printf("  reproduce loadcost.c 4-load:  32 KB %.1f vs 1864.2 (%+.1f%%)"
               "   64 MB %.1f vs 760.8 (%+.1f%%)\n",
               g32, 100.0 * (g32 / 1864.2 - 1.0), g64, 100.0 * (g64 / 760.8 - 1.0));
    }
    printf("  (sink %.1f)\n", sink);
    return 0;
}
