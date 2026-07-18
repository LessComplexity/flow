//! Value contracts for `Update` (ADR-0021, plan-array-update WP U1).
//!
//! The interpreter is the oracle: these assertions are the normative denotation
//! every future backend must reproduce —
//!   `⟦update⟧((a, i, v)) = [a[0], …, a[i-1], v, a[i+1], …, a[n-1]]`  (i in [0,n))
//!   `⟦update⟧((a, i, v)) = Trapped(IndexOob)`                        (i < 0 ∨ i ≥ n)
//! The index zero/sign-extends exactly like `Index` (shared `as_int`): a `u8`
//! index ≥ 128 stays positive (never sign-flips to negative).
//!
//! IR is built directly with the `flow-ir` builder (lower's `c[i] <- x` sugar is
//! WP U3, landing later) — keeps these contracts independent of that crate.

use flow_interp::{Outcome, RValue, TrapKind, eval_call};
use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Ty, Value, validate};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };
const BUDGET: u64 = 100_000;

fn i32_arr(n: u64) -> Ty {
    Ty::Array {
        elem: Box::new(Ty::i32()),
        size: n,
    }
}

fn scal(n: i32) -> RValue {
    RValue::Scalar(Value::I32(n))
}

/// `[i32;n] -> [i32;n]` that writes `v` at a `u8` index (constant `i`, `v`).
/// A `u8` index exercises the zero-extension path (i ≥ 128 must stay in-bounds
/// on a large array, never wrap negative).
fn update_u8_program(n: u64, i: u8, v: i32) -> flow_ir::CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", i32_arr(n), i32_arr(n), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let ci = fb.constant(Value::U8(i), L).unwrap();
        let cv = fb.constant(Value::I32(v), L).unwrap();
        fb.update(a, ci, cv, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

/// `[i32;n] -> [i32;n]` writing `v` at a signed `i32` index (may be negative).
fn update_i32_program(n: u64, i: i32, v: i32) -> flow_ir::CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", i32_arr(n), i32_arr(n), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let ci = fb.constant(Value::I32(i), L).unwrap();
        let cv = fb.constant(Value::I32(v), L).unwrap();
        fb.update(a, ci, cv, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

#[test]
fn update_in_bounds_replaces_one_slot() {
    let ir = update_i32_program(4, 2, 99);
    assert!(validate(&ir).is_empty());
    let arg = RValue::Array(vec![scal(10), scal(20), scal(30), scal(40)]);
    let out = eval_call(&ir, f_of(&ir), arg, BUDGET);
    // Only slot 2 changes; the rest are the untouched base (value semantics).
    assert_eq!(
        out,
        Outcome::Done(RValue::Array(vec![scal(10), scal(20), scal(99), scal(40)]))
    );
}

#[test]
fn update_at_n_traps() {
    // n == 3, i == 3 ⇒ OOB (upper bound, same class as Index).
    let ir = update_i32_program(3, 3, 0);
    let arg = RValue::Array(vec![scal(1), scal(2), scal(3)]);
    assert_eq!(
        eval_call(&ir, f_of(&ir), arg, BUDGET),
        Outcome::Trapped(TrapKind::IndexOob)
    );
}

#[test]
fn update_negative_traps() {
    // i == -1 ⇒ OOB (lower bound).
    let ir = update_i32_program(3, -1, 0);
    let arg = RValue::Array(vec![scal(1), scal(2), scal(3)]);
    assert_eq!(
        eval_call(&ir, f_of(&ir), arg, BUDGET),
        Outcome::Trapped(TrapKind::IndexOob)
    );
}

#[test]
fn update_u8_index_ge_128_in_bounds() {
    // u8 index 200 zero-extends to +200 (never a negative sign-flip); on a
    // 256-long array it is a legal in-bounds write, not a trap.
    let ir = update_u8_program(256, 200, 7);
    let arg = RValue::Array((0..256).map(scal).collect());
    let out = eval_call(&ir, f_of(&ir), arg, BUDGET);
    let c = match out {
        Outcome::Done(RValue::Array(es)) => es,
        other => panic!("expected Done(Array), got {other:?}"),
    };
    assert_eq!(c[200], scal(7), "slot 200 written");
    assert_eq!(c[199], scal(199), "neighbor untouched");
    assert_eq!(c[201], scal(201), "neighbor untouched");
}

/// The single Named entry of a one-function graph built above.
fn f_of(ir: &flow_ir::CategoryIr) -> flow_ir::FuncId {
    ir.entry()
}
