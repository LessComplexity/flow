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

fn slice_ranges(def: TaskDef, threads: usize) -> Vec<(i64, i64)> {
    let slices = if def.kind == 0 {
        1
    } else if def.n == 0 {
        0
    } else {
        usize::try_from((def.n as u64).div_ceil(GRAIN as u64))
            .unwrap_or(usize::MAX)
            .min(threads)
    };
    let mut ranges = Vec::with_capacity(slices);
    let mut lo = 0;
    for slice in 0..slices {
        let hi = lo + def.n / slices as i64 + i64::from((slice as i64) < def.n % slices as i64);
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

fn configured_threads() -> usize {
    std::env::var("FLOW_PAR")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|&value| value >= 1)
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        })
}

fn global_pool() -> Arc<Pool> {
    static POOL: OnceLock<Arc<Pool>> = OnceLock::new();
    POOL.get_or_init(|| Pool::new(configured_threads())).clone()
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
                let (ranges, unlocked) = {
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
                        (Vec::new(), unlocked)
                    } else {
                        state.tasks[task_idx].slices_left = ranges.len();
                        (ranges, Vec::new())
                    }
                };

                ready.extend(unlocked);
                for (lo, hi) in ranges {
                    let lane = match placement {
                        Placement::Seed => {
                            let lane = cursor % self.pool.threads;
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
    task.def = Some(TaskDef { kind, f, n, rank });
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
            flow_par_task(handle, 0, 1, hit_slice, n as i64, 0);
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
            flow_par_task(handle, 0, 1, zero_split, 0, 0);
            flow_par_task(handle, 1, 0, zero_dependent, 0, 0);
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
            flow_par_task(handle, 0, 1, dep_producer, n as i64, 0);
            flow_par_task(handle, 1, 0, dep_consumer, 0, u32::MAX);
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
            flow_par_task(handle, 2, 0, record_order, 12, 100);
            flow_par_task(handle, 0, 0, record_order, 10, 0);
            flow_par_task(handle, 1, 0, record_order, 11, 50);
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
            flow_par_task(handle, 0, 0, trap_after_barrier, 50, 0);
            flow_par_task(handle, 1, 0, trap_after_barrier, 7, 0);
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
            flow_par_task(handle, 0, 0, trapping_task, 0, 0);
            flow_par_task(handle, 1, 0, dependent_after_trap, 0, 0);
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
            flow_par_task(handle, 0, 0, publish_then_wait, 0, 0);
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
            flow_par_task(handle, 0, 1, pinned_slice, n, 0);
            flow_par_task(handle, 1, 0, pinned_dependent, 0, 0);
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
            flow_par_task(handle, 0, 0, pinned_slice, 1, 0);
            flow_par_task(handle, 1, 0, pinned_dependent, 0, 0);
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
            flow_par_task(blocker_run, 0, 0, blocking_task, 0, 0);
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
            flow_par_task(helped_run, 0, 0, helped_task, 0, 0);
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
            flow_par_task(inner, 0, 0, nested_inner, 0, 0);
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
            flow_par_task(outer, 0, 0, nested_outer, 0, 0);
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
