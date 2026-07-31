// tblock.c — what makes working-set blocking pay on this part: CAPACITY or
// CONFLICT? The transpose rung of the shape ladder is the instrument.
//
// S44. S43 established that on this M4 Pro an operand living in L2 rather than
// L1 costs **nothing** — flat ~1990 GF/s / 249 GB/s from a 32 KB buffer to an
// 8 MB one, no cliff at L1D, none at the L2 slice. That is a statement about
// CAPACITY, and `tlbreach.c` deliberately used ODD strides to keep power-of-two
// set-index collapse out of the measurement.
//
// This probe measures the thing that was held out. The ladder's transpose is
// `b[i·S + j] = a[j·S + i]`: the read stride is `S·4` bytes, which at S = 1024 is
// 4096 = 2¹². With 128 B lines (`hw.cachelinesize`) the set index advances
// 4096/128 = 32 sets per read, so over a 128-set L1D the walk lands on
// gcd(32,128) → only **4 distinct sets**. Capacity is 1024 lines; the walk can
// use `4 × ways`.
//
// TWO AXES, interleaved in ONE arm rotation so they are directly comparable:
//   * `bs`  — square blocking of the traversal. Caps the rows in flight.
//   * `pad` — the read array's ROW STRIDE, `lda = S + pad`. Changes NOTHING about
//     the traversal; it only breaks the power-of-two set-index collapse.
//
// **`pad` is the mechanism test and it is the point of this file.** If padding
// alone recovers the win with `bs` untouched, the finding is "this machine
// punishes power-of-two strides", which is general and needs no compiler change.
// If only `bs` recovers it, the finding is about working-set size after all.
//
// MEASUREMENT DISCIPLINE (S43 rules):
//  * ONE loop body serves every arm — `bs` and `lda` are runtime arguments, so a
//    per-arm transformation is impossible by construction. `bs == side` IS the
//    unblocked traversal (one block), not a separate code path.
//  * Every buffer is WRITTEN before timing: a page fault is a kernel trap, not
//    the effect under test.
//  * `a` is filled BY LOGICAL INDEX, so every pad arm holds the same logical
//    matrix and every arm's output checksum is bit-identical. Checked before any
//    timing is read.
//  * Arms interleave round-robin after a warm-up, and each carries a NULL
//    CONTROL measured back-to-back inside the same cell (rule 22). The control
//    reads `b` — which `transpose` writes — because a control reading only
//    `const *restrict a` gets CSE'd away once the constant-trip arm loop is
//    unrolled. That happened and voided run 1 (rule 23).
//
// build: clang -O3 -march=armv8-a+sme2 benches/shapes/tblock.c -o tblock -lm
// run:   ./tblock <side> <cycles>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1e3 + ts.tv_nsec * 1e-6;
}

// THE one loop body. `bs` and `lda` are runtime arguments; `bs == side` collapses
// the outer pair to a single trip and reproduces the flat `for i for j` traversal
// the emitter produces today, instruction for instruction.
__attribute__((noinline)) static void transpose(const float *restrict a,
                                                float *restrict b, long side,
                                                long bs, long lda) {
    for (long ii = 0; ii < side; ii += bs) {
        long i_hi = ii + bs < side ? ii + bs : side;
        for (long jj = 0; jj < side; jj += bs) {
            long j_hi = jj + bs < side ? jj + bs : side;
            for (long i = ii; i < i_hi; ++i) {
                for (long j = jj; j < j_hi; ++j) {
                    b[i * side + j] = a[j * lda + i];
                }
            }
        }
    }
}

// The null arm: fixed work, no `bs` and no `lda` term anywhere. It must not move
// across the sweep, or the run is void.
__attribute__((noinline)) static float control(const float *restrict b, long n) {
    float s = 0.0f;
    for (long i = 0; i < n; ++i) s += b[i];
    return s;
}

static double checksum(const float *b, long n) {
    double s = 0.0;
    for (long i = 0; i < n; ++i) s += (double)b[i] * (double)((i % 97) + 1);
    return s;
}

static int cmp_d(const void *x, const void *y) {
    double a = *(const double *)x, b = *(const double *)y;
    return (a > b) - (a < b);
}

// The arm table: (block size, row-stride padding). `bs == 0` means "unblocked".
struct Arm { long bs; long pad; };
static const struct Arm ARMS[] = {
    {0, 0},                                              // the reference
    {8, 0},   {16, 0},  {24, 0},  {32, 0},  {48, 0},     // blocking sweep
    {64, 0},  {128, 0},
    {0, 1},   {0, 2},   {0, 4},   {0, 8},                // padding sweep
    {0, 16},  {0, 32},  {0, 33},
    {16, 16},                                            // both at once
};
#define NARM ((int)(sizeof(ARMS) / sizeof(ARMS[0])))
#define MAXPAD 33

int main(int argc, char **argv) {
    long side = argc > 1 ? atol(argv[1]) : 1024;
    int cycles = argc > 2 ? atoi(argv[2]) : 11;
    long n = side * side;

    // One `a` buffer per DISTINCT pad, each filled by logical index so all arms
    // read the same logical matrix and produce the same output bit-for-bit.
    float *A[MAXPAD + 1];
    memset(A, 0, sizeof(A));
    for (int t = 0; t < NARM; ++t) {
        long pad = ARMS[t].pad;
        if (A[pad]) continue;
        long lda = side + pad;
        A[pad] = malloc((size_t)side * lda * sizeof(float));
        if (!A[pad]) { fprintf(stderr, "alloc failed (pad %ld)\n", pad); return 2; }
        // Written, not just mapped: every page resident and every PTE populated.
        for (long r = 0; r < side; ++r)
            for (long c = 0; c < lda; ++c)
                A[pad][r * lda + c] =
                    c < side ? (float)(((r * side + c) * 7 + 13) % 101 - 50) : 0.0f;
    }
    float *b = malloc((size_t)n * sizeof(float));
    if (!b) { fprintf(stderr, "alloc failed (b)\n"); return 2; }
    for (long i = 0; i < n; ++i) b[i] = 0.0f;

    // Value gate, before any timing.
    transpose(A[0], b, side, side, side);
    double ref = checksum(b, n);
    for (int t = 0; t < NARM; ++t) {
        long bs = ARMS[t].bs ? ARMS[t].bs : side, pad = ARMS[t].pad;
        memset(b, 0, (size_t)n * sizeof(float));
        transpose(A[pad], b, side, bs, side + pad);
        double got = checksum(b, n);
        if (got != ref) {
            fprintf(stderr, "VALUE MISMATCH bs=%ld pad=%ld: %.17g != %.17g\n", bs,
                    pad, got, ref);
            return 1;
        }
    }
    printf("values: identical at every arm (checksum %.17g)\n", ref);

    // Warm the clock (rule 1: 1.73x cold-vs-warm on identical code).
    double warm_until = now_ms() + 1000.0;
    float sink = 0.0f;
    while (now_ms() < warm_until) { transpose(A[0], b, side, 64, side); sink += b[0]; }

    double *ts = calloc((size_t)NARM * cycles, sizeof(double));
    double *cs = calloc((size_t)NARM * cycles, sizeof(double));
    for (int c = 0; c < cycles; ++c) {
        for (int t = 0; t < NARM; ++t) {
            long bs = ARMS[t].bs ? ARMS[t].bs : side, pad = ARMS[t].pad;
            double t0 = now_ms();
            transpose(A[pad], b, side, bs, side + pad);
            double t1 = now_ms();
            double c0 = now_ms();
            sink += control(b, n);
            double c1 = now_ms();
            ts[(size_t)t * cycles + c] = t1 - t0;
            cs[(size_t)t * cycles + c] = c1 - c0;
        }
    }

    // The set-index arithmetic this probe exists to test, printed with the data.
    long line = 128, l1d = 128 * 1024, sets8 = l1d / line / 8;
    printf("side=%ld (%.1f MB/array) cycles=%d sink=%g\n", side,
           (double)n * 4 / (1 << 20), cycles, (double)sink);
    printf("L1D %ld KB, %ld B lines, %ld sets at 8-way\n", l1d / 1024, line, sets8);
    printf("%6s %6s %8s %10s %10s %10s %10s %8s\n", "bs", "pad", "sets", "min",
           "median", "max", "ctl med", "GB/s");
    double ctl_lo = 1e300, ctl_hi = 0.0, ref_med = 0.0;
    for (int t = 0; t < NARM; ++t) {
        double *v = ts + (size_t)t * cycles, *w = cs + (size_t)t * cycles;
        qsort(v, cycles, sizeof(double), cmp_d);
        qsort(w, cycles, sizeof(double), cmp_d);
        double med = v[cycles / 2], cmed = w[cycles / 2];
        if (cmed < ctl_lo) ctl_lo = cmed;
        if (cmed > ctl_hi) ctl_hi = cmed;
        if (t == 0) ref_med = med;
        long bs = ARMS[t].bs ? ARMS[t].bs : side, pad = ARMS[t].pad;
        // Distinct L1D sets the strided read walks: the set stride is
        // (lda*4/line) mod nsets, and the walk covers nsets/gcd(stride, nsets).
        long ss = ((side + pad) * 4 / line) % sets8, g = ss, h = sets8;
        while (h) { long r = g % h; g = h; h = r; }
        long nsets = (side + pad) * 4 % line ? sets8 : sets8 / (g ? g : sets8);
        printf("%6ld %6ld %8ld %10.3f %10.3f %10.3f %10.3f %8.1f  %.3fx\n", bs,
               pad, nsets, v[0], med, v[cycles - 1], cmed,
               (double)n * 8 / (med * 1e-3) / 1e9, ref_med / med);
    }
    double drift = ctl_hi / ctl_lo;
    printf("control spread: %.3fx -> %s\n", drift,
           drift > 1.06 ? "VOID (rule 22: the null arm moved)" : "clean");
    return 0;
}
