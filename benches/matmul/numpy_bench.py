#!/usr/bin/env python3
"""numpy matmul baseline (fp32, the box's BLAS). Usage: numpy_bench.py N ITERS"""
import sys, time
import numpy as np

n = int(sys.argv[1]); iters = int(sys.argv[2])
nn = n * n
a = np.array([((i * 7 + 13) % 101) - 50 for i in range(nn)], dtype=np.float32).reshape(n, n)
b = np.array([((i * 7 + 57) % 101) - 50 for i in range(nn)], dtype=np.float32).reshape(n, n)
for _ in range(3):
    c = a @ b
best = float("inf")
for _ in range(iters):
    t0 = time.perf_counter()
    c = a @ b
    best = min(best, time.perf_counter() - t0)
ms = best * 1e3
print(f"numpy N={n} {ms:.4f} ms {2.0 * n**3 / (ms * 1e6):.1f} GFLOP/s c0={c.flat[0]:.1f} clast={c.flat[nn - 1]:.1f}")
