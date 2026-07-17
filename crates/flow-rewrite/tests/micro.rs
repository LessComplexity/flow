//! DESIGN §8.4 — per-rule micro-goldens for layer 1 (map fusion + `map(id)`).
//!
//! Each test builds a tiny program through the public `IrBuilder`, runs
//! `analyze_map_fusion` + `replay`, and asserts the before/after shape (via
//! `validate` + morphism/function counts) and the interp value contract (via
//! `eval_call`). Covered rows: fused body shape, orphan bodies dropped,
//! out-degree-2 intermediate NOT fused, loop-bearing body NOT fused (P3),
//! `map(id)` elimination, and `map(id)` targeting Return NOT eliminated (P1).

use flow_interp::{Outcome, RValue, eval_call};
use flow_ir::{
    CategoryIr, Dest, FuncId, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate,
};
use flow_rewrite::{analyze_map_fusion, replay};

const BUDGET: u64 = 100_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn arr_ty(size: u64) -> Ty {
    Ty::Array {
        elem: Box::new(Ty::i32()),
        size,
    }
}

/// An i32 array `RValue`.
fn arr(xs: &[i32]) -> RValue {
    RValue::Array(xs.iter().map(|&x| RValue::Scalar(Value::I32(x))).collect())
}

/// Count `Map` morphisms in a graph.
fn map_count(ir: &CategoryIr) -> usize {
    ir.morphisms()
        .filter(|(_, m)| matches!(m.op, Operation::Map { .. }))
        .count()
}

/// A `MapBody` `x -> x <op> k`, targeting Return directly.
fn binop_body(b: &mut IrBuilder, name: &str, op: Operation, k: i32) -> FuncId {
    let f = b
        .declare(FuncKind::MapBody, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let c = fb.constant(Value::I32(k), L).unwrap();
    fb.binop(op, x, c, Dest::Ret { slot: None }, L).unwrap();
    fb.finish().unwrap();
    f
}

/// An identity `MapBody` `x -> ret`.
fn id_body(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(FuncKind::MapBody, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    fb.output(x, None, L).unwrap();
    fb.finish().unwrap();
    f
}

/// A `MapBody` with a canonical bounded loop `x -> {while s>0 {s=s-1}; s}` (⇒ 0
/// for x ≥ 0). Not loop-free ⇒ fusion must decline (P3).
fn loop_body(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(FuncKind::MapBody, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let lh = fb.begin_loop(x, L).unwrap();
    let s = fb.merge_of(&lh);
    let zero = fb.constant(Value::I32(0), L).unwrap();
    let cond = fb
        .binop(Operation::Gt, s, zero, Dest::Fresh(None), L)
        .unwrap();
    let one = fb.constant(Value::I32(1), L).unwrap();
    let next = fb
        .binop(Operation::Sub, s, one, Dest::Fresh(None), L)
        .unwrap();
    fb.loop_back(&lh, next, cond, L).unwrap();
    fb.loop_exit(&lh, s, cond, Dest::Ret { slot: None }, L)
        .unwrap();
    fb.end_loop(lh).unwrap();
    fb.finish().unwrap();
    f
}

// --- fused body shape + orphan bodies dropped ------------------------------

#[test]
fn map_fusion_fuses_and_drops_orphans() {
    // main([i32;3]) -> [i32;3] { map(g=*2, map(f=+1, arr)) }
    let mut b = IrBuilder::new();
    let f = binop_body(&mut b, "f", Operation::Add, 1);
    let g = binop_body(&mut b, "g", Operation::Mul, 2);
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), arr_ty(3), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        let mid = fb.map(f, a, Dest::Fresh(Some("mid".into())), L).unwrap();
        fb.map(g, mid, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(map_count(&ir), 2, "input has two Map edges");
    assert_eq!(ir.funcs().count(), 3, "input has main + f + g");

    let plan = analyze_map_fusion(&ir);
    assert_eq!(plan.fuse.len(), 1, "one fusion");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    // Fused body shape: exactly one (fused) Map edge survives.
    assert_eq!(map_count(&out), 1, "fused to one Map edge");
    // Orphan bodies dropped: only main + the synthesized fused body remain.
    assert_eq!(ir.funcs().count(), 3);
    assert_eq!(out.funcs().count(), 2, "f + g orphans dropped");

    // Interp value contract: (e+1)*2 elementwise.
    let want = Outcome::Done(arr(&[4, 6, 8]));
    let before = eval_call(&ir, main, arr(&[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), arr(&[1, 2, 3]), BUDGET);
    assert_eq!(before, want);
    assert_eq!(after, before);
}

#[test]
fn map_fusion_through_temporary() {
    // main([i32;3]) -> i32 { out = map(g=*2, map(f=+1, arr)); out[0] }
    // The fused map's result is a Temporary (consumed by Index), exercising the
    // reconstruct_temp fusion path (vs. the Return-writer path above).
    let mut b = IrBuilder::new();
    let f = binop_body(&mut b, "f", Operation::Add, 1);
    let g = binop_body(&mut b, "g", Operation::Mul, 2);
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        let mid = fb.map(f, a, Dest::Fresh(Some("mid".into())), L).unwrap();
        let out = fb.map(g, mid, Dest::Fresh(Some("out".into())), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        fb.index(out, zero, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert_eq!(plan.fuse.len(), 1);

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_count(&out), 1, "fused to one Map edge");
    assert_eq!(out.funcs().count(), 2, "f + g orphans dropped");

    // (arr[0] + 1) * 2 = 4.
    let before = eval_call(&ir, main, arr(&[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), arr(&[1, 2, 3]), BUDGET);
    assert_eq!(before, Outcome::Done(RValue::Scalar(Value::I32(4))));
    assert_eq!(after, before);
}

// --- out-degree-2 intermediate NOT fused -----------------------------------

#[test]
fn map_fusion_skips_shared_intermediate() {
    // main([i32;3]) -> ([i32;3],[i32;3]) { mid = map(f,arr); (map(g,mid), map(h,mid)) }
    let mut b = IrBuilder::new();
    let f = binop_body(&mut b, "f", Operation::Add, 1);
    let g = binop_body(&mut b, "g", Operation::Mul, 2);
    let h = binop_body(&mut b, "h", Operation::Mul, 3);
    let out_ty = Ty::Tuple(vec![arr_ty(3), arr_ty(3)]);
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), out_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        let mid = fb.map(f, a, Dest::Fresh(Some("mid".into())), L).unwrap();
        fb.map(g, mid, Dest::Ret { slot: Some(0) }, L).unwrap();
        fb.map(h, mid, Dest::Ret { slot: Some(1) }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert!(
        plan.fuse.is_empty(),
        "out-degree-2 intermediate must not fuse"
    );

    // Replay is a no-op-shaped identity; all three Map edges survive.
    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_count(&out), 3);
}

// --- loop-bearing body NOT fused (P3) --------------------------------------

#[test]
fn map_fusion_skips_loop_bearing_body() {
    // main([i32;3]) -> [i32;3] { map(g=+1, map(loopf, arr)) } — loopf has a loop.
    let mut b = IrBuilder::new();
    let lf = loop_body(&mut b, "loopf");
    let g = binop_body(&mut b, "g", Operation::Add, 1);
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), arr_ty(3), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        let mid = fb.map(lf, a, Dest::Fresh(Some("mid".into())), L).unwrap();
        fb.map(g, mid, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert!(plan.fuse.is_empty(), "P3: loop-bearing body must not fuse");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_count(&out), 2, "both maps kept");
}

// --- map(id) elimination ---------------------------------------------------

#[test]
fn map_id_eliminated_when_targeting_temporary() {
    // main([i32;3]) -> [i32;3] { mid = map(id, arr); mid -> ret }
    let mut b = IrBuilder::new();
    let idf = id_body(&mut b, "idf");
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), arr_ty(3), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        let mid = fb.map(idf, a, Dest::Fresh(Some("mid".into())), L).unwrap();
        fb.output(mid, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert_eq!(plan.alias.len(), 1, "map(id) forwards its result to arr");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_count(&out), 0, "the identity Map is gone");
    assert_eq!(out.funcs().count(), 1, "the id body orphan dropped");

    // Interp value contract: identity on the array.
    let before = eval_call(&ir, main, arr(&[5, 6, 7]), BUDGET);
    let after = eval_call(&out, out.entry(), arr(&[5, 6, 7]), BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[5, 6, 7])));
    assert_eq!(after, before);
}

// --- map(id) targeting Return NOT eliminated (P1) --------------------------

#[test]
fn map_id_targeting_return_not_eliminated() {
    // main([i32;3]) -> [i32;3] { map(id, arr) -> ret }  (map(id) writes Return)
    let mut b = IrBuilder::new();
    let idf = id_body(&mut b, "idf");
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), arr_ty(3), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let a = fb.input();
        fb.map(idf, a, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert!(
        plan.alias.is_empty() && plan.fuse.is_empty(),
        "P1: a Return-writing map(id) must not be forwarded"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_count(&out), 1, "the identity Map survives");
    assert_eq!(out.funcs().count(), 2, "the id body is still referenced");

    let before = eval_call(&ir, main, arr(&[5, 6, 7]), BUDGET);
    let after = eval_call(&out, out.entry(), arr(&[5, 6, 7]), BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[5, 6, 7])));
    assert_eq!(after, before);
}
