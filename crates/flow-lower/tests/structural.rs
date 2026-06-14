//! Structural assertions (DESIGN §14.3): the shape half of the semantic pins.
//! These read the lowered graph directly (objects, morphisms, edges) rather than
//! the Mermaid text, so they survive cosmetic dump changes.

mod common;
use common::{example, lower_ok};

use flow_ir::{CategoryIr, FuncKind, ObjectKind, Operation};

/// Count morphisms of a given op across the whole graph.
fn count_op(ir: &CategoryIr, pred: impl Fn(&Operation) -> bool) -> usize {
    ir.morphisms().filter(|(_, m)| pred(&m.op)).count()
}

/// The function with the given name.
fn func_named(ir: &CategoryIr, name: &str) -> flow_ir::FuncId {
    ir.funcs()
        .find(|(_, d)| d.name == name)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no fn `{name}`"))
}

#[test]
fn sum_to_n_loop_shape() {
    let ir = lower_ok(&example("sum_to_n"));
    // Exactly one LoopEnter / LoopBack / LoopExit (a single canonical loop).
    assert_eq!(count_op(&ir, |o| *o == Operation::LoopEnter), 1);
    assert_eq!(count_op(&ir, |o| *o == Operation::LoopBack), 1);
    assert_eq!(count_op(&ir, |o| *o == Operation::LoopExit), 1);

    // Both routes' slot-1 Pair source is one shared cond object (pin 5).
    let back = ir
        .morphisms()
        .find(|(_, m)| m.op == Operation::LoopBack)
        .unwrap()
        .1;
    let exit = ir
        .morphisms()
        .find(|(_, m)| m.op == Operation::LoopExit)
        .unwrap()
        .1;
    let cond_of = |route: flow_ir::ObjectId| -> flow_ir::ObjectId {
        // slot-1 Pair in-edge of the route object is the cond.
        ir.in_edges(route)
            .iter()
            .find_map(|&mid| {
                let m = ir.morphism(mid).unwrap();
                matches!(m.op, Operation::Pair { slot: 1, .. }).then_some(m.source)
            })
            .expect("route has a slot-1 cond")
    };
    let back_cond = cond_of(back.source);
    let exit_cond = cond_of(exit.source);
    assert_eq!(back_cond, exit_cond, "back and exit must share one cond");

    // The exit-route slot-0 source is a Proj of the merge (the merge-state view),
    // NOT the jump arm's recomputed `acc + i` (the 55-not-66 regression).
    let exit_payload = ir
        .in_edges(exit.source)
        .iter()
        .find_map(|&mid| {
            let m = ir.morphism(mid).unwrap();
            matches!(m.op, Operation::Pair { slot: 0, .. }).then_some(m.source)
        })
        .expect("exit route slot-0");
    // exit_payload is produced by a Proj whose source is the LoopMerge.
    let producer = ir
        .in_edges(exit_payload)
        .iter()
        .map(|&mid| ir.morphism(mid).unwrap())
        .next()
        .expect("exit payload has a producer");
    assert!(
        matches!(producer.op, Operation::Proj { .. }),
        "exit payload must be a Proj of the merge, found {:?}",
        producer.op
    );
    let merge = ir.object(producer.source).unwrap();
    assert_eq!(
        merge.kind,
        ObjectKind::LoopMerge,
        "exit payload Proj source must be the LoopMerge (merge-state view, not recomputed state)"
    );
}

#[test]
fn abs_phi_and_neg_literal() {
    let ir = lower_ok(&example("abs"));
    // Exactly one Phi.
    assert_eq!(count_op(&ir, |o| *o == Operation::Phi), 1);
    // No Neg morphism (the `-1` folds into a Constant; pin 3).
    assert_eq!(count_op(&ir, |o| *o == Operation::Neg), 0);
    // A `-1` Constant exists.
    let has_neg_one = ir.objects().any(|(_, o)| {
        o.kind == ObjectKind::Constant && matches!(&o.value, Some(flow_ir::Value::I32(-1)))
    });
    assert!(has_neg_one, "expected a folded -1 Constant");
}

#[test]
fn sepia_map_fold_kinds() {
    let ir = lower_ok(&example("sepia"));
    let map_bodies = ir
        .funcs()
        .filter(|(_, d)| d.kind == FuncKind::MapBody)
        .count();
    let fold_bodies = ir
        .funcs()
        .filter(|(_, d)| d.kind == FuncKind::FoldBody)
        .count();
    assert_eq!(map_bodies, 1, "one MapBody fn");
    assert_eq!(fold_bodies, 1, "one FoldBody fn");

    // The fold's source ty is `(f32, [Pixel; 16])`.
    let fold = ir
        .morphisms()
        .find(|(_, m)| matches!(m.op, Operation::Fold { .. }))
        .unwrap()
        .1;
    let src_ty = &ir.object(fold.source).unwrap().ty;
    let expected = flow_ir::Ty::Tuple(vec![
        flow_ir::Ty::f32(),
        flow_ir::Ty::Array {
            elem: Box::new(flow_ir::Ty::Struct {
                name: "Pixel".into(),
                fields: vec![
                    ("r".into(), flow_ir::Ty::f32()),
                    ("g".into(), flow_ir::Ty::f32()),
                    ("b".into(), flow_ir::Ty::f32()),
                ],
            }),
            size: 16,
        },
    ]);
    assert_eq!(*src_ty, expected, "fold source ty");
}

#[test]
fn fanout_main_print_token_order() {
    let ir = lower_ok(&example("fanout"));
    let main = func_named(&ir, "main");
    let def = ir.func(main).unwrap();
    // Collect the two Print morphisms in this fn.
    let prints: Vec<&flow_ir::Morphism> = def
        .morphisms
        .iter()
        .map(|&m| ir.morphism(m).unwrap())
        .filter(|m| matches!(m.op, Operation::Print { .. }))
        .collect();
    assert_eq!(prints.len(), 2, "main has two prints");
    // Print2's token (slot-0 of its source pair) is Print1's output token.
    let p1_out = prints[0].target; // first print's result token
    let p2_pair = prints[1].source;
    let p2_token = ir
        .in_edges(p2_pair)
        .iter()
        .find_map(|&mid| {
            let m = ir.morphism(mid).unwrap();
            matches!(m.op, Operation::Pair { slot: 0, .. }).then_some(m.source)
        })
        .expect("print2 token slot");
    assert_eq!(p2_token, p1_out, "print2 consumes print1's output token");
}

#[test]
fn countdown_token_bearing_loop() {
    let ir = lower_ok(COUNTDOWN);
    let cd = func_named(&ir, "countdown");
    // The LoopMerge of countdown carries a token (`U = (i32, IoToken)`).
    let merge = ir
        .objects()
        .find(|(id, o)| ir.owner(*id) == cd && o.kind == ObjectKind::LoopMerge)
        .map(|(_, o)| o)
        .expect("countdown has a loop merge");
    assert!(
        flow_ir::ty_contains_token(&merge.ty),
        "countdown's U must carry a token"
    );
    // The exit payload carries the token (token-in ⇒ token-out).
    let exit = ir
        .morphisms()
        .find(|(_, m)| m.op == Operation::LoopExit)
        .unwrap()
        .1;
    assert!(
        flow_ir::ty_contains_token(&ir.object(exit.target).unwrap().ty),
        "the loop exit payload must carry the token"
    );
}

#[test]
fn declared_signatures_match_table() {
    // pipeline `f : i32 -> i32` (pure), `main : io -> io` (effectful entry).
    let ir = lower_ok(&example("pipeline"));
    let f = ir.func(func_named(&ir, "f")).unwrap();
    assert_eq!(ir.object(f.input).unwrap().ty, flow_ir::Ty::i32());
    assert_eq!(ir.object(f.output).unwrap().ty, flow_ir::Ty::i32());
    let main = ir.func(func_named(&ir, "main")).unwrap();
    assert_eq!(ir.object(main.input).unwrap().ty, flow_ir::Ty::IoToken);
    assert_eq!(ir.object(main.output).unwrap().ty, flow_ir::Ty::IoToken);
}

#[test]
fn fn_body_tail_returns() {
    // `fn f(x: i32) -> i32 { x + 1 }` — the fn-body tail returns (LD21).
    let src = "fn f(x: i32) -> i32 { x + 1 }\nfn main() { 1 -> f -> r; r -> print; }\n";
    let ir = lower_ok(src);
    let f = ir.func(func_named(&ir, "f")).unwrap();
    // The Return object has a writer (an Add into Ret).
    let writers = ir.in_edges(f.output).len();
    assert!(writers >= 1, "fn-body tail must write Return");
}

const COUNTDOWN: &str = r#"fn countdown(mut n: i32) {
    loop {
        n -> print;
        (n > 0) -> {
            -true-> { n - 1 -> n; -> loop; }
            -false-> -> ret;
        }
    }
}

fn main() {
    5 -> countdown;
}
"#;

#[test]
fn atk_02_effectful_call_in_loop_carries_token() {
    // ATK-02: a loop body whose only effect is a call to an effectful fn (no
    // direct print) must carry the IoToken in its merge `U` — the effect must
    // NOT hoist out of the loop cycle. The exit arm is a bare value `i;`, which
    // in a token-carrying loop rebinds the token register to `proj(exit, 0)` so
    // the post-loop `9 -> print` still has a token (§8.5 / I4b).
    let src = r#"fn log(x: i32) { x -> print; }
fn main() {
    mut i: i32 <- 0;
    loop {
        i -> log;
        (i < 3) -> {
            -true-> { i + 1 -> i; -> loop; }
            -false-> i;
        }
    }
    9 -> print;
}
"#;
    let ir = lower_ok(src);
    // The loop merge `U` carries a token.
    let merge = ir
        .objects()
        .find(|(_, o)| o.kind == ObjectKind::LoopMerge)
        .map(|(_, o)| o)
        .expect("loop merge");
    assert!(
        flow_ir::ty_contains_token(&merge.ty),
        "ATK-02: the loop merge must carry the IoToken (merge ty = {:?})",
        merge.ty
    );
    // The effectful Call is inside the cycle (token-bearing).
    assert_eq!(
        count_op(&ir, |o| matches!(o, Operation::Call { .. })),
        1,
        "the effectful call is emitted"
    );
    // Two token route consumers: the carried token is read by both the LoopBack
    // next-state and the LoopExit payload (the back edge and the exit both thread
    // the token — I4's exactly-two-consumer fork).
    assert_eq!(count_op(&ir, |o| *o == Operation::LoopBack), 1);
    assert_eq!(count_op(&ir, |o| *o == Operation::LoopExit), 1);
    // validate clean (asserted in lower_ok) ⇒ no dangling token (I4b): the final
    // `9 -> print` consumes a live token rebound off the exit.
}
