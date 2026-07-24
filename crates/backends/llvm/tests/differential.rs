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
use flow_ir::{CategoryIr, Operation, Ty};
use flow_rewrite::{PassId, rewrite};

// The testgen program generator (shared with flow-rewrite's differential duty).
#[path = "../../../flow-rewrite/tests/testgen/mod.rs"]
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
        "{}/../../../target/debug/libflow_rt.a",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// Compile `.ll` + libflow_rt.a at optimization level `opt` and run, time-boxed.
/// `None` on a timeout (the harness fails loudly rather than hanging). Panics if
/// clang errors.
fn compile_run(clang: &str, ll: &str, tag: &str, opt: &str) -> Option<(Vec<u8>, i32)> {
    let (_dir, exe) = compile_exe(clang, ll, tag, opt);
    run_exe(&exe, TIMEOUT_SECS, None)
}

fn compile_exe(clang: &str, ll: &str, tag: &str, opt: &str) -> (tempfile::TempDir, PathBuf) {
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
    (dir, exe)
}

fn run_exe(exe: &Path, secs: u64, flow_par: Option<&str>) -> Option<(Vec<u8>, i32)> {
    let mut command = Command::new(exe);
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_remove("FLOW_PAR");
    if let Some(value) = flow_par {
        command.env("FLOW_PAR", value);
    }
    let mut child = command.spawn().unwrap();
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
        "{}/../../../examples/{}.flow",
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
    let mut lifted = 0usize;
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
            if res
                .report
                .applied
                .iter()
                .any(|(pass, count)| *pass == PassId::LiftLoops && *count > 0)
            {
                lifted += 1;
            }
            let rr2 = run(&res.ir, BUDGET);
            if let Some(j) = make_job(&res.ir, &rr2, format!("testgen#{n}/rewritten")) {
                jobs.push(j);
            }
            n += 1;
        }
    }
    assert!(n >= 256, "expected ≥256 closed cases, got {n}");
    assert!(lifted > 0, "testgen sweep did not exercise loop lifting");
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

#[test]
fn differential_tiled_matmul() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    [ -37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0] -> a: [f32; 16];
    [7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0] -> b: [f32; 16];
    16 -> iota -> cells;
    4 -> iota -> ks;
    cells -> map { cell ->
        cell / 4 -> i;
        cell % 4 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] }
    } -> c;
    c[0] -> println;
    c[15] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("matmul oracle completes") else {
        panic!("matmul oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    // S26 nest: flat [TILE_I * TILE_J] acc scratch, the head/interior/tail row
    // split (biased-`lo` + direct-`hi` udivs bound the interior full-window
    // rows), and a constant-TILE_J lane bound on the main j body. SSA tmp names
    // renumber with any emitter edit, so the lane bound pins the line shape
    // (`icmp uge i64 .., 16`), not a name.
    assert!(
        tiled.contains(" = alloca [64 x float]")
            && tiled.contains(" = udiv i64 %lo, 4")
            && tiled.contains(" = add i64 %hi, 3")
            && tiled.contains(" = add i64 %lo, 3")
            && tiled.contains(" = udiv i64 %hi, 4")
            && tiled
                .lines()
                .any(|l| l.contains(" = icmp uge i64 ") && l.ends_with(", 16")),
        "split task must contain the TI-blocked tiled row-clipping nest:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();
    assert!(
        untiled.contains(" = call float @fn"),
        "untiled map must retain its body call:\n{untiled}"
    );

    for opt in ["-O0", "-O2"] {
        let tiled_run = compile_run(&clang, &tiled, &format!("tiled_matmul/{opt}"), opt)
            .unwrap_or_else(|| panic!("tiled_matmul/{opt}: tiled run timed out"));
        let untiled_run = compile_run(&clang, &untiled, &format!("untiled_matmul/{opt}"), opt)
            .unwrap_or_else(|| panic!("tiled_matmul/{opt}: untiled run timed out"));
        assert_eq!(tiled_run.1, 0, "tiled_matmul/{opt}: tiled exit code");
        assert_eq!(untiled_run.1, 0, "tiled_matmul/{opt}: untiled exit code");
        assert_eq!(
            String::from_utf8_lossy(&tiled_run.0),
            want,
            "tiled_matmul/{opt}: oracle stdout"
        );
        assert_eq!(
            tiled_run.0, untiled_run.0,
            "tiled_matmul/{opt}: tiled/untiled stdout"
        );
    }
}

#[test]
fn differential_tiled_matmul_via_helper_fn() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn dot(a: [f32; 16], b: [f32; 16], kr: [i32; 4], cell: i32) -> f32 {
    cell / 4 -> i;
    cell % 4 -> j;
    (0.0, kr) -> fold { acc, k -> acc + a[i * 4 + k] * b[k * 4 + j] } -> ret;
}
fn main() {
    [ -37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0] -> a: [f32; 16];
    [7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0] -> b: [f32; 16];
    16 -> iota -> cells;
    4 -> iota -> kr;
    cells -> map { cell -> (a, b, kr, cell) -> dot } -> c;
    c[0] -> println;
    c[15] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    assert!(
        ir.morphisms()
            .all(|(_, m)| !matches!(m.op, Operation::Call(_))),
        "default rewrite must strip the helper Call inside the MapBody"
    );
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("helper matmul oracle completes") else {
        panic!("helper matmul oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        tiled
            .lines()
            .any(|line| line.ends_with(" = alloca [64 x float], align 64"))
            && tiled
                .lines()
                .any(|line| line.ends_with(" = alloca [64 x float]")),
        "helper-free MapBody must expose the packed tiled nest and accumulator:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();

    assert_tiled_parity(&clang, &tiled, &untiled, &want, "tiled_matmul_via_helper");
}

#[test]
fn differential_default_rewritten_matmul4_lifts_and_tiles() {
    let Some(clang) = clang() else {
        return;
    };
    let ir = rewrite(build_example("matmul4")).ir;
    assert!(
        ir.morphisms()
            .all(|(_, m)| !matches!(m.op, Operation::Call(_))),
        "the lifted cell/matmul callees must inline"
    );
    assert!(
        ir.funcs().all(|(f, _)| ir.loop_structure(f).is_empty()),
        "the default-rewritten matmul4 must be loop-SCC-free"
    );
    assert!(
        ir.morphisms().any(|(_, m)| {
            let Operation::Map { body, .. } = m.op else {
                return false;
            };
            ir.func(body).expect("MapBody").morphisms.iter().any(|&bm| {
                matches!(
                    ir.morphism(bm).expect("body morphism").op,
                    Operation::Fold { .. }
                )
            })
        }),
        "the lifted graph must contain a Map with a Fold body"
    );

    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "-275\n3748\n", "interp oracle contract");
    let ll = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        ll.lines()
            .any(|line| line.ends_with(" = alloca [64 x float], align 64")),
        "default-rewritten matmul4 must tile with a packed align-64 panel:\n{ll}"
    );

    for opt in ["-O0", "-O2"] {
        let (_dir, exe) = compile_exe(&clang, &ll, "default_rewritten_matmul4", opt);
        let default = run_exe(&exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("default_rewritten_matmul4/{opt}: timed out"));
        let one = run_exe(&exe, TIMEOUT_SECS, Some("1"))
            .unwrap_or_else(|| panic!("default_rewritten_matmul4/{opt}/FLOW_PAR=1: timed out"));
        assert_eq!(default.1, 0, "default_rewritten_matmul4/{opt}: exit");
        assert_eq!(
            String::from_utf8_lossy(&default.0),
            rr.output,
            "default_rewritten_matmul4/{opt}: oracle stdout"
        );
        assert_eq!(
            default, one,
            "default_rewritten_matmul4/{opt}: FLOW_PAR=1 parity"
        );
    }
}

#[test]
fn differential_tiled_fir() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    71 -> iota -> tx;
    tx -> map { t -> (t * 7 + 13) % 101 - 50 -> widen_f32 } -> x;
    8 -> iota -> kr;
    kr -> map { t -> (t * 5 + 3) % 31 - 15 -> widen_f32 } -> w;
    64 -> iota -> ts;
    ts -> map { t ->
        (0.0, kr) -> fold { acc, k -> acc + w[k] * x[t + k] }
    } -> y;
    y[0] -> println;
    y[63] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("FIR oracle completes") else {
        panic!("FIR oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        tiled.contains(" = alloca [16 x float]")
            && tiled.contains(" = udiv i64 %lo, 64")
            && tiled.contains(" = add i64 %hi, 63"),
        "split task must contain the collapsed one-row tile nest:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();

    for opt in ["-O0", "-O2"] {
        let tiled_run = compile_run(&clang, &tiled, &format!("tiled_fir/{opt}"), opt)
            .unwrap_or_else(|| panic!("tiled_fir/{opt}: tiled run timed out"));
        let untiled_run = compile_run(&clang, &untiled, &format!("untiled_fir/{opt}"), opt)
            .unwrap_or_else(|| panic!("tiled_fir/{opt}: untiled run timed out"));
        assert_eq!(tiled_run.1, 0, "tiled_fir/{opt}: tiled exit code");
        assert_eq!(untiled_run.1, 0, "tiled_fir/{opt}: untiled exit code");
        assert_eq!(
            String::from_utf8_lossy(&tiled_run.0),
            want,
            "tiled_fir/{opt}: oracle stdout"
        );
        assert_eq!(
            tiled_run.0, untiled_run.0,
            "tiled_fir/{opt}: tiled/untiled stdout"
        );
    }
}

/// The tiled differential run loop: compile tiled + untiled at -O0 and -O2,
/// run each — exit 0, tiled stdout byte-equal to the interp oracle, and
/// tiled == untiled byte-equal (per-cell bit-exactness is the tile nest's
/// hard invariant).
fn assert_tiled_parity(clang: &str, tiled: &str, untiled: &str, want: &str, tag: &str) {
    for opt in ["-O0", "-O2"] {
        let tag_o = format!("{tag}/{opt}");
        let tiled_run = compile_run(clang, tiled, &format!("{tag_o}/tiled"), opt)
            .unwrap_or_else(|| panic!("{tag_o}/tiled: run timed out"));
        let untiled_run = compile_run(clang, untiled, &format!("{tag_o}/untiled"), opt)
            .unwrap_or_else(|| panic!("{tag_o}/untiled: run timed out"));
        assert_eq!(tiled_run.1, 0, "{tag_o}/tiled: exit code");
        assert_eq!(untiled_run.1, 0, "{tag_o}/untiled: exit code");
        assert_eq!(
            String::from_utf8_lossy(&tiled_run.0),
            want,
            "{tag_o}/tiled: oracle stdout"
        );
        assert_eq!(tiled_run.0, untiled_run.0, "{tag_o}: tiled/untiled stdout");
    }
}

/// S26 coverage gap (a): a j **remainder after a full main tile** plus a
/// `rows % TILE_I` i-tail. rows=5, C=20, K=7: each row runs one constant-16
/// main j-tile then the runtime `tj = 4` remainder; `rows % 4 == 1` sends row
/// 4 through the TI=1 tail path (the 4x4 case above has C < TILE_J, so its
/// main body never executes, and rows=4 leaves no tail).
#[test]
fn differential_tiled_matmul_r5_c20_k7() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    [-30.0, -25.0, -20.0, -15.0, -10.0, -5.0, 0.0, 5.0,
      10.0, 15.0, 20.0, 25.0, 30.0, -32.0, -27.0, -22.0,
     -17.0, -12.0, -7.0, -2.0, 3.0, 8.0, 13.0, 18.0,
      23.0, 28.0, 33.0, -29.0, -24.0, -19.0, -14.0, -9.0,
      -4.0, 1.0, 6.0] -> a: [f32; 35];
    [-37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0,
     -26.0, -19.0, -12.0, -5.0, 2.0, 9.0, 16.0, 23.0,
      30.0, 37.0, 44.0, -50.0, -43.0, -36.0, -29.0, -22.0,
     -15.0, -8.0, -1.0, 6.0, 13.0, 20.0, 27.0, 34.0,
      41.0, 48.0, -46.0, -39.0, -32.0, -25.0, -18.0, -11.0,
      -4.0, 3.0, 10.0, 17.0, 24.0, 31.0, 38.0, 45.0,
     -49.0, -42.0, -35.0, -28.0, -21.0, -14.0, -7.0, 0.0,
      7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0,
      18.0, 25.0, 32.0, 39.0, 46.0, -48.0, -41.0, -34.0,
     -27.0, -20.0, -13.0, -6.0, 1.0, 8.0, 15.0, 22.0,
      29.0, 36.0, 43.0, 50.0, -44.0, -37.0, -30.0, -23.0,
     -16.0, -9.0, -2.0, 5.0, 12.0, 19.0, 26.0, 33.0,
      40.0, 47.0, -47.0, -40.0, -33.0, -26.0, -19.0, -12.0,
      -5.0, 2.0, 9.0, 16.0, 23.0, 30.0, 37.0, 44.0,
     -50.0, -43.0, -36.0, -29.0, -22.0, -15.0, -8.0, -1.0,
      6.0, 13.0, 20.0, 27.0] -> b: [f32; 140];
    100 -> iota -> cells;
    7 -> iota -> ks;
    cells -> map { cell ->
        cell / 20 -> i;
        cell % 20 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 7 + k] * b[k * 20 + j] }
    } -> c;
    c[0] -> println;
    c[80] -> println;
    c[99] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("matmul r5_c20_k7 oracle completes")
    else {
        panic!("matmul r5_c20_k7 oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        tiled.contains(" = alloca [64 x float]")
            && tiled.contains(" = udiv i64 %lo, 20")
            && tiled.contains(" = udiv i64 %hi, 20"),
        "split task must contain the TI-blocked tiled nest:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();
    assert!(
        untiled.contains(" = call float @fn"),
        "untiled map must retain its body call:\n{untiled}"
    );

    assert_tiled_parity(&clang, &tiled, &untiled, &want, "tiled_matmul_r5_c20_k7");
}

/// S26 coverage gap (b): **two full main j-tiles, no remainder** plus a
/// two-row i-tail. rows=6, C=32, K=5: j runs the constant-16 main body twice
/// and the remainder check falls through (`32 % 16 == 0`); `rows % 4 == 2`
/// sends rows 4-5 through the TI=1 tail path.
#[test]
fn differential_tiled_matmul_r6_c32_k5() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    [-30.0, -25.0, -20.0, -15.0, -10.0, -5.0, 0.0, 5.0,
      10.0, 15.0, 20.0, 25.0, 30.0, -32.0, -27.0, -22.0,
     -17.0, -12.0, -7.0, -2.0, 3.0, 8.0, 13.0, 18.0,
      23.0, 28.0, 33.0, -29.0, -24.0, -19.0] -> a: [f32; 30];
    [-37.0, -30.0, -23.0, -16.0, -9.0, -2.0, 5.0, 12.0,
      19.0, 26.0, 33.0, 40.0, 47.0, -47.0, -40.0, -33.0,
     -26.0, -19.0, -12.0, -5.0, 2.0, 9.0, 16.0, 23.0,
      30.0, 37.0, 44.0, -50.0, -43.0, -36.0, -29.0, -22.0,
     -15.0, -8.0, -1.0, 6.0, 13.0, 20.0, 27.0, 34.0,
      41.0, 48.0, -46.0, -39.0, -32.0, -25.0, -18.0, -11.0,
      -4.0, 3.0, 10.0, 17.0, 24.0, 31.0, 38.0, 45.0,
     -49.0, -42.0, -35.0, -28.0, -21.0, -14.0, -7.0, 0.0,
      7.0, 14.0, 21.0, 28.0, 35.0, 42.0, 49.0, -45.0,
     -38.0, -31.0, -24.0, -17.0, -10.0, -3.0, 4.0, 11.0,
      18.0, 25.0, 32.0, 39.0, 46.0, -48.0, -41.0, -34.0,
     -27.0, -20.0, -13.0, -6.0, 1.0, 8.0, 15.0, 22.0,
      29.0, 36.0, 43.0, 50.0, -44.0, -37.0, -30.0, -23.0,
     -16.0, -9.0, -2.0, 5.0, 12.0, 19.0, 26.0, 33.0,
      40.0, 47.0, -47.0, -40.0, -33.0, -26.0, -19.0, -12.0,
      -5.0, 2.0, 9.0, 16.0, 23.0, 30.0, 37.0, 44.0,
     -50.0, -43.0, -36.0, -29.0, -22.0, -15.0, -8.0, -1.0,
      6.0, 13.0, 20.0, 27.0, 34.0, 41.0, 48.0, -46.0,
     -39.0, -32.0, -25.0, -18.0, -11.0, -4.0, 3.0, 10.0,
      17.0, 24.0, 31.0, 38.0, 45.0, -49.0, -42.0, -35.0] -> b: [f32; 160];
    192 -> iota -> cells;
    5 -> iota -> ks;
    cells -> map { cell ->
        cell / 32 -> i;
        cell % 32 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 5 + k] * b[k * 32 + j] }
    } -> c;
    c[0] -> println;
    c[128] -> println;
    c[191] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("matmul r6_c32_k5 oracle completes")
    else {
        panic!("matmul r6_c32_k5 oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        tiled.contains(" = alloca [64 x float]")
            && tiled.contains(" = udiv i64 %lo, 32")
            && tiled.contains(" = udiv i64 %hi, 32"),
        "split task must contain the TI-blocked tiled nest:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();
    assert!(
        untiled.contains(" = call float @fn"),
        "untiled map must retain its body call:\n{untiled}"
    );

    assert_tiled_parity(&clang, &tiled, &untiled, &want, "tiled_matmul_r6_c32_k5");
}

/// S27 f64 width pin: TJ=8 gives two full panels plus a four-lane remainder;
/// K=5 exercises the unrolled pair body and its trailing single-k step, while
/// rows=6 retains the two-row TI tail.
#[test]
fn differential_tiled_matmul_r6_c20_k5_f64() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    30 -> iota -> ais;
    ais -> map { x -> (x * 5 + 7) % 67 - 33 -> widen_f64 } -> a;
    100 -> iota -> bis;
    bis -> map { x -> (x * 7 + 11) % 101 - 50 -> widen_f64 } -> b;
    120 -> iota -> cells;
    5 -> iota -> ks;
    cells -> map { cell ->
        cell / 20 -> i;
        cell % 20 -> j;
        (0.0, ks) -> fold { acc, k -> acc + a[i * 5 + k] * b[k * 20 + j] }
    } -> c;
    c[0] -> println;
    c[80] -> println;
    c[119] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("matmul r6_c20_k5_f64 oracle completes")
    else {
        panic!("matmul r6_c20_k5_f64 oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        tiled.contains(" = alloca [32 x double]")
            && tiled.contains(" = alloca [120 x double], align 64")
            && tiled.contains("call void @llvm.prefetch.p0("),
        "f64 tiled nest must use TJ=8, packed panels, and prefetch:\n{tiled}"
    );
    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();
    let nopack = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            packing: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();

    assert_tiled_parity(
        &clang,
        &tiled,
        &untiled,
        &want,
        "tiled_matmul_r6_c20_k5_f64",
    );
    for opt in ["-O0", "-O2"] {
        let packed_run = compile_run(
            &clang,
            &tiled,
            &format!("tiled_matmul_r6_c20_k5_f64/{opt}/packed"),
            opt,
        )
        .unwrap_or_else(|| panic!("tiled_matmul_r6_c20_k5_f64/{opt}/packed: timed out"));
        let nopack_run = compile_run(
            &clang,
            &nopack,
            &format!("tiled_matmul_r6_c20_k5_f64/{opt}/nopack"),
            opt,
        )
        .unwrap_or_else(|| panic!("tiled_matmul_r6_c20_k5_f64/{opt}/nopack: timed out"));
        assert_eq!(
            packed_run, nopack_run,
            "tiled_matmul_r6_c20_k5_f64/{opt}: --no-pack parity"
        );
    }
}

#[test]
fn differential_tiled_matmul_loop_carried_pack() {
    let Some(clang) = clang() else {
        return;
    };
    let src = r#"
fn main() {
    [2.0, 0.0, 0.0, 0.0,
     0.0, 2.0, 0.0, 0.0,
     0.0, 0.0, 2.0, 0.0,
     0.0, 0.0, 0.0, 2.0] -> a: [f32; 16];
    [1.0, 2.0, 3.0, 4.0,
     5.0, 6.0, 7.0, 8.0,
     9.0, 10.0, 11.0, 12.0,
     13.0, 14.0, 15.0, 16.0] -> b0: [f32; 16];
    4 -> iota -> kr;
    16 -> iota -> cells;
    mut b: [f32; 16] <- b0;
    mut iteration: i32 <- 0;
    loop {
        (iteration < 2) -> {
            -true-> {
                cells -> map { cell ->
                    cell / 4 -> i;
                    cell % 4 -> j;
                    (0.0, kr) -> fold { acc, k ->
                        acc + a[i * 4 + k] * b[k * 4 + j]
                    }
                } -> c;
                c -> b;
                iteration + 1 -> iteration;
                -> loop;
            }
            -false-> b -> out;
        }
    }
    8 -> iota -> xs;
    xs -> map { x -> x * 3 + 1 } -> extra;
    out[0] -> println;
    out[15] -> println;
    extra[7] -> println;
}
"#;
    let ir = rewrite(lower_src(src)).ir;
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("loop-carried pack oracle completes")
    else {
        panic!("loop-carried pack oracle must complete");
    };
    let tiled = flow_backend_llvm::emit(&ir).unwrap();
    let entry = tiled
        .split("define internal void @flow_main(")
        .nth(1)
        .and_then(|s| s.split("\n}\n").next())
        .expect("parallel flow_main");
    assert!(
        entry.contains("call ptr @flow_par_begin(")
            && entry.contains("call void @flow_par_launch("),
        "entry must retain a multi-path parallel plan:\n{tiled}"
    );
    let loop_task = tiled
        .split("define internal void @task")
        .skip(1)
        .filter_map(|s| s.split("\n}\n").next())
        .find(|task| task.contains("alloca [64 x float], align 64"))
        .unwrap_or_else(|| panic!("loop task must allocate and pack b inline:\n{tiled}"));
    assert!(
        loop_task.contains("getelementptr [64 x float], ptr") && !tiled.contains("%pack_field"),
        "Seq-loop packing must stay in its task, not the frame:\n{tiled}"
    );

    let untiled = flow_backend_llvm::emit_with_opts(
        &ir,
        &flow_backend_llvm::EmitOpts {
            tiling: false,
            ..flow_backend_llvm::EmitOpts::default()
        },
    )
    .unwrap();
    for opt in ["-O0", "-O2"] {
        let (_tiled_dir, tiled_exe) =
            compile_exe(&clang, &tiled, "tiled_matmul_loop_carried_pack", opt);
        let (_untiled_dir, untiled_exe) =
            compile_exe(&clang, &untiled, "untiled_matmul_loop_carried_pack", opt);
        let default = run_exe(&tiled_exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("loop-carried pack/{opt}/default: timed out"));
        let one = run_exe(&tiled_exe, TIMEOUT_SECS, Some("1"))
            .unwrap_or_else(|| panic!("loop-carried pack/{opt}/FLOW_PAR=1: timed out"));
        let untiled = run_exe(&untiled_exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("loop-carried pack/{opt}/untiled: timed out"));
        assert_eq!(default.1, 0, "loop-carried pack/{opt}: exit code");
        assert_eq!(
            String::from_utf8_lossy(&default.0),
            want,
            "loop-carried pack/{opt}: oracle stdout"
        );
        assert_eq!(default, one, "loop-carried pack/{opt}: FLOW_PAR=1 parity");
        assert_eq!(default, untiled, "loop-carried pack/{opt}: tiled parity");
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
    2 -> iota -> a;
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

#[test]
fn differential_parallel_bign() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_parallel_bign: clang not found");
        return;
    };
    let src = r#"
fn main() {
    65536 -> iota -> cells;
    4 -> iota -> ks;
    cells -> map { cell ->
        cell / 256 -> row;
        cell % 256 -> col;
        (0, ks) -> fold { acc, k -> acc + (row + k) * (col + k) } -> value;
        value
    } -> out;
    out[65535] -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    let ll = flow_backend_llvm::emit(&ir).unwrap();
    assert!(
        ll.contains("i32 1, ptr @task") && ll.contains("i64 65536"),
        "the large map must use a split task:\n{ll}"
    );
    assert_parity(&clang, &ir, &rr, "parallel_bign");
}

#[test]
fn differential_parallel_trap_order() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_parallel_trap_order: clang not found");
        return;
    };
    let src = r#"
fn main() {
    32 -> iota -> xs;
    xs[0] -> first;
    first -> println;
    xs -> map { x ->
        x - first -> shifted;
        shifted - 7 -> divisor;
        100 / divisor
    } -> doomed;
    "after" -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Trapped(_)));
    let prefix = run(
        &lower_src("fn main() { 32 -> iota -> xs; xs[0] -> first; first -> println; }"),
        BUDGET,
    )
    .output;
    assert_eq!(prefix, "0\n");
    let ll = flow_backend_llvm::emit(&ir).unwrap();
    assert!(ll.contains("call void @flow_par_trap(i64 "));
    for opt in ["-O0", "-O2"] {
        let (_dir, exe) = compile_exe(&clang, &ll, "parallel_trap_order", opt);
        let (out, code) = run_exe(&exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("parallel_trap_order/{opt}: timed out"));
        assert_eq!(code, 101, "parallel_trap_order/{opt}: exit code");
        assert_eq!(
            String::from_utf8_lossy(&out),
            prefix,
            "parallel_trap_order/{opt}: stdout prefix"
        );
    }
}

const PARALLEL_STABLE_SRC: &str = r#"
fn main() {
    8192 -> iota -> xs;
    3 -> scale;
    xs -> map { x -> x * scale } -> ys;
    (0, ys) -> fold { acc, x -> acc + x } -> total;
    total -> println;
}
"#;

#[test]
fn differential_parallel_env_matrix() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_parallel_env_matrix: clang not found");
        return;
    };
    let ir = lower_src(PARALLEL_STABLE_SRC);
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("oracle completes") else {
        panic!("parallel env fixture must complete");
    };
    let ll = flow_backend_llvm::emit(&ir).unwrap();
    for opt in ["-O0", "-O2"] {
        let (_dir, exe) = compile_exe(&clang, &ll, "parallel_env_matrix", opt);
        for flow_par in [Some("1"), Some("8"), None] {
            let (out, code) = run_exe(&exe, TIMEOUT_SECS, flow_par)
                .unwrap_or_else(|| panic!("parallel_env_matrix/{opt}/{flow_par:?}: timed out"));
            assert_eq!(code, 0, "parallel_env_matrix/{opt}/{flow_par:?}");
            assert_eq!(
                String::from_utf8_lossy(&out),
                want,
                "parallel_env_matrix/{opt}/{flow_par:?}"
            );
        }
    }
}

#[test]
fn differential_parallel_run_twice() {
    let Some(clang) = clang() else {
        eprintln!("SKIP differential_parallel_run_twice: clang not found");
        return;
    };
    let ir = lower_src(PARALLEL_STABLE_SRC);
    let rr = run(&ir, BUDGET);
    let (Some(want), 0) = expect_native(&ir, &rr).expect("oracle completes") else {
        panic!("parallel repeat fixture must complete");
    };
    let ll = flow_backend_llvm::emit(&ir).unwrap();
    for opt in ["-O0", "-O2"] {
        let (_dir, exe) = compile_exe(&clang, &ll, "parallel_run_twice", opt);
        let first = run_exe(&exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("parallel_run_twice/{opt}/first: timed out"));
        let second = run_exe(&exe, TIMEOUT_SECS, None)
            .unwrap_or_else(|| panic!("parallel_run_twice/{opt}/second: timed out"));
        assert_eq!(first, second, "parallel_run_twice/{opt}: schedule drift");
        assert_eq!(first.1, 0, "parallel_run_twice/{opt}: exit code");
        assert_eq!(
            String::from_utf8_lossy(&first.0),
            want,
            "parallel_run_twice/{opt}: oracle"
        );
    }
}
