//! Parser property tests (DESIGN §20 item 5): J1 totality + termination over
//! arbitrary strings and a flow-soup generator; J2 span nesting; J3 (zero
//! diagnostics ⇒ no Error nodes / rejected-but-kept forms); J6 lex-diagnostic
//! preservation; determinism (`parse(s) == parse(s)`).
//!
//! Config mirrors `proptest_lexer.rs` (2048 cases). Totality is the load-bearing
//! property: the parser must never panic or hang on adversarial input — the depth
//! guard (P0011) and the progress lemma are what make this hold.

use flow_syntax::*;
use proptest::prelude::*;

// ==========================================================================
// J2 — span-nesting walker: every node span is within source bounds, every
// child span is within its parent, sibling statements are ordered.
// ==========================================================================

fn within(child: SourceLoc, parent: SourceLoc) -> bool {
    child.start >= parent.start && child.end <= parent.end
}

fn check_program(p: &Program, len: u32) {
    assert!(p.span.end <= len, "program span out of bounds");
    for item in &p.items {
        match item {
            Item::Fn(f) => {
                assert!(
                    within(f.span, p.span),
                    "fn {:?} ⊄ program {:?}",
                    f.span,
                    p.span
                );
                check_block(&f.body, f.span, len);
                for param in &f.params {
                    check_ty(&param.ty, param.span, len);
                }
                if let Some(rt) = &f.ret_ty {
                    check_ty(rt, f.span, len);
                }
            }
            Item::Type(t) => {
                assert!(
                    within(t.span, p.span),
                    "type {:?} ⊄ program {:?}",
                    t.span,
                    p.span
                );
                for field in &t.fields {
                    check_ty(&field.ty, field.span, len);
                }
            }
            Item::Error(s) => {
                assert!(s.end <= len);
                assert!(
                    within(*s, p.span),
                    "item::error {s:?} ⊄ program {:?}",
                    p.span
                );
            }
        }
    }
}

fn check_block(b: &Block, parent: SourceLoc, len: u32) {
    assert!(b.span.end <= len, "block span {:?} > len {len}", b.span);
    assert!(within(b.span, parent), "block {:?} ⊄ {:?}", b.span, parent);
    for it in &b.items {
        match it {
            BlockItem::Stmt(s) => check_stmt(s, b.span, len),
            BlockItem::Arm(a) => {
                assert!(within(a.span, b.span), "arm ⊄ block");
                check_payload(&a.payload, a.span, len);
            }
        }
    }
    if let Some(t) = &b.tail {
        check_chain(t, b.span, len);
    }
}

fn check_stmt(s: &Stmt, parent: SourceLoc, len: u32) {
    assert!(s.span.end <= len);
    assert!(within(s.span, parent), "stmt {:?} ⊄ {:?}", s.span, parent);
    match &s.kind {
        StmtKind::Chain(c) => check_chain(c, s.span, len),
        StmtKind::Bind(b) => check_expr(&b.value, s.span, len),
        StmtKind::Loop(l) => check_block(&l.body, s.span, len),
        StmtKind::Error => {}
    }
}

fn check_payload(p: &ArmPayload, parent: SourceLoc, len: u32) {
    match p {
        ArmPayload::Chain(c) => check_chain(c, parent, len),
        ArmPayload::Block(b) => check_block(b, parent, len),
    }
}

fn check_chain(c: &Chain, parent: SourceLoc, len: u32) {
    assert!(c.span.end <= len);
    assert!(within(c.span, parent), "chain {:?} ⊄ {:?}", c.span, parent);
    if let Some(h) = &c.head {
        check_expr(h, c.span, len);
    }
    for st in &c.stages {
        assert!(within(st.span, c.span), "stage ⊄ chain");
        match &st.kind {
            StageKind::Expr(e) | StageKind::OpShorthand { expr: e } => check_expr(e, st.span, len),
            StageKind::Bind { ty: Some(ty), .. } => check_ty(ty, st.span, len),
            StageKind::Guard(arms) => {
                for a in arms {
                    assert!(within(a.span, st.span), "stage-arm ⊄ stage");
                    assert!(within(a.discr_span, a.span), "arm discr ⊄ arm");
                    check_payload(&a.payload, st.span, len);
                }
            }
            StageKind::Fanout { branches, .. } => {
                for br in branches {
                    check_chain(br, st.span, len);
                }
            }
            StageKind::MapFold { params, body, .. } => {
                for p in params {
                    assert!(within(p.span, st.span), "map/fold param ⊄ stage");
                }
                check_block(body, st.span, len)
            }
            StageKind::StmtBlock(body) => check_block(body, st.span, len),
            _ => {}
        }
    }
}

fn check_expr(e: &Expr, parent: SourceLoc, len: u32) {
    assert!(e.span.end <= len, "expr span {:?} > len {len}", e.span);
    assert!(within(e.span, parent), "expr {:?} ⊄ {:?}", e.span, parent);
    match &e.kind {
        ExprKind::Unary { operand, .. } => check_expr(operand, e.span, len),
        ExprKind::Binary { lhs, rhs, .. } => {
            check_expr(lhs, e.span, len);
            check_expr(rhs, e.span, len);
        }
        ExprKind::Member { base, .. } => check_expr(base, e.span, len),
        ExprKind::Index { base, index } => {
            check_expr(base, e.span, len);
            check_expr(index, e.span, len);
        }
        ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
            for x in xs {
                check_expr(x, e.span, len);
            }
        }
        ExprKind::Struct { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    check_expr(v, f.span, len);
                }
            }
        }
        ExprKind::Call { callee, args } => {
            check_expr(callee, e.span, len);
            for a in args {
                check_expr(a, e.span, len);
            }
        }
        ExprKind::Question(b) => check_expr(b, e.span, len),
        _ => {}
    }
}

// ==========================================================================
// J3 — zero diagnostics ⇒ no Error nodes and no rejected-but-kept forms.
// ==========================================================================

fn has_error_or_rejected(p: &Program) -> bool {
    p.items.iter().any(|i| match i {
        Item::Error(_) => true,
        Item::Type(t) => t.fields.iter().any(|f| ty_bad(&f.ty)),
        Item::Fn(f) => {
            f.params.iter().any(|p| ty_bad(&p.ty))
                || f.ret_ty.as_ref().map(ty_bad).unwrap_or(false)
                || block_bad(&f.body)
        }
    })
}

fn ty_bad(t: &Ty) -> bool {
    match &t.kind {
        TyKind::Error | TyKind::Dynamic(_) => true,
        TyKind::Named(_) => false,
        TyKind::Array { elem, .. } => ty_bad(elem),
        TyKind::Tuple(es) => es.iter().any(ty_bad),
    }
}

/// J2 helper: a `Ty` and its nested element types are within bounds and parent.
fn check_ty(t: &Ty, parent: SourceLoc, len: u32) {
    assert!(t.span.end <= len, "ty span {:?} > len {len}", t.span);
    assert!(within(t.span, parent), "ty {:?} ⊄ {:?}", t.span, parent);
    match &t.kind {
        TyKind::Array { elem, .. } | TyKind::Dynamic(elem) => check_ty(elem, t.span, len),
        TyKind::Tuple(es) => {
            for e in es {
                check_ty(e, t.span, len);
            }
        }
        _ => {}
    }
}

fn block_bad(b: &Block) -> bool {
    b.items.iter().any(|it| match it {
        BlockItem::Stmt(s) => stmt_bad(s),
        BlockItem::Arm(a) => a.discr == GuardDiscr::OutOfCore || payload_bad(&a.payload),
    }) || b.tail.as_ref().map(chain_bad).unwrap_or(false)
}

fn payload_bad(p: &ArmPayload) -> bool {
    match p {
        ArmPayload::Chain(c) => chain_bad(c),
        ArmPayload::Block(b) => block_bad(b),
    }
}

fn stmt_bad(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Error => true,
        StmtKind::Chain(c) => chain_bad(c),
        StmtKind::Bind(b) => expr_bad(&b.value),
        StmtKind::Loop(l) => matches!(l.label, LoopLabel::Custom(_)) || block_bad(&l.body),
    }
}

fn chain_bad(c: &Chain) -> bool {
    c.head.as_ref().map(expr_bad).unwrap_or(false)
        || c.stages.iter().any(|st| match &st.kind {
            StageKind::Error(_) | StageKind::StmtBlock(_) => true,
            StageKind::Fanout { kind, branches } => {
                matches!(kind, FanoutKind::Void(_)) || branches.iter().any(chain_bad)
            }
            StageKind::Expr(e) | StageKind::OpShorthand { expr: e } => expr_bad(e),
            StageKind::Guard(arms) => arms
                .iter()
                .any(|a| a.discr == GuardDiscr::OutOfCore || payload_bad(&a.payload)),
            StageKind::MapFold { body, .. } => block_bad(body),
            StageKind::Bind { ty, .. } => ty.as_ref().map(ty_bad).unwrap_or(false),
            _ => false,
        })
}

fn expr_bad(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Error | ExprKind::Hole | ExprKind::Call { .. } | ExprKind::Question(_) => true,
        ExprKind::Unary { operand, .. } => expr_bad(operand),
        ExprKind::Binary { lhs, rhs, .. } => expr_bad(lhs) || expr_bad(rhs),
        ExprKind::Member { base, .. } => expr_bad(base),
        ExprKind::Index { base, index } => expr_bad(base) || expr_bad(index),
        ExprKind::Tuple(xs) | ExprKind::Array(xs) => xs.iter().any(expr_bad),
        ExprKind::Struct { fields, .. } => fields
            .iter()
            .any(|f| f.value.as_ref().map(expr_bad).unwrap_or(false)),
        _ => false,
    }
}

/// All-in-one property checker: J1 (no panic — implied by reaching here), J2
/// (span nesting), J3 (clean ⇒ no Error/rejected forms), J6 (lex diags ⊆ parse
/// diags), and determinism.
fn assert_properties(src: &str) {
    let len = src.len() as u32;
    let out = parse(src);
    // J2.
    check_program(&out.program, len);
    // J3.
    if out.diagnostics.is_empty() {
        assert!(
            !has_error_or_rejected(&out.program),
            "J3: zero diagnostics but the tree has Error/rejected-kept forms: {src:?}"
        );
    }
    // J6: every lex diagnostic is preserved in parse output.
    let lexed = lex(src);
    for ld in &lexed.diagnostics {
        assert!(
            out.diagnostics.contains(ld),
            "J6: lex diagnostic dropped: {ld:?} for {src:?}"
        );
    }
    // Determinism.
    let again = parse(src);
    assert_eq!(out, again, "parse is deterministic for {src:?}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    /// Arbitrary strings: no panic, no hang; J2/J3/J6 hold; deterministic.
    #[test]
    fn arbitrary_strings_total(s in ".{0,256}") {
        assert_properties(&s);
    }

    /// Arbitrary unicode (incl. control chars, multibyte): totality.
    #[test]
    fn arbitrary_unicode_total(s in "\\PC{0,128}") {
        assert_properties(&s);
    }
}

// --- flow-soup generator (adapted from proptest_lexer.rs) -----------------

fn flow_piece() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("fn".to_string()),
        Just("type".to_string()),
        Just("mut".to_string()),
        Just("loop".to_string()),
        Just("ret".to_string()),
        Just("seq".to_string()),
        Just("map".to_string()),
        Just("fold".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        Just("void".to_string()),
        Just("category".to_string()),
        Just("print".to_string()),
        "[a-z]{1,6}".prop_map(|s| s),
        "[0-9]{1,6}".prop_map(|s| s),
        "[0-9]{1,3}\\.[0-9]{1,3}".prop_map(|s| s),
        Just("\"str\"".to_string()),
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
    proptest::collection::vec((flow_piece(), whitespace()), 0..32).prop_map(|parts| {
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

    /// Flow-ish soup: totality + determinism + all invariants.
    #[test]
    fn flow_soup_total(s in flow_soup()) {
        assert_properties(&s);
    }
}
