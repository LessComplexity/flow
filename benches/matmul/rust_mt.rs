// Multithread naive triple-loop GEMM in Rust — the threaded host-language
// baseline (S26b, Sapir's framing directive: par-on-par comparisons only).
// Mirrors rust_naive.rs exactly — same i/j/k per-cell math, same init, same
// min-of-iters timing style — but the outer (row) loop is partitioned across
// std::thread scoped workers (no external deps: the box builds with direct
// rustc). Row partitioning preserves every cell's k-order, so outputs are
// byte-equal to the naive build at any thread count.
//   Width: quota-aware like mapal-rt — $THREADS override, else cgroup v2
//   cpu.max, else v1 cfs_quota/period (div-ceil), capped by
//   available_parallelism; the box shows 128 threads at a ~61.4-core quota.
//   Timing: workers spawn ONCE; each keeps its own min-of-iters over its
//   slice; the reported ms is the max over workers (uniform slices, so this
//   is the min full-iteration time; one-time spawn cost never enters a min).
// Build: rustc -O -C target-cpu=native rust_mt.rs -o rust_mt   (the naive recipe)
// Usage: rust_mt [N] [ITERS]
use std::time::Instant;

fn div_ceil(a: u64, b: u64) -> u64 {
    a.div_ceil(b).max(1)
}

fn thread_width() -> usize {
    if let Ok(v) = std::env::var("THREADS") {
        if let Ok(n) = v.parse::<usize>() {
            if n >= 1 {
                return n;
            }
        }
    }
    let hw = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let mut quota: Option<u64> = None;
    if let Ok(text) = std::fs::read_to_string("/sys/fs/cgroup/cpu.max") {
        // cgroup v2: "max 100000" or "<quota_us> <period_us>"
        let mut parts = text.split_whitespace();
        if let (Some(q), Some(p)) = (parts.next(), parts.next()) {
            if q != "max" {
                if let (Ok(q), Ok(p)) = (q.parse::<u64>(), p.parse::<u64>()) {
                    if p > 0 {
                        quota = Some(div_ceil(q, p));
                    }
                }
            }
        }
    }
    if quota.is_none() {
        // cgroup v1: quota -1 = uncapped
        if let (Ok(q), Ok(p)) = (
            std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us"),
            std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us"),
        ) {
            if let (Ok(q), Ok(p)) = (
                q.trim().parse::<i64>(),
                p.trim().parse::<u64>(),
            ) {
                if q > 0 && p > 0 {
                    quota = Some(div_ceil(q as u64, p));
                }
            }
        }
    }
    match quota {
        Some(q) => hw.min(q as usize).max(1),
        None => hw,
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let iters: usize = std::env::args().nth(2).unwrap().parse().unwrap();
    let nn = n * n;
    let a: Vec<f32> = (0..nn).map(|i| (((i * 7 + 13) % 101) as i32 - 50) as f32).collect();
    let b: Vec<f32> = (0..nn).map(|i| (((i * 7 + 57) % 101) as i32 - 50) as f32).collect();
    let mut c = vec![0f32; nn];
    let t = thread_width().min(n);
    // Disjoint row slices, t*n/T telescoping partition (same shape as cpp_mt).
    let mut rest: &mut [f32] = &mut c;
    let mut slices: Vec<(usize, &mut [f32])> = Vec::new();
    for w in 0..t {
        let r0 = w * n / t;
        let r1 = (w + 1) * n / t;
        let (head, tail) = rest.split_at_mut((r1 - r0) * n);
        slices.push((r0, head));
        rest = tail;
    }
    let best = std::thread::scope(|s| {
        let handles: Vec<_> = slices
            .into_iter()
            .map(|(r0, slice)| {
                let (a, b) = (&a, &b);
                s.spawn(move || {
                    let rows = slice.len() / n;
                    let mut my_best = f64::INFINITY;
                    for _ in 0..iters {
                        let t0 = Instant::now();
                        for i in 0..rows {
                            for j in 0..n {
                                let mut acc = 0f32;
                                for k in 0..n {
                                    acc += a[(r0 + i) * n + k] * b[k * n + j];
                                }
                                slice[i * n + j] = acc;
                            }
                        }
                        my_best = my_best.min(t0.elapsed().as_secs_f64());
                    }
                    my_best
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .fold(0.0f64, f64::max) // slowest worker's min = the min full-iteration time
    });
    let ms = best * 1e3;
    println!(
        "rust-mt N={} {:.4} ms {:.1} GFLOP/s c0={:.1} clast={:.1} T={}",
        n,
        ms,
        2.0 * (n as f64).powi(3) / (ms * 1e6),
        c[0],
        c[nn - 1],
        t
    );
}
