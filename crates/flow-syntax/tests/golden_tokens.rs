//! Golden token-stream snapshots for the eight shipped examples (DESIGN §9.1).
//!
//! Each example must lex to **zero diagnostics** and produce no `Error` tokens.
//! The rendered stream is snapshotted with `insta`; reviewing the `.snap`
//! against the source is the verification that tokenization is *correct*, not
//! merely stable.

mod support;

use flow_syntax::TokenKind;
use support::{check_invariants, read_example, render_tokens};

/// Lex an example, assert zero diagnostics + no Error tokens, and snapshot the
/// rendered token stream.
fn check_example(name: &str) {
    let src = read_example(name);
    // Enforce I1–I3 on every example (DESIGN §8 I3: coverage re-walk on all
    // fixtures). `check_invariants` re-validates that every inter-token gap is
    // whitespace/comment-only, so a dropped source byte would fail here.
    let out = check_invariants(&src);
    assert!(
        out.diagnostics.is_empty(),
        "{name}.flow produced diagnostics: {:#?}",
        out.diagnostics
    );
    assert!(
        !out.tokens.iter().any(|t| t.kind == TokenKind::Error),
        "{name}.flow produced Error tokens"
    );
    assert_eq!(
        out.tokens.last().map(|t| t.kind),
        Some(TokenKind::Eof),
        "{name}.flow missing trailing Eof"
    );
    let rendered = render_tokens(&src, &out);
    insta::assert_snapshot!(format!("golden_{name}"), rendered);
}

#[test]
fn golden_abs() {
    check_example("abs");
}

#[test]
fn golden_fanout() {
    check_example("fanout");
}

#[test]
fn golden_fir() {
    check_example("fir");
}

#[test]
fn golden_pipeline() {
    check_example("pipeline");
}

#[test]
fn golden_sepia() {
    check_example("sepia");
}

#[test]
fn golden_sum_to_n() {
    check_example("sum_to_n");
}

#[test]
fn golden_zip_demo() {
    check_example("zip_demo");
}

#[test]
fn golden_vector_add() {
    check_example("vector_add");
}

/// seq_demo (ADR-0019): the `seq { … }` statement-block showcase.
#[test]
fn golden_seq_demo() {
    check_example("seq_demo");
}
