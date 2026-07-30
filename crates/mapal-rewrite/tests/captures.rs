//! ADR-0027 micro-goldens — capture-aware rewrite behavior for `Map`/`Fold`
//! with `captures > 0`.
//!
//! Pins:
//! - fusion with **identical** capture objects fuses (and evaluates equal);
//! - fusion with **differing** capture sets (or counts) does NOT fuse;
//! - identity replay of a captured map re-threads the capture components into
//!   the rebuilt source product (never a silent k=0 rebuild);
//! - a captured map + captured fold survive the full `rewrite()` pipeline
//!   validate-clean and oracle-equal;
//! - DCE treats capture edges as ordinary dataflow (live capture cone kept, a
//!   dead pure temp dropped — no special-casing);
//! - CSE keys Map/Fold including their captures (identical captured maps
//!   merge; differing captures do not).

use mapal_interp::{Outcome, RValue, eval_call};
use mapal_ir::{
    CategoryIr, Dest, FuncId, FuncKind, IrBuilder, ObjectId, Operation, SourceLoc, Ty, Value,
    validate,
};
use mapal_rewrite::{RewritePlan, analyze_cse, analyze_dce, analyze_map_fusion, replay, rewrite};

const BUDGET: u64 = 100_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

fn i32_arr(size: u64) -> Ty {
    Ty::Array {
        elem: Box::new(Ty::i32()),
        size,
    }
}

/// An i32 array `RValue`.
fn arr(xs: &[i32]) -> RValue {
    RValue::array(xs.iter().map(|&x| RValue::Scalar(Value::I32(x))).collect())
}

/// An `(i32, [i32;3])` call argument.
fn cap_arr_arg(cap: i32, xs: &[i32]) -> RValue {
    RValue::Tuple(vec![RValue::Scalar(Value::I32(cap)), arr(xs)])
}

/// The capture counts of every `Map` edge in the graph, in morphism order.
fn map_capture_counts(ir: &CategoryIr) -> Vec<u32> {
    ir.morphisms()
        .filter_map(|(_, m)| match m.op {
            Operation::Map { captures, .. } => Some(captures),
            _ => None,
        })
        .collect()
}

/// Slot feeders of a product object, in slot order (Pair declaration order).
fn slot_feeders(ir: &CategoryIr, product: ObjectId) -> Vec<ObjectId> {
    let mut v: Vec<(u32, ObjectId)> = ir
        .in_edges(product)
        .iter()
        .filter_map(|&m| {
            let mo = ir.morphism(m).expect("in-edge");
            if let Operation::Pair { slot, .. } = mo.op {
                Some((slot, mo.source))
            } else {
                None
            }
        })
        .collect();
    v.sort_by_key(|(s, _)| *s);
    v.into_iter().map(|(_, s)| s).collect()
}

/// A `MapBody` `(i32 cap, i32 elem) -> cap + elem` — reads its capture.
fn body_cap_add(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(
            FuncKind::MapBody,
            name,
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let p = fb.input();
    let c = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
    let x = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
    fb.binop(Operation::Add, c, x, Dest::Ret { slot: None }, L)
        .unwrap();
    fb.finish().unwrap();
    f
}

/// A `MapBody` `(i32 cap, i32 elem) -> cap * elem` — reads its capture.
fn body_cap_mul(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(
            FuncKind::MapBody,
            name,
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let p = fb.input();
    let c = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
    let x = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
    fb.binop(Operation::Mul, c, x, Dest::Ret { slot: None }, L)
        .unwrap();
    fb.finish().unwrap();
    f
}

/// A plain `MapBody` `x -> x * 2` (k=0 shape).
fn body_double(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(FuncKind::MapBody, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let two = fb.constant(Value::I32(2), L).unwrap();
    fb.binop(Operation::Mul, x, two, Dest::Ret { slot: None }, L)
        .unwrap();
    fb.finish().unwrap();
    f
}

/// A `FoldBody` `(i32 cap, i32 acc, i32 elem) -> acc + elem + cap`.
fn body_cap_fold(b: &mut IrBuilder, name: &str) -> FuncId {
    let f = b
        .declare(
            FuncKind::FoldBody,
            name,
            Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let p = fb.input();
    let c = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
    let acc = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
    let x = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
    let s = fb
        .binop(Operation::Add, acc, x, Dest::Fresh(None), L)
        .unwrap();
    fb.binop(Operation::Add, s, c, Dest::Ret { slot: None }, L)
        .unwrap();
    fb.finish().unwrap();
    f
}

// --- fusion: identical captures FUSE ----------------------------------------

#[test]
fn map_fusion_identical_captures_fuses() {
    // main(cap, [i32;3]) { mid = map(f=cap+x, [cap], arr); map(g=cap*x, [cap], mid) }
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let g = body_cap_mul(&mut b, "g");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let mid = fb
            .map_captured(f, &[cap], a, Dest::Fresh(Some("mid".into())), L)
            .unwrap();
        fb.map_captured(g, &[cap], mid, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(map_capture_counts(&ir), vec![1, 1]);

    let plan = analyze_map_fusion(&ir);
    assert_eq!(plan.fuse.len(), 1, "identical capture objects must fuse");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(
        map_capture_counts(&out),
        vec![1],
        "one captured Map survives"
    );
    assert_eq!(
        out.funcs().count(),
        2,
        "main + fused body; f/g orphans dropped"
    );

    // (cap + x) * cap elementwise: cap=10, [1,2,3] → [110,120,130].
    let before = eval_call(&ir, main, cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[110, 120, 130])));
    assert_eq!(after, before);
}

// --- fusion: differing captures do NOT fuse ----------------------------------

#[test]
fn map_fusion_differing_captures_skip() {
    // main(cap1, cap2, [i32;3]) { mid = map(f, [cap1], arr); map(g, [cap2], mid) }
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let g = body_cap_mul(&mut b, "g");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap1 = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let cap2 = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        let mid = fb
            .map_captured(f, &[cap1], a, Dest::Fresh(Some("mid".into())), L)
            .unwrap();
        fb.map_captured(g, &[cap2], mid, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert!(
        plan.fuse.is_empty() && plan.drop.is_empty(),
        "differing capture objects must not fuse (recorded headroom)"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_capture_counts(&out), vec![1, 1], "both maps kept");

    // cap1=10, cap2=100, [1,2,3] → [1100,1200,1300].
    let arg = RValue::Tuple(vec![
        RValue::Scalar(Value::I32(10)),
        RValue::Scalar(Value::I32(100)),
        arr(&[1, 2, 3]),
    ]);
    let before = eval_call(&ir, main, arg.clone(), BUDGET);
    let after = eval_call(&out, out.entry(), arg, BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[1100, 1200, 1300])));
    assert_eq!(after, before);
}

#[test]
fn map_fusion_capture_count_mismatch_skips() {
    // f has one capture; g is a capture-free map — capture sets differ.
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let g = body_double(&mut b, "g");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let mid = fb
            .map_captured(f, &[cap], a, Dest::Fresh(Some("mid".into())), L)
            .unwrap();
        fb.map(g, mid, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_map_fusion(&ir);
    assert!(
        plan.fuse.is_empty() && plan.drop.is_empty(),
        "k=1 ∘ k=0 capture sets differ — no fusion"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_capture_counts(&out), vec![1, 0]);

    let before = eval_call(&ir, main, cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[22, 24, 26])));
    assert_eq!(after, before);
}

// --- replay: captured map keeps its capture edges ----------------------------

#[test]
fn replay_captured_map_preserves_capture_edges() {
    // main(cap, [i32;3]) { map(f=cap+x, [cap], arr) } under the EMPTY plan:
    // the rebuilt Map must read a (cap′, arr′) product — never a bare array.
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.map_captured(f, &[cap], a, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let out = replay(&ir, &RewritePlan::new()).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));

    // Exactly one Map edge, still captures=1…
    let maps: Vec<_> = out
        .morphisms()
        .filter(|(_, m)| matches!(m.op, Operation::Map { captures: 1, .. }))
        .collect();
    assert_eq!(maps.len(), 1, "the captured Map survives replay");
    let map_edge = maps[0].1;

    // …reading a 2-component source product whose slot feeders are the
    // replayed capture (π₀ of the fn input) and the replayed array (π₁).
    let src_feeders = slot_feeders(&out, map_edge.source);
    assert_eq!(src_feeders.len(), 2, "source is the (cap, arr) product");
    let input = out.func(out.entry()).expect("main").input;
    for (feeder, want_idx) in src_feeders.iter().zip([0, 1]) {
        let ins = out.in_edges(*feeder);
        assert_eq!(ins.len(), 1, "feeder defined by one edge");
        let def = out.morphism(ins[0]).expect("feeder def");
        assert!(
            def.op == Operation::Proj { index: want_idx } && def.source == input,
            "slot {want_idx} feeder must be π{want_idx} of the fn input"
        );
    }

    let before = eval_call(&ir, main, cap_arr_arg(7, &[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), cap_arr_arg(7, &[1, 2, 3]), BUDGET);
    assert_eq!(before, Outcome::Done(arr(&[8, 9, 10])));
    assert_eq!(after, before);
}

// --- full pipeline: captured map + fold stay validate-clean ------------------

#[test]
fn rewrite_pipeline_captured_map_and_fold() {
    // main(cap_m, cap_f, seed, [i32;3]) {
    //   dead  = cap_m + cap_f                    // dead pure temp — DCE fires,
    //   scaled = map(cap_m + x, [cap_m], arr)    // forcing a real replay round
    //   fold(acc+e+cap_f, [cap_f], seed, scaled)
    // }
    let mut b = IrBuilder::new();
    let mf = body_cap_add(&mut b, "mf");
    let ff = body_cap_fold(&mut b, "ff");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::i32(), i32_arr(3)]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap_m = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let cap_f = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let seed = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 3, Dest::Fresh(None), L).unwrap();
        fb.binop(
            Operation::Add,
            cap_m,
            cap_f,
            Dest::Fresh(Some("dead".into())),
            L,
        )
        .unwrap();
        let scaled = fb
            .map_captured(mf, &[cap_m], a, Dest::Fresh(Some("scaled".into())), L)
            .unwrap();
        fb.fold_captured(ff, &[cap_f], seed, scaled, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let res = rewrite(ir);
    let out = res.ir;
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert!(
        !res.report.skipped_non_canonical && res.report.rounds >= 1,
        "the pipeline really ran"
    );
    assert_eq!(
        map_capture_counts(&out),
        vec![1],
        "the captured Map keeps its capture count"
    );
    let fold_caps: Vec<u32> = out
        .morphisms()
        .filter_map(|(_, m)| match m.op {
            Operation::Fold { captures, .. } => Some(captures),
            _ => None,
        })
        .collect();
    assert_eq!(
        fold_caps,
        vec![1],
        "the captured Fold keeps its capture count"
    );

    // cap_m=10 → scaled=[11,12,13]; fold with cap_f=100 from seed=1000:
    // 1000+11+100=1111 → 1111+12+100=1223 → 1223+13+100=1336.
    let arg = RValue::Tuple(vec![
        RValue::Scalar(Value::I32(10)),
        RValue::Scalar(Value::I32(100)),
        RValue::Scalar(Value::I32(1000)),
        arr(&[1, 2, 3]),
    ]);
    let after = eval_call(&out, out.entry(), arg, BUDGET);
    assert_eq!(after, Outcome::Done(RValue::Scalar(Value::I32(1336))));
}

// --- DCE: capture edges are ordinary dataflow --------------------------------

#[test]
fn dce_keeps_live_capture_drops_dead_temp() {
    // main(cap, [i32;3]) {
    //   cap_live = cap + 1    // feeds the map's source product
    //   cap_dead = cap * 2    // feeds nothing
    //   map(f=cap_live + x, [cap_live], arr)
    // }
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            i32_arr(3),
            L,
        )
        .unwrap();
    let cap_live;
    let cap_dead;
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        cap_live = fb
            .binop(Operation::Add, cap, one, Dest::Fresh(None), L)
            .unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        cap_dead = fb
            .binop(Operation::Mul, cap, two, Dest::Fresh(None), L)
            .unwrap();
        fb.map_captured(f, &[cap_live], a, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_dce(&ir);
    assert!(
        !plan.drop.contains_key(cap_live),
        "a used capture is kept — the Pair edge into the source product is ordinary dataflow"
    );
    assert!(
        plan.drop.contains_key(cap_dead),
        "a dead pure temp is dropped, capture-shaped or not"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    let before = eval_call(&ir, main, cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    let after = eval_call(&out, out.entry(), cap_arr_arg(10, &[1, 2, 3]), BUDGET);
    // cap_live = 11 → [12,13,14].
    assert_eq!(before, Outcome::Done(arr(&[12, 13, 14])));
    assert_eq!(after, before);
}

// --- CSE: Map/Fold keys include captures -------------------------------------

#[test]
fn cse_merges_identical_captured_maps() {
    // Two maps with the same body, same capture object, same array: one survives.
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), i32_arr(3)]),
            Ty::Tuple(vec![i32_arr(3), i32_arr(3)]),
            L,
        )
        .unwrap();
    let m2;
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.map_captured(f, &[cap], a, Dest::Ret { slot: Some(0) }, L)
            .unwrap();
        m2 = fb
            .map_captured(f, &[cap], a, Dest::Fresh(Some("dup".into())), L)
            .unwrap();
        fb.output(m2, Some(1), L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_cse(&ir);
    assert!(
        plan.alias.contains_key(m2),
        "identical captured maps share a key — the duplicate is aliased"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(map_capture_counts(&out), vec![1], "one Map survives CSE");
}

#[test]
fn cse_distinguishes_differing_captures() {
    // Same body, same array, different capture objects: no merge.
    let mut b = IrBuilder::new();
    let f = body_cap_add(&mut b, "f");
    let main = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![Ty::i32(), Ty::i32(), i32_arr(3)]),
            Ty::Tuple(vec![i32_arr(3), i32_arr(3)]),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let p = fb.input();
        let cap1 = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let cap2 = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        fb.map_captured(f, &[cap1], a, Dest::Ret { slot: Some(0) }, L)
            .unwrap();
        fb.map_captured(f, &[cap2], a, Dest::Ret { slot: Some(1) }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_cse(&ir);
    assert!(
        plan.alias.is_empty(),
        "maps differing only in their capture objects must not share a key"
    );

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(map_capture_counts(&out), vec![1, 1]);
}
