// Is the A-pack itself slower when k is blocked -- with no SME involved at all?
//
// THE BISECTION SO FAR (N=4096, 1 thread, M4 Pro). Blocking makes the kernel
// FASTER, 931 -> 1598 GFLOP/s, and loses it all to non-kernel cost:
//
//   non-kernel cost (kernel forced to K=1):  unblocked 29.97 ms, blocked 139.76 ms
//
// Eliminated as causes, each by measurement:
//   * the loop nest        -- emission verified index by index; work counts exact
//   * the B layout         -- 1.065x (benches/sme/bslice.c)
//   * the read-out code    -- emitted asm is 4 instructions per tile, no spills
//   * the streaming-mode ABI -- 1.0 ms over 131072 calls (benches/sme/smcost.c)
//
// What is left is the pack, and the pack moves the SAME 16.78M elements either
// way (8 x 128 x 512 x 32 blocked, 128 x 4096 x 32 unblocked). So either its
// memory behaviour genuinely differs, or the emitter's generated pack code is
// the defect. This file settles which, in plain C with no SME, no ZA, no
// streaming mode -- just the two loop shapes over the same array.
//
// The loops replicate the EMITTED nest exactly (func/sme.rs::emit_tiled_map_sme):
//   unblocked   for i0        { for pk in 0..k  { for pi in 0..32 { ap[pk*32+pi] = a[(i0+pi)*N + pk]      } } }
//   blocked     for k0 { for i0 { for pk in 0..kc { for pi in 0..32 { ap[pk*32+pi] = a[(i0+pi)*N + k0+pk] } } } }
//
// If blocked is ~4.66x slower here too, it is a real memory effect and the
// emitter is not at fault -- KC blocking is simply incompatible with this pack
// order. If the two are close, the emitter's pack codegen is the defect.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

#define PANEL_ROWS 32

// one i-panel, k in [k0, k0+depth) -- the emitted pack loop, pk outer, pi inner
static void pack(const float *a, float *ap, long N, long i0, long k0, long depth) {
    for (long pk = 0; pk < depth; pk++)
        for (long pi = 0; pi < PANEL_ROWS; pi++)
            ap[pk * PANEL_ROWS + pi] = a[(i0 + pi) * N + k0 + pk];
}

int main(int argc, char **argv) {
    long N = argc > 1 ? atol(argv[1]) : 4096;
    long kc = argc > 2 ? atol(argv[2]) : 512;
    int reps = argc > 3 ? atoi(argv[3]) : 5;

    float *a = aligned_alloc(64, (size_t)N * N * 4);
    // the blocked buffer is kc deep, the unblocked one the whole k axis -- as
    // the emitter allocates them (`panel_rows * kc` vs `panel_rows * k`)
    float *ap_b = aligned_alloc(64, (size_t)PANEL_ROWS * kc * 4);
    float *ap_u = aligned_alloc(64, (size_t)PANEL_ROWS * N * 4);
    if (!a || !ap_b || !ap_u) { fprintf(stderr, "alloc failed\n"); return 1; }
    for (long t = 0; t < N * N; t++) a[t] = (float)((t * 7) % 13) * 0.01f + 1.0f;
    memset(ap_b, 0, (size_t)PANEL_ROWS * kc * 4);
    memset(ap_u, 0, (size_t)PANEL_ROWS * N * 4);

    double bu = 1e18, bb = 1e18;
    double sink = 0;
    for (int r = 0; r < reps; r++) {
        // UNBLOCKED: one pack per i-panel, the whole k axis deep
        double t0 = now_ms();
        for (long i0 = 0; i0 < N; i0 += PANEL_ROWS) pack(a, ap_u, N, i0, 0, N);
        double d = now_ms() - t0; if (d < bu) bu = d;
        sink += ap_u[0];

        // BLOCKED: k blocks outermost, one kc-deep pack per (k0, i0)
        t0 = now_ms();
        for (long k0 = 0; k0 < N; k0 += kc)
            for (long i0 = 0; i0 < N; i0 += PANEL_ROWS) pack(a, ap_b, N, i0, k0, kc);
        d = now_ms() - t0; if (d < bb) bb = d;
        sink += ap_b[0];
    }

    long elems = N * N;  // identical for both
    printf("N=%ld kc=%ld, best of %d, 1 thread, NO SME -- pure pack loops\n", N, kc, reps);
    printf("  %-34s %9.3f ms   %6.2f ns/elem\n", "unblocked (one pack per i-panel)",
           bu, bu * 1e6 / elems);
    printf("  %-34s %9.3f ms   %6.2f ns/elem\n", "blocked (one pack per k0,i0)",
           bb, bb * 1e6 / elems);
    printf("  both move %ld elements. blocked is %.3fx the unblocked time.\n", elems, bb / bu);
    printf("  emitter measured 29.97 -> 139.76 ms non-kernel, a 4.66x jump.\n");
    printf("  => %s\n", bb / bu > 2.0
        ? "REAL MEMORY EFFECT: the pack order is what blocking breaks, not our codegen"
        : "the pack is NOT the cause; the emitter's generated pack is the suspect");
    printf("  (sink %.1f)\n", sink);
    return 0;
}
