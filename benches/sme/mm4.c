// Same GEMM as mmN.c but accumulating into FOUR ZA tiles in a 2x2 arrangement:
// a 32x32 output block per panel instead of 16x16.
// 4 independent fmopa chains, and 4 loads feed 4 fmopa (1 load/fmopa) instead
// of 2 loads feeding 1 (2 loads/fmopa).
#include <arm_sme.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

__arm_new("za")
static void mm_panel4(const float *ap0, const float *ap1, const float *b,
                      float *c, int N, int i0, int j0) __arm_streaming {
    svbool_t pg = svptrue_b32();
    svzero_za();
    for (int k = 0; k < N; k++) {
        svfloat32_t zn0 = svld1_f32(pg, &ap0[k * 16]);
        svfloat32_t zn1 = svld1_f32(pg, &ap1[k * 16]);
        svfloat32_t zm0 = svld1_f32(pg, &b[k * N + j0]);
        svfloat32_t zm1 = svld1_f32(pg, &b[k * N + j0 + 16]);
        svmopa_za32_f32_m(0, pg, pg, zn0, zm0);
        svmopa_za32_f32_m(1, pg, pg, zn0, zm1);
        svmopa_za32_f32_m(2, pg, pg, zn1, zm0);
        svmopa_za32_f32_m(3, pg, pg, zn1, zm1);
    }
    for (int i = 0; i < 16; i++) {
        svst1_f32(pg, &c[(i0+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 0, i));
        svst1_f32(pg, &c[(i0+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 1, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0],      svread_hor_za32_f32_m(svundef_f32(), pg, 2, i));
        svst1_f32(pg, &c[(i0+16+i)*N + j0 + 16], svread_hor_za32_f32_m(svundef_f32(), pg, 3, i));
    }
}
static double now_ms(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec*1e3+t.tv_nsec/1e6;}
int main(int argc,char**argv){
    int N=argc>1?atoi(argv[1]):1024, REPS=argc>2?atoi(argv[2]):7;
    float *a=aligned_alloc(64,(size_t)N*N*4),*b=aligned_alloc(64,(size_t)N*N*4);
    float *c=aligned_alloc(64,(size_t)N*N*4);
    float *ap0=aligned_alloc(64,(size_t)N*16*4),*ap1=aligned_alloc(64,(size_t)N*16*4);
    for(long t=0;t<(long)N*N;t++){a[t]=(float)((t*7)%13)*0.01f+1.0f;b[t]=(float)((t*5)%17)*0.01f+1.0f;}
    double best=1e18;
    for(int r=0;r<REPS;r++){
        double t0=now_ms();
        for(int i0=0;i0<N;i0+=32){
            for(int k=0;k<N;k++)for(int i=0;i<16;i++){ap0[k*16+i]=a[(i0+i)*N+k];ap1[k*16+i]=a[(i0+16+i)*N+k];}
            for(int j0=0;j0<N;j0+=32) mm_panel4(ap0,ap1,b,c,N,i0,j0);
        }
        double dt=now_ms()-t0; if(dt<best)best=dt;
    }
    printf("N=%-5d SME 2x2 tiles 1t: %8.4f ms  %7.1f GFLOP/s  c[0]=%.4f\n",
           N,best,2.0*N*N*N/(best*1e6),c[0]);
    return 0;
}
