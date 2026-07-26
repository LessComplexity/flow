// CPU baselines for the fir/conv2d Mapal shapes, size-parameterized (S29 scale-up).
// Build: clang++ -O3 -march=native -ffp-contract=fast -std=c++17 shapes_baseline.cpp -o shapes_baseline -pthread
#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <thread>
#include <vector>

static unsigned thread_width() {
    if (const char* value = std::getenv("THREADS")) {
        int threads = std::atoi(value);
        if (threads >= 1) return static_cast<unsigned>(threads);
    }

    long quota = -1;
    if (FILE* file = std::fopen("/sys/fs/cgroup/cpu.max", "r")) {
        char value[64] = {};
        long period = 0;
        if (std::fscanf(file, "%63s %ld", value, &period) == 2 &&
            std::strcmp(value, "max") != 0 && period > 0)
            quota = (std::atol(value) + period - 1) / period;
        std::fclose(file);
    }
    if (quota < 0) {
        long value = -1, period = -1;
        if (FILE* file = std::fopen("/sys/fs/cgroup/cpu/cpu.cfs_quota_us", "r")) {
            if (std::fscanf(file, "%ld", &value) != 1) value = -1;
            std::fclose(file);
        }
        if (FILE* file = std::fopen("/sys/fs/cgroup/cpu/cpu.cfs_period_us", "r")) {
            if (std::fscanf(file, "%ld", &period) != 1) period = -1;
            std::fclose(file);
        }
        if (value > 0 && period > 0) quota = (value + period - 1) / period;
    }

    unsigned threads = std::thread::hardware_concurrency();
    if (threads == 0) threads = 1;
    if (quota > 0) threads = std::min(threads, static_cast<unsigned>(quota));
    return threads;
}

static void fir_range(const std::vector<float>& x, const std::vector<float>& w,
                      std::vector<float>& y, size_t begin, size_t end) {
    for (size_t t = begin; t < end; ++t) {
        float acc = 0.0f;
        for (size_t k = 0; k < w.size(); ++k) acc += w[k] * x[t + k];
        y[t] = acc;
    }
}

static void conv_range(const std::vector<float>& img, const std::vector<float>& w,
                       std::vector<float>& out, size_t side, size_t row_begin, size_t row_end) {
    const size_t stride = side + 2;
    for (size_t i = row_begin; i < row_end; ++i) {
        for (size_t j = 0; j < side; ++j) {
            float acc = 0.0f;
            for (size_t k = 0; k < 9; ++k)
                acc += w[k] * img[(i + k / 3) * stride + j + k % 3];
            out[i * side + j] = acc;
        }
    }
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

static void run_fir(bool multithreaded, int iters, size_t n) {
    std::vector<float> x(n + 63), w(64), y(n);
    for (size_t t = 0; t < x.size(); ++t)
        x[t] = static_cast<float>(static_cast<int>((t * 7 + 13) % 101) - 50);
    for (size_t k = 0; k < w.size(); ++k)
        w[k] = static_cast<float>(static_cast<int>((k * 5 + 3) % 31) - 15);

    const unsigned threads = std::min<unsigned>(thread_width(), y.size());
    run_iters(iters, [&] {
        if (!multithreaded) {
            fir_range(x, w, y, 0, y.size());
            return;
        }
        std::vector<std::thread> workers;
        workers.reserve(threads);
        for (unsigned lane = 0; lane < threads; ++lane) {
            const size_t begin = lane * y.size() / threads;
            const size_t end = (lane + 1) * y.size() / threads;
            workers.emplace_back(fir_range, std::cref(x), std::cref(w), std::ref(y), begin, end);
        }
        for (auto& worker : workers) worker.join();
    });
    std::printf("%.9g\n%.9g\n", static_cast<double>(y.front()), static_cast<double>(y.back()));
}

static void run_conv(bool multithreaded, int iters, size_t side) {
    const size_t stride = side + 2;
    std::vector<float> img(stride * stride), w(9), out(side * side);
    for (size_t t = 0; t < img.size(); ++t)
        img[t] = static_cast<float>(static_cast<int>((t * 7 + 13) % 101) - 50);
    for (size_t k = 0; k < w.size(); ++k)
        w[k] = static_cast<float>(static_cast<int>((k * 5 + 3) % 31) - 15);

    const unsigned threads = std::min<unsigned>(thread_width(), side);
    run_iters(iters, [&] {
        if (!multithreaded) {
            conv_range(img, w, out, side, 0, side);
            return;
        }
        std::vector<std::thread> workers;
        workers.reserve(threads);
        for (unsigned lane = 0; lane < threads; ++lane) {
            const size_t begin = lane * side / threads;
            const size_t end = (lane + 1) * side / threads;
            workers.emplace_back(conv_range, std::cref(img), std::cref(w), std::ref(out), side, begin, end);
        }
        for (auto& worker : workers) worker.join();
    });
    std::printf("%.9g\n%.9g\n", static_cast<double>(out.front()), static_cast<double>(out.back()));
}

int main(int argc, char** argv) {
    if (argc < 4 || argc > 5 || (std::strcmp(argv[1], "fir") != 0 && std::strcmp(argv[1], "conv2d") != 0) ||
        (std::strcmp(argv[2], "1t") != 0 && std::strcmp(argv[2], "mt") != 0)) {
        std::fprintf(stderr, "usage: %s <fir|conv2d> <1t|mt> <iters> [n|side]\n", argv[0]);
        return 2;
    }
    const int iters = std::atoi(argv[3]);
    if (iters < 1) {
        std::fprintf(stderr, "iters must be >= 1\n");
        return 2;
    }
    const bool multithreaded = std::strcmp(argv[2], "mt") == 0;
    const bool fir = std::strcmp(argv[1], "fir") == 0;
    const size_t default_n = fir ? 65536 : 512;
    const size_t n = argc == 5 ? static_cast<size_t>(std::atoll(argv[4])) : default_n;
    if (fir)
        run_fir(multithreaded, iters, n);
    else
        run_conv(multithreaded, iters, n);
}
