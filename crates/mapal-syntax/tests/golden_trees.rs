//! Golden parse trees (DESIGN §20 item 1): the acceptance surface (J4).
//!
//! Each of the eight shipped examples must parse with **zero** diagnostics; the
//! rendered tree is snapshotted with `insta`. Reviewing the `.snap` against the
//! source and the grammar is the verification that parsing is *correct*, not
//! merely stable (the stated failure mode is wrong-but-stable output).

mod support;

use mapal_syntax::{BlockItem, ExprKind, Item, StmtKind, parse};
use support::{read_example, render_tree};

/// Parse an example, assert zero diagnostics (J4), and snapshot the rendered
/// tree.
fn check_example(name: &str) {
    let src = read_example(name);
    let out = parse(&src);
    assert!(
        out.diagnostics.is_empty(),
        "{name}.mapal produced diagnostics: {:#?}",
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

#[test]
fn tree_zip_demo() {
    check_example("zip_demo");
}

#[test]
fn tree_vector_add() {
    check_example("vector_add");
}

/// seq_demo (ADR-0019): `seq { … }` statement block parses with zero diags.
#[test]
fn tree_seq_demo() {
    check_example("seq_demo");
}

/// `()` is the Unit chain head (plan-time-builtin), no longer the P0001
/// "empty parentheses" rejection: `() -> time -> t0;` parses with zero diags
/// and the chain's head node is `ExprKind::Unit`. The tree is snapshotted so
/// the wire-LESS head stays visible in the golden form.
#[test]
fn tree_unit_time_head() {
    let src = "fn main() {\n    () -> time -> t0;\n    t0 -> println;\n}\n";
    let out = parse(src);
    assert!(
        out.diagnostics.is_empty(),
        "`() -> time` produced diagnostics: {:#?}",
        out.diagnostics
    );
    // The first statement's head is Unit — asserted directly, so a
    // wrong-but-stable snapshot cannot hide a regression to P0001/Error.
    let Some(Item::Fn(f)) = out.program.items.first() else {
        panic!("expected `fn main`");
    };
    let Some(BlockItem::Stmt(s)) = f.body.items.first() else {
        panic!("expected a statement");
    };
    let StmtKind::Chain(c) = &s.kind else {
        panic!("expected a chain statement, got {:?}", s.kind);
    };
    let head = c.head.as_ref().expect("`()` is the chain head");
    assert!(
        matches!(head.kind, ExprKind::Unit),
        "chain head must be ExprKind::Unit, got {:?}",
        head.kind
    );
    insta::assert_snapshot!("tree_unit_time_head", render_tree(src, &out.program));
}
