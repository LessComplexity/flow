// Multithread naive triple-loop GEMM in C++ — the threaded host-language
// baseline (S26b, Sapir's framing directive: par-on-par comparisons only).
// Mirrors cpp_naive.cpp exactly — same i/j/k per-cell math, same init, same
// min-of-iters timing style — but the outer (row) loop is partitioned across
// std::thread workers. Row partitioning preserves every cell's k-order, so
// outputs are byte-equal to the naive build at any thread count.
//   Width: quota-aware like mapal-rt — $THREADS override, else cgroup v2
//   cpu.max, else v1 cfs_quota/period (div-ceil), capped by
//   hardware_concurrency; the box shows 128 threads at a ~61.4-core quota and
//   a naive hardware_concurrency read would oversubscribe 2x.
//   Timing: workers spawn ONCE; each keeps its own min-of-iters over its
//   slice; the reported ms is the max over workers (uniform slices, so this
//   is the min full-iteration time; one-time spawn cost never enters a min).
// Build: clang++ -O3 -march=native cpp_mt.cpp -o cpp_mt -lpthread   (the naive recipe)
// Usage: cpp_mt [N] [ITERS] [f32|f64]   (defaults: 512 50 f32)
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <chrono>
#include <thread>
#include <vector>
#include <algorithm>

static int thread_width() {
    if (const char* e = getenv("THREADS")) {
        int v = atoi(e);
        if (v >= 1) return v;
    }
    long quota = -1;
    if (FILE* f = fopen("/sys/fs/cgroup/cpu.max", "r")) {  // cgroup v2
        char q[64] = {0};
        long p = 0;
        if (fscanf(f, "%63s %ld", q, &p) == 2 && strcmp(q, "max") != 0 && p > 0)
            quota = (atol(q) + p - 1) / p;  // div-ceil, mapal-rt's rule
        fclose(f);
    }
    if (quota < 0) {  // cgroup v1
        long qv = -1, pv = -1;
        if (FILE* f = fopen("/sys/fs/cgroup/cpu/cpu.cfs_quota_us", "r")) {
            if (fscanf(f, "%ld", &qv) != 1) qv = -1;
            fclose(f);
        }
        if (FILE* f = fopen("/sys/fs/cgroup/cpu/cpu.cfs_period_us", "r")) {
            if (fscanf(f, "%ld", &pv) != 1) pv = -1;
            fclose(f);
        }
        if (qv > 0 && pv > 0) quota = (qv + pv - 1) / pv;
    }
    unsigned w = std::thread::hardware_concurrency();
    if (w == 0) w = 1;
    if (quota > 0 && (unsigned long)quota < w) w = (unsigned)quota;
    return (int)std::max(w, 1u);
}

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
    int nth = std::min(thread_width(), n);
    std::vector<double> best(nth, 1e300);
    {
        std::vector<std::thread> pool;
        pool.reserve(nth);
        for (int t = 0; t < nth; t++) {
            int r0 = (int)((long long)t * n / nth), r1 = (int)((long long)(t + 1) * n / nth);
            pool.emplace_back([&, t, r0, r1]() {
                double my_best = 1e300;
                for (int it = 0; it < iters; it++) {
                    auto t0 = std::chrono::steady_clock::now();
                    for (int i = r0; i < r1; i++) {
                        for (int j = 0; j < n; j++) {
                            T acc = 0;
                            for (int k = 0; k < n; k++) acc += a[(size_t)i * n + k] * b[(size_t)k * n + j];
                            c[(size_t)i * n + j] = acc;
                        }
                    }
                    double dt = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
                    if (dt < my_best) my_best = dt;
                }
                best[t] = my_best;
            });
        }
        for (auto& th : pool) th.join();
    }
    double ms = *std::max_element(best.begin(), best.end()) * 1e3;
    printf("%s N=%d %.4f ms %.1f GFLOP/s c0=%.1f clast=%.1f T=%d\n",
           label, n, ms, 2.0 * n * n * n / (ms * 1e6), (double)c[0], (double)c[nn - 1], nth);
    free(a);
    free(b);
    free(c);
}

int main(int argc, char** argv) {
    int n = argc > 1 ? atoi(argv[1]) : 512;
    int iters = argc > 2 ? atoi(argv[2]) : 50;
    const char* width = argc > 3 ? argv[3] : "f32";
    if (strcmp(width, "f64") == 0) {
        run<double>(n, iters, "cpp-mt-f64");
    } else {
        run<float>(n, iters, "cpp-mt-f32");
    }
    return 0;
}
