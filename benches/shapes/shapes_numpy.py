#!/usr/bin/env python3
"""NumPy CPU baselines for fir_65536.flow and conv2d_512.flow."""
import sys
import time

import numpy as np


def inputs(count: int, mul: int, add: int, modulus: int, subtract: int) -> np.ndarray:
    # Flow does integer arithmetic first, then widen_f32.
    values = np.arange(count, dtype=np.int64)
    return ((values * mul + add) % modulus - subtract).astype(np.float32)


def render(value: np.float32) -> str:
    return np.format_float_positional(value, unique=True, trim="-")


def run_fir(iters: int) -> None:
    x = inputs(65599, 7, 13, 101, 50)
    w = inputs(64, 5, 3, 31, 15)
    # np.correlate over float32 is single-threaded C; OPENBLAS_NUM_THREADS is irrelevant.
    for _ in range(iters):
        start = time.perf_counter()
        y = np.correlate(x, w, mode="valid")
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(y[0]))
    print(render(y[65535]))


def run_conv(iters: int) -> None:
    img = inputs(514 * 514, 7, 13, 101, 50).reshape(514, 514)
    w = inputs(9, 5, 3, 31, 15)
    out = np.empty((512, 512), dtype=np.float32)
    for _ in range(iters):
        start = time.perf_counter()
        out.fill(0.0)
        for k in range(9):
            out += w[k] * img[k // 3 : k // 3 + 512, k % 3 : k % 3 + 512]
        print(f"iter ms={(time.perf_counter() - start) * 1000:.6f}")
    print(render(out[0, 0]))
    print(render(out[511, 511]))


def main() -> None:
    if len(sys.argv) != 4 or sys.argv[1] not in {"fir", "conv2d"} or sys.argv[2] not in {
        "1t",
        "--1t",
    }:
        raise SystemExit(f"usage: {sys.argv[0]} <fir|conv2d> <1t|--1t> <iters>")
    try:
        iters = int(sys.argv[3])
    except ValueError:
        raise SystemExit("iters must be >= 1") from None
    if iters < 1:
        raise SystemExit("iters must be >= 1")
    (run_fir if sys.argv[1] == "fir" else run_conv)(iters)


if __name__ == "__main__":
    main()
