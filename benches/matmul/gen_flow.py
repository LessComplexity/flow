#!/usr/bin/env python3
"""Generate matmul{N}.flow — the flattened loop-driven matmul (the only
Core-expressible shape today: L1108 bodies don't close over arrays, so the
one-kernel map+fold form is out). Pattern: crates/flow-interp/tests/update_pipeline.rs."""
import sys

def gen(n: int) -> str:
    nn = n * n
    return f"""fn cell(a: [f32; {nn}], b: [f32; {nn}], i: i32, j: i32) -> f32 {{
    mut k: i32   <- 0;
    mut acc: f32 <- 0.0;
    loop {{
        (k < {n}) -> {{
            -true-> {{
                acc + a[i * {n} + k] * b[k * {n} + j] -> acc;
                k + 1 -> k;
                -> loop;
            }}
            -false-> acc -> ret;
        }}
    }}
}}

fn matmul(a: [f32; {nn}], b: [f32; {nn}]) -> [f32; {nn}] {{
    mut c: [f32; {nn}] <- b;
    mut t: i32       <- 0;
    loop {{
        (t < {nn}) -> {{
            -true-> {{
                t / {n} -> i;
                t % {n} -> j;
                (a, b, i, j) -> cell -> v;
                c[t] <- v;
                t + 1 -> t;
                -> loop;
            }}
            -false-> c -> ret;
        }}
    }}
}}

fn main() {{
    iota({nn}) -> ta;
    ta -> map {{ t -> (t * 7 + 13) % 101 - 50 -> widen_f32 }} -> a;
    ta -> map {{ t -> (t * 7 + 57) % 101 - 50 -> widen_f32 }} -> b;
    (a, b) -> matmul -> c;
    c[0] -> println;
    c[{nn - 1}] -> println;
}}
"""

if __name__ == "__main__":
    n = int(sys.argv[1])
    out = sys.argv[2] if len(sys.argv) > 2 else f"matmul{n}.flow"
    with open(out, "w") as f:
        f.write(gen(n))
    print(f"wrote {out} (N={n}, NN={n*n})")
