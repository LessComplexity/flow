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
  int64_t t0 = (int64_t)((int32_t)((uint32_t)((int32_t)((uint32_t)in.f1 * (uint32_t)1024)) + (uint32_t)o7));
  int64_t t1 = (int64_t)((int32_t)((uint32_t)((int32_t)((uint32_t)o7 * (uint32_t)1024)) + (uint32_t)in.f3));
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
  o10.f1 = (o5 / 1024);
  o10.f3 = (o5 % 1024);
  float t0 = o10.f4;
  FlowProd_floatp_int32_t_floatp_int32_t_float_int32_t pair;
  pair.f0 = o10.f0;
  pair.f1 = o10.f1;
  pair.f2 = o10.f2;
  pair.f3 = o10.f3;
  for (unsigned long long t1 = 0; t1 < 1024ULL; t1++) {
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
  if (i < 1048576ULL) {
    out[i] = fn1(in[i]);
  }
}
__global__ void k0_3(float* out, int32_t* in) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 1048576ULL) {
    out[i] = fn2(in[i]);
  }
}
__global__ void k0_4(float* out, int32_t* in, int32_t* cap0, float* cap1, float* cap2) {
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 1048576ULL) {
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
  cudaEvent_t fev0_start, fev0_stop;
  cudaEvent_t fev1_start, fev1_stop;
  cudaEvent_t fev2_start, fev2_stop;
  cudaEvent_t fev3_start, fev3_stop;
  cudaEvent_t fev4_start, fev4_stop;
  cudaEvent_t fev5_start, fev5_stop;
  cudaEvent_t fev6_start, fev6_stop;
  float flow_perf_total = 0.0f;
  float flow_perf_ms = 0.0f;
  float* t0 = nullptr;
  float* t1 = nullptr;
  cu_check(cudaMalloc((void**)&arena0, 16781824ULL), "cudaMalloc(arena0)");
  cu_check(cudaEventCreate(&fev0_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev0_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev1_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev1_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev2_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev2_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev3_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev3_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev4_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev4_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev5_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev5_stop), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev6_start), "cudaEventCreate");
  cu_check(cudaEventCreate(&fev6_stop), "cudaEventCreate");
  o2 = (int32_t*)(arena0 + 0ULL);
  cu_check(cudaEventRecord(fev0_start), "cudaEventRecord");
  k0_0<<<(unsigned int)((1048576ULL + 255ULL) / 256ULL), 256>>>(o2, 1048576);
  cu_check(cudaEventRecord(fev0_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev0_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev0_start, fev0_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_0 ms=%.4f\n", flow_perf_ms);
  o3 = (int32_t*)(arena0 + 4194304ULL);
  cu_check(cudaEventRecord(fev1_start), "cudaEventRecord");
  k0_0<<<(unsigned int)((1024ULL + 255ULL) / 256ULL), 256>>>(o3, 1024);
  cu_check(cudaEventRecord(fev1_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev1_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev1_start, fev1_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_0 ms=%.4f\n", flow_perf_ms);
  o4 = (float*)(arena0 + 4198400ULL);
  cu_check(cudaEventRecord(fev2_start), "cudaEventRecord");
  k0_2<<<(unsigned int)((1048576ULL + 255ULL) / 256ULL), 256>>>(o4, o2);
  cu_check(cudaEventRecord(fev2_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev2_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev2_start, fev2_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_2 ms=%.4f\n", flow_perf_ms);
  o5 = (float*)(arena0 + 8392704ULL);
  cu_check(cudaEventRecord(fev3_start), "cudaEventRecord");
  k0_3<<<(unsigned int)((1048576ULL + 255ULL) / 256ULL), 256>>>(o5, o2);
  cu_check(cudaEventRecord(fev3_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev3_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev3_start, fev3_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_3 ms=%.4f\n", flow_perf_ms);
  o6.f3 = o2;
  o6.f0 = o3;
  o6.f1 = o4;
  o6.f2 = o5;
  o7 = (float*)(arena0 + 12587008ULL);
  cu_check(cudaEventRecord(fev4_start), "cudaEventRecord");
  k0_4<<<(unsigned int)((1048576ULL + 255ULL) / 256ULL), 256>>>(o7, o2, o3, o4, o5);
  cu_check(cudaEventRecord(fev4_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev4_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev4_start, fev4_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_4 ms=%.4f\n", flow_perf_ms);
  t0 = (float*)(arena0 + 16781312ULL);
  cu_check(cudaEventRecord(fev5_start), "cudaEventRecord");
  k0_5<<<1, 1>>>(t0, o7, (int64_t)0);
  cu_check(cudaEventRecord(fev5_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev5_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev5_start, fev5_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_5 ms=%.4f\n", flow_perf_ms);
  cu_check(cudaMemcpy(&o9, t0, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  t1 = (float*)(arena0 + 16781568ULL);
  cu_check(cudaEventRecord(fev6_start), "cudaEventRecord");
  k0_5<<<1, 1>>>(t1, o7, (int64_t)1048575);
  cu_check(cudaEventRecord(fev6_stop), "cudaEventRecord");
  cu_check(cudaEventSynchronize(fev6_stop), "cudaEventSynchronize");
  cu_check(cudaEventElapsedTime(&flow_perf_ms, fev6_start, fev6_stop), "cudaEventElapsedTime");
  flow_perf_total += flow_perf_ms;
  printf("FLOW_PERF launch=k0_5 ms=%.4f\n", flow_perf_ms);
  cu_check(cudaMemcpy(&o11, t1, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  o12 = o9;
  o14 = o11;
  flow_print_f32(o12, true);
  flow_print_f32(o14, true);
  printf("FLOW_PERF total ms=%.4f\n", flow_perf_total);
  cu_check(cudaEventDestroy(fev0_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev0_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev1_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev1_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev2_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev2_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev3_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev3_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev4_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev4_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev5_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev5_stop), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev6_start), "cudaEventDestroy");
  cu_check(cudaEventDestroy(fev6_stop), "cudaEventDestroy");
  cu_check(cudaFree(arena0), "cudaFree(arena0)");
  return;
}

int main() {
  trap_init();
  flow_main();
  cu_check(cudaFree(d_trap), "cudaFree(d_trap)");
  return 0;
}
