//! Acceptance goldens — the M1 line (interp DESIGN §11.1, §11.2).
//!
//! For each `examples/*.flow`: `parse → lower → run(budget)` and assert the
//! exact `RunResult.output`. Plus the by-execution value contracts:
//! `sum_to_n(10) == 55`, `fir4(...) == 5.375`, and the `countdown` token-through-
//! loop golden (built from the committed source const).

use flow_interp::{Outcome, RValue, run};
use flow_ir::{CategoryIr, FuncId, Ty, Value};

const BUDGET: u64 = 100_000;

/// Read one of the acceptance programs from `examples/`.
fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Parse + lower a Flow source into a sealed IR (asserting parse + lower clean).
fn build(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    flow_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower failed: {ds:?}"))
}

/// The countdown program is not in `examples/`; its source is the committed
/// flow-lower golden fixture (interp DESIGN §11.5).
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
    let out = flow_interp::eval_call(&ir, f, RValue::Scalar(Value::I32(10)), BUDGET);
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
    let out = flow_interp::eval_call(&ir, f, arg, BUDGET);
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
