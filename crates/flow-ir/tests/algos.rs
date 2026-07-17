//! SCC / topo / loop-structure / deep-graph tests (DESIGN §16 item 4).

use flow_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// Build the canonical single `mut i` counting loop: `i` from 0, guard `i < 10`,
/// body `i + 1`, exit value = the merge-view `i`. Returns the sealed graph and
/// its entry FuncId.
fn counting_loop() -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "count", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        // guard: i < 10.
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let cond = fb
            .binop(
                Operation::Lt,
                merge,
                ten,
                Dest::Fresh(Some("cond".into())),
                L,
            )
            .unwrap();
        // body: i' = i + 1.
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(
                Operation::Add,
                merge,
                one,
                Dest::Fresh(Some("inext".into())),
                L,
            )
            .unwrap();
        // back edge fires on cond==true.
        fb.loop_back(&lh, inext, cond, L).unwrap();
        // exit reads the merge-view i (not inext); fires on cond==false.
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    (ir, f)
}

#[test]
fn counting_loop_validates_clean() {
    let (ir, _f) = counting_loop();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
}

#[test]
fn loop_has_exactly_one_nontrivial_scc_with_the_merge() {
    let (ir, f) = counting_loop();
    let sccs = ir.sccs(f);
    let nontrivial: Vec<&Vec<_>> = sccs
        .iter()
        .filter(|c| {
            c.len() > 1
                || c.first()
                    .map(|&o| {
                        ir.out_edges(o)
                            .iter()
                            .any(|&m| ir.morphism(m).unwrap().target == o)
                    })
                    .unwrap_or(false)
        })
        .collect();
    assert_eq!(nontrivial.len(), 1, "exactly one loop region");
    // The merge object is in that SCC.
    let merges_in: usize = nontrivial[0]
        .iter()
        .filter(|&&o| ir.object(o).unwrap().kind == flow_ir::ObjectKind::LoopMerge)
        .count();
    assert_eq!(merges_in, 1);
}

#[test]
fn loopback_edges_are_exactly_the_cycle_breakers() {
    // Removing LoopBack edges must make the function acyclic — assert by
    // checking topo_order emits every non-LoopBack morphism (Kahn terminates).
    let (ir, f) = counting_loop();
    let order = ir.topo_order(f);
    let total = ir.func(f).unwrap().morphisms.len();
    // Every morphism appears exactly once in the order (LoopBack included —
    // emitted but not gating).
    assert_eq!(order.len(), total, "topo emits every morphism once");
    // The LoopBack morphism is present in the order.
    let has_loopback = order
        .iter()
        .any(|&m| ir.morphism(m).unwrap().op == Operation::LoopBack);
    assert!(has_loopback, "LoopBack is emitted in the order");
}

#[test]
fn topo_places_header_first_body_before_exit_before_consumer() {
    // A counting loop whose exit value is post-processed (`exit + 1 -> ret`) so
    // there IS a consumer of the exit object downstream of `LoopExit` — the §16
    // item-4 "body before LoopExit before consumers" obligation needs one.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "count1", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let cond = fb
            .binop(
                Operation::Lt,
                merge,
                ten,
                Dest::Fresh(Some("cond".into())),
                L,
            )
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(
                Operation::Add,
                merge,
                one,
                Dest::Fresh(Some("inext".into())),
                L,
            )
            .unwrap();
        fb.loop_back(&lh, inext, cond, L).unwrap();
        // exit value into a FRESH object, then post-process it into ret.
        let exit_v = fb
            .loop_exit(&lh, merge, cond, Dest::Fresh(Some("exit_v".into())), L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        let one2 = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, exit_v, one2, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let order = ir.topo_order(f);

    let pos_of = |pred: &dyn Fn(&flow_ir::Morphism) -> bool| -> Option<usize> {
        order.iter().position(|&m| pred(ir.morphism(m).unwrap()))
    };

    // Header-first: LoopEnter is emitted before LoopExit.
    let enter_pos = pos_of(&|m| m.op == Operation::LoopEnter).expect("has enter");
    let exit_pos = pos_of(&|m| m.op == Operation::LoopExit).expect("has exit");
    assert!(
        enter_pos < exit_pos,
        "header-first: LoopEnter before LoopExit"
    );

    // The exit object (LoopExit's target) is post-processed: every edge whose
    // SOURCE is that exit object (its consumers) is ordered AFTER the LoopExit.
    let exit_obj = ir.morphism(order[exit_pos]).unwrap().target;
    for (i, &m) in order.iter().enumerate() {
        if ir.morphism(m).unwrap().source == exit_obj {
            assert!(
                i > exit_pos,
                "consumer of the exit object must follow LoopExit"
            );
        }
    }
    // And there is at least one such consumer (the `exit_v + 1` Pair edge),
    // so the assertion above is not vacuous.
    assert!(
        order
            .iter()
            .any(|&m| ir.morphism(m).unwrap().source == exit_obj),
        "the exit object has a downstream consumer"
    );

    // Body-before-exit: the guard `Lt` and body `Add` (both inside the SCC) are
    // ordered before the LoopExit edge that gates on the same cond.
    let lt_pos = pos_of(&|m| m.op == Operation::Lt).expect("has guard");
    let add_pos = pos_of(&|m| m.op == Operation::Add).expect("has body add");
    assert!(lt_pos < exit_pos, "guard before LoopExit");
    assert!(add_pos < exit_pos, "body before LoopExit");
}

#[test]
fn loop_free_graph_has_all_trivial_sccs() {
    // A simple chain: x -> +1 -> ret, no loops.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    for comp in ir.sccs(f) {
        let self_loop = comp.len() == 1
            && ir
                .out_edges(comp[0])
                .iter()
                .any(|&m| ir.morphism(m).unwrap().target == comp[0]);
        assert!(comp.len() == 1 && !self_loop, "all trivial");
    }
    assert!(ir.loop_structure(f).is_empty());
}

#[test]
fn loop_structure_single_loop_is_accept_shape() {
    // single-loop (tuple-carried) reports accept-shape: len==1, one merge.
    let (ir, f) = counting_loop();
    let ls = ir.loop_structure(f);
    assert_eq!(ls.len(), 1);
    assert_eq!(ls[0].merges.len(), 1);
}

#[test]
fn deep_chain_100k_no_stack_overflow() {
    // 100k-object add chain: sccs + topo_order complete without overflow (J1).
    let n = 100_000usize;
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let mut acc = fb.constant(Value::I32(0), L).unwrap();
        for i in 0..n {
            let dest = if i + 1 == n {
                Dest::Ret { slot: None }
            } else {
                Dest::Fresh(None)
            };
            acc = fb.binop(Operation::Add, acc, one, dest, L).unwrap();
        }
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let sccs = ir.sccs(f);
    // No non-trivial SCC in a chain.
    assert!(sccs.iter().all(|c| c.len() == 1));
    let order = ir.topo_order(f);
    assert_eq!(order.len(), ir.func(f).unwrap().morphisms.len());
}

#[test]
fn nested_loops_multi_merge_one_scc_reject_shape() {
    // Two merges fused into ONE SCC (DESIGN §7 "one merged SCC with two merges —
    // valid per the invariants"): the Verilog-reject shape (HANDOFF §4.3 accepts
    // single-loop FSM only). We build two loops whose carried states cross-feed:
    // the inner's next state reads the outer merge, the outer's next state reads
    // the inner merge. That makes om reach im and im reach om through body
    // objects, so Tarjan fuses both merges into one non-trivial SCC, while each
    // LoopEnter is still seeded from an external constant (I5: enter source
    // outside the SCC, both back sources inside it).
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "nested", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        // Each loop seeded from its own external constant (enter source ∉ SCC).
        let c0 = fb.constant(Value::I32(0), L).unwrap();
        let c1 = fb.constant(Value::I32(0), L).unwrap();
        let outer = fb.begin_loop(c0, L).unwrap();
        let om = fb.merge_of(&outer);
        let inner = fb.begin_loop(c1, L).unwrap();
        let im = fb.merge_of(&inner);
        let ct = fb.constant(Value::Bool(true), L).unwrap();
        let cf = fb.constant(Value::Bool(false), L).unwrap();
        // Cross-feed: inner next reads the OUTER merge; outer next reads the
        // INNER merge. om → inext → im and im → onext → om close the joint cycle.
        let inext = fb
            .binop(Operation::Add, om, im, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        fb.loop_back(&inner, inext, ct, L).unwrap();
        let onext = fb
            .binop(Operation::Add, im, om, Dest::Fresh(Some("onext".into())), L)
            .unwrap();
        fb.loop_back(&outer, onext, ct, L).unwrap();
        // Each loop has its own exit leaving the SCC (inner → fresh; outer → ret).
        let _iout = fb
            .loop_exit(&inner, im, cf, Dest::Fresh(Some("iout".into())), L)
            .unwrap();
        fb.end_loop(inner).unwrap();
        fb.loop_exit(&outer, om, cf, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(outer).unwrap();
        fb.finish().unwrap();
    }
    let ir = b
        .seal(f)
        .expect("fused nested loops are legal-but-degenerate (§7)");

    // Exactly one non-trivial SCC, and it contains BOTH LoopMerge objects.
    let nontrivial: Vec<Vec<flow_ir::ObjectId>> =
        ir.sccs(f).into_iter().filter(|c| c.len() > 1).collect();
    assert_eq!(nontrivial.len(), 1, "the two loops fuse into one SCC");
    let merges_in_scc = nontrivial[0]
        .iter()
        .filter(|&&o| ir.object(o).unwrap().kind == flow_ir::ObjectKind::LoopMerge)
        .count();
    assert_eq!(merges_in_scc, 2, "both merges land in the one SCC");

    // topo_order must not panic on the multi-merge SCC, and emits every morphism.
    let order = ir.topo_order(f);
    assert_eq!(order.len(), ir.func(f).unwrap().morphisms.len());

    // loop_structure reports it as the multi-merge reject shape: one LoopScc with
    // two merges (Verilog capability gate would reject — not a single-loop FSM).
    let ls = ir.loop_structure(f);
    assert_eq!(ls.len(), 1, "one loop region");
    assert_eq!(ls[0].merges.len(), 2, "reject shape: two merges in one SCC");

    // And the whole thing is well-formed by the independent oracle.
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
}

/// S12 regression: a multi-hop loop-invariant computation (`x * 2` — pair +
/// Mul, neither initially inserted before the loop) must be topo-ordered
/// BEFORE the LoopEnter edge. Previously FIFO-Kahn released LoopEnter as soon
/// as the init completed, ordering derived invariants after the header; the
/// interp driver (and any straight-line backend) then read them before write.
/// The rule: LoopEnter edges are deferred until no other morphism is ready.
#[test]
fn topo_orders_multi_hop_invariants_before_loop_enter() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        // Loop first (insertion order!), invariant chain emitted inside the body
        // — the shape lower produces for `loop { ... acc + x*2 ... }`.
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let three = fb.constant(Value::I32(3), L).unwrap();
        let cond = fb
            .binop(Operation::Lt, merge, three, Dest::Fresh(None), L)
            .unwrap();
        // The 2-hop invariant: t = x * 2 (source objects complete at start, but
        // the Mul becomes ready only after its internal pair fires).
        let two = fb.constant(Value::I32(2), L).unwrap();
        let t = fb
            .binop(Operation::Mul, x, two, Dest::Fresh(Some("t".into())), L)
            .unwrap();
        let next = fb
            .binop(Operation::Add, merge, t, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());

    let order = ir.topo_order(f);
    let pos = |mid: flow_ir::MorphismId| order.iter().position(|&m| m == mid).unwrap();

    // Locate the LoopEnter edge and the Mul edge (the invariant's definer).
    let mut enter = None;
    let mut mul = None;
    for (mid, morph) in ir.morphisms() {
        match morph.op {
            Operation::LoopEnter => enter = Some(mid),
            Operation::Mul => mul = Some(mid),
            _ => {}
        }
    }
    let (enter, mul) = (enter.unwrap(), mul.unwrap());

    // The invariant definer AND its feeding Pair edges precede the header.
    assert!(
        pos(mul) < pos(enter),
        "multi-hop invariant must precede LoopEnter (S12)"
    );
    for (mid, morph) in ir.morphisms() {
        if matches!(morph.op, Operation::Pair { .. })
            && morph.target == ir.morphism(mul).unwrap().source
        {
            assert!(pos(mid) < pos(enter), "invariant pair edge precedes header");
        }
    }
}
