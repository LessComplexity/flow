//! S21: replay faithfulness — the iota/fill+zip shape must reach a rewrite
//! fixpoint. The box-differential P0: CSE aliased the two structurally-equal
//! `(7, 4)` fill-tuples, but replay's `fill(x, n)` sugar re-minted a fresh
//! internal tuple every round, resurrecting the eliminated duplicate —
//! `MAX_ROUNDS hit without a fixpoint` (driver.rs). Fixed by the `fill_from`
//! replay entry (emits the `Fill` edge from the existing tuple, mints nothing).

use flow_interp::run;
use flow_rewrite::rewrite;

const BUDGET: u64 = 10_000_000;

fn build() -> flow_ir::CategoryIr {
    let src = r#"
fn main() {
    4 -> iota -> a;
    (7, 4) -> fill -> b;
    (a, b) -> zip -> map { p -> p.0 + p.1 } -> c;
    c[3] -> println;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "{:?}", po.diagnostics);
    flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
}

#[test]
fn fixpoint_iota_fill_cse() {
    let rr = run(&build(), BUDGET);
    assert_eq!(rr.output, "10\n", "oracle contract");

    // Pre-fix this panicked (debug_assert MAX_ROUNDS) before reaching the
    // assertions; the fixpoint itself is the regression.
    let res = rewrite(build());
    assert!(flow_ir::validate(&res.ir).is_empty());
    let rr2 = run(&res.ir, BUDGET);
    assert_eq!(rr2.output, rr.output, "rewritten oracle contract");
}
