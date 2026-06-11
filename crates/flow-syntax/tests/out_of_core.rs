//! Golden out-of-Core (DESIGN §20 item 3, C8/C13 end-to-end): the lexer
//! tokenizes the full v0.2 surface cleanly and the parser rejects every
//! out-of-Core construct with a dedicated P-code. We exercise every code
//! P0101..P0116 via `out_of_core.flow`, and additionally parse the existing
//! `full_surface.flow` lexer fixture and assert its exact P-code multiset.

mod support;

use flow_syntax::parse;
use std::collections::BTreeMap;
use support::{read_fixture, render_diagnostics, render_tree};

/// Count diagnostics by code.
fn code_counts(src: &str) -> BTreeMap<&'static str, usize> {
    let out = parse(src);
    let mut m = BTreeMap::new();
    for d in &out.diagnostics {
        *m.entry(d.code.0).or_insert(0) += 1;
    }
    m
}

#[test]
fn out_of_core_every_p01xx() {
    let src = read_fixture("out_of_core.flow");
    let counts = code_counts(&src);
    // Every out-of-Core code fires at least once.
    for code in [
        "P0101", "P0102", "P0103", "P0104", "P0105", "P0106", "P0107", "P0108", "P0109", "P0110",
        "P0111", "P0112", "P0113", "P0114", "P0115", "P0116",
    ] {
        assert!(
            counts.get(code).copied().unwrap_or(0) >= 1,
            "expected {code} from out_of_core.flow; got {counts:?}"
        );
    }
    // The pattern-arm-only block yields exactly two P0106 (one per arm) and the
    // `?` operators yield exactly two P0101.
    assert_eq!(counts.get("P0106").copied(), Some(2), "P0106 ×2 (two arms)");
    assert_eq!(counts.get("P0101").copied(), Some(2), "P0101 ×2 (two `?`)");
    // No spurious bare-expression statements leaked into the fixture.
    assert!(
        !counts.contains_key("P0003"),
        "no incidental P0003: {counts:?}"
    );
}

#[test]
fn full_surface_exact_pcode_multiset() {
    // C8 end-to-end: the existing lexer fixture lexes cleanly (zero L-codes) and
    // the parser rejects each out-of-Core construct precisely. The exact multiset
    // (per DESIGN §14.1/§16): two annotations (P0102 ×2), `f? -> g?` (P0101 ×2),
    // `pub use` (P0112), `channel<i32>` + `Result<T, E>` type positions
    // (P0103 ×2), `List::map` (P0109 — chain continues), `[head, ...tail]`
    // (P0107), `void;` (P0113), and the `executor` decl (P0111).
    let src = read_fixture("full_surface.flow");
    let out = parse(&src);

    // The lexer fixture has ZERO lexical errors (C8 premise).
    assert!(
        out.diagnostics.iter().all(|d| !d.code.0.starts_with('L')),
        "full_surface.flow must lex cleanly; got {:#?}",
        out.diagnostics
            .iter()
            .filter(|d| d.code.0.starts_with('L'))
            .collect::<Vec<_>>()
    );

    let counts = code_counts(&src);
    let expected: BTreeMap<&'static str, usize> = [
        ("P0101", 2),
        ("P0102", 2),
        ("P0103", 2),
        ("P0107", 1),
        ("P0109", 1),
        ("P0111", 1),
        ("P0112", 1),
        ("P0113", 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        counts, expected,
        "full_surface.flow exact P-code multiset mismatch"
    );

    // `List::map -> out;` yields exactly one P0109 and the chain continues: the
    // `over` fn body still has its three statements parsed.
    if let Some(flow_syntax::Item::Fn(f)) = out.program.items.iter().find(|i| {
        matches!(i, flow_syntax::Item::Fn(f) if {
            let n = &src[f.name.span.start as usize..f.name.span.end as usize];
            n == "over"
        })
    }) {
        assert!(
            f.body.items.len() >= 2,
            "the `over` fn body kept its statements after `::` recovery"
        );
    } else {
        panic!("expected the `over` fn to be parsed");
    }
}

#[test]
fn out_of_core_tree_snapshot() {
    let src = read_fixture("out_of_core.flow");
    let out = parse(&src);
    insta::assert_snapshot!("out_of_core_tree", render_tree(&src, &out.program));
}

#[test]
fn out_of_core_diagnostics_snapshot() {
    let src = read_fixture("out_of_core.flow");
    let out = parse(&src);
    insta::assert_snapshot!(
        "out_of_core_diagnostics",
        render_diagnostics(&src, &out.diagnostics)
    );
}

#[test]
fn full_surface_tree_snapshot() {
    let src = read_fixture("full_surface.flow");
    let out = parse(&src);
    insta::assert_snapshot!("full_surface_parse_tree", render_tree(&src, &out.program));
}

#[test]
fn full_surface_diagnostics_snapshot() {
    let src = read_fixture("full_surface.flow");
    let out = parse(&src);
    insta::assert_snapshot!(
        "full_surface_parse_diagnostics",
        render_diagnostics(&src, &out.diagnostics)
    );
}
