//! Golden full-surface (C8 evidence): the out-of-Core v0.2 surface lexes to
//! non-Error tokens with zero diagnostics (DESIGN §9.3).

mod support;

use mapal_syntax::{TokenKind, lex};
use support::{check_invariants, read_fixture, render_tokens};

#[test]
fn full_surface_zero_diagnostics_no_errors() {
    let src = read_fixture("full_surface.mapal");
    // I1–I3 on the full out-of-Core surface, incl. the gap-is-trivia coverage
    // check (DESIGN §8 I3).
    let out = check_invariants(&src);
    assert!(
        out.diagnostics.is_empty(),
        "full_surface.mapal produced diagnostics: {:#?}",
        out.diagnostics
    );
    assert!(
        !out.tokens.iter().any(|t| t.kind == TokenKind::Error),
        "full_surface.mapal produced Error tokens"
    );
    // Spot-check that the intended out-of-Core tokens were produced.
    let kinds: Vec<TokenKind> = out.tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&TokenKind::At), "expected `@` annotations");
    assert!(kinds.contains(&TokenKind::Question), "expected `?`");
    assert!(kinds.contains(&TokenKind::DotDotDot), "expected `...`");
    assert!(kinds.contains(&TokenKind::KwPub), "expected `pub`");
    assert!(kinds.contains(&TokenKind::KwUse), "expected `use`");
    assert!(
        kinds.contains(&TokenKind::KwExecutor),
        "expected `executor`"
    );
    assert!(kinds.contains(&TokenKind::KwVoid), "expected `void`");
}

#[test]
fn full_surface_snapshot() {
    let src = read_fixture("full_surface.mapal");
    let out = lex(&src);
    let rendered = render_tokens(&src, &out);
    insta::assert_snapshot!("full_surface_tokens", rendered);
}
