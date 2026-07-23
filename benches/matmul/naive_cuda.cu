// Naive global-memory GEMM: one thread per output cell (the same algorithm a
// capture-less Flow matmul would need to avoid per-op launches — the control
// for "the algorithm without execution-strategy overhead").
// Width: argv[3] "f32" (default) or "f64" — the f64 row is the like-for-like
// comparator for flow's f64 kernel (S24 close review, Sapir: the baseline was
// f32-only, so flow-f64 had only chapel-gpu to compare against).
// Build: nvcc -O3 -arch=sm_89 naive_cuda.cu -o naive_cuda
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cuda_runtime.h>

template <typename T>
__global__ void gemm_naive(const T* a, const T* b, T* c, int n) {
    unsigned long long t = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
    unsigned long long nn = (unsigned long long)n * (unsigned long long)n;
    if (t < nn) {
        int i = (int)(t / n), j = (int)(t % n);
        T acc = (T)0;
        for (int k = 0; k < n; k++) acc += a[(size_t)i * n + k] * b[(size_t)k * n + j];
        c[t] = acc;
    }
}

template <typename T>
static void run(int n, int iters, const char* tag) {
    size_t nn = (size_t)n * n, bytes = nn * sizeof(T);
    T *ha = (T*)malloc(bytes), *hb = (T*)malloc(bytes), *hc = (T*)malloc(bytes);
    for (size_t i = 0; i < nn; i++) {
        ha[i] = (T)((int)((i * 7 + 13) % 101) - 50);
        hb[i] = (T)((int)((i * 7 + 57) % 101) - 50);
    }
    T *da, *db, *dc;
    cudaMalloc(&da, bytes); cudaMalloc(&db, bytes); cudaMalloc(&dc, bytes);
    cudaMemcpy(da, ha, bytes, cudaMemcpyHostToDevice);
    cudaMemcpy(db, hb, bytes, cudaMemcpyHostToDevice);
    unsigned grid = (unsigned)((nn + 255ULL) / 256ULL);
    for (int i = 0; i < 5; i++) gemm_naive<T><<<grid, 256>>>(da, db, dc, n); // warmup
    cudaDeviceSynchronize();
    cudaEvent_t s, e; cudaEventCreate(&s); cudaEventCreate(&e);
    cudaEventRecord(s);
    for (int i = 0; i < iters; i++) gemm_naive<T><<<grid, 256>>>(da, db, dc, n);
    cudaEventRecord(e); cudaEventSynchronize(e);
    float ms = 0; cudaEventElapsedTime(&ms, s, e); ms /= iters;
    cudaMemcpy(hc, dc, bytes, cudaMemcpyDeviceToHost);
    printf("%s N=%d %.4f ms %.1f GFLOP/s c0=%.1f clast=%.1f\n",
           tag, n, ms, 2.0 * n * n * n / (ms * 1e6), (double)hc[0], (double)hc[nn - 1]);
}

int main(int argc, char** argv) {
    int n = argc > 1 ? atoi(argv[1]) : 512;
    int iters = argc > 2 ? atoi(argv[2]) : 50;
    const char* width = argc > 3 ? argv[3] : "f32";
    if (strcmp(width, "f64") == 0) run<double>(n, iters, "naive-cuda-f64");
    else run<float>(n, iters, "naive-cuda");
    return 0;
}
