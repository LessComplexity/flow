//! DESIGN §4/§6 test plan item 1/3: the compile-and-run differential harness.
//!
//! For each program (the 10 examples, two sequential loops, trap cases, and a
//! closed-mode testgen sweep), on **raw and `rewrite()`d** IR: `emit → clang
//! <prog>.ll libflow_rt.a -o prog → run (time-boxed) → compare per L1` — `Done`
//! ⇒ exit 0 + stdout byte-equal to the oracle; `Trapped` ⇒ exit 101 (stdout
//! ignored). Every case is compiled at **both `-O0` and `-O2`** against the same
//! oracle expectations (the DESIGN §8 `-O2` row — optimization is where
//! accidentally-relied-on LLVM-level UB would surface). `clang` absent ⇒ the
//! whole harness skips-with-reason (never a faked pass). Open-mode testgen
//! (`i32 → i32`) is excluded — no native `@main` analog (BL8).

#![allow(clippy::type_complexity)]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use flow_interp::{Outcome, RValue, RunResult, render, run};
use flow_ir::{CategoryIr, Ty};
use flow_rewrite::rewrite;

// The testgen program generator (shared with flow-rewrite's differential duty).
#[path = "../../flow-rewrite/tests/testgen/mod.rs"]
mod testgen;

use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{Built, build, prog_strategy};

const BUDGET: u64 = 10_000_000;
const TIMEOUT_SECS: u64 = 15;

const EXAMPLES: &[&str] = &[
    "abs",
    "calc",
    "fanout",
    "fir",
    "pipeline",
    "sepia",
    "seq_demo",
    "sum_to_n",
    "vector_add",
    "zip_demo",
];

// --- toolchain ------------------------------------------------------------

fn clang() -> Option<String> {
    // Respect CC, else find clang on PATH.
    if let Ok(cc) = std::env::var("CC") {
        return Some(cc);
    }
    let out = Command::new("which").arg("clang").output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

fn rt_lib() -> PathBuf {
    static BUILD: Once = Once::new();
    // Serialize the flow-rt staticlib build so parallel test binaries don't race.
    BUILD.call_once(|| {
        let ok = Command::new("cargo")
            .args(["build", "-p", "flow-rt"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "cargo build -p flow-rt failed");
    });
    PathBuf::from(format!(
        "{}/../../target/debug/libflow_rt.a",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Compile `.ll` + libflow_rt.a at optimization level `opt` and run, time-boxed.
/// `None` on a timeout (the harness fails loudly rather than hanging). Panics if
/// clang errors.
fn compile_run(clang: &str, ll: &str, tag: &str, opt: &str) -> Option<(Vec<u8>, i32)> {
    let dir = tempfile::tempdir().unwrap();
    let llp = dir.path().join("p.ll");
    let exe = dir.path().join("p");
    std::fs::write(&llp, ll).unwrap();
    let out = Command::new(clang)
        .arg(opt)
        .arg(&llp)
        .arg(rt_lib())
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{tag}: clang failed:\n{}\n---\n{ll}",
        String::from_utf8_lossy(&out.stderr)
    );
    run_exe(&exe, TIMEOUT_SECS)
}

fn run_exe(exe: &Path, secs: u64) -> Option<(Vec<u8>, i32)> {
    let mut child = Command::new(exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut buf = Vec::new();
            child.stdout.take().unwrap().read_to_end(&mut buf).unwrap();
            return Some((buf, status.code().unwrap_or(-1)));
        }
        if start.elapsed() > Duration::from_secs(secs) {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// --- oracle expectation ---------------------------------------------------

/// The expected native observable from the interp oracle (L1 / BL8). Returns
/// `None` for a Diverged run (skip). `Some((None, 101))` for a trap (stdout
/// ignored). `Some((Some(stdout), 0))` for a Done program — the entry protocol
/// decides the observable: an `IoToken` entry's stdout is the token log; a
/// closed `Unit → scalar` entry's is the wrapper-printed return.
fn expect_native(ir: &CategoryIr, rr: &RunResult) -> Option<(Option<String>, i32)> {
    match &rr.outcome {
        Outcome::Diverged => None,
        Outcome::Trapped(_) => Some((None, 101)),
        Outcome::Done(v) => {
            let entry = ir.entry();
            let in_ty = &ir.object(ir.func(entry).unwrap().input).unwrap().ty;
            let stdout = if *in_ty == Ty::IoToken {
                rr.output.clone()
            } else if let RValue::Scalar(sv) = v {
                // The @main wrapper prints the scalar return with a newline.
                format!("{}\n", render(sv))
            } else {
                String::new() // non-printable return: wrapper prints nothing
            };
            Some((Some(stdout), 0))
        }
    }
}

/// Emit `ir`, compile+run at **both `-O0` and `-O2`**, and assert L1 parity
/// against `rr` at each level — the same oracle expectations (DESIGN §8 `-O2`
/// row: optimization is where accidentally-relied-on LLVM-level UB surfaces).
fn assert_parity(clang: &str, ir: &CategoryIr, rr: &RunResult, tag: &str) {
    let Some((want_out, want_code)) = expect_native(ir, rr) else {
        return; // diverged — skip
    };
    let ll = flow_backend_llvm::emit(ir).unwrap_or_else(|e| panic!("{tag}: emit failed: {e:?}"));
    for opt in ["-O0", "-O2"] {
        let tag_o = format!("{tag}/{opt}");
        let (got_out, got_code) = compile_run(clang, &ll, &tag_o, opt)
            .unwrap_or_else(|| panic!("{tag_o}: native run timed out ({TIMEOUT_SECS}s)"));
        assert_eq!(got_code, want_code, "{tag_o}: exit code");
        if let Some(want) = &want_out {
            assert_eq!(
                String::from_utf8_lossy(&got_out),
                *want,
                "{tag_o}: stdout mismatch"
            );
        }
    }
}

fn lower_src(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

fn build_example(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap();
    lower_src(&src)
}

// --- tests ----------------------------------------------------------------

/// The M2 line: the 10 examples, raw and rewritten, compile-and-run oracle-equal
/// at `-O0` and `-O2`.
#[test]
fn differential_examples_raw_and_rewritten() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_examples: clang not found");
        return;
    };
    for name in EXAMPLES {
        let ir = build_example(name);
        let rr = run(&ir, BUDGET);
        assert_parity(&clang, &ir, &rr, &format!("{name}/raw"));

        // Rebuild (CategoryIr is not Clone) and rewrite.
        let res = rewrite(build_example(name));
        let rr2 = run(&res.ir, BUDGET);
        assert_parity(&clang, &res.ir, &rr2, &format!("{name}/rewritten"));
    }
}

/// Two sequential loops in one fn (S12 P0 shape) compile-and-run to "20\n".
#[test]
fn differential_two_sequential_loops() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut a: i32 <- 0;
    loop { (i < n) -> { -true-> { a + 2 -> a; i + 1 -> i; -> loop; } -false-> a -> aa; } }
    mut j: i32 <- 0;
    mut b: i32 <- 0;
    loop { (j < n) -> { -true-> { b + 3 -> b; j + 1 -> j; -> loop; } -false-> b -> bb; } }
    aa + bb -> ret;
}
fn main() { 4 -> f -> r; r -> println; }
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "20\n");
    assert_parity(&clang, &ir, &rr, "two_loops");
}

/// Trap parity (exit 101): div-by-zero and an out-of-bounds `Update`.
#[test]
fn differential_traps_exit_101() {
    let Some(clang) = clang() else {
        return;
    };
    // Div by zero.
    let div0 = r#"
fn f(a: i32, b: i32) -> i32 { a / b -> ret; }
fn main() { (1, 0) -> f -> r; r -> println; }
"#;
    let ir = lower_src(div0);
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Trapped(_)));
    assert_parity(&clang, &ir, &rr, "div0");

    // Out-of-bounds Update (index 9 into [i32; 4]) — built via IrBuilder.
    use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Value};
    const L: SourceLoc = SourceLoc { start: 0, end: 0 };
    let mut b = IrBuilder::new();
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    // Unit -> i32 closed pure: build [0;4], update OOB, index [0] → print via wrapper.
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let arr = fb
            .pack_array(&[zero, zero, zero, zero], Dest::Fresh(None), L)
            .unwrap();
        let oob = fb.constant(Value::I32(9), L).unwrap();
        let v = fb.constant(Value::I32(7), L).unwrap();
        let up = fb.update(arr, oob, v, Dest::Fresh(None), L).unwrap();
        let i0 = fb.constant(Value::I32(0), L).unwrap();
        fb.index(up, i0, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let _ = arr_ty;
    let ir = b.seal(f).unwrap();
    let rr = run(&ir, BUDGET);
    assert!(
        matches!(rr.outcome, Outcome::Trapped(_)),
        "expected OOB trap"
    );
    assert_parity(&clang, &ir, &rr, "update_oob");
}

/// The u8 value path (DESIGN §1): the `flow_print_u8` `i8 zeroext` ABI and the
/// u8 `Index` `zext`+guard. DESIGN §1 says a dropped `zeroext` prints garbage for
/// u8 > 127 on arm64 and *only the differential* can catch it (the flow-rt unit
/// table can't). No example or testgen program uses u8, so this is the sole
/// compile-and-run cover for that class. Built via `IrBuilder` (no surface u8).
#[test]
fn differential_u8_index_and_print() {
    let Some(clang) = clang() else {
        return;
    };
    use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Value};
    const L: SourceLoc = SourceLoc { start: 0, end: 0 };
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let tok = fb.input();
        // [u8; 4] with high-bit-set values (200, 255 > 127).
        let e0 = fb.constant(Value::U8(200), L).unwrap();
        let e1 = fb.constant(Value::U8(50), L).unwrap();
        let e2 = fb.constant(Value::U8(255), L).unwrap();
        let e3 = fb.constant(Value::U8(10), L).unwrap();
        let arr = fb
            .pack_array(&[e0, e1, e2, e3], Dest::Fresh(None), L)
            .unwrap();
        // u8-typed index ⇒ exercises the `zext`+guard path; result 255 > 127
        // ⇒ exercises the print `i8 zeroext` ABI.
        let idx = fb.constant(Value::U8(2), L).unwrap();
        let got = fb.index(arr, idx, Dest::Fresh(None), L).unwrap();
        let tok = fb.println(tok, got, L).unwrap();
        fb.output(tok, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "255\n");
    assert_parity(&clang, &ir, &rr, "u8_index_print");
}

/// One compile-and-run job: the emitted `.ll` and its expected native observable
/// (compiled at both `-O0` and `-O2` in Phase 2).
struct Job {
    tag: String,
    ll: String,
    want_out: Option<String>,
    want_code: i32,
}

/// Build a job from an `ir` + its oracle run. `None` if the run diverged (skip).
fn make_job(ir: &CategoryIr, rr: &RunResult, tag: String) -> Option<Job> {
    let (want_out, want_code) = expect_native(ir, rr)?;
    let ll = flow_backend_llvm::emit(ir).unwrap_or_else(|e| panic!("{tag}: emit: {e:?}"));
    Some(Job {
        tag,
        ll,
        want_out,
        want_code,
    })
}

/// Closed-mode testgen sweep (≥ 256; DESIGN §6.3): random Core programs — arrays,
/// updates, multi-loop fns, traps — raw and rewritten, oracle-equal at `-O0` and
/// `-O2`. Open mode (`i32 → i32`) is excluded (no native observable, BL8).
/// Compilation is fanned out across threads (clang subprocess spawn dominates
/// wall time).
#[test]
fn differential_testgen_closed_sweep() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_testgen: clang not found");
        return;
    };
    let _ = rt_lib(); // build the runtime once, before fan-out

    // Phase 1 (serial, fast): generate programs, build raw + rewritten IR, emit.
    let t0 = Instant::now();
    let mut runner = TestRunner::deterministic();
    let mut jobs: Vec<Job> = Vec::new();
    let mut n = 0usize;
    for (count, trap_free) in [(256usize, false), (64usize, true)] {
        let strat = prog_strategy(trap_free, false);
        for _ in 0..count {
            let prog = strat.new_tree(&mut runner).unwrap().current();
            let Built { ir, open, .. } = build(&prog);
            if open {
                continue; // excluded (BL8)
            }
            let rr = run(&ir, BUDGET);
            if let Some(j) = make_job(&ir, &rr, format!("testgen#{n}/raw")) {
                jobs.push(j);
            } else {
                continue; // diverged
            }
            let res = rewrite(build(&prog).ir);
            let rr2 = run(&res.ir, BUDGET);
            if let Some(j) = make_job(&res.ir, &rr2, format!("testgen#{n}/rewritten")) {
                jobs.push(j);
            }
            n += 1;
        }
    }
    assert!(n >= 256, "expected ≥256 closed cases, got {n}");
    eprintln!(
        "differential_testgen: phase 1 (generate+emit) {:?}; {} jobs",
        t0.elapsed(),
        jobs.len()
    );
    let t1 = Instant::now();

    // Phase 2 (parallel): compile + run each job at both opt levels, collect L1
    // failures.
    let next = AtomicUsize::new(0);
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        // Fan out to the host's core count — the -O2 row doubles clang/ld
        // subprocess spawns, which dominate wall time.
        let threads = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(8);
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() {
                        break;
                    }
                    let j = &jobs[i];
                    // Same job, same oracle expectations, `-O0` then `-O2`
                    // (DESIGN §8 `-O2` row).
                    for opt in ["-O0", "-O2"] {
                        let Some((out, code)) = compile_run(&clang, &j.ll, &j.tag, opt) else {
                            failures
                                .lock()
                                .unwrap()
                                .push(format!("{} {opt}: timeout", j.tag));
                            continue;
                        };
                        if code != j.want_code {
                            failures
                                .lock()
                                .unwrap()
                                .push(format!("{} {opt}: exit {code} != {}", j.tag, j.want_code));
                        } else if let Some(w) = &j.want_out {
                            let got = String::from_utf8_lossy(&out);
                            if got != *w {
                                failures
                                    .lock()
                                    .unwrap()
                                    .push(format!("{} {opt}: stdout {got:?} != {w:?}", j.tag));
                            }
                        }
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap();
    assert!(
        failures.is_empty(),
        "{} testgen divergences:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "differential_testgen: {n} closed programs (raw + rewritten) OK at -O0 and -O2; phase 2 (compile+run) {:?}",
        t1.elapsed()
    );
}

/// ADR-0027, end-to-end NATIVE: capturing map/fold bodies compile-and-run
/// oracle-equal — the capture components reach the body call as leading
/// arguments (the goldens pin the emission shape; this proves the emitted
/// `.ll` is valid and computes the oracle's values). The third program is the
/// ADR-0021 motivating matmul in its natural one-kernel map+fold form, pinned
/// to the S16 reference values (`-275` / `3748`).
#[test]
fn differential_captures() {
    let Some(clang) = clang() else {
        return;
    };
    let cases: &[(&str, &str)] = &[
        (
            "capture_map",
            r#"
fn main() {
    3 -> scale;
    [1, 2, 3] -> a;
    a -> map { x -> x * scale } -> b;
    b[1] -> println;
}
"#,
        ),
        (
            "capture_fold",
            r#"
fn main() {
    3 -> scale;
    [1, 2, 3] -> a;
    (0, a) -> fold { acc, x -> acc + x * scale } -> total;
    total -> println;
}
"#,
        ),
        (
            "capture_one_kernel_matmul",
            r#"
fn matmul(a: [f32; 16], b: [f32; 16], ks: [i32; 4]) -> [f32; 16] {
    a -> enumerate -> map { p ->
        p.0 / 4 -> i;
        p.0 % 4 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] } -> cell;
        cell
    } -> c;
    c -> ret;
}
fn main() {
    [ -37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0] -> a: [f32; 16];
    [7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0] -> b: [f32; 16];
    [0, 1, 2, 3] -> ks: [i32; 4];
    (a, b, ks) -> matmul -> c;
    c[0] -> println;
    c[15] -> println;
}
"#,
        ),
    ];
    let want: &[&str] = &["6\n", "18\n", "-275\n3748\n"];
    for ((tag, src), expected) in cases.iter().zip(want) {
        let ir = lower_src(src);
        let rr = run(&ir, BUDGET);
        assert_eq!(rr.output, *expected, "{tag}: oracle contract");
        assert_parity(&clang, &ir, &rr, &format!("{tag}/raw"));

        let res = rewrite(lower_src(src));
        let rr2 = run(&res.ir, BUDGET);
        assert_parity(&clang, &res.ir, &rr2, &format!("{tag}/rewritten"));
    }
}
/// loop building the result via a loop-carried `mut c` array and `c[t] <- v`
/// (the U4 contract) — compiled and run through clang, oracle-equal ("8\n136\n").
/// Covers the combined loop-carried + `Update` shape that testgen's disjoint
/// Update/loop steps never compose (S13 orchestrator ruling on review finding
/// `loop-carried-update-never-clanged`).
#[test]
fn differential_matmul_loop_driven_update() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn cell(a: [f32; 16], b: [f32; 16], i: i32, j: i32) -> f32 {
    mut k: i32   <- 0;
    mut acc: f32 <- 0.0;
    loop {
        (k < 4) -> {
            -true-> {
                acc + a[i * 4 + k] * b[k * 4 + j] -> acc;
                k + 1 -> k;
                -> loop;
            }
            -false-> acc -> ret;
        }
    }
}

fn matmul4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    mut c: [f32; 16] <- b;
    mut t: i32       <- 0;
    loop {
        (t < 16) -> {
            -true-> {
                t / 4 -> i;
                t % 4 -> j;
                (a, b, i, j) -> cell -> v;
                c[t] <- v;
                t + 1 -> t;
                -> loop;
            }
            -false-> c -> ret;
        }
    }
}

fn main() {
    [ 1.0,  2.0,  3.0,  4.0,
      5.0,  6.0,  7.0,  8.0,
      9.0, 10.0, 11.0, 12.0,
     13.0, 14.0, 15.0, 16.0] -> a: [f32; 16];

    [1.0, 0.0, 0.0, 0.0,
     0.0, 1.0, 0.0, 0.0,
     0.0, 0.0, 1.0, 0.0,
     0.0, 0.0, 0.0, 1.0] -> b: [f32; 16];

    (a, b) -> matmul4 -> c;

    c[7] -> println;
    (0.0, c) -> fold { acc, x -> acc + x } -> sum;
    sum -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "8\n136\n", "oracle contract");
    assert_parity(&clang, &ir, &rr, "matmul_loop_driven/raw");

    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_parity(&clang, &res.ir, &rr2, "matmul_loop_driven/rewritten");
}

/// ADR-0027 review blocker #1, end-to-end NATIVE: a computed exit payload and
/// an exit-arm captured map — chains that feed only the loop's exit route, so
/// they leave the SCC — compile-and-run oracle-equal (raw and rewritten).
/// Pre-fix, llvm emitted the payload chain AFTER the loop (silent miscompile:
/// the exit route read a never-written alloca).
#[test]
fn differential_computed_exit_payloads() {
    let Some(clang) = clang() else {
        return;
    };
    let cases: &[(&str, &str, &str)] = &[
        (
            "computed_exit_payload",
            r#"
fn f(n: i32) -> i32 {
    mut t: i32 <- 1;
    mut k: i32 <- 0;
    loop {
        (k < n) -> {
            -true-> {
                t + 1 -> t;
                k + 1 -> k;
                -> loop;
            }
            -false-> t * 2 -> ret;
        }
    }
}
fn main() { 3 -> f -> r; r -> println; }
"#,
            "8\n",
        ),
        (
            "exit_arm_captured_map",
            r#"
fn f(xs: [i32; 3], n: i32) -> [i32; 3] {
    mut t: i32 <- 10;
    mut k: i32 <- 0;
    loop {
        (k < n) -> {
            -true-> {
                t + 1 -> t;
                k + 1 -> k;
                -> loop;
            }
            -false-> xs -> map { e -> e + t } -> r;
        }
    }
    r
}
fn main() { ([1, 2, 3], 2) -> f -> r; r[0] -> println; r[2] -> println; }
"#,
            "13\n15\n",
        ),
    ];
    for (tag, src, expected) in cases {
        let ir = lower_src(src);
        let rr = run(&ir, BUDGET);
        assert_eq!(rr.output, *expected, "{tag}: oracle contract");
        assert_parity(&clang, &ir, &rr, &format!("{tag}/raw"));

        let res = rewrite(lower_src(src));
        let rr2 = run(&res.ir, BUDGET);
        assert_parity(&clang, &res.ir, &rr2, &format!("{tag}/rewritten"));
    }
}

/// BL5 amendment (suggestions #8), end-to-end NATIVE: the by-ref call-arg
/// shapes the examples don't isolate — a Named fn returning its whole by-ref
/// input product (the `load_whole` escaping-use assembly) and a bulk op reading
/// an array straight off the by-ref input product (`zip` on the input itself)
/// — compile-and-run oracle-equal (raw and rewritten, `-O0` and `-O2`).
#[test]
fn differential_byref_call_args() {
    let Some(clang) = clang() else {
        return;
    };
    let cases: &[(&str, &str, &str)] = &[
        (
            "whole_input_return",
            r#"
fn id(p: ([i32; 2], i32)) -> ([i32; 2], i32) {
    p -> ret;
}
fn main() { ([7, 9], 5) -> id -> q; q.0[1] -> println; q.1 -> println; }
"#,
            "9\n5\n",
        ),
        (
            "zip_on_input_product",
            r#"
fn dot(ab: ([i32; 3], [i32; 3])) -> i32 {
    ab -> zip -> zs;
    (0, zs) -> fold { acc, p -> acc + p.0 * p.1 } -> r;
    r -> ret;
}
fn main() { ([1, 2, 3], [4, 5, 6]) -> dot -> r; r -> println; }
"#,
            "32\n",
        ),
    ];
    for (tag, src, expected) in cases {
        let ir = lower_src(src);
        let rr = run(&ir, BUDGET);
        assert_eq!(rr.output, *expected, "{tag}: oracle contract");
        assert_parity(&clang, &ir, &rr, &format!("{tag}/raw"));

        let res = rewrite(lower_src(src));
        let rr2 = run(&res.ir, BUDGET);
        assert_parity(&clang, &res.ir, &rr2, &format!("{tag}/rewritten"));
    }
}

/// ADR-0029 stage 1: `iota`/`fill` — the loop-skeleton `.ll` compiled and run.
/// The only end-to-end cover until the lower stage lands (built via `IrBuilder`;
/// no surface syntax yet).
#[test]
fn differential_iota_fill() {
    let Some(clang) = clang() else {
        return;
    };
    use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Value};
    const L: SourceLoc = SourceLoc { start: 0, end: 0 };
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let tok = fb.input();
        // iota(5)[3] = 3 → println; fill(7, 4)[2] = 7 → println.
        let five = fb.constant(Value::I32(5), L).unwrap();
        let seq = fb.iota(five, Dest::Fresh(None), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let x = fb.index(seq, three, Dest::Fresh(None), L).unwrap();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let four = fb.constant(Value::I32(4), L).unwrap();
        let filled = fb.fill(seven, four, Dest::Fresh(None), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let y = fb.index(filled, two, Dest::Fresh(None), L).unwrap();
        let tok = fb.println(tok, x, L).unwrap();
        let tok = fb.println(tok, y, L).unwrap();
        fb.output(tok, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let rr = run(&ir, BUDGET);
    assert_parity(&clang, &ir, &rr, "iota_fill");
}

#[test]
fn differential_widen() {
    let src = r#"
fn main() {
    iota(2) -> a;
    a[1] - 2 -> x;
    x -> widen_i64 -> i;
    x -> widen_f32 -> f;
    x -> widen_f64 -> d;
    f -> widen_f64 -> fd;
    i -> println;
    f -> println;
    d -> println;
    fd -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "-1\n-1\n-1\n-1\n", "oracle contract");

    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, rr.output, "rewritten oracle contract");

    let Some(clang) = clang() else {
        eprintln!("SKIP differential_widen: clang not found");
        return;
    };
    assert_parity(&clang, &ir, &rr, "widen/raw");
    assert_parity(&clang, &res.ir, &rr2, "widen/rewritten");
}
