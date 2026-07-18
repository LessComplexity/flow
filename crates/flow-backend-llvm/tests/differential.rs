//! DESIGN §4/§6 test plan item 1/3: the compile-and-run differential harness.
//!
//! For each program (the 10 examples, two sequential loops, trap cases, and a
//! closed-mode testgen sweep), on **raw and `rewrite()`d** IR: `emit → clang
//! <prog>.ll libflow_rt.a -o prog → run (time-boxed) → compare per L1` — `Done`
//! ⇒ exit 0 + stdout byte-equal to the oracle; `Trapped` ⇒ exit 101 (stdout
//! ignored). `clang` absent ⇒ the whole harness skips-with-reason (never a faked
//! pass). Open-mode testgen (`i32 → i32`) is excluded — no native `@main` analog
//! (BL8).

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

/// Compile `.ll` + libflow_rt.a and run, time-boxed. `None` on a timeout (the
/// harness fails loudly rather than hanging). Panics if clang errors.
fn compile_run(clang: &str, ll: &str, tag: &str) -> Option<(Vec<u8>, i32)> {
    let dir = tempfile::tempdir().unwrap();
    let llp = dir.path().join("p.ll");
    let exe = dir.path().join("p");
    std::fs::write(&llp, ll).unwrap();
    let out = Command::new(clang)
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

/// Emit `ir`, compile+run, and assert L1 parity against `rr`.
fn assert_parity(clang: &str, ir: &CategoryIr, rr: &RunResult, tag: &str) {
    let Some((want_out, want_code)) = expect_native(ir, rr) else {
        return; // diverged — skip
    };
    let ll = flow_backend_llvm::emit(ir).unwrap_or_else(|e| panic!("{tag}: emit failed: {e:?}"));
    let (got_out, got_code) = compile_run(clang, &ll, tag)
        .unwrap_or_else(|| panic!("{tag}: native run timed out ({TIMEOUT_SECS}s)"));
    assert_eq!(got_code, want_code, "{tag}: exit code");
    if let Some(want) = want_out {
        assert_eq!(
            String::from_utf8_lossy(&got_out),
            want,
            "{tag}: stdout mismatch"
        );
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

/// The M2 line: the 10 examples, raw and rewritten, compile-and-run oracle-equal.
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

/// One compile-and-run job: the emitted `.ll` and its expected native observable.
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
/// updates, multi-loop fns, traps — raw and rewritten, oracle-equal. Open mode
/// (`i32 → i32`) is excluded (no native observable, BL8). Compilation is fanned
/// out across threads (clang subprocess spawn dominates wall time).
#[test]
fn differential_testgen_closed_sweep() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_testgen: clang not found");
        return;
    };
    let _ = rt_lib(); // build the runtime once, before fan-out

    // Phase 1 (serial, fast): generate programs, build raw + rewritten IR, emit.
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

    // Phase 2 (parallel): compile + run each job, collect L1 failures.
    let next = AtomicUsize::new(0);
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() {
                        break;
                    }
                    let j = &jobs[i];
                    let Some((out, code)) = compile_run(&clang, &j.ll, &j.tag) else {
                        failures.lock().unwrap().push(format!("{}: timeout", j.tag));
                        continue;
                    };
                    if code != j.want_code {
                        failures
                            .lock()
                            .unwrap()
                            .push(format!("{}: exit {code} != {}", j.tag, j.want_code));
                    } else if let Some(w) = &j.want_out {
                        let got = String::from_utf8_lossy(&out);
                        if got != *w {
                            failures
                                .lock()
                                .unwrap()
                                .push(format!("{}: stdout {got:?} != {w:?}", j.tag));
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
    eprintln!("differential_testgen: {n} closed programs (raw + rewritten) OK");
}

/// ADR-0021's motivating program, end-to-end NATIVE: matmul4 as one flattened
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
