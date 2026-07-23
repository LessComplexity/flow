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

struct FlowProd_floatp_floatp_int32_t_int32_t {
    float* f0;
    float* f1;
    int32_t f2;
    int32_t f3;
};
static_assert(sizeof(FlowProd_floatp_floatp_int32_t_int32_t) == 24, "FlowProd_floatp_floatp_int32_t_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_floatp {
    float* f0;
    float* f1;
};
static_assert(sizeof(FlowProd_floatp_floatp) == 16, "FlowProd_floatp_floatp: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_t_float {
    int32_t f0;
    float f1;
};
static_assert(sizeof(FlowProd_int32_t_float) == 8, "FlowProd_int32_t_float: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_int32_t_int32_t {
    int32_t f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_int32_t_int32_t) == 8, "FlowProd_int32_t_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_int32_t {
    float* f0;
    int32_t f1;
};
static_assert(sizeof(FlowProd_floatp_int32_t) == 16, "FlowProd_floatp_int32_t: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_float_float {
    float f0;
    float f1;
};
static_assert(sizeof(FlowProd_float_float) == 8, "FlowProd_float_float: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_FlowProd_int32_t_float_bool {
    FlowProd_int32_t_float f0;
    bool f1;
};
static_assert(sizeof(FlowProd_FlowProd_int32_t_float_bool) == 12, "FlowProd_FlowProd_int32_t_float_bool: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_float_bool {
    float f0;
    bool f1;
};
static_assert(sizeof(FlowProd_float_bool) == 8, "FlowProd_float_bool: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_int32_t_float {
    float* f0;
    int32_t f1;
    float f2;
};
static_assert(sizeof(FlowProd_floatp_int32_t_float) == 16, "FlowProd_floatp_int32_t_float: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_FlowProd_floatp_int32_t_bool {
    FlowProd_floatp_int32_t f0;
    bool f1;
};
static_assert(sizeof(FlowProd_FlowProd_floatp_int32_t_bool) == 24, "FlowProd_FlowProd_floatp_int32_t_bool: abi_sizeof drift (plan-smart-arenas)");
struct FlowProd_floatp_bool {
    float* f0;
    bool f1;
};
static_assert(sizeof(FlowProd_floatp_bool) == 16, "FlowProd_floatp_bool: abi_sizeof drift (plan-smart-arenas)");

__global__ void k0_0(float* result, float* arr, int64_t idx, unsigned int* trap) {
  if (idx < 0 || idx >= (int64_t)1024) { *trap = 2u; return; }
  *result = arr[(unsigned long long)idx];
}
__global__ void k1_0(float* out, float* src, int64_t idx, float val, unsigned int* trap) {
  if (idx < 0 || idx >= (int64_t)1024) { *trap = 2u; return; }
  unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;
  if (i < 1024ULL) {
    out[i] = ((int64_t)i == idx) ? val : src[i];
  }
}
__global__ void k2_0(float* result, float* arr, int64_t idx) {
  *result = arr[(unsigned long long)idx];
}

static float fn0(FlowProd_floatp_floatp_int32_t_int32_t in);
static float* fn1(FlowProd_floatp_floatp in);
static void flow_main();

static float fn0(FlowProd_floatp_floatp_int32_t_int32_t in) {
  float o1;
  float* o2 = nullptr;
  float* o3 = nullptr;
  int32_t o5;
  FlowProd_int32_t_float o6;
  FlowProd_int32_t_float o7;
  int32_t o8;
  float o9;
  FlowProd_int32_t_int32_t o10;
  bool o11;
  int32_t o13;
  FlowProd_int32_t_int32_t o14;
  int32_t o15;
  FlowProd_floatp_int32_t o16;
  float o17;
  FlowProd_int32_t_int32_t o18;
  int32_t o19;
  FlowProd_int32_t_int32_t o20;
  int32_t o21;
  FlowProd_floatp_int32_t o22;
  float o23;
  FlowProd_float_float o24;
  float o25;
  FlowProd_float_float o26;
  float o27;
  FlowProd_int32_t_int32_t o28;
  int32_t o29;
  FlowProd_int32_t_float o30;
  FlowProd_FlowProd_int32_t_float_bool o31;
  FlowProd_float_bool o32;
  float* t0 = nullptr;
  float* t1 = nullptr;
  o2 = in.f0;
  o3 = in.f1;
  o5 = in.f3;
  o6.f0 = 0;
  o6.f1 = 0e0f;
  o13 = (int32_t)((uint32_t)in.f2 * (uint32_t)32);
  o7 = o6;
  while (true) {
    o10.f1 = 32;
    o8 = o7.f0;
    o9 = o7.f1;
    o10.f0 = o8;
    o32.f0 = o9;
    o11 = o10.f0 < o10.f1;
    o32.f1 = o11;
    if (!o32.f1) { break; }
    o18.f1 = 32;
    o28.f1 = 1;
    o16.f0 = o2;
    o22.f0 = o3;
    o20.f1 = o5;
    o14.f0 = o13;
    o14.f1 = o8;
    o18.f0 = o8;
    o28.f0 = o8;
    o26.f0 = o9;
    o15 = (int32_t)((uint32_t)o14.f0 + (uint32_t)o14.f1);
    o19 = (int32_t)((uint32_t)o18.f0 * (uint32_t)o18.f1);
    o29 = (int32_t)((uint32_t)o28.f0 + (uint32_t)o28.f1);
    o31.f1 = o11;
    o16.f1 = o15;
    o20.f0 = o19;
    o30.f0 = o29;
    cu_check(cudaMalloc((void**)&t0, sizeof(float) * 1ULL), "cudaMalloc(t0)");
    k0_0<<<1, 1>>>(t0, o2, (int64_t)o15, d_trap);
    trap_check_after_launch();
    cu_check(cudaMemcpy(&o17, t0, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
    o21 = (int32_t)((uint32_t)o20.f0 + (uint32_t)o20.f1);
    o24.f0 = o17;
    o22.f1 = o21;
    cu_check(cudaMalloc((void**)&t1, sizeof(float) * 1ULL), "cudaMalloc(t1)");
    k0_0<<<1, 1>>>(t1, o3, (int64_t)o21, d_trap);
    trap_check_after_launch();
    cu_check(cudaMemcpy(&o23, t1, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
    o24.f1 = o23;
    o25 = o24.f0 * o24.f1;
    o26.f1 = o25;
    o27 = o26.f0 + o26.f1;
    o30.f1 = o27;
    o31.f0 = o30;
    o7 = o31.f0;
  }
  o1 = o32.f0;
  cu_check(cudaFree(t0), "cudaFree(t0)");
  cu_check(cudaFree(t1), "cudaFree(t1)");
  return o1;
}

static float* fn1(FlowProd_floatp_floatp in) {
  float* o1 = nullptr;
  float* o2 = nullptr;
  float* o3 = nullptr;
  FlowProd_floatp_int32_t o4;
  FlowProd_floatp_int32_t o5;
  float* o6 = nullptr;
  int32_t o7;
  FlowProd_int32_t_int32_t o8;
  bool o9;
  FlowProd_int32_t_int32_t o10;
  int32_t o11;
  FlowProd_int32_t_int32_t o12;
  int32_t o13;
  FlowProd_floatp_floatp_int32_t_int32_t o14;
  float o15;
  FlowProd_floatp_int32_t_float o16;
  float* o17 = nullptr;
  FlowProd_int32_t_int32_t o18;
  int32_t o19;
  FlowProd_floatp_int32_t o20;
  FlowProd_FlowProd_floatp_int32_t_bool o21;
  FlowProd_floatp_bool o22;
  o2 = in.f0;
  o3 = in.f1;
  o4.f1 = 0;
  o4.f0 = o3;
  o5 = o4;
  while (true) {
    o8.f1 = 1024;
    o6 = o5.f0;
    o7 = o5.f1;
    o22.f0 = o6;
    o8.f0 = o7;
    o9 = o8.f0 < o8.f1;
    o22.f1 = o9;
    if (!o22.f1) { break; }
    o10.f1 = 32;
    o12.f1 = 32;
    o18.f1 = 1;
    o14.f0 = o2;
    o14.f1 = o3;
    o16.f0 = o6;
    o10.f0 = o7;
    o12.f0 = o7;
    o16.f1 = o7;
    o18.f0 = o7;
    o11 = o10.f0 / o10.f1;
    o13 = o12.f0 % o12.f1;
    o19 = (int32_t)((uint32_t)o18.f0 + (uint32_t)o18.f1);
    o21.f1 = o9;
    o14.f2 = o11;
    o14.f3 = o13;
    o20.f1 = o19;
    o15 = fn0(o14);
    o16.f2 = o15;
    cu_check(cudaMalloc((void**)&o17, sizeof(float) * 1024ULL), "cudaMalloc(o17)");
    k1_0<<<(unsigned int)((1024ULL + 255ULL) / 256ULL), 256>>>(o17, o6, (int64_t)o7, o15, d_trap);
    trap_check_after_launch();
    o20.f0 = o17;
    o21.f0 = o20;
    o5 = o21.f0;
  }
  o1 = o22.f0;
  if (o17 != o1) {
    cu_check(cudaFree(o17), "cudaFree(o17)");
  }
  return o1;
}

static void flow_main() {
  float* o2 = nullptr;
  float* o3 = nullptr;
  FlowProd_floatp_floatp o4;
  float* o5 = nullptr;
  float o7;
  float o8;
  float o11;
  float o12;
  char* arena0 = nullptr;
  float* t2 = nullptr;
  float* t3 = nullptr;
  cu_check(cudaMalloc((void**)&arena0, 8704ULL), "cudaMalloc(arena0)");
  static const float lit0[1024] = { -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f };
  o2 = (float*)(arena0 + 0ULL);
  cu_check(cudaMemcpy(o2, lit0, sizeof(lit0), cudaMemcpyHostToDevice), "cudaMemcpy(literal)");
  static const float lit1[1024] = { 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f, 4e0f, 1.1e1f, 1.8e1f, 2.5e1f, 3.2e1f, 3.9e1f, 4.6e1f, -4.8e1f, -4.1e1f, -3.4e1f, -2.7e1f, -2e1f, -1.3e1f, -6e0f, 1e0f, 8e0f, 1.5e1f, 2.2e1f, 2.9e1f, 3.6e1f, 4.3e1f, 5e1f, -4.4e1f, -3.7e1f, -3e1f, -2.3e1f, -1.6e1f, -9e0f, -2e0f, 5e0f, 1.2e1f, 1.9e1f, 2.6e1f, 3.3e1f, 4e1f, 4.7e1f, -4.7e1f, -4e1f, -3.3e1f, -2.6e1f, -1.9e1f, -1.2e1f, -5e0f, 2e0f, 9e0f, 1.6e1f, 2.3e1f, 3e1f, 3.7e1f, 4.4e1f, -5e1f, -4.3e1f, -3.6e1f, -2.9e1f, -2.2e1f, -1.5e1f, -8e0f, -1e0f, 6e0f, 1.3e1f, 2e1f, 2.7e1f, 3.4e1f, 4.1e1f, 4.8e1f, -4.6e1f, -3.9e1f, -3.2e1f, -2.5e1f, -1.8e1f, -1.1e1f, -4e0f, 3e0f, 1e1f, 1.7e1f, 2.4e1f, 3.1e1f, 3.8e1f, 4.5e1f, -4.9e1f, -4.2e1f, -3.5e1f, -2.8e1f, -2.1e1f, -1.4e1f, -7e0f, 0e0f, 7e0f, 1.4e1f, 2.1e1f, 2.8e1f, 3.5e1f, 4.2e1f, 4.9e1f, -4.5e1f, -3.8e1f, -3.1e1f, -2.4e1f, -1.7e1f, -1e1f, -3e0f };
  o3 = (float*)(arena0 + 4096ULL);
  cu_check(cudaMemcpy(o3, lit1, sizeof(lit1), cudaMemcpyHostToDevice), "cudaMemcpy(literal)");
  o4.f0 = o2;
  o4.f1 = o3;
  o5 = fn1(o4);
  t2 = (float*)(arena0 + 8192ULL);
  k2_0<<<1, 1>>>(t2, o5, (int64_t)0);
  cu_check(cudaMemcpy(&o7, t2, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  t3 = (float*)(arena0 + 8448ULL);
  k2_0<<<1, 1>>>(t3, o5, (int64_t)1023);
  cu_check(cudaMemcpy(&o11, t3, sizeof(float), cudaMemcpyDeviceToHost), "cudaMemcpy(index)");
  o8 = o7;
  o12 = o11;
  flow_print_f32(o8, true);
  flow_print_f32(o12, true);
  cu_check(cudaFree(arena0), "cudaFree(arena0)");
  return;
}

int main() {
  trap_init();
  flow_main();
  cu_check(cudaFree(d_trap), "cudaFree(d_trap)");
  return 0;
}
