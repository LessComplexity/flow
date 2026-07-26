"""NumPy baselines for the shape-ladder-v2 classes: saxpy, reduce, transpose, gather.

Same rules as every other leg: data generation happens outside the timer, the kernel
is the timed region, and the same two probe values plus `iter ms=` are printed.

NumPy is the expert-tuned column here in the same sense as the matmul table: these
are the vectorized primitives a working numerical programmer would actually reach
for, not a Python loop.
"""

import sys
import time

import numpy as np


def gen(n: int, mul: int, add: int, modulus: int, sub: int) -> np.ndarray:
    t = np.arange(n, dtype=np.int64)
    return ((t * mul + add) % modulus - sub).astype(np.float32)


def render(value) -> str:
    text = repr(float(value))
    return text[:-2] if text.endswith(".0") else text


def run_saxpy(iters: int, n: int) -> None:
    x = gen(n, 7, 13, 101, 50)
    y0 = gen(n, 5, 3, 31, 15)
    y = np.empty_like(x)
    for _ in range(iters):
        start = time.perf_counter()
        np.add(np.multiply(x, np.float32(2.5)), y0, out=y)
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(y[0]))
    print(render(y[-1]))


def run_reduce(iters: int, n: int) -> None:
    x = gen(n, 7, 13, 101, 50)
    total = np.float32(0)
    for _ in range(iters):
        start = time.perf_counter()
        total = x.sum(dtype=np.float32)
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(total))


def run_transpose(iters: int, side: int) -> None:
    a = gen(side * side, 7, 13, 101, 50).reshape(side, side)
    b = np.empty_like(a)
    for _ in range(iters):
        start = time.perf_counter()
        # .T is a view; the copy is the actual data movement, which is the point.
        b = np.ascontiguousarray(a.T)
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    flat = b.reshape(-1)
    print(render(flat[0]))
    print(render(flat[-1]))


def run_gather(iters: int, n: int) -> None:
    x = gen(n, 7, 13, 101, 50)
    idx = ((np.arange(n, dtype=np.int64) * 1021 + 12347) % n).astype(np.int64)
    y = np.empty_like(x)
    for _ in range(iters):
        start = time.perf_counter()
        np.take(x, idx, out=y)
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(y[0]))
    print(render(y[-1]))


def main() -> None:
    if len(sys.argv) < 3:
        print("usage: ladder2_numpy.py <saxpy|reduce|transpose|gather> <iters> [size]",
              file=sys.stderr)
        raise SystemExit(2)
    shape, iters = sys.argv[1], int(sys.argv[2])
    size = int(sys.argv[3]) if len(sys.argv) > 3 else 1048576
    {"saxpy": run_saxpy, "reduce": run_reduce,
     "transpose": run_transpose, "gather": run_gather}[shape](iters, size)


if __name__ == "__main__":
    main()
