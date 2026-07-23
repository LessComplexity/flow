#!/usr/bin/env python3
"""Runs every benchmark leg, writes results.csv (leg,N,ms,gflops,note).
flow-cuda, flow-llvm, flow-cuda-cap-{f64,f32} and flow-llvm-cap-{f64,f32} are
process-wall min-of-3 with an adaptive cap; flow-cuda-cap-kernel-{f64,f32} are
per-iteration compute (sum of the binary's FLOW_PERF kernel-event times, min of
3 process runs); the compiled CUDA / BLAS / numpy / rust / cpp / chapel legs
self-report per-iteration times."""
import subprocess, time, csv, sys, re

ROWS = []

def run(cmd, timeout=None, cap=None):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)

def add(leg, n, ms, note=""):
    gf = 2.0 * n**3 / (ms * 1e6)
    ROWS.append((leg, n, f"{ms:.4f}", f"{gf:.2f}", note))
    print(f"{leg:12s} N={n:5d} {ms:12.4f} ms {gf:10.2f} GFLOP/s {note}", flush=True)

# --- flow-cuda (process wall, min of 3; correctness stdout shown for N=4) ---
for n in (4, 16, 32, 64, 128):
    try:
        best, out = float("inf"), ""
        for _ in range(3):
            t0 = time.perf_counter()
            r = run([f"./mm_cu_{n}"], timeout=3600)
            dt = (time.perf_counter() - t0) * 1e3
            if r.returncode != 0:
                print(f"flow-cuda N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                best = None
                break
            best = min(best, dt)
            out = r.stdout.strip().replace("\n", "/")
        if best is None:
            continue
        add("flow-cuda", n, best, f"out={out}")
        if n == 64 and best > 600_000:
            print("flow-cuda N=64 over 600 s — skipping N=128", flush=True)
            break
    except FileNotFoundError:
        break
    except subprocess.TimeoutExpired:
        print(f"flow-cuda N={n} TIMEOUT — stopping leg", flush=True)
        break

# --- flow-llvm / flow-cuda-cap-{f64,f32} / flow-llvm-cap-{f64,f32} ---
# (process wall, min of 3) — same shape as flow-cuda above; cap_at/cap_ms give
# each leg its adaptive skip (the llvm loop form hits the naive-Update N^4
# wall, its capture form the by-value-capture N^4 wall — stop before the 1 h
# timeout).
for leg, fmt, sizes, cap_at, cap_ms in (
    ("flow-llvm", "./mm_ll_{}", (4, 16, 32, 64, 128), 64, 600_000),
    ("flow-cuda-cap-f64", "./mm_cu_cap_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 64, 600_000),
    ("flow-cuda-cap-f32", "./mm_cu_cap_f32_{}", (16, 64, 128, 256, 512, 1024, 2048, 4096), 64, 600_000),
    # S21: the llvm cap legs run every size — WP3b killed the by-value/aggregate
    # walls; the 256-checkpoint cap still guards the 512 leg adaptively.
    # S24: the plain legs run the parallel orchestrator (FLOW_PAR unset = all
    # cores); the -1t rows pin the same binaries to one thread at the
    # comparison sizes — the single-thread baseline in the same table.
    ("flow-llvm-cap-f64", "./mm_ll_cap_{}", (16, 64, 128, 256, 512, 1024), 256, 60_000),
    ("flow-llvm-cap-f32", "./mm_ll_cap_f32_{}", (16, 64, 128, 256, 512, 1024), 256, 60_000),
    ("flow-llvm-cap-f64-1t", "FLOW_PAR=1 ./mm_ll_cap_{}", (512, 1024), 1024, 3_600_000),
    ("flow-llvm-cap-f32-1t", "FLOW_PAR=1 ./mm_ll_cap_f32_{}", (512, 1024), 1024, 3_600_000),
):
    for n in sizes:
        try:
            best, out = float("inf"), ""
            cmd = [fmt.format(n)]
            if leg.startswith("flow-llvm"):
                # allocas hold the arrays; N>=1024 needs a big stack (heap
                # lowering is the recorded fix). No `exec` — the -1t legs carry
                # an env prefix; the ~ms bash wrapper cost is identical across
                # every flow-llvm row, so within-table ratios stay clean.
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

# --- flow-cuda-cap-kernel-{f64,f32} (per-iteration compute: sum of the ---
# binary's FLOW_PERF launch= CUDA-event times; process run repeated 3x, min of
# sums — startup EXCLUDED, unlike the process-wall legs above; the note keeps
# the per-launch detail + the binary's own FLOW_PERF total) ---
for leg, fmt in (
    ("flow-cuda-cap-kernel-f64", "./mm_cu_cap_perf_{}"),
    ("flow-cuda-cap-kernel-f32", "./mm_cu_cap_f32_perf_{}"),
):
    for n in (16, 64, 128, 256, 512, 1024, 2048, 4096):
        try:
            best, note = float("inf"), ""
            for _ in range(3):
                r = run([fmt.format(n)], timeout=3600)
                if r.returncode != 0:
                    print(f"{leg} N={n} FAILED rc={r.returncode}: {r.stderr[-300:]}", flush=True)
                    best = None
                    break
                launches = re.findall(r"FLOW_PERF launch=(\S+) ms=([\d.]+)", r.stdout)
                if not launches:
                    print(f"{leg} N={n} FAILED: no FLOW_PERF lines in stdout", flush=True)
                    best = None
                    break
                best = min(best, sum(float(ms) for _, ms in launches))
                out = "/".join(l for l in r.stdout.strip().splitlines() if not l.startswith("FLOW_PERF"))
                total = re.search(r"FLOW_PERF total ms=([\d.]+)", r.stdout)
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

# --- self-timing legs ---
SCHED = {
    "naive-cuda": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "hip-naive":  [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "cublas":     [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "numpy":      [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "rust-naive": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2)],
    "cpp-naive-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2)],
    "cpp-naive-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2)],
    "chapel-f32": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2)],
    "chapel-f64": [(64, 50), (128, 20), (256, 10), (512, 5), (1024, 2)],
    "chapel-gpu-f32": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
    "chapel-gpu-f64": [(64, 200), (128, 200), (256, 100), (512, 50), (1024, 20), (2048, 5), (4096, 3)],
}
BINS = {"naive-cuda": "./naive_cuda", "hip-naive": "./hip_naive", "cublas": "./cublas_gemm",
        "numpy": None, "rust-naive": "./rust_naive",
        "cpp-naive-f32": "./cpp_naive", "cpp-naive-f64": "./cpp_naive",
        "chapel-f32": "./chapel_matmul", "chapel-f64": "./chapel_matmul",
        "chapel-gpu-f32": "./chapel_matmul_gpu", "chapel-gpu-f64": "./chapel_matmul_gpu"}
WIDTH = {"cpp-naive-f32": "f32", "cpp-naive-f64": "f64"}
for leg, sizes in SCHED.items():
    for n, iters in sizes:
        if leg.startswith("chapel-"):
            # Chapel config consts take --name=value (no positional args).
            cmd = [BINS[leg], f"--n={n}", f"--iters={iters}", f"--width={leg.split(chr(45))[-1]}"]
        else:
            args = [str(n), str(iters)] + ([WIDTH[leg]] if leg in WIDTH else [])
            cmd = [BINS[leg], *args] if BINS[leg] else ["python3", "numpy_bench.py", *args]
        try:
            r = run(cmd, timeout=1800)
            line = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else r.stderr[-200:]
            print("  " + line, flush=True)
            parts = line.split()
            ms = float(parts[2])
            add(leg, n, ms, line.split("c0=")[-1] if "c0=" in line else "")
        except Exception as e:
            print(f"{leg} N={n} ERROR: {e}", flush=True)

with open("results.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["leg", "N", "ms", "gflops", "note"])
    w.writerows(ROWS)
print("wrote results.csv", flush=True)
