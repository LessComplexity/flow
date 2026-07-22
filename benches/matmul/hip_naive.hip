// Naive global-memory GEMM in HIP — naive_cuda.cu ported 1:1 to the HIP API.
// On NVIDIA hardware (HIP_PLATFORM=nvidia) hipcc wraps nvcc and every hip*
// call macro-expands to its cuda* twin: this leg measures the portability
// layer's tax over naive-cuda (expected ~zero; the measurement proves it).
// Build (NVIDIA): HIP_PLATFORM=nvidia hipcc -O3 -arch=sm_89 hip_naive.hip -o hip_naive
#include <cstdio>
#include <cstdlib>
#include <hip/hip_runtime.h>

__global__ void gemm_naive(const float* a, const float* b, float* c, int n) {
    unsigned long long t = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long nn = (unsigned long long)n * (unsigned long long)n;
    if (t < nn) {
        int i = (int)(t / n), j = (int)(t % n);
        float acc = 0.f;
        for (int k = 0; k < n; k++) acc += a[(size_t)i * n + k] * b[(size_t)k * n + j];
        c[t] = acc;
    }
}

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
    hipMalloc(&da, nn * 4); hipMalloc(&db, nn * 4); hipMalloc(&dc, nn * 4);
    hipMemcpy(da, ha, nn * 4, hipMemcpyHostToDevice);
    hipMemcpy(db, hb, nn * 4, hipMemcpyHostToDevice);
    unsigned grid = (unsigned)((nn + 255ULL) / 256ULL);
    for (int i = 0; i < 5; i++) gemm_naive<<<grid, 256>>>(da, db, dc, n); // warmup
    hipDeviceSynchronize();
    hipEvent_t s, e; hipEventCreate(&s); hipEventCreate(&e);
    hipEventRecord(s);
    for (int i = 0; i < iters; i++) gemm_naive<<<grid, 256>>>(da, db, dc, n);
    hipEventRecord(e); hipEventSynchronize(e);
    float ms = 0; hipEventElapsedTime(&ms, s, e); ms /= iters;
    hipMemcpy(hc, dc, nn * 4, hipMemcpyDeviceToHost);
    printf("hip-naive N=%d %.4f ms %.1f GFLOP/s c0=%.1f clast=%.1f\n",
           n, ms, 2.0 * n * n * n / (ms * 1e6), hc[0], hc[nn - 1]);
    return 0;
}
