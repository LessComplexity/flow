// What does one SME panel CALL cost, before it does any work?
//
// WHY. S42's KC bisection found that blocking makes the kernel 1.7x faster
// (931 -> 1598 GFLOP/s at N=4096) and then loses it all to non-kernel cost,
// which goes 29.97 -> 139.76 ms. Blocking makes 8x more panel calls (131072 vs
// 16384), and the emitted kernel differs from the hand-written probe's in
// exactly two ways at the ABI level:
//
//   emitted   `aarch64_pstate_sm_body`  == C's __arm_locally_streaming:
//             the CALLEE enters and leaves streaming mode itself, and because
//             changing streaming mode clobbers d8-d15 it must spill all eight.
//             Verified in the asm: 4x stp on entry, 4x ldp on exit, plus
//             `smstart za` AND `smstart sm`, and two smstops.
//
//   probe     `__arm_streaming`: the CALLER is already streaming, so the callee
//             transitions nothing and spills nothing. Verified: no stp/ldp, only
//             `smstart za`.
//
// S41 chose `_body` deliberately, and recorded why (benches/sme/README.md):
// it keeps the kernel self-contained so "no other emitted function needs to know
// streaming mode exists", which is what makes SME a leaf swap instead of an ABI
// change. This probe prices that choice per call, so the tradeoff is a number
// rather than an argument.
//
// The functions do NO arithmetic -- just enough that they cannot be optimised
// away. What is being timed is entry and exit, nothing else.
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// What the EMITTER emits: the callee owns the streaming-mode transition.
// __arm_locally_streaming is the C spelling of `aarch64_pstate_sm_body`.
__arm_new("za") __arm_locally_streaming
static void callee_transitions(float *sink) {
    svbool_t pg = svptrue_b32();
    svzero_za();
    svst1_f32(pg, sink, svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0));
}

// What the hand-written probe uses: the CALLER is already in streaming mode.
__arm_new("za")
static void caller_transitions(float *sink) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    svst1_f32(pg, sink, svread_hor_za32_f32_m(svundef_f32(), pg, 0, 0));
}

// Driving `caller_transitions` from a streaming caller is what lets the
// transition be paid once per BATCH rather than once per call -- the thing the
// emitter cannot currently express, because its nest is not streaming-aware.
__arm_locally_streaming
static void batch_of_calls(float *sink, long n) {
    for (long i = 0; i < n; i++) caller_transitions(sink);
}

int main(int argc, char **argv) {
    long calls = argc > 1 ? atol(argv[1]) : 131072;  // the N=4096 blocked count
    int reps = argc > 2 ? atoi(argv[2]) : 5;
    float *sink = aligned_alloc(64, 4096);

    // warm the clock (measurement rule 14)
    double w0 = now_ms();
    while (now_ms() - w0 < 300.0) callee_transitions(sink);

    double a = 1e18, b = 1e18;
    for (int r = 0; r < reps; r++) {
        double t0 = now_ms();
        for (long i = 0; i < calls; i++) callee_transitions(sink);
        double d = now_ms() - t0; if (d < a) a = d;

        t0 = now_ms();
        batch_of_calls(sink, calls);
        d = now_ms() - t0; if (d < b) b = d;
    }

    printf("%ld calls, best of %d, N=4096's blocked panel-call count\n", calls, reps);
    printf("  %-42s %9.3f ms  %8.1f ns/call\n",
           "callee transitions (what we emit)", a, a * 1e6 / calls);
    printf("  %-42s %9.3f ms  %8.1f ns/call\n",
           "caller already streaming (the probe)", b, b * 1e6 / calls);
    printf("  the emitted form costs %.2fx more per call, %+.1f ms over %ld calls\n",
           a / b, a - b, calls);
    printf("  (for scale: the blocked run's non-kernel cost is 139.76 ms and the\n"
           "   unblocked one's is 29.97 ms, a 109.8 ms gap over 8x the calls)\n");
    printf("  sink %.1f\n", (double)sink[0]);
    return 0;
}
