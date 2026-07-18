//! DESIGN §8.1 — the identity-replay soundness anchor.
//!
//! For each of the 10 in-Core examples (all `examples/*.flow` except
//! `vector.flow`, the out-of-Core generics sketch): `parse → lower →
//! replay(ir, ∅)` and assert
//!   - `validate(&out).is_empty()`  (R2),
//!   - interp `RunResult` byte-equal vs the original (R1, budget 100_000),
//!   - `to_mermaid` lint-clean (`lint_mermaid`).
//!
//! Plus RW8: a hand-built multi-merge nested loop makes `replay` decline with
//! `NonCanonicalLoop`.

use flow_interp::{RunResult, run};
use flow_ir::{
    CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, lint_mermaid, validate,
};
use flow_rewrite::{ReplayError, RewritePlan, is_canonical, replay, rewrite};

const BUDGET: u64 = 100_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// The 10 in-Core examples (every `examples/*.flow` except `vector.flow`).
const EXAMPLES: &[&str] = &[
    "abs",
    "calc",
    "fanout",
    "fir",
    "pipeline",
    "sepia",
    "seq_demo",
    "sum_to_n",
    "vector_add",
    "zip_demo",
];

/// Parse + lower one `examples/*.flow` into a sealed IR (asserting clean).
fn build(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let po = flow_syntax::parse(&src);
    assert!(
        po.diagnostics.is_empty(),
        "{name}: parse diagnostics: {:?}",
        po.diagnostics
    );
    flow_lower::lower(&src, &po.program).unwrap_or_else(|ds| panic!("{name}: lower failed: {ds:?}"))
}

#[test]
fn identity_replay_matches_every_example() {
    for &name in EXAMPLES {
        let ir = build(name);
        assert!(is_canonical(&ir), "{name}: expected canonical loops");

        let before: RunResult = run(&ir, BUDGET);

        let out = replay(&ir, &RewritePlan::new())
            .unwrap_or_else(|e| panic!("{name}: replay failed: {e:?}"));

        // R2 — validate-clean by construction.
        let v = validate(&out);
        assert!(v.is_empty(), "{name}: validate found {v:?}");

        // R1 — interp RunResult byte-equal.
        let after: RunResult = run(&out, BUDGET);
        assert_eq!(
            after, before,
            "{name}: interp RunResult diverged under replay"
        );

        // Mermaid lint-clean.
        let lints = lint_mermaid(&out.to_mermaid());
        assert!(lints.is_empty(), "{name}: mermaid lints {lints:?}");
    }
}

/// ADR-0021 U1: an `Update`-bearing graph survives the optimizer as an identity
/// on observable behavior — `rewrite` returns a validate-clean graph whose interp
/// `RunResult` is byte-equal to the original (the op is may-trap-kept, no L-a
/// operand to fold here). Built via `IrBuilder` (lower sugar is WP U3).
#[test]
fn update_graph_round_trips_through_rewrite() {
    let mut b = IrBuilder::new();
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: 4,
    };
    let f = b
        .declare(FuncKind::Named, "main", arr_ty.clone(), arr_ty, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let a = fb.input();
        let i = fb.constant(Value::I32(2), L).unwrap();
        let v = fb.constant(Value::I32(99), L).unwrap();
        fb.update(a, i, v, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());

    let arg = flow_interp::RValue::Array(
        (0..4)
            .map(|n| flow_interp::RValue::Scalar(Value::I32(n)))
            .collect(),
    );
    let before = flow_interp::eval_call(&ir, f, arg.clone(), BUDGET);

    let res = rewrite(ir);
    assert!(
        validate(&res.ir).is_empty(),
        "rewrite output validate-clean"
    );
    let after = flow_interp::eval_call(&res.ir, res.ir.entry(), arg, BUDGET);
    assert_eq!(
        after, before,
        "Update-bearing graph must round-trip interp-equal"
    );
    // Sanity: the write landed (slot 2 replaced), proving replay rebuilt the op.
    assert_eq!(
        after,
        flow_interp::Outcome::Done(flow_interp::RValue::Array(vec![
            flow_interp::RValue::Scalar(Value::I32(0)),
            flow_interp::RValue::Scalar(Value::I32(1)),
            flow_interp::RValue::Scalar(Value::I32(99)),
            flow_interp::RValue::Scalar(Value::I32(3)),
        ]))
    );
}

/// A multi-merge nested loop (two loops cross-fed into one SCC) — not the
/// canonical quartet. Shape copied from
/// `crates/flow-ir/tests/algos.rs::nested_loops_multi_merge_one_scc`.
fn multi_merge_nested_loop() -> CategoryIr {
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
        // Cross-feed: inner next reads the OUTER merge; outer next reads the INNER.
        let inext = fb
            .binop(Operation::Add, om, im, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        fb.loop_back(&inner, inext, ct, L).unwrap();
        let onext = fb
            .binop(Operation::Add, im, om, Dest::Fresh(Some("onext".into())), L)
            .unwrap();
        fb.loop_back(&outer, onext, ct, L).unwrap();
        fb.loop_exit(&inner, im, cf, Dest::Fresh(Some("iout".into())), L)
            .unwrap();
        fb.end_loop(inner).unwrap();
        fb.loop_exit(&outer, om, cf, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(outer).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f)
        .expect("fused nested loops seal (legal-but-degenerate)")
}

/// RW8 — a multi-merge nested loop is not the canonical quartet; `replay`
/// declines with `NonCanonicalLoop`.
#[test]
fn multi_merge_nested_loop_is_non_canonical() {
    let ir = multi_merge_nested_loop();
    assert!(
        !is_canonical(&ir),
        "multi-merge SCC must read non-canonical"
    );
    assert!(matches!(
        replay(&ir, &RewritePlan::new()),
        Err(ReplayError::NonCanonicalLoop)
    ));
}

/// RW8 driver path — `rewrite` on a non-canonical loop is the whole-graph
/// identity: `skipped_non_canonical`, no rounds, no applications, and a
/// byte-identical (`to_mermaid`) graph handed back untouched.
#[test]
fn multi_merge_nested_loop_rewrite_is_identity() {
    let ir = multi_merge_nested_loop();
    let before = ir.to_mermaid();

    let res = rewrite(ir);
    assert!(
        res.report.skipped_non_canonical,
        "RW8: non-canonical loop forces the whole-graph identity"
    );
    assert_eq!(res.report.rounds, 0, "no fixpoint rounds run");
    assert!(res.report.applied.is_empty(), "no pass applies");
    assert_eq!(res.ir.to_mermaid(), before, "graph handed back untouched");
}

/// S12 regression: two sequential canonical loops in one fn are lower-reachable
/// and now rewritable — exit attribution is by route-feeder membership in the
/// specific merge's SCC (the interp driver's rule), never by reachability, which
/// falsely attributed the second loop's exit to the first merge and forced the
/// whole-graph identity path.
#[test]
fn two_sequential_loops_rewrite_not_skipped() {
    let src = r#"
fn f(n: i32) -> i32 {
    mut i: i32 <- 0;
    mut a: i32 <- 0;
    loop {
        (i < n) -> {
            -true-> { a + 2 -> a; i + 1 -> i; -> loop; }
            -false-> a -> aa;
        }
    }
    mut j: i32 <- 0;
    mut b: i32 <- 0;
    loop {
        (j < n) -> {
            -true-> { b + 3 -> b; j + 1 -> j; -> loop; }
            -false-> b -> bb;
        }
    }
    aa + bb -> ret;
}

fn main() {
    4 -> f -> r;
    r -> println;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "{:?}", po.diagnostics);
    let ir = flow_lower::lower(src, &po.program).expect("lowers clean");
    assert!(is_canonical(&ir), "two canonical loops are canonical");
    let before = run(&ir, BUDGET);
    assert_eq!(before.output, "20\n");

    let res = rewrite(ir);
    assert!(
        !res.report.skipped_non_canonical,
        "two-loop fn must not take the identity path"
    );
    assert!(validate(&res.ir).is_empty());
    let after = run(&res.ir, BUDGET);
    assert_eq!(after.output, before.output);
    assert_eq!(after.outcome, before.outcome);
}
