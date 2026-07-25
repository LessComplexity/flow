//! DESIGN §8.3 — example goldens. Every in-Core example through the full
//! `rewrite`:
//!   - interp `RunResult` **exactly** unchanged (the original run IS the
//!     acceptance string — R1's `Done` clause is byte-exact),
//!   - an insta snapshot of the rewritten `to_mermaid` and the `RewriteReport`
//!     (which laws fired — e.g. `sepia`'s duplicates collapse under CSE).

use flow_interp::{RunResult, run};
use flow_ir::{CategoryIr, validate};
use flow_rewrite::rewrite;

const BUDGET: u64 = 200_000;
const EXAMPLES: &[&str] = &[
    "abs",
    "calc",
    "fanout",
    "fir",
    "matmul4_loop",
    "pipeline",
    "sepia",
    "seq_demo",
    "sum_to_n",
    "vector_add",
    "zip_demo",
];

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
        "{name}: parse {:?}",
        po.diagnostics
    );
    flow_lower::lower(&src, &po.program).unwrap_or_else(|d| panic!("{name}: lower {d:?}"))
}

#[test]
fn example_goldens() {
    for &name in EXAMPLES {
        let before: RunResult = run(&build(name), BUDGET);
        let res = rewrite(build(name));

        // R2 — validate-clean.
        let v = validate(&res.ir);
        assert!(v.is_empty(), "{name}: validate {v:?}");

        // R1 — interp output EXACTLY unchanged (Done byte-equal, or same class).
        let after = run(&res.ir, BUDGET);
        assert_eq!(after, before, "{name}: rewrite changed the run");

        insta::assert_snapshot!(format!("{name}_mermaid"), res.ir.to_mermaid());
        insta::assert_snapshot!(format!("{name}_report"), format!("{:#?}", res.report));
    }
}
