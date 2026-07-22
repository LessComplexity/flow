// Naive triple-loop GEMM in Rust — the host-language baseline (same i/j/k
// order as the Flow program's cell fn). Build: rustc -O -C target-cpu=native rust_naive.rs -o rust_naive
use std::time::Instant;

fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let iters: usize = std::env::args().nth(2).unwrap().parse().unwrap();
    let nn = n * n;
    let a: Vec<f32> = (0..nn).map(|i| (((i * 7 + 13) % 101) as i32 - 50) as f32).collect();
    let b: Vec<f32> = (0..nn).map(|i| (((i * 7 + 57) % 101) as i32 - 50) as f32).collect();
    let mut c = vec![0f32; nn];
    let mut best = f64::INFINITY;
    for _ in 0..iters {
        let t0 = Instant::now();
        for i in 0..n {
            for j in 0..n {
                let mut acc = 0f32;
                for k in 0..n {
                    acc += a[i * n + k] * b[k * n + j];
                }
                c[i * n + j] = acc;
            }
        }
        best = best.min(t0.elapsed().as_secs_f64());
    }
    let ms = best * 1e3;
    println!(
        "rust-naive N={} {:.4} ms {:.1} GFLOP/s c0={:.1} clast={:.1}",
        n,
        ms,
        2.0 * (n as f64).powi(3) / (ms * 1e6),
        c[0],
        c[nn - 1]
    );
}
