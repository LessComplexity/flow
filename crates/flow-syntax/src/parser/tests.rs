//! Parser unit tests (DESIGN §20 item 4): the §3.6 precedence examples verbatim,
//! the precedence/associativity table, hole-expression shapes, every §17 ledger
//! row, the §14.4 classification table, ADR-0011 scan cases, the termination-rule
//! matrix, trailing commas, `ret.0` targets, the guard-payload F-matrix,
//! `category` recovery, and the pinned design-review regressions.

use super::*;
use crate::loc::LineIndex;

// --- helpers --------------------------------------------------------------

fn codes(src: &str) -> Vec<&'static str> {
    parse(src).diagnostics.iter().map(|d| d.code.0).collect()
}

/// The multiset of parser P-codes (lex L-codes excluded), as a sorted vec.
fn pcodes(src: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = parse(src)
        .diagnostics
        .iter()
        .map(|d| d.code.0)
        .filter(|c| c.starts_with('P'))
        .collect();
    v.sort_unstable();
    v
}

fn no_diags(src: &str) {
    let out = parse(src);
    assert!(
        out.diagnostics.is_empty(),
        "expected zero diagnostics for {src:?}, got {:#?}",
        out.diagnostics
    );
}

/// Parse and return the single fn body's first statement chain, panicking
/// otherwise. Convenience for inspecting expression/stage shapes.
fn parse_one_fn(src: &str) -> FnDecl {
    let out = parse(src);
    match out.program.items.into_iter().next() {
        Some(Item::Fn(f)) => f,
        other => panic!("expected one fn, got {other:?}"),
    }
}

/// Walk every node and assert child spans are within parent spans and within
/// source bounds (J2). Returns the count of nodes visited.
fn assert_spans(src: &str) {
    let out = parse(src);
    let len = src.len() as u32;
    for item in &out.program.items {
        if let Item::Fn(f) = item {
            assert!(f.span.end <= len);
            walk_block(&f.body, f.span, len);
        }
    }
}

fn within(child: SourceLoc, parent: SourceLoc) -> bool {
    child.start >= parent.start && child.end <= parent.end
}

/// J2 helper: a `Ty` and its nested element types are within bounds and parent.
fn walk_ty(t: &Ty, parent: SourceLoc, len: u32) {
    assert!(t.span.end <= len, "ty span out of bounds: {:?}", t.span);
    assert!(within(t.span, parent), "ty {:?} ⊄ {:?}", t.span, parent);
    match &t.kind {
        TyKind::Array { elem, .. } | TyKind::Dynamic(elem) => walk_ty(elem, t.span, len),
        TyKind::Tuple(es) => {
            for e in es {
                walk_ty(e, t.span, len);
            }
        }
        _ => {}
    }
}

fn walk_block(b: &Block, parent: SourceLoc, len: u32) {
    assert!(b.span.end <= len, "block span out of bounds");
    assert!(within(b.span, parent), "block {:?} ⊄ {:?}", b.span, parent);
    for it in &b.items {
        match it {
            BlockItem::Stmt(s) => walk_stmt(s, b.span, len),
            BlockItem::Arm(a) => {
                assert!(within(a.span, b.span), "arm ⊄ block");
                walk_payload(&a.payload, a.span, len);
            }
        }
    }
    if let Some(t) = &b.tail {
        assert!(within(t.span, b.span), "tail ⊄ block");
        walk_chain(t, b.span, len);
    }
}

fn walk_stmt(s: &Stmt, parent: SourceLoc, len: u32) {
    assert!(within(s.span, parent), "stmt {:?} ⊄ {:?}", s.span, parent);
    match &s.kind {
        StmtKind::Chain(c) => walk_chain(c, s.span, len),
        StmtKind::Bind(b) => walk_expr(&b.value, s.span, len),
        StmtKind::Loop(l) => walk_block(&l.body, s.span, len),
        StmtKind::Error => {}
    }
}

fn walk_payload(p: &ArmPayload, parent: SourceLoc, len: u32) {
    match p {
        ArmPayload::Chain(c) => walk_chain(c, parent, len),
        ArmPayload::Block(b) => walk_block(b, parent, len),
    }
}

fn walk_chain(c: &Chain, parent: SourceLoc, len: u32) {
    assert!(within(c.span, parent), "chain {:?} ⊄ {:?}", c.span, parent);
    if let Some(h) = &c.head {
        walk_expr(h, c.span, len);
    }
    for st in &c.stages {
        assert!(within(st.span, c.span), "stage ⊄ chain");
        match &st.kind {
            StageKind::Expr(e) | StageKind::OpShorthand { expr: e } => walk_expr(e, st.span, len),
            StageKind::Bind { ty: Some(ty), .. } => walk_ty(ty, st.span, len),
            StageKind::Guard(arms) => {
                for a in arms {
                    assert!(within(a.span, st.span), "stage-arm ⊄ stage");
                    assert!(within(a.discr_span, a.span), "arm discr ⊄ arm");
                    walk_payload(&a.payload, st.span, len);
                }
            }
            StageKind::Fanout { branches, .. } => {
                for br in branches {
                    walk_chain(br, st.span, len);
                }
            }
            StageKind::MapFold { params, body, .. } => {
                for p in params {
                    assert!(within(p.span, st.span), "map/fold param ⊄ stage");
                }
                walk_block(body, st.span, len)
            }
            StageKind::StmtBlock(body) => walk_block(body, st.span, len),
            _ => {}
        }
    }
}

fn walk_expr(e: &Expr, parent: SourceLoc, len: u32) {
    assert!(e.span.end <= len, "expr span out of bounds: {:?}", e.span);
    assert!(within(e.span, parent), "expr {:?} ⊄ {:?}", e.span, parent);
    match &e.kind {
        ExprKind::Unary { operand, .. } => walk_expr(operand, e.span, len),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, e.span, len);
            walk_expr(rhs, e.span, len);
        }
        ExprKind::Member { base, .. } => walk_expr(base, e.span, len),
        ExprKind::Index { base, index } => {
            walk_expr(base, e.span, len);
            walk_expr(index, e.span, len);
        }
        ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
            for x in xs {
                walk_expr(x, e.span, len);
            }
        }
        ExprKind::Struct { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    walk_expr(v, f.span, len);
                }
            }
        }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, e.span, len);
            for a in args {
                walk_expr(a, e.span, len);
            }
        }
        ExprKind::Question(b) => walk_expr(b, e.span, len),
        _ => {}
    }
}

/// Extract the first statement's chain from a fn body.
fn first_chain(f: &FnDecl) -> &Chain {
    match &f.body.items[0] {
        BlockItem::Stmt(Stmt {
            kind: StmtKind::Chain(c),
            ..
        }) => c,
        other => panic!("expected a chain statement, got {other:?}"),
    }
}

// ======================================================================
// Acceptance: examples parse with zero diagnostics (J4 — also in golden tests)
// ======================================================================

#[test]
fn examples_parse_clean() {
    for name in ["abs", "fanout", "fir", "pipeline", "sepia", "sum_to_n"] {
        let path = format!("{}/../../examples/{name}.flow", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path).expect("read example");
        let out = parse(&src);
        assert!(
            out.diagnostics.is_empty(),
            "{name}.flow produced diagnostics: {:#?}",
            out.diagnostics
        );
        assert_spans(&src);
    }
}

// ======================================================================
// §3.6 precedence examples verbatim (DESIGN §20 item 4)
// ======================================================================

#[test]
fn s36_a_plus_b_arrow_c() {
    // `a + b -> c` : the head is `(a + b)`, one stage `-> c`.
    let f = parse_one_fn("fn m() { a + b -> c; }");
    let c = first_chain(&f);
    match &c.head.as_ref().unwrap().kind {
        ExprKind::Binary { op: BinOp::Add, .. } => {}
        other => panic!("expected Add head, got {other:?}"),
    }
    assert_eq!(c.stages.len(), 1);
}

#[test]
fn s36_a_arrow_b_plus_c_arrow_d() {
    // ADR-0005: `a -> b + c -> d` ≡ `a -> (b + c) -> d`. Head `a`, two stages;
    // the first stage is the expression `b + c`.
    let f = parse_one_fn("fn m() { a -> b + c -> d; }");
    let c = first_chain(&f);
    assert!(matches!(c.head.as_ref().unwrap().kind, ExprKind::Var(_)));
    assert_eq!(c.stages.len(), 2, "two stages");
    match &c.stages[0].kind {
        StageKind::Expr(Expr {
            kind: ExprKind::Binary { op: BinOp::Add, .. },
            ..
        }) => {}
        other => panic!("first stage should be `b + c`, got {other:?}"),
    }
}

#[test]
fn s36_x_arrow_f_dot_method() {
    // `x -> f.method` : head `x`, one stage which is a member access.
    let f = parse_one_fn("fn m() { x -> f.method; }");
    let c = first_chain(&f);
    assert_eq!(c.stages.len(), 1);
    match &c.stages[0].kind {
        StageKind::Expr(Expr {
            kind: ExprKind::Member { .. },
            ..
        }) => {}
        other => panic!("expected member-access stage, got {other:?}"),
    }
}

// ======================================================================
// precedence / associativity table (W14, W15)
// ======================================================================

#[test]
fn mul_binds_tighter_than_add() {
    // `-> + 2 * 3` ⇒ `· + (2 * 3)`. Use a pipeline so we exercise the climber.
    let f = parse_one_fn("fn m() { x -> + 2 * 3; }");
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::OpShorthand {
            expr:
                Expr {
                    kind:
                        ExprKind::Binary {
                            op: BinOp::Add,
                            rhs,
                            ..
                        },
                    ..
                },
        } => {
            assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Mul, .. }));
        }
        other => panic!("expected `· + (2*3)`, got {other:?}"),
    }
}

#[test]
fn op_shorthand_simple_form() {
    // `-> + 5` ⇒ Binary(Add, Hole, Int 5).
    let f = parse_one_fn("fn m() { x -> + 5; }");
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::OpShorthand {
            expr:
                Expr {
                    kind:
                        ExprKind::Binary {
                            op: BinOp::Add,
                            lhs,
                            rhs,
                            ..
                        },
                    ..
                },
        } => {
            assert!(matches!(lhs.kind, ExprKind::Hole), "hole is leftmost leaf");
            assert!(matches!(rhs.kind, ExprKind::Int(5)));
        }
        other => panic!("expected `· + 5`, got {other:?}"),
    }
}

#[test]
fn op_shorthand_mul_then_add() {
    // `-> * 2 + 1` ⇒ `(· * 2) + 1` = Binary(Add, Binary(Mul, Hole, Int 2), Int 1).
    let f = parse_one_fn("fn m() { x -> * 2 + 1; }");
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::OpShorthand {
            expr:
                Expr {
                    kind:
                        ExprKind::Binary {
                            op: BinOp::Add,
                            lhs,
                            rhs,
                            ..
                        },
                    ..
                },
        } => {
            assert!(matches!(rhs.kind, ExprKind::Int(1)));
            match &lhs.kind {
                ExprKind::Binary {
                    op: BinOp::Mul,
                    lhs: ll,
                    ..
                } => {
                    assert!(matches!(ll.kind, ExprKind::Hole));
                }
                other => panic!("expected `(· * 2)`, got {other:?}"),
            }
        }
        other => panic!("expected `(· * 2) + 1`, got {other:?}"),
    }
}

#[test]
fn exactly_one_hole_leftmost() {
    // Invariant: exactly one Hole, leftmost leaf, nowhere else.
    let f = parse_one_fn("fn m() { x -> + 2 * 3; }");
    let c = first_chain(&f);
    let StageKind::OpShorthand { expr } = &c.stages[0].kind else {
        panic!("op-shorthand");
    };
    assert_eq!(count_holes(expr), 1);
}

fn count_holes(e: &Expr) -> usize {
    match &e.kind {
        ExprKind::Hole => 1,
        ExprKind::Binary { lhs, rhs, .. } => count_holes(lhs) + count_holes(rhs),
        ExprKind::Unary { operand, .. } => count_holes(operand),
        _ => 0,
    }
}

#[test]
fn unary_neg_precedence_w15() {
    // `x * -1` ⇒ `x * (-1)`  (abs.flow). Note: in expression position, after
    // `*` a `-` is unary minus (not op-shorthand — that is stage-position only).
    let f = parse_one_fn("fn m() { x * -1 -> ret; }");
    let c = first_chain(&f);
    match &c.head.as_ref().unwrap().kind {
        ExprKind::Binary {
            op: BinOp::Mul,
            rhs,
            ..
        } => {
            assert!(matches!(rhs.kind, ExprKind::Unary { op: UnOp::Neg, .. }));
        }
        other => panic!("expected `x * (-1)`, got {other:?}"),
    }
}

#[test]
fn unary_neg_member_w15() {
    // `-x.f` ⇒ `-(x.f)` : unary looser than postfix member.
    let f = parse_one_fn("fn m() { -x.f -> ret; }");
    let c = first_chain(&f);
    match &c.head.as_ref().unwrap().kind {
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => {
            assert!(matches!(operand.kind, ExprKind::Member { .. }));
        }
        other => panic!("expected `-(x.f)`, got {other:?}"),
    }
}

#[test]
fn not_and_precedence_w15() {
    // `!a && b` ⇒ `(!a) && b`.
    let f = parse_one_fn("fn m() { !a && b -> ret; }");
    let c = first_chain(&f);
    match &c.head.as_ref().unwrap().kind {
        ExprKind::Binary {
            op: BinOp::And,
            lhs,
            ..
        } => {
            assert!(matches!(lhs.kind, ExprKind::Unary { op: UnOp::Not, .. }));
        }
        other => panic!("expected `(!a) && b`, got {other:?}"),
    }
}

#[test]
fn chained_comparison_p0007_w14() {
    // `a < b < c` → P0007 (non-associative); lhs `a < b` kept.
    assert_eq!(pcodes("fn m() { a < b < c -> ret; }"), vec!["P0007"]);
}

#[test]
fn equality_chain_p0007() {
    // `a == b == c` → P0007.
    assert_eq!(pcodes("fn m() { a == b == c -> ret; }"), vec!["P0007"]);
}

#[test]
fn cmp_chain_cascade_single_p0007() {
    // `a < b < c < d` (4-deep chain) → exactly ONE P0007 and ZERO P0001: the
    // recovery loop absorbs every further `CMPOP add` pair under one P0007
    // (DESIGN §17 P0007: one per chain, no cascade).
    let pc = pcodes("fn m() { a < b < c < d -> ret; }");
    assert_eq!(pc.iter().filter(|c| **c == "P0007").count(), 1, "one P0007");
    assert_eq!(pc.iter().filter(|c| **c == "P0001").count(), 0, "no P0001");
}

#[test]
fn op_shorthand_cmp_chain_p0007() {
    // op-shorthand hole climber (level 5) must enforce non-associativity too:
    // `-> < a < b` → exactly one P0007, no generic P0001 (FIX: mirror parse_cmp
    // on the climber path; DESIGN §17 P0007 "enforced on both expression paths").
    let pc = pcodes("fn m() { x -> < a < b; }");
    assert_eq!(pc.iter().filter(|c| **c == "P0007").count(), 1, "one P0007");
    assert_eq!(pc.iter().filter(|c| **c == "P0001").count(), 0, "no P0001");
}

// ======================================================================
// §14.4 classification table + arm-after-`}` lexer gate interplay
// ======================================================================

#[test]
fn guard_block_classification() {
    // abs-style guard block, all arms → StageKind::Guard, zero diags.
    let src = "fn m() { (x > 0) -> { -true-> x; -false-> y; } -> ret; }";
    no_diags(src);
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    assert!(matches!(c.stages[0].kind, StageKind::Guard(_)));
}

#[test]
fn fanout_classification() {
    // `6 -> { -> a; -> b; }` → Fanout with 2 branches.
    let src = "fn m() { 6 -> { -> a; -> b; } sq -> print; }";
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::Fanout {
            kind: FanoutKind::Plain,
            branches,
        } => {
            assert_eq!(branches.len(), 2);
        }
        other => panic!("expected Fanout(2), got {other:?}"),
    }
}

#[test]
fn empty_block_fanout_zero_p0010() {
    // `x -> {}` → Fanout(0) + P0010.
    assert_eq!(pcodes("fn m() { x -> {}; }"), vec!["P0010"]);
}

#[test]
fn arm_after_rbrace_lexer_gate() {
    // sepia-style nested guard whose `-false->` arm has a block payload that
    // ends in `}` then the next arm starts after `}` (lexer gate passes).
    let src = "fn m() { (a) -> { -true-> hi; -false-> { (b) -> { -true-> lo; -false-> v; } -> bounded; bounded } } -> ret; }";
    no_diags(src);
}

// ======================================================================
// ADR-0011 scan cases (§14.5)
// ======================================================================

#[test]
fn struct_literal_heads_chain() {
    // `Pixel { r: nr } -> ret;` — no `;`/arrow/guard inside braces ⇒ struct.
    let src = "fn m() { Pixel { r: nr, g: ng } -> ret; }";
    no_diags(src);
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    assert!(matches!(
        c.head.as_ref().unwrap().kind,
        ExprKind::Struct { .. }
    ));
}

#[test]
fn labeled_loop_p0110() {
    // `search { x -> y; }` — arrows inside ⇒ labeled loop (P0110).
    let src = "fn m() { search { x -> y; } }";
    assert_eq!(pcodes(src), vec!["P0110"]);
    let f = parse_one_fn(src);
    match &f.body.items[0] {
        BlockItem::Stmt(Stmt {
            kind:
                StmtKind::Loop(LoopStmt {
                    label: LoopLabel::Custom(_),
                    ..
                }),
            ..
        }) => {}
        other => panic!("expected a custom-labeled loop, got {other:?}"),
    }
}

#[test]
fn labeled_loop_arrows_no_top_level_semi() {
    // Pinned regression: `search`-labeled loop whose body has arrows but no
    // top-level `;` ⇒ P0110 via the four-token scan.
    let src = "fn m() { search { x -> ret } }";
    assert_eq!(pcodes(src), vec!["P0110"]);
}

#[test]
fn empty_braces_struct_w13() {
    // `X { }` statement-initial → struct literal (no loop tokens). It becomes a
    // bare-expression tail of the fn body (no `;`), so no P0003.
    let src = "fn m() { X { } }";
    no_diags(src);
    let f = parse_one_fn(src);
    assert!(f.body.tail.is_some(), "struct-literal tail");
}

#[test]
fn flow_free_labeled_body_is_struct_w13() {
    // `X { x }` — no `;`/arrow/guard ⇒ struct literal (operationally-empty loop).
    let src = "fn m() { X { x } }";
    no_diags(src);
}

#[test]
fn demoted_scan_hint_mentions_sigil() {
    // The demoted ADR-0011 scan (§14.5) still parses a loop-shaped un-sigiled
    // `Ident {` as a labeled block for recovery, but its P0110 message now points
    // at the sigiled spelling `:search { … }`.
    let src = "fn m() { search { x -> y; } }";
    assert_eq!(pcodes(src), vec!["P0110"]);
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0110")
        .expect("a P0110");
    assert!(
        d.message.contains(":search { … }"),
        "demoted-scan P0110 should hint the sigiled spelling, got {:?}",
        d.message
    );
}

// ======================================================================
// ADR-0012 sigiled labeled blocks (`:label { … }`) and jumps (`-> :label`)
// ======================================================================

#[test]
fn sigiled_labeled_block_p0110() {
    // `:search { x -> y; }` statement ⇒ P0110, parsed as a custom-labeled loop.
    let src = "fn m() { :search { x -> y; } }";
    assert_eq!(pcodes(src), vec!["P0110"]);
    let f = parse_one_fn(src);
    match &f.body.items[0] {
        BlockItem::Stmt(Stmt {
            kind:
                StmtKind::Loop(LoopStmt {
                    label: LoopLabel::Custom(name),
                    span,
                    ..
                }),
            ..
        }) => {
            // Span convention: the `Custom` Name span is the bare identifier
            // (`search`), while the `LoopStmt` span begins at the `:` sigil.
            assert_eq!(
                &src[name.span.start as usize..name.span.end as usize],
                "search"
            );
            assert_eq!(&src[span.start as usize..span.start as usize + 1], ":");
        }
        other => panic!("expected a custom-labeled loop, got {other:?}"),
    }
}

#[test]
fn sigiled_labeled_block_diag_covers_sigil() {
    // The P0110 diagnostic span covers `:search` (sigil + ident).
    let src = "fn m() { :search { x -> y; } }";
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0110")
        .expect("a P0110");
    assert_eq!(&src[d.span.start as usize..d.span.end as usize], ":search");
}

#[test]
fn labeled_jump_p0110_chain_terminates() {
    // `-> :search;` in a chain ⇒ one P0110; the `;` terminates the chain normally
    // and the surrounding fn body keeps parsing.
    let src = "fn m() { x -> :search; y -> ret; }";
    assert_eq!(pcodes(src), vec!["P0110"]);
    let f = parse_one_fn(src);
    // Two statements survived: the jump chain and the `y -> ret;` chain.
    assert_eq!(f.body.items.len(), 2, "chain terminated, both stmts parsed");
    // The jump stage is an Error stage; the `;` terminated the chain.
    let c = first_chain(&f);
    assert!(matches!(c.stages.last().unwrap().kind, StageKind::Error(_)));
}

#[test]
fn labeled_jump_diag_covers_sigil() {
    let src = "fn m() { x -> :search; }";
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0110")
        .expect("a P0110");
    assert_eq!(&src[d.span.start as usize..d.span.end as usize], ":search");
}

#[test]
fn statement_initial_colon_non_ident_is_generic_error() {
    // A statement-initial `:` NOT followed by `Ident {` is NOT a label: keep the
    // pre-existing generic-error behavior (P0001), never P0110.
    let src = "fn m() { : 5 -> x; }";
    let codes = pcodes(src);
    assert!(
        !codes.contains(&"P0110"),
        "no P0110 for non-label `:`, got {codes:?}"
    );
    assert!(
        codes.contains(&"P0001"),
        "generic error expected, got {codes:?}"
    );
}

#[test]
fn path_colons_after_arrow_not_a_label() {
    // `x -> ::weird;` — a `::` path (P0109), NOT a labeled jump (no P0110). The
    // label form requires `Colon Ident`; a second `Colon` excludes it.
    let src = "fn m() { x -> ::weird; }";
    let codes = pcodes(src);
    assert!(
        !codes.contains(&"P0110"),
        "`::` is not a label, got {codes:?}"
    );
}

#[test]
fn nested_labeled_blocks_and_jump() {
    // `:outer { :inner { -> :inner; } }` — P0110 per labeled block (×2) plus
    // P0110 per labeled jump (×1) = three, and no panic. The headless `-> :inner;`
    // chain is the inner block's content.
    let src = "fn m() { :outer { :inner { -> :inner; } } }";
    assert_eq!(pcodes(src), vec!["P0110", "P0110", "P0110"]);
    assert_spans(src);
}

#[test]
fn unsigiled_jump_is_plain_var_stage_w25() {
    // W25: an un-sigiled `-> search;` is a plain name stage (variable flow), never
    // a jump — zero diagnostics.
    let src = "fn m() { x -> search; }";
    no_diags(src);
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    match &c.stages.last().unwrap().kind {
        StageKind::Expr(Expr {
            kind: ExprKind::Var(_),
            ..
        }) => {}
        other => panic!("expected a plain Var stage, got {other:?}"),
    }
}

// ======================================================================
// termination-rule matrix (W10, W11): `;`, `}`, block-final, `};`
// ======================================================================

#[test]
fn term_semicolon_statement() {
    no_diags("fn m() { x -> y; }");
}

#[test]
fn term_block_tail_no_semi() {
    // A chain without `;` ending at `}` is the block's tail.
    let f = parse_one_fn("fn m() { x -> y }");
    assert!(f.body.tail.is_some());
    assert!(f.body.items.is_empty());
}

#[test]
fn term_block_final_stage_no_semi() {
    // fanout.flow: `6 -> { … }` (block-final) followed by another statement
    // with no `;` after the brace.
    let src = "fn m() { 6 -> { -> a; -> b; } sq -> print; }";
    no_diags(src);
}

#[test]
fn term_block_final_with_semi_w10() {
    // `};` also legal (user-guide §5.2).
    let src = "fn m() { 6 -> { -> a; -> b; }; sq -> print; }";
    no_diags(src);
}

#[test]
fn term_missing_semicolon_p0001() {
    // `x -> y z` (two chains, no `;`) → P0001 expected `;`.
    let src = "fn m() { x -> y z -> w; }";
    assert!(pcodes(src).contains(&"P0001"));
}

#[test]
fn bare_expr_statement_p0003() {
    // A bare-expression chain as a `;`-terminated statement → P0003.
    assert_eq!(pcodes("fn m() { x; y -> ret; }"), vec!["P0003"]);
}

#[test]
fn bare_expr_tail_ok() {
    // The same bare expression as a block tail is legal.
    no_diags("fn m() { x }");
}

// ======================================================================
// trailing commas (DESIGN §14.6)
// ======================================================================

#[test]
fn trailing_comma_struct_decl_and_literal() {
    no_diags("type P { r: f32, g: f32, b: f32, }");
    no_diags("fn m() { P { r: a, g: b, } }");
}

#[test]
fn trailing_comma_array_and_tuple() {
    no_diags("fn m() { [1, 2, 3,] -> v; }");
    no_diags("fn m() { (a, b,) -> v; }");
}

#[test]
fn trailing_comma_params() {
    no_diags("fn f(a: i32, b: i32,) { a -> ret; }");
}

/// Extract the single MapFold stage from the first chain of a fn body.
fn first_map_fold(f: &FnDecl) -> (&[Name], &Block) {
    let c = first_chain(f);
    for st in &c.stages {
        if let StageKind::MapFold { params, body, .. } = &st.kind {
            return (params, body);
        }
    }
    panic!("expected a map/fold stage in the first chain");
}

#[test]
fn op_block_param_no_trailing_comma() {
    // op-block params: §14.3 grammar permits NO trailing comma before `->`. The
    // parser rejects it with P0001 but keeps the parsed param `x` and continues
    // recovery (the `->` and body still parse) — no panic, deterministic.
    let src = "fn m() { x -> map { x, -> a; } -> v; }";
    assert!(
        pcodes(src).contains(&"P0001"),
        "trailing-comma op-block param → P0001: {:?}",
        pcodes(src)
    );
    let f = parse_one_fn(src);
    let (params, _body) = first_map_fold(&f);
    assert_eq!(params.len(), 1, "the one param `x` is kept after recovery");
    let p = params[0].span;
    assert_eq!(&src[p.start as usize..p.end as usize], "x", "param is `x`");
}

#[test]
fn op_block_zero_params_p0001() {
    // §14.3: an op-block needs at least one parameter; `map { -> … }` has none →
    // P0001, recovered (the body still parses, MapFold kept with zero params).
    let src = "fn m() { x -> map { -> a; } -> v; }";
    assert!(
        pcodes(src).contains(&"P0001"),
        "zero-param op-block → P0001: {:?}",
        pcodes(src)
    );
    let f = parse_one_fn(src);
    let (params, _body) = first_map_fold(&f);
    assert!(params.is_empty(), "no params parsed");
}

// ======================================================================
// ret.0 targets, ret / loop
// ======================================================================

#[test]
fn ret_projection() {
    // `-> ret.0`.
    let f = parse_one_fn("fn m() { x -> ret.0; }");
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::Ret { proj: Some((0, _)) } => {}
        other => panic!("expected ret.0, got {other:?}"),
    }
}

#[test]
fn ret_plain() {
    let f = parse_one_fn("fn m() { x -> ret; }");
    let c = first_chain(&f);
    assert!(matches!(c.stages[0].kind, StageKind::Ret { proj: None }));
}

#[test]
fn loop_jump_stage() {
    let f = parse_one_fn("fn m() { loop { -> loop; } }");
    match &f.body.items[0] {
        BlockItem::Stmt(Stmt {
            kind: StmtKind::Loop(l),
            ..
        }) => {
            let inner = &l.body.items[0];
            match inner {
                BlockItem::Stmt(Stmt {
                    kind: StmtKind::Chain(c),
                    ..
                }) => assert!(matches!(c.stages[0].kind, StageKind::LoopJump)),
                other => panic!("expected `-> loop;` chain, got {other:?}"),
            }
        }
        other => panic!("expected loop, got {other:?}"),
    }
}

#[test]
fn ret_as_chain_head_p0001_w20() {
    // `ret -> f;` → P0001 (ret is a stage form, not an expression).
    assert!(pcodes("fn m() { ret -> f; }").contains(&"P0001"));
}

// ======================================================================
// guard-payload F-matrix (expr / chain / block / headless / nested-guard)
// ======================================================================

#[test]
fn payload_bare_expr() {
    // `-true-> x;`
    no_diags("fn m() { (a) -> { -true-> x; -false-> y; } -> ret; }");
}

#[test]
fn payload_chain() {
    // `-false-> acc -> ret;` (sum_to_n / fir form).
    no_diags("fn m() { (a) -> { -true-> hi; -false-> acc -> ret; } }");
}

#[test]
fn payload_headless_chain() {
    // `-false-> -> ret;` (headless payload).
    no_diags("fn m() { (a) -> { -true-> hi; -false-> -> ret; } }");
}

#[test]
fn payload_block() {
    // `-true-> { … -> loop; }` mixes headed and headless statements legally.
    let src = "fn m() { loop { (a) -> { -true-> { x -> y; -> loop; } -false-> z -> ret; } } }";
    no_diags(src);
}

#[test]
fn payload_nested_guard() {
    // sepia: a `-false->` arm whose block payload contains another guard.
    let src = "fn m() { (a) -> { -true-> hi; -false-> { (b) -> { -true-> lo; -false-> v; } -> bounded; bounded } } -> ret; }";
    no_diags(src);
}

// ======================================================================
// §17 ledger rows
// ======================================================================

#[test]
fn w12_op_shorthand_minus_is_subtraction() {
    // `-> - 5` is subtraction shorthand: Binary(Sub, Hole, Int 5).
    let f = parse_one_fn("fn m() { x -> - 5; }");
    let c = first_chain(&f);
    match &c.stages[0].kind {
        StageKind::OpShorthand {
            expr:
                Expr {
                    kind:
                        ExprKind::Binary {
                            op: BinOp::Sub,
                            lhs,
                            ..
                        },
                    ..
                },
        } => assert!(matches!(lhs.kind, ExprKind::Hole)),
        other => panic!("expected `· - 5`, got {other:?}"),
    }
}

#[test]
fn w17_double_bind_p0008() {
    // `a <- b <- c` → P0008.
    assert!(pcodes("fn m() { x <- a <- b; }").contains(&"P0008"));
}

#[test]
fn w18_mixed_direction_p0001() {
    // `a -> b <- c` → P0001.
    assert!(pcodes("fn m() { a -> b <- c; }").contains(&"P0001"));
}

#[test]
fn w20_loop_in_expr_p0001() {
    // `loop + 1` → P0001 (loop is a stage form, not an expression).
    assert!(pcodes("fn m() { x -> y; loop + 1 -> z; }").contains(&"P0001"));
}

#[test]
fn w21_named_param_partial_is_member() {
    // `15 -> add.a;` parses as a member-expression stage (no special case).
    let f = parse_one_fn("fn m() { 15 -> add.a; }");
    let c = first_chain(&f);
    assert!(matches!(
        c.stages[0].kind,
        StageKind::Expr(Expr {
            kind: ExprKind::Member { .. },
            ..
        })
    ));
}

#[test]
fn w24_all_spaced_guard_block() {
    // Pinned regression: a block written entirely with spaced arms still
    // classifies as a GuardBlock; P0005 per arm, no P0006.
    let src = "fn m() { (a) -> { - true -> x; - false -> y; } -> ret; }";
    let pc = pcodes(src);
    assert_eq!(pc.iter().filter(|c| **c == "P0005").count(), 2, "P0005 ×2");
    assert!(!pc.contains(&"P0006"), "no mixing diagnostic: {pc:?}");
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    assert!(
        matches!(c.stages[0].kind, StageKind::Guard(_)),
        "GuardBlock"
    );
}

// ======================================================================
// stray guard W1, spaced guard, mixing
// ======================================================================

#[test]
fn stray_guard_p0004() {
    // `-7-> x;` at statement-initial position (after `{`), no guard block → a
    // stray guard. P0004 with a source-sliced fix.
    let src = "fn m() { -7-> x; }";
    let out = parse(src);
    let pc: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| d.code.0 == "P0004")
        .collect();
    assert_eq!(pc.len(), 1, "exactly one P0004");
    let fix = pc[0].fix.as_ref().expect("P0004 has a fix");
    // The fix slices the source lexeme minus the trailing `->`, plus ` ->`.
    assert_eq!(fix.replacement, "-7 ->");
}

#[test]
fn stray_guard_over_u64_slices_source() {
    // Over-u64 stray guard: the fix must slice the source lexeme verbatim, never
    // re-render from the clamped GuardKind::Int (which would rewrite the literal).
    let src = "fn m() { -99999999999999999999-> x; }";
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0004")
        .expect("P0004");
    let fix = d.fix.as_ref().expect("fix");
    assert_eq!(fix.replacement, "-99999999999999999999 ->");
    // The lexer's L0008 is preserved and NOT duplicated (C14/J6).
    assert_eq!(codes(src).iter().filter(|c| **c == "L0008").count(), 1);
}

#[test]
fn spaced_guard_p0005_recovered_as_arm() {
    // `- true ->` inside a guard block → P0005, recovered as the meant arm.
    let src = "fn m() { (a) -> { - true -> x; -false-> y; } -> ret; }";
    assert!(pcodes(src).contains(&"P0005"));
}

#[test]
fn mixing_statement_among_arms_p0006() {
    // Pinned regression: `- 7 -> x;` (a statement) among arms → statement + P0006
    // with the bidirectional hint.
    let src = "fn m() { (a) -> { -true-> x; - 7 -> y; } -> ret; }";
    let pc = pcodes(src);
    assert!(pc.contains(&"P0006"), "P0006 present: {pc:?}");
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0006")
        .unwrap();
    assert!(
        d.message.contains("-7->"),
        "bidirectional hint mentions `-7->`: {}",
        d.message
    );
}

// ======================================================================
// category recovery, `i<-1`, pinned regressions
// ======================================================================

#[test]
fn category_decl_recovery_no_double_report() {
    // `category Foo { … }` — L0004 already emitted by the lexer; the parser
    // parses it as a `type` with NO new diagnostic. Only one L0004, no P-code.
    let src = "category Foo { r: f32 }";
    let out = parse(src);
    let lcount = out
        .diagnostics
        .iter()
        .filter(|d| d.code.0 == "L0004")
        .count();
    assert_eq!(lcount, 1, "exactly one L0004 (no double report)");
    assert!(
        out.diagnostics.iter().all(|d| d.code.0 == "L0004"),
        "no new diagnostic from the parser: {:#?}",
        out.diagnostics
    );
    // And it parses as a Type item.
    assert!(matches!(out.program.items[0], Item::Type(_)));
}

#[test]
fn i_backarrow_one_p0009() {
    // `i<-1` inside an expression position → P0009 with the W2 hint.
    let src = "fn m() { (i<-1) -> ret; }";
    let out = parse(src);
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.code.0 == "P0009")
        .expect("P0009");
    assert!(d.fix.is_some(), "P0009 has the `< -` fix");
}

#[test]
fn pattern_arm_only_block_p0106_twice() {
    // Pinned regression: `opt -> { -Some(x)-> y; -None-> 0; }` ⇒ P0106 ×2, no
    // spurious P0108.
    let src = "fn m() { opt -> { -Some(x)-> y; -None-> 0; } -> ret; }";
    let pc = pcodes(src);
    assert_eq!(
        pc.iter().filter(|c| **c == "P0106").count(),
        2,
        "P0106 ×2: {pc:?}"
    );
    assert!(!pc.contains(&"P0108"), "no spurious P0108: {pc:?}");
}

#[test]
fn rest_element_exactly_p0107() {
    // Pinned regression: `[head, ...tail]` ⇒ exactly P0107.
    let src = "fn m() { [head, ...tail] -> rest; }";
    assert_eq!(pcodes(src), vec!["P0107"]);
}

#[test]
fn list_map_path_exactly_p0109_chain_continues() {
    // Pinned regression: `List::map -> out;` ⇒ exactly P0109, chain continues.
    let src = "fn m() { data -> List::map -> out; }";
    assert_eq!(pcodes(src), vec!["P0109"]);
    // The chain still has the two stages after recovery.
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    assert_eq!(c.stages.len(), 2, "chain continued past the `::` path");
}

#[test]
fn call_expression_p0108() {
    // `arr.len()` → P0108.
    assert!(pcodes("fn m() { x -> arr.len(); }").contains(&"P0108"));
}

#[test]
fn question_postfix_p0101() {
    // `f? -> g?` → P0101 ×2, one per `?`.
    let src = "fn m() { x -> f? -> g?; }";
    assert_eq!(pcodes(src).iter().filter(|c| **c == "P0101").count(), 2);
}

#[test]
fn annotation_p0102_item_kept() {
    // `@executor(ThreadPool) fn …` → P0102, the fn is kept.
    let src = "@executor(ThreadPool)\nfn m() { x -> ret; }";
    assert!(pcodes(src).contains(&"P0102"));
    let out = parse(src);
    assert!(
        out.program.items.iter().any(|i| matches!(i, Item::Fn(_))),
        "the annotated fn is kept"
    );
}

// ======================================================================
// totality / depth (J1) + determinism
// ======================================================================

#[test]
fn deep_nesting_p0011_no_panic() {
    // 300 nested groups exceed the depth limit; P0011 fires, no panic (J1).
    let src = format!(
        "fn m() {{ {}1{} -> ret; }}",
        "(".repeat(300),
        ")".repeat(300)
    );
    let out = parse(&src);
    assert!(
        out.diagnostics.iter().any(|d| d.code.0 == "P0011"),
        "expected P0011 on deep nesting"
    );
}

#[test]
fn deep_array_type_p0011_no_panic() {
    // J1 totality: a type with 300 nested `[` exceeds the depth limit (128). The
    // shared depth guard must fire P0011 and unwind without a stack overflow.
    let src = format!(
        "fn f(p: {}i32{}) {{ p -> ret; }}",
        "[".repeat(300),
        "]".repeat(300)
    );
    let out = parse(&src);
    assert!(
        out.diagnostics.iter().any(|d| d.code.0 == "P0011"),
        "expected P0011 on a deeply nested array type"
    );
}

#[test]
fn deep_tuple_type_p0011_no_panic() {
    // J1 totality: a type with 300 nested `(` tuple-types likewise hits the
    // shared depth guard → P0011, no panic.
    let src = format!(
        "fn f(p: {}i32, i32{}) {{ p -> ret; }}",
        "(".repeat(300),
        ")".repeat(300)
    );
    let out = parse(&src);
    assert!(
        out.diagnostics.iter().any(|d| d.code.0 == "P0011"),
        "expected P0011 on a deeply nested tuple type"
    );
}

#[test]
fn determinism() {
    let src = "fn m() { (x > 0) -> { -true-> x; -false-> y; } -> ret; }";
    assert_eq!(parse(src), parse(src));
}

#[test]
fn empty_input_no_panic() {
    let out = parse("");
    assert!(out.program.items.is_empty());
    assert!(out.diagnostics.is_empty());
}

#[test]
fn top_level_statement_p0012() {
    // A statement at the top level → P0012.
    assert!(pcodes("x -> y;").contains(&"P0012"));
}

#[test]
fn lex_diags_preserved_j6() {
    // J6: parse diagnostics ⊇ lex diagnostics. The over-u64 int literal's L0008
    // survives into parse output.
    let src = "fn m() { 99999999999999999999999999 -> ret; }";
    let pout = parse(src);
    let lout = crate::lex(src);
    for ld in &lout.diagnostics {
        assert!(
            pout.diagnostics.contains(ld),
            "lex diagnostic not preserved: {ld:?}"
        );
    }
}

#[test]
fn int_clamp_no_double_report_c14() {
    // An over-u64 int literal: lexer emits L0008 once; the parser re-parses the
    // span and clamps silently (no new diagnostic).
    let src = "fn m() { 99999999999999999999999999 -> ret; }";
    assert_eq!(codes(src).iter().filter(|c| **c == "L0008").count(), 1);
    let f = parse_one_fn(src);
    let c = first_chain(&f);
    assert!(matches!(
        c.head.as_ref().unwrap().kind,
        ExprKind::Int(u64::MAX)
    ));
}

// ======================================================================
// diagnostics merge + sort (DESIGN §15/§18)
// ======================================================================

#[test]
fn diagnostics_sorted_by_span() {
    let src = "fn m() { x -> f? -> g?; }";
    let out = parse(src);
    let spans: Vec<(u32, u32)> = out
        .diagnostics
        .iter()
        .map(|d| (d.span.start, d.span.end))
        .collect();
    let mut sorted = spans.clone();
    sorted.sort_unstable();
    assert_eq!(spans, sorted, "diagnostics stably sorted by (start, end)");
}

#[test]
fn line_index_span_starts() {
    // The `?` postfix on line 2 is the sole diagnostic (P0101). Its span start
    // must map to the exact 1-based line:col of the `?` in the source. Line 1 is
    // `fn m() {`, line 2 is `  x -> f?;` — the `?` is the 9th column of line 2.
    let src = "fn m() {\n  x -> f?;\n}";
    let out = parse(src);
    assert_eq!(out.diagnostics.len(), 1, "exactly one diagnostic");
    let d = &out.diagnostics[0];
    assert_eq!(d.code.0, "P0101");
    let idx = LineIndex::new(src);
    let lc = idx.line_col(d.span.start);
    assert_eq!((lc.line, lc.col), (2, 9), "`?` is at line 2, col 9");
}
