//! Value contracts for ADR-0027 capture semantics (plan-capture-semantics).
//!
//! The interpreter is the oracle: these assertions are the normative denotation
//! every backend must reproduce —
//!   `⟦map⟧((c₁…cₖ, a)) = [body(c…, a[0]), …, body(c…, a[n-1])]`  (captures broadcast)
//!   `⟦fold⟧((c…, seed, a))` left-sequential, body `(c…, acc, e)` per step
//! Captures are read-at-position: the value as of the fanout site, exactly as if
//! passed explicitly (loop-carried captures read the current iteration's value).

use flow_interp::{Outcome, RValue, eval_call};
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

fn f_of(ir: &flow_ir::CategoryIr) -> flow_ir::FuncId {
    ir.entry()
}

/// `mul_body : (cap, x) -> cap * x` (MapBody with one i32 capture).
fn declare_mul_body(b: &mut IrBuilder) -> flow_ir::FuncId {
    let body = b
        .declare(
            FuncKind::MapBody,
            "mul",
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let pin = fb.input();
        let c = fb.proj(pin, 0, Dest::Fresh(None), L).unwrap();
        let x = fb.proj(pin, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Mul, c, x, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    body
}

#[test]
fn map_with_scalar_capture_broadcasts() {
    let mut b = IrBuilder::new();
    let body = declare_mul_body(&mut b);
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.map_captured(body, &[cap], a, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let arg = RValue::Tuple(vec![
        scal(3),
        RValue::Array(vec![scal(1), scal(2), scal(3)]),
    ]);
    let out = eval_call(&ir, f_of(&ir), arg, BUDGET);
    assert_eq!(
        out,
        Outcome::Done(RValue::Array(vec![scal(3), scal(6), scal(9)]))
    );
}

#[test]
fn fold_with_capture_accumulates() {
    // sum cap*x over [1,2,3] with cap=2 ⇒ 12. Body (cap, acc, x) -> acc + cap*x.
    let mut b = IrBuilder::new();
    let body_in = Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::i32()]);
    let body = b
        .declare(FuncKind::FoldBody, "macc", body_in, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let pin = fb.input();
        let c = fb.proj(pin, 0, Dest::Fresh(None), L).unwrap();
        let acc = fb.proj(pin, 1, Dest::Fresh(None), L).unwrap();
        let x = fb.proj(pin, 2, Dest::Fresh(None), L).unwrap();
        let p = fb
            .binop(Operation::Mul, c, x, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, acc, p, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let seed = fb.constant(Value::I32(0), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.fold_captured(body, &[cap], seed, a, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let arg = RValue::Tuple(vec![
        scal(2),
        RValue::Array(vec![scal(1), scal(2), scal(3)]),
    ]);
    let out = eval_call(&ir, f_of(&ir), arg, BUDGET);
    assert_eq!(out, Outcome::Done(scal(12)));
}

#[test]
fn captured_array_is_indexed_inside_the_body() {
    // Body (arr, x) -> arr[1] * x: the capture is a whole array read by Index.
    let mut b = IrBuilder::new();
    let body_in = Ty::Tuple(vec![i32_arr(3), Ty::i32()]);
    let body = b
        .declare(FuncKind::MapBody, "idxbody", body_in, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let pin = fb.input();
        let arr = fb.proj(pin, 0, Dest::Fresh(None), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let v = fb.index(arr, one, Dest::Fresh(None), L).unwrap();
        let x = fb.proj(pin, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Mul, v, x, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![i32_arr(3), i32_arr(2)]),
            i32_arr(2),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let xs = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.map_captured(body, &[cap], xs, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    let arg = RValue::Tuple(vec![
        RValue::Array(vec![scal(5), scal(7), scal(9)]),
        RValue::Array(vec![scal(2), scal(3)]),
    ]);
    let out = eval_call(&ir, f_of(&ir), arg, BUDGET);
    assert_eq!(out, Outcome::Done(RValue::Array(vec![scal(14), scal(21)])));
}

use flow_ir::Operation;

// --- ADR-0027 review major #3: capture-walk scope leaks ----------------------
//
// The capture walk's Guard and Loop arms walked arms/bodies with the *shared*
// local set, so a shadow inside one guard arm or a loop-local binding leaked
// outward — later reads of the enclosing name were never recorded as captures
// and lowering died with a misleading L1101. The walk now save/restores per
// arm and per loop body, matching the emitter's scope model.

fn run_src(src: &str) -> flow_interp::RunResult {
    let po = flow_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse-clean: {:?}",
        po.diagnostics
    );
    let ir = flow_lower::lower(src, &po.program).expect("lowers clean");
    flow_interp::run(&ir, BUDGET)
}

#[test]
fn guard_arm_shadow_does_not_leak_into_sibling_arm() {
    // The true arm shadows `c` arm-locally; the false arm must still read the
    // ENCLOSING `c` (a capture), not the leaked shadow.
    let src = r#"
fn main() {
    7 -> c;
    [1, 0, 2] -> map { x -> x > 1 -> { -true-> { 1 -> c: i32; c } -false-> c } } -> ys: [i32; 3];
    ys[0] -> println;
    ys[1] -> println;
    ys[2] -> println;
}
"#;
    let rr = run_src(src);
    // x=1: 1>1 false → 7; x=0: false → 7; x=2: true → arm-local 1.
    assert_eq!(rr.output, "7\n7\n1\n", "outcome {:?}", rr.outcome);
}

#[test]
fn loop_local_shadow_does_not_leak_out_of_body_loop() {
    // `99 -> c` is a loop-body-local shadow; after the loop `x + c` must read
    // the ENCLOSING `c` (a capture), not the leaked loop-local.
    let src = r#"
fn main() {
    10 -> c;
    [1, 2] -> map { x ->
        mut i: i32 <- 0;
        loop {
            (i < 1) -> {
                -true-> { 99 -> c; i + 1 -> i; -> loop; }
                -false-> 0 -> r;
            }
        }
        x + c
    } -> ys: [i32; 2];
    ys[0] -> println;
    ys[1] -> println;
}
"#;
    let rr = run_src(src);
    // c is the enclosing 10 in both body instances: 1+10, 2+10.
    assert_eq!(rr.output, "11\n12\n", "outcome {:?}", rr.outcome);
}

// --- ADR-0027 headline semantics: oracle pins through the full pipeline ------
//
// "Captures are read-at-position: the value as of the fanout site, exactly as
// if passed explicitly (loop-carried captures read the current iteration's
// value)." Each pin is value-sensitive to the alternative misread (a stale
// init-value read, or a post-Update read) — see the traces inline.

#[test]
fn capturing_map_and_fold_evaluate_through_the_pipeline() {
    // Surface-level counterparts of the IrBuilder contracts above (also the
    // interp-value half of flow-lower/tests/captures.rs's product pins —
    // flow-lower cannot dev-depend on flow-interp): a capturing map
    // broadcasts, a capturing fold accumulates.
    let src = r#"
fn main() {
    3 -> k;
    [1, 2, 3] -> map { x -> x * k } -> ys: [i32; 3];
    (0, ys) -> fold { acc, x -> acc + x * k } -> total;
    total -> println;
}
"#;
    let rr = run_src(src);
    // map: [1*3, 2*3, 3*3] = [3, 6, 9]; fold over ys: 0 + 3*3 + 6*3 + 9*3 = 54.
    assert_eq!(rr.output, "54\n", "outcome {:?}", rr.outcome);
}

#[test]
fn loop_carried_capture_advance_cone_reads_current_iteration() {
    // A map inside a loop body captures the CARRIED `acc` and feeds the next
    // state. Reading the current iteration's value: ys=[1+acc, 2+acc], then
    // acc += ys[0] ⇒ 0 → 1 → 3 → 7. A stale (init-value) capture read would
    // give ys=[1,2] every iteration ⇒ 0 → 1 → 2 → 3: the pin separates them.
    let src = r#"
fn main() {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        (i < 3) -> {
            -true-> {
                [1, 2] -> map { x -> x + acc } -> ys: [i32; 2];
                acc + ys[0] -> acc;
                i + 1 -> i;
                -> loop;
            }
            -false-> acc -> done;
        }
    }
    done -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(rr.output, "7\n", "outcome {:?}", rr.outcome);
}

#[test]
fn loop_carried_capture_decide_cone_reads_current_iteration() {
    // A captured map feeds the loop GUARD (the decide cone): ys=[10+i, 20+i]
    // each iteration; the loop runs while 20+i < 23, accumulating 10+i.
    // i=0: 20<23, acc=10; i=1: 21<23, acc=21; i=2: 22<23, acc=33; i=3: exit.
    // A stale capture read pins ys=[10,20] forever — the guard never turns.
    let src = r#"
fn main() {
    mut i: i32 <- 0;
    mut acc: i32 <- 0;
    loop {
        [10, 20] -> map { x -> x + i } -> ys: [i32; 2];
        (ys[1] < 23) -> {
            -true-> { acc + ys[0] -> acc; i + 1 -> i; -> loop; }
            -false-> acc -> done;
        }
    }
    done -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(rr.output, "33\n", "outcome {:?}", rr.outcome);
}

#[test]
fn capture_reads_the_value_at_the_fanout_site_not_a_later_update() {
    // The map site precedes the Update in the same iteration: the body must
    // read the PRE-Update array (the value as of the fanout site).
    // i=0: c=[10]  → ys=[11];  c[0] <- 110; seen=11.
    // i=1: c=[110] → ys=[111]; c[0] <- 210; seen=122.
    // A post-Update read gives 111+211=322; a stale-init read gives 11+11=22.
    let src = r#"
fn main() {
    mut i: i32 <- 0;
    mut c: [i32; 1] <- [10];
    mut seen: i32 <- 0;
    loop {
        (i < 2) -> {
            -true-> {
                [1] -> map { x -> x + c[0] } -> ys: [i32; 1];
                c[0] <- c[0] + 100;
                seen + ys[0] -> seen;
                i + 1 -> i;
                -> loop;
            }
            -false-> seen -> done;
        }
    }
    done -> println;
}
"#;
    let rr = run_src(src);
    assert_eq!(rr.output, "122\n", "outcome {:?}", rr.outcome);
}
