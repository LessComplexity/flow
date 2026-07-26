//! The shared differential generator draws every ADR-0029 widening edge.

mod testgen;

use mapal_ir::{Operation, Ty, validate};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{build, prog_strategy};

#[test]
fn random_programs_cover_widen_lattice() {
    let mut runner = TestRunner::deterministic();
    let mut seen = [false; 4];
    for (count, trap_free) in [(128, false), (64, true)] {
        let strat = prog_strategy(trap_free, false);
        for _ in 0..count {
            let prog = strat.new_tree(&mut runner).unwrap().current();
            let ir = build(&prog).ir;
            assert!(validate(&ir).is_empty());
            for (_, m) in ir.morphisms().filter(|(_, m)| m.op == Operation::Widen) {
                let source = &ir.object(m.source).unwrap().ty;
                let target = &ir.object(m.target).unwrap().ty;
                match (source, target) {
                    (s, t) if *s == Ty::i32() && *t == Ty::i64() => seen[0] = true,
                    (s, t) if *s == Ty::i32() && *t == Ty::f32() => seen[1] = true,
                    (s, t) if *s == Ty::i32() && *t == Ty::f64() => seen[2] = true,
                    (s, t) if *s == Ty::f32() && *t == Ty::f64() => seen[3] = true,
                    pair => panic!("generator emitted illegal Widen pair: {pair:?}"),
                }
            }
        }
    }
    assert!(
        seen.iter().all(|x| *x),
        "missing Widen lattice edges: {seen:?}"
    );
}
