#include <stdio.h>
#include <stdlib.h>
#include <math.h>
void mapal_sme_panel(const float *ap, const float *b, float *c, long N, long K);
int main(void) {
    const long N = 64, K = 48;                 // one 16x16 panel of a bigger C
    float *A = malloc(16*K*sizeof(float));     // logical A[16][K]
    float *B = malloc(K*N*sizeof(float));
    float *C = calloc(16*N, sizeof(float));
    float *ap = malloc(16*K*sizeof(float));
    for (long i=0;i<16;i++) for (long k=0;k<K;k++) A[i*K+k] = 1.0f + ((i*7+k*3)%13)*1e-3f;
    for (long k=0;k<K;k++) for (long j=0;j<N;j++) B[k*N+j] = 1.0f + ((j*5+k*11)%17)*1e-3f;
    for (long k=0;k<K;k++) for (long i=0;i<16;i++) ap[k*16+i] = A[i*K+k];   // pack
    mapal_sme_panel(ap, B, C, N, K);
    int bad=0; double worst=0;
    for (long i=0;i<16;i++) for (long j=0;j<16;j++) {
        float s=0.0f; for (long k=0;k<K;k++) s = fmaf(A[i*K+k], B[k*N+j], s);
        double d = fabs((double)C[i*N+j] - (double)s);
        if (d > worst) worst = d;
        if (C[i*N+j] != s) bad++;
    }
    // columns past the 16-wide panel must be untouched
    int spill=0; for (long i=0;i<16;i++) for (long j=16;j<N;j++) if (C[i*N+j]!=0.0f) spill++;
    printf("SME panel vs fused reference: %d/256 differ (max abs %.3g)\n", bad, worst);
    printf("wrote outside the 16-wide panel: %d cells\n", spill);
    printf("C[0][0]=%.6f  C[15][15]=%.6f\n", C[0], C[15*N+15]);
    return bad || spill;
}
