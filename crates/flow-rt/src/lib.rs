//! `flow-rt` — the shared runtime seam (backend-llvm DESIGN §1, ADR-0020 §2).
//!
//! The emitted native program calls these `extern "C"` symbols for every
//! `Print` and every trap, so float formatting and trap/exit behaviour are the
//! interpreter oracle's *by construction*: each print renders via Rust
//! shortest-round-trip `Display`, exactly as `flow_interp::render` does (both
//! call the same formatter). `flow_trap` writes the defined message to stderr
//! and exits 101 (ADR-0013 / ADR-0020 §3).
//!
//! Stdout is flushed on every call — the differential harness reads pipes.

use std::{
    alloc::{self, Layout},
    cell::RefCell,
    collections::VecDeque,
    io::{self, Write},
    ptr,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicI64, AtomicPtr, AtomicU64, Ordering},
    },
    thread,
};

/// Render `v` via `Display` (== interp `render`) and write it to stdout,
/// appending a newline when `newline`; flush before returning.
fn emit(v: &dyn std::fmt::Display, newline: bool) {
    let mut out = io::stdout();
    // Ignore write errors: a broken pipe is the harness's business, not ours.
    if newline {
        let _ = writeln!(out, "{v}");
    } else {
        let _ = write!(out, "{v}");
    }
    let _ = out.flush();
}

/// Define one `flow_print_<ty>(v, newline)` extern. `Display` for every scalar
/// type is the shortest round-trip, matching interp `render` verbatim.
macro_rules! print_fn {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(v: $ty, newline: bool) {
            emit(&v, newline);
        }
    };
}

print_fn!(flow_print_i32, i32);
print_fn!(flow_print_i64, i64);
print_fn!(flow_print_u8, u8);
print_fn!(flow_print_bool, bool);
print_fn!(flow_print_f32, f32);
print_fn!(flow_print_f64, f64);

/// Print a UTF-8 string given as `(ptr, len)` (interp `render(Str) == s`).
///
/// # Safety
/// `ptr` must point to `len` initialised bytes of valid UTF-8 (the emitter only
/// ever passes a private `Str` global constant — never data, since lower
/// rejects strings-as-data, DESIGN §2).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_print_str(ptr: *const u8, len: usize, newline: bool) {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
    emit(&s, newline);
}

/// A defined trap: write the message to stderr and exit 101 (ADR-0013).
/// `0 = div_zero`, `1 = index_oob`; the emitter only ever passes these two.
#[unsafe(no_mangle)]
pub extern "C" fn flow_trap(kind: u32) -> ! {
    let msg = match kind {
        0 => "div_zero",
        1 => "index_oob",
        _ => "unknown", // ponytail: emitter passes only 0/1; total match anyway
    };
    // Flush any buffered stdout first (belt-and-braces; prints already flush).
    let _ = io::stdout().flush();
    eprintln!("flow trap: {msg}");
    std::process::exit(101);
}

/// One emitted task body.
///
/// `lo..hi` is the task's complete sequential range or one disjoint slice of a
/// split task. `frame` is the emitted function frame and remains valid through
/// `flow_par_finish`.
pub type TaskFn = extern "C" fn(lo: i64, hi: i64, frame: *mut u8);

const GRAIN: i64 = 4096;

#[derive(Clone, Copy)]
struct TaskDef {
    kind: u32,
    f: TaskFn,
    n: i64,
    rank: u32,
    /// Elements per slice, decided at COMPILE TIME (plan-s32 §2.5: the compiler
    /// sizes, the runtime assigns). `0` = none supplied, keep the legacy rule.
    slice_elems: i64,
    /// How many pieces per lane the compiler wants, given the region's reuse
    /// structure. `0` = none supplied. A read that is row-invariant pays nothing
    /// at a slice boundary and wants over-decomposition so stealing can balance;
    /// a sliding read re-pays its window overlap at every boundary and does not.
    /// The compiler knows which from the recorded coefficients; only the runtime
    /// knows how many lanes exist, so the two multiply here.
    oversub: u32,
    /// Lanes this dispatch should spread across, decided at compile time.
    /// `0` = none supplied, use the whole pool. Stealing is deliberately NOT
    /// confined to them: an idle lane helping is the runtime's assignment
    /// freedom, which is the half of the decision the compiler cannot make.
    width: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskState {
    Building,
    Waiting,
    Running,
    Done,
}

struct Task {
    def: Option<TaskDef>,
    dependents: Vec<usize>,
    deps_left: u32,
    state: TaskState,
    slices_left: usize,
    watermark: AtomicI64,
    pinned: bool,
}

struct RunState {
    tasks: Vec<Task>,
    launched: bool,
    remaining: usize,
    epoch: u64,
}

struct Run {
    pool: Arc<Pool>,
    frame: AtomicPtr<u8>,
    trap: AtomicU64,
    state: Mutex<RunState>,
    changed: Condvar,
}

/// Opaque handle for one emitted task DAG invocation.
///
/// The emitter owns this handle from `flow_par_begin` through exactly one
/// `flow_par_finish`; all registration happens before `flow_par_launch`.
#[repr(C)]
pub struct FlowParRun {
    run: Arc<Run>,
}

struct Work {
    run: Arc<Run>,
    task: usize,
    lo: i64,
    hi: i64,
}

struct Pool {
    threads: usize,
    queues: Vec<Mutex<VecDeque<Work>>>,
    wake_epoch: Mutex<u64>,
    wake: Condvar,
}

#[derive(Clone, Copy)]
enum Placement {
    Seed,
    Local(usize),
}

/// How many pieces a dispatch is cut into, and therefore whether work stealing
/// has anything to steal.
///
/// **The legacy rule (`slice_elems == 0`) is `ceil(n/GRAIN).min(threads)` —
/// exactly one piece per worker.** That leaves the deques empty, so a fast lane
/// finishing early cannot help a slow one: the dispatch ends when the slowest
/// piece ends, and on an asymmetric machine (10 P + 4 E) that is the whole wave
/// waiting on an E-core. Measured cost: matmul512 at 14 lanes went 0.778 ms to
/// 0.463 ms purely from cutting more pieces (plan-s32 §1(c)).
///
/// A compile-time `slice_elems` overrides it. It is NOT simply "smaller is
/// better" — finer slicing re-pays every boundary's reuse halo and shortens the
/// run over which a packed panel amortises, which is why the same 8x
/// over-decomposition that wins 68% on matmul512 costs conv2d ~20% (§2.6). The
/// size is a per-region decision; this function only applies it.
/// Experiment lever: override every dispatch's slice size (`FLOW_SLICE=<elems>`).
///
/// Not part of the design — the compiler is what sets slice sizes (plan-s32
/// §2.5). This exists so the sizing model can be swept and falsified on real
/// kernels *before* the deduction is written, and so ADR-0034's autotuner has a
/// knob to search. `FLOW_PAR`'s sibling, with the same status.
fn slice_override() -> i64 {
    static OVERRIDE: OnceLock<i64> = OnceLock::new();
    *OVERRIDE.get_or_init(|| {
        std::env::var("FLOW_SLICE")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(0)
    })
}

fn slice_ranges(def: TaskDef, threads: usize) -> Vec<(i64, i64)> {
    // The lever forces an EXACT slice size, bypassing the floor/oversub rule —
    // that is what makes it a probe of the sizing model rather than of the rule.
    let forced = slice_override();
    let def = if forced > 0 {
        TaskDef {
            slice_elems: forced,
            oversub: u32::MAX,
            ..def
        }
    } else {
        def
    };
    let slices = if def.kind == 0 {
        1
    } else if def.n == 0 {
        0
    } else if def.slice_elems > 0 {
        // `slice_elems` is a FLOOR, not an exact size: cutting below it drops
        // the dispatch off the register-blocked path onto the TI=1 fallback,
        // which measured 7x on matmul1024 (plan-s32 §2.6). Within that floor the
        // runtime picks the count, because lane count is the one input the
        // compiler does not have.
        // Cut EQUAL numbers of blocks, never an arbitrary count.
        //
        // A count that does not divide the block total leaves slices of
        // different sizes, and the cost is out of all proportion to the 1.25x
        // size ratio it creates. Measured on matmul1024 at 14 lanes (256
        // blocks): every count that divides 256 runs 2.49-2.73 ms (32, 64, 128,
        // 256) while neighbouring counts that do not run 3.3-5.8 (41, 43, 47,
        // 52, 57) — 1.8x for being ragged. Deriving blocks-per-slice first and
        // the count from it makes ragged counts unreachable.
        let blocks = usize::try_from((def.n as u64).div_ceil(def.slice_elems as u64))
            .unwrap_or(usize::MAX)
            .max(1);
        let wanted = threads.saturating_mul(def.oversub.max(1) as usize).max(1);
        let per = (blocks / wanted).max(1);
        blocks.div_ceil(per)
    } else {
        usize::try_from((def.n as u64).div_ceil(GRAIN as u64))
            .unwrap_or(usize::MAX)
            .min(threads)
    };
    // Cut on the region's quantum, not on equal element counts.
    //
    // Equal division is what the count-first rule did, and it silently destroys
    // whatever alignment the size asked for: 1048576 over 52 slices is 20164.9
    // elements, so every boundary lands mid-row, every slice starts with a
    // partial register block, and the blocked kernel degenerates at both ends of
    // every piece. Measured on matmul1024 at 14 lanes: slice counts that happen
    // to divide n evenly (32, 64, 128) run 2.5-2.6 ms, while neighbouring counts
    // that do not (43, 52, 57) run 4.4-5.8 — a 2x swing with no relation to how
    // many pieces there are. `slice_elems` carries the quantum (TI * c, one
    // whole register block of rows), so honour it.
    let quantum = def.slice_elems.max(1);
    let mut ranges = Vec::with_capacity(slices);
    let mut lo = 0;
    for slice in 1..=slices {
        let hi = if slice == slices {
            def.n
        } else {
            let target = (slice as i64).saturating_mul(def.n) / slices as i64;
            (target / quantum * quantum).clamp(lo, def.n)
        };
        // Every slice is emitted, empty ones included: a `Seq` task carries
        // n = 0 and must still run exactly once.
        ranges.push((lo, hi));
        lo = hi;
    }
    ranges
}

#[derive(Clone, Copy)]
struct CurrentRun {
    run: *const Run,
    task: usize,
}

thread_local! {
    static CURRENT_RUNS: RefCell<Vec<CurrentRun>> = const { RefCell::new(Vec::new()) };
    static WORKER_LANE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap()
}

fn parse_cpu_max(text: &str) -> Option<usize> {
    let mut fields = text.split_whitespace();
    let quota = fields.next()?;
    let period = fields.next()?;
    if fields.next().is_some() || quota == "max" {
        return None;
    }
    let quota = quota.parse::<u64>().ok()?;
    let period = period.parse::<u64>().ok()?;
    (period > 0)
        .then(|| quota.div_ceil(period).max(1))
        .and_then(|threads| usize::try_from(threads).ok())
}

fn parse_cfs(quota: &str, period: &str) -> Option<usize> {
    let quota = quota.trim().parse::<i64>().ok()?;
    let period = period.trim().parse::<u64>().ok()?;
    if quota < 0 || period == 0 {
        return None;
    }
    usize::try_from((quota as u64).div_ceil(period).max(1)).ok()
}

fn cgroup_quota() -> Option<usize> {
    std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|text| parse_cpu_max(&text))
        .or_else(|| {
            let quota = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").ok()?;
            let period = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us").ok()?;
            parse_cfs(&quota, &period)
        })
}

fn thread_count(flow_par: Option<&str>, available: usize, quota: Option<usize>) -> usize {
    flow_par
        .and_then(|value| value.parse().ok())
        .filter(|&value| value >= 1)
        .unwrap_or_else(|| quota.map_or(available, |quota| available.min(quota)).max(1))
}

fn configured_threads() -> usize {
    let flow_par = std::env::var("FLOW_PAR").ok();
    let available = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    thread_count(flow_par.as_deref(), available, cgroup_quota())
}

fn global_pool() -> Arc<Pool> {
    static POOL: OnceLock<Arc<Pool>> = OnceLock::new();
    POOL.get_or_init(|| Pool::new(configured_threads())).clone()
}

static PERF_START: Mutex<Option<std::time::Instant>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn flow_perf_begin() {
    if configured_threads() > 1 {
        drop(global_pool());
    }
    *lock(&PERF_START) = Some(std::time::Instant::now());
}

#[unsafe(no_mangle)]
pub extern "C" fn flow_perf_end() {
    let elapsed = lock(&PERF_START)
        .take()
        .expect("flow_perf_end called without flow_perf_begin")
        .elapsed()
        .as_secs_f64()
        * 1000.0;
    emit(&format_args!("FLOW_PERF total ms={elapsed:.4}"), true);
}

/// The process-lifetime monotonic epoch for `flow_time_ms` (plan-time-builtin):
/// one `Instant` shared by every call in the process, so two calls are
/// non-decreasing and a difference is real elapsed milliseconds.
static TIME_EPOCH: OnceLock<std::time::Instant> = OnceLock::new();

/// The `time` builtin's runtime seam: milliseconds (f64) from the same
/// monotonic clock (`std::time::Instant`) as `flow_perf_begin`/`flow_perf_end`,
/// measured against the process-lifetime [`TIME_EPOCH`].
#[unsafe(no_mangle)]
pub extern "C" fn flow_time_ms() -> f64 {
    TIME_EPOCH
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
}

/// The heap-lowering arena (plan-s29 emission item 4): every block handed out
/// by [`flow_rt_alloc`], kept as `(address, layout)` so [`flow_rt_free_all`]
/// can release it. Addresses are stored as `usize` — a raw pointer is not
/// `Send`, and the arena is shared across the worker pool.
///
/// ponytail: ONE global mutex and a free-everything teardown. The emitter's
/// contract makes that enough — it only lowers the *entry* function's own
/// big blocks, so allocation happens a handful of times in the program's
/// prologue and never in a hot loop, and the single teardown sits after the
/// last reader (`flow_par_finish` / fn end). Ceiling: a program that wanted
/// per-allocation lifetimes, or heap-lowered a repeatedly-called fn, needs
/// `flow_rt_free(ptr)` + an emitter-side last-use point instead.
static ARENA: Mutex<Vec<(usize, Layout)>> = Mutex::new(Vec::new());

/// Allocate one arena block of `bytes` at `align`, uninitialised — `alloca`
/// storage is not zeroed either, and every emitted consumer writes before it
/// reads. Aborts on allocation failure (the emitted program has no path to
/// handle OOM; `flow_trap`'s exit-101 contract is for *language* traps).
///
/// The block comes back **resident**: see [`reside`] for why that is part of
/// the contract and not an optimisation (plan-s33 CR-1).
#[unsafe(no_mangle)]
pub extern "C" fn flow_rt_alloc(bytes: i64, align: i64) -> *mut u8 {
    let layout = Layout::from_size_align(bytes as usize, align as usize)
        .expect("flow_rt_alloc: emitter passes a valid size/alignment pair");
    // Zero-sized blocks would alias; the emitter never asks (the threshold is
    // 256 KB) but `alloc` requires size > 0, so keep the guard honest.
    assert!(layout.size() > 0, "flow_rt_alloc: zero-sized block");
    let ptr = unsafe { alloc::alloc(layout) };
    if ptr.is_null() {
        alloc::handle_alloc_error(layout);
    }
    unsafe { reside(ptr, layout.size()) };
    lock(&ARENA).push((ptr as usize, layout));
    ptr
}

/// Force physical residence of `[ptr, ptr + bytes)` — plan-s33's `reside`
/// morphism, and the reason `flow_rt_alloc`'s result is a *materialised* block
/// rather than only a reserved address range.
///
/// A large `alloc` is served by `mmap`, which delivers **no** physical memory —
/// only a promise that touching the range will produce zeroed pages. The first
/// store to each page therefore traps, and the kernel must find and zero a page
/// (2 MiB at a time under transparent huge pages) before the store completes.
/// The emitter allocates in the entry-block prologue but the tasks store later,
/// so without this those faults land inside whatever region happens to write
/// first — including a `() -> time` bracket, which then charges page-zeroing to
/// the kernel it was meant to be timing. Measured at ~0.10 ms warm / 0.30 ms
/// cold per 4 MB, which is what made conv2d look 1.55x slower than naive C++
/// when its kernel is in fact faster (`docs/performance/conv2d-per-core-gap.md`).
///
/// This costs the program nothing overall: the same pages are faulted and zeroed
/// either way. It only decides *when*, and so what a clock can see.
///
/// # Safety
/// `ptr` must be valid for writes over `bytes`, and uninitialised — every byte
/// written here is a `0` into a page the kernel has just zeroed anyway, so no
/// caller-visible value changes.
unsafe fn reside(ptr: *mut u8, bytes: usize) {
    // ponytail: one byte per 4 KiB, NOT a memset. The fault's own zeroing IS
    // the initialisation, so a memset would zero every page a second time —
    // 2x the memory traffic, ~15 ms wasted on a 64 MB matmul frame. Under 2 MiB
    // huge pages this touches ~512x more often than strictly needed, which is
    // ~4k stores per 16 MB: not worth probing the page size to avoid.
    // Ceiling: assumes pages are at least 4 KiB (true on every target we emit
    // for). On a multi-socket host this also pins every page to the touching
    // thread's NUMA node; if a parallel leg ever regresses there, the fix is to
    // make this lane-aware rather than to drop it (plan-s33 §5).
    let mut offset = 0usize;
    while offset < bytes {
        // `write_volatile` so LLVM cannot discard the loop as dead stores.
        unsafe { ptr.add(offset).write_volatile(0) };
        offset += 4096;
    }
}

/// Release every arena block. Emitted once per entry function, after the last
/// point that can read arena memory (post-`flow_par_finish` in the parallel
/// flavor, immediately before `ret` otherwise).
#[unsafe(no_mangle)]
pub extern "C" fn flow_rt_free_all() {
    for (addr, layout) in lock(&ARENA).drain(..) {
        unsafe { alloc::dealloc(addr as *mut u8, layout) };
    }
}

impl Pool {
    fn new(threads: usize) -> Arc<Self> {
        let workers = if threads == 1 { 0 } else { threads };
        Self::with_workers(threads, workers)
    }

    fn with_workers(threads: usize, workers: usize) -> Arc<Self> {
        assert!(threads >= 1);
        assert!(workers <= threads);
        let pool = Arc::new(Self {
            threads,
            queues: (0..threads).map(|_| Mutex::new(VecDeque::new())).collect(),
            wake_epoch: Mutex::new(0),
            wake: Condvar::new(),
        });

        // FLOW_PAR=1 is the thread-free sequential lever. With a real pool the
        // host remains an additional help-first executor at joins.
        for lane in 0..workers {
            let worker_pool = Arc::clone(&pool);
            thread::Builder::new()
                .name(format!("flow-par-{lane}"))
                .spawn(move || {
                    WORKER_LANE.set(Some(lane));
                    worker_pool.worker_loop(lane);
                })
                .expect("failed to create flow parallel worker");
        }
        pool
    }

    fn enqueue(&self, work: Vec<(usize, Work)>) {
        if work.is_empty() {
            return;
        }
        for (lane, item) in work {
            lock(&self.queues[lane]).push_back(item);
        }
        let mut epoch = lock(&self.wake_epoch);
        *epoch = epoch.wrapping_add(1);
        drop(epoch);
        self.wake.notify_all();
    }

    fn take_any(&self, lane: usize) -> Option<Work> {
        if let Some(work) = lock(&self.queues[lane]).pop_front() {
            return Some(work);
        }
        for offset in 1..self.threads {
            let victim = (lane + offset) % self.threads;
            if let Some(work) = lock(&self.queues[victim]).pop_back() {
                return Some(work);
            }
        }
        None
    }

    fn take_for(&self, run: &Arc<Run>, lane: usize) -> Option<Work> {
        if self.threads == 1 {
            let mut queue = lock(&self.queues[0]);
            let position = queue
                .iter()
                .enumerate()
                .filter(|(_, work)| Arc::ptr_eq(&work.run, run))
                .min_by_key(|(_, work)| (work.task, work.lo))
                .map(|(position, _)| position)?;
            return queue.remove(position);
        }

        let mut own = lock(&self.queues[lane]);
        if let Some(position) = own.iter().position(|work| Arc::ptr_eq(&work.run, run)) {
            return own.remove(position);
        }
        drop(own);

        for offset in 1..self.threads {
            let victim = (lane + offset) % self.threads;
            let mut queue = lock(&self.queues[victim]);
            if let Some(position) = queue.iter().rposition(|work| Arc::ptr_eq(&work.run, run)) {
                return queue.remove(position);
            }
        }
        None
    }

    fn worker_loop(&self, lane: usize) {
        loop {
            if let Some(work) = self.take_any(lane) {
                execute(work, lane);
                continue;
            }

            let mut epoch = lock(&self.wake_epoch);
            if let Some(work) = self.take_any(lane) {
                drop(epoch);
                execute(work, lane);
                continue;
            }
            let seen = *epoch;
            while *epoch == seen {
                epoch = self.wake.wait(epoch).unwrap();
            }
        }
    }
}

impl Run {
    fn new(n_tasks: u32, pool: Arc<Pool>) -> Arc<Self> {
        Arc::new(Self {
            pool,
            frame: AtomicPtr::new(ptr::null_mut()),
            trap: AtomicU64::new(0),
            state: Mutex::new(RunState {
                tasks: (0..n_tasks)
                    .map(|_| Task {
                        def: None,
                        dependents: Vec::new(),
                        deps_left: 0,
                        state: TaskState::Building,
                        slices_left: 0,
                        watermark: AtomicI64::new(-1),
                        pinned: false,
                    })
                    .collect(),
                launched: false,
                remaining: 0,
                epoch: 0,
            }),
            changed: Condvar::new(),
        })
    }

    fn bump(&self) {
        let mut state = lock(&self.state);
        state.epoch = state.epoch.wrapping_add(1);
        drop(state);
        self.changed.notify_all();
    }

    fn schedule(self: &Arc<Self>, mut ready: Vec<usize>, placement: Placement) {
        let mut queued = Vec::new();
        let mut cursor = 0;

        while !ready.is_empty() {
            {
                let state = lock(&self.state);
                match placement {
                    Placement::Seed if self.pool.threads > 1 => ready.sort_by(|&a_idx, &b_idx| {
                        let a = state.tasks[a_idx].def.unwrap();
                        let b = state.tasks[b_idx].def.unwrap();
                        b.rank.cmp(&a.rank).then_with(|| a_idx.cmp(&b_idx))
                    }),
                    _ => ready.sort_unstable(),
                }
            }

            let batch = std::mem::take(&mut ready);
            for task_idx in batch {
                let (ranges, unlocked, width) = {
                    let mut state = lock(&self.state);
                    if state.tasks[task_idx].state != TaskState::Waiting
                        || state.tasks[task_idx].deps_left != 0
                    {
                        continue;
                    }
                    let def = state.tasks[task_idx].def.unwrap();
                    let ranges = slice_ranges(def, self.pool.threads);

                    if ranges.is_empty() {
                        state.tasks[task_idx].state = TaskState::Done;
                        state.remaining -= 1;
                        state.epoch = state.epoch.wrapping_add(1);
                        let dependents = state.tasks[task_idx].dependents.clone();
                        let mut unlocked = Vec::new();
                        for dependent in dependents {
                            state.tasks[dependent].deps_left -= 1;
                            if state.tasks[dependent].deps_left == 0 {
                                unlocked.push(dependent);
                            }
                        }
                        (Vec::new(), unlocked, 0)
                    } else {
                        state.tasks[task_idx].slices_left = ranges.len();
                        (ranges, Vec::new(), def.width)
                    }
                };

                ready.extend(unlocked);
                for (lo, hi) in ranges {
                    let lane = match placement {
                        Placement::Seed => {
                            // plan-s32 step 1: lay the dispatch across the lanes
                            // the compiler asked for. Stealing still lets any
                            // idle lane help — placement is a starting point,
                            // not a fence, because which lane is free is the one
                            // thing only the runtime can see.
                            let span = if width == 0 {
                                self.pool.threads
                            } else {
                                (width as usize).clamp(1, self.pool.threads)
                            };
                            let lane = cursor % span;
                            cursor += 1;
                            lane
                        }
                        Placement::Local(lane) => lane,
                    };
                    queued.push((
                        lane,
                        Work {
                            run: Arc::clone(self),
                            task: task_idx,
                            lo,
                            hi,
                        },
                    ));
                }
            }
        }

        if !queued.is_empty() {
            self.pool.enqueue(queued);
            self.bump();
        } else {
            self.changed.notify_all();
        }
    }

    fn complete_slice(self: &Arc<Self>, task_idx: usize, lane: usize) {
        let mut ready = Vec::new();
        let terminal = {
            let mut state = lock(&self.state);
            let task = &mut state.tasks[task_idx];
            if task.state == TaskState::Done {
                return;
            }
            task.slices_left -= 1;
            if task.slices_left != 0 {
                false
            } else {
                task.state = TaskState::Done;
                state.remaining -= 1;
                let dependents = state.tasks[task_idx].dependents.clone();
                for dependent in dependents {
                    state.tasks[dependent].deps_left -= 1;
                    // Pinned dependents are never queued: the host spine runs
                    // them via flow_par_run_pinned; the epoch bump below wakes
                    // any host wait on this completion.
                    if state.tasks[dependent].deps_left == 0 && !state.tasks[dependent].pinned {
                        ready.push(dependent);
                    }
                }
                state.epoch = state.epoch.wrapping_add(1);
                true
            }
        };

        if terminal {
            self.changed.notify_all();
        }
        if !ready.is_empty() {
            self.schedule(ready, Placement::Local(lane));
        }
    }

    fn help_until(self: &Arc<Self>, done: impl Fn(&RunState) -> bool) {
        let lane = WORKER_LANE
            .with(|lane| lane.get())
            .unwrap_or_else(|| self.pool.threads - 1);
        loop {
            let seen = {
                let state = lock(&self.state);
                if done(&state) {
                    return;
                }
                state.epoch
            };

            if let Some(work) = self.pool.take_for(self, lane) {
                execute(work, lane);
                continue;
            }

            let mut state = lock(&self.state);
            if done(&state) {
                return;
            }
            while state.epoch == seen {
                state = self.changed.wait(state).unwrap();
            }
        }
    }
}

fn execute(work: Work, lane: usize) {
    let task = {
        let mut state = lock(&work.run.state);
        let task = &mut state.tasks[work.task];
        assert!(!task.pinned, "pinned task reached a pool worker");
        match task.state {
            TaskState::Waiting => task.state = TaskState::Running,
            TaskState::Running => {}
            TaskState::Done => return,
            TaskState::Building => panic!("flow scheduler queued an unready task"),
        }
        task.def.unwrap()
    };

    CURRENT_RUNS.with(|runs| {
        runs.borrow_mut().push(CurrentRun {
            run: Arc::as_ptr(&work.run),
            task: work.task,
        });
    });
    (task.f)(work.lo, work.hi, work.run.frame.load(Ordering::Acquire));
    CURRENT_RUNS.with(|runs| {
        runs.borrow_mut().pop().unwrap();
    });

    work.run.complete_slice(work.task, lane);
}

unsafe fn run_handle<'a>(handle: *mut FlowParRun) -> &'a FlowParRun {
    assert!(!handle.is_null(), "null flow parallel run");
    unsafe { &*handle }
}

fn allocate_run(n_tasks: u32, pool: Arc<Pool>) -> *mut FlowParRun {
    Box::into_raw(Box::new(FlowParRun {
        run: Run::new(n_tasks, pool),
    }))
}

/// Allocate a task-DAG run containing exactly `n_tasks` indexed task slots.
///
/// The returned handle must be configured, launched, and passed exactly once to
/// `flow_par_finish`.
#[unsafe(no_mangle)]
pub extern "C" fn flow_par_begin(n_tasks: u32) -> *mut FlowParRun {
    allocate_run(n_tasks, global_pool())
}

/// Register task `idx` before launch.
///
/// `kind == 0` executes exactly one `f(0, n, frame)` call. `kind == 1`
/// executes disjoint slices covering `[0, n)` exactly once, using at most
/// `min(T, ceil(n / 4096))` calls.
///
/// # Safety
/// `handle` must be a live handle from `flow_par_begin`, and every task index
/// must be registered exactly once before launch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_task(
    handle: *mut FlowParRun,
    idx: u32,
    kind: u32,
    f: TaskFn,
    n: i64,
    rank: u32,
    slice_elems: i64,
    oversub: u32,
    width: u32,
) {
    let run = &unsafe { run_handle(handle) }.run;
    let mut state = lock(&run.state);
    assert!(!state.launched, "cannot register a task after launch");
    let task = &mut state.tasks[idx as usize];
    assert!(task.def.is_none(), "task registered twice");
    assert!(kind <= 1, "unknown parallel task kind");
    assert!(
        kind == 0 || n >= 0,
        "split task length must be non-negative"
    );
    task.def = Some(TaskDef {
        kind,
        f,
        n,
        rank,
        slice_elems,
        oversub,
        width,
    });
}

/// Add a dependency edge: `after` cannot execute before `before` completes.
///
/// # Safety
/// `handle` must be live, both indices must be in range, and registration must
/// still be open before `flow_par_launch`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_dep(handle: *mut FlowParRun, before: u32, after: u32) {
    let run = &unsafe { run_handle(handle) }.run;
    let mut state = lock(&run.state);
    assert!(!state.launched, "cannot register a dependency after launch");
    assert_ne!(before, after, "task cannot depend on itself");
    state.tasks[before as usize].dependents.push(after as usize);
    state.tasks[after as usize].deps_left = state.tasks[after as usize]
        .deps_left
        .checked_add(1)
        .expect("task dependency count overflow");
}

/// Seal the registered DAG, attach its frame, and seed all initially-ready
/// tasks. This returns after seeding; workers and later help-first waits execute
/// the work.
///
/// # Safety
/// `handle` must be live and unlaunched, every task slot must be registered,
/// and `frame` must remain valid through `flow_par_finish`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_launch(handle: *mut FlowParRun, frame: *mut u8) {
    let run = Arc::clone(&unsafe { run_handle(handle) }.run);
    run.frame.store(frame, Ordering::Release);
    let ready = {
        let mut state = lock(&run.state);
        assert!(!state.launched, "parallel run launched twice");
        assert!(
            state.tasks.iter().all(|task| task.def.is_some()),
            "parallel run has unregistered tasks"
        );
        state.launched = true;
        state.remaining = state.tasks.len();
        for task in &mut state.tasks {
            task.state = TaskState::Waiting;
        }
        state
            .tasks
            .iter()
            .enumerate()
            .filter_map(|(idx, task)| (task.deps_left == 0 && !task.pinned).then_some(idx))
            .collect()
    };
    run.schedule(ready, Placement::Seed);
}

/// Mark task `idx` as host-pinned before launch: the scheduler never queues it;
/// the host spine executes it via `flow_par_run_pinned` at its topo position.
///
/// # Safety
/// `handle` must be live, `idx` in range, and registration still open.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_pin(handle: *mut FlowParRun, idx: u32) {
    let run = &unsafe { run_handle(handle) }.run;
    let mut state = lock(&run.state);
    assert!(!state.launched, "cannot pin a task after launch");
    state.tasks[idx as usize].pinned = true;
}

/// Help this run until every packed wait entry is satisfied.
///
/// The calling thread executes or steals ready work from this run instead of
/// blocking behind workers.
///
/// # Safety
/// `handle` must be live and launched. When `len != 0`, `entries` must point to
/// `len` packed `(task_idx << 32 | threshold)` entries with valid task indices.
/// `u32::MAX` requires completion; other thresholds also accept a published
/// watermark at or above the threshold.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_wait(handle: *mut FlowParRun, entries: *const u64, len: u32) {
    let run = Arc::clone(&unsafe { run_handle(handle) }.run);
    let entries = if len == 0 {
        &[]
    } else {
        assert!(!entries.is_null(), "null parallel wait entry list");
        unsafe { std::slice::from_raw_parts(entries, len as usize) }
    };
    {
        let state = lock(&run.state);
        assert!(state.launched, "cannot wait before launch");
        assert!(
            entries
                .iter()
                .all(|&entry| (entry >> 32) < state.tasks.len() as u64),
            "parallel wait task index out of range"
        );
    }
    run.help_until(|state| {
        entries.iter().all(|&entry| {
            let task = &state.tasks[(entry >> 32) as usize];
            let threshold = entry as u32;
            task.state == TaskState::Done
                || (threshold != u32::MAX
                    && task.watermark.load(Ordering::Acquire) >= i64::from(threshold))
        })
    });
}

fn check_trap(run: &Run, topo: i64) {
    let trap = run.trap.load(Ordering::Acquire);
    if trap != 0 && ((trap >> 32) as i64) < topo {
        flow_trap((trap as u32) - 1);
    }
}

/// Observe the run's deterministic trap flag at an oracle checkpoint.
///
/// A recorded trap with `topo_idx < topo` is delivered through `flow_trap`;
/// `i64::MAX` therefore checks every representable task trap.
///
/// # Safety
/// `handle` must be live and launched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_check(handle: *mut FlowParRun, topo: i64) {
    check_trap(&unsafe { run_handle(handle) }.run, topo);
}

/// Record a trap from inside the currently-executing emitted task.
///
/// The innermost nested run receives the flag. The runtime records only the
/// minimum topo index and never exits from a worker.
#[unsafe(no_mangle)]
pub extern "C" fn flow_par_trap(topo: i64, kind: u32) {
    assert!(
        (0..=u32::MAX as i64).contains(&topo),
        "parallel trap topo index out of range"
    );
    assert_ne!(kind, u32::MAX, "parallel trap kind cannot use the sentinel");
    CURRENT_RUNS.with(|runs| {
        let runs = runs.borrow();
        let current = runs
            .last()
            .expect("flow_par_trap called outside a parallel task");
        let run = unsafe { &*current.run };
        let new = ((topo as u64) << 32) | u64::from(kind + 1);
        let mut old = run.trap.load(Ordering::Acquire);
        loop {
            if old != 0 && old >> 32 <= topo as u64 {
                return;
            }
            match run
                .trap
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return,
                Err(actual) => old = actual,
            }
        }
    });
}

/// Publish that the current scalar task has decided every trap site through
/// `topo`.
#[unsafe(no_mangle)]
pub extern "C" fn flow_par_watermark(topo: i64) {
    assert!(
        (0..u32::MAX as i64).contains(&topo),
        "parallel watermark topo index out of range"
    );
    let current = CURRENT_RUNS.with(|runs| {
        *runs
            .borrow()
            .last()
            .expect("flow_par_watermark called outside a parallel task")
    });
    let run = unsafe { &*current.run };
    let mut state = lock(&run.state);
    let task = &state.tasks[current.task];
    assert_eq!(
        task.def.unwrap().kind,
        0,
        "only scalar tasks publish watermarks"
    );
    assert_eq!(
        task.state,
        TaskState::Running,
        "watermark task is not running"
    );
    task.watermark.store(topo, Ordering::Release);
    state.epoch = state.epoch.wrapping_add(1);
    drop(state);
    run.changed.notify_all();
}

/// Execute one ready task inline on the calling host-spine thread.
///
/// # Safety
/// `handle` must be live and launched, and `idx` must name an unstarted task
/// whose dependencies have completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_run_pinned(handle: *mut FlowParRun, idx: u32) {
    let run = Arc::clone(&unsafe { run_handle(handle) }.run);
    let task_idx = idx as usize;
    let (def, ranges) = {
        let mut state = lock(&run.state);
        assert!(state.launched, "cannot run a pinned task before launch");
        let task = &mut state.tasks[task_idx];
        let def = task.def.expect("pinned task is not registered");
        assert!(task.pinned, "flow_par_run_pinned on an unpinned task");
        assert_eq!(task.deps_left, 0, "pinned task dependencies are incomplete");
        assert!(
            task.state == TaskState::Waiting,
            "pinned task has already started"
        );
        task.state = TaskState::Running;
        task.slices_left = 1;
        (def, slice_ranges(def, run.pool.threads))
    };

    CURRENT_RUNS.with(|runs| {
        runs.borrow_mut().push(CurrentRun {
            run: Arc::as_ptr(&run),
            task: task_idx,
        });
    });
    let frame = run.frame.load(Ordering::Acquire);
    for (lo, hi) in ranges {
        (def.f)(lo, hi, frame);
    }
    CURRENT_RUNS.with(|runs| {
        runs.borrow_mut().pop().unwrap();
    });

    let lane = WORKER_LANE
        .with(|lane| lane.get())
        .unwrap_or(run.pool.threads - 1);
    run.complete_slice(task_idx, lane);
}

/// Help until every task is complete, deliver any recorded trap, and free the
/// run.
///
/// # Safety
/// `handle` must be a live launched handle and must not be used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_par_finish(handle: *mut FlowParRun) {
    assert!(!handle.is_null(), "null flow parallel run");
    let handle = unsafe { Box::from_raw(handle) };
    let run = Arc::clone(&handle.run);
    assert!(lock(&run.state).launched, "cannot finish before launch");
    run.help_until(|state| state.remaining == 0);
    check_trap(&run, i64::MAX);
    drop(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_interp::render;
    use flow_ir::Value;
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicBool, AtomicU8, AtomicU32},
            mpsc,
        },
        time::{Duration, Instant},
    };

    /// The arena hands out usable, distinct storage and releases it. Writing
    /// the whole block is the point: a short allocation would corrupt the heap
    /// silently, which is exactly what the emitter's size arithmetic risks.
    ///
    /// The third block covers `reside` (plan-s33 CR-1): it is larger than one
    /// page and deliberately **not** a page multiple, so the pre-touch loop has
    /// to walk many pages and stop inside the final partial one. A stride or
    /// bound slip there writes past the block and corrupts the allocator, which
    /// the writes-then-free below turn into a hard failure. Residence *itself*
    /// is not observable from Rust — it is pinned on the box by differencing
    /// `perf stat -e page-faults` (plan-s33 acceptance 1).
    #[test]
    fn arena_alloc_is_usable_and_freed() {
        let a = flow_rt_alloc(4096, 64);
        let b = flow_rt_alloc(4096, 64);
        let big_bytes = 1024 * 1024 + 1; // > 1 page, not a page multiple
        let c = flow_rt_alloc(big_bytes as i64, 64);
        assert!(!a.is_null() && !b.is_null() && !c.is_null());
        assert!(a != b && b != c && a != c);
        assert_eq!(a as usize % 64, 0, "requested alignment honoured");
        assert_eq!(c as usize % 64, 0, "requested alignment honoured");
        unsafe {
            // `reside` writes a 0 per page, so a freshly handed-out block reads
            // as zero at every page boundary it touched, including the last one.
            assert_eq!(*c, 0);
            assert_eq!(*c.add(big_bytes - 1 - (big_bytes - 1) % 4096), 0);
            ptr::write_bytes(a, 0xAB, 4096);
            ptr::write_bytes(b, 0xCD, 4096);
            ptr::write_bytes(c, 0xEF, big_bytes);
            assert_eq!(*a.add(4095), 0xAB);
            assert_eq!(*b.add(4095), 0xCD);
            assert_eq!(
                *c.add(big_bytes - 1),
                0xEF,
                "last byte of the block is ours"
            );
        }
        assert!(lock(&ARENA).len() >= 3);
        flow_rt_free_all();
        assert!(lock(&ARENA).is_empty());
    }

    #[test]
    fn parse_cpu_max_cases() {
        assert_eq!(parse_cpu_max("max 100000"), None);
        assert_eq!(parse_cpu_max("4800000 100000"), Some(48));
        assert_eq!(parse_cpu_max("100 100000"), Some(1));
        assert_eq!(parse_cpu_max("garbage"), None);
    }

    #[test]
    fn parse_cfs_cases() {
        assert_eq!(parse_cfs("-1", "100000"), None);
        assert_eq!(parse_cfs("4800000", "100000"), Some(48));
        assert_eq!(parse_cfs("100", "100000"), Some(1));
        assert_eq!(parse_cfs("garbage", "100000"), None);
        assert_eq!(parse_cfs("100", "0"), None);
    }

    #[test]
    fn flow_par_override_wins() {
        assert_eq!(thread_count(Some("7"), 64, Some(2)), 7);
    }

    #[test]
    fn quota_caps_available() {
        // The S24 box shape: 384 visible host threads, ≈48-core cgroup quota.
        assert_eq!(thread_count(None, 384, Some(48)), 48);
        assert_eq!(thread_count(None, 8, Some(48)), 8);
        assert_eq!(thread_count(None, 8, None), 8);
    }

    /// Render-parity table (DESIGN §1): every scalar flow-rt prints must be
    /// byte-identical to `flow_interp::render`. flow-rt prints `v` via
    /// `Display`; assert that string equals the oracle's render for the same
    /// value. Covers the pinned tricky cases: `4080.0 → "4080"`, `5.375`,
    /// `-0.0`, `NaN`, `inf`, u8 `255`, bools, i64 extremes.
    #[test]
    fn render_parity() {
        macro_rules! check {
            ($disp:expr, $val:expr) => {{
                let rt = format!("{}", $disp); // what flow-rt's emit() writes
                let oracle = render(&$val);
                assert_eq!(rt, oracle, "parity mismatch for {:?}", $val);
            }};
        }

        // f64 — the formatting-sensitive cases.
        check!(4080.0_f64, Value::F64(4080.0));
        check!(5.375_f64, Value::F64(5.375));
        check!(-0.0_f64, Value::F64(-0.0));
        check!(0.0_f64, Value::F64(0.0));
        check!(f64::NAN, Value::F64(f64::NAN));
        check!(f64::INFINITY, Value::F64(f64::INFINITY));
        check!(f64::NEG_INFINITY, Value::F64(f64::NEG_INFINITY));
        check!(f64::MAX, Value::F64(f64::MAX));

        // f32.
        check!(4080.0_f32, Value::F32(4080.0));
        check!(5.375_f32, Value::F32(5.375));
        check!(-0.0_f32, Value::F32(-0.0));
        check!(f32::NAN, Value::F32(f32::NAN));
        check!(f32::INFINITY, Value::F32(f32::INFINITY));

        // u8 — incl. the high half (255, > 127).
        check!(0_u8, Value::U8(0));
        check!(127_u8, Value::U8(127));
        check!(128_u8, Value::U8(128));
        check!(255_u8, Value::U8(255));

        // bools.
        check!(true, Value::Bool(true));
        check!(false, Value::Bool(false));

        // Str — flow_print_str renders bytes via Display (== render(Str) == s).
        check!("hello", Value::Str("hello".into()));
        check!("", Value::Str("".into()));
        check!("4080", Value::Str("4080".into()));

        // i32 / i64 extremes.
        check!(0_i32, Value::I32(0));
        check!(-1_i32, Value::I32(-1));
        check!(i32::MIN, Value::I32(i32::MIN));
        check!(i32::MAX, Value::I32(i32::MAX));
        check!(i64::MIN, Value::I64(i64::MIN));
        check!(i64::MAX, Value::I64(i64::MAX));
    }

    fn test_run(n_tasks: u32, threads: usize) -> *mut FlowParRun {
        allocate_run(n_tasks, Pool::new(threads))
    }

    fn frame_ptr<T>(frame: &T) -> *mut u8 {
        ptr::from_ref(frame).cast_mut().cast()
    }

    fn wait_entry(task: u32, threshold: u32) -> u64 {
        (u64::from(task) << 32) | u64::from(threshold)
    }

    fn completion_entry(task: u32) -> u64 {
        wait_entry(task, u32::MAX)
    }

    unsafe fn trap_flag(handle: *mut FlowParRun) -> u64 {
        unsafe { run_handle(handle) }
            .run
            .trap
            .load(Ordering::SeqCst)
    }

    unsafe fn clear_trap(handle: *mut FlowParRun) {
        unsafe { run_handle(handle) }
            .run
            .trap
            .store(0, Ordering::SeqCst);
    }

    struct SplitFrame {
        n: usize,
        hits: Vec<AtomicU64>,
        calls: AtomicU32,
        bad_range: AtomicBool,
        duplicate: AtomicBool,
    }

    extern "C" fn hit_slice(lo: i64, hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<SplitFrame>() };
        frame.calls.fetch_add(1, Ordering::SeqCst);
        if lo < 0 || hi < lo || hi as usize > frame.n {
            frame.bad_range.store(true, Ordering::SeqCst);
            return;
        }
        for index in lo as usize..hi as usize {
            let bit = 1_u64 << (index % 64);
            if frame.hits[index / 64].fetch_or(bit, Ordering::SeqCst) & bit != 0 {
                frame.duplicate.store(true, Ordering::SeqCst);
            }
        }
    }

    struct ZeroFrame {
        split_calls: AtomicU32,
        dependent_calls: AtomicU32,
    }

    extern "C" fn zero_split(_lo: i64, _hi: i64, frame: *mut u8) {
        unsafe { &*frame.cast::<ZeroFrame>() }
            .split_calls
            .fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn zero_dependent(_lo: i64, _hi: i64, frame: *mut u8) {
        unsafe { &*frame.cast::<ZeroFrame>() }
            .dependent_calls
            .fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn split_covers_each_index_once() {
        let n = (GRAIN * 3 + 17) as usize;
        let frame = Box::new(SplitFrame {
            n,
            hits: (0..n.div_ceil(64)).map(|_| AtomicU64::new(0)).collect(),
            calls: AtomicU32::new(0),
            bad_range: AtomicBool::new(false),
            duplicate: AtomicBool::new(false),
        });
        let handle = test_run(1, 4);
        unsafe {
            flow_par_task(handle, 0, 1, hit_slice, n as i64, 0, 0, 0, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
            flow_par_finish(handle);
        }
        assert_eq!(frame.calls.load(Ordering::SeqCst), 4);
        assert!(!frame.bad_range.load(Ordering::SeqCst));
        assert!(!frame.duplicate.load(Ordering::SeqCst));
        assert!((0..n).all(|index| {
            frame.hits[index / 64].load(Ordering::SeqCst) & (1_u64 << (index % 64)) != 0
        }));

        let zero = Box::new(ZeroFrame {
            split_calls: AtomicU32::new(0),
            dependent_calls: AtomicU32::new(0),
        });
        let handle = test_run(2, 4);
        unsafe {
            flow_par_task(handle, 0, 1, zero_split, 0, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, zero_dependent, 0, 0, 0, 0, 0);
            flow_par_dep(handle, 0, 1);
            flow_par_launch(handle, frame_ptr(&*zero));
            flow_par_finish(handle);
        }
        assert_eq!(zero.split_calls.load(Ordering::SeqCst), 0);
        assert_eq!(zero.dependent_calls.load(Ordering::SeqCst), 1);
    }

    struct DepFrame {
        hits: Vec<AtomicU8>,
        dependent_calls: AtomicU32,
        violation: AtomicBool,
    }

    extern "C" fn dep_producer(lo: i64, hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<DepFrame>() };
        for hit in &frame.hits[lo as usize..hi as usize] {
            hit.store(1, Ordering::SeqCst);
        }
    }

    extern "C" fn dep_consumer(_lo: i64, _hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<DepFrame>() };
        if frame.hits.iter().any(|hit| hit.load(Ordering::SeqCst) != 1) {
            frame.violation.store(true, Ordering::SeqCst);
        }
        frame.dependent_calls.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn dependent_starts_after_all_slices_complete() {
        let n = (GRAIN * 3 + 1) as usize;
        let frame = Box::new(DepFrame {
            hits: (0..n).map(|_| AtomicU8::new(0)).collect(),
            dependent_calls: AtomicU32::new(0),
            violation: AtomicBool::new(false),
        });
        let handle = test_run(2, 4);
        unsafe {
            flow_par_task(handle, 0, 1, dep_producer, n as i64, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, dep_consumer, 0, u32::MAX, 0, 0, 0);
            flow_par_dep(handle, 0, 1);
            flow_par_launch(handle, frame_ptr(&*frame));
            flow_par_finish(handle);
        }
        assert_eq!(frame.dependent_calls.load(Ordering::SeqCst), 1);
        assert!(!frame.violation.load(Ordering::SeqCst));
    }

    struct OrderFrame {
        calls: Mutex<Vec<(i64, i64)>>,
    }

    extern "C" fn record_order(lo: i64, hi: i64, frame: *mut u8) {
        lock(&unsafe { &*frame.cast::<OrderFrame>() }.calls).push((lo, hi));
    }

    #[test]
    fn inline_runs_ready_tasks_by_ascending_index() {
        let frame = Box::new(OrderFrame {
            calls: Mutex::new(Vec::new()),
        });
        let handle = test_run(3, 1);
        unsafe {
            flow_par_task(handle, 2, 0, record_order, 12, 100, 0, 0, 0);
            flow_par_task(handle, 0, 0, record_order, 10, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, record_order, 11, 50, 0, 0, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
        }
        assert!(lock(&frame.calls).is_empty(), "launch must only seed");
        unsafe { flow_par_finish(handle) };
        assert_eq!(*lock(&frame.calls), [(0, 10), (0, 11), (0, 12)]);
    }

    struct TrapFrame {
        barrier: Barrier,
    }

    extern "C" fn trap_after_barrier(_lo: i64, hi: i64, frame: *mut u8) {
        unsafe { &*frame.cast::<TrapFrame>() }.barrier.wait();
        flow_par_trap(hi, u32::from(hi == 7));
    }

    #[test]
    fn trap_flag_keeps_minimum_topo() {
        let frame = Box::new(TrapFrame {
            barrier: Barrier::new(2),
        });
        let handle = test_run(2, 2);
        unsafe {
            flow_par_task(handle, 0, 0, trap_after_barrier, 50, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, trap_after_barrier, 7, 0, 0, 0, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
            let entries = [completion_entry(0), completion_entry(1)];
            flow_par_wait(handle, entries.as_ptr(), entries.len() as u32);
            assert_eq!(trap_flag(handle), (7_u64 << 32) | 2);
            clear_trap(handle);
            flow_par_finish(handle);
        }
    }

    struct SpeculateFrame {
        trapping_completed: AtomicBool,
        dependent_calls: AtomicU32,
    }

    extern "C" fn trapping_task(_lo: i64, _hi: i64, frame: *mut u8) {
        flow_par_trap(3, 1);
        unsafe { &*frame.cast::<SpeculateFrame>() }
            .trapping_completed
            .store(true, Ordering::SeqCst);
    }

    extern "C" fn dependent_after_trap(_lo: i64, _hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<SpeculateFrame>() };
        assert!(frame.trapping_completed.load(Ordering::SeqCst));
        frame.dependent_calls.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn trapped_task_completes_and_unlocks_its_dependent() {
        let frame = Box::new(SpeculateFrame {
            trapping_completed: AtomicBool::new(false),
            dependent_calls: AtomicU32::new(0),
        });
        let handle = test_run(2, 1);
        unsafe {
            flow_par_task(handle, 0, 0, trapping_task, 0, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, dependent_after_trap, 0, 0, 0, 0, 0);
            flow_par_dep(handle, 0, 1);
            flow_par_launch(handle, frame_ptr(&*frame));
            let entries = [completion_entry(1)];
            flow_par_wait(handle, entries.as_ptr(), 1);
            assert!(frame.trapping_completed.load(Ordering::SeqCst));
            assert_eq!(frame.dependent_calls.load(Ordering::SeqCst), 1);
            assert_eq!(trap_flag(handle), (3_u64 << 32) | 2);
            clear_trap(handle);
            flow_par_finish(handle);
        }
    }

    struct WatermarkFrame {
        published: AtomicBool,
        gate: AtomicBool,
        completed: AtomicBool,
    }

    extern "C" fn publish_then_wait(_lo: i64, _hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<WatermarkFrame>() };
        flow_par_watermark(10);
        frame.published.store(true, Ordering::SeqCst);
        while !frame.gate.load(Ordering::Acquire) {
            thread::yield_now();
        }
        frame.completed.store(true, Ordering::SeqCst);
    }

    #[test]
    fn watermark_wait_can_finish_before_task_completion() {
        let frame = Box::new(WatermarkFrame {
            published: AtomicBool::new(false),
            gate: AtomicBool::new(false),
            completed: AtomicBool::new(false),
        });
        let handle = test_run(1, 2);
        unsafe {
            flow_par_task(handle, 0, 0, publish_then_wait, 0, 0, 0, 0, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        while !frame.published.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::yield_now();
        }
        assert!(frame.published.load(Ordering::SeqCst));

        let below = [wait_entry(0, 10)];
        unsafe { flow_par_wait(handle, below.as_ptr(), 1) };
        assert!(!frame.completed.load(Ordering::SeqCst));

        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let handle_addr = handle as usize;
        let waiter = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let above = [wait_entry(0, 11)];
            unsafe { flow_par_wait(handle_addr as *mut FlowParRun, above.as_ptr(), 1) };
            done_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(!frame.completed.load(Ordering::SeqCst));

        frame.gate.store(true, Ordering::Release);
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        waiter.join().unwrap();
        assert!(frame.completed.load(Ordering::SeqCst));
        unsafe { flow_par_finish(handle) };
    }

    struct PinnedFrame {
        host: thread::ThreadId,
        ranges: Mutex<Vec<(i64, i64, thread::ThreadId)>>,
        dependent_calls: AtomicU32,
    }

    extern "C" fn pinned_slice(lo: i64, hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<PinnedFrame>() };
        lock(&frame.ranges).push((lo, hi, thread::current().id()));
    }

    extern "C" fn pinned_dependent(_lo: i64, _hi: i64, frame: *mut u8) {
        unsafe { &*frame.cast::<PinnedFrame>() }
            .dependent_calls
            .fetch_add(1, Ordering::SeqCst);
    }

    fn def_of(n: i64, slice_elems: i64, oversub: u32) -> TaskDef {
        extern "C" fn noop(_: i64, _: i64, _: *mut u8) {}
        TaskDef {
            kind: 1,
            f: noop,
            n,
            rank: 0,
            slice_elems,
            oversub,
            width: 0,
        }
    }

    /// Every slice boundary must land on the region's quantum, and the ranges
    /// must still tile `[0, n)` exactly.
    ///
    /// The bug this pins: deriving a COUNT and then equal-dividing `n` destroys
    /// whatever alignment the size asked for. 1048576 over 52 slices is 20164.9
    /// elements, so every boundary lands mid-row and every piece starts with a
    /// partial register block. Measured on matmul1024 at 14 lanes, counts that
    /// happen to divide n evenly (32/64/128) ran 2.5-2.6 ms while neighbouring
    /// counts that do not (43/52/57) ran 4.4-5.8 — a 2x swing with no relation
    /// to how many pieces there are.
    #[test]
    fn slices_are_cut_on_the_quantum_and_tile_the_range() {
        let n = 1_048_576;
        let quantum = 4096; // TI=4 rows of c=1024
        for threads in [1usize, 4, 8, 14] {
            for oversub in [1u32, 4, 8] {
                let ranges = slice_ranges(def_of(n, quantum, oversub), threads);
                assert!(!ranges.is_empty(), "threads={threads} oversub={oversub}");
                assert_eq!(ranges[0].0, 0, "must start at 0");
                assert_eq!(ranges[ranges.len() - 1].1, n, "must cover n");
                for window in ranges.windows(2) {
                    assert_eq!(window[0].1, window[1].0, "ranges must be contiguous");
                }
                for &(lo, hi) in &ranges {
                    assert!(lo <= hi, "ranges must be ordered");
                    assert_eq!(lo % quantum, 0, "every start is on the quantum: {lo}");
                }
                // Only the final boundary may be short of a whole quantum, and
                // here n is an exact multiple so none may be.
                for &(lo, hi) in &ranges {
                    assert_eq!((hi - lo) % quantum, 0, "whole blocks only: {lo}..{hi}");
                }
            }
        }
    }

    /// Ragged slice counts are unreachable: whatever is asked for, the pieces
    /// carry equal numbers of blocks. Measured cost of raggedness on matmul1024
    /// at 14 lanes: 1.8x (a count of 57 over 256 blocks ran 4.48 ms against
    /// 2.49-2.73 for every count that divides 256).
    #[test]
    fn slice_counts_are_never_ragged() {
        let (n, quantum) = (1_048_576, 4096);
        let blocks = n / quantum;
        for threads in [1usize, 4, 8, 14] {
            for oversub in [1u32, 4, 16, 64] {
                let ranges = slice_ranges(def_of(n, quantum, oversub), threads);
                let sizes: Vec<i64> = ranges.iter().map(|&(lo, hi)| (hi - lo) / quantum).collect();
                let (min, max) = (
                    *sizes.iter().min().expect("slices"),
                    *sizes.iter().max().expect("slices"),
                );
                assert!(
                    max - min <= 1,
                    "threads={threads} oversub={oversub}: block counts {sizes:?}"
                );
                assert!(ranges.len() <= blocks as usize, "never below one block");
            }
        }
    }

    /// The floor is a coherence constraint: a slice below one register block
    /// drops the dispatch onto the TI=1 fallback (7x on matmul1024). No
    /// requested over-decomposition may cut below it.
    #[test]
    fn over_decomposition_never_cuts_below_one_block() {
        let n = 16_384;
        let quantum = 4096; // only four blocks exist
        let ranges = slice_ranges(def_of(n, quantum, 64), 14);
        assert!(
            ranges.len() <= 4,
            "cannot make more pieces than blocks: {ranges:?}"
        );
        for &(lo, hi) in &ranges {
            assert!(
                hi - lo >= quantum || hi == n,
                "no sub-block slice: {lo}..{hi}"
            );
        }
    }

    /// A `Seq` task carries `n = 0` and must still run exactly once — the
    /// quantised slicer must not optimise its empty range away. An empty
    /// `Split`, by contrast, has genuinely nothing to run.
    #[test]
    fn sequential_tasks_still_get_exactly_one_range() {
        let mut seq = def_of(0, 0, 1);
        seq.kind = 0;
        assert_eq!(slice_ranges(seq, 14), vec![(0, 0)], "Seq runs once");
        assert!(
            slice_ranges(def_of(0, 0, 1), 14).is_empty(),
            "an empty Split has no work"
        );
    }

    #[test]
    fn pinned_task_runs_inline_and_unlocks_dependents() {
        let n = GRAIN * 2 + 1;
        let frame = Box::new(PinnedFrame {
            host: thread::current().id(),
            ranges: Mutex::new(Vec::new()),
            dependent_calls: AtomicU32::new(0),
        });
        let handle = allocate_run(2, Pool::with_workers(4, 0));
        unsafe {
            flow_par_task(handle, 0, 1, pinned_slice, n, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, pinned_dependent, 0, 0, 0, 0, 0);
            flow_par_dep(handle, 0, 1);
            flow_par_pin(handle, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
            flow_par_run_pinned(handle, 0);
            let entries = [completion_entry(1)];
            flow_par_wait(handle, entries.as_ptr(), 1);
            flow_par_finish(handle);
        }

        let ranges = lock(&frame.ranges);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges.first().map(|range| range.0), Some(0));
        assert_eq!(ranges.last().map(|range| range.1), Some(n));
        assert!(ranges.windows(2).all(|pair| pair[0].1 == pair[1].0));
        assert!(ranges.iter().all(|range| range.2 == frame.host));
        assert_eq!(frame.dependent_calls.load(Ordering::SeqCst), 1);
    }

    /// Regression (review find): a zero-dep pinned task must never be seeded to
    /// the pool at launch — a worker grabbing it would run host-flavor code on
    /// a worker thread. Pinning happens at registration, so the pool cannot see
    /// it regardless of timing; only flow_par_run_pinned executes it.
    #[test]
    fn pinned_zero_dep_task_is_never_pool_seeded() {
        let frame = Box::new(PinnedFrame {
            host: thread::current().id(),
            ranges: Mutex::new(Vec::new()),
            dependent_calls: AtomicU32::new(0),
        });
        let handle = allocate_run(2, Pool::with_workers(4, 4));
        unsafe {
            flow_par_task(handle, 0, 0, pinned_slice, 1, 0, 0, 0, 0);
            flow_par_task(handle, 1, 0, pinned_dependent, 0, 0, 0, 0, 0);
            flow_par_pin(handle, 0);
            flow_par_launch(handle, frame_ptr(&*frame));
            // Workers finish the normal sibling; the pinned task stays untouched.
            let entries = [completion_entry(1)];
            flow_par_wait(handle, entries.as_ptr(), 1);
            assert!(
                lock(&frame.ranges).is_empty(),
                "pool executed a pinned task"
            );
            flow_par_run_pinned(handle, 0);
            flow_par_finish(handle);
        }
        let ranges = lock(&frame.ranges);
        assert_eq!(ranges.len(), 1);
        assert!(ranges.iter().all(|range| range.2 == frame.host));
    }

    struct BlockFrame {
        entered: Mutex<bool>,
        entered_cv: Condvar,
        released: Mutex<bool>,
        released_cv: Condvar,
    }

    extern "C" fn blocking_task(_lo: i64, _hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<BlockFrame>() };
        *lock(&frame.entered) = true;
        frame.entered_cv.notify_all();
        let mut released = lock(&frame.released);
        while !*released {
            released = frame.released_cv.wait(released).unwrap();
        }
    }

    struct HelpFrame {
        host: thread::ThreadId,
        calls: AtomicU32,
        ran_on_host: AtomicBool,
    }

    extern "C" fn helped_task(_lo: i64, _hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<HelpFrame>() };
        frame.calls.fetch_add(1, Ordering::SeqCst);
        frame
            .ran_on_host
            .store(thread::current().id() == frame.host, Ordering::SeqCst);
    }

    #[test]
    fn wait_helps_while_the_background_worker_is_busy() {
        // Exercise one busy worker through the same queues; production T==1
        // remains thread-free.
        let pool = Pool::with_workers(1, 1);
        let blocker = Box::new(BlockFrame {
            entered: Mutex::new(false),
            entered_cv: Condvar::new(),
            released: Mutex::new(false),
            released_cv: Condvar::new(),
        });
        let blocker_run = allocate_run(1, Arc::clone(&pool));
        unsafe {
            flow_par_task(blocker_run, 0, 0, blocking_task, 0, 0, 0, 0, 0);
            flow_par_launch(blocker_run, frame_ptr(&*blocker));
        }

        let entered = lock(&blocker.entered);
        let (entered, _) = blocker
            .entered_cv
            .wait_timeout_while(entered, Duration::from_secs(2), |entered| !*entered)
            .unwrap();
        let worker_entered = *entered;
        drop(entered);

        let help = Box::new(HelpFrame {
            host: thread::current().id(),
            calls: AtomicU32::new(0),
            ran_on_host: AtomicBool::new(false),
        });
        let helped_run = allocate_run(1, pool);
        unsafe {
            flow_par_task(helped_run, 0, 0, helped_task, 0, 0, 0, 0, 0);
            flow_par_launch(helped_run, frame_ptr(&*help));
            let entries = [completion_entry(0)];
            flow_par_wait(helped_run, entries.as_ptr(), 1);
        }

        *lock(&blocker.released) = true;
        blocker.released_cv.notify_all();
        unsafe {
            flow_par_finish(helped_run);
            flow_par_finish(blocker_run);
        }

        assert!(worker_entered, "background worker did not start blocker");
        assert_eq!(help.calls.load(Ordering::SeqCst), 1);
        assert!(help.ran_on_host.load(Ordering::SeqCst));
    }

    struct NestedFrame {
        inner_trap: AtomicU64,
    }

    extern "C" fn nested_inner(_lo: i64, _hi: i64, _frame: *mut u8) {
        flow_par_trap(20, 1);
    }

    extern "C" fn nested_outer(_lo: i64, _hi: i64, frame: *mut u8) {
        let nested = unsafe { &*frame.cast::<NestedFrame>() };
        let inner = test_run(1, 1);
        unsafe {
            flow_par_task(inner, 0, 0, nested_inner, 0, 0, 0, 0, 0);
            flow_par_launch(inner, frame);
            let entries = [completion_entry(0)];
            flow_par_wait(inner, entries.as_ptr(), 1);
            nested.inner_trap.store(trap_flag(inner), Ordering::SeqCst);
            clear_trap(inner);
            flow_par_finish(inner);
        }
        flow_par_trap(10, 0);
    }

    #[test]
    fn nested_run_uses_innermost_tls_and_restores_outer() {
        let frame = Box::new(NestedFrame {
            inner_trap: AtomicU64::new(0),
        });
        let outer = test_run(1, 1);
        unsafe {
            flow_par_task(outer, 0, 0, nested_outer, 0, 0, 0, 0, 0);
            flow_par_launch(outer, frame_ptr(&*frame));
            let entries = [completion_entry(0)];
            flow_par_wait(outer, entries.as_ptr(), 1);
            assert_eq!(frame.inner_trap.load(Ordering::SeqCst), (20_u64 << 32) | 2);
            assert_eq!(trap_flag(outer), (10_u64 << 32) | 1);
            clear_trap(outer);
            flow_par_finish(outer);
        }
    }

    struct DagFrame {
        predecessors: Vec<Vec<usize>>,
        counts: Vec<AtomicU8>,
        violation: AtomicBool,
    }

    extern "C" fn dag_task(lo: i64, hi: i64, frame: *mut u8) {
        let frame = unsafe { &*frame.cast::<DagFrame>() };
        let task = hi as usize;
        if lo != 0
            || task >= frame.counts.len()
            || frame.predecessors[task]
                .iter()
                .any(|&before| frame.counts[before].load(Ordering::SeqCst) != 1)
        {
            frame.violation.store(true, Ordering::SeqCst);
            return;
        }
        if frame.counts[task].fetch_add(1, Ordering::SeqCst) != 0 {
            frame.violation.store(true, Ordering::SeqCst);
        }
        if task.is_multiple_of(11) {
            flow_par_trap(task as i64 + 1, (task % 2) as u32);
        }
    }

    fn lcg(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        *state
    }

    #[test]
    fn seeded_random_dags_complete_once() {
        let pool = Pool::new(4);
        let mut seed = 0x5eed_cafe_d15c_a11e;
        for run_idx in 0..64 {
            let n_tasks = 32 + (lcg(&mut seed) % 17) as usize;
            let mut predecessors = vec![Vec::new(); n_tasks];
            for (after, task_predecessors) in predecessors.iter_mut().enumerate().skip(1) {
                for before in 0..after {
                    if lcg(&mut seed).is_multiple_of(7) {
                        task_predecessors.push(before);
                    }
                }
            }
            let frame = Box::new(DagFrame {
                predecessors,
                counts: (0..n_tasks).map(|_| AtomicU8::new(0)).collect(),
                violation: AtomicBool::new(false),
            });
            let handle = allocate_run(n_tasks as u32, Arc::clone(&pool));
            unsafe {
                for task in 0..n_tasks {
                    flow_par_task(
                        handle,
                        task as u32,
                        0,
                        dag_task,
                        task as i64,
                        lcg(&mut seed) as u32,
                        0,
                        0,
                        0,
                    );
                }
                for after in 0..n_tasks {
                    for &before in &frame.predecessors[after] {
                        flow_par_dep(handle, before as u32, after as u32);
                    }
                }
                flow_par_launch(handle, frame_ptr(&*frame));
                let entries: Vec<_> = (0..n_tasks)
                    .map(|task| completion_entry(task as u32))
                    .collect();
                flow_par_wait(handle, entries.as_ptr(), entries.len() as u32);
                assert_ne!(trap_flag(handle), 0);
                clear_trap(handle);
                flow_par_finish(handle);
            }
            assert!(
                !frame.violation.load(Ordering::SeqCst),
                "DAG invariant failed on run {run_idx}, seed {seed:#x}"
            );
            assert!(
                frame
                    .counts
                    .iter()
                    .all(|count| count.load(Ordering::SeqCst) == 1),
                "task coverage failed on run {run_idx}, seed {seed:#x}"
            );
        }
    }
}
