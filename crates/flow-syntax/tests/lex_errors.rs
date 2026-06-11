//! Golden diagnostics: `tests/fixtures/lex_errors.flow` exercises every L-code
//! (L0001–L0008, incl. `category` and an over-u64 guard discriminant). We
//! snapshot the `Debug`-formatted `LexOutput` (structured values — not
//! rendering, C3) and additionally assert every code appears.

mod support;

use flow_syntax::lex;
use support::{check_invariants, read_fixture};

#[test]
fn lex_errors_all_codes_present() {
    let src = read_fixture("lex_errors.flow");
    // I1–I3 must hold even on the error-laden fixture: Error tokens cover the
    // unknown-char bytes, and only whitespace/comments fall in the gaps
    // (DESIGN §8 I3).
    let out = check_invariants(&src);
    let codes: std::collections::BTreeSet<&str> =
        out.diagnostics.iter().map(|d| d.code.0).collect();
    for code in [
        "L0001", "L0002", "L0003", "L0004", "L0005", "L0006", "L0007", "L0008",
    ] {
        assert!(
            codes.contains(code),
            "missing diagnostic {code}; got {codes:?}"
        );
    }
    // Every Error token must overlap >= 1 diagnostic (I4).
    for t in out
        .tokens
        .iter()
        .filter(|t| t.kind == flow_syntax::TokenKind::Error)
    {
        let overlaps = out
            .diagnostics
            .iter()
            .any(|d| (d.span.start < t.span.end && t.span.start < d.span.end) || d.span == t.span);
        assert!(overlaps, "Error token {t:?} has no overlapping diagnostic");
    }
}

#[test]
fn lex_errors_snapshot() {
    let src = read_fixture("lex_errors.flow");
    let out = lex(&src);
    // Snapshot the structured LexOutput (Debug). No rendering (C3).
    insta::assert_debug_snapshot!("lex_errors_output", out);
}
