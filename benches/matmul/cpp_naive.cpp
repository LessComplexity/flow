// Naive triple-loop GEMM in C++ — the host-language baseline in both widths
// (same i/j/k order as the Mapal program's cell fn and rust_naive.rs; f32 for
// like-for-like against rust_naive/numpy, f64 against the f64 capture legs).
// Build: clang++ -O3 -march=native cpp_naive.cpp -o cpp_naive
// Usage: cpp_naive [N] [ITERS] [f32|f64]   (defaults: 512 50 f32)
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>

template <typename T>
static void run(int n, int iters, const char* label) {
    size_t nn = (size_t)n * n;
    T* a = (T*)malloc(nn * sizeof(T));
    T* b = (T*)malloc(nn * sizeof(T));
    T* c = (T*)malloc(nn * sizeof(T));
    for (size_t i = 0; i < nn; i++) {
        a[i] = (T)((int)((i * 7 + 13) % 101) - 50);
        b[i] = (T)((int)((i * 7 + 57) % 101) - 50);
    }
    double best = 1e300;
    for (int it = 0; it < iters; it++) {
        auto t0 = std::chrono::steady_clock::now();
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                T acc = 0;
                for (int k = 0; k < n; k++) acc += a[(size_t)i * n + k] * b[(size_t)k * n + j];
                c[(size_t)i * n + j] = acc;
            }
        }
        double dt = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
        if (dt < best) best = dt;
    }
    double ms = best * 1e3;
    printf("%s N=%d %.4f ms %.1f GFLOP/s c0=%.1f clast=%.1f\n",
           label, n, ms, 2.0 * n * n * n / (ms * 1e6), (double)c[0], (double)c[nn - 1]);
    free(a);
    free(b);
    free(c);
}

int main(int argc, char** argv) {
    int n = argc > 1 ? atoi(argv[1]) : 512;
    int iters = argc > 2 ? atoi(argv[2]) : 50;
    const char* width = argc > 3 ? argv[3] : "f32";
    if (strcmp(width, "f64") == 0) {
        run<double>(n, iters, "cpp-naive-f64");
    } else {
        run<float>(n, iters, "cpp-naive-f32");
    }
    return 0;
}
