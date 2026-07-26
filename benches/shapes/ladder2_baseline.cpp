// CPU baselines for the shape-ladder-v2 classes: saxpy, reduce, transpose, gather.
// Build: clang++ -std=c++17 -O3 -march=native -ffp-contract=fast ladder2_baseline.cpp -o ladder2_baseline -pthread
//
// Every leg mirrors the Mapal shape exactly: the same procedural data generation
// (outside the timer), the same kernel, the same two printed values, and the same
// `iter ms=` line. Generation is deliberately outside the measured region on both
// sides — the S28 gen-boundary finding.
#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

static unsigned thread_width() {
    if (const char* value = std::getenv("THREADS")) {
        int threads = std::atoi(value);
        if (threads >= 1) return static_cast<unsigned>(threads);
    }
    unsigned threads = std::thread::hardware_concurrency();
    return threads == 0 ? 1u : threads;
}

// Split [0, n) across `width` threads and join. One helper for every leg so the
// threading overhead is identical across shapes.
template <typename Body>
static void parallel_for(bool multithreaded, size_t n, Body body) {
    const unsigned width = multithreaded ? thread_width() : 1u;
    if (width <= 1) { body(0, n); return; }
    std::vector<std::thread> pool;
    pool.reserve(width);
    const size_t chunk = (n + width - 1) / width;
    for (unsigned t = 0; t < width; ++t) {
        const size_t lo = std::min(n, static_cast<size_t>(t) * chunk);
        const size_t hi = std::min(n, lo + chunk);
        if (lo < hi) pool.emplace_back([=] { body(lo, hi); });
    }
    for (auto& thread : pool) thread.join();
}

template <typename Work>
static void run_iters(int iters, Work work) {
    for (int iter = 0; iter < iters; ++iter) {
        const auto start = std::chrono::steady_clock::now();
        work();
        const double ms =
            std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start)
                .count();
        std::printf("iter ms=%.6f\n", ms);
    }
}

// The Mapal shapes print f32 through Rust's Display. Match the two-value probe
// with %g, which agrees on these exact-in-binary values.
static void probe(float a, float b) { std::printf("%g\n%g\n", a, b); }

static std::vector<float> gen(size_t n, long mul, long add, long modulus, long sub) {
    std::vector<float> out(n);
    for (size_t i = 0; i < n; ++i)
        out[i] = static_cast<float>(static_cast<long>(i) * mul % modulus + add % modulus - sub);
    return out;
}

// y[i] = 2.5*x[i] + y0[i] — streaming, bandwidth-bound.
static void run_saxpy(bool mt, int iters, size_t n) {
    std::vector<float> x(n), y0(n), y(n);
    for (size_t i = 0; i < n; ++i) {
        x[i] = static_cast<float>((static_cast<long>(i) * 7 + 13) % 101 - 50);
        y0[i] = static_cast<float>((static_cast<long>(i) * 5 + 3) % 31 - 15);
    }
    run_iters(iters, [&] {
        parallel_for(mt, n, [&](size_t lo, size_t hi) {
            for (size_t i = lo; i < hi; ++i) y[i] = 2.5f * x[i] + y0[i];
        });
    });
    probe(y[0], y[n - 1]);
}

// total = Σ x — reduction, no output array.
static void run_reduce(bool mt, int iters, size_t n) {
    std::vector<float> x(n);
    for (size_t i = 0; i < n; ++i)
        x[i] = static_cast<float>((static_cast<long>(i) * 7 + 13) % 101 - 50);
    float total = 0.0f;
    run_iters(iters, [&] {
        const unsigned width = mt ? thread_width() : 1u;
        std::vector<float> partial(width, 0.0f);
        unsigned slot = 0;
        std::vector<std::thread> pool;
        const size_t chunk = (n + width - 1) / width;
        for (unsigned t = 0; t < width; ++t) {
            const size_t lo = std::min(n, static_cast<size_t>(t) * chunk);
            const size_t hi = std::min(n, lo + chunk);
            if (lo >= hi) continue;
            float* out = &partial[slot++];
            if (width == 1) {
                float acc = 0.0f;
                for (size_t i = lo; i < hi; ++i) acc += x[i];
                *out = acc;
            } else {
                pool.emplace_back([=] {
                    float acc = 0.0f;
                    for (size_t i = lo; i < hi; ++i) acc += x[i];
                    *out = acc;
                });
            }
        }
        for (auto& thread : pool) thread.join();
        total = 0.0f;
        for (float value : partial) total += value;
    });
    std::printf("%g\n", total);
}

// b[t] = a[(t % side)*side + t / side] — pure permutation, zero arithmetic.
static void run_transpose(bool mt, int iters, size_t side) {
    const size_t n = side * side;
    std::vector<float> a(n), b(n);
    for (size_t i = 0; i < n; ++i)
        a[i] = static_cast<float>((static_cast<long>(i) * 7 + 13) % 101 - 50);
    run_iters(iters, [&] {
        parallel_for(mt, n, [&](size_t lo, size_t hi) {
            for (size_t t = lo; t < hi; ++t) b[t] = a[(t % side) * side + t / side];
        });
    });
    probe(b[0], b[n - 1]);
}

// y[i] = x[idx[i]] — data-dependent reads over a stride-1021 permutation.
static void run_gather(bool mt, int iters, size_t n) {
    std::vector<float> x(n), y(n);
    std::vector<int> idx(n);
    for (size_t i = 0; i < n; ++i) {
        x[i] = static_cast<float>((static_cast<long>(i) * 7 + 13) % 101 - 50);
        idx[i] = static_cast<int>((static_cast<long>(i) * 1021 + 12347) % static_cast<long>(n));
    }
    run_iters(iters, [&] {
        parallel_for(mt, n, [&](size_t lo, size_t hi) {
            for (size_t i = lo; i < hi; ++i) y[i] = x[idx[i]];
        });
    });
    probe(y[0], y[n - 1]);
}

int main(int argc, char** argv) {
    if (argc < 4) {
        std::fprintf(stderr,
                     "usage: %s <saxpy|reduce|transpose|gather> <1t|mt> <iters> [size]\n", argv[0]);
        return 2;
    }
    const std::string shape = argv[1];
    const bool mt = std::strcmp(argv[2], "mt") == 0;
    const int iters = std::atoi(argv[3]);
    const size_t size = argc > 4 ? std::strtoull(argv[4], nullptr, 10) : 1048576;

    if (shape == "saxpy") run_saxpy(mt, iters, size);
    else if (shape == "reduce") run_reduce(mt, iters, size);
    else if (shape == "transpose") run_transpose(mt, iters, size);
    else if (shape == "gather") run_gather(mt, iters, size);
    else { std::fprintf(stderr, "unknown shape %s\n", shape.c_str()); return 2; }
    return 0;
}
