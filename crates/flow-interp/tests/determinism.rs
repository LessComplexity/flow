//! Determinism (E2; interp DESIGN §11.6).
//!
//! Build + run each example twice ⇒ byte-identical `output` and identical
//! `outcome`. No `HashMap` anywhere in the interpreter makes this structural;
//! this test pins it.

use flow_interp::run;
use flow_ir::CategoryIr;

const BUDGET: u64 = 100_000;

fn example(name: &str) -> String {
    let path = format!(
        "{}/../../examples/{}.flow",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn build(src: &str) -> CategoryIr {
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty());
    flow_lower::lower(src, &po.program).unwrap()
}

fn assert_twice_identical(name: &str) {
    let ir = build(&example(name));
    let r1 = run(&ir, BUDGET);
    let r2 = run(&ir, BUDGET);
    assert_eq!(r1.output, r2.output, "{name}: output differs across runs");
    assert_eq!(
        r1.outcome, r2.outcome,
        "{name}: outcome differs across runs"
    );
}

#[test]
fn deterministic_abs() {
    assert_twice_identical("abs");
}

#[test]
fn deterministic_sum_to_n() {
    assert_twice_identical("sum_to_n");
}

#[test]
fn deterministic_pipeline() {
    assert_twice_identical("pipeline");
}

#[test]
fn deterministic_fanout() {
    assert_twice_identical("fanout");
}

#[test]
fn deterministic_fir() {
    assert_twice_identical("fir");
}

#[test]
fn deterministic_sepia() {
    assert_twice_identical("sepia");
}
