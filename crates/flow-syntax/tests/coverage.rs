//! I3 coverage enforcement (DESIGN §8): every input byte belongs to exactly one
//! token span *or* a trivia gap, and every trivia gap is whitespace/comments
//! only. These tests pin the coverage check itself — both that it accepts
//! genuine trivia gaps and that it *rejects* a dropped (non-trivia) source byte,
//! so a future lexer change that silently swallowed a byte would fail CI.

mod support;

use flow_syntax::{LexOutput, SourceLoc, Token, TokenKind, lex};
use support::{assert_invariants, check_invariants};

/// Real sources with assorted trivia (whitespace, line comments incl. UTF-8,
/// block comments, CRLF) must pass the coverage check.
#[test]
fn trivia_gaps_pass_coverage() {
    let sources = [
        "",
        "   ",
        "fn f ( x ) -> ret ;",
        "x;   // trailing line comment\n y ;",
        "a // ∘ × — unicode in comment\n;",
        "p /* block comment trivia */ q ;",
        "r /* unterminated block to eof",
        "first\r\n\tsecond;",
        "  -true-> x ;",
    ];
    for src in sources {
        // `check_invariants` re-lexes and re-walks the gaps; it panics on any
        // I1/I2/I3 violation.
        check_invariants(src);
    }
}

/// The decisive regression test for the review finding: if the lexer ever
/// dropped a *non-trivia* source byte (a gap that is not whitespace/comment),
/// the coverage check must report an I3 violation. We simulate that here by
/// building a `LexOutput` whose token spans skip a real `+` byte, and assert the
/// shared coverage helper rejects it. Before this enforcement existed, such an
/// output passed every test.
#[test]
fn dropped_non_trivia_byte_fails_coverage() {
    // Source: `a+b`. A correct lexer emits Ident(a) Plus(+) Ident(b) Eof.
    // We deliberately OMIT the `+` token, leaving byte 1 in a non-trivia gap.
    let src = "a+b";
    let bad = LexOutput {
        tokens: vec![
            Token {
                kind: TokenKind::Ident,
                span: SourceLoc { start: 0, end: 1 }, // `a`
            },
            // (no token for `+` at byte 1 — the "dropped byte")
            Token {
                kind: TokenKind::Ident,
                span: SourceLoc { start: 2, end: 3 }, // `b`
            },
            Token {
                kind: TokenKind::Eof,
                span: SourceLoc { start: 3, end: 3 },
            },
        ],
        diagnostics: vec![],
    };

    let result = assert_invariants(src, &bad);
    let err = result.expect_err("dropped non-trivia `+` byte must fail I3 coverage");
    assert!(
        err.contains("I3 violated"),
        "expected an I3 coverage violation, got: {err}"
    );

    // Sanity: the *correct* lexer output for the same source passes.
    let good: LexOutput = lex(src);
    assert_invariants(src, &good).expect("correct lexing of `a+b` must satisfy I3");
}

/// A dropped *trivia* byte (e.g. an extra space the lexer skipped) is fine — the
/// gap is still trivia. This guards against the check being overzealous.
#[test]
fn dropped_trivia_byte_passes_coverage() {
    let src = "a b"; // space at byte 1 is legitimately skipped trivia.
    let out = LexOutput {
        tokens: vec![
            Token {
                kind: TokenKind::Ident,
                span: SourceLoc { start: 0, end: 1 },
            },
            Token {
                kind: TokenKind::Ident,
                span: SourceLoc { start: 2, end: 3 },
            },
            Token {
                kind: TokenKind::Eof,
                span: SourceLoc { start: 3, end: 3 },
            },
        ],
        diagnostics: vec![],
    };
    assert_invariants(src, &out).expect("a whitespace gap is valid trivia");
}
