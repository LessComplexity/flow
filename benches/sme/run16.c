#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// C[16x16] = A[16xK] * B[Kx16], accumulated in a ZA tile via fmopa.
// A is passed already packed as K columns of 16 (a[k*16 + i]) so each
// fmopa operand is a contiguous 16-float vector: exactly the rank-1 update
// the NEON micro-kernel already performs, 16x16 at a time instead of 4x16.
__arm_new("za")
static void mm16(const float *a_packed, const float *b, float *c, int K)
    __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < K; k++) {
        svfloat32_t zn = svld1_f32(pg, &a_packed[k * 16]);
        svfloat32_t zm = svld1_f32(pg, &b[k * 16]);
        svmopa_za32_f32_m(0, pg, pg, zn, zm);
    }
    for (int i = 0; i < 16; i++) {
        svfloat32_t row = svread_hor_za32_f32_m(svundef_f32(), pg, 0, i);
        svst1_f32(pg, &c[i * 16], row);
    }
}

int main(void) {
    const int K = 32;
    float *a = malloc(16 * K * sizeof(float));   // packed: a[k*16+i] = A[i][k]
    float *b = malloc(K * 16 * sizeof(float));
    float *c = calloc(16 * 16, sizeof(float));
    float *ref = calloc(16 * 16, sizeof(float));
    for (int k = 0; k < K; k++)
        for (int i = 0; i < 16; i++) a[k * 16 + i] = (float)((i * 7 + k * 3) % 13) - 6.0f;
    for (int k = 0; k < K; k++)
        for (int j = 0; j < 16; j++) b[k * 16 + j] = (float)((j * 5 + k * 11) % 17) - 8.0f;
    for (int i = 0; i < 16; i++)
        for (int j = 0; j < 16; j++) {
            float s = 0.0f;
            for (int k = 0; k < K; k++) s += a[k * 16 + i] * b[k * 16 + j];
            ref[i * 16 + j] = s;
        }
    mm16(a, b, c, K);
    int bad = 0;
    for (int t = 0; t < 256; t++) if (c[t] != ref[t]) bad++;
    printf("SME 16x16 matmul: %s  (%d/256 mismatched)\n", bad ? "MISMATCH" : "EXACT", bad);
    printf("c[0][0]=%.1f ref=%.1f   c[15][15]=%.1f ref=%.1f\n",
           c[0], ref[0], c[255], ref[255]);
    return bad != 0;
}
