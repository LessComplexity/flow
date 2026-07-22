//! WP2 oracle contract for the four ADR-0029 widening lattice edges.

use flow_interp::{Outcome, RValue, eval_call};
use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc, Ty, Value, validate};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn check(input: Value, target: Ty, expected: Value) {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "widen", input.ty(), target.clone(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let src = fb.input();
        let widened = fb.widen(src, target, Dest::Fresh(None), L).unwrap();
        fb.output(widened, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    assert_eq!(
        eval_call(&ir, f, RValue::Scalar(input), 10),
        Outcome::Done(RValue::Scalar(expected))
    );
}

#[test]
fn widen_value_contract() {
    check(Value::I32(i32::MIN), Ty::i64(), Value::I64(i32::MIN as i64));
    check(
        Value::I32(16_777_217),
        Ty::f32(),
        Value::F32(16_777_217_i32 as f32),
    );
    check(
        Value::I32(-2_000_000_001),
        Ty::f64(),
        Value::F64(-2_000_000_001_i32 as f64),
    );
    check(Value::F32(0.1), Ty::f64(), Value::F64(0.1_f32 as f64));
}
