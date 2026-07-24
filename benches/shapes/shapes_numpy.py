#!/usr/bin/env python3
"""NumPy CPU baselines for the fir/conv2d Flow shapes, size-parameterized (S29)."""
import sys
import time

import numpy as np


def inputs(count: int, mul: int, add: int, modulus: int, subtract: int) -> np.ndarray:
    # Flow does integer arithmetic first, then widen_f32.
    values = np.arange(count, dtype=np.int64)
    return ((values * mul + add) % modulus - subtract).astype(np.float32)


def render(value: np.float32) -> str:
    return np.format_float_positional(value, unique=True, trim="-")


def run_fir(iters: int, n: int) -> None:
    x = inputs(n + 63, 7, 13, 101, 50)
    w = inputs(64, 5, 3, 31, 15)
    # np.correlate over float32 is single-threaded C; OPENBLAS_NUM_THREADS is irrelevant.
    for _ in range(iters):
        start = time.perf_counter()
        y = np.correlate(x, w, mode="valid")
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(y[0]))
    print(render(y[n - 1]))


def run_conv(iters: int, side: int) -> None:
    stride = side + 2
    img = inputs(stride * stride, 7, 13, 101, 50).reshape(stride, stride)
    w = inputs(9, 5, 3, 31, 15)
    out = np.empty((side, side), dtype=np.float32)
    for _ in range(iters):
        start = time.perf_counter()
        out.fill(0.0)
        for k in range(9):
            out += w[k] * img[k // 3 : k // 3 + side, k % 3 : k % 3 + side]
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(out[0, 0]))
    print(render(out[side - 1, side - 1]))


def main() -> None:
    if len(sys.argv) not in {4, 5} or sys.argv[1] not in {"fir", "conv2d"} or sys.argv[2] not in {
        "1t",
        "--1t",
    }:
        raise SystemExit(f"usage: {sys.argv[0]} <fir|conv2d> <1t|--1t> <iters> [n|side]")
    try:
        iters = int(sys.argv[3])
    except ValueError:
        raise SystemExit("iters must be >= 1") from None
    if iters < 1:
        raise SystemExit("iters must be >= 1")
    default_n = 65536 if sys.argv[1] == "fir" else 512
    n = int(sys.argv[4]) if len(sys.argv) == 5 else default_n
    (run_fir if sys.argv[1] == "fir" else run_conv)(iters, n)


if __name__ == "__main__":
    main()
