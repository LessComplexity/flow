//! DESIGN §6 — the remote compile-and-run differential harness (WP6), ported
//! from `flow-backend-llvm/tests/differential.rs` with `nvcc` swapped for
//! `clang` per the §4/§6.6 recipe.
//!
//! For each program (the 10 examples, the pinned special cases, and a
//! closed-mode testgen sweep), on **raw and `rewrite()`d** IR: `emit → nvcc
//! -std=c++17 -fmad=false -arch=sm_89 prog.cu libflow_rt.a -o prog -lpthread
//! -ldl -lm` (120 s compile timeout) `→ run (15 s timeout) → compare per
//! L1/§3`: `Done` ⇒ exit 0 + stdout **byte-equal** to the oracle; `Trapped`
//! ⇒ exit 101 (stdout ignored; classes never cross); **exit 102 ⇒ infra
//! failure** — the test fails naming the program, but the row is NEVER an R1
//! data point (DESIGN §3); any other exit ⇒ failure with captured stderr;
//! timeout ⇒ failure naming the program. Oracle expectations are computed
//! before `rewrite()` (IR taken by value); `Unit → i32` closed entries get
//! the result-printing `main` wrapper (BL8, ported); Diverged programs skip;
//! open-mode testgen (`i32 → i32`) is excluded — no native observable (BL8).
//!
//! `nvcc` is discovered `$NVCC → $CUDA_HOME/bin/nvcc → which nvcc` (§6.5);
//! absent ⇒ **skip-with-reason** (HANDOFF §5.5 — never a faked pass, never a
//! failure). This macOS host has no nvcc: local runs exercise the skip path
//! plus the `local` unit tests of the pure harness pieces (discovery order,
//! the recipe argv, the exit classifier). The pinned cases additionally run
//! their oracle assertions locally before the skip gate, so a malformed pin
//! fails here, not on the box.
//!
//! Deviations from the llvm harness (the RULES are verbatim; toolchain-side
//! only):
//! - ONE compile per program — the llvm `-O0`/`-O2` double row has no M3
//!   analog (an nvcc `-O3` row is DESIGN §7 headroom).
//! - nvcc compile gets a 120 s timeout (§6.7); llvm's clang call is unbounded.
//! - stdout AND stderr are captured via temp files (llvm pipes stdout, nulls
//!   stderr): stderr feeds §6.7's "any other exit ⇒ failure with captured
//!   stderr", and files can't pipe-buffer-deadlock a chatty program.

#![allow(clippy::type_complexity)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

use flow_interp::{Outcome, RValue, RunResult, render, run};
use flow_ir::{CategoryIr, SourceLoc, Ty};
use flow_rewrite::rewrite;

// The testgen program generator (shared with flow-rewrite's differential duty).
#[path = "../../../flow-rewrite/tests/testgen/mod.rs"]
mod testgen;

use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{Built, build, prog_strategy};

const BUDGET: u64 = 10_000_000;
/// The inherited llvm run timeout (§6.7).
const TIMEOUT_SECS: u64 = 15;
/// The §6.7 nvcc compile timeout.
const COMPILE_TIMEOUT_SECS: u64 = 120;

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

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

/// The one skip reason (HANDOFF §5.5), printed by every skip path.
const SKIP_REASON: &str = "nvcc not found ($NVCC unset, $CUDA_HOME/bin/nvcc missing, \
     not on PATH) — the CUDA differential runs on the remote vast.ai 4090 box \
     (DESIGN §6); skipping, never a faked pass (HANDOFF §5.5)";

// --- toolchain ------------------------------------------------------------

/// The pure discovery-order core (§6.5), unit-tested without touching the
/// process env or PATH: `$NVCC` (respected verbatim, llvm's `$CC` rule) →
/// `$CUDA_HOME/bin/nvcc` → `which nvcc`. Candidates arrive pre-validated;
/// the first present wins. `None` ⇒ skip-with-reason.
fn pick_nvcc(
    env_nvcc: Option<String>,
    cuda_home_nvcc: Option<String>,
    which_nvcc: Option<String>,
) -> Option<String> {
    env_nvcc.or(cuda_home_nvcc).or(which_nvcc)
}

/// The `$CUDA_HOME` candidate path (§6.5's middle leg).
fn cuda_home_nvcc(home: &str) -> String {
    format!("{home}/bin/nvcc")
}

/// Discover `nvcc` (§6.5). `$NVCC` is used verbatim (non-empty); a stale
/// `$CUDA_HOME` (candidate not a file) falls through to PATH.
fn nvcc() -> Option<String> {
    if let Ok(v) = std::env::var("NVCC")
        && !v.is_empty()
    {
        return Some(v);
    }
    if let Ok(h) = std::env::var("CUDA_HOME") {
        let p = cuda_home_nvcc(&h);
        if Path::new(&p).is_file() {
            return Some(p);
        }
    }
    let out = Command::new("which").arg("nvcc").output().ok()?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
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

/// The §4/§6.6 compile recipe argv (unit-tested): flags first, then the
/// translation unit, the flow-rt staticlib, `-o exe`, and the pinned Linux
/// link tail LAST. `--use_fast_math` and host `-march=native`/`-mfma` are
/// forbidden (§4) — `local::compile_recipe_argv_pinned` asserts their absence.
fn nvcc_argv(cu: &Path, rt: &Path, exe: &Path) -> Vec<String> {
    let s = |p: &Path| p.to_str().expect("temp paths are UTF-8").to_string();
    // `-fmad=false` here is the CONFORMANCE pin (byte-parity vs the oracle),
    // deliberately NOT the product default: Sapir flipped the product/bench
    // recipe to `-fmad=true` at S24b close (DESIGN §4 amendment). This gate
    // measures semantics, not speed — contraction stays off.
    vec![
        "-std=c++17".into(),
        "-fmad=false".into(),
        "-arch=sm_89".into(),
        s(cu),
        s(rt),
        "-o".into(),
        s(exe),
        "-lpthread".into(),
        "-ldl".into(),
        "-lm".into(),
    ]
}

/// One time-boxed subprocess run, stdout/stderr captured to files in the
/// tempdir (no pipe-buffer deadlock; stderr feeds the failure reports).
struct ProcOut {
    /// `None` = killed at the timeout.
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Spawn `cmd`, time-boxed at `secs` (5 ms poll, the llvm cadence). On
/// timeout the child is killed AND reaped; partial output is still read back.
fn run_timeboxed(cmd: &mut Command, dir: &Path, stem: &str, secs: u64) -> ProcOut {
    let out_path = dir.join(format!("{stem}.stdout"));
    let err_path = dir.join(format!("{stem}.stderr"));
    let mut child = cmd
        .stdout(Stdio::from(std::fs::File::create(&out_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err_path).unwrap()))
        .spawn()
        .unwrap_or_else(|e| {
            // A bogus $NVCC (respected verbatim, llvm's CC rule) surfaces
            // here — name the tool, not just the OS error.
            panic!(
                "spawn `{}` failed: {e}",
                cmd.get_program().to_string_lossy()
            )
        });
    let start = Instant::now();
    let code = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status.code().unwrap_or(-1)); // signal death → -1
        }
        if start.elapsed() > Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait(); // reap
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    ProcOut {
        code,
        stdout: std::fs::read(&out_path).unwrap(),
        stderr: std::fs::read(&err_path).unwrap(),
    }
}

/// What the native run did (the classifier's input).
#[derive(Debug)]
enum RunOut {
    /// The process exited on its own: exit code (-1 on signal death),
    /// captured stdout bytes, captured stderr bytes.
    Exited {
        code: i32,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// Killed at the 15 s run timeout.
    TimedOut,
}

/// The L1/§3 verdict for one compile+run (§6.7).
#[derive(Debug)]
enum Verdict {
    /// exit 0 ∧ stdout byte-equal, or exit 101 matching an oracle trap.
    Pass,
    /// A parity failure — an R1 data point against the backend (wrong exit
    /// class, mismatched stdout, timeout, or any other exit).
    Parity(String),
    /// Exit 102 — a harness-visible infra failure (box/toolchain/emitter
    /// bug), NEVER an R1 data point (DESIGN §3). Fails the test, reported
    /// apart from parity so it can masquerade as neither parity nor trap.
    Infra(String),
}

/// Classify one native run against the oracle expectation (§6.7 rules).
/// `want` is `(None, 101)` for an oracle trap (stdout ignored) or
/// `(Some(stdout), 0)` for a Done program. Exit 102 is checked FIRST — it is
/// infra regardless of the expected class.
fn classify(tag: &str, want_out: &Option<String>, want_code: i32, got: &RunOut) -> Verdict {
    let RunOut::Exited {
        code,
        stdout,
        stderr,
    } = got
    else {
        return Verdict::Parity(format!("{tag}: native run timed out ({TIMEOUT_SECS}s)"));
    };
    if *code == 102 {
        return Verdict::Infra(format!(
            "{tag}: exit 102 — CUDA infra failure (box/toolchain/emitter bug, \
             never an R1 data point); stderr: {}",
            String::from_utf8_lossy(stderr)
        ));
    }
    if *code != want_code {
        return Verdict::Parity(format!(
            "{tag}: exit {code} != oracle {want_code} (trap/done classes never cross); \
             stderr: {}",
            String::from_utf8_lossy(stderr)
        ));
    }
    if let Some(want) = want_out
        && stdout.as_slice() != want.as_bytes()
    {
        let got_text = String::from_utf8_lossy(stdout);
        return Verdict::Parity(format!("{tag}: stdout {got_text:?} != oracle {want:?}"));
    }
    Verdict::Pass
}

/// Compile `.cu` + libflow_rt.a per the §4/§6.6 recipe and run, time-boxed
/// (120 s compile, 15 s run). Panics — naming the program, with stderr and
/// the full source — on nvcc failure or compile timeout (an emitter bug or a
/// broken box; llvm's clang-failure panic carried over).
fn compile_run(nvcc: &str, cu: &str, tag: &str) -> RunOut {
    let dir = tempfile::tempdir().unwrap();
    let cup = dir.path().join("p.cu");
    let exe = dir.path().join("p");
    std::fs::write(&cup, cu).unwrap();
    let argv = nvcc_argv(&cup, &rt_lib(), &exe);
    let cc = run_timeboxed(
        Command::new(nvcc).args(&argv),
        dir.path(),
        "nvcc",
        COMPILE_TIMEOUT_SECS,
    );
    match cc.code {
        Some(0) => {}
        Some(c) => panic!(
            "{tag}: nvcc exit {c}:\n{}\n---\n{cu}",
            String::from_utf8_lossy(&cc.stderr)
        ),
        None => panic!("{tag}: nvcc timed out ({COMPILE_TIMEOUT_SECS}s)"),
    }
    match run_timeboxed(&mut Command::new(&exe), dir.path(), "run", TIMEOUT_SECS) {
        ProcOut {
            code: Some(code),
            stdout,
            stderr,
        } => RunOut::Exited {
            code,
            stdout,
            stderr,
        },
        ProcOut { code: None, .. } => RunOut::TimedOut,
    }
}

// --- oracle expectation ---------------------------------------------------

/// The expected native observable from the interp oracle (L1 / BL8, ported
/// verbatim). Returns `None` for a Diverged run (skip). `Some((None, 101))`
/// for a trap (stdout ignored). `Some((Some(stdout), 0))` for a Done program —
/// the entry protocol decides the observable: an `IoToken` entry's stdout is
/// the token log; a closed `Unit → scalar` entry's is the wrapper-printed
/// return.
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
                // The main wrapper prints the scalar return with a newline.
                format!("{}\n", render(sv))
            } else {
                String::new() // non-printable return: wrapper prints nothing
            };
            Some((Some(stdout), 0))
        }
    }
}

/// Emit `ir`, nvcc-compile+run, and assert L1/§3 parity against `rr`
/// (Diverged ⇒ skip). Parity and infra verdicts both fail the test naming
/// the program; the verdict text says which class fired.
fn assert_parity(nvcc: &str, ir: &CategoryIr, rr: &RunResult, tag: &str) {
    let Some((want_out, want_code)) = expect_native(ir, rr) else {
        return; // diverged — skip
    };
    let cu = flow_backend_cuda::emit(ir).unwrap_or_else(|e| panic!("{tag}: emit failed: {e:?}"));
    match classify(tag, &want_out, want_code, &compile_run(nvcc, &cu, tag)) {
        Verdict::Pass => {}
        Verdict::Parity(m) | Verdict::Infra(m) => panic!("{m}"),
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

/// The §6.8 line, part 1: the 10 examples (`vector.flow` excluded — the
/// aspirational exhibit, not Core), raw and rewritten, compile-and-run
/// oracle-equal.
#[test]
fn differential_examples_raw_and_rewritten() {
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_examples_raw_and_rewritten: {SKIP_REASON}");
        return;
    };
    for name in EXAMPLES {
        let ir = build_example(name);
        let rr = run(&ir, BUDGET);
        assert_parity(&nvcc, &ir, &rr, &format!("{name}/raw"));

        // Rebuild (CategoryIr is not Clone) and rewrite.
        let res = rewrite(build_example(name));
        let rr2 = run(&res.ir, BUDGET);
        assert_parity(&nvcc, &res.ir, &rr2, &format!("{name}/rewritten"));
    }
}

#[test]
fn differential_iota_fill() {
    let src = r#"
fn main() {
    4 -> iota -> a;
    (7, 4) -> fill -> b;
    (a, b) -> zip -> map { p -> p.0 + p.1 } -> c;
    c[3] -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "10\n", "oracle contract");
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_iota_fill (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "iota_fill/raw");

    // Rebuild (CategoryIr is not Clone) and rewrite — the replay Iota/Fill
    // arms are on this path.
    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, "10\n", "rewritten oracle contract");
    assert_parity(&nvcc, &res.ir, &rr2, "iota_fill/rewritten");
}

#[test]
fn differential_widen() {
    let src = r#"
fn main() {
    2 -> iota -> a;
    a[1] - 2 -> x;
    x -> widen_i64 -> i;
    a -> map { y -> y - 2 -> widen_f32 } -> fs;
    a -> map { y -> y - 2 -> widen_f64 } -> ds;
    fs -> map { y -> y -> widen_f64 } -> fds;
    i -> println;
    fs[1] -> println;
    ds[1] -> println;
    fds[1] -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(rr.output, "-1\n-1\n-1\n-1\n", "oracle contract");

    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, rr.output, "rewritten oracle contract");

    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_widen (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "widen/raw");
    assert_parity(&nvcc, &res.ir, &rr2, "widen/rewritten");
}

/// Two sequential loops in one fn (S12 P0 shape) compile-and-run to "20\n" —
/// raw and rewritten (§6.8).
#[test]
fn differential_two_sequential_loops() {
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
    assert_eq!(rr.output, "20\n", "oracle contract");
    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, "20\n", "rewritten oracle contract");
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_two_sequential_loops (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "two_loops/raw");
    assert_parity(&nvcc, &res.ir, &rr2, "two_loops/rewritten");
}

/// The llvm `exit_only_payload_emitted_once` pin (`golden_ll.rs:198`),
/// ported: an exit-only computed payload (`acc + 12345`, consumed only by
/// the exit route) belongs to the decide cone but lives outside the loop
/// SCC. The driver-ownership skip must emit it exactly once — re-emission
/// after the loop is dead recompute for values and a DOUBLE side effect for
/// an exit-arm Print (the llvm miscompile class). TEXT-ONLY like llvm's, on
/// raw AND rewritten `.cu` (§6.8): the interp oracle PANICS on this shape
/// today (a decide-cone read-before-write in flow-interp's loop driver —
/// the reason llvm pins it textually), so no oracle expectation exists to
/// compile-run against; and a pure recompute would be runtime-unobservable
/// anyway — the occurrence count IS the detector. Runs wherever loop
/// emission exists (WP4 landed).
#[test]
fn differential_exit_only_payload_emitted_once() {
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { acc + 2 -> acc; i + 1 -> i; -> loop; }
            -false-> acc + 12345 -> ret;
        }
    }
}
fn main() { 4 -> f -> r; r -> println; }
"#;
    // Raw and rewritten (§6.8): the payload constant appears exactly once.
    for tag in ["raw", "rewritten"] {
        let ir = if tag == "raw" {
            lower_src(src)
        } else {
            rewrite(lower_src(src)).ir
        };
        let cu = match flow_backend_cuda::emit(&ir) {
            Ok(cu) => cu,
            Err(e) => panic!("exit_only_payload/{tag}: emit failed: {e:?}"),
        };
        assert_eq!(
            cu.matches("12345").count(),
            1,
            "exit_only_payload/{tag}: exit-only payload must be emitted exactly once \
             (driver-owned):\n{cu}"
        );
    }
}

/// Trap parity (exit 101): div-by-zero and an out-of-bounds `Update` — raw
/// and rewritten, the rewritten oracle re-asserted trapped (R1: rewrite
/// preserves the trap class).
#[test]
fn differential_traps_exit_101() {
    // Div by zero (surface).
    let div0 = r#"
fn f(a: i32, b: i32) -> i32 { a / b -> ret; }
fn main() { (1, 0) -> f -> r; r -> println; }
"#;
    let ir = lower_src(div0);
    let rr = run(&ir, BUDGET);
    assert!(
        matches!(rr.outcome, Outcome::Trapped(_)),
        "oracle must trap"
    );
    // Out-of-bounds Update (index 9 into [i32; 4]) — IrBuilder (llvm port).
    let oob = build_update_oob();
    let rr_oob = run(&oob, BUDGET);
    assert!(
        matches!(rr_oob.outcome, Outcome::Trapped(_)),
        "expected OOB trap"
    );
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_traps_exit_101 (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "div0/raw");
    let res = rewrite(lower_src(div0));
    let rr2 = run(&res.ir, BUDGET);
    assert!(
        matches!(rr2.outcome, Outcome::Trapped(_)),
        "rewritten oracle must trap"
    );
    assert_parity(&nvcc, &res.ir, &rr2, "div0/rewritten");
    assert_parity(&nvcc, &oob, &rr_oob, "update_oob/raw");
    let res2 = rewrite(build_update_oob());
    let rr2_oob = run(&res2.ir, BUDGET);
    assert!(
        matches!(rr2_oob.outcome, Outcome::Trapped(_)),
        "rewritten OOB oracle must trap"
    );
    assert_parity(&nvcc, &res2.ir, &rr2_oob, "update_oob/rewritten");
}

/// `Unit → i32` closed pure: build [0;4], `Update` at 9 (OOB), `Index` [0]
/// into the result — the wrapper would print the return; the trap fires
/// first (llvm harness construction, ported).
fn build_update_oob() -> CategoryIr {
    use flow_ir::{Dest, FuncKind, IrBuilder, Value};
    let mut b = IrBuilder::new();
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
    b.seal(f).unwrap()
}

/// Float-print parity (§6.8 pin): f64 shortest round-trip Display
/// (`0.1 + 0.2`), an f32 division, IEEE float ÷0 (±inf/NaN — the ADR-0013
/// S13 amendment: NO trap), and float `Mod` (Rust `%` ≡ C `fmod` — the
/// DESIGN §4/§10 open question this case pins). Every print flows through
/// the same flow-rt on the host, so what the pin checks is VALUE parity
/// (`-fmad=false`, BC9) rendered byte-equal.
#[test]
fn differential_float_print_parity() {
    let src = r#"
fn main() {
    0.1 + 0.2 -> s;
    s -> println;
    1.0 / 3.0 -> t: f32;
    t -> println;
    1.0 / 0.0 -> pinf;
    pinf -> println;
    0.0 - 1.0 -> neg1;
    neg1 / 0.0 -> ninf;
    ninf -> println;
    0.0 / 0.0 -> nan;
    nan -> println;
    7.5 % 2.0 -> m;
    m -> println;
    0.0 - 7.5 -> neg75;
    neg75 % 2.0 -> nm;
    nm -> println;
}
"#;
    let ir = lower_src(src);
    let rr = run(&ir, BUDGET);
    assert_eq!(
        rr.output, "0.30000000000000004\n0.33333334\ninf\n-inf\nNaN\n1.5\n-1.5\n",
        "oracle contract"
    );
    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, rr.output, "rewritten oracle contract");
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_float_print_parity (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "float_print/raw");
    assert_parity(&nvcc, &res.ir, &rr2, "float_print/rewritten");
}

/// ADR-0021's motivating program, end-to-end through nvcc: matmul4 as one
/// flattened loop building the result via a loop-carried `mut c` array and
/// `c[t] <- v` (the U4 contract) — oracle-equal ("8\n136\n"), raw and
/// rewritten. Covers the combined loop-carried + `Update` shape that
/// testgen's disjoint Update/loop steps never compose (llvm harness port).
#[test]
fn differential_matmul_loop_driven_update() {
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
    let res = rewrite(lower_src(src));
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, "8\n136\n", "rewritten oracle contract");
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_matmul_loop_driven_update (native leg): {SKIP_REASON}");
        return;
    };
    assert_parity(&nvcc, &ir, &rr, "matmul_loop_driven/raw");
    assert_parity(&nvcc, &res.ir, &rr2, "matmul_loop_driven/rewritten");
}

/// One compile-and-run job: the emitted `.cu` and its expected native
/// observable.
struct Job {
    tag: String,
    cu: String,
    want_out: Option<String>,
    want_code: i32,
}

/// Build a job from an `ir` + its oracle run. `None` if the run diverged.
fn make_job(ir: &CategoryIr, rr: &RunResult, tag: String) -> Option<Job> {
    let (want_out, want_code) = expect_native(ir, rr)?;
    let cu = flow_backend_cuda::emit(ir).unwrap_or_else(|e| panic!("{tag}: emit: {e:?}"));
    Some(Job {
        tag,
        cu,
        want_out,
        want_code,
    })
}

/// Closed-mode testgen sweep at the llvm row's scale (§6.8): random Core
/// programs — arrays, updates, multi-loop fns, traps — raw and rewritten,
/// oracle-equal. Reproducibility is by MECHANISM: `TestRunner::deterministic()`
/// plus the synced pinned `Cargo.lock`; 320 draws (256 default + 64
/// trap-free), ≥ 256 closed non-diverged required. Open mode (`i32 → i32`)
/// is excluded (no native observable, BL8). Compile+run is fanned across
/// `available_parallelism` threads (nvcc spawn dominates wall time); parity
/// failures and exit-102 infra failures are collected and reported apart.
#[test]
fn differential_testgen_closed_sweep() {
    let Some(nvcc) = nvcc() else {
        eprintln!("SKIP differential_testgen_closed_sweep: {SKIP_REASON}");
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

    // Phase 2 (parallel): nvcc-compile + run each job, collecting failures —
    // parity (R1 data points) and infra (exit 102, never R1) reported apart.
    let next = AtomicUsize::new(0);
    let parity: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let infra: Mutex<Vec<String>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        // Fan out to the box's core count — nvcc subprocess spawns dominate
        // wall time (§6: ≈ 2–6 s per small translation unit).
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
                    let got = compile_run(&nvcc, &j.cu, &j.tag);
                    match classify(&j.tag, &j.want_out, j.want_code, &got) {
                        Verdict::Pass => {}
                        Verdict::Parity(m) => parity.lock().unwrap().push(m),
                        Verdict::Infra(m) => infra.lock().unwrap().push(m),
                    }
                }
            });
        }
    });
    let parity = parity.into_inner().unwrap();
    let infra = infra.into_inner().unwrap();
    assert!(
        parity.is_empty() && infra.is_empty(),
        "{} parity divergences + {} infra failures (exit 102 — never R1 data points):\n{}\n{}",
        parity.len(),
        infra.len(),
        parity.join("\n"),
        infra.join("\n")
    );
    eprintln!(
        "differential_testgen: {n} closed programs (raw + rewritten) oracle-equal; \
         phase 2 (nvcc+run) {:?}",
        t1.elapsed()
    );
}

// --- local unit tests (no nvcc needed) -------------------------------------

/// The pure harness pieces, exercised without the toolchain — the only
/// coverage a CUDA-less host gets (discovery order, recipe argv, classifier).
#[cfg(test)]
mod local {
    use super::*;

    #[test]
    fn nvcc_discovery_order() {
        // $NVCC wins, verbatim …
        assert_eq!(
            pick_nvcc(
                Some("/opt/nvcc".into()),
                Some("/cuda/bin/nvcc".into()),
                Some("/usr/bin/nvcc".into())
            ),
            Some("/opt/nvcc".into())
        );
        // … then $CUDA_HOME/bin/nvcc …
        assert_eq!(
            pick_nvcc(
                None,
                Some("/cuda/bin/nvcc".into()),
                Some("/usr/bin/nvcc".into())
            ),
            Some("/cuda/bin/nvcc".into())
        );
        // … then PATH.
        assert_eq!(
            pick_nvcc(None, None, Some("/usr/bin/nvcc".into())),
            Some("/usr/bin/nvcc".into())
        );
        // Absent everywhere ⇒ None ⇒ skip-with-reason (never a faked run).
        assert_eq!(pick_nvcc(None, None, None), None);
        // The middle-leg candidate path shape.
        assert_eq!(cuda_home_nvcc("/opt/cuda"), "/opt/cuda/bin/nvcc");
    }

    #[test]
    fn compile_recipe_argv_pinned() {
        let argv = nvcc_argv(Path::new("p.cu"), Path::new("libflow_rt.a"), Path::new("p"));
        // §4 recipe, pinned in order: ISO C++17, no FMA contraction, sm_89.
        assert_eq!(&argv[..3], ["-std=c++17", "-fmad=false", "-arch=sm_89"]);
        // Then the translation unit and the flow-rt staticlib, then -o exe.
        assert_eq!(&argv[3..5], ["p.cu", "libflow_rt.a"]);
        assert_eq!(&argv[5..7], ["-o", "p"]);
        // §6.6 link tail LAST, in pinned order.
        assert_eq!(&argv[argv.len() - 3..], ["-lpthread", "-ldl", "-lm"]);
        // Forbidden (§4): fast math, host contraction flags, any opt row.
        for bad in [
            "--use_fast_math",
            "-march=native",
            "-mfma",
            "-O3",
            "--fmad=true",
        ] {
            assert!(!argv.iter().any(|a| a == bad), "forbidden flag {bad}");
        }
        assert!(
            !argv.iter().any(|a| a.starts_with("-march")),
            "no -march=* ever: {argv:?}"
        );
    }

    fn done(out: &str) -> (Option<String>, i32) {
        (Some(out.into()), 0)
    }

    fn trapped() -> (Option<String>, i32) {
        (None, 101)
    }

    fn exit(code: i32, out: &[u8], err: &[u8]) -> RunOut {
        RunOut::Exited {
            code,
            stdout: out.to_vec(),
            stderr: err.to_vec(),
        }
    }

    #[test]
    fn classify_done_byte_equal() {
        let (wo, wc) = done("42\n");
        assert!(matches!(
            classify("p", &wo, wc, &exit(0, b"42\n", b"")),
            Verdict::Pass
        ));
        // Byte-different stdout ⇒ parity failure naming the program.
        match classify("prog42", &wo, wc, &exit(0, b"42\nx", b"")) {
            Verdict::Parity(m) => assert!(m.contains("prog42"), "{m}"),
            v => panic!("expected Parity, got {v:?}"),
        }
    }

    #[test]
    fn classify_trap_classes_never_cross() {
        let (wo, wc) = trapped();
        // 101 matching an oracle trap passes — stdout IGNORED.
        assert!(matches!(
            classify(
                "p",
                &wo,
                wc,
                &exit(101, b"garbage", b"flow trap: div_zero\n")
            ),
            Verdict::Pass
        ));
        // Oracle trapped but native exited 0 ⇒ class cross ⇒ Parity.
        match classify("p", &wo, wc, &exit(0, b"", b"")) {
            Verdict::Parity(m) => assert!(m.contains("classes never cross"), "{m}"),
            v => panic!("expected Parity, got {v:?}"),
        }
        // Oracle done but native trapped ⇒ class cross ⇒ Parity.
        let (wo0, wc0) = done("");
        match classify("p", &wo0, wc0, &exit(101, b"", b"")) {
            Verdict::Parity(m) => assert!(m.contains("classes never cross"), "{m}"),
            v => panic!("expected Parity, got {v:?}"),
        }
    }

    #[test]
    fn classify_exit_102_is_infra_never_a_data_point() {
        // Regardless of the oracle's class — a 102 can never complete an R1
        // row, even when the oracle wanted a trap.
        for (wo, wc) in [done(""), trapped()] {
            match classify("gpu_prog", &wo, wc, &exit(102, b"", b"cudaErrorNoDevice")) {
                Verdict::Infra(m) => {
                    assert!(m.contains("gpu_prog"), "{m}");
                    assert!(m.contains("102"), "{m}");
                    assert!(m.contains("cudaErrorNoDevice"), "{m}");
                }
                v => panic!("expected Infra, got {v:?}"),
            }
        }
    }

    #[test]
    fn classify_timeout_and_other_exits() {
        let (wo, wc) = done("x\n");
        match classify("slow_prog", &wo, wc, &RunOut::TimedOut) {
            Verdict::Parity(m) => {
                assert!(m.contains("slow_prog"), "{m}");
                assert!(m.contains("timed out"), "{m}");
            }
            v => panic!("expected Parity, got {v:?}"),
        }
        // Any other exit ⇒ failure WITH captured stderr.
        match classify("crash_prog", &wo, wc, &exit(1, b"", b"segv-ish")) {
            Verdict::Parity(m) => {
                assert!(m.contains("crash_prog"), "{m}");
                assert!(m.contains("segv-ish"), "{m}");
            }
            v => panic!("expected Parity, got {v:?}"),
        }
        // Signal death (code −1) is the same class.
        assert!(matches!(
            classify("p", &wo, wc, &exit(-1, b"", b"")),
            Verdict::Parity(_)
        ));
    }
}
