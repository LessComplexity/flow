//! WP1 value contracts for `Iota` / `Fill` (ADR-0029, stage 1).
//!
//! The interpreter is the oracle: these assertions are the normative denotation
//! every backend must reproduce —
//!   `⟦iota⟧(n)     = [0, 1, …, n-1] as [i32; n]`
//!   `⟦fill⟧(x, n)  = [x, x, …, x]  as [T; n]`
//!
//! IR is built directly with the `mapal-ir` builder (no surface syntax yet —
//! the lower/syntax stages land separately), keeping the oracle contract
//! independent of those crates.

use mapal_interp::{Outcome, RValue, eval_call};
use mapal_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };
const BUDGET: u64 = 100_000;

fn scal(n: i32) -> RValue {
    RValue::Scalar(Value::I32(n))
}

/// `iota(5)` — the index sequence, element type pinned `i32`.
#[test]
fn iota_value_contract() {
    let mut b = IrBuilder::new();
    let out_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 5,
    };
    let f = b
        .declare(FuncKind::Named, "iota5", Ty::Unit, out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let n = fb.constant(Value::I32(5), L).unwrap();
        fb.iota(n, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let out = eval_call(&ir, f, RValue::Unit, BUDGET);
    assert_eq!(
        out,
        Outcome::Done(RValue::Array(vec![
            scal(0),
            scal(1),
            scal(2),
            scal(3),
            scal(4),
        ]))
    );
}

/// `fill(2.5, 4)` — every element the source value; count from the internal pair.
#[test]
fn fill_value_contract() {
    let mut b = IrBuilder::new();
    let out_ty = Ty::Array {
        elem: Box::new(Ty::f64()),
        size: 4,
    };
    let f = b
        .declare(FuncKind::Named, "fill4", Ty::Unit, out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.constant(Value::F64(2.5), L).unwrap();
        let n = fb.constant(Value::I32(4), L).unwrap();
        fb.fill(x, n, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let out = eval_call(&ir, f, RValue::Unit, BUDGET);
    assert_eq!(
        out,
        Outcome::Done(RValue::Array(vec![
            RValue::Scalar(Value::F64(2.5)),
            RValue::Scalar(Value::F64(2.5)),
            RValue::Scalar(Value::F64(2.5)),
            RValue::Scalar(Value::F64(2.5)),
        ]))
    );
}

/// `iota` feeds a `map` — the procedural-array composition ADR-0029 exists for
/// (the benchmark generators' shape): `iota(8) -> map { t -> t * 7 + 13 }`.
#[test]
fn iota_feeds_map_contract() {
    let mut b = IrBuilder::new();
    let body = b
        .declare(FuncKind::MapBody, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let t = fb.input();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let thirteen = fb.constant(Value::I32(13), L).unwrap();
        let m = fb
            .binop(Operation::Mul, t, seven, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, m, thirteen, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let out_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 8,
    };
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let n = fb.constant(Value::I32(8), L).unwrap();
        let tr = fb.iota(n, Dest::Fresh(None), L).unwrap();
        fb.map(body, tr, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));

    let out = eval_call(&ir, f, RValue::Unit, BUDGET);
    let c = match out {
        Outcome::Done(RValue::Array(es)) => es,
        other => panic!("expected Done(Array), got {other:?}"),
    };
    for (t, v) in c.iter().enumerate() {
        assert_eq!(*v, scal(t as i32 * 7 + 13), "c[{t}]");
    }
}

// --- The full parse → lower → interp path (ADR-0029 surface forms) -----------

fn run_src(src: &str) -> mapal_interp::RunResult {
    let po = mapal_syntax::parse(src);
    assert!(
        po.diagnostics.is_empty(),
        "parse-clean: {:?}",
        po.diagnostics
    );
    let ir = mapal_lower::lower(src, &po.program).expect("lowers clean");
    mapal_interp::run(&ir, BUDGET)
}

/// `4 -> iota -> t; (1.5, 4) -> fill -> s;` end to end: parse (ADR-0031),
/// lower (constant minting + the ops), interp (the stage-1 oracle arms).
#[test]
fn iota_fill_pipeline_e2e() {
    let rr = run_src(
        "fn main() {\n    4 -> iota -> t;\n    (1.5, 4) -> fill -> s;\n    t[2] -> println;\n    s[3] -> println;\n}\n",
    );
    assert_eq!(rr.output, "2\n1.5\n");
}

/// An annotated fill: `(0.0, 4) -> fill -> s: [f32; 4]` — the literal-width
/// unification reaches the element through the produced array.
#[test]
fn fill_annotated_f32_pipeline() {
    let rr =
        run_src("fn main() {\n    (0.0, 4) -> fill -> s: [f32; 4];\n    s[1] -> println;\n}\n");
    assert_eq!(rr.output, "0\n");
}
