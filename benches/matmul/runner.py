#!/usr/bin/env python3
"""Runs every benchmark leg, writes results.csv (leg,N,ms,gflops,note).
mapal-cuda, mapal-llvm, mapal-cuda-cap-{f64,f32} and mapal-llvm-cap-{f64,f32} are
process-wall min-of-3 with an adaptive cap; mapal-cuda-cap-kernel-{f64,f32} are
per-iteration compute (sum of the binary's MAPAL_PERF kernel-event times, min of
3 process runs); mapal-llvm-cap-compute-{f64,f32} are mapal_main compute time,
also min-of-3; the compiled CUDA / BLAS / numpy / rust / cpp / chapel legs
self-report per-iteration times. A machine-spec comment header (utc / cpu /
threads / core quota / RAM / clang) is stamped above the CSV header (S26
standing rule: comparisons are same-machine, specs on the record).
S26b: cpp-mt/rust-mt quota-aware threaded baselines + numpy-1t/chapel-1t
env-pinned variants (Sapir's par-on-par directive; existing legs untouched).
An optional argv leg filter (`python3 runner.py leg1 leg2 ...`) runs only the
named legs; no args = the full standing sweep."""
import subprocess, time, csv, sys, re, os, datetime

ROWS = []

# --- machine-spec header probes (S26 rule; every probe guarded -> "unknown", ---
# an absent cgroup quota reads "none" = uncapped; safe on macOS + minimal images)
def _probe_cpu():
    try:
        m = re.search(r"model name\s*:\s*(.+)", open("/proc/cpuinfo").read())
        return m.group(1).strip() if m else "unknown"
    except Exception:
        pass
    try:  # macOS (S27 local runs)
        return subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"],
                              capture_output=True, text=True, timeout=10).stdout.strip() or "unknown"
    except Exception:
        return "unknown"

def _probe_quota():
    try:  # cgroup v2: "max 100000" or "<quota_us> <period_us>"
        v = open("/sys/fs/cgroup/cpu.max").read().split()
        return "none" if v[0] == "max" else f"{int(v[0]) / int(v[1]):.2f}"
    except Exception:
        pass
    try:  # cgroup v1: quota -1 = uncapped (paths carry the cpu/ controller dir)
        q = int(open("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").read())
        p = int(open("/sys/fs/cgroup/cpu/cpu.cfs_period_us").read())
        return "none" if q <= 0 else f"{q / p:.2f}"
    except Exception:
        return "unknown"

def _probe_ram_gb():
    try:
        m = re.search(r"MemTotal:\s*(\d+) kB", open("/proc/meminfo").read())
        return f"{int(m.group(1)) / 2**20:.0f}" if m else "unknown"
    except Exception:
        pass
    try:  # macOS (S27 local runs)
        b = subprocess.run(["sysctl", "-n", "hw.memsize"],
                           capture_output=True, text=True, timeout=10).stdout.strip()
        return f"{int(b) / 2**30:.0f}"
    except Exception:
        return "unknown"

def _probe_clang():
    try:
        return subprocess.run(["clang", "--version"], capture_output=True, text=True, timeout=10).stdout.splitlines()[0]
    except Exception:
        return "unknown"

SPEC = [
    ("utc", datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")),
    ("cpu", _probe_cpu()),
    ("threads", os.cpu_count() or "unknown"),
    ("core_quota", _probe_quota()),
    ("ram_gb", _probe_ram_gb()),
    ("clang", _probe_clang()),
]

def run(cmd, timeout=None, cap=None, env=None):
    # env: None = inherit; a dict of deltas is expanded over os.environ here.
    if env:
        env = {**os.environ, **env}
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=env)

ONLY = set(sys.argv[1:])  # optional leg filter (S26b trimmed box runs)

# S27: optional size ceiling for local runs (macOS stack caps the llvm legs;
# naive 4096 baselines cost ~8 min each). Unset = every leg's full size list.
MAX_N = int(os.environ.get("MAPAL_BENCH_MAX_N", "0")) or None

def clamp(sizes):
    return tuple(n for n in sizes if MAX_N is None or (n if isinstance(n, int) else n[0]) <= MAX_N)

def wanted(leg):
    return not ONLY or leg in ONLY

def add(leg, n, ms, note=""):
    gf = 2.0 * n**3 / (ms * 1e6)
    ROWS.append((leg, n, f"{ms:.4f}", f"{gf:.2f}", note))
    print(f"{leg:12s} N={n:5d} {ms:12.4f} ms {gf:10.2f} GFLOP/s {note}", flush=True)

# --- mapal-cuda (process wall, min of 3; correctness stdout shown for N=4) ---
for n in ((4, 16, 32, 64, 128) if wanted("mapal-cuda") else ()):
    try:
        best, out = float("inf"), ""
        for _ in range(3):
            t0 = time.perf_counter()
            r = run([f"./mm_cu_{n}"], timeout=3600)
            dt = (time.perf_counter() - t0) * 1e3
            if r.returncode != 0:
                print(f"mapal-cuda N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                best = None
                break
            best = min(best, dt)
            out = r.stdout.strip().replace("\n", "/")
        if best is None:
            continue
        add("mapal-cuda", n, best, f"out={out}")
        if n == 64 and best > 600_000:
            print("mapal-cuda N=64 over 600 s — skipping N=128", flush=True)
            break
    except FileNotFoundError:
        break
    except subprocess.TimeoutExpired:
        print(f"mapal-cuda N={n} TIMEOUT — stopping leg", flush=True)
        break

# --- mapal-llvm / mapal-cuda-cap-{f64,f32} / mapal-llvm-cap-{f64,f32} ---
# (process wall, min of 3) — same shape as mapal-cuda above; cap_at/cap_ms give
# each leg its adaptive skip (the llvm loop form hits the naive-Update N^4
# wall, its capture form the by-value-capture N^4 wall — stop before the 1 h
# timeout).
for leg, fmt, sizes, cap_at, cap_ms in (
    ("mapal-llvm", "./mm_ll_{}", (4, 16, 32, 64, 128), 64, 600_000),
    ("mapal-cuda-cap-f64", "./mm_cu_cap_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 64, 600_000),
    ("mapal-cuda-cap-f32", "./mm_cu_cap_f32_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 64, 600_000),
    # S21: the llvm cap legs run every size — WP3b killed the by-value/aggregate
    # walls; the 256-checkpoint cap still guards the 512 leg adaptively.
    # S24: the plain legs run the parallel orchestrator (MAPAL_PAR unset = all
    # cores); the -1t rows pin the same binaries to one thread at the
    # comparison sizes — the single-thread baseline in the same table.
    # S27: sizes run to 4096 (the S26c 4096-minimum directive; the ulimit
    # wrapper below is what makes the 2048/4096 alloca stacks viable), and the
    # -fma legs are the product face (contract flags in the .ll — outputs are
    # numerically-equal-not-byte-equal to the conformance legs by design).
    ("mapal-llvm-cap-f64", "./mm_ll_cap_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 256, 120_000),
    ("mapal-llvm-cap-f32", "./mm_ll_cap_f32_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 256, 120_000),
    ("mapal-llvm-cap-f64-1t", "MAPAL_PAR=1 ./mm_ll_cap_{}", (512, 1024, 2048, 4096), 4096, 3_600_000),
    ("mapal-llvm-cap-f32-1t", "MAPAL_PAR=1 ./mm_ll_cap_f32_{}", (512, 1024, 2048, 4096), 4096, 3_600_000),
    ("mapal-llvm-cap-f64-fma", "./mm_ll_fma_cap_{}", (256, 512, 1024, 2048, 4096), 256, 120_000),
    ("mapal-llvm-cap-f32-fma", "./mm_ll_fma_cap_f32_{}", (256, 512, 1024, 2048, 4096), 256, 120_000),
    ("mapal-llvm-cap-f64-fma-1t", "MAPAL_PAR=1 ./mm_ll_fma_cap_{}", (512, 1024, 2048, 4096), 4096, 3_600_000),
    ("mapal-llvm-cap-f32-fma-1t", "MAPAL_PAR=1 ./mm_ll_fma_cap_f32_{}", (512, 1024, 2048, 4096), 4096, 3_600_000),
):
    if not wanted(leg):
        continue
    for n in clamp(sizes):
        try:
            best, out = float("inf"), ""
            cmd = [fmt.format(n)]
            if leg.startswith("mapal-llvm"):
                # allocas hold the arrays; N>=1024 needs a big stack (heap
                # lowering is the recorded fix). No `exec` — the -1t legs carry
                # an env prefix; the ~ms bash wrapper cost is identical across
                # every mapal-llvm row, so within-table ratios stay clean.
                cmd = ["bash", "-c", f"ulimit -s unlimited 2>/dev/null || ulimit -s hard; {cmd[0]}"]
            for _ in range(3):
                t0 = time.perf_counter()
                r = run(cmd, timeout=3600)
                dt = (time.perf_counter() - t0) * 1e3
                if r.returncode != 0:
                    print(f"{leg} N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                    best = None
                    break
                best = min(best, dt)
                out = r.stdout.strip().replace("\n", "/")
            if best is None:
                continue
            add(leg, n, best, f"out={out}")
            if n == cap_at and best > cap_ms:
                print(f"{leg} N={n} over {cap_ms / 1e3:.0f} s — skipping larger sizes", flush=True)
                break
        except FileNotFoundError:
            break
        except subprocess.TimeoutExpired:
            print(f"{leg} N={n} TIMEOUT — stopping leg", flush=True)
            break

# --- mapal-cuda-cap-kernel-{f64,f32} (per-iteration compute: sum of the ---
# binary's MAPAL_PERF launch= CUDA-event times; process run repeated 3x, min of
# sums — startup EXCLUDED, unlike the process-wall legs above; the note keeps
# the per-launch detail + the binary's own MAPAL_PERF total) ---
for leg, fmt in (
    ("mapal-cuda-cap-kernel-f64", "./mm_cu_cap_perf_{}"),
    ("mapal-cuda-cap-kernel-f32", "./mm_cu_cap_f32_perf_{}"),
):
    if not wanted(leg):
        continue
    for n in clamp((16, 64, 128, 256, 512, 1024, 2048, 4096)):
        try:
            best, note = float("inf"), ""
            for _ in range(3):
                r = run([fmt.format(n)], timeout=3600)
                if r.returncode != 0:
                    print(f"{leg} N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                    best = None
                    break
                launches = re.findall(r"MAPAL_PERF launch=(\S+) ms=([\d.]+)", r.stdout)
                if not launches:
                    print(f"{leg} N={n} FAILED: no MAPAL_PERF lines in stdout", flush=True)
                    best = None
                    break
                best = min(best, sum(float(ms) for _, ms in launches))
                out = "/".join(l for l in r.stdout.strip().splitlines() if not l.startswith("MAPAL_PERF"))
                total = re.search(r"MAPAL_PERF total ms=([\d.]+)", r.stdout)
                note = f"out={out} " + " ".join(f"{k}:{ms}" for k, ms in launches)
                if total:
                    note += f" total:{total.group(1)}"
            if best is None:
                continue
            add(leg, n, best, note)
        except FileNotFoundError:
            break
        except subprocess.TimeoutExpired:
            print(f"{leg} N={n} TIMEOUT — stopping leg", flush=True)
            break

# --- mapal-llvm-cap-compute-{f64,f32}[-1t] (mapal_main timer, min of 3) ---
# -fma-compute twins are the product face; sizes run to 4096. -1t twins pin
# MAPAL_PAR=1 (replaces the S27c manual 1t rows).
for leg, fmt, par in (
    ("mapal-llvm-cap-compute-f64", "./mm_ll_perf_cap_{}", "par"),
    ("mapal-llvm-cap-compute-f32", "./mm_ll_perf_cap_f32_{}", "par"),
    ("mapal-llvm-cap-fma-compute-f64", "./mm_ll_fma_perf_cap_{}", "par"),
    ("mapal-llvm-cap-fma-compute-f32", "./mm_ll_fma_perf_cap_f32_{}", "par"),
    ("mapal-llvm-cap-compute-f64-1t", "./mm_ll_perf_cap_{}", "1"),
    ("mapal-llvm-cap-compute-f32-1t", "./mm_ll_perf_cap_f32_{}", "1"),
    ("mapal-llvm-cap-fma-compute-f64-1t", "./mm_ll_fma_perf_cap_{}", "1"),
    ("mapal-llvm-cap-fma-compute-f32-1t", "./mm_ll_fma_perf_cap_f32_{}", "1"),
):
    if not wanted(leg):
        continue
    for n in (16, 64, 128, 256, 512, 1024, 2048, 4096):
        try:
            best, note = float("inf"), ""
            cmd = [
                "bash",
                "-c",
                f"export MAPAL_PAR={par}; ulimit -s unlimited 2>/dev/null || ulimit -s hard; {fmt.format(n)}",
            ]
            for _ in range(3):
                r = run(cmd, timeout=3600)
                if r.returncode != 0:
                    print(f"{leg} N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                    best = None
                    break
                total = re.search(r"MAPAL_PERF total ms=([\d.]+)", r.stdout)
                if not total:
                    print(f"{leg} N={n} FAILED: no MAPAL_PERF total in stdout", flush=True)
                    best = None
                    break
                best = min(best, float(total.group(1)))
                out = "/".join(l for l in r.stdout.strip().splitlines() if not l.startswith("MAPAL_PERF"))
                note = f"out={out} total:{total.group(1)}"
            if best is None:
                continue
            add(leg, n, best, note)
        except FileNotFoundError:
            break
        except subprocess.TimeoutExpired:
            print(f"{leg} N={n} TIMEOUT — stopping leg", flush=True)
            break

# --- self-timing legs ---
SCHED = {
    "naive-cuda": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    # S24 close review (Sapir): the naive baseline was f32-only — flow-f64's
    # like-for-like GPU comparator. Same binary, width arg (cpp legs' shape).
    "naive-cuda-f64": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "hip-naive":  [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "cublas":     [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "numpy":      [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    # S27/S26c: 2048/4096 on every CPU baseline. The 1t naive legs run ONE rep
    # at 4096 (~8 min each — min-of-1, labeled); par legs stay multi-rep.
    "rust-naive": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 1), (4096, 1)],
    "cpp-naive-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 1), (4096, 1)],
    "cpp-naive-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 1), (4096, 1)],
    "chapel-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 2), (4096, 1)],
    "chapel-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 2), (4096, 1)],
    "chapel-gpu-f32": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "chapel-gpu-f64": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    # S26b framing directive (Sapir): par-on-par + 1t-on-1t only. The mt legs
    # are the quota-aware threaded twins (cpp_mt/rust_mt); the -1t legs pin
    # the threaded runtimes to one worker via env (same binary, same recipe).
    "cpp-mt-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 3), (4096, 2)],
    "cpp-mt-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 3), (4096, 2)],
    "rust-mt":    [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 3), (4096, 2)],
    "numpy-1t":   [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "chapel-1t-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 1), (4096, 1)],
    "chapel-1t-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2), (2048, 1), (4096, 1)],
}
BINS = {"naive-cuda": "./naive_cuda", "naive-cuda-f64": "./naive_cuda",
        "hip-naive": "./hip_naive", "cublas": "./cublas_gemm",
        "numpy": None, "rust-naive": "./rust_naive",
        "cpp-naive-f32": "./cpp_naive", "cpp-naive-f64": "./cpp_naive",
        "chapel-f32": "./chapel_matmul", "chapel-f64": "./chapel_matmul",
        "chapel-gpu-f32": "./chapel_matmul_gpu", "chapel-gpu-f64": "./chapel_matmul_gpu",
        "cpp-mt-f32": "./cpp_mt", "cpp-mt-f64": "./cpp_mt",
        "rust-mt": "./rust_mt", "numpy-1t": None,
        "chapel-1t-f32": "./chapel_matmul", "chapel-1t-f64": "./chapel_matmul"}
WIDTH = {"cpp-naive-f32": "f32", "cpp-naive-f64": "f64", "naive-cuda-f64": "f64",
         "cpp-mt-f32": "f32", "cpp-mt-f64": "f64"}
ENV = {"numpy-1t": {"OPENBLAS_NUM_THREADS": "1"},
       "chapel-1t-f32": {"CHPL_RT_NUM_THREADS_PER_LOCALE": "1"},
       "chapel-1t-f64": {"CHPL_RT_NUM_THREADS_PER_LOCALE": "1"}}
for leg, sizes in SCHED.items():
    if not wanted(leg):
        continue
    for n, iters in clamp(sizes):
        if leg.startswith("chapel-"):
            # Chapel config consts take --name=value (no positional args).
            cmd = [BINS[leg], f"--n={n}", f"--iters={iters}", f"--width={leg.split(chr(45))[-1]}"]
        else:
            args = [str(n), str(iters)] + ([WIDTH[leg]] if leg in WIDTH else [])
            cmd = [BINS[leg], *args] if BINS[leg] else ["python3", "numpy_bench.py", *args]
        try:
            r = run(cmd, timeout=1800, env=ENV.get(leg))
            line = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else r.stderr[-200:]
            print("  " + line, flush=True)
            parts = line.split()
            ms = float(parts[2])
            add(leg, n, ms, line.split("c0=")[-1] if "c0=" in line else "")
        except Exception as e:
            print(f"{leg} N={n} ERROR: {e}", flush=True)

with open("results.csv", "w", newline="") as f:
    for k, v in SPEC:
        f.write(f"# {k}: {v}\n")
    w = csv.writer(f)
    w.writerow(["leg", "N", "ms", "gflops", "note"])
    w.writerows(ROWS)
print("wrote results.csv", flush=True)
