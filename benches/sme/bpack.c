// The serial B pack is 30% of the threaded SME GEMM. What fixes it? (S43)
//
// FINDING BEING ACTED ON. `benches/results-s43/threaded-ceiling.md`: at N=4096 the
// shipped threaded matmul is 54.164 ms, of which the emitter's B pack (`@task7`,
// emitted `kind=0` = Seq) is 16.349 ms on ONE thread while 13 lanes idle. The
// parallel matmul phase already runs at 3831 GF/s = 93% of the two-unit ceiling.
//
// The pack is bound by PAGE VISITS, not bytes -- measured across N=1024/2048/4096,
// time tracked the 8x-per-step page-crossing count, not the 4x-per-step byte count,
// and at N=4096 it moves 128 MB at 7.8 GB/s against a ~95 GB/s DRAM floor.
// The cause is in the emitted loop order: a b row is 4096 floats = 16384 B = EXACTLY
// one page (`hw.pagesize` = 16384), so
//
//     for jt: for k: for lane:  packed[jt*16*N + k*16 + lane] = b[k*N + jt*16 + lane]
//
// touches one 64 B line of each of 4096 pages per `jt`, and repeats it 256 times.
//
// So there are two independent defects and this probe separates them:
//   1. it is SERIAL     -- trivially parallel over jt (256 independent panels)
//   2. it is PAGE-HOSTILE -- k-blocking makes the row working set TLB-resident and
//      reused across all jt, instead of streaming 4096 pages 256 times over
//
// Arms (all produce byte-identical output, checked before any timing):
//   base    the emitter's loop, verbatim, serial          -- the control
//   par     the emitter's loop, parallel over jt          -- the MINIMAL emitter fix
//   blk     k-blocked, serial                             -- prices loop order alone
//   parblk  k-blocked and parallel                        -- prices the whole fix
//
// Build: clang -O2 -march=armv8-a+sme2 -o bpack bpack.c
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define T 16
#define KB 512               // k-block: 512 rows = 512 pages, inside TLB reach

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

static long NN, PANELS;
static float *b, *bp;
static _Atomic int next_jt;
static int nthreads;

// --- the emitter's loop, verbatim (m4096.ll @task7 bb3/bb6/bb9).
static void pack_range(long jt0, long jt1) {
    for (long jt = jt0; jt < jt1; jt++)
        for (long k = 0; k < NN; k++)
            for (long lane = 0; lane < T; lane++)
                bp[jt * T * NN + k * T + lane] = b[k * NN + jt * T + lane];
}

// --- k-blocked: the row working set is KB pages and is reused across every jt,
// instead of sweeping all N pages once per jt.
static void pack_range_blk(long jt0, long jt1) {
    for (long k0 = 0; k0 < NN; k0 += KB) {
        long k1 = k0 + KB < NN ? k0 + KB : NN;
        for (long jt = jt0; jt < jt1; jt++)
            for (long k = k0; k < k1; k++)
                for (long lane = 0; lane < T; lane++)
                    bp[jt * T * NN + k * T + lane] = b[k * NN + jt * T + lane];
    }
}

static void *worker(void *arg) {
    int blocked = *(int *)arg;
    for (;;) {
        int jt = atomic_fetch_add(&next_jt, 1);
        if (jt >= PANELS) break;
        if (blocked) pack_range_blk(jt, jt + 1); else pack_range(jt, jt + 1);
    }
    return NULL;
}

static void run_par(int blocked) {
    pthread_t th[64];
    atomic_store(&next_jt, 0);
    for (int i = 0; i < nthreads; i++) pthread_create(&th[i], NULL, worker, &blocked);
    for (int i = 0; i < nthreads; i++) pthread_join(th[i], NULL);
}

static void arm_base(void)   { pack_range(0, PANELS); }
static void arm_par(void)    { run_par(0); }
static void arm_blk(void)    { pack_range_blk(0, PANELS); }
static void arm_parblk(void) { run_par(1); }

static int cmp(const void *x, const void *y) {
    double d = *(const double *)x - *(const double *)y;
    return d < 0 ? -1 : d > 0;
}

int main(int argc, char **argv) {
    NN = argc > 1 ? atol(argv[1]) : 4096;
    int reps = argc > 2 ? atoi(argv[2]) : 15;
    nthreads = argc > 3 ? atoi(argv[3]) : 14;
    PANELS = NN / T;

    size_t bytes = (size_t)NN * NN * 4;
    b = aligned_alloc(64, bytes);
    bp = aligned_alloc(64, bytes);
    float *ref = aligned_alloc(64, bytes);
    for (long i = 0; i < NN * NN; i++) b[i] = (float)((i * 7 + 57) % 101 - 50);

    void (*arms[4])(void) = {arm_base, arm_par, arm_blk, arm_parblk};
    const char *names[4] = {"base (emitted)", "par", "blk", "par+blk"};

    // VALUE GATE before any timing: every arm must reproduce the emitted arm's
    // output byte for byte, or the comparison is meaningless.
    memset(bp, 0, bytes); arm_base(); memcpy(ref, bp, bytes);
    for (int v = 1; v < 4; v++) {
        memset(bp, 0, bytes);
        arms[v]();
        if (memcmp(ref, bp, bytes)) { printf("VALUE GATE FAILED: arm %s differs\n", names[v]); return 1; }
    }
    printf("value gate: all 4 arms byte-identical over %zu MB\n", bytes >> 20);

    double w0 = now_ms();
    while (now_ms() - w0 < 500.0) arm_base();     // warm the clock (rule 14)

    double s[4][64];
    // rep OUTER, arm INNER (rules 14/22) so drift cannot land on the arm axis.
    for (int r = 0; r < reps; r++)
        for (int v = 0; v < 4; v++) {
            double t0 = now_ms(); arms[v](); s[v][r] = now_ms() - t0;
        }

    printf("\nN=%ld, %d threads where parallel, %d alternating reps\n", NN, nthreads, reps);
    printf("%16s %10s %10s %10s %10s %10s\n", "arm", "min ms", "median", "max", "GB/s", "vs base");
    double base = 0;
    for (int v = 0; v < 4; v++) {
        qsort(s[v], reps, sizeof(double), cmp);
        double med = s[v][reps / 2];
        if (v == 0) base = med;
        printf("%16s %10.3f %10.3f %10.3f %10.1f %9.3fx\n", names[v],
               s[v][0], med, s[v][reps - 1], 2.0 * bytes / (med * 1e6), base / med);
    }
    printf("\nbase is the emitted pack. 'par' is the minimal emitter fix (make @task7\n"
           "parallel over jt); 'blk' is the loop-order fix alone; 'par+blk' is both.\n");
    return 0;
}
