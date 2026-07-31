// Split the threaded SME GEMM into KERNEL vs EVERYTHING-ELSE. (S43)
//
// WHY. Threaded, N=4096 runs 54.164 ms = 2537 GF/s against a measured ~4100 GF/s
// two-unit ceiling. S43 refuted, threaded, every candidate on the memory side and
// the dispatch side: loads do not compete with `fmopa` for shared-unit issue; loads
// are free from L2 as well as L1; making the whole problem L2- and TLB-resident
// buys nothing (N=2048 is inside both walls, N=4096 outside both, 1.5% apart);
// slice quantization costs <=1%; and the pack/read-out/per-call terms are O(N^2)
// against an O(N^3) k loop that fits to ~0.
//
// THE SPLIT. This driver calls the REAL EMITTED KERNEL -- `@mapal_sme_panel` lifted
// verbatim out of the emitted module, linkage changed and nothing else -- with the
// exact arguments the emitter passes, from n threads, with `mapal-rt` removed.
// It measured 36.028 ms at 14 threads against the shipped 54.164: 1.50x, disjoint.
// The gap is ~18 ms at ONE thread and ~18 ms at FOURTEEN -- additive, not
// multiplicative, i.e. a SERIAL term inside the timed region that the driver omits.
//
// THE SUSPECT, read out of the emitted IR. `@task7` is `kind=0` (Seq): it runs ONCE,
// on ONE thread, packs the whole of b, and only then opens a nested parallel run for
// the matmul. Its loop is
//
//     for jt in 0..N/16: for k in 0..N: for lane in 0..16:
//         packed[jt*16*N + k*16 + lane] = b[k*N + jt*16 + lane]
//
// and for fixed `jt`, consecutive `k` are N*4 bytes apart -- at N=4096 that is
// 16384 B, EXACTLY ONE PAGE (`hw.pagesize` = 16384). So the walk touches one 64 B
// line per page across all 4096 pages of b, and repeats that for each of the 256
// panels: 1,048,576 page-crossing accesses against a measured TLB reach of ~2k-4k
// pages. At N=2048 b spans only 1024 pages and sits INSIDE that reach -- which is
// the prediction this sweep exists to test, and the reason the N^2 fit never saw it.
//
// Arms: `bpack` (the emitter's pack loop, serial, verbatim), `full` (pack + panel
// calls), `kernel` (panel calls only -- values wrong by construction, timing only).
//
// Build: clang -O2 -march=armv8-a+sme2 -o paneldrive paneldrive.c panel.ll
//        (NEVER armv9-a: implies +sve, this part has SME without SVE, SIGILL)
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define T 16
#define TI 2
#define TJ 2
#define PR (TI * T)          // panel rows = 32
#define PC (TJ * T)          // panel cols = 32

void mapal_sme_panel(const float *ap, const float *b, float *c,
                     long bn, long bj, long cn, long K);

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

static long NN;                       // matrix side, runtime
static float *a, *b, *bp, *out;
static _Atomic int next_panel;

// The emitter's B pack, verbatim from @task7 (m4096.ll bb3/bb6/bb9). Serial, as
// emitted: `task7` is kind=0 so exactly one thread ever runs this.
static void pack_b(void) {
    long panels = NN / T;
    for (long jt = 0; jt < panels; jt++)
        for (long k = 0; k < NN; k++)
            for (long lane = 0; lane < T; lane++)
                bp[jt * T * NN + k * T + lane] = b[k * NN + jt * T + lane];
}

typedef struct { float *ap; int do_pack; double ms; } Arg;

static void *worker(void *p) {
    Arg *w = (Arg *)p;
    double t0 = now_ms();
    for (;;) {
        int panel = atomic_fetch_add(&next_panel, 1);
        if (panel >= NN / PR) break;
        long i0 = (long)panel * PR;
        // The A pack, exactly as func/sme.rs emits it: row outer, k inner.
        if (w->do_pack) {
            for (long pi = 0; pi < PR; pi++) {
                const float *row = a + (i0 + pi) * NN;
                for (long pk = 0; pk < NN; pk++) w->ap[pk * PR + pi] = row[pk];
            }
        }
        for (long j0 = 0; j0 < NN; j0 += PC)
            mapal_sme_panel(w->ap, bp + j0 * NN, out + i0 * NN + j0, T, T * NN, NN, NN);
    }
    w->ms = now_ms() - t0;
    return NULL;
}

static double run(int nthreads, int do_pack, float **aps) {
    pthread_t th[64];
    Arg args[64];
    atomic_store(&next_panel, 0);
    for (int i = 0; i < nthreads; i++)
        args[i] = (Arg){ .ap = aps[i], .do_pack = do_pack, .ms = 0 };
    double t0 = now_ms();
    for (int i = 0; i < nthreads; i++) pthread_create(&th[i], NULL, worker, &args[i]);
    for (int i = 0; i < nthreads; i++) pthread_join(th[i], NULL);
    return now_ms() - t0;
}

static int cmp(const void *x, const void *y) {
    double d = *(const double *)x - *(const double *)y;
    return d < 0 ? -1 : d > 0;
}

static void report(const char *tag, int nthr, double *s, int reps) {
    qsort(s, reps, sizeof(double), cmp);
    printf("%6ld %8d %9s %10.3f %10.3f %10.3f %10.1f\n", NN, nthr, tag,
           s[0], s[reps / 2], s[reps - 1],
           2.0 * NN * NN * NN / (s[reps / 2] * 1e6));
    fflush(stdout);
}

int main(int argc, char **argv) {
    int reps = argc > 1 ? atoi(argv[1]) : 9;
    long sizes[] = {1024, 2048, 4096};
    int nsz = (int)(sizeof sizes / sizeof *sizes);
    int tcounts[] = {1, 14};
    int ntc = (int)(sizeof tcounts / sizeof *tcounts);

    printf("%6s %8s %9s %10s %10s %10s %10s\n",
           "N", "threads", "arm", "min ms", "median", "max", "GF/s");

    for (int si = 0; si < nsz; si++) {
        NN = sizes[si];
        size_t bytes = (size_t)NN * NN * 4;
        a = aligned_alloc(64, bytes);
        b = aligned_alloc(64, bytes);
        bp = aligned_alloc(64, bytes);
        out = aligned_alloc(64, bytes);
        float *aps[64];
        for (int i = 0; i < 14; i++) aps[i] = aligned_alloc(64, (size_t)PR * NN * 4);
        for (long i = 0; i < NN * NN; i++) {
            a[i] = (float)((i * 7 + 13) % 101 - 50);
            b[i] = (float)((i * 7 + 57) % 101 - 50);
        }
        memset(bp, 0, bytes);
        memset(out, 0, bytes);
        pack_b();

        // VALUE GATE, before any timing. An independent scalar reference over 97
        // spread cells, built from the ROW-MAJOR b, so it proves both the pack and
        // the kernel rather than letting them agree with each other.
        run(14, 1, aps);
        long bad = 0;
        for (long s = 0; s < 97; s++) {
            long i = (s * 1543) % NN, j = (s * 3079) % NN;
            float ref = 0;
            for (long k = 0; k < NN; k++) ref += a[i * NN + k] * b[k * NN + j];
            float d = out[i * NN + j] - ref; if (d < 0) d = -d;
            float m = ref < 0 ? -ref : ref;
            if (d > 1e-3f * m + 1e-2f) bad++;
        }
        if (bad) { printf("N=%ld VALUE GATE FAILED: %ld/97 differ\n", NN, bad); return 1; }
        printf("N=%ld value gate: 0/97 cells differ against a scalar reference\n", NN);

        double w0 = now_ms();
        while (now_ms() - w0 < 800.0) run(14, 0, aps);   // warm the clock (rule 14)

        // rep OUTER, arm INNER (rules 14/22): the arms alternate, so drift cannot
        // land on the arm axis.
        double sb[64], sf[2][64], sk[2][64];
        for (int r = 0; r < reps; r++) {
            double t0 = now_ms(); pack_b(); sb[r] = now_ms() - t0;
            for (int ti = 0; ti < ntc; ti++) {
                sf[ti][r] = run(tcounts[ti], 1, aps);
                sk[ti][r] = run(tcounts[ti], 0, aps);
            }
        }
        report("bpack", 1, sb, reps);
        for (int ti = 0; ti < ntc; ti++) {
            report("full", tcounts[ti], sf[ti], reps);
            report("kernel", tcounts[ti], sk[ti], reps);
        }

        free(a); free(b); free(bp); free(out);
        for (int i = 0; i < 14; i++) free(aps[i]);
    }

    printf("\n'bpack' is the emitter's SERIAL b pack, verbatim. The shipped path pays it\n"
           "inside the timed region on ONE thread; this driver's 'full'/'kernel' arms do\n"
           "not. GF/s on the bpack row is meaningless -- read its ms against the shipped\n"
           "minus driver gap (~18 ms at N=4096, at BOTH 1 and 14 threads).\n");
    return 0;
}
