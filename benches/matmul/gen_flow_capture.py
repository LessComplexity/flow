#!/usr/bin/env python3
"""Generate matmul{N}_cap.mapal — the capture-form matmul (ADR-0027): one map
over cells, inner fold over captured arrays. The one-kernel form — the S16
benchmark's unblock. `--width f32` widens the generated values to f32 (default
f64) and names the output matmul{N}_cap_f32.mapal — the like-for-like variant
against the f32 baselines (rust_naive/numpy/naive_cuda).

S30: the kernel map is bracketed by the `time` builtin and the elapsed printed as
`iter ms=` — the baselines' own format. The generated legs are therefore
COMPUTE-ONLY like every baseline, and MAPAL_PERF (which brackets all of
`mapal_main`, data generation included) is no longer needed to time them."""
import sys

def gen(n: int, width: str = "f64") -> str:
    nn = n * n
    widen = f"widen_{width}"
    return f"""fn main() {{
    {nn} -> iota -> ta;
    ta -> map {{ t -> (t * 7 + 13) % 101 - 50 -> {widen} }} -> a;
    ta -> map {{ t -> (t * 7 + 57) % 101 - 50 -> {widen} }} -> b;
    {n} -> iota -> krange;
    () -> time -> t0;
    ta -> map {{ t ->
        t / {n} -> i;
        t % {n} -> j;
        (0.0, krange) -> fold {{ acc, k -> acc + a[i * {n} + k] * b[k * {n} + j] }}
    }} -> c;
    () -> time -> t1;
    c[0] -> println;
    c[{nn - 1}] -> println;
    "iter ms=" -> print;
    t1 - t0 -> println;
}}
"""

if __name__ == "__main__":
    argv = sys.argv[1:]
    width = "f64"
    if "--width" in argv:
        i = argv.index("--width")
        width = argv[i + 1]
        del argv[i:i + 2]
    if width not in ("f32", "f64"):
        sys.exit(f"--width must be f32 or f64, got {width!r}")
    n = int(argv[0])
    default = f"matmul{n}_cap_f32.mapal" if width == "f32" else f"matmul{n}_cap.mapal"
    out = argv[1] if len(argv) > 1 else default
    with open(out, "w") as f:
        f.write(gen(n, width))
    print(f"wrote {out} (N={n}, NN={n*n}, {width})")
