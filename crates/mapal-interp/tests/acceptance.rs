//! Acceptance goldens — the M1 line (interp DESIGN §11.1, §11.2).
//!
//! For each `examples/*.mapal`: `parse → lower → run(budget)` and assert the
//! exact `RunResult.output`. Plus the by-execution value contracts:
//! `sum_to_n(10) == 55`, `fir4(...) == 5.375`, and the `countdown` token-through-
//! loop golden (built from the committed source const).

use mapal_interp::{Outcome, RValue, run};
use mapal_ir::{CategoryIr, FuncId, Ty, Value};

const BUDGET: u64 = 100_000;

/// Read one of the acceptance programs from `examples/`.
fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/{}.mapal",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Parse + lower a Mapal source into a sealed IR (asserting parse + lower clean).
fn build(src: &str) -> CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    mapal_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower failed: {ds:?}"))
}

/// The countdown program is not in `examples/`; its source is the committed
/// mapal-lower golden fixture (interp DESIGN §11.5).
const COUNTDOWN_SRC: &str = r#"
fn countdown(mut n: i32) {
    loop {
        n -> println;
        (n > 0) -> {
            -true-> { n - 1 -> n; -> loop; }
            -false-> -> ret;
        }
    }
}
fn main() { 5 -> countdown; }
"#;

#[test]
fn golden_abs() {
    let ir = build(&example("abs"));
    assert_eq!(run(&ir, BUDGET).output, "7\n");
}

#[test]
fn golden_sum_to_n() {
    let ir = build(&example("sum_to_n"));
    assert_eq!(run(&ir, BUDGET).output, "55\n");
}

#[test]
fn golden_pipeline() {
    let ir = build(&example("pipeline"));
    assert_eq!(run(&ir, BUDGET).output, "f(10) = 25\n");
}

#[test]
fn golden_fanout() {
    let ir = build(&example("fanout"));
    assert_eq!(run(&ir, BUDGET).output, "36\n12\n");
}

#[test]
fn golden_fir() {
    let ir = build(&example("fir"));
    assert_eq!(run(&ir, BUDGET).output, "5.375\n");
}

#[test]
fn golden_sepia() {
    let ir = build(&example("sepia"));
    assert_eq!(run(&ir, BUDGET).output, "4080\n");
}

#[test]
fn golden_zip_demo() {
    // ADR-0018 builtin showcase: `zip` add (c[k]=k+100) then `enumerate` add
    // (e[k]=k+k=2k). Preserves the pre-ADR-0018 c[0]=100 / c[15]=115 contract.
    let ir = build(&example("zip_demo"));
    assert_eq!(
        run(&ir, BUDGET).output,
        "c[0]  = 100\nc[15] = 115\ne[0]  = 0\ne[15] = 30\n"
    );
}

#[test]
fn golden_vector_add() {
    // ADR-0018: elementwise add via `zip`, then fold sum = 1720.
    let ir = build(&example("vector_add"));
    assert_eq!(
        run(&ir, BUDGET).output,
        "c[0]  = 100\nc[15] = 115\nsum   = 1720\n"
    );
}

#[test]
fn golden_countdown() {
    let ir = build(COUNTDOWN_SRC);
    assert_eq!(run(&ir, BUDGET).output, "5\n4\n3\n2\n1\n0\n");
}

#[test]
fn golden_seq_demo() {
    // ADR-0019: pure fanout (sq=36, db=12) then a `seq` block prints them in a
    // guaranteed order. The no-IR-delta claim end to end — `seq` produces no
    // node; the IoToken thread orders the two `Println`s.
    let ir = build(&example("seq_demo"));
    assert_eq!(run(&ir, BUDGET).output, "36\n12\n");
}

/// `time` (plan-time-builtin): the interpreter is the oracle for the clock —
/// `Operation::TimeMs` reads a process-lifetime `Instant` epoch (eval.rs), so
/// two reads bracketing real work are non-decreasing. The contract is
/// monotonicity and finiteness only; an absolute duration is a property of the
/// machine, never of the denotation.
#[test]
fn time_brackets_are_monotone_and_finite() {
    let src = r#"
fn main() {
    () -> time -> t0;
    64 -> iota -> a;
    a -> map { x -> x * x } -> b;
    () -> time -> t1;
    b[63] -> println;
    t0 -> println;
    t1 -> println;
}
"#;
    let ir = build(src);
    let out = run(&ir, BUDGET).output;
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "output: {out:?}");
    // The bracketed work still computes: 63 * 63.
    assert_eq!(lines[0], "3969");
    let t0: f64 = lines[1]
        .parse()
        .unwrap_or_else(|e| panic!("t0 {e}: {out:?}"));
    let t1: f64 = lines[2]
        .parse()
        .unwrap_or_else(|e| panic!("t1 {e}: {out:?}"));
    assert!(t0.is_finite() && t1.is_finite(), "{t0} {t1}");
    assert!(t0 >= 0.0, "epoch-relative ms are non-negative: {t0}");
    assert!(t1 >= t0, "monotonic non-decreasing: {t0} then {t1}");
}

// --- by-execution value contracts (interp DESIGN §11.2) -------------------

fn func_named(ir: &CategoryIr, name: &str) -> FuncId {
    ir.funcs()
        .find(|(_, d)| d.name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no fn named {name}"))
}

#[test]
fn sum_to_n_value_contract() {
    let ir = build(&example("sum_to_n"));
    let f = func_named(&ir, "sum_to_n");
    let out = mapal_interp::eval_call(&ir, f, RValue::Scalar(Value::I32(10)), BUDGET);
    assert_eq!(out, Outcome::Done(RValue::Scalar(Value::I32(55))));
}

/// WP2 fixer regression (mapal-lower finding F3): a loop-carried `mut` reassigned
/// inside a `seq` (or a fanout branch) in the loop body must still be threaded
/// through the loop merge. The carried-set discovery descends into both
/// containers (ADR-0019 §8.10); without the descent `acc` was dropped, the loop
/// recomputed it from the constant 0 each iteration, and `sum_to_n(10)` yielded 0.
#[test]
fn sum_to_n_seq_wrapped_reassign_value_contract() {
    // `acc + i -> acc` wrapped in a `seq` inside the loop body.
    let src = "fn sum_to_n(n: i32) -> i32 {\n    mut i: i32 <- 1;\n    mut acc: i32 <- 0;\n    loop {\n        (i <= n) -> {\n            -true-> {\n                acc -> seq { acc + i -> acc; };\n                i + 1 -> i;\n                -> loop;\n            }\n            -false-> acc -> ret;\n        }\n    }\n}\nfn main() {}\n";
    let ir = build(src);
    let f = func_named(&ir, "sum_to_n");
    let out = mapal_interp::eval_call(&ir, f, RValue::Scalar(Value::I32(10)), BUDGET);
    assert_eq!(out, Outcome::Done(RValue::Scalar(Value::I32(55))));
}

#[test]
fn fir4_value_contract() {
    let ir = build(&example("fir"));
    let f = func_named(&ir, "fir4");
    // fir4's input is one Parameter of ty ([f32;8], [f32;4]).
    let signal = RValue::Array(
        (1..=8)
            .map(|n| RValue::Scalar(Value::F32(n as f32)))
            .collect(),
    );
    let coeffs = RValue::Array(vec![
        RValue::Scalar(Value::F32(0.5)),
        RValue::Scalar(Value::F32(0.25)),
        RValue::Scalar(Value::F32(0.125)),
        RValue::Scalar(Value::F32(0.0625)),
    ]);
    let arg = RValue::Tuple(vec![signal, coeffs]);
    let out = mapal_interp::eval_call(&ir, f, arg, BUDGET);
    assert_eq!(out, Outcome::Done(RValue::Scalar(Value::F32(5.375))));
}

#[test]
fn sepia_input_ty_is_named_product() {
    // Sanity: fir4's parameter is exactly the tuple ty we feed it.
    let ir = build(&example("fir"));
    let f = func_named(&ir, "fir4");
    let input = ir.func(f).unwrap().input;
    let ty = &ir.object(input).unwrap().ty;
    assert_eq!(
        ty,
        &Ty::Tuple(vec![
            Ty::Array {
                elem: Box::new(Ty::f32()),
                size: 8
            },
            Ty::Array {
                elem: Box::new(Ty::f32()),
                size: 4
            },
        ])
    );
}
