// Rust baselines for the shape-ladder-v2 classes — saxpy (streaming), reduce
// (reduction), transpose (data movement), gather (irregular reads).
// Build: rustc -O -C target-cpu=native ladder2_baseline.rs -o ladder2_baseline_rs
//
// A line-for-line mirror of `ladder2_baseline.cpp`: the same generation
// formulas, the same kernel bodies, the same [0, n) split across threads, and
// the same `iter ms=` / probe output — so the two baselines are comparable to
// each other as well as to the Mapal leg. The reduce leg keeps the C++ leg's
// summation ORDER (per-thread partials, then a sequential fold over them),
// because f32 addition is not associative and a different order is a different
// answer, not a faster one.
use std::time::Instant;

fn thread_width() -> usize {
    if let Some(threads) = std::env::var("THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&threads| threads >= 1)
    {
        return threads;
    }
    std::thread::available_parallelism().map(usize::from).unwrap_or(1)
}

/// Split `out` across `width` lanes, handing each body its own slice and the
/// global index its slice starts at. One helper for every leg, so the threading
/// overhead is identical across shapes (the C++ `parallel_for`).
fn parallel_for<T, F>(multithreaded: bool, out: &mut [T], body: F)
where
    T: Send,
    F: Fn(&mut [T], usize) + Send + Sync,
{
    let width = if multithreaded { thread_width().min(out.len().max(1)) } else { 1 };
    if width <= 1 {
        body(out, 0);
        return;
    }
    let chunk = out.len().div_ceil(width);
    std::thread::scope(|scope| {
        for (lane, slice) in out.chunks_mut(chunk).enumerate() {
            let body = &body;
            scope.spawn(move || body(slice, lane * chunk));
        }
    });
}

fn run_iters<F: FnMut()>(iters: usize, mut work: F) {
    for _ in 0..iters {
        let start = Instant::now();
        work();
        println!("iter ms={:.6}", start.elapsed().as_secs_f64() * 1000.0);
    }
}

fn ramp(n: usize, mul: i64, add: i64, modulus: i64, sub: i64) -> Vec<f32> {
    (0..n)
        .map(|i| (((i as i64 * mul + add) % modulus) - sub) as f32)
        .collect()
}

// y[i] = 2.5*x[i] + y0[i] — streaming, bandwidth-bound.
fn run_saxpy(mt: bool, iters: usize, n: usize) {
    let x = ramp(n, 7, 13, 101, 50);
    let y0 = ramp(n, 5, 3, 31, 15);
    let mut y = vec![0.0f32; n];
    run_iters(iters, || {
        parallel_for(mt, &mut y, |slice, begin| {
            for (offset, value) in slice.iter_mut().enumerate() {
                let i = begin + offset;
                *value = 2.5f32 * x[i] + y0[i];
            }
        });
    });
    println!("{}\n{}", y[0], y[n - 1]);
}

// total = Σ x — reduction, no output array.
fn run_reduce(mt: bool, iters: usize, n: usize) {
    let x = ramp(n, 7, 13, 101, 50);
    let mut total = 0.0f32;
    run_iters(iters, || {
        let width = if mt { thread_width().min(n.max(1)) } else { 1 };
        let chunk = n.div_ceil(width);
        let partial: Vec<f32> = if width <= 1 {
            vec![x.iter().fold(0.0f32, |acc, &value| acc + value)]
        } else {
            std::thread::scope(|scope| {
                let handles: Vec<_> = x
                    .chunks(chunk)
                    .map(|slice| scope.spawn(move || slice.iter().fold(0.0f32, |acc, &v| acc + v)))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            })
        };
        total = partial.iter().fold(0.0f32, |acc, &value| acc + value);
    });
    println!("{total}");
}

// b[t] = a[(t % side)*side + t / side] — pure permutation, zero arithmetic.
fn run_transpose(mt: bool, iters: usize, side: usize) {
    let n = side * side;
    let a = ramp(n, 7, 13, 101, 50);
    let mut b = vec![0.0f32; n];
    run_iters(iters, || {
        parallel_for(mt, &mut b, |slice, begin| {
            for (offset, value) in slice.iter_mut().enumerate() {
                let t = begin + offset;
                *value = a[(t % side) * side + t / side];
            }
        });
    });
    println!("{}\n{}", b[0], b[n - 1]);
}

// y[i] = x[idx[i]] — data-dependent reads over a stride-1021 permutation.
fn run_gather(mt: bool, iters: usize, n: usize) {
    let x = ramp(n, 7, 13, 101, 50);
    let idx: Vec<usize> = (0..n)
        .map(|i| ((i as i64 * 1021 + 12347) % n as i64) as usize)
        .collect();
    let mut y = vec![0.0f32; n];
    run_iters(iters, || {
        parallel_for(mt, &mut y, |slice, begin| {
            for (offset, value) in slice.iter_mut().enumerate() {
                *value = x[idx[begin + offset]];
            }
        });
    });
    println!("{}\n{}", y[0], y[n - 1]);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !(4..=5).contains(&args.len())
        || !matches!(args[1].as_str(), "saxpy" | "reduce" | "transpose" | "gather")
        || !matches!(args[2].as_str(), "1t" | "mt")
    {
        eprintln!("usage: {} <saxpy|reduce|transpose|gather> <1t|mt> <iters> [size]", args[0]);
        std::process::exit(2);
    }
    let iters = args[3].parse::<usize>().ok().filter(|&v| v >= 1).unwrap_or_else(|| {
        eprintln!("iters must be >= 1");
        std::process::exit(2);
    });
    let default = if args[1] == "transpose" { 1024 } else { 1048576 };
    let size = args.get(4).and_then(|v| v.parse::<usize>().ok()).unwrap_or(default);
    let mt = args[2] == "mt";
    match args[1].as_str() {
        "saxpy" => run_saxpy(mt, iters, size),
        "reduce" => run_reduce(mt, iters, size),
        "transpose" => run_transpose(mt, iters, size),
        _ => run_gather(mt, iters, size),
    }
}
