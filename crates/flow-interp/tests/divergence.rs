//! Fueled divergence (E1; interp DESIGN §11.3).
//!
//! - A hand-built loop with a constant-`true` guard (`LoopBack` always taken,
//!   `LoopExit` present-but-never-fired) ⇒ `Diverged` under a small budget, and
//!   it MUST RETURN (no hang).
//! - `sum_to_n(10)` with a budget large enough ⇒ `Done`; one too small ⇒
//!   `Diverged` (boundary tested).

use flow_interp::{Outcome, RValue};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

fn loc() -> SourceLoc {
    SourceLoc::empty_at(0)
}

fn build(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    flow_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower failed: {ds:?}"))
}

/// A function `fn loop_forever(start: i32) -> i32` whose guard is a constant
/// `true`: the back edge fires forever; the exit edge is structurally present
/// but never selected. State is `i32`, next-state is `state + 1`.
fn constant_true_loop() -> (CategoryIr, flow_ir::FuncId) {
    let mut ib = IrBuilder::new();
    let f = ib
        .declare(FuncKind::Named, "loop_forever", Ty::i32(), Ty::i32(), loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(f).unwrap();
        let start = fb.input();
        let lh = fb.begin_loop(start, loc()).unwrap();
        let merge = fb.merge_of(&lh);
        // next_state = merge + 1
        let one = fb.constant(Value::I32(1), loc()).unwrap();
        let next = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), loc())
            .unwrap();
        // cond = true (a constant). Back fires on true, exit on false.
        let t = fb.constant(Value::Bool(true), loc()).unwrap();
        fb.loop_back(&lh, next, t, loc()).unwrap();
        // Exit route gated by the same constant `true` ⇒ never selected.
        let exit = fb
            .loop_exit(&lh, merge, t, Dest::Fresh(None), loc())
            .unwrap();
        fb.output(exit, None, loc()).unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = ib.seal(f).unwrap();
    (ir, f)
}

#[test]
fn constant_true_loop_diverges_and_returns() {
    let (ir, f) = constant_true_loop();
    // Small budget: must RETURN Diverged, never hang.
    let out = flow_interp::eval_call(&ir, f, RValue::Scalar(Value::I32(0)), 50);
    assert_eq!(out, Outcome::Diverged);
}

#[test]
fn constant_true_loop_diverges_at_larger_budget_too() {
    let (ir, f) = constant_true_loop();
    let out = flow_interp::eval_call(&ir, f, RValue::Scalar(Value::I32(0)), 5_000);
    assert_eq!(out, Outcome::Diverged);
}

#[test]
fn sum_to_n_budget_boundary() {
    let ir = build(&example_sum_to_n());
    let f = ir
        .funcs()
        .find(|(_, d)| d.name == "sum_to_n")
        .map(|(id, _)| id)
        .unwrap();
    let arg = || RValue::Scalar(Value::I32(10));

    // Find the minimal budget that yields Done by linear search from below.
    let mut min_done = None;
    for b in 1..=2000u64 {
        match flow_interp::eval_call(&ir, f, arg(), b) {
            Outcome::Done(RValue::Scalar(Value::I32(55))) => {
                min_done = Some(b);
                break;
            }
            Outcome::Done(other) => panic!("unexpected Done value: {other:?}"),
            Outcome::Diverged => {}
            Outcome::Trapped(k) => panic!("unexpected trap: {k:?}"),
        }
    }
    let min_done = min_done.expect("sum_to_n(10) never completed within 2000 budget");

    // Boundary: exactly min_done ⇒ Done(55); min_done - 1 ⇒ Diverged.
    assert_eq!(
        flow_interp::eval_call(&ir, f, arg(), min_done),
        Outcome::Done(RValue::Scalar(Value::I32(55)))
    );
    assert_eq!(
        flow_interp::eval_call(&ir, f, arg(), min_done - 1),
        Outcome::Diverged
    );
}

fn example_sum_to_n() -> String {
    let path = format!(
        "{}/../../examples/sum_to_n.flow",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap()
}
