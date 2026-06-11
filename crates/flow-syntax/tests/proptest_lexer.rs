//! Property tests (DESIGN §9.5): for arbitrary strings the lexer never panics
//! and invariants I1–I3 hold; a Flow-ish token-soup generator additionally
//! checks determinism.

mod support;

use flow_syntax::{LexOutput, TokenKind, lex};
use proptest::prelude::*;
use support::assert_invariants as assert_coverage;

/// Check invariants I1–I4 on an arbitrary output. Panics on violation (which
/// proptest reports as a failing case with a shrunk input).
///
/// I1–I3 (incl. the gap-is-trivia coverage check) are delegated to the shared
/// `support::assert_invariants` so the property tests enforce *exactly* the same
/// I3 contract as the fixture tests (DESIGN §8 I3). I4 (Error tokens overlap a
/// diagnostic) is checked here since it also needs `diagnostics`.
fn assert_invariants(source: &str, out: &LexOutput) {
    // I1 + I2 + I3 coverage (every inter-token gap is whitespace/comment only).
    assert_coverage(source, out).unwrap_or_else(|e| panic!("property violated: {e}"));

    // I4: every Error token overlaps a diagnostic.
    for t in out.tokens.iter().filter(|t| t.kind == TokenKind::Error) {
        let overlaps = out
            .diagnostics
            .iter()
            .any(|d| (d.span.start < t.span.end && t.span.start < d.span.end) || d.span == t.span);
        assert!(
            overlaps,
            "property violated: I4: Error overlaps a diagnostic"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Arbitrary strings: no panic; I1–I4 hold.
    #[test]
    fn arbitrary_strings_total(s in ".{0,256}") {
        let out = lex(&s);
        assert_invariants(&s, &out);
    }

    /// Arbitrary byte-ish unicode (incl. control chars, multibyte): no panic.
    #[test]
    fn arbitrary_unicode_total(s in "\\PC{0,128}") {
        let out = lex(&s);
        assert_invariants(&s, &out);
    }
}

/// A Flow-ish token piece.
fn flow_piece() -> impl Strategy<Value = String> {
    prop_oneof![
        // keywords / idents
        Just("fn".to_string()),
        Just("type".to_string()),
        Just("mut".to_string()),
        Just("loop".to_string()),
        Just("ret".to_string()),
        Just("map".to_string()),
        Just("fold".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("category".to_string()),
        Just("print".to_string()),
        "[a-z]{1,6}".prop_map(|s| s),
        // literals
        "[0-9]{1,6}".prop_map(|s| s),
        "[0-9]{1,3}\\.[0-9]{1,3}".prop_map(|s| s),
        Just("\"str\"".to_string()),
        // operators / punctuation
        Just("->".to_string()),
        Just("<-".to_string()),
        Just("-true->".to_string()),
        Just("-false->".to_string()),
        Just("-_->".to_string()),
        Just("-7->".to_string()),
        Just("==".to_string()),
        Just("!=".to_string()),
        Just("<=".to_string()),
        Just(">=".to_string()),
        Just("&&".to_string()),
        Just("||".to_string()),
        Just("...".to_string()),
        Just("?".to_string()),
        Just("@".to_string()),
        Just("(".to_string()),
        Just(")".to_string()),
        Just("{".to_string()),
        Just("}".to_string()),
        Just("[".to_string()),
        Just("]".to_string()),
        Just(",".to_string()),
        Just(":".to_string()),
        Just(";".to_string()),
        Just(".".to_string()),
        Just("+".to_string()),
        Just("-".to_string()),
        Just("*".to_string()),
        Just("/".to_string()),
        Just("%".to_string()),
        // a stray unknown to provoke L0001
        Just("$".to_string()),
    ]
}

fn whitespace() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(" ".to_string()),
        Just("  ".to_string()),
        Just("\t".to_string()),
        Just("\n".to_string()),
        Just("\r\n".to_string()),
        Just("".to_string()),
    ]
}

/// Build a Flow-ish token soup: pieces joined with random whitespace.
fn flow_soup() -> impl Strategy<Value = String> {
    proptest::collection::vec((flow_piece(), whitespace()), 0..24).prop_map(|parts| {
        let mut s = String::new();
        for (piece, ws) in parts {
            s.push_str(&piece);
            s.push_str(&ws);
        }
        s
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Flow-ish soup: deterministic + I1–I4 hold.
    #[test]
    fn flow_soup_deterministic_and_total(s in flow_soup()) {
        let a = lex(&s);
        let b = lex(&s);
        prop_assert_eq!(&a, &b, "lexing is deterministic");
        assert_invariants(&s, &a);
    }
}
