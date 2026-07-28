//! plan-s39: guards gate the flow — an arm that is not taken does not fire.
//!
//! Negative controls, recorded PRE (main@8b40442): T1 and T2 both trapped
//! (`Trapped(DivZero)`), because every Phi arm computed regardless of the
//! condition. POST: the condition picks the arm; only that arm's work fires.

use mapal_interp::{Outcome, run};

const BUDGET: u64 = 1_000_000;

fn build(src: &str) -> mapal_ir::CategoryIr {
    let po = mapal_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        po.diagnostics
    );
    mapal_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower failed: {ds:?}"))
}

/// T1 — the untaken scalar arm's `7 / 0` must not fire.
#[test]
fn untaken_arm_division_does_not_trap() {
    let ir = build(
        "fn main() {\n  (1 > 0) -> {\n    -true-> 42;\n    -false-> 7 / 0;\n  } -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "42\n");
}

/// T2 — a whole `map` in the untaken arm must not fire (one rule, two sizes).
#[test]
fn untaken_arm_map_does_not_fire() {
    let ir = build(
        "fn main() {\n  4 -> iota -> src;\n  (1 > 0) -> {\n    -true-> 99;\n    -false-> {\n      src -> map { x -> x / 0 } -> bad;\n      bad[0]\n    }\n  } -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "99\n");
}

/// Control — a trap in the TAKEN arm still fires.
#[test]
fn taken_arm_division_still_traps() {
    let ir = build(
        "fn main() {\n  (0 > 1) -> {\n    -true-> 42;\n    -false-> 7 / 0;\n  } -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert_eq!(
        rr.outcome,
        Outcome::Trapped(mapal_interp::TrapKind::DivZero),
        "{:?}",
        rr.outcome
    );
}

/// Control — both directions: the condition picks the OTHER arm.
#[test]
fn condition_false_picks_false_arm() {
    let ir = build(
        "fn main() {\n  (0 > 1) -> {\n    -true-> 7 / 0;\n    -false-> 17;\n  } -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "17\n");
}

/// A guard inside a called function (the calc shape): the dispatch works even
/// with a zero divisor in an unselected arm.
#[test]
fn calc_dispatch_with_zero_divisor_unselected() {
    let ir = build(
        "fn calc(op: i32, a: i32, b: i32) -> i32 {\n  op -> {\n    -0-> a + b;\n    -1-> a - b;\n    -2-> a * b;\n    -3-> a / b;\n    -_-> a % b;\n  } -> ret;\n}\nfn main() {\n  (0, 20, 0) -> calc -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "20\n");
}

/// The selected division still traps through the same dispatch.
#[test]
fn calc_dispatch_selected_division_traps() {
    let ir = build(
        "fn calc(op: i32, a: i32, b: i32) -> i32 {\n  op -> {\n    -0-> a + b;\n    -1-> a - b;\n    -2-> a * b;\n    -3-> a / b;\n    -_-> a % b;\n  } -> ret;\n}\nfn main() {\n  (3, 20, 0) -> calc -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert_eq!(
        rr.outcome,
        Outcome::Trapped(mapal_interp::TrapKind::DivZero),
        "{:?}",
        rr.outcome
    );
}

/// Nested guards (the sepia clamp shape) — inner site fires only inside the
/// chosen outer arm.
#[test]
fn nested_guard_gates_correctly() {
    let ir = build(
        "fn f(v: i32) -> i32 {\n  (v > 10) -> {\n    -true-> 10;\n    -false-> {\n      (v < 0) -> {\n        -true-> 0 / 0;\n        -false-> v;\n      } -> bounded;\n      bounded\n    }\n  } -> ret;\n}\nfn main() {\n  5 -> f -> println;\n  20 -> f -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "5\n10\n");
}

/// Value equality with the pre-change semantics on a trap-free guard: `abs`.
#[test]
fn trap_free_guard_value_unchanged() {
    let ir = build(
        "fn abs(x: i32) -> i32 {\n  (x > 0) -> {\n    -true-> x;\n    -false-> x * -1;\n  } -> ret;\n}\nfn main() {\n  (0 - 7) -> abs -> println;\n}\n",
    );
    let rr = run(&ir, BUDGET);
    assert!(matches!(rr.outcome, Outcome::Done(_)), "{:?}", rr.outcome);
    assert_eq!(rr.output, "7\n");
}

// --- plan-s40: the arm owns the loop (builder-built IR — L1406 keeps these
// shapes out of surface Mapal; testgen builds them directly) -------------------

use mapal_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// A loop whose exit feeds one Phi arm exclusively. The body divides by the
/// merge (starts 0), so RUNNING the loop traps immediately.
fn loop_in_arm(cond: bool) -> mapal_ir::CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "la", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c = fb.constant(Value::Bool(cond), L).unwrap();
        let t = fb.constant(Value::I32(42), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let lc = fb
            .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
            .unwrap();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let next = fb
            .binop(Operation::Div, seven, merge, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, next, lc, L).unwrap();
        let ex = fb
            .loop_exit(&lh, merge, lc, Dest::Fresh(Some("ex".into())), L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.phi(t, ex, c, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

/// The untaken arm's LOOP does not run — its first iteration would trap.
#[test]
fn untaken_arm_loop_does_not_run() {
    let rr = run(&loop_in_arm(true), BUDGET);
    assert!(
        matches!(rr.outcome, Outcome::Done(_)),
        "untaken loop ran: {:?}",
        rr.outcome
    );
}

/// Control — the TAKEN arm's loop runs and traps.
#[test]
fn taken_arm_loop_still_runs_and_traps() {
    let rr = run(&loop_in_arm(false), BUDGET);
    assert_eq!(
        rr.outcome,
        Outcome::Trapped(mapal_interp::TrapKind::DivZero),
        "{:?}",
        rr.outcome
    );
}

/// The dual topology: an arm INSIDE the loop body. The false arm's `7 / 0` is
/// never selected, so ten iterations complete and the loop exits with 10.
#[test]
fn untaken_arm_inside_loop_body_does_not_fire() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "lb", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let lc = fb
            .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
            .unwrap();
        let hundred = fb.constant(Value::I32(100), L).unwrap();
        let bc = fb
            .binop(Operation::Lt, merge, hundred, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inc = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
            .unwrap();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let zc = fb.constant(Value::I32(0), L).unwrap();
        let bad = fb
            .binop(Operation::Div, seven, zc, Dest::Fresh(None), L)
            .unwrap();
        let next = fb.phi(inc, bad, bc, Dest::Fresh(None), L).unwrap();
        fb.loop_back(&lh, next, lc, L).unwrap();
        fb.loop_exit(&lh, merge, lc, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let rr = run(&ir, BUDGET);
    assert!(
        matches!(&rr.outcome, Outcome::Done(v) if format!("{v:?}").contains("I32(10)")),
        "in-body untaken arm fired: {:?}",
        rr.outcome
    );
}
