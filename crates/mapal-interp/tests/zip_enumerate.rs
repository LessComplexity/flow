//! WP3 value contracts for `Zip` / `Enumerate` (ADR-0018, plan-zip-enumerate §WP3).
//!
//! The interpreter is the oracle: these assertions are the normative denotation
//! every future backend must reproduce —
//!   `⟦zip⟧((a,b))       = [(a[0],b[0]), …, (a[n-1],b[n-1])]`
//!   `⟦enumerate⟧(a)     = [(0 as i32, a[0]), …, (n-1 as i32, a[n-1])]`
//!
//! IR is built directly with the `mapal-ir` builder (not parse→lower): lower's
//! `zip`/`enumerate` builtin routing is WP2, landing concurrently — building the
//! graph here keeps WP3 independent of that crate's in-flight state.

use mapal_interp::{Outcome, RValue, eval_call};
use mapal_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};

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

/// `(a[i], b[i])` — the value shape a `Zip` element takes.
fn pair(a: i32, b: i32) -> RValue {
    RValue::Tuple(vec![scal(a), scal(b)])
}

/// Zip'd elementwise add: `(a, b) -> zip -> map { p -> p.0 + p.1 }`.
/// The `zip_demo` contract — `a = [0..15]`, `b = [100;16]` ⇒ `c[k] = k + 100`.
#[test]
fn zip_add_value_contract() {
    let mut b = IrBuilder::new();
    // Map body: (i32, i32) -> i32, adds the two components.
    let body = b
        .declare(
            FuncKind::MapBody,
            "add_pair",
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, a, bb, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    // Entry: ([i32;16], [i32;16]) -> [i32;16].
    let in_ty = Ty::Tuple(vec![i32_arr(16), i32_arr(16)]);
    let f = b
        .declare(FuncKind::Named, "vadd", in_ty, i32_arr(16), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        let a = fb.proj(inp, 0, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(inp, 1, Dest::Fresh(None), L).unwrap();
        let z = fb.zip(a, bb, Dest::Fresh(None), L).unwrap();
        fb.map(body, z, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let a = RValue::Array((0..16).map(scal).collect());
    let bvec = RValue::Array((0..16).map(|_| scal(100)).collect());
    let arg = RValue::Tuple(vec![a, bvec]);
    let out = eval_call(&ir, f, arg, BUDGET);
    let c = match out {
        Outcome::Done(RValue::Array(es)) => es,
        other => panic!("expected Done(Array), got {other:?}"),
    };
    assert_eq!(c[0], scal(100), "c[0]");
    assert_eq!(c[15], scal(115), "c[15]");
    // Full pointwise: c[k] = k + 100.
    for (k, v) in c.iter().enumerate() {
        assert_eq!(*v, scal(k as i32 + 100), "c[{k}]");
    }
}

/// `enumerate` pins the index component `i32`, `0..n-1`, paired with each element.
#[test]
fn enumerate_indices_contract() {
    let mut b = IrBuilder::new();
    let out_ty = Ty::Array {
        elem: Box::new(Ty::Tuple(vec![Ty::i32(), Ty::i32()])),
        size: 4,
    };
    let f = b
        .declare(FuncKind::Named, "enum4", i32_arr(4), out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        fb.enumerate(a, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let arg = RValue::Array(vec![scal(10), scal(20), scal(30), scal(40)]);
    let out = eval_call(&ir, f, arg, BUDGET);
    assert_eq!(
        out,
        Outcome::Done(RValue::Array(vec![
            pair(0, 10),
            pair(1, 20),
            pair(2, 30),
            pair(3, 40),
        ]))
    );
}

/// Pure-stage fanout: an `enumerate` result feeds two parallel `Index` consumers.
/// Proves the pair-producing op is legal under fanout and evaluates once/correctly.
#[test]
fn enumerate_under_fanout() {
    let mut b = IrBuilder::new();
    let pair_ty = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let out_ty = Ty::Tuple(vec![pair_ty.clone(), pair_ty]);
    let f = b
        .declare(FuncKind::Named, "fan", i32_arr(4), out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let e = fb.enumerate(a, Dest::Fresh(None), L).unwrap();
        // e fans out to two independent Index consumers.
        let i0 = fb.constant(Value::I32(0), L).unwrap();
        let i3 = fb.constant(Value::I32(3), L).unwrap();
        let x0 = fb.index(e, i0, Dest::Fresh(None), L).unwrap();
        let x3 = fb.index(e, i3, Dest::Fresh(None), L).unwrap();
        fb.pack(&[x0, x3], Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let arg = RValue::Array(vec![scal(10), scal(20), scal(30), scal(40)]);
    let out = eval_call(&ir, f, arg, BUDGET);
    assert_eq!(
        out,
        Outcome::Done(RValue::Tuple(vec![pair(0, 10), pair(3, 40)]))
    );
}
