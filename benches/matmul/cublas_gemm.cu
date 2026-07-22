// cuBLAS SGEMM baseline — the optimized ceiling. Row-major C = A·B computed
// via the column-major identity Cᵀ = Bᵀ·Aᵀ (no data movement; same FLOPs).
// Build: nvcc -O3 -arch=sm_89 cublas_gemm.cu -lcublas -o cublas_gemm
#include <cstdio>
#include <cstdlib>
#include <cuda_runtime.h>
#include <cublas_v2.h>

int main(int argc, char** argv) {
    int n = argc > 1 ? atoi(argv[1]) : 512;
    int iters = argc > 2 ? atoi(argv[2]) : 50;
    size_t nn = (size_t)n * n;
    float *ha = (float*)malloc(nn * 4), *hb = (float*)malloc(nn * 4), *hc = (float*)malloc(nn * 4);
    for (size_t i = 0; i < nn; i++) {
        ha[i] = (float)((int)((i * 7 + 13) % 101) - 50);
        hb[i] = (float)((int)((i * 7 + 57) % 101) - 50);
    }
    float *da, *db, *dc;
    cudaMalloc(&da, nn * 4); cudaMalloc(&db, nn * 4); cudaMalloc(&dc, nn * 4);
    cudaMemcpy(da, ha, nn * 4, cudaMemcpyHostToDevice);
    cudaMemcpy(db, hb, nn * 4, cudaMemcpyHostToDevice);
    cublasHandle_t h; cublasCreate(&h);
    cublasSetMathMode(h, CUBLAS_DEFAULT_MATH); // no TF32 — honest fp32
    float alpha = 1.f, beta = 0.f;
    // C = A·B (row-major) ≡ cublas(B, A) under the transpose identity.
    for (int i = 0; i < 5; i++)
        cublasSgemm(h, CUBLAS_OP_N, CUBLAS_OP_N, n, n, n, &alpha, db, n, da, n, &beta, dc, n);
    cudaDeviceSynchronize();
    cudaEvent_t s, e; cudaEventCreate(&s); cudaEventCreate(&e);
    cudaEventRecord(s);
    for (int i = 0; i < iters; i++)
        cublasSgemm(h, CUBLAS_OP_N, CUBLAS_OP_N, n, n, n, &alpha, db, n, da, n, &beta, dc, n);
    cudaEventRecord(e); cudaEventSynchronize(e);
    float ms = 0; cudaEventElapsedTime(&ms, s, e); ms /= iters;
    cudaMemcpy(hc, dc, nn * 4, cudaMemcpyDeviceToHost);
    printf("cublas N=%d %.4f ms %.1f GFLOP/s c0=%.1f clast=%.1f\n",
           n, ms, 2.0 * n * n * n / (ms * 1e6), hc[0], hc[nn - 1]);
    return 0;
}
