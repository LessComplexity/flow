//! Pass 2 rejection & acceptance fixtures (DESIGN §7.3) — real `parse → lower →
//! check`. Effects are graph-invisible (CK1): a fanout branch carries no kind
//! marker in the graph, and `seq` has **no IR footprint at all** (ADR-0019 pin
//! d) — so print-in-fanout and print-in-seq seal to indistinguishable graphs,
//! every source below *lowers clean*, and check (reading the tree's
//! `Fanout`-vs-`SeqBlock` node distinction) is what distinguishes them.

use flow_check::check;

/// Parse + lower `src` (asserting both clean) then run check, returning the
/// diagnostics.
fn check_src(src: &str) -> Vec<flow_syntax::Diagnostic> {
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    let ir = flow_lower::lower(src, &po.program).unwrap_or_else(|ds| panic!("lower: {ds:?}"));
    check(src, &po.program, &ir)
}

fn codes(diags: &[flow_syntax::Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code.0).collect()
}

/// A `print` in a `Plain` fanout branch → T0201 whose message names `seq`.
#[test]
fn print_in_plain_branch_is_t0201_naming_seq() {
    let src = r#"
fn main() {
    6 -> {
        -> println;
        -> println;
    }
}
"#;
    let diags = check_src(src);
    assert_eq!(codes(&diags), ["T0201", "T0201"]);
    for d in &diags {
        assert!(d.message.contains("seq"), "message must name seq: {d:?}");
        assert!(
            d.message.contains("println"),
            "message must name the builtin: {d:?}"
        );
    }
    // §2 span discipline: T0201 points at the offending builtin's own span, not
    // the fanout/branch head. The two prints are at the two `println` tokens.
    let p1 = src.find("println").unwrap() as u32;
    let p2 = src[p1 as usize + 1..].find("println").unwrap() as u32 + p1 + 1;
    assert_eq!((diags[0].span.start, diags[0].span.end), (p1, p1 + 7));
    assert_eq!((diags[1].span.start, diags[1].span.end), (p2, p2 + 7));
}

/// An effectful *call* (a fn that prints) in a `Plain` branch → T0201 (the
/// closure case: `effectful?` is read off the lowered signature, CK4).
#[test]
fn effectful_call_in_branch_is_t0201() {
    let src = r#"
fn emit(x: i32) -> i32 { x -> println; x -> ret; }
fn main() {
    6 -> {
        -> emit -> a;
        -> emit -> b;
    }
    a -> println;
    b -> println;
}
"#;
    let diags = check_src(src);
    assert_eq!(codes(&diags), ["T0201", "T0201"]);
    for d in &diags {
        assert!(
            d.message.contains("emit"),
            "message must name the fn: {d:?}"
        );
        assert!(d.message.contains("seq"), "message must name seq: {d:?}");
    }
}

/// A nested `Plain` fanout: the inner branch's `print` is still in context →
/// T0201 (recurse; one report per offending site).
#[test]
fn nested_plain_fanout_inner_print_is_t0201() {
    let src = r#"
fn main() {
    6 -> {
        -> { -> println; -> println; };
        -> println;
    }
}
"#;
    // Two inner prints + one outer print, all inside a Plain branch context.
    let diags = check_src(src);
    assert_eq!(codes(&diags), ["T0201", "T0201", "T0201"]);
}

/// C-check-4 (CK5, now a theorem — ADR-0019): a `seq` block **inside** a
/// `Plain` branch does NOT clear the illegal context. This falls out of
/// composition, not a conservative pin: an effectful `seq` in a branch is an
/// effectful morphism in a branch (the sticky context), T0201 at each print.
#[test]
fn seq_inside_plain_branch_still_t0201() {
    let src = r#"
fn main() {
    6 -> {
        -> seq { -> println; -> println; };
        -> println;
    }
}
"#;
    let diags = check_src(src);
    // The two seq-nested prints + the sibling branch print all stay illegal.
    assert_eq!(codes(&diags), ["T0201", "T0201", "T0201"]);
}

/// The OQ-C1 loosening (closed by ADR-0019, free by construction): a **pure**
/// `seq` block inside a `Plain` branch → CLEAN. `effectful?(seq b) = ⋁
/// effectful?(body)`, so a seq of pure calls is a pure composite; nothing in the
/// branch is an effect site. (The old conservative CK5 pin would have needed a
/// blanket rule; composition gives this for free.)
#[test]
fn pure_seq_inside_plain_branch_is_clean() {
    let src = r#"
fn square(x: i32) -> i32 { x * x -> ret; }
fn double(x: i32) -> i32 { x * 2 -> ret; }
fn main() {
    6 -> {
        -> seq { -> square -> sq; };
        -> double -> db;
    }
    sq -> println;
    db -> println;
}
"#;
    assert!(check_src(src).is_empty());
}

/// The other nesting direction (WP3 matrix "nested fanout/seq mixes"): a
/// `Fanout` **inside** a top-level `seq`. The seq is sequential (`in_fanout`
/// stays false through the `SeqBlock` arm), but the inner `Fanout` node forces
/// the illegal context unconditionally — both branch prints are T0201. Guards
/// the WP3 `Fanout` unconditional-`true` line: a revert to a kind-gated open
/// would silently pass this.
#[test]
fn fanout_inside_seq_is_t0201() {
    let src = r#"
fn main() {
    6 -> seq {
        -> { -> println; -> println; };
    }
}
"#;
    let diags = check_src(src);
    assert_eq!(codes(&diags), ["T0201", "T0201"]);
}

/// Composition rule 2 ("OR over body") reaches the seq body's *statement forms*,
/// not just its chains: a `mut i <- …` rebind and a `loop` both live in this
/// seq. The seq sits inside a `Plain` branch, so the sticky context carries
/// `in_fanout = true` through the `SeqBlock` arm, through the rebind, and into
/// the loop body — the in-loop `println` is the lone effect site → one T0201.
/// (The sibling branch is pure; `toplevel_seq_is_clean` is the sequential
/// control for the same shape.)
#[test]
fn loop_and_rebind_inside_seq_in_branch_is_t0201() {
    let src = r#"
fn square(x: i32) -> i32 { x * x -> ret; }
fn main() {
    6 -> {
        -> seq {
            mut i: i32 <- 3;
            loop {
                (i > 0) -> {
                    -true-> {
                        i -> println;
                        i - 1 -> i;
                        -> loop;
                    }
                    -false-> i -> done;
                }
            }
        };
        -> square -> s;
    }
    s -> println;
}
"#;
    let diags = check_src(src);
    assert_eq!(codes(&diags), ["T0201"]);
    assert!(
        diags[0].message.contains("println"),
        "the caught site is the in-loop println: {:?}",
        diags[0]
    );
}

/// A **top-level** `seq { print; print }` is the legal effect site → CLEAN.
/// Since ADR-0019 `seq` is its own node (`StageKind::SeqBlock`), so the walk
/// simply never opens the illegal context here (node-kind discrimination) — no
/// same-node-kind trap to avoid.
#[test]
fn toplevel_seq_is_clean() {
    let src = r#"
fn main() {
    6 -> seq {
        -> println;
        -> println;
    }
}
"#;
    assert!(check_src(src).is_empty());
}

/// Effects in a linear chain are sequential by construction → CLEAN.
#[test]
fn prints_in_linear_chain_clean() {
    let src = r#"
fn main() {
    6 -> println;
    7 -> println;
}
"#;
    assert!(check_src(src).is_empty());
}

/// A pure parallel fanout (`fanout.flow`: pure `square`/`double` branches) →
/// CLEAN — the pure-fanout baseline.
#[test]
fn pure_fanout_clean() {
    let src = std::fs::read_to_string(format!(
        "{}/../../examples/fanout.flow",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let po = flow_syntax::parse(&src);
    assert!(po.diagnostics.is_empty());
    let ir = flow_lower::lower(&src, &po.program).unwrap();
    assert!(check(&src, &po.program, &ir).is_empty());
}

/// The local-shadowing case (DESIGN §7.3): a local binding shadowing an
/// effectful fn name, used as a bare stage in a `Plain` branch. Lower REJECTS
/// the shape upstream (L1105 FunctionAsValue) — so check never sees it; this
/// test documents that rejection (per DESIGN §7.3's "if lower rejects the shape
/// upstream, the test documents that instead"). check's own effects walk is
/// scope-aware regardless (it mirrors lower's discipline), but the shape can
/// never reach it through the real pipeline.
#[test]
fn local_shadowing_is_rejected_by_lower() {
    let src = r#"
fn g(x: i32) -> i32 { x -> println; x -> ret; }
fn main() {
    5 -> g -> g;
    6 -> {
        -> g;
        -> g;
    }
    g -> println;
}
"#;
    let po = flow_syntax::parse(src);
    assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
    let err = flow_lower::lower(src, &po.program)
        .expect_err("lower should reject a local shadowing a fn name used as a stage");
    assert!(
        err.iter().any(|d| d.code.0 == "L1105"),
        "expected L1105 FunctionAsValue, got {:?}",
        err.iter().map(|d| d.code.0).collect::<Vec<_>>()
    );
}
