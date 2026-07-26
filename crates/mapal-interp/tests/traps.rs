//! Trap goldens (ADR-0013; interp DESIGN §11.4).
//!
//! Hand-built IR (no surface for div/OOB in the six examples):
//! - integer `Div` by `0` ⇒ `Trapped(DivZero)`
//! - `Index` with `i = n` and `i = -1` ⇒ `Trapped(IndexOob)`
//! - float `1.0 / 0.0` ⇒ `Done` (IEEE inf), NOT a trap.

use mapal_interp::{Outcome, RValue, TrapKind, run};
use mapal_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const BUDGET: u64 = 10_000;

fn loc() -> SourceLoc {
    SourceLoc::empty_at(0)
}

/// `fn main() : io -> io` that prints `a / b` (integer) for two int constants.
/// Returns a sealed IR. (Div trap aborts before the print.)
fn div_program(a: i32, b: i32) -> mapal_ir::CategoryIr {
    let mut ib = IrBuilder::new();
    let main = ib
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(main).unwrap();
        let tok = fb.input();
        let ca = fb.constant(Value::I32(a), loc()).unwrap();
        let cb = fb.constant(Value::I32(b), loc()).unwrap();
        let q = fb
            .binop(Operation::Div, ca, cb, Dest::Fresh(None), loc())
            .unwrap();
        let out = fb.println(tok, q, loc()).unwrap();
        fb.output(out, None, loc()).unwrap();
        fb.finish().unwrap();
    }
    ib.seal(main).unwrap()
}

#[test]
fn integer_div_by_zero_traps() {
    let ir = div_program(7, 0);
    let r = run(&ir, BUDGET);
    assert_eq!(r.outcome, Outcome::Trapped(TrapKind::DivZero));
    assert_eq!(r.output, "");
}

#[test]
fn integer_mod_by_zero_traps() {
    let mut ib = IrBuilder::new();
    let main = ib
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(main).unwrap();
        let tok = fb.input();
        let ca = fb.constant(Value::I32(7), loc()).unwrap();
        let cb = fb.constant(Value::I32(0), loc()).unwrap();
        let q = fb
            .binop(Operation::Mod, ca, cb, Dest::Fresh(None), loc())
            .unwrap();
        let out = fb.println(tok, q, loc()).unwrap();
        fb.output(out, None, loc()).unwrap();
        fb.finish().unwrap();
    }
    let ir = ib.seal(main).unwrap();
    assert_eq!(
        run(&ir, BUDGET).outcome,
        Outcome::Trapped(TrapKind::DivZero)
    );
}

#[test]
fn integer_div_nonzero_is_done() {
    let ir = div_program(8, 2);
    let r = run(&ir, BUDGET);
    assert_eq!(r.outcome, Outcome::Done(RValue::Token("4\n".into())));
    assert_eq!(r.output, "4\n");
}

/// `fn main() : io -> io` that indexes `[10, 20, 30] : [i32;3]` at constant `i`.
fn index_program(i: i32) -> mapal_ir::CategoryIr {
    let mut ib = IrBuilder::new();
    let main = ib
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(main).unwrap();
        let tok = fb.input();
        let e0 = fb.constant(Value::I32(10), loc()).unwrap();
        let e1 = fb.constant(Value::I32(20), loc()).unwrap();
        let e2 = fb.constant(Value::I32(30), loc()).unwrap();
        let arr = fb
            .pack_array(&[e0, e1, e2], Dest::Fresh(None), loc())
            .unwrap();
        let ci = fb.constant(Value::I32(i), loc()).unwrap();
        let elem = fb.index(arr, ci, Dest::Fresh(None), loc()).unwrap();
        let out = fb.println(tok, elem, loc()).unwrap();
        fb.output(out, None, loc()).unwrap();
        fb.finish().unwrap();
    }
    ib.seal(main).unwrap()
}

#[test]
fn index_in_bounds_is_done() {
    let ir = index_program(1);
    assert_eq!(run(&ir, BUDGET).output, "20\n");
}

#[test]
fn index_at_n_traps() {
    // n == 3, i == 3 ⇒ OOB.
    let ir = index_program(3);
    assert_eq!(
        run(&ir, BUDGET).outcome,
        Outcome::Trapped(TrapKind::IndexOob)
    );
}

#[test]
fn index_negative_traps() {
    let ir = index_program(-1);
    assert_eq!(
        run(&ir, BUDGET).outcome,
        Outcome::Trapped(TrapKind::IndexOob)
    );
}

/// `fn main() : io -> io` that prints `(NaN op 1.0)` for a float comparison `op`.
fn cmp_nan_program(op: Operation) -> mapal_ir::CategoryIr {
    let mut ib = IrBuilder::new();
    let main = ib
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(main).unwrap();
        let tok = fb.input();
        let nan = fb.constant(Value::F32(f32::NAN), loc()).unwrap();
        let one = fb.constant(Value::F32(1.0), loc()).unwrap();
        let b = fb.binop(op, nan, one, Dest::Fresh(None), loc()).unwrap();
        let out = fb.println(tok, b, loc()).unwrap();
        fb.output(out, None, loc()).unwrap();
        fb.finish().unwrap();
    }
    ib.seal(main).unwrap()
}

#[test]
fn nan_ordering_is_ieee() {
    // IEEE: every ordered comparison with NaN is false (incl. Le/Ge, which must
    // NOT be `!Lt`); Eq is false; Neq is true. The oracle is diffed against
    // backends, so this must match real IEEE exactly (interp DESIGN §3).
    for op in [
        Operation::Lt,
        Operation::Gt,
        Operation::Le,
        Operation::Ge,
        Operation::Eq,
    ] {
        assert_eq!(
            run(&cmp_nan_program(op), BUDGET).output,
            "false\n",
            "{op:?}"
        );
    }
    assert_eq!(
        run(&cmp_nan_program(Operation::Neq), BUDGET).output,
        "true\n"
    );
}

#[test]
fn float_div_by_zero_is_done_not_trap() {
    // 1.0 / 0.0 ⇒ IEEE +inf ⇒ Done, NOT a trap (IN-6).
    let mut ib = IrBuilder::new();
    let main = ib
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(main).unwrap();
        let tok = fb.input();
        let ca = fb.constant(Value::F32(1.0), loc()).unwrap();
        let cb = fb.constant(Value::F32(0.0), loc()).unwrap();
        let q = fb
            .binop(Operation::Div, ca, cb, Dest::Fresh(None), loc())
            .unwrap();
        let out = fb.println(tok, q, loc()).unwrap();
        fb.output(out, None, loc()).unwrap();
        fb.finish().unwrap();
    }
    let ir = ib.seal(main).unwrap();
    let r = run(&ir, BUDGET);
    assert_eq!(r.output, "inf\n");
    assert!(matches!(r.outcome, Outcome::Done(_)));
}
