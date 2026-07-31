// Cache reach or TLB reach? Hold BYTES constant, vary PAGE SPAN.
//
// THE CONFOUND THIS EXISTS TO BREAK. The S43 operand-window arms measured a
// 1.71x win by shrinking the B stream's window. But shrinking a window shrinks
// the byte footprint AND the page footprint together, so "operand cache
// residency" and "operand page (TLB) residency" predict the whole arm table
// identically. Nothing in the kernel separates them, because the kernel's k
// offset cannot be pushed past ~32 pages without leaving its allocation.
//
// THE SEPARATION. `hw.pagesize` = 16384. One iteration touches one 256 B chunk
// (4 loads at +0/+64/+128/+192, 2 cache lines) and does 4 `fmopa` -- byte- and
// flop-identical to `loadcost.c`'s row and `loadlevel.c`'s variant (a), so the
// numbers are directly comparable to both. Chunk `j` sits at byte `j * 256 * M`.
// Sweeping the odd multiplier M at a FIXED chunk count N:
//
//     bytes touched = N * 256                      -- CONSTANT along an M sweep
//     pages touched = min(N, floor((N-1)*M/64)+1)  -- rises 64x with M
//     span          = N * 256 * M                  -- rises with M
//
// At N = 4096 the working set is 1 MB at EVERY M. `loadlevel.c` measured this
// machine dead flat from 32 KB to 8 MB, so 1 MB is free of capacity effects by
// that instrument's own reading. Anything that happens as M drives the page
// count from 64 to 4096 is therefore TRANSLATION, not capacity.
//
// WHY M IS ODD. A power-of-two stride puts every chunk in the same cache set
// and measures conflict misses, not residency. Odd multiples of 256 rotate the
// set index. M in {1,3,5,9,17,33,65,129,257}: for M >= 64 every chunk lands on
// its own page, so M = 65 / 129 / 257 hold BOTH bytes and pages constant and
// vary only the span -- the built-in control for any residual conflict or DRAM
// row effect.
//
// WHY BOTH ORDERS. Raising M destroys spatial locality as well as page
// locality, so a plain M sweep confounds the TLB with the hardware prefetcher.
// `rev` visits the same chunk set in bit-reversed index order, which is
// maximally prefetch-hostile at EVERY M. Reading the M sweep along rev=1 holds
// prefetch hostility constant and moves only the page count. The rev=0 row at
// M=1 is the sequential stream, and it is the calibration cell.
//
// IDENTICAL CODE IN EVERY CELL (rule 15). `n`, `reps`, `stride_f`, `rsh` and
// `rev` are all runtime arguments of ONE function, so a cell change moves
// register contents and nothing else. Only the rev=0/rev=1 paths may differ,
// if the compiler unswitches; the M sweep within a fixed order cannot.
//
// CALIBRATION GATE. (32 KB, M=1, rev=0) IS `loadcost.c`'s 4-load row and
// `loadlevel.c`'s 32 KB variant (a). It must land near 1990-2005 GFLOP/s. If it
// does not, the index arithmetic is costing and every number below is measuring
// that instead. The gate is printed, not assumed.
//
// BUILD -- NEVER armv9-a+sme2, it implies +sve and this part SIGILLs:
//   clang -O2 -march=armv8-a+sme2 -o tlbreach tlbreach.c
//   benches/perflock.sh ./tlbreach 30000000 11 1000
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define PAGE 16384L
#define CHUNK 256L
#define ITER_BYTES 256.0
#define ITER_FLOPS (4.0 * 512.0)

static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// The one loop. Reading ZA back at the end keeps the whole fmopa chain -- and
// therefore every load feeding it -- live.
__arm_new("za")
static float walk(const float *buf, long n, long reps, long stride_f, int rsh,
                  int rev) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (long r = 0; r < reps; r++) {
        for (long i = 0; i < n; i++) {
            unsigned long b = __builtin_bitreverse64((unsigned long)i) >> rsh;
            unsigned long idx = rev ? b : (unsigned long)i;
            const float *p = buf + idx * (unsigned long)stride_f;
            svfloat32_t a0 = svld1_f32(pg, p);
            svfloat32_t a1 = svld1_f32(pg, p + 16);
            svfloat32_t b0 = svld1_f32(pg, p + 32);
            svfloat32_t b1 = svld1_f32(pg, p + 48);
            svmopa_za32_f32_m(0, pg, pg, a0, b0);
            svmopa_za32_f32_m(1, pg, pg, a0, b1);
            svmopa_za32_f32_m(2, pg, pg, a1, b0);
            svmopa_za32_f32_m(3, pg, pg, a1, b1);
        }
    }
    return svlasta_f32(svpfalse(),
                       svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0));
}

static const int LGN[] = {7, 10, 12, 14};   // 32 KB, 256 KB, 1 MB, 4 MB touched
static const long MUL[] = {1, 3, 5, 9, 17, 33, 65, 129, 257};
#define NLG ((int)(sizeof LGN / sizeof *LGN))
#define NMU ((int)(sizeof MUL / sizeof *MUL))
#define NREV 2
#define NCELL (NLG * NMU * NREV)
#define MAXREP 64

// Cap the span so the resident set stays sane; cells past it are skipped and
// SAID to be skipped rather than silently dropped.
#define SPAN_CAP (320L << 20)

static int cmpd(const void *x, const void *y) {
    double a = *(const double *)x, b = *(const double *)y;
    return (a > b) - (a < b);
}

static void human(long b, char *out, size_t n) {
    if (b >= 1 << 20) snprintf(out, n, "%ldM", b >> 20);
    else if (b >= 1 << 10) snprintf(out, n, "%ldK", b >> 10);
    else snprintf(out, n, "%ldB", b);
}

// distinct pages hit by chunks 0..n-1 at byte stride 256*m
static long pages_of(long n, long m) {
    long per = PAGE / CHUNK;            // 64 chunks per page at m == 1
    if (m >= per) return n;             // every chunk on its own page
    return ((n - 1) * m) / per + 1;
}

int main(int argc, char **argv) {
    long iters = argc > 1 ? atol(argv[1]) : 30000000L;
    int reps = argc > 2 ? atoi(argv[2]) : 11;
    double warm_ms = argc > 3 ? atof(argv[3]) : 1000.0;
    if (reps > MAXREP) reps = MAXREP;

    struct cell {
        int lg, rv;
        long n, m, stride_f, span, bytes, pages, pass;
        int rsh, live;
    } c[NCELL];
    int nc = 0, skipped = 0;
    long need = 0;
    for (int g = 0; g < NLG; g++)
        for (int u = 0; u < NMU; u++)
            for (int v = 0; v < NREV; v++) {
                long n = 1L << LGN[g], m = MUL[u];
                long span = n * CHUNK * m;
                struct cell *x = &c[nc];
                x->lg = LGN[g]; x->rv = v; x->n = n; x->m = m;
                x->stride_f = CHUNK * m / 4;
                x->span = span; x->bytes = n * CHUNK;
                x->pages = pages_of(n, m);
                x->rsh = 64 - LGN[g];
                x->pass = (iters + n - 1) / n;
                x->live = span + CHUNK <= SPAN_CAP;
                if (!x->live) skipped++;
                else if (span > need) need = span;
                nc++;
            }
    need += CHUNK;

    float *buf = aligned_alloc(PAGE, (size_t)((need + PAGE - 1) / PAGE * PAGE));
    if (!buf) { fprintf(stderr, "alloc of %ld MB failed\n", need >> 20); return 1; }

    // Pre-fault and initialise ONLY the chunks any live cell touches -- a page
    // fault inside a timed region would be measured as a translation cost, which
    // is exactly the quantity under test.
    for (int x = 0; x < nc; x++) {
        if (!c[x].live) continue;
        for (long j = 0; j < c[x].n; j++) {
            float *p = buf + (unsigned long)j * (unsigned long)c[x].stride_f;
            for (int w = 0; w < 64; w++) p[w] = 1.0f + (w % 7) * 0.01f;
        }
    }

    double sink = 0, w0 = now_ms();
    while (now_ms() - w0 < warm_ms) sink += walk(buf, 4096, 8, 64, 52, 0);

    // one untimed visit per cell, so no cell pays another's first-touch
    for (int x = 0; x < nc; x++)
        if (c[x].live) sink += walk(buf, c[x].n, 1, c[x].stride_f, c[x].rsh, c[x].rv);

    static double t[NCELL][MAXREP];
    for (int r = 0; r < reps; r++)
        for (int x = 0; x < nc; x++) {
            if (!c[x].live) { t[x][r] = 0; continue; }
            double t0 = now_ms();
            sink += walk(buf, c[x].n, c[x].pass, c[x].stride_f, c[x].rsh, c[x].rv);
            t[x][r] = now_ms() - t0;
        }

    printf("SME operand reach -- bytes held CONSTANT along each M sweep, page\n"
           "span varied 64x. 4 fmopa and 256 B of fresh operand line per\n"
           "iteration in EVERY row. page %ld B | L1D 128 KB | L2 slice ~3.2 MB |\n"
           "shared L2 16 MB.  %ld iterations/cell (nominal), %d interleaved reps,\n"
           "%.0f ms warmup, buffer %ld MB, %d cells (%d skipped past the %ld MB\n"
           "span cap).\n\n",
           PAGE, iters, reps, warm_ms, need >> 20, nc, skipped, SPAN_CAP >> 20);
    printf("  %7s %4s %8s %7s %7s %4s %9s %9s %9s %9s %9s\n", "bytes", "M",
           "stride", "pages", "span", "ord", "ms min", "ms med", "ms max",
           "GF/s med", "GB/s med");

    double gf[NCELL];
    int lastlg = -1;
    for (int x = 0; x < nc; x++) {
        char bz[12], sz[12], st[12];
        human(c[x].bytes, bz, sizeof bz);
        human(c[x].span, sz, sizeof sz);
        snprintf(st, sizeof st, "%ldB", CHUNK * c[x].m);
        if (c[x].lg != lastlg) { printf("\n"); lastlg = c[x].lg; }
        if (!c[x].live) {
            printf("  %7s %4ld %8s %7ld %7s %4s   skipped (span past cap)\n",
                   bz, c[x].m, st, c[x].pages, sz, c[x].rv ? "rev" : "seq");
            gf[x] = 0;
            continue;
        }
        double v[MAXREP];
        for (int r = 0; r < reps; r++) v[r] = t[x][r];
        qsort(v, reps, sizeof *v, cmpd);
        double lo = v[0], md = v[reps / 2], hi = v[reps - 1];
        double it = (double)c[x].n * c[x].pass;
        gf[x] = ITER_FLOPS * it / (md * 1e6);
        printf("  %7s %4ld %8s %7ld %7s %4s %9.3f %9.3f %9.3f %9.1f %9.1f\n",
               bz, c[x].m, st, c[x].pages, sz, c[x].rv ? "rev" : "seq", lo, md,
               hi, gf[x], ITER_BYTES * it / (md * 1e6));
    }

    // ---- the two readings the sweep exists for ----
    printf("\n  --- constant bytes, rising pages (the separation) ---\n");
    printf("  %7s %4s %9s %9s %9s %9s\n", "bytes", "ord", "pg lo", "pg hi",
           "GF/s lo", "GF/s hi");
    for (int g = 0; g < NLG; g++)
        for (int v = 0; v < NREV; v++) {
            int first = -1, last = -1;
            for (int x = 0; x < nc; x++)
                if (c[x].lg == LGN[g] && c[x].rv == v && c[x].live) {
                    if (first < 0) first = x;
                    last = x;
                }
            if (first < 0) continue;
            char bz[12];
            human(c[first].bytes, bz, sizeof bz);
            printf("  %7s %4s %9ld %9ld %9.1f %9.1f   %.3fx over %.0fx more pages\n",
                   bz, v ? "rev" : "seq", c[first].pages, c[last].pages,
                   gf[first], gf[last], gf[first] / gf[last],
                   (double)c[last].pages / c[first].pages);
        }

    printf("\n  drift (cell 0): rep0 %.3f ms, rep%d %.3f ms  (%.3fx)\n", t[0][0],
           reps - 1, t[0][reps - 1], t[0][0] / t[0][reps - 1]);
    // (32 KB, M=1, seq) IS loadcost.c's 4-load row / loadlevel.c's 32 KB (a).
    for (int x = 0; x < nc; x++)
        if (c[x].lg == 7 && c[x].m == 1 && c[x].rv == 0)
            printf("  CALIBRATION (32 KB, M=1, seq) = %.1f GF/s vs loadlevel.c's "
                   "~1990-2005 (%+.1f%% vs 1997)\n"
                   "  -> off by more than a few %% means the index arithmetic is "
                   "costing and the sweep is measuring THAT.\n",
                   gf[x], 100.0 * (gf[x] / 1997.0 - 1.0));
    printf("  (sink %.1f)\n", sink);
    return 0;
}
