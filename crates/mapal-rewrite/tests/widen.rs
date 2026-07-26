//! ADR-0029 widening replay and oracle-exact constant folding.

use mapal_interp::{Outcome, RValue, eval_call};
use mapal_ir::{
    CategoryIr, Dest, FuncKind, IrBuilder, ObjectId, Operation, SourceLoc, Ty, Value, validate,
};
use mapal_rewrite::{RewritePlan, analyze_const_fold, replay, rewrite};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn cases() -> Vec<(Value, Ty, Value)> {
    vec![
        (Value::I32(i32::MIN), Ty::i64(), Value::I64(i32::MIN as i64)),
        (
            Value::I32(16_777_217),
            Ty::f32(),
            Value::F32(16_777_217_i32 as f32),
        ),
        (
            Value::I32(-2_000_000_001),
            Ty::f64(),
            Value::F64(-2_000_000_001_i32 as f64),
        ),
        (Value::F32(0.1), Ty::f64(), Value::F64(0.1_f32 as f64)),
    ]
}

fn build(input: Value, target: Ty) -> (CategoryIr, ObjectId) {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, target.clone(), L)
        .unwrap();
    let widened;
    {
        let mut fb = b.build_fn(f).unwrap();
        let c = fb.constant(input, L).unwrap();
        widened = fb.widen(c, target, Dest::Fresh(None), L).unwrap();
        fb.output(widened, None, L).unwrap();
        fb.finish().unwrap();
    }
    (b.seal(f).unwrap(), widened)
}

fn value(ir: &CategoryIr) -> Outcome {
    eval_call(ir, ir.entry(), RValue::Unit, 10)
}

#[test]
fn widen_replay_round_trips() {
    for (input, target, expected) in cases() {
        let (ir, _) = build(input, target);
        let out = replay(&ir, &RewritePlan::new()).unwrap();
        assert!(validate(&out).is_empty());
        assert_eq!(value(&out), Outcome::Done(RValue::Scalar(expected)));
        assert_eq!(
            out.morphisms()
                .filter(|(_, m)| m.op == Operation::Widen)
                .count(),
            1
        );
    }
}

#[test]
fn widen_constants_fold_exactly() {
    for (input, target, expected) in cases() {
        let (ir, widened) = build(input, target);
        assert_eq!(analyze_const_fold(&ir).constify[widened], expected);
        let out = rewrite(ir).ir;
        assert!(validate(&out).is_empty());
        assert_eq!(value(&out), Outcome::Done(RValue::Scalar(expected)));
        assert!(out.morphisms().all(|(_, m)| m.op != Operation::Widen));
    }
}
