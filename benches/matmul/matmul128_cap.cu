// flow-backend-cuda emitted translation unit
// build: nvcc -std=c++17 -fmad=false -arch=sm_89 prog.cu libflow_rt.a -lpthread -ldl -lm -o prog
// DESIGN §4: -fmad=false pins oracle float parity; host -march=native/-mfma forbidden.

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstddef>
#include <cmath>
#include <cuda_runtime.h>

extern "C" {
void flow_print_i32(int32_t v, bool newline);
void flow_print_i64(int64_t v, bool newline);
void flow_print_u8(uint8_t v, bool newline);
void flow_print_bool(bool v, bool newline);
void flow_print_f32(float v, bool newline);
void flow_print_f64(double v, bool newline);
void flow_print_str(const uint8_t* ptr, size_t len, bool newline);
// Matches the Rust `-> !` (flow-rt/src/lib.rs): lets the compiler drop the
// dead fall-through after host guards. C++11 attribute, legal on an
// extern "C" declaration; host-only, so no device-pass concern.
[[noreturn]] void flow_trap(uint32_t kind);
}

// --- trap flag + CUDA error protocol (DESIGN §3) ---------------------------
static unsigned int* d_trap = nullptr;

// Assert one CUDA API return; on error print to stderr and exit 102 (the
// harness-visible infra-failure class — never an R1 data point).
static void cu_check(cudaError_t err, const char* what) {
    if (err != cudaSuccess) {
        fprintf(stderr, "flow-cuda infra error at %s: %s\n", what, cudaGetErrorString(err));
        exit(102);
    }
}

// Zero the trap flag — once per process, called at main start (H→D, §2 item 3).
static void trap_init() {
    cu_check(cudaMalloc((void**)&d_trap, sizeof(unsigned int)), "cudaMalloc(d_trap)");
    cu_check(cudaMemset(d_trap, 0, sizeof(unsigned int)), "cudaMemset(d_trap)");
}

// Trap-capable launch sites call this after EVERY such kernel launch (#14:
// a provably trap-free kernel takes no trap argument and skips this
// readback): cudaGetLastError
// (exit-102 protocol), then a host-synchronizing D→H read of the flag (the
// memcpy is the sync point); nonzero kind ⇒ flow_trap(kind - 1) on the host
// (exit 101) — the flag stores kind + 1 (0 = quiescent after trap_init's
// memset; 1 = div_zero, 2 = index_oob), decoded here to the flow-rt kinds.
[[maybe_unused]] static void trap_check_after_launch() {
    cu_check(cudaGetLastError(), "kernel launch");
    unsigned int kind = 0;
    cu_check(cudaMemcpy(&kind, d_trap, sizeof(unsigned int), cudaMemcpyDeviceToHost),
             "cudaMemcpy(d_trap)");
    if (kind != 0) {
        flow_trap(kind - 1);
    }
}

struct FlowProd_int32_t_int32_t {
    int32_t f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_int32_t_int32_t) == 8, "FlowProd_int32_t_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t {
    double* f0;
    int32_t f1;
    double* f2;
    int32_t f3;
    double f4;
    int32_t f5;
};
static_assert(sizeof(FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t) == 48, "FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_doublep_int32_t {
    double* f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_doublep_int32_t) == 16, "FlowProd_doublep_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_double_double {
    double f0;
    double f1;
};
static_assert(sizeof(FlowProd_double_double) == 16, "FlowProd_double_double: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_tp_doublep_doublep_int32_t {
    int32_t* f0;
    double* f1;
    double* f2;
    int32_t f3;
};
static_assert(sizeof(FlowProd_int32_tp_doublep_doublep_int32_t) == 32, "FlowProd_int32_tp_doublep_doublep_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_double_int32_tp {
    double f0;
    int32_t* f1;
};
static_assert(sizeof(FlowProd_double_int32_tp) == 16, "FlowProd_double_int32_tp: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_doublep_int32_t_doublep_int32_t_double_int32_tp {
    double* f0;
    int32_t f1;
    double* f2;
    int32_t f3;
    double f4;
    int32_t* f5;
};
static_assert(sizeof(FlowProd_doublep_int32_t_doublep_int32_t_double_int32_tp) == 48, "FlowProd_doublep_int32_t_doublep_int32_t_double_int32_tp: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_tp_doublep_doublep_int32_tp {
    int32_t* f0;
    double* f1;
    double* f2;
    int32_t* f3;
};
static_assert(sizeof(FlowProd_int32_tp_doublep_doublep_int32_tp) == 32, "FlowProd_int32_tp_doublep_doublep_int32_tp: abi_sizeof drift (plan-smart-arenas)");

static __host__ __device__ double fn1(int32_t in);
static __host__ __device__ double fn2(int32_t in);
static __device__ double d_fn3(FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t in);
static __device__ double d_fn4(FlowProd_int32_tp_doublep_doublep_int32_t in);

static __host__ __device__ double fn1(int32_t in) {
  int32_t o0;
  double o1;
  FlowProd_int32_t_int32_t o2;
  int32_t o3;
  FlowProd_int32_t_int32_t o4;
  int32_t o5;
  FlowProd_int32_t_int32_t o6;
  int32_t o7;
  FlowProd_int32_t_int32_t o8;
  int32_t o9;
  o0 = in;
  o2.f0 = o0;
  o2.f1 = 7;
  o4.f1 = 13;
  o6.f1 = 101;
  o8.f1 = 50;
  o3 = (int32_t)((uint32_t)o2.f0 * (uint32_t)o2.f1);
  o4.f0 = o3;
  o5 = (int32_t)((uint32_t)o4.f0 + (uint32_t)o4.f1);
  o6.f0 = o5;
  o7 = o6.f0 % o6.f1;
  o8.f0 = o7;
  o9 = (int32_t)((uint32_t)o8.f0 - (uint32_t)o8.f1);
  o1 = (double)(o9);
  return o1;
}
static __host__ __device__ double fn2(int32_t in) {
  int32_t o0;
  double o1;
  FlowProd_int32_t_int32_t o2;
  int32_t o3;
  FlowProd_int32_t_int32_t o4;
  int32_t o5;
  FlowProd_int32_t_int32_t o6;
  int32_t o7;
  FlowProd_int32_t_int32_t o8;
  int32_t o9;
  o0 = in;
  o2.f0 = o0;
  o2.f1 = 7;
  o4.f1 = 57;
  o6.f1 = 101;
  o8.f1 = 50;
  o3 = (int32_t)((uint32_t)o2.f0 * (uint32_t)o2.f1);
  o4.f0 = o3;
  o5 = (int32_t)((uint32_t)o4.f0 + (uint32_t)o4.f1);
  o6.f0 = o5;
  o7 = o6.f0 % o6.f1;
  o8.f0 = o7;
  o9 = (int32_t)((uint32_t)o8.f0 - (uint32_t)o8.f1);
  o1 = (double)(o9);
  return o1;
}
static __device__ double d_fn3(FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t in) {
  FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t o0;
  double o1;
  double* o2;
  int32_t o3;
  double* o4;
  int32_t o5;
  double o6;
  int32_t o7;
  FlowProd_int32_t_int32_t o8;
  int32_t o9;
  FlowProd_int32_t_int32_t o10;
  int32_t o11;
  FlowProd_doublep_int32_t o12;
  double o13;
  FlowProd_int32_t_int32_t o14;
  int32_t o15;
  FlowProd_int32_t_int32_t o16;
  int32_t o17;
  FlowProd_doublep_int32_t o18;
  double o19;
  FlowProd_double_double o20;
  double o21;
  FlowProd_double_double o22;
  o0 = in;
  o2 = o0.f0;
  o3 = o0.f1;
  o4 = o0.f2;
  o5 = o0.f3;
  o6 = o0.f4;
  o7 = o0.f5;
  o8.f1 = 128;
  o14.f1 = 128;
  o12.f0 = o2;
  o8.f0 = o3;
  o18.f0 = o4;
  o16.f1 = o5;
  o22.f0 = o6;
  o10.f1 = o7;
  o14.f0 = o7;
  o9 = (int32_t)((uint32_t)o8.f0 * (uint32_t)o8.f1);
  o15 = (int32_t)((uint32_t)o14.f0 * (uint32_t)o14.f1);
  o10.f0 = o9;
  o16.f0 = o15;
  o11 = (int32_t)((uint32_t)o10.f0 + (uint32_t)o10.f1);
  o17 = (int32_t)((uint32_t)o16.f0 + (uint32_t)o16.f1);
  o12.f1 = o11;
  o18.f1 = o17;
  int64_t t0 = (int64_t)o12.f1;
  o13 = o2[(unsigned long long)t0];
  int64_t t1 = (int64_t)o18.f1;
  o19 = o4[(unsigned long long)t1];
  o20.f0 = o13;
  o20.f1 = o19;
  o21 = o20.f0 * o20.f1;
  o22.f1 = o21;
  o1 = o22.f0 + o22.f1;
  return o1;
}
static __device__ double d_fn4(FlowProd_int32_tp_doublep_doublep_int32_t in) {
  FlowProd_int32_tp_doublep_doublep_int32_t o0;
  double o1;
  int32_t* o2;
  double* o3;
  double* o4;
  int32_t o5;
  FlowProd_int32_t_int32_t o6;
  int32_t o7;
  FlowProd_int32_t_int32_t o8;
  int32_t o9;
  FlowProd_double_int32_tp o10;
  double o11;
  int32_t* o12;
  FlowProd_doublep_int32_t_doublep_int32_t_double_int32_tp o13;
  o0 = in;
  o2 = o0.f0;
  o3 = o0.f1;
  o4 = o0.f2;
  o5 = o0.f3;
  o6.f1 = 128;
  o8.f1 = 128;
  o10.f0 = 0e0;
  o10.f1 = o2;
  o13.f0 = o3;
  o13.f2 = o4;
  o6.f0 = o5;
  o8.f0 = o5;
  o11 = o10.f0;
  o12 = o10.f1;
  o7 = o6.f0 / o6.f1;
  o9 = o8.f0 % o8.f1;
  o13.f4 = o11;
  o13.f5 = o12;
  o13.f1 = o7;
  o13.f3 = o9;
  double t0 = o13.f4;
  for (unsigned long long t1 = 0; t1 < 128ULL; t1++) {
    FlowProd_doublep_int32_t_doublep_int32_t_double_int32_t pair;
    pair.f0 = o3;
    pair.f1 = o7;
    pair.f2 = o4;
    pair.f3 = o9;
    pair.f4 = t0;
    pair.f5 = o12[t1];
    t0 = d_fn3(pair);
  }
  o1 = t0;
  return o1;
}

__global__ void k0_0(int32_t* out, long long n) {
  long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < n) out[i] = (int32_t)i;
}
__global__ void k0_2(double* out, int32_t* in) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 16384ULL) {
    out[i] = fn1(in[i]);
  }
}
__global__ void k0_3(double* out, int32_t* in) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 16384ULL) {
    out[i] = fn2(in[i]);
  }
}
__global__ void k0_4(double* out, int32_t* in, int32_t* cap0, double* cap1, double* cap2) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 16384ULL) {
    FlowProd_int32_tp_doublep_doublep_int32_t pair;
    pair.f0 = cap0;
    pair.f1 = cap1;
    pair.f2 = cap2;
    pair.f3 = in[i];
    out[i] = d_fn4(pair);
  }
}
__global__ void k0_5(double* result, double* arr, int64_t idx) {
  *result = arr[(unsigned long long)idx];
}

static void flow_main();

static void flow_main() {
  int32_t* o2 = nullptr;
  double* o3 = nullptr;
  double* o4 = nullptr;
  int32_t* o5 = nullptr;
  FlowProd_int32_tp_doublep_doublep_int32_tp o6;
  double* o7 = nullptr;
  FlowProd_doublep_int32_t o8;
  double o9;
  double o10;
  FlowProd_doublep_int32_t o12;
  double o13;
  double o14;
  char* arena0 = nullptr;
  double* t0 = nullptr;
  double* t1 = nullptr;
  cu_check(cudaMalloc((void**)&arena0, 459776ULL), "cudaMalloc(arena0)");
  o2 = (int32_t*)(arena0 + 0ULL);
  k0_0<<<(unsigned int)((16384ULL + 255ULL) / 256ULL), 256>>>(o2, 16384);
  o5 = (int32_t*)(arena0 + 65536ULL);
  k0_0<<<(unsigned int)((128ULL + 255ULL) / 256ULL), 256>>>(o5, 128);
  o8.f1 = 0;
  o12.f1 = 16383;
  o3 = (double*)(arena0 + 66048ULL);
  k0_2<<<(unsigned int)((16384ULL + 255ULL) / 256ULL), 256>>>(o3, o2);
  o4 = (double*)(arena0 + 197120ULL);
  k0_3<<<(unsigned int)((16384ULL + 255ULL) / 256ULL), 256>>>(o4, o2);
  o6.f3 = o2;
  o6.f0 = o5;
  o6.f1 = o3;
  o6.f2 = o4;
  o7 = (double*)(arena0 + 328192ULL);
  k0_4<<<(unsigned int)((16384ULL + 255ULL) / 256ULL), 256>>>(o7, o2, o5, o3, o4);
  o8.f0 = o7;
  o12.f0 = o7;
  t0 = (double*)(arena0 + 459264ULL);
  k0_5<<<1, 1>>>(t0, o7, (int64_t)0);
  cu_check(cudaMemcpy(&o9, t0, sizeof(double), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  t1 = (double*)(arena0 + 459520ULL);
  k0_5<<<1, 1>>>(t1, o7, (int64_t)16383);
  cu_check(cudaMemcpy(&o13, t1, sizeof(double), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  o10 = o9;
  o14 = o13;
  flow_print_f64(o10, true);
  flow_print_f64(o14, true);
  cu_check(cudaFree(arena0), "cudaFree(arena0)");
  return;
}

int main() {
  trap_init();
  flow_main();
  cu_check(cudaFree(d_trap), "cudaFree(d_trap)");
  return 0;
}
