//! Golden parse trees (DESIGN §20 item 1): the acceptance surface (J4).
//!
//! Each of the six shipped examples must parse with **zero** diagnostics; the
//! rendered tree is snapshotted with `insta`. Reviewing the `.snap` against the
//! source and the grammar is the verification that parsing is *correct*, not
//! merely stable (the stated failure mode is wrong-but-stable output).

mod support;

use flow_syntax::parse;
use support::{read_example, render_tree};

/// Parse an example, assert zero diagnostics (J4), and snapshot the rendered
/// tree.
fn check_example(name: &str) {
    let src = read_example(name);
    let out = parse(&src);
    assert!(
        out.diagnostics.is_empty(),
        "{name}.flow produced diagnostics: {:#?}",
        out.diagnostics
    );
    let rendered = render_tree(&src, &out.program);
    insta::assert_snapshot!(format!("tree_{name}"), rendered);
}

#[test]
fn tree_abs() {
    check_example("abs");
}

#[test]
fn tree_fanout() {
    check_example("fanout");
}

#[test]
fn tree_fir() {
    check_example("fir");
}

#[test]
fn tree_pipeline() {
    check_example("pipeline");
}

#[test]
fn tree_sepia() {
    check_example("sepia");
}

#[test]
fn tree_sum_to_n() {
    check_example("sum_to_n");
}
