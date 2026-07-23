//! SCC / topo / loop-structure / deep-graph tests (DESIGN §16 item 4).

use flow_ir::{
    Dest, FuncKind, IrBuilder, MorphismId, Operation, PathPlan, SourceLoc, TaskKind, TileRead,
    TileSite, Ty, Value, WaitEntry, validate,
};
use proptest::prelude::*;

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

// ---------------------------------------------------------------------------
// last_use_plan (docs/components/ir/plans/plan-last-use.md §2, rules 1-6)
// ---------------------------------------------------------------------------

/// Find the one morphism of `op` in `ir` (test helper — the shapes below
/// have exactly one of each).
fn one_op(ir: &flow_ir::CategoryIr, op: Operation) -> flow_ir::MorphismId {
    let mut hits: Vec<flow_ir::MorphismId> = ir
        .morphisms()
        .filter(|(_, m)| m.op == op)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(hits.len(), 1, "expected exactly one {op:?}");
    hits.remove(0)
}

/// The matmul4-class shape as the smallest honest version: one array carried
/// through a canonical loop — `merge` is the carried array, the guard reads
/// it (`Index < n`), the advance cone writes it (`Update`), the exit
/// releases it. Returns the sealed graph and the entry id.
fn carried_array_loop() -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let arr4 = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "build", Ty::i32(), arr4, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let n = fb.input();
        let z0 = fb.constant(Value::I32(0), L).unwrap();
        let z = fb
            .pack_array(&[z0, z0, z0, z0], Dest::Fresh(Some("z".into())), L)
            .unwrap();
        let lh = fb.begin_loop(z, L).unwrap();
        let merge = fb.merge_of(&lh);
        // guard: merge[0] < n (a decide-cone read of the carried array).
        let e = fb.index(merge, z0, Dest::Fresh(None), L).unwrap();
        let cond = fb.binop(Operation::Lt, e, n, Dest::Fresh(None), L).unwrap();
        // advance: merge' = update(merge, 0, 1).
        let one = fb.constant(Value::I32(1), L).unwrap();
        let upd = fb.update(merge, z0, one, Dest::Fresh(None), L).unwrap();
        fb.loop_back(&lh, upd, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    (ir, f)
}

/// The same loop over a BORROWED init (the fn's array parameter feeds
/// `LoopEnter` directly).
fn borrowed_init_loop() -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let arr4 = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "build", arr4.clone(), arr4, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let lh = fb.begin_loop(a, L).unwrap();
        let merge = fb.merge_of(&lh);
        let z0 = fb.constant(Value::I32(0), L).unwrap();
        let e = fb.index(merge, z0, Dest::Fresh(None), L).unwrap();
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let cond = fb
            .binop(Operation::Lt, e, ten, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let upd = fb.update(merge, z0, one, Dest::Fresh(None), L).unwrap();
        fb.loop_back(&lh, upd, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    (ir, f)
}

#[test]
fn last_use_carried_pair_update_result_carried_source_dead() {
    let (ir, f) = carried_array_loop();
    let plan = ir.last_use_plan(f);
    let update_m = one_op(&ir, Operation::Update);
    let update = ir.morphism(update_m).unwrap().target;
    let merge = ir
        .objects()
        .find(|(_, o)| o.kind == flow_ir::ObjectKind::LoopMerge)
        .map(|(id, _)| id)
        .unwrap();
    let back_route = ir
        .morphism(one_op(&ir, Operation::LoopBack))
        .unwrap()
        .source;

    // Rule 3: the Update result and the route cross the LoopBack into merge.
    assert_eq!(plan.carried_by(update), Some(merge));
    assert_eq!(plan.carried_by(back_route), Some(merge));
    // …but the merge itself, the init literal, and the guard's Index are not
    // carried.
    assert_eq!(plan.carried_by(merge), None);
    // The guard cond rides slot 1 of the route — consumed, never carried.
    let cond = ir.morphism(one_op(&ir, Operation::Lt)).unwrap().target;
    assert_eq!(plan.carried_by(cond), None);

    // Rule 2: the merge does NOT escape through its own loop's exit (the
    // per-iteration release valve; the exit object itself is what escapes).
    assert!(!plan.escapes(merge));
    assert!(plan.escapes(ir.func(f).unwrap().output));
    // The Parameter always escapes (borrowed — never written in place).
    let input = ir.func(f).unwrap().input;
    assert!(plan.escapes(input));
    // The local init literal escapes too, conservatively: on the
    // zero-iteration path its buffer IS the returned buffer (the consumer's
    // pointer-value guard is the finer instrument — plan §3's note).
    let init = ir
        .morphism(one_op(&ir, Operation::LoopEnter))
        .unwrap()
        .source;
    assert!(plan.escapes(init));

    // Rule 4 / the matmul4 note: the merge (the Update's source) is dead
    // after the update — all its uses (the decide-cone Index read, the
    // exit-route Pair and its LoopExit pin) rank before the advance-cone
    // Update; it does not escape; it is not carried.
    let upd_pos = plan.position(update_m).unwrap();
    assert!(
        plan.dead_after(merge, upd_pos),
        "source dead_after the update"
    );
    // …but not earlier: at the decide-cone Index read it is still live.
    let idx_pos = plan.position(one_op(&ir, Operation::Index)).unwrap();
    assert!(!plan.dead_after(merge, idx_pos));
    assert!(!plan.dead_after(update, upd_pos), "carried is never dead");
    // death(merge) is the Update's position — its greatest use.
    assert_eq!(plan.death(merge), Some(upd_pos));

    // Rule 1's ranking: LoopExit between the cones, LoopBack past all body.
    let exit_pos = plan.position(one_op(&ir, Operation::LoopExit)).unwrap();
    let back_pos = plan.position(one_op(&ir, Operation::LoopBack)).unwrap();
    assert!(idx_pos < exit_pos, "decide before LoopExit");
    assert!(exit_pos < upd_pos, "LoopExit before advance");
    assert!(upd_pos < back_pos, "advance before LoopBack");
}

#[test]
fn last_use_borrowed_init_is_never_dead() {
    let (ir, f) = borrowed_init_loop();
    let plan = ir.last_use_plan(f);
    let enter_m = one_op(&ir, Operation::LoopEnter);
    let init = ir.morphism(enter_m).unwrap().source;
    let merge = ir
        .objects()
        .find(|(_, o)| o.kind == flow_ir::ObjectKind::LoopMerge)
        .map(|(id, _)| id)
        .unwrap();

    // Rule 2: a Parameter feeding LoopEnter escapes — never dead, never
    // written in place ("Borrowed/init handles fail rule 2").
    assert!(plan.escapes(init));
    assert_eq!(plan.death(init), None);
    assert!(!plan.dead_after(init, plan.position(enter_m).unwrap()));
    assert!(!plan.dead_after(init, u32::MAX));
    // The merge's own dead_after stays true (the carried-update case of rule
    // 3) — so the CONSUMER's composition is "dead_after(source, idx) AND NOT
    // escapes(loop init)": the borrowed handle fails the second clause and
    // the loop falls back to the full-copy Update.
    let upd_pos = plan.position(one_op(&ir, Operation::Update)).unwrap();
    assert!(plan.dead_after(merge, upd_pos));
    assert!(plan.escapes(init), "the consumer's borrowed-init veto");
}

#[test]
fn last_use_chain_death_positions() {
    // x -> +1 -> a; a -> +2 -> b; b -> +3 -> ret, plus one Fresh object
    // nobody consumes (dead from definition).
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut adds: Vec<flow_ir::MorphismId> = Vec::new();
    let dead;
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        dead = fb
            .binop(Operation::Add, x, one, Dest::Fresh(Some("dead".into())), L)
            .unwrap();
        let a = fb
            .binop(Operation::Add, x, one, Dest::Fresh(Some("a".into())), L)
            .unwrap();
        let bb = fb
            .binop(Operation::Add, a, two, Dest::Fresh(Some("b".into())), L)
            .unwrap();
        fb.binop(Operation::Add, bb, three, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.last_use_plan(f);
    for (mid, morph) in ir.morphisms() {
        if morph.op == Operation::Add && morph.target != dead {
            adds.push(mid);
        }
    }
    adds.sort_by_key(|&m| plan.position(m).unwrap());
    let (add1, add2, add3) = (adds[0], adds[1], adds[2]);
    let a = ir.morphism(add1).unwrap().target;
    let bb = ir.morphism(add2).unwrap().target;

    // death is the consuming Add's position (through the operand-product
    // retention pin: a's handle lives until the product's own last use).
    assert_eq!(plan.death(a), Some(plan.position(add2).unwrap()));
    assert_eq!(plan.death(bb), Some(plan.position(add3).unwrap()));
    assert!(plan.dead_after(a, plan.position(add2).unwrap()));
    assert!(!plan.dead_after(a, plan.position(add1).unwrap()));
    // The Parameter escapes (rule 2).
    assert!(plan.escapes(ir.func(f).unwrap().input));
    assert_eq!(plan.death(ir.func(f).unwrap().input), None);
    // A use-less object: no use position (death None) but dead everywhere.
    assert_eq!(plan.death(dead), None);
    assert!(plan.dead_after(dead, 0));
}

#[test]
fn last_use_diamond_greatest_use_position() {
    // c -> a = c+1; c -> b = c+2; a, b -> s = a+b -> ret.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c = fb.constant(Value::I32(5), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let a = fb
            .binop(Operation::Add, c, one, Dest::Fresh(Some("a".into())), L)
            .unwrap();
        let bb = fb
            .binop(Operation::Add, c, two, Dest::Fresh(Some("b".into())), L)
            .unwrap();
        fb.binop(Operation::Add, a, bb, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.last_use_plan(f);
    let mut adds: Vec<flow_ir::MorphismId> = ir
        .morphisms()
        .filter(|(_, m)| m.op == Operation::Add)
        .map(|(id, _)| id)
        .collect();
    adds.sort_by_key(|&m| plan.position(m).unwrap());
    let (a1, a2, sum) = (adds[0], adds[1], adds[2]);
    let a = ir.morphism(a1).unwrap().target;
    let c = ir.morphism(a1).unwrap().source; // the (c, 1) operand product
    let c_obj = ir
        .in_edges(c)
        .iter()
        .map(|&m| ir.morphism(m).unwrap().source)
        .find(|&o| ir.object(o).unwrap().kind == flow_ir::ObjectKind::Constant)
        .unwrap();

    // The shared feeder dies at the LATER of its two uses.
    let later = plan.position(a1).unwrap().max(plan.position(a2).unwrap());
    assert_eq!(plan.death(c_obj), Some(later));
    assert!(!plan.dead_after(c_obj, plan.position(a1).unwrap()));
    // Each arm dies at the Sum.
    assert_eq!(plan.death(a), Some(plan.position(sum).unwrap()));
    assert!(plan.dead_after(a, plan.position(sum).unwrap()));
}

#[test]
fn last_use_escape_via_pair_field() {
    // A locally-built array packed into a returned product escapes (rule 2's
    // "through Pair fields"): death ⊥, never dead_after.
    let arr2 = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 2,
    };
    let ret_ty = Ty::Tuple(vec![arr2.clone(), Ty::i32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "mk", Ty::i32(), ret_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let a = fb
            .pack_array(&[one, two], Dest::Fresh(Some("a".into())), L)
            .unwrap();
        fb.pack(&[a, x], Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.last_use_plan(f);
    let a = ir
        .objects()
        .find(|(_, o)| o.name.as_deref() == Some("a"))
        .map(|(id, _)| id)
        .unwrap();
    assert!(plan.escapes(a));
    assert_eq!(plan.death(a), None, "escaping ⇒ death ⊥");
    assert!(!plan.dead_after(a, 0));
    assert!(!plan.dead_after(a, u32::MAX));
}

#[test]
fn last_use_two_sequential_loops_own_carried_sets() {
    // The S12 attribution trap: two sequential canonical loops in one fn —
    // each loop's carried set is its own, never the per-function union.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let n = fb.input();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let lh1 = fb.begin_loop(zero, L).unwrap();
        let m1 = fb.merge_of(&lh1);
        let c1 = fb
            .binop(Operation::Lt, m1, n, Dest::Fresh(None), L)
            .unwrap();
        let i1 = fb
            .binop(Operation::Add, m1, one, Dest::Fresh(Some("i1".into())), L)
            .unwrap();
        fb.loop_back(&lh1, i1, c1, L).unwrap();
        let aa = fb
            .loop_exit(&lh1, m1, c1, Dest::Fresh(Some("aa".into())), L)
            .unwrap();
        fb.end_loop(lh1).unwrap();

        let lh2 = fb.begin_loop(zero, L).unwrap();
        let m2 = fb.merge_of(&lh2);
        let c2 = fb
            .binop(Operation::Lt, m2, n, Dest::Fresh(None), L)
            .unwrap();
        let j2 = fb
            .binop(Operation::Add, m2, two, Dest::Fresh(Some("j2".into())), L)
            .unwrap();
        fb.loop_back(&lh2, j2, c2, L).unwrap();
        let bb = fb
            .loop_exit(&lh2, m2, c2, Dest::Fresh(Some("bb".into())), L)
            .unwrap();
        fb.end_loop(lh2).unwrap();

        fb.binop(Operation::Add, aa, bb, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let plan = ir.last_use_plan(f);
    let obj_named = |name: &str| {
        ir.objects()
            .find(|(_, o)| o.name.as_deref() == Some(name))
            .map(|(id, _)| id)
            .unwrap()
    };
    let merges: Vec<flow_ir::ObjectId> = ir
        .objects()
        .filter(|(_, o)| o.kind == flow_ir::ObjectKind::LoopMerge)
        .map(|(id, _)| id)
        .collect();
    let (m1, m2) = (merges[0], merges[1]);
    let (i1, j2) = (obj_named("i1"), obj_named("j2"));

    // Each loop's carried set is its own.
    assert_eq!(plan.carried_by(i1), Some(m1));
    assert_eq!(plan.carried_by(j2), Some(m2));
    assert_ne!(plan.carried_by(i1), Some(m2));
    assert_ne!(plan.carried_by(j2), Some(m1));
    // merge1 does not escape through its own exit even though its value
    // flows (via the exit object) into the post-loop Add — which CONSUMES it
    // (a scalar; computational edges carry no escape, only retention edges
    // do — rule 2's "through Pair fields and Phi arms").
    assert!(!plan.escapes(m1));
    assert!(!plan.escapes(obj_named("aa")));
    assert!(!plan.escapes(m2));
    // Rule 1 per loop: LoopBack past all of ITS loop's body morphisms.
    let backs: Vec<flow_ir::MorphismId> = ir
        .morphisms()
        .filter(|(_, m)| m.op == Operation::LoopBack)
        .map(|(id, _)| id)
        .collect();
    let (lb1, lb2) = (backs[0], backs[1]);
    let add_of = |tgt: flow_ir::ObjectId| {
        ir.in_edges(tgt)
            .iter()
            .map(|&m| ir.morphism(m).unwrap().id)
            .find(|&m| ir.morphism(m).unwrap().op == Operation::Add)
            .unwrap()
    };
    assert!(
        plan.position(lb1).unwrap() > plan.position(add_of(i1)).unwrap(),
        "loop1's LoopBack past its body"
    );
    assert!(
        plan.position(lb2).unwrap() > plan.position(add_of(j2)).unwrap(),
        "loop2's LoopBack past its body"
    );
}

#[test]
fn last_use_determinism_run_twice_equal() {
    let (ir, f) = carried_array_loop();
    assert_eq!(ir.last_use_plan(f), ir.last_use_plan(f));
    let (ir, f) = borrowed_init_loop();
    assert_eq!(ir.last_use_plan(f), ir.last_use_plan(f));
}

#[test]
fn last_use_total_on_noncanonical_and_deep_graphs() {
    // The fused multi-merge graph (legal-but-degenerate): loop_plan is None
    // for both merges, so the plan is all-conservative — no carried set, no
    // re-ranking, full escape reachability — and must not panic (rule 6).
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "nested", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let c0 = fb.constant(Value::I32(0), L).unwrap();
        let c1 = fb.constant(Value::I32(0), L).unwrap();
        let outer = fb.begin_loop(c0, L).unwrap();
        let om = fb.merge_of(&outer);
        let inner = fb.begin_loop(c1, L).unwrap();
        let im = fb.merge_of(&inner);
        let ct = fb.constant(Value::Bool(true), L).unwrap();
        let cf = fb.constant(Value::Bool(false), L).unwrap();
        let inext = fb
            .binop(Operation::Add, om, im, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        fb.loop_back(&inner, inext, ct, L).unwrap();
        let onext = fb
            .binop(Operation::Add, im, om, Dest::Fresh(Some("onext".into())), L)
            .unwrap();
        fb.loop_back(&outer, onext, ct, L).unwrap();
        let _iout = fb
            .loop_exit(&inner, im, cf, Dest::Fresh(Some("iout".into())), L)
            .unwrap();
        fb.end_loop(inner).unwrap();
        fb.loop_exit(&outer, om, cf, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(outer).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.last_use_plan(f);
    // No canonical plan ⇒ no carried classification (consumers fall back).
    for (o, _) in ir.objects() {
        assert_eq!(plan.carried_by(o), None, "non-canonical: nothing carried");
    }
    // Every morphism still has a position (the plain topo ranking).
    for (m, _) in ir.morphisms() {
        assert!(plan.position(m).is_some());
    }
    // The OUTER merge reaches the Return through retention edges (its exit
    // payload is the Return) ⇒ escapes (rule 2, conservative); the inner's
    // exit object is never consumed, so the inner merge genuinely does not
    // escape (its exit value is dropped). (loop_structure reports the merges
    // in insertion order: outer, then inner.)
    let (om, im) = (
        ir.loop_structure(f)[0].merges[0],
        ir.loop_structure(f)[0].merges[1],
    );
    assert!(plan.escapes(om));
    assert!(!plan.escapes(im));

    // The §16 deep chain (100k): the sweep stays linear and stackless (J1).
    let (ir, f) = {
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
        (b.seal(f).unwrap(), f)
    };
    let plan = ir.last_use_plan(f);
    // The chain's second-to-last accumulator dies at the final Add.
    let mut adds: Vec<flow_ir::MorphismId> = ir
        .morphisms()
        .filter(|(_, m)| m.op == Operation::Add)
        .map(|(id, _)| id)
        .collect();
    adds.sort_by_key(|&m| plan.position(m).unwrap());
    let last = *adds.last().unwrap();
    let penultimate_acc = ir.morphism(adds[adds.len() - 2]).unwrap().target;
    assert_eq!(
        plan.death(penultimate_acc),
        Some(plan.position(last).unwrap())
    );
}

#[test]
fn last_use_total_on_alg_suite_graphs() {
    // Totality sweep: every graph this suite builds plans without panic
    // (the flow-rewrite testgen generator is NOT reachable from flow-ir's
    // tests — it imports flow_interp, which is downstream of flow-ir — so
    // the plan §6.1 brute-force agreement row lives with the consumers).
    let (ir, f) = counting_loop();
    let _ = ir.last_use_plan(f);
    let (ir, f) = carried_array_loop();
    let _ = ir.last_use_plan(f);
    let (ir, f) = borrowed_init_loop();
    let _ = ir.last_use_plan(f);
    // And every fn of a multi-fn module: plan per fn, independently.
    let mut b = IrBuilder::new();
    let g = b
        .declare(FuncKind::Named, "g", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(g).unwrap();
        let x = fb.input();
        fb.output(x, None, L).unwrap();
        fb.finish().unwrap();
    }
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.call(g, one, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    for (id, _) in ir.funcs() {
        let _ = ir.last_use_plan(id);
    }
}

// --- bounds_proof (the S20 kernel-gap analysis: provably-in-bounds Index) -----

fn i32_arr(n: u64) -> Ty {
    Ty::Array {
        elem: Box::new(Ty::i32()),
        size: n,
    }
}

fn index_morphs(ir: &flow_ir::CategoryIr, f: flow_ir::FuncId) -> Vec<flow_ir::MorphismId> {
    ir.morphisms()
        .filter(|(m, _)| ir.morphism(*m).unwrap().op == Operation::Index)
        .map(|(m, _)| m)
        .filter(|&m| {
            let mm = ir.morphism(m).unwrap();
            ir.try_owner(mm.source) == Some(f) || ir.try_owner(mm.target) == Some(f)
        })
        .collect()
}

#[test]
fn bounds_constant_index_in_range_proven_out_not() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", i32_arr(16), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let x = fb.index(a, three, Dest::Fresh(None), L).unwrap();
        let twenty = fb.constant(Value::I32(20), L).unwrap();
        let y = fb.index(a, twenty, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, x, y, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let bp = ir.bounds_proof(f);
    let ms = index_morphs(&ir, f);
    assert_eq!(ms.len(), 2);
    assert!(bp.proven(ms[0]), "constant 3 < 16 proves");
    assert!(
        !bp.proven(ms[1]),
        "constant 20 >= 16 does not (the guard stays)"
    );
}

#[test]
fn bounds_iota_element_ranged() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(
            FuncKind::Named,
            "m",
            Ty::Tuple(vec![i32_arr(16), Ty::Unit]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        let a = fb.proj(inp, 0, Dest::Fresh(None), L).unwrap();
        let n = fb.constant(Value::I32(16), L).unwrap();
        let tr = fb.iota(n, Dest::Fresh(None), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let e = fb.index(tr, three, Dest::Fresh(None), L).unwrap();
        fb.index(a, e, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let bp = ir.bounds_proof(f);
    let ms = index_morphs(&ir, f);
    assert_eq!(ms.len(), 2);
    assert!(
        bp.proven(ms[1]),
        "the iota element [0,16) is in-bounds of [i32;16]"
    );
}

/// The acceptance shape: an enumerate'd map body with the matmul's affine
/// indexing (`i = t / 4`, `j = t % 4`, `a[i*4+j]`) — proven when `a` is big
/// enough, not when it isn't.
fn enumerate_cell(arr_size: u64) -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let mut b = IrBuilder::new();
    let body = b
        .declare(
            FuncKind::MapBody,
            "cell",
            Ty::Tuple(vec![
                i32_arr(arr_size),
                Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            ]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let p = fb.input();
        let cap = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let e = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let t = fb.proj(e, 0, Dest::Fresh(None), L).unwrap();
        let four = fb.constant(Value::I32(4), L).unwrap();
        let i = fb
            .binop(Operation::Div, t, four, Dest::Fresh(None), L)
            .unwrap();
        let j = fb
            .binop(Operation::Mod, t, four, Dest::Fresh(None), L)
            .unwrap();
        let i4 = fb
            .binop(Operation::Mul, i, four, Dest::Fresh(None), L)
            .unwrap();
        let y = fb
            .binop(Operation::Add, i4, j, Dest::Fresh(None), L)
            .unwrap();
        fb.index(cap, y, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![i32_arr(64), i32_arr(arr_size)]),
            i32_arr(64),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        let seed = fb.proj(inp, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(inp, 1, Dest::Fresh(None), L).unwrap();
        let en = fb.enumerate(seed, Dest::Fresh(None), L).unwrap();
        fb.map_captured(body, &[a], en, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    (ir, body)
}

#[test]
fn bounds_enumerate_affine_proven_when_sized() {
    let (ir, body) = enumerate_cell(64);
    let bp = ir.bounds_proof(body);
    let ms = index_morphs(&ir, body);
    assert_eq!(ms.len(), 1);
    assert!(bp.proven(ms[0]), "i*4+j <= 15*4+3 = 63 < 64 proves");
}

#[test]
fn bounds_enumerate_affine_not_proven_when_undersized() {
    let (ir, body) = enumerate_cell(32);
    let bp = ir.bounds_proof(body);
    let ms = index_morphs(&ir, body);
    assert_eq!(ms.len(), 1);
    assert!(!bp.proven(ms[0]), "63 >= 32 does not prove (guard stays)");
}

#[test]
fn bounds_literal_ramp_elements() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(
            FuncKind::Named,
            "m",
            Ty::Tuple(vec![i32_arr(4), i32_arr(2)]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        let a4 = fb.proj(inp, 0, Dest::Fresh(None), L).unwrap();
        let a2 = fb.proj(inp, 1, Dest::Fresh(None), L).unwrap();
        let z = fb.constant(Value::I32(0), L).unwrap();
        let o = fb.constant(Value::I32(1), L).unwrap();
        let tw = fb.constant(Value::I32(2), L).unwrap();
        let th = fb.constant(Value::I32(3), L).unwrap();
        let kr = fb
            .pack_array(&[z, o, tw, th], Dest::Fresh(None), L)
            .unwrap();
        let e = fb.index(kr, o, Dest::Fresh(None), L).unwrap();
        let x = fb.index(a4, e, Dest::Fresh(None), L).unwrap();
        let y = fb.index(a2, e, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, x, y, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let bp = ir.bounds_proof(f);
    let ms = index_morphs(&ir, f);
    assert_eq!(ms.len(), 3);
    assert!(bp.proven(ms[1]), "ramp element [0,3] < 4");
    assert!(!bp.proven(ms[2]), "ramp element [0,3] >= 2 — guard stays");
}

#[test]
fn bounds_wrap_and_negative_bail() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", i32_arr(300), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        // u8: 16 * 17 = 272 > 255 — wraparound possible, range unknown.
        let x = fb.constant(Value::U8(16), L).unwrap();
        let y = fb.constant(Value::U8(17), L).unwrap();
        let m = fb
            .binop(Operation::Mul, x, y, Dest::Fresh(None), L)
            .unwrap();
        // 3 - 4 could wrap negative — range unknown.
        let three = fb.constant(Value::I32(3), L).unwrap();
        let four = fb.constant(Value::I32(4), L).unwrap();
        let d = fb
            .binop(Operation::Sub, three, four, Dest::Fresh(None), L)
            .unwrap();
        let p = fb.index(a, m, Dest::Fresh(None), L).unwrap();
        let q = fb.index(a, d, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, p, q, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let bp = ir.bounds_proof(f);
    let ms = index_morphs(&ir, f);
    assert_eq!(ms.len(), 2);
    assert!(!bp.proven(ms[0]), "u8 mul overflow — unknown — not proven");
    assert!(
        !bp.proven(ms[1]),
        "negative-going Sub — unknown — not proven"
    );
}

#[test]
fn bounds_determinism_and_totality() {
    let (ir, body) = enumerate_cell(64);
    let main = ir.entry();
    let a = ir.bounds_proof(body);
    let b2 = ir.bounds_proof(body);
    assert_eq!(a, b2, "same graph → same plan (L2)");
    // totality: every fn in the graph answers, including the entry.
    let _ = ir.bounds_proof(main);
    let _ = ir.bounds_proof(body);
}

/// The real bench shape: a map body computing `i = t/8`, `j = t%8` from the
/// enumerate element, with a nested fold capturing `i`/`j`/arrays over a
/// literal ramp — the fold's `a[i*8+k]`/`b[k*8+j]` reads. The capture-range
/// threading must prove BOTH Index reads (the S20 kernel-gap acceptance).
#[test]
fn bounds_capture_shape_matmul_proven() {
    let mut b = IrBuilder::new();
    // fold body: (i, j, a, b, acc, k) -> acc + a[i*8+k] * b[k*8+j]
    let dot = b
        .declare(
            FuncKind::FoldBody,
            "dot",
            Ty::Tuple(vec![
                Ty::i32(),
                Ty::i32(),
                i32_arr(64),
                i32_arr(64),
                Ty::i32(),
                Ty::i32(),
            ]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(dot).unwrap();
        let p = fb.input();
        let i = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let j = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(p, 3, Dest::Fresh(None), L).unwrap();
        let acc = fb.proj(p, 4, Dest::Fresh(None), L).unwrap();
        let k = fb.proj(p, 5, Dest::Fresh(None), L).unwrap();
        let eight = fb.constant(Value::I32(8), L).unwrap();
        let i8 = fb
            .binop(Operation::Mul, i, eight, Dest::Fresh(None), L)
            .unwrap();
        let y1 = fb
            .binop(Operation::Add, i8, k, Dest::Fresh(None), L)
            .unwrap();
        let k8 = fb
            .binop(Operation::Mul, k, eight, Dest::Fresh(None), L)
            .unwrap();
        let y2 = fb
            .binop(Operation::Add, k8, j, Dest::Fresh(None), L)
            .unwrap();
        let xa = fb.index(a, y1, Dest::Fresh(None), L).unwrap();
        let xb = fb.index(bb, y2, Dest::Fresh(None), L).unwrap();
        let pr = fb
            .binop(Operation::Mul, xa, xb, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, acc, pr, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    // map body: (a, b, E) -> fold(dot, [i, j, a, b], 0, krange)
    let cell = b
        .declare(
            FuncKind::MapBody,
            "cell",
            Ty::Tuple(vec![
                i32_arr(64),
                i32_arr(64),
                Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            ]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(cell).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        let e = fb.proj(p, 2, Dest::Fresh(None), L).unwrap();
        let t = fb.proj(e, 0, Dest::Fresh(None), L).unwrap();
        let eight = fb.constant(Value::I32(8), L).unwrap();
        let i = fb
            .binop(Operation::Div, t, eight, Dest::Fresh(None), L)
            .unwrap();
        let j = fb
            .binop(Operation::Mod, t, eight, Dest::Fresh(None), L)
            .unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let four = fb.constant(Value::I32(4), L).unwrap();
        let five = fb.constant(Value::I32(5), L).unwrap();
        let six = fb.constant(Value::I32(6), L).unwrap();
        let seven = fb.constant(Value::I32(7), L).unwrap();
        let kr = fb
            .pack_array(
                &[zero, one, two, three, four, five, six, seven],
                Dest::Fresh(None),
                L,
            )
            .unwrap();
        let z = fb.constant(Value::I32(0), L).unwrap();
        fb.fold_captured(dot, &[i, j, a, bb], z, kr, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    // main: (seed, a, b) -> enumerate -> map_captured(cell, [a, b])
    let f = b
        .declare(
            FuncKind::Named,
            "main",
            Ty::Tuple(vec![i32_arr(64), i32_arr(64), i32_arr(64)]),
            i32_arr(64),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        let seed = fb.proj(inp, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(inp, 1, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(inp, 2, Dest::Fresh(None), L).unwrap();
        let en = fb.enumerate(seed, Dest::Fresh(None), L).unwrap();
        fb.map_captured(cell, &[a, bb], en, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    let bp = ir.bounds_proof(dot);
    let ms = index_morphs(&ir, dot);
    assert_eq!(ms.len(), 2);
    assert!(
        bp.proven(ms[0]) && bp.proven(ms[1]),
        "capture-threaded: i*8+k and k*8+j both prove (<= 7*8+7 = 63 < 64)"
    );
}

// --- tile_plan (bit-exact map/fold matmul tiling) ----------------------------

#[derive(Clone, Copy)]
enum TileFixtureVariant {
    Standard,
    Postprocess,
    NonconstantSeed,
    RowMajorB,
    UndersizedA,
    DeadCall,
}

fn tile_array(elem: &Ty, size: u64) -> Ty {
    Ty::Array {
        elem: Box::new(elem.clone()),
        size,
    }
}

fn tile_count(n: u64) -> Value {
    Value::I32(i32::try_from(n).unwrap())
}

fn tile_zero(elem: &Ty) -> Value {
    match elem {
        Ty::Float { bits: 32 } => Value::F32(0.0),
        Ty::Float { bits: 64 } => Value::F64(0.0),
        _ => panic!("tile fixture only uses f32/f64"),
    }
}

fn tile_one(elem: &Ty) -> Value {
    match elem {
        Ty::Float { bits: 32 } => Value::F32(1.0),
        Ty::Float { bits: 64 } => Value::F64(1.0),
        _ => panic!("tile fixture only uses f32/f64"),
    }
}

fn tile_matmul_fixture_with(
    rows: u64,
    c: u64,
    k: u64,
    elem: Ty,
    variant: TileFixtureVariant,
) -> (flow_ir::CategoryIr, flow_ir::FuncId, flow_ir::FuncId) {
    let mapped_size = rows.checked_mul(c).unwrap();
    let full_a_size = rows.checked_mul(k).unwrap();
    let a_size = if matches!(variant, TileFixtureVariant::UndersizedA) {
        full_a_size - 1
    } else {
        full_a_size
    };
    let full_b_size = k.checked_mul(c).unwrap();
    let b_size = if matches!(variant, TileFixtureVariant::RowMajorB) {
        c.checked_sub(1)
            .unwrap()
            .checked_mul(c)
            .unwrap()
            .checked_add(k)
            .unwrap()
    } else {
        full_b_size
    };
    let a_ty = tile_array(&elem, a_size);
    let b_ty = tile_array(&elem, b_size);
    let k_ty = i32_arr(k);

    let mut b = IrBuilder::new();
    // DeadCall: a Named identity whose call in the map body is dead — the
    // micro-kernel would skip it, so tile_plan must refuse (a skipped callee
    // could trap in general; R1 requires refusal, not hope).
    let id_fn = matches!(variant, TileFixtureVariant::DeadCall).then(|| {
        let id_fn = b
            .declare(FuncKind::Named, "tile_id", Ty::i32(), Ty::i32(), L)
            .unwrap();
        let mut fb = b.build_fn(id_fn).unwrap();
        let input = fb.input();
        let z = fb.constant(Value::I32(0), L).unwrap();
        fb.binop(Operation::Add, input, z, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
        id_fn
    });
    let dot = b
        .declare(
            FuncKind::FoldBody,
            "tile_dot",
            Ty::Tuple(vec![
                a_ty.clone(),
                Ty::i32(),
                b_ty.clone(),
                Ty::i32(),
                elem.clone(),
                Ty::i32(),
            ]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(dot).unwrap();
        let input = fb.input();
        let a = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let i = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let j = fb.proj(input, 3, Dest::Fresh(None), L).unwrap();
        let acc = fb.proj(input, 4, Dest::Fresh(None), L).unwrap();
        let kk = fb.proj(input, 5, Dest::Fresh(None), L).unwrap();
        let ca = fb.constant(tile_count(k), L).unwrap();
        let cb = fb.constant(tile_count(c), L).unwrap();
        let i_ca = fb
            .binop(Operation::Mul, i, ca, Dest::Fresh(None), L)
            .unwrap();
        let a_index = fb
            .binop(Operation::Add, i_ca, kk, Dest::Fresh(None), L)
            .unwrap();
        let b_index = if matches!(variant, TileFixtureVariant::RowMajorB) {
            let j_cb = fb
                .binop(Operation::Mul, j, cb, Dest::Fresh(None), L)
                .unwrap();
            fb.binop(Operation::Add, j_cb, kk, Dest::Fresh(None), L)
                .unwrap()
        } else {
            let k_cb = fb
                .binop(Operation::Mul, kk, cb, Dest::Fresh(None), L)
                .unwrap();
            fb.binop(Operation::Add, k_cb, j, Dest::Fresh(None), L)
                .unwrap()
        };
        let a_value = fb.index(a, a_index, Dest::Fresh(None), L).unwrap();
        let b_value = fb.index(bb, b_index, Dest::Fresh(None), L).unwrap();
        let product = fb
            .binop(Operation::Mul, a_value, b_value, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, acc, product, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    let cell = b
        .declare(
            FuncKind::MapBody,
            "tile_cell",
            Ty::Tuple(vec![k_ty.clone(), a_ty.clone(), b_ty.clone(), Ty::i32()]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(cell).unwrap();
        let input = fb.input();
        let krange = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let bb = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let t = fb.proj(input, 3, Dest::Fresh(None), L).unwrap();
        let columns = fb.constant(tile_count(c), L).unwrap();
        if let Some(id_fn) = id_fn {
            fb.call(id_fn, t, Dest::Fresh(None), L).unwrap();
        }
        let i = fb
            .binop(Operation::Div, t, columns, Dest::Fresh(None), L)
            .unwrap();
        let j = fb
            .binop(Operation::Mod, t, columns, Dest::Fresh(None), L)
            .unwrap();
        let zero = fb.constant(tile_zero(&elem), L).unwrap();
        let seed = if matches!(variant, TileFixtureVariant::NonconstantSeed) {
            fb.binop(Operation::Add, zero, zero, Dest::Fresh(None), L)
                .unwrap()
        } else {
            zero
        };
        let dest = if matches!(variant, TileFixtureVariant::Postprocess) {
            Dest::Fresh(None)
        } else {
            Dest::Ret { slot: None }
        };
        let folded = fb
            .fold_captured(dot, &[a, i, bb, j], seed, krange, dest, L)
            .unwrap();
        if matches!(variant, TileFixtureVariant::Postprocess) {
            let one = fb.constant(tile_one(&elem), L).unwrap();
            fb.binop(Operation::Add, folded, one, Dest::Ret { slot: None }, L)
                .unwrap();
        }
        fb.finish().unwrap();
    }

    let main = b
        .declare(
            FuncKind::Named,
            "tile_main",
            Ty::Unit,
            tile_array(&elem, mapped_size),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let mapped_count = fb.constant(tile_count(mapped_size), L).unwrap();
        let mapped = fb.iota(mapped_count, Dest::Fresh(None), L).unwrap();
        let k_count = fb.constant(tile_count(k), L).unwrap();
        let krange = fb.iota(k_count, Dest::Fresh(None), L).unwrap();
        let one = fb.constant(tile_one(&elem), L).unwrap();
        let a_count = fb.constant(tile_count(a_size), L).unwrap();
        let a = fb.fill(one, a_count, Dest::Fresh(None), L).unwrap();
        let b_count = fb.constant(tile_count(b_size), L).unwrap();
        let bb = fb.fill(one, b_count, Dest::Fresh(None), L).unwrap();
        fb.map_captured(cell, &[krange, a, bb], mapped, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    (ir, main, dot)
}

fn tile_matmul_fixture(
    rows: u64,
    c: u64,
    k: u64,
) -> (flow_ir::CategoryIr, flow_ir::FuncId, flow_ir::FuncId) {
    tile_matmul_fixture_with(rows, c, k, Ty::f64(), TileFixtureVariant::Standard)
}

#[test]
fn tile_matmul_site_recognized() {
    let (ir, main, _) = tile_matmul_fixture(512, 512, 512);
    let plan = ir.tile_plan(main);
    assert_eq!(plan.sites.len(), 1);
    let (site_id, site) = plan.sites.iter().next().unwrap();
    assert!(matches!(
        ir.morphism(site_id).unwrap().op,
        Operation::Map { .. }
    ));
    assert_eq!(
        site,
        &TileSite {
            rows: 512,
            c: 512,
            k: 512,
            a: TileRead {
                slot: 1,
                base: 0,
                ci: 512,
                ck: 1,
                clane: 0,
            },
            b: TileRead {
                slot: 2,
                base: 0,
                ci: 0,
                ck: 512,
                clane: 1,
            },
            seed: Value::F64(0.0),
            elem: Ty::f64(),
            mul_a_first: true,
            add_acc_first: true,
        }
    );
}

#[test]
fn bounds_matmul_fold_body_proven_through_captures() {
    let (ir, _, fold_body) = tile_matmul_fixture(8, 8, 8);
    let bounds = ir.bounds_proof(fold_body);
    let indexes = index_morphs(&ir, fold_body);
    assert_eq!(indexes.len(), 2);
    assert!(indexes.into_iter().all(|m| bounds.proven(m)));
}

#[test]
fn tile_refuses_postprocessed_fold() {
    let (ir, main, _) =
        tile_matmul_fixture_with(2, 3, 4, Ty::f64(), TileFixtureVariant::Postprocess);
    assert!(ir.tile_plan(main).sites.is_empty());
}

#[test]
fn tile_refuses_nonconstant_seed() {
    let (ir, main, _) =
        tile_matmul_fixture_with(2, 3, 4, Ty::f64(), TileFixtureVariant::NonconstantSeed);
    assert!(ir.tile_plan(main).sites.is_empty());
}

#[test]
fn tile_refuses_nonunit_lane_stride() {
    let (ir, main, fold_body) =
        tile_matmul_fixture_with(2, 3, 4, Ty::f64(), TileFixtureVariant::RowMajorB);
    let bounds = ir.bounds_proof(fold_body);
    assert!(
        index_morphs(&ir, fold_body)
            .into_iter()
            .all(|m| bounds.proven(m)),
        "row-major b[j*C+k] is in bounds; refusal is clane=C"
    );
    assert!(ir.tile_plan(main).sites.is_empty());
}

#[test]
fn tile_refuses_unproven_index() {
    let (ir, main, _) =
        tile_matmul_fixture_with(2, 3, 4, Ty::f64(), TileFixtureVariant::UndersizedA);
    assert!(ir.tile_plan(main).sites.is_empty());
}

fn tile_fir_fixture(
    m: u64,
    k: u64,
    lane_stride: u64,
) -> (flow_ir::CategoryIr, flow_ir::FuncId, flow_ir::FuncId) {
    let elem = Ty::f32();
    let w_ty = tile_array(&elem, k);
    let x_size = m
        .checked_sub(1)
        .unwrap()
        .checked_mul(lane_stride)
        .unwrap()
        .checked_add(k)
        .unwrap();
    let x_ty = tile_array(&elem, x_size);
    let k_ty = i32_arr(k);

    let mut b = IrBuilder::new();
    let dot = b
        .declare(
            FuncKind::FoldBody,
            "tile_fir_dot",
            Ty::Tuple(vec![
                w_ty.clone(),
                x_ty.clone(),
                Ty::i32(),
                elem.clone(),
                Ty::i32(),
            ]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(dot).unwrap();
        let input = fb.input();
        let w = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let x = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let t = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let acc = fb.proj(input, 3, Dest::Fresh(None), L).unwrap();
        let kk = fb.proj(input, 4, Dest::Fresh(None), L).unwrap();
        let x_base = if lane_stride == 1 {
            t
        } else {
            let stride = fb.constant(tile_count(lane_stride), L).unwrap();
            fb.binop(Operation::Mul, t, stride, Dest::Fresh(None), L)
                .unwrap()
        };
        let x_index = fb
            .binop(Operation::Add, x_base, kk, Dest::Fresh(None), L)
            .unwrap();
        let w_value = fb.index(w, kk, Dest::Fresh(None), L).unwrap();
        let x_value = fb.index(x, x_index, Dest::Fresh(None), L).unwrap();
        let product = fb
            .binop(Operation::Mul, w_value, x_value, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, acc, product, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    let sample = b
        .declare(
            FuncKind::MapBody,
            "tile_fir_sample",
            Ty::Tuple(vec![w_ty.clone(), x_ty.clone(), k_ty.clone(), Ty::i32()]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(sample).unwrap();
        let input = fb.input();
        let w = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let x = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let kr = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let t = fb.proj(input, 3, Dest::Fresh(None), L).unwrap();
        let seed = fb.constant(Value::F32(0.0), L).unwrap();
        fb.fold_captured(dot, &[w, x, t], seed, kr, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    let main = b
        .declare(
            FuncKind::Named,
            "tile_fir_main",
            Ty::Unit,
            tile_array(&elem, m),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let one = fb.constant(Value::F32(1.0), L).unwrap();
        let w_count = fb.constant(tile_count(k), L).unwrap();
        let w = fb.fill(one, w_count, Dest::Fresh(None), L).unwrap();
        let x_count = fb.constant(tile_count(x_size), L).unwrap();
        let x = fb.fill(one, x_count, Dest::Fresh(None), L).unwrap();
        let k_count = fb.constant(tile_count(k), L).unwrap();
        let kr = fb.iota(k_count, Dest::Fresh(None), L).unwrap();
        let mapped_count = fb.constant(tile_count(m), L).unwrap();
        let mapped = fb.iota(mapped_count, Dest::Fresh(None), L).unwrap();
        fb.map_captured(sample, &[w, x, kr], mapped, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    (ir, main, dot)
}

#[test]
fn tile_fir_site_recognized() {
    let (ir, main, _) = tile_fir_fixture(64, 8, 1);
    let plan = ir.tile_plan(main);
    assert_eq!(plan.sites.len(), 1);
    assert_eq!(
        plan.sites.iter().next().unwrap().1,
        &TileSite {
            rows: 1,
            c: 64,
            k: 8,
            a: TileRead {
                slot: 0,
                base: 0,
                ci: 0,
                ck: 1,
                clane: 0,
            },
            b: TileRead {
                slot: 1,
                base: 0,
                ci: 0,
                ck: 1,
                clane: 1,
            },
            seed: Value::F32(0.0),
            elem: Ty::f32(),
            mul_a_first: true,
            add_acc_first: true,
        }
    );
}

#[test]
fn tile_refuses_fir_nonunit_lane_stride() {
    let (ir, main, fold_body) = tile_fir_fixture(64, 8, 2);
    let bounds = ir.bounds_proof(fold_body);
    assert!(
        index_morphs(&ir, fold_body)
            .into_iter()
            .all(|m| bounds.proven(m)),
        "x[2*t+k] is in bounds; refusal is clane=2"
    );
    assert!(ir.tile_plan(main).sites.is_empty());
}

fn tile_no_split_fixture() -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let elem = Ty::f64();
    let array = tile_array(&elem, 8);
    let krange = i32_arr(4);
    let mut b = IrBuilder::new();
    let fold_body = b
        .declare(
            FuncKind::FoldBody,
            "tile_1d_fold",
            Ty::Tuple(vec![array.clone(), Ty::i32(), elem.clone(), Ty::i32()]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(fold_body).unwrap();
        let input = fb.input();
        let a = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let i = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let acc = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let value = fb.index(a, i, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, acc, value, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let map_body = b
        .declare(
            FuncKind::MapBody,
            "tile_1d_map",
            Ty::Tuple(vec![krange.clone(), array.clone(), Ty::i32()]),
            elem.clone(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(map_body).unwrap();
        let input = fb.input();
        let kr = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let a = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let i = fb.proj(input, 2, Dest::Fresh(None), L).unwrap();
        let seed = fb.constant(Value::F64(0.0), L).unwrap();
        fb.fold_captured(fold_body, &[a, i], seed, kr, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let main = b
        .declare(FuncKind::Named, "tile_1d", Ty::Unit, array, L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let mapped_count = fb.constant(Value::I32(8), L).unwrap();
        let mapped = fb.iota(mapped_count, Dest::Fresh(None), L).unwrap();
        let k_count = fb.constant(Value::I32(4), L).unwrap();
        let kr = fb.iota(k_count, Dest::Fresh(None), L).unwrap();
        let one = fb.constant(Value::F64(1.0), L).unwrap();
        let a_count = fb.constant(Value::I32(8), L).unwrap();
        let a = fb.fill(one, a_count, Dest::Fresh(None), L).unwrap();
        fb.map_captured(map_body, &[kr, a], mapped, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert!(validate(&ir).is_empty(), "{:?}", validate(&ir));
    (ir, main)
}

#[test]
fn tile_refuses_non_fma_1d_map() {
    let (ir, main) = tile_no_split_fixture();
    assert!(ir.tile_plan(main).sites.is_empty());
}

#[test]
fn tile_refuses_dead_call_in_map_body() {
    let (ir, main, _) = tile_matmul_fixture_with(2, 3, 4, Ty::f64(), TileFixtureVariant::DeadCall);
    assert!(ir.tile_plan(main).sites.is_empty());
}

#[test]
fn tile_matmul_f32_site_recognized() {
    let (ir, main, _) =
        tile_matmul_fixture_with(1024, 1024, 1024, Ty::f32(), TileFixtureVariant::Standard);
    let plan = ir.tile_plan(main);
    assert_eq!(plan.sites.len(), 1);
    let site = plan.sites.iter().next().unwrap().1;
    assert_eq!(site.elem, Ty::f32());
    assert_eq!(site.seed, Value::F32(0.0));
}

// --- path_plan (parallel task DAG + host-spine checkpoints) ------------------

fn identity_map_body(b: &mut IrBuilder) -> flow_ir::FuncId {
    let body = b
        .declare(FuncKind::MapBody, "identity", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        fb.output(fb.input(), None, L).unwrap();
        fb.finish().unwrap();
    }
    body
}

fn add_fold_body(b: &mut IrBuilder) -> flow_ir::FuncId {
    let body = b
        .declare(
            FuncKind::FoldBody,
            "sum",
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let input = fb.input();
        let acc = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let elem = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, acc, elem, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    body
}

fn task_for(plan: &PathPlan, morphism: MorphismId) -> usize {
    plan.tasks
        .iter()
        .position(|task| match &task.kind {
            TaskKind::Split { site, .. } => *site == morphism,
            TaskKind::Seq { morphisms } => morphisms.contains(&morphism),
        })
        .expect("morphism belongs to a task")
}

fn func_ops(
    ir: &flow_ir::CategoryIr,
    f: flow_ir::FuncId,
    pred: impl Fn(Operation) -> bool,
) -> Vec<MorphismId> {
    ir.func(f)
        .unwrap()
        .morphisms
        .iter()
        .copied()
        .filter(|&m| pred(ir.morphism(m).unwrap().op))
        .collect()
}

fn topo_pos(ir: &flow_ir::CategoryIr, f: flow_ir::FuncId, m: MorphismId) -> u32 {
    ir.topo_order(f).iter().position(|&site| site == m).unwrap() as u32
}

fn diamond_path_fixture() -> (flow_ir::CategoryIr, flow_ir::FuncId) {
    let mut b = IrBuilder::new();
    let body = identity_map_body(&mut b);
    let output = Ty::Array {
        elem: Box::new(Ty::Tuple(vec![Ty::i32(), Ty::i32()])),
        size: 8,
    };
    let f = b
        .declare(FuncKind::Named, "diamond", i32_arr(8), output, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        let left = fb.map(body, input, Dest::Fresh(None), L).unwrap();
        let right = fb.map(body, input, Dest::Fresh(None), L).unwrap();
        fb.zip(left, right, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    (ir, f)
}

#[test]
fn path_diamond_has_independent_maps_and_dataflow_join() {
    let (ir, f) = diamond_path_fixture();
    let plan = ir.path_plan(f);
    let maps = func_ops(&ir, f, |op| matches!(op, Operation::Map { .. }));
    let zip = func_ops(&ir, f, |op| op == Operation::Zip)[0];
    assert_eq!(maps.len(), 2);

    let left = task_for(&plan, maps[0]);
    let right = task_for(&plan, maps[1]);
    let join = task_for(&plan, zip);
    assert_ne!(left, right);
    assert!(plan.tasks[left].deps.is_empty());
    assert!(plan.tasks[right].deps.is_empty());

    // Zip consumes its Pair-built product; that scalar glue task depends on
    // both independent map producers.
    assert_eq!(plan.tasks[join].deps.len(), 1);
    let glue = plan.tasks[join].deps[0];
    assert_eq!(plan.tasks[glue].deps, vec![left, right]);
    assert!(!plan.is_single_path());
}

#[test]
fn path_fold_and_independent_map_have_no_edge() {
    let mut b = IrBuilder::new();
    let map_body = identity_map_body(&mut b);
    let fold_body = add_fold_body(&mut b);
    let f = b
        .declare(FuncKind::Named, "fork", i32_arr(8), i32_arr(8), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let fold_input = fb.pack(&[zero, input], Dest::Fresh(None), L).unwrap();
        fb.fold(fold_body, fold_input, Dest::Fresh(None), L)
            .unwrap();
        fb.map(map_body, input, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.path_plan(f);
    let fold = task_for(
        &plan,
        func_ops(&ir, f, |op| matches!(op, Operation::Fold { .. }))[0],
    );
    let map = task_for(
        &plan,
        func_ops(&ir, f, |op| matches!(op, Operation::Map { .. }))[0],
    );

    assert!(matches!(
        plan.tasks[fold].kind,
        TaskKind::Seq { ref morphisms } if morphisms.len() == 1
    ));
    assert!(matches!(plan.tasks[map].kind, TaskKind::Split { n: 8, .. }));
    assert!(!plan.tasks[fold].deps.contains(&map));
    assert!(!plan.tasks[map].deps.contains(&fold));
}

#[test]
fn path_loop_scc_is_one_seq_task() {
    let (ir, f) = counting_loop();
    let plan = ir.path_plan(f);
    assert_eq!(plan.tasks.len(), 1);
    assert_eq!(
        plan.tasks[0].kind,
        TaskKind::Seq {
            morphisms: ir.topo_order(f)
        }
    );
    assert!(plan.is_single_path());
}

#[test]
fn path_effectful_loop_stays_entirely_on_host_spine() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "countdown", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let token = fb.input();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let init = fb.pack(&[three, token], Dest::Fresh(None), L).unwrap();
        let lh = fb.begin_loop(init, L).unwrap();
        let merge = fb.merge_of(&lh);
        let i = fb.proj(merge, 0, Dest::Fresh(None), L).unwrap();
        let token = fb.proj(merge, 1, Dest::Fresh(None), L).unwrap();
        let token = fb.print(token, i, L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let cond = fb
            .binop(Operation::Gt, i, zero, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let next_i = fb
            .binop(Operation::Sub, i, one, Dest::Fresh(None), L)
            .unwrap();
        let next = fb.pack(&[next_i, token], Dest::Fresh(None), L).unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        let token = fb
            .loop_exit(&lh, token, cond, Dest::Fresh(None), L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.output(token, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let print = func_ops(&ir, f, |op| matches!(op, Operation::Print { .. }))[0];
    let plan = ir.path_plan(f);

    assert!(plan.tasks.is_empty());
    assert_eq!(plan.checkpoints.len(), 2);
    assert_eq!(plan.checkpoints[0].topo, topo_pos(&ir, f, print));
    assert!(plan.checkpoints[0].wait.is_empty());
    assert_eq!(plan.checkpoints[1].topo, u32::MAX);
    assert!(plan.checkpoints[1].wait.is_empty());
}

#[test]
fn path_print_waits_for_value_producer_and_every_earlier_trap() {
    let mut b = IrBuilder::new();
    let body = identity_map_body(&mut b);
    let f = b
        .declare(
            FuncKind::Named,
            "print_mid",
            Ty::Tuple(vec![Ty::IoToken, Ty::i32()]),
            Ty::IoToken,
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        let token = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let unknown = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let eight = fb.constant(Value::I32(8), L).unwrap();
        let trap_array = fb.iota(eight, Dest::Fresh(None), L).unwrap();
        fb.index(trap_array, unknown, Dest::Fresh(None), L).unwrap();
        let value_array = fb.iota(eight, Dest::Fresh(None), L).unwrap();
        let mapped = fb.map(body, value_array, Dest::Fresh(None), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let value = fb.index(mapped, zero, Dest::Fresh(None), L).unwrap();
        let token = fb.print(token, value, L).unwrap();
        fb.output(token, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.path_plan(f);
    let indexes = func_ops(&ir, f, |op| op == Operation::Index);
    assert_eq!(indexes.len(), 2);
    let trap = task_for(&plan, indexes[0]);
    let producer = task_for(&plan, indexes[1]);
    assert!(plan.tasks[trap].trap_min.is_some());
    assert_eq!(plan.tasks[producer].trap_min, None);

    assert_eq!(plan.checkpoints.len(), 2);
    let print = &plan.checkpoints[0];
    let mut expected = vec![
        WaitEntry {
            task: trap,
            threshold: Some(topo_pos(&ir, f, indexes[0])),
        },
        WaitEntry {
            task: producer,
            threshold: None,
        },
    ];
    expected.sort_unstable_by_key(|entry| entry.task);
    assert_eq!(print.wait, expected);
    assert_eq!(
        plan.checkpoints[1].wait,
        (0..plan.tasks.len())
            .map(|task| WaitEntry {
                task,
                threshold: None
            })
            .collect::<Vec<_>>()
    );
    assert!(!plan.is_single_path());
}

fn constant_index_path(index: i32) -> (PathPlan, MorphismId, u32) {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "index", i32_arr(16), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let array = fb.input();
        let index = fb.constant(Value::I32(index), L).unwrap();
        fb.index(array, index, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let index = func_ops(&ir, f, |op| op == Operation::Index)[0];
    let topo = ir.topo_order(f).iter().position(|&m| m == index).unwrap() as u32;
    (ir.path_plan(f), index, topo)
}

#[test]
fn path_trap_min_exempts_only_bounds_proven_index() {
    let (proven, index, _) = constant_index_path(3);
    assert_eq!(proven.tasks[task_for(&proven, index)].trap_min, None);

    let (unproven, index, topo) = constant_index_path(20);
    assert_eq!(
        unproven.tasks[task_for(&unproven, index)].trap_min,
        Some(topo)
    );
}

#[test]
fn path_map_trap_min_uses_body_capability_at_site() {
    let mut b = IrBuilder::new();
    let trapping = b
        .declare(FuncKind::MapBody, "lookup", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(trapping).unwrap();
        let index = fb.input();
        let four = fb.constant(Value::I32(4), L).unwrap();
        let array = fb.iota(four, Dest::Fresh(None), L).unwrap();
        fb.index(array, index, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let safe = identity_map_body(&mut b);
    let f = b
        .declare(
            FuncKind::Named,
            "maps",
            Ty::Tuple(vec![Ty::IoToken, i32_arr(8)]),
            Ty::IoToken,
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        let token = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let array = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        fb.map(trapping, array, Dest::Fresh(None), L).unwrap();
        fb.map(safe, array, Dest::Fresh(None), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let token = fb.print(token, zero, L).unwrap();
        fb.output(token, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let maps = func_ops(&ir, f, |op| matches!(op, Operation::Map { .. }));
    let plan = ir.path_plan(f);

    let trapping_task_id = task_for(&plan, maps[0]);
    let trapping_task = &plan.tasks[trapping_task_id];
    let site_topo = topo_pos(&ir, f, maps[0]);
    assert_eq!(trapping_task.trap_min, Some(topo_pos(&ir, f, maps[0])));
    assert!(!trapping_task.pinned);
    assert_eq!(plan.tasks[task_for(&plan, maps[1])].trap_min, None);
    assert_eq!(
        plan.checkpoints[0].wait,
        vec![WaitEntry {
            task: trapping_task_id,
            threshold: Some(site_topo),
        }]
    );
}

#[test]
fn path_wait_uses_max_threshold_and_data_completion_wins() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "thresholds", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let token = fb.input();
        let lhs = fb.constant(Value::I32(1), L).unwrap();
        let rhs = fb.constant(Value::I32(0), L).unwrap();
        fb.binop(Operation::Div, lhs, rhs, Dest::Fresh(None), L)
            .unwrap();
        let second = fb
            .binop(Operation::Div, lhs, rhs, Dest::Fresh(None), L)
            .unwrap();
        let token = fb.print(token, rhs, L).unwrap();
        let token = fb.print(token, second, L).unwrap();
        fb.output(token, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.path_plan(f);
    let divs = func_ops(&ir, f, |op| op == Operation::Div);
    let task = task_for(&plan, divs[0]);
    assert_eq!(task_for(&plan, divs[1]), task);
    assert_eq!(
        plan.checkpoints
            .iter()
            .map(|checkpoint| checkpoint.wait.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![WaitEntry {
                task,
                threshold: Some(topo_pos(&ir, f, divs[1])),
            }],
            vec![WaitEntry {
                task,
                threshold: None,
            }],
            vec![WaitEntry {
                task,
                threshold: None,
            }],
        ]
    );
}

#[test]
fn path_fold_trap_min_uses_body_capability_at_site() {
    let mut b = IrBuilder::new();
    let body = b
        .declare(
            FuncKind::FoldBody,
            "divide",
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let input = fb.input();
        let acc = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let elem = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Div, acc, elem, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let f = b
        .declare(FuncKind::Named, "fold_div", i32_arr(8), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let input = fb.input();
        let fold_input = fb.pack(&[one, input], Dest::Fresh(None), L).unwrap();
        fb.fold(body, fold_input, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let fold = func_ops(&ir, f, |op| matches!(op, Operation::Fold { .. }))[0];
    let plan = ir.path_plan(f);

    let task = &plan.tasks[task_for(&plan, fold)];
    assert_eq!(task.trap_min, Some(topo_pos(&ir, f, fold)));
    assert!(!task.pinned);
}

fn call_trap_path(transitive: bool) -> (PathPlan, MorphismId, u32) {
    let mut b = IrBuilder::new();
    let args = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let leaf = b
        .declare(FuncKind::Named, "divide", args.clone(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(leaf).unwrap();
        let input = fb.input();
        let lhs = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let rhs = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Div, lhs, rhs, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let callee = if transitive {
        let middle = b
            .declare(FuncKind::Named, "middle", args.clone(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(middle).unwrap();
            let input = fb.input();
            fb.call(leaf, input, Dest::Ret { slot: None }, L).unwrap();
            fb.finish().unwrap();
        }
        middle
    } else {
        leaf
    };
    let f = b
        .declare(FuncKind::Named, "caller", args, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        fb.call(callee, input, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let call = func_ops(&ir, f, |op| matches!(op, Operation::Call(_)))[0];
    let topo = topo_pos(&ir, f, call);
    (ir.path_plan(f), call, topo)
}

#[test]
fn path_trap_capable_calls_pin_direct_and_transitive_tasks() {
    for transitive in [false, true] {
        let (plan, call, topo) = call_trap_path(transitive);
        let task = &plan.tasks[task_for(&plan, call)];
        assert!(task.pinned);
        assert_eq!(task.trap_min, Some(topo));
    }
}

#[test]
fn path_rank_prefers_heavy_bulk_long_path_to_light_scalar_chain() {
    let mut b = IrBuilder::new();
    let body = identity_map_body(&mut b);
    let f = b
        .declare(FuncKind::Named, "rank", Ty::Unit, i32_arr(128), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let mut scalar = one;
        for _ in 0..8 {
            scalar = fb
                .binop(Operation::Add, scalar, one, Dest::Fresh(None), L)
                .unwrap();
        }
        let n = fb.constant(Value::I32(128), L).unwrap();
        let range = fb.iota(n, Dest::Fresh(None), L).unwrap();
        fb.map(body, range, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.path_plan(f);
    let scalar = task_for(&plan, func_ops(&ir, f, |op| op == Operation::Add)[0]);
    let heavy = task_for(&plan, func_ops(&ir, f, |op| op == Operation::Iota)[0]);
    assert!(plan.tasks[heavy].rank > plan.tasks[scalar].rank);
}

#[test]
fn path_plan_is_deterministic() {
    let (ir, f) = diamond_path_fixture();
    assert_eq!(ir.path_plan(f), ir.path_plan(f));
}

#[test]
fn emission_nested_product_dissolution_is_pair_built_only() {
    // inner = (x, 1); outer = (inner, 1); back = outer.0; ret back.1
    // outer: Pair-built, only-Proj consumers -> Dissolved.
    // inner: its consumer is a Pair edge (a field of outer) -> never
    // Dissolved; one effective consumer through the dissolved outer ->
    // product-typed Inline (the compound-literal case WP-B must handle).
    // back: Proj-PRODUCED tuple -> not Pair-built -> never Dissolved even
    // with only-Proj consumers; single consumer -> Inline.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "nested", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let inner;
    let outer;
    let back;
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        inner = fb.pack(&[x, one], Dest::Fresh(None), L).unwrap();
        outer = fb.pack(&[inner, one], Dest::Fresh(None), L).unwrap();
        back = fb.proj(outer, 0, Dest::Fresh(None), L).unwrap();
        fb.proj(back, 1, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.emission_plan(f);
    assert!(
        plan.class(outer).is_some_and(|c| c.is_dissolved()),
        "outer: Pair-built + only-Proj -> Dissolved"
    );
    assert!(
        plan.class(inner).is_some_and(|c| c.is_inline()),
        "inner: Pair-edge consumer blocks dissolution; effective count 1"
    );
    assert!(
        plan.class(back).is_some_and(|c| c.is_inline()),
        "back: Proj-produced, not dissolvable; single consumer -> Inline"
    );
}

#[test]
fn emission_proj_produced_tuple_fanout_is_named_not_dropped() {
    // The R-NODUP regression for the silent-count-drop bug: a Proj-produced
    // tuple read by TWO Projs must be Named. If it dissolved, its consumers
    // would vanish from the counts (pair_slot_source = None on a Proj-built
    // product), the shared computed field chain would classify Inline, and
    // the emitter would duplicate its text.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "projfan", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let s;
    let back;
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        s = fb
            .binop(Operation::Add, x, one, Dest::Fresh(None), L)
            .unwrap();
        let inner = fb.pack(&[s, one], Dest::Fresh(None), L).unwrap();
        let outer = fb.pack(&[inner, one], Dest::Fresh(None), L).unwrap();
        back = fb.proj(outer, 0, Dest::Fresh(None), L).unwrap();
        let r1 = fb.proj(back, 0, Dest::Fresh(None), L).unwrap();
        let r2 = fb.proj(back, 1, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Add, r1, r2, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.emission_plan(f);
    assert!(
        plan.class(back).is_some_and(|c| c.is_named()),
        "Proj-produced tuple with 2 consumers must be Named (count intact)"
    );
    assert!(
        plan.class(s).is_some_and(|c| c.is_inline()),
        "the computed field chain stays single-reference"
    );
}

#[test]
fn emission_pure_chain_dissolves_products_and_inlines_values() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "chain", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let input;
    let one;
    let mut intermediates = Vec::new();
    {
        let mut fb = b.build_fn(f).unwrap();
        input = fb.input();
        one = fb.constant(Value::I32(1), L).unwrap();
        let mut value = input;
        for i in 0..4 {
            value = fb
                .binop(
                    Operation::Add,
                    value,
                    one,
                    if i == 3 {
                        Dest::Ret { slot: None }
                    } else {
                        Dest::Fresh(None)
                    },
                    L,
                )
                .unwrap();
            if i != 3 {
                intermediates.push(value);
            }
        }
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.emission_plan(f);
    let products: Vec<_> = ir
        .objects()
        .filter(|(o, obj)| ir.owner(*o) == f && obj.ty.product_arity().is_some())
        .map(|(o, _)| o)
        .collect();

    assert_eq!(products.len(), 4, "one internal product per binary op");
    assert!(
        products
            .iter()
            .all(|&o| plan.class(o).is_some_and(|c| c.is_dissolved()))
    );
    assert!(plan.class(input).is_some_and(|c| c.is_inline()));
    assert!(
        intermediates
            .iter()
            .all(|&o| plan.class(o).is_some_and(|c| c.is_inline()))
    );
    assert!(
        plan.class(ir.func(f).unwrap().output)
            .is_some_and(|c| c.is_named())
    );
    assert!(plan.class(one).is_none(), "constants never materialize");
}

#[test]
fn emission_fanout_has_one_named_split_point() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "fanout", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let shared;
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        shared = fb
            .binop(Operation::Add, fb.input(), one, Dest::Fresh(None), L)
            .unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let left = fb
            .binop(Operation::Mul, shared, two, Dest::Fresh(None), L)
            .unwrap();
        let right = fb
            .binop(Operation::Sub, shared, three, Dest::Fresh(None), L)
            .unwrap();
        fb.binop(Operation::Add, left, right, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.emission_plan(f);
    let output = ir.func(f).unwrap().output;
    let named_non_output: Vec<_> = ir
        .objects()
        .filter(|(o, obj)| {
            ir.owner(*o) == f
                && *o != output
                && obj.kind != flow_ir::ObjectKind::Constant
                && plan.class(*o).is_some_and(|c| c.is_named())
        })
        .map(|(o, _)| o)
        .collect();

    assert_eq!(named_non_output, vec![shared]);
    assert_eq!(
        ir.out_edges(shared)
            .iter()
            .filter(|&&m| matches!(ir.morphism(m).unwrap().op, Operation::Pair { .. }))
            .count(),
        2,
        "both consumer products reference the one split value"
    );
}

#[test]
fn emission_div_guard_classification_uses_divisor_facts() {
    let mut b = IrBuilder::new();
    let unsafe_f = b
        .declare(FuncKind::Named, "unsafe_div", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let safe_f = b
        .declare(FuncKind::Named, "safe_div", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let neg_one_f = b
        .declare(FuncKind::Named, "neg_one_div", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let float_f = b
        .declare(FuncKind::Named, "float_div", Ty::f32(), Ty::f32(), L)
        .unwrap();
    let unsafe_div;
    let safe_div;
    let neg_one_div;
    let float_div;
    {
        let mut fb = b.build_fn(unsafe_f).unwrap();
        let twelve = fb.constant(Value::I32(12), L).unwrap();
        unsafe_div = fb
            .binop(Operation::Div, twelve, fb.input(), Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, unsafe_div, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    {
        let mut fb = b.build_fn(safe_f).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        safe_div = fb
            .binop(Operation::Div, fb.input(), two, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, safe_div, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    {
        let mut fb = b.build_fn(neg_one_f).unwrap();
        let minus_one = fb.constant(Value::I32(-1), L).unwrap();
        neg_one_div = fb
            .binop(Operation::Div, fb.input(), minus_one, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(
            Operation::Add,
            neg_one_div,
            one,
            Dest::Ret { slot: None },
            L,
        )
        .unwrap();
        fb.finish().unwrap();
    }
    {
        let mut fb = b.build_fn(float_f).unwrap();
        let two = fb.constant(Value::F32(2.0), L).unwrap();
        float_div = fb
            .binop(Operation::Div, fb.input(), two, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::F32(1.0), L).unwrap();
        fb.binop(Operation::Add, float_div, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(unsafe_f).unwrap();

    assert!(
        ir.emission_plan(unsafe_f)
            .class(unsafe_div)
            .is_some_and(|c| c.is_named())
    );
    assert!(
        ir.emission_plan(safe_f)
            .class(safe_div)
            .is_some_and(|c| c.is_inline())
    );
    assert!(
        ir.emission_plan(neg_one_f)
            .class(neg_one_div)
            .is_some_and(|c| c.is_named())
    );
    assert!(
        ir.emission_plan(float_f)
            .class(float_div)
            .is_some_and(|c| c.is_inline())
    );
}

#[test]
fn emission_index_guard_classification_uses_bounds_proof() {
    let array_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let mut b = IrBuilder::new();
    let f = b
        .declare(
            FuncKind::Named,
            "indexes",
            Ty::Tuple(vec![array_ty, Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    let proven;
    let unproven;
    {
        let mut fb = b.build_fn(f).unwrap();
        let input = fb.input();
        let array = fb.proj(input, 0, Dest::Fresh(None), L).unwrap();
        let dynamic = fb.proj(input, 1, Dest::Fresh(None), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        proven = fb.index(array, one, Dest::Fresh(None), L).unwrap();
        unproven = fb.index(array, dynamic, Dest::Fresh(None), L).unwrap();
        fb.binop(
            Operation::Add,
            proven,
            unproven,
            Dest::Ret { slot: None },
            L,
        )
        .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let plan = ir.emission_plan(f);

    assert!(plan.class(proven).is_some_and(|c| c.is_inline()));
    assert!(plan.class(unproven).is_some_and(|c| c.is_named()));
}

#[test]
fn emission_loop_merge_routes_and_cones_are_named() {
    let (ir, f) = counting_loop();
    let merge = ir.loop_structure(f)[0].merges[0];
    let loop_plan = ir.loop_plan(f, merge).unwrap();
    let plan = ir.emission_plan(f);
    let mut expected = vec![loop_plan.merge, loop_plan.back_route, loop_plan.exit_route];
    for &m in loop_plan
        .decide_order
        .iter()
        .chain(&loop_plan.advance_order)
        .chain(&loop_plan.exits)
    {
        let morph = ir.morphism(m).unwrap();
        expected.push(morph.source);
        expected.push(morph.target);
    }
    for o in expected {
        let obj = ir.object(o).unwrap();
        if obj.kind != flow_ir::ObjectKind::Constant && !flow_ir::ty_contains_token(&obj.ty) {
            assert!(
                plan.class(o).is_some_and(|c| c.is_named()),
                "loop object {o:?} must be Named"
            );
        }
    }
}

#[test]
fn emission_call_argument_product_and_array_literal_are_named() {
    let mut b = IrBuilder::new();
    let callee = b
        .declare(
            FuncKind::Named,
            "callee",
            Ty::Tuple(vec![Ty::i32(), Ty::i32()]),
            Ty::i32(),
            L,
        )
        .unwrap();
    let caller = b
        .declare(FuncKind::Named, "caller", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(callee).unwrap();
        fb.proj(fb.input(), 0, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let call_arg;
    let array;
    {
        let mut fb = b.build_fn(caller).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let two = fb.constant(Value::I32(2), L).unwrap();
        call_arg = fb.pack(&[one, two], Dest::Fresh(None), L).unwrap();
        array = fb.pack_array(&[one, two], Dest::Fresh(None), L).unwrap();
        fb.call(callee, call_arg, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(caller).unwrap();
    let plan = ir.emission_plan(caller);

    assert!(plan.class(call_arg).is_some_and(|c| c.is_named()));
    assert!(plan.class(array).is_some_and(|c| c.is_named()));
}

fn emission_effective_counts(
    ir: &flow_ir::CategoryIr,
    f: flow_ir::FuncId,
) -> Vec<(flow_ir::ObjectId, u32)> {
    let plan = ir.emission_plan(f);
    let mut counts: Vec<_> = ir
        .objects()
        .filter(|(o, obj)| {
            ir.owner(*o) == f
                && obj.kind != flow_ir::ObjectKind::Constant
                && !flow_ir::ty_contains_token(&obj.ty)
                && !plan.class(*o).is_some_and(|c| c.is_dissolved())
        })
        .map(|(o, _)| (o, 0))
        .collect();
    let increment = |counts: &mut Vec<(flow_ir::ObjectId, u32)>, o| {
        if let Some((_, count)) = counts.iter_mut().find(|(id, _)| *id == o) {
            *count += 1;
        }
    };

    for (o, _) in &counts.clone() {
        for &m in ir.out_edges(*o) {
            let morph = ir.morphism(m).unwrap();
            let transparent_pair = matches!(morph.op, Operation::Pair { .. })
                && plan.class(morph.target).is_some_and(|c| c.is_dissolved());
            if !transparent_pair {
                increment(&mut counts, *o);
            }
        }
    }
    for (product, _) in ir
        .objects()
        .filter(|(o, _)| ir.owner(*o) == f && plan.class(*o).is_some_and(|c| c.is_dissolved()))
    {
        for &m in ir.out_edges(product) {
            match ir.morphism(m).unwrap().op {
                Operation::Proj { index } => {
                    if let Some(source) = ir.in_edges(product).iter().find_map(|&pair| {
                        let morph = ir.morphism(pair).unwrap();
                        matches!(morph.op, Operation::Pair { slot, .. } if slot == index)
                            .then_some(morph.source)
                    }) {
                        increment(&mut counts, source);
                    }
                }
                _ => {
                    for &pair in ir.in_edges(product) {
                        let morph = ir.morphism(pair).unwrap();
                        if matches!(morph.op, Operation::Pair { .. }) {
                            increment(&mut counts, morph.source);
                        }
                    }
                }
            }
        }
    }
    counts
}

fn emission_boundaries(ir: &flow_ir::CategoryIr, f: flow_ir::FuncId) -> Vec<flow_ir::ObjectId> {
    let mut boundary = vec![ir.func(f).unwrap().output];
    let mut add = |o| {
        if !boundary.contains(&o) {
            boundary.push(o);
        }
    };
    for (o, obj) in ir.objects().filter(|(o, _)| ir.owner(*o) == f) {
        if obj.kind == flow_ir::ObjectKind::LoopMerge || matches!(obj.ty, Ty::Array { .. }) {
            add(o);
        }
    }

    let bounds = ir.bounds_proof(f);
    for (m, morph) in ir.morphisms().filter(|(_, m)| ir.owner(m.source) == f) {
        let guarded = match morph.op {
            Operation::Div | Operation::Mod => {
                !matches!(ir.object(morph.target).unwrap().ty, Ty::Float { .. })
                    && ir
                        .in_edges(morph.source)
                        .iter()
                        .find_map(|&pair| {
                            let pair = ir.morphism(pair).unwrap();
                            matches!(pair.op, Operation::Pair { slot: 1, .. })
                                .then_some(pair.source)
                        })
                        .is_none_or(|o| {
                            let divisor = ir.object(o).unwrap();
                            divisor.kind != flow_ir::ObjectKind::Constant
                                || !match divisor.value.as_ref() {
                                    Some(Value::I32(v)) => *v != 0 && *v != -1,
                                    Some(Value::I64(v)) => *v != 0 && *v != -1,
                                    Some(Value::U8(v)) => *v != 0,
                                    _ => false,
                                }
                        })
            }
            Operation::Index | Operation::Update => !bounds.proven(m),
            _ => false,
        };
        if guarded {
            add(morph.target);
        }
        match morph.op {
            Operation::Output => {
                add(morph.source);
                add(morph.target);
            }
            Operation::Call(_) => add(morph.source),
            Operation::Map { .. }
            | Operation::Fold { .. }
            | Operation::Zip
            | Operation::Enumerate
            | Operation::Update
            | Operation::Iota
            | Operation::Fill
                if ir
                    .object(morph.source)
                    .unwrap()
                    .ty
                    .product_arity()
                    .is_some() =>
            {
                add(morph.source);
            }
            Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
                add(morph.source);
                add(morph.target);
            }
            _ => {}
        }
    }
    for scc in ir.loop_structure(f) {
        for merge in scc.merges {
            if let Some(plan) = ir.loop_plan(f, merge) {
                for &m in plan.decide_order.iter().chain(&plan.advance_order) {
                    let morph = ir.morphism(m).unwrap();
                    add(morph.source);
                    add(morph.target);
                }
            }
        }
    }
    boundary
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn emission_plan_totality_and_laws(
        ops in prop::collection::vec((0u8..5, any::<usize>(), any::<usize>()), 1..24)
    ) {
        let mut b = IrBuilder::new();
        let f = b.declare(FuncKind::Named, "generated", Ty::i32(), Ty::i32(), L).unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let mut pool = vec![
                fb.input(),
                fb.constant(Value::I32(0), L).unwrap(),
                fb.constant(Value::I32(1), L).unwrap(),
                fb.constant(Value::I32(-1), L).unwrap(),
                fb.constant(Value::I32(2), L).unwrap(),
            ];
            for (kind, a, c) in ops {
                let lhs = pool[a % pool.len()];
                let rhs = pool[c % pool.len()];
                let op = match kind {
                    0 => Operation::Add,
                    1 => Operation::Sub,
                    2 => Operation::Mul,
                    3 => Operation::Div,
                    _ => Operation::Mod,
                };
                pool.push(fb.binop(op, lhs, rhs, Dest::Fresh(None), L).unwrap());
            }
            fb.output(*pool.last().unwrap(), None, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let plan = ir.emission_plan(f);
        prop_assert_eq!(&plan, &ir.emission_plan(f), "R-DET");
        let counts = emission_effective_counts(&ir, f);
        let boundaries = emission_boundaries(&ir, f);

        for (o, obj) in ir.objects().filter(|(o, _)| ir.owner(*o) == f) {
            if obj.kind == flow_ir::ObjectKind::Constant || flow_ir::ty_contains_token(&obj.ty) {
                prop_assert!(plan.class(o).is_none());
                continue;
            }
            let class = plan.class(o).expect("classification is total");
            if class.is_inline() {
                let count = counts.iter().find(|(id, _)| *id == o).unwrap().1;
                prop_assert_eq!(count, 1, "R-NODUP for {:?}", o);
            }
            if class.is_dissolved() {
                prop_assert!(obj.ty.product_arity().is_some());
                prop_assert!(!boundaries.contains(&o), "boundary {:?}", o);
            }
            if boundaries.contains(&o) {
                prop_assert!(class.is_named(), "boundary {o:?} must be Named");
            }
        }
    }
}
