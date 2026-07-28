//! plan-s39: the structural invariants `guard_plan`'s arm ownership must hold,
//! checked over the closed testgen corpus.
//!
//! The load-bearing one is CONSUMER CLOSURE (`own_is_consumer_closed`). An
//! earlier implementation derived ownership from *liveness* — "removing this
//! arm's edge makes the object dead" — which is unsound here, because nothing
//! deletes dead code before execution: the interpreter walks every morphism in
//! topo order. A `Proj` feeding both an arm and a *dead* `Neg` passed the
//! liveness test, got gated, and the dead `Neg` then read an object the
//! unchosen arm never wrote (`read before write`, testgen case #94).

#[path = "testgen/mod.rs"]
mod testgen;

use std::collections::HashSet;

use mapal_ir::{CategoryIr, FuncId, MorphismId, ObjectId, Operation};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{Built, build, prog_strategy};

/// The loop unit owned through a `LoopEnter` handle whose target is `merge`
/// (plan-s40): SCC-incident morphisms plus the `loop_plan` cones — exactly the
/// region the driver fires, reconstructed from the public queries.
fn loop_unit_members(ir: &CategoryIr, f: FuncId, merge: ObjectId) -> HashSet<MorphismId> {
    let mut members = HashSet::new();
    for scc in ir.loop_structure(f) {
        if !scc.merges.contains(&merge) {
            continue;
        }
        let objs: HashSet<ObjectId> = scc.objects.iter().copied().collect();
        for &m in &ir.topo_order(f) {
            let morph = ir.morphism(m).unwrap();
            if objs.contains(&morph.source) || objs.contains(&morph.target) {
                members.insert(m);
            }
        }
        for &mg in &scc.merges {
            if let Some(plan) = ir.loop_plan(f, mg) {
                members.extend(plan.decide_order.iter().copied());
                members.extend(plan.advance_order.iter().copied());
                members.extend(plan.exits.iter().copied());
            }
        }
    }
    members
}

/// Every closed testgen program, built.
fn corpus() -> Vec<mapal_ir::CategoryIr> {
    let mut runner = TestRunner::deterministic();
    let mut out = Vec::new();
    for (count, trap_free) in [(256usize, false), (64usize, true)] {
        let strat = prog_strategy(trap_free, false);
        for _ in 0..count {
            let prog = strat.new_tree(&mut runner).unwrap().current();
            let Built { ir, open, .. } = build(&prog);
            if !open {
                out.push(ir);
            }
        }
    }
    out
}

#[test]
fn guard_arm_ownership_invariants() {
    let mut sites = 0usize;
    for ir in corpus() {
        for (f, _) in ir.funcs() {
            let plan = ir.guard_plan(f);
            let topo = ir.topo_order(f);
            let pos = |m: MorphismId| topo.iter().position(|&x| x == m);
            for s in &plan {
                sites += 1;
                let owned: HashSet<MorphismId> = s
                    .on_true
                    .own
                    .iter()
                    .chain(s.on_false.own.iter())
                    .copied()
                    .collect();

                for (tag, arm) in [("true", &s.on_true), ("false", &s.on_false)] {
                    let arm_owned: HashSet<MorphismId> = arm.own.iter().copied().collect();

                    // plan-s40: a LoopEnter handle stands for its whole unit —
                    // consumers inside it are fired by the driver, under the
                    // gate. Internals must never appear in an own-list.
                    let mut unit_members: HashSet<MorphismId> = HashSet::new();
                    let mut has_handle = false;
                    for &m in &arm.own {
                        let morph = ir.morphism(m).unwrap();
                        assert!(
                            !matches!(morph.op, Operation::LoopBack | Operation::LoopExit),
                            "{tag} arm owns loop-internal machinery {m:?} ({:?})",
                            morph.op
                        );
                        if matches!(morph.op, Operation::LoopEnter) {
                            has_handle = true;
                            unit_members.extend(loop_unit_members(&ir, f, morph.target));
                        }
                    }
                    if has_handle {
                        assert!(arm.heavy, "{tag} arm owns a loop but is not heavy");
                        for &m in &arm.own {
                            assert!(
                                !unit_members.contains(&m)
                                    || matches!(ir.morphism(m).unwrap().op, Operation::LoopEnter),
                                "{tag} arm owns unit-internal {m:?} — only the \
                                 handle may stand for the unit"
                            );
                        }
                    }

                    // CONSUMER CLOSURE: every consumer of an owned morphism's
                    // target is owned too — except the boundary edge, whose
                    // target is the shared triple, and consumers inside an
                    // owned loop unit, which the driver fires (plan-s40).
                    for &m in &arm.own {
                        if m == arm.edge {
                            continue;
                        }
                        let t = ir.morphism(m).unwrap().target;
                        for &c in ir.out_edges(t) {
                            assert!(
                                arm_owned.contains(&c) || unit_members.contains(&c),
                                "{tag} arm owns {m:?} but its target {t:?} has \
                                 un-owned consumer {c:?} ({:?}) — it would read an \
                                 object the unchosen arm never wrote",
                                ir.morphism(c).unwrap().op
                            );
                        }
                    }

                    // The boundary edge is last, and the list is topo-ordered.
                    assert_eq!(*arm.own.last().unwrap(), arm.edge);
                    for w in arm.own.windows(2) {
                        assert!(pos(w[0]) < pos(w[1]), "own list not in topo order");
                    }
                }

                // Arms are disjoint, and the condition's producer is unowned —
                // the condition must be computable before either arm runs.
                for m in &s.on_true.own {
                    assert!(!s.on_false.own.contains(m), "arms share {m:?}");
                }
                for &c in ir.in_edges(s.cond) {
                    assert!(!owned.contains(&c), "condition producer {c:?} is arm-owned");
                }
            }
        }
    }
    assert!(
        sites > 0,
        "corpus produced no guard sites — the test is vacuous"
    );
    eprintln!("checked {sites} guard sites");
}
