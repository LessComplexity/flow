//! DESIGN §8.4 — per-rule micro-goldens for layer 3 (§2 const-fold + algebraic
//! identities), covering the plan-consistency pins P1/P2 (§1.2) and the token
//! exclusion. Each is a CONFIRMED blocker repro whose guard, if removed, drops or
//! aliases a value that the builder then rejects at re-seal — inputs no generated
//! program produces, so these are the only regression net for those guards.

use flow_interp::{Outcome, RValue, eval_call, run};
use flow_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};
use flow_rewrite::{analyze_const_fold, is_canonical, rewrite};

const BUDGET: u64 = 100_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// F1 (I4 token linearity) — const-fold must NOT alias a token-bearing
/// `proj∘pack` result: `main(io) -> io { tup = (io, 5); tup.0 }`. Aliasing
/// `tup.0 → io` would give `io` two token consumers (the re-packed tuple and the
/// Return) ⇒ `TokenNotLinear` at re-seal. The pack's slot 0 carries the token, so
/// `analyze_const_fold` must leave the Proj as an op edge.
#[test]
fn token_proj_pack_not_forwarded() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let io = fb.input();
        let c5 = fb.constant(Value::I32(5), L).unwrap();
        let tup = fb
            .pack(&[io, c5], Dest::Fresh(Some("tup".into())), L)
            .unwrap();
        let t0 = fb.proj(tup, 0, Dest::Fresh(Some("t0".into())), L).unwrap();
        fb.output(t0, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "input seals clean");
    assert!(is_canonical(&ir));

    // The token-slot Proj must NOT be aliased (would be 1 before the guard).
    assert!(
        analyze_const_fold(&ir).alias.is_empty(),
        "token-bearing proj∘pack must not be forwarded"
    );

    let before = run(&ir, BUDGET);
    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty(), "rewrite re-seals clean");
    assert_eq!(run(&res.ir, BUDGET), before, "run unchanged");
}

/// P1 — a primitive targeting Return directly on constants (`fn f() -> i32
/// { 2 + 3 }`) is left alone. Folding it would drop the sole Return writer
/// (I-RET). `rewrite` applies nothing; re-seal is clean; the value is still 5.
#[test]
fn const_add_into_return_survives() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let _x = fb.input();
        let c2 = fb.constant(Value::I32(2), L).unwrap();
        let c3 = fb.constant(Value::I32(3), L).unwrap();
        fb.binop(Operation::Add, c2, c3, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();

    assert!(
        analyze_const_fold(&ir).is_empty(),
        "P1: a const binop into Return must not fold"
    );

    let entry = ir.entry();
    let before = eval_call(&ir, entry, RValue::Scalar(Value::I32(0)), BUDGET);
    let res = rewrite(ir);
    assert!(res.report.applied.is_empty(), "P1: nothing applies");
    assert!(validate(&res.ir).is_empty());
    let after = eval_call(
        &res.ir,
        res.ir.entry(),
        RValue::Scalar(Value::I32(0)),
        BUDGET,
    );
    assert_eq!(before, Outcome::Done(RValue::Scalar(Value::I32(5))));
    assert_eq!(after, before);
}

/// P1 — `x + 0 -> ret` (identity binop into Return) is left alone for the same
/// reason: forwarding it would drop the sole Return writer.
#[test]
fn identity_add_into_return_survives() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let c0 = fb.constant(Value::I32(0), L).unwrap();
        fb.binop(Operation::Add, x, c0, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();

    assert!(
        analyze_const_fold(&ir).is_empty(),
        "P1: an identity binop into Return must not forward"
    );

    let before = eval_call(&ir, ir.entry(), RValue::Scalar(Value::I32(9)), BUDGET);
    let res = rewrite(ir);
    assert!(res.report.applied.is_empty());
    assert!(validate(&res.ir).is_empty());
    let after = eval_call(
        &res.ir,
        res.ir.entry(),
        RValue::Scalar(Value::I32(9)),
        BUDGET,
    );
    assert_eq!(before, Outcome::Done(RValue::Scalar(Value::I32(9))));
    assert_eq!(after, before);
}

/// P2 — a loop-carried `x * 0` (the next-state morphism) is NOT constified.
/// Constifying it would replace the `LoopBack` state source with an in-degree-0
/// constant outside the merge's SCC ⇒ `LoopBackOutsideScc` at re-seal. The SCC
/// guard leaves every in-SCC object untouched.
#[test]
fn loop_carried_mul_zero_not_constified() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let lh = fb.begin_loop(x, L).unwrap();
        let s = fb.merge_of(&lh);
        let zero = fb.constant(Value::I32(0), L).unwrap();
        // next = s * 0 — in-SCC (feeds the back edge). A P2 violation would
        // constify it to 0 and detach the back-edge source from the SCC.
        let next = fb
            .binop(Operation::Mul, s, zero, Dest::Fresh(Some("next".into())), L)
            .unwrap();
        let cond = fb
            .binop(Operation::Gt, s, zero, Dest::Fresh(Some("cond".into())), L)
            .unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        fb.loop_exit(&lh, s, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    assert!(is_canonical(&ir));

    let plan = analyze_const_fold(&ir);
    assert!(
        plan.constify.is_empty() && plan.alias.is_empty(),
        "P2: no in-SCC object may be constified or aliased"
    );

    let before = eval_call(&ir, ir.entry(), RValue::Scalar(Value::I32(5)), BUDGET);
    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty(), "rewrite re-seals clean");
    let after = eval_call(
        &res.ir,
        res.ir.entry(),
        RValue::Scalar(Value::I32(5)),
        BUDGET,
    );
    assert_eq!(after, before);
}
