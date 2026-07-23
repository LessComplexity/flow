// flow-backend-cuda emitted translation unit
// build: nvcc -std=c++17 -fmad=true -arch=sm_89 prog.cu libflow_rt.a -lpthread -ldl -lm -o prog
// DESIGN §4 (amended S24b, Sapir): -fmad=true is the product/perf default; conformance runs pin -fmad=false for oracle bit-parity. Host -march=native/-mfma stays forbidden in conformance.

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

struct FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t {
    float* f0;
    int32_t f1;
    float* f2;
    int32_t f3;
    float f4;
    int32_t f5;
};
static_assert(sizeof(FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t) == 40, "FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_tp_floatp_floatp_int32_t {
    int32_t* f0;
    float* f1;
    float* f2;
    int32_t f3;
};
static_assert(sizeof(FlowProd_int32_tp_floatp_floatp_int32_t) == 32, "FlowProd_int32_tp_floatp_floatp_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_tp_floatp_floatp_int32_tp {
    int32_t* f0;
    float* f1;
    float* f2;
    int32_t* f3;
};
static_assert(sizeof(FlowProd_int32_tp_floatp_floatp_int32_tp) == 32, "FlowProd_int32_tp_floatp_floatp_int32_tp: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_int32_t {
    float* f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_floatp_int32_t) == 16, "FlowProd_floatp_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_t_int32_t {
    int32_t f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_int32_t_int32_t) == 8, "FlowProd_int32_t_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_float_float {
    float f0;
    float f1;
};
static_assert(sizeof(FlowProd_float_float) == 8, "FlowProd_float_float: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_int32_t_floatp_int32_t_float_int32_tp {
    float* f0;
    int32_t f1;
    float* f2;
    int32_t f3;
    float f4;
    int32_t* f5;
};
static_assert(sizeof(FlowProd_floatp_int32_t_floatp_int32_t_float_int32_tp) == 40, "FlowProd_floatp_int32_t_floatp_int32_t_float_int32_tp: abi_sizeof drift (plan-smart-arenas)");

static __host__ __device__ float fn1(int32_t in);
static __host__ __device__ float fn2(int32_t in);
static __device__ float d_fn3(FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t in);
static __device__ float d_fn4(FlowProd_int32_tp_floatp_floatp_int32_t in);

static __host__ __device__ float fn1(int32_t in) {
  float o1;
  o1 = (float)(((int32_t)((uint32_t)(((int32_t)((uint32_t)((int32_t)((uint32_t)in * (uint32_t)7)) + (uint32_t)13)) % 101) - (uint32_t)50)));
  return o1;
}
static __host__ __device__ float fn2(int32_t in) {
  float o1;
  o1 = (float)(((int32_t)((uint32_t)(((int32_t)((uint32_t)((int32_t)((uint32_t)in * (uint32_t)7)) + (uint32_t)57)) % 101) - (uint32_t)50)));
  return o1;
}
static __device__ float d_fn3(FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t in) {
  float o1;
  float* o2;
  float* o4;
  int32_t o7;
  o2 = in.f0;
  o4 = in.f2;
  o7 = in.f5;
  int64_t t0 = (int64_t)((int32_t)((uint32_t)((int32_t)((uint32_t)in.f1 * (uint32_t)256)) + (uint32_t)o7));
  int64_t t1 = (int64_t)((int32_t)((uint32_t)((int32_t)((uint32_t)o7 * (uint32_t)256)) + (uint32_t)in.f3));
  o1 = in.f4 + ((o2[(unsigned long long)t0]) * (o4[(unsigned long long)t1]));
  return o1;
}
static __device__ float d_fn4(FlowProd_int32_tp_floatp_floatp_int32_t in) {
  float o1;
  int32_t* o2;
  float* o3;
  float* o4;
  int32_t o5;
  FlowProd_floatp_int32_t_floatp_int32_t_float_int32_tp o10;
  o2 = in.f0;
  o3 = in.f1;
  o4 = in.f2;
  o5 = in.f3;
  o10.f4 = 0e0f;
  o10.f5 = o2;
  o10.f0 = o3;
  o10.f2 = o4;
  o10.f1 = (o5 / 256);
  o10.f3 = (o5 % 256);
  float t0 = o10.f4;
  FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t pair;
  pair.f0 = o10.f0;
  pair.f1 = o10.f1;
  pair.f2 = o10.f2;
  pair.f3 = o10.f3;
  for (unsigned long long t1 = 0; t1 < 256ULL; t1++) {
    pair.f4 = t0;
    pair.f5 = o2[t1];
    t0 = d_fn3(pair);
  }
  o1 = t0;
  return o1;
}

__global__ void k0_0(int32_t* out, long long n) {
  long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < n) out[i] = (int32_t)i;
}
__global__ void k0_2(float* out, int32_t* in) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 65536ULL) {
    out[i] = fn1(in[i]);
  }
}
__global__ void k0_3(float* out, int32_t* in) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 65536ULL) {
    out[i] = fn2(in[i]);
  }
}
__global__ void k0_4(float* out, int32_t* in, int32_t* cap0, float* cap1, float* cap2) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 65536ULL) {
    FlowProd_int32_tp_floatp_floatp_int32_t pair;
    pair.f0 = cap0;
    pair.f1 = cap1;
    pair.f2 = cap2;
    pair.f3 = in[i];
    out[i] = d_fn4(pair);
  }
}
__global__ void k0_5(float* result, float* arr, int64_t idx) {
  *result = arr[(unsigned long long)idx];
}

static void flow_main();

static void flow_main() {
  int32_t* o2 = nullptr;
  int32_t* o3 = nullptr;
  float* o4 = nullptr;
  float* o5 = nullptr;
  FlowProd_int32_tp_floatp_floatp_int32_tp o6;
  float* o7 = nullptr;
  float o9;
  float o11;
  float o12;
  float o14;
  char* arena0 = nullptr;
  float* t0 = nullptr;
  float* t1 = nullptr;
  cu_check(cudaMalloc((void**)&arena0, 1050112ULL), "cudaMalloc(arena0)");
  o2 = (int32_t*)(arena0 + 0ULL);
  k0_0<<<(unsigned int)((65536ULL + 255ULL) / 256ULL), 256>>>(o2, 65536);
  o3 = (int32_t*)(arena0 + 262144ULL);
  k0_0<<<(unsigned int)((256ULL + 255ULL) / 256ULL), 256>>>(o3, 256);
  o4 = (float*)(arena0 + 263168ULL);
  k0_2<<<(unsigned int)((65536ULL + 255ULL) / 256ULL), 256>>>(o4, o2);
  o5 = (float*)(arena0 + 525312ULL);
  k0_3<<<(unsigned int)((65536ULL + 255ULL) / 256ULL), 256>>>(o5, o2);
  o6.f3 = o2;
  o6.f0 = o3;
  o6.f1 = o4;
  o6.f2 = o5;
  o7 = (float*)(arena0 + 787456ULL);
  k0_4<<<(unsigned int)((65536ULL + 255ULL) / 256ULL), 256>>>(o7, o2, o3, o4, o5);
  t0 = (float*)(arena0 + 1049600ULL);
  k0_5<<<1, 1>>>(t0, o7, (int64_t)0);
  cu_check(cudaMemcpy(&o9, t0, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  t1 = (float*)(arena0 + 1049856ULL);
  k0_5<<<1, 1>>>(t1, o7, (int64_t)65535);
  cu_check(cudaMemcpy(&o11, t1, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  o12 = o9;
  o14 = o11;
  flow_print_f32(o12, true);
  flow_print_f32(o14, true);
  cu_check(cudaFree(arena0), "cudaFree(arena0)");
  return;
}

int main() {
  trap_init();
  flow_main();
  cu_check(cudaFree(d_trap), "cudaFree(d_trap)");
  return 0;
}
