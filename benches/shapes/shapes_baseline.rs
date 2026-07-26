// CPU baselines for the fir/conv2d Mapal shapes, size-parameterized (S29 scale-up).
// Build: rustc -O -C target-cpu=native shapes_baseline.rs -o shapes_baseline_rs
use std::time::Instant;

fn cgroup_quota() -> Option<usize> {
    std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|text| {
            let mut fields = text.split_whitespace();
            let quota = fields.next()?;
            let period = fields.next()?.parse::<usize>().ok()?;
            if quota == "max" || period == 0 {
                return None;
            }
            quota
                .parse::<usize>()
                .ok()
                .map(|quota| quota.div_ceil(period).max(1))
        })
        .or_else(|| {
            let quota = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")
                .ok()?
                .trim()
                .parse::<isize>()
                .ok()?;
            let period = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us")
                .ok()?
                .trim()
                .parse::<usize>()
                .ok()?;
            (quota > 0 && period > 0).then(|| (quota as usize).div_ceil(period).max(1))
        })
}

fn thread_width() -> usize {
    if let Some(threads) = std::env::var("THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&threads| threads >= 1)
    {
        return threads;
    }
    let available = std::thread::available_parallelism().map(usize::from).unwrap_or(1);
    cgroup_quota().map_or(available, |quota| available.min(quota))
}

fn fir_range(x: &[f32], w: &[f32], y: &mut [f32], begin: usize) {
    for (offset, value) in y.iter_mut().enumerate() {
        let t = begin + offset;
        let mut acc = 0.0f32;
        for k in 0..w.len() {
            acc += w[k] * x[t + k];
        }
        *value = acc;
    }
}

fn conv_rows(img: &[f32], w: &[f32], out: &mut [f32], side: usize, row_begin: usize) {
    let stride = side + 2;
    for (row_offset, row) in out.chunks_exact_mut(side).enumerate() {
        let i = row_begin + row_offset;
        for (j, value) in row.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for k in 0..9 {
                acc += w[k] * img[(i + k / 3) * stride + j + k % 3];
            }
            *value = acc;
        }
    }
}

fn run_fir(multithreaded: bool, iters: usize, n: usize) {
    let x: Vec<f32> = (0..n + 63)
        .map(|t| (((t * 7 + 13) % 101) as i32 - 50) as f32)
        .collect();
    let w: Vec<f32> = (0..64)
        .map(|k| (((k * 5 + 3) % 31) as i32 - 15) as f32)
        .collect();
    let mut y = vec![0.0f32; n];
    let threads = thread_width().min(y.len());
    let chunk = y.len().div_ceil(threads);

    for _ in 0..iters {
        let start = Instant::now();
        if multithreaded {
            std::thread::scope(|scope| {
                for (lane, slice) in y.chunks_mut(chunk).enumerate() {
                    let x = &x;
                    let w = &w;
                    scope.spawn(move || fir_range(x, w, slice, lane * chunk));
                }
            });
        } else {
            fir_range(&x, &w, &mut y, 0);
        }
        println!("iter ms={:.6}", start.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{}\n{}", y[0], y[n - 1]);
}

fn run_conv(multithreaded: bool, iters: usize, side: usize) {
    let stride = side + 2;
    let img: Vec<f32> = (0..stride * stride)
        .map(|t| (((t * 7 + 13) % 101) as i32 - 50) as f32)
        .collect();
    let w: Vec<f32> = (0..9)
        .map(|k| (((k * 5 + 3) % 31) as i32 - 15) as f32)
        .collect();
    let mut out = vec![0.0f32; side * side];
    let threads = thread_width().min(side);
    let rows_per_thread = side.div_ceil(threads);
    let chunk = rows_per_thread * side;

    for _ in 0..iters {
        let start = Instant::now();
        if multithreaded {
            std::thread::scope(|scope| {
                for (lane, slice) in out.chunks_mut(chunk).enumerate() {
                    let img = &img;
                    let w = &w;
                    scope.spawn(move || conv_rows(img, w, slice, side, lane * rows_per_thread));
                }
            });
        } else {
            conv_rows(&img, &w, &mut out, side, 0);
        }
        println!("iter ms={:.6}", start.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{}\n{}", out[0], out[side * side - 1]);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if !(4..=5).contains(&args.len())
        || !matches!(args[1].as_str(), "fir" | "conv2d")
        || !matches!(args[2].as_str(), "1t" | "mt")
    {
        eprintln!("usage: {} <fir|conv2d> <1t|mt> <iters> [n|side]", args[0]);
        std::process::exit(2);
    }
    let iters = args[3]
        .parse::<usize>()
        .ok()
        .filter(|&value| value >= 1)
        .unwrap_or_else(|| {
            eprintln!("iters must be >= 1");
            std::process::exit(2);
        });
    let default_n = if args[1] == "fir" { 65536 } else { 512 };
    let n = args
        .get(4)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default_n);
    if args[1] == "fir" {
        run_fir(args[2] == "mt", iters, n);
    } else {
        run_conv(args[2] == "mt", iters, n);
    }
}
