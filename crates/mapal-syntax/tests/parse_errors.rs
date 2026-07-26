//! Golden parse errors (DESIGN §20 item 2): `tests/fixtures/parse_errors.mapal`
//! holds ≥6 independent syntax defects exercising the syntax-class P-codes
//! (P0001–P0010, P0012). We snapshot the rendered tree + `Debug` diagnostics and
//! assert (non-snapshot) that the expected distinct P-codes are present, proving
//! multi-error recovery (ADR-0008a).

mod support;

use mapal_syntax::parse;
use std::collections::BTreeSet;
use support::{read_fixture, render_diagnostics, render_tree};

#[test]
fn parse_errors_multi_recovery() {
    let src = read_fixture("parse_errors.mapal");
    let out = parse(&src);
    let pcodes: BTreeSet<&str> = out
        .diagnostics
        .iter()
        .map(|d| d.code.0)
        .filter(|c| c.starts_with('P'))
        .collect();

    // Every syntax-class defect the fixture isolates must be reported — no
    // masking. (P0011 is exercised in unit/proptest, not here.)
    for code in [
        "P0001", "P0002", "P0003", "P0004", "P0005", "P0006", "P0007", "P0008", "P0009", "P0010",
        "P0012",
    ] {
        assert!(
            pcodes.contains(code),
            "expected {code} from the fixture; got {pcodes:?}"
        );
    }

    // The P0004 stray-guard fix must slice the over-u64 source lexeme verbatim
    // (never re-render the clamped GuardKind::Int).
    let p0004 = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0004")
        .expect("P0004 present");
    let fix = p0004.fix.as_ref().expect("P0004 has a fix");
    assert_eq!(
        fix.replacement, "-99999999999999999999 ->",
        "the fix slices the source lexeme verbatim"
    );

    // The lexer's L0008 (over-u64 discriminant) is preserved and not duplicated.
    let l0008 = out
        .diagnostics
        .iter()
        .filter(|d| d.code.0 == "L0008")
        .count();
    assert_eq!(l0008, 1, "exactly one L0008 (C14: no double report)");
}

#[test]
fn parse_errors_tree_snapshot() {
    let src = read_fixture("parse_errors.mapal");
    let out = parse(&src);
    insta::assert_snapshot!("parse_errors_tree", render_tree(&src, &out.program));
}

#[test]
fn parse_errors_diagnostics_snapshot() {
    let src = read_fixture("parse_errors.mapal");
    let out = parse(&src);
    insta::assert_snapshot!(
        "parse_errors_diagnostics",
        render_diagnostics(&src, &out.diagnostics)
    );
}
