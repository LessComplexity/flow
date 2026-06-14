//! Golden Mermaid dumps (DESIGN §16 item 2). Each snapshot is read against
//! DESIGN §14 (format) and §16 (the graph shape it claims) before accepting —
//! wrong-but-stable is the known failure mode. Every dump also passes
//! `lint_mermaid`.

use flow_ir::{
    CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, lint_mermaid,
};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// Assert a dump lints clean, then snapshot it.
fn check(name: &str, ir: &CategoryIr) {
    let dump = ir.to_mermaid();
    let lints = lint_mermaid(&dump);
    assert!(lints.is_empty(), "lint failures for {name}: {lints:?}");
    insta::assert_snapshot!(name, dump);
}

/// (a) §4.1 `a + b` — two i32 params projected from a tuple input, added to ret.
#[test]
fn golden_a_add() {
    let input = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "add", input, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(Some("a".into())), L).unwrap();
        let bb = fb.proj(p, 1, Dest::Fresh(Some("b".into())), L).unwrap();
        fb.binop(Operation::Add, a, bb, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("a_add", &ir);
}

/// (b) §4.3 pipeline `data * 2 -> + 5 -> * 3 -> ret`.
#[test]
fn golden_b_pipeline() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "pipe", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let data = fb.input();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let s1 = fb
            .binop(Operation::Mul, data, two, Dest::Fresh(None), L)
            .unwrap();
        let five = fb.constant(Value::I32(5), L).unwrap();
        let s2 = fb
            .binop(Operation::Add, s1, five, Dest::Fresh(None), L)
            .unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        fb.binop(Operation::Mul, s2, three, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("b_pipeline", &ir);
}

/// (c) §4.4 conditional with Phi: `cond ? a : b`.
#[test]
fn golden_c_phi() {
    let input = Ty::Tuple(vec![Ty::i32(), Ty::i32(), Ty::Bool]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "sel", input, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(Some("a".into())), L).unwrap();
        let bb = fb.proj(p, 1, Dest::Fresh(Some("b".into())), L).unwrap();
        let c = fb.proj(p, 2, Dest::Fresh(Some("c".into())), L).unwrap();
        fb.phi(a, bb, c, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("c_phi", &ir);
}

/// (d) §4.5 loop — canonical two-route form, `B = U` (i32 counting loop).
#[test]
fn golden_d_loop_b_eq_u() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "count", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let merge = fb.merge_of(&lh);
        let ten = fb.constant(Value::I32(10), L).unwrap();
        let cond = fb
            .binop(
                Operation::Lt,
                merge,
                ten,
                Dest::Fresh(Some("cond".into())),
                L,
            )
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(
                Operation::Add,
                merge,
                one,
                Dest::Fresh(Some("inext".into())),
                L,
            )
            .unwrap();
        fb.loop_back(&lh, inext, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("d_loop_b_eq_u", &ir);
}

/// (d') general `B ≠ U` shape: U = (i32, i32) carried, exit payload is just one
/// i32 (B = i32).
#[test]
fn golden_d_loop_b_ne_u() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "sum_to_n", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        // carried (i, acc) starting (1, 0).
        let one0 = fb.constant(Value::I32(1), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let init = fb
            .pack(&[one0, zero], Dest::Fresh(Some("init".into())), L)
            .unwrap();
        let lh = fb.begin_loop(init, L).unwrap();
        let merge = fb.merge_of(&lh);
        let i = fb.proj(merge, 0, Dest::Fresh(Some("i".into())), L).unwrap();
        let acc = fb
            .proj(merge, 1, Dest::Fresh(Some("acc".into())), L)
            .unwrap();
        // guard i <= n  (use input n).
        let n = fb.input();
        let cond = fb
            .binop(Operation::Le, i, n, Dest::Fresh(Some("cond".into())), L)
            .unwrap();
        // body: i' = i+1, acc' = acc + i.
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(Operation::Add, i, one, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        let accnext = fb
            .binop(
                Operation::Add,
                acc,
                i,
                Dest::Fresh(Some("accnext".into())),
                L,
            )
            .unwrap();
        let next = fb
            .pack(&[inext, accnext], Dest::Fresh(Some("next".into())), L)
            .unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        // exit payload is the merge-view acc (B = i32 ≠ U).
        fb.loop_exit(&lh, acc, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("d_loop_b_ne_u", &ir);
}

/// (e) Appendix-B-shaped composite: a struct pack of two scalars.
#[test]
fn golden_e_struct_composite() {
    let pt = Ty::Struct {
        name: "Point".into(),
        fields: vec![("x".into(), Ty::f32()), ("y".into(), Ty::f32())],
    };
    let input = Ty::Tuple(vec![Ty::f32(), Ty::f32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "mk_point", input, pt.clone(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let x = fb.proj(p, 0, Dest::Fresh(Some("x".into())), L).unwrap();
        let y = fb.proj(p, 1, Dest::Fresh(Some("y".into())), L).unwrap();
        fb.pack_struct(pt, &[x, y], Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("e_struct_composite", &ir);
}

/// (f1) fanout — separately-bound outputs: two independent chains, NO join.
#[test]
fn golden_f_fanout_no_join() {
    let out = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "fan", Ty::i32(), out, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        // chain 1 → ret.0, chain 2 → ret.1; no join object.
        fb.binop(Operation::Mul, x, two, Dest::Ret { slot: Some(0) }, L)
            .unwrap();
        fb.binop(Operation::Mul, x, three, Dest::Ret { slot: Some(1) }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("f_fanout_no_join", &ir);
}

/// (f2) fanout — tupled outputs: a pack join object.
#[test]
fn golden_f_fanout_join() {
    let out = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "fan2", Ty::i32(), out, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let three = fb.constant(Value::I32(3), L).unwrap();
        let a = fb
            .binop(Operation::Mul, x, two, Dest::Fresh(Some("a".into())), L)
            .unwrap();
        let c = fb
            .binop(Operation::Mul, x, three, Dest::Fresh(Some("c".into())), L)
            .unwrap();
        // pack join → ret.
        fb.pack(&[a, c], Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("f_fanout_join", &ir);
}

/// (g) two sequential prints — token chain, `main : IoToken → IoToken`.
#[test]
fn golden_g_two_prints() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s1 = fb.constant(Value::Str("hello".into()), L).unwrap();
        let t1 = fb.print(t0, s1, L).unwrap();
        let s2 = fb.constant(Value::Str("world".into()), L).unwrap();
        let t2 = fb.print(t1, s2, L).unwrap();
        fb.output(t2, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("g_two_prints", &ir);
}

/// (g') `println` (ADR-0015) renders a `Println` edge label and lints clean.
#[test]
fn println_renders_println_label() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s1 = fb.constant(Value::Str("hi".into()), L).unwrap();
        let t1 = fb.println(t0, s1, L).unwrap();
        fb.output(t1, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let dump = ir.to_mermaid();
    assert!(
        dump.contains("Println"),
        "println must render a `Println` edge label"
    );
    assert!(
        lint_mermaid(&dump).is_empty(),
        "println dump must lint clean"
    );
}

/// (h) print-inside-loop in a Unit-surface fn: token-bearing U, token-bearing
/// exit (the F2/L2-01 case).
#[test]
fn golden_h_print_in_loop() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        // U = (i32, IoToken).
        let init = fb
            .pack(&[zero, t0], Dest::Fresh(Some("init".into())), L)
            .unwrap();
        let lh = fb.begin_loop(init, L).unwrap();
        let merge = fb.merge_of(&lh);
        let i = fb.proj(merge, 0, Dest::Fresh(Some("i".into())), L).unwrap();
        let tok = fb
            .proj(merge, 1, Dest::Fresh(Some("tok".into())), L)
            .unwrap();
        // print i.
        let tok2 = fb.print(tok, i, L).unwrap();
        // guard i < 3.
        let three = fb.constant(Value::I32(3), L).unwrap();
        let cond = fb
            .binop(Operation::Lt, i, three, Dest::Fresh(Some("cond".into())), L)
            .unwrap();
        // body i' = i + 1; next = (i', tok2).
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(Operation::Add, i, one, Dest::Fresh(Some("inext".into())), L)
            .unwrap();
        let next = fb
            .pack(&[inext, tok2], Dest::Fresh(Some("next".into())), L)
            .unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        // The loop fork: tok2 forks into the back route (in `next`) and the exit
        // route — exactly two token consumers, both Pair edges (the §8 sanctioned
        // exception). The exit payload B = IoToken carries the token out.
        let exit_tok = fb
            .loop_exit(&lh, tok2, cond, Dest::Fresh(Some("exit_tok".into())), L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        // Thread the escaped token to Return (token-in ⇒ token-out, §8).
        fb.output(exit_tok, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("h_print_in_loop", &ir);
}

/// (i) a 3-way value-guard Phi chain (right-folded): match k against 0/1, else d.
#[test]
fn golden_i_phi_chain() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "vmatch", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let k = fb.input();
        let e0 = fb.constant(Value::I32(100), L).unwrap();
        let e1 = fb.constant(Value::I32(200), L).unwrap();
        let ed = fb.constant(Value::I32(999), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let c0 = fb
            .binop(Operation::Eq, k, zero, Dest::Fresh(Some("c0".into())), L)
            .unwrap();
        let c1 = fb
            .binop(Operation::Eq, k, one, Dest::Fresh(Some("c1".into())), L)
            .unwrap();
        // inner = phi(e1, ed, c1); outer = phi(e0, inner, c0).
        let inner = fb
            .phi(e1, ed, c1, Dest::Fresh(Some("inner".into())), L)
            .unwrap();
        fb.phi(e0, inner, c0, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    check("i_phi_chain", &ir);
}

/// F5: a valid single-`-->` dump whose LABEL contains `---` (a `Str` constant
/// `"a---b"`, rendered as a node label) must NOT be flagged by the arrow-style
/// lint — the check is confined to the structural arrow position, not label
/// content (DESIGN §14 lint group 2).
#[test]
fn lint_ignores_dashes_inside_a_str_label() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s = fb.constant(Value::Str("a---b".into()), L).unwrap();
        let t1 = fb.print(t0, s, L).unwrap();
        fb.output(t1, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let dump = ir.to_mermaid();
    // The dump genuinely contains `---` inside the quoted Str label.
    assert!(dump.contains("a---b"));
    // ...but it is valid Mermaid with the single `-->` arrow style, so the lint
    // is clean.
    let lints = lint_mermaid(&dump);
    assert!(lints.is_empty(), "false-flagged label content: {lints:?}");
}

/// F5 guard: a STRUCTURAL `---` link (outside any label) is still flagged — the
/// fix must not blind the lint to a real disallowed arrow style.
#[test]
fn lint_still_flags_structural_dashes() {
    let bad = "flowchart LR\n    a --- b\n";
    let lints = lint_mermaid(bad);
    assert!(
        lints.iter().any(|m| m.contains("`---`")),
        "structural `---` must still be flagged: {lints:?}"
    );
    // And a disallowed dashed/thick arrow outside a label is still caught.
    let bad2 = "flowchart LR\n    a -.-> b\n";
    assert!(lint_mermaid(bad2).iter().any(|m| m.contains("arrow style")));
}
