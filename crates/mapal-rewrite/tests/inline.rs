//! Region-emission plan Move 1 — the `inline` strip pass: directed pins +
//! the R1 proptest battery (raw vs inlined against the interp oracle).
//!
//! Covered rows: a two-fn program (entry calls a helper twice) inlines to one
//! graph (orphan callee dropped); an oversized callee (> `INLINE_MAX_BODY`)
//! stays a call, a callee exactly at the cap inlines (the `≤` boundary); the
//! entry fn is never inlined (a `Call` cycle — the fourth guard — is
//! unrepresentable: the builder rejects recursive Calls at seal); a
//! diamond-shared callee inlines per site (the documented duplication
//! policy); loop-bearing callees stay calls; tuple (slot-wise) returns and
//! direct-to-Return call sites substitute correctly; Calls inside collection
//! bodies strip; determinism (L2, byte-identical mermaid) + idempotence
//! (inline ∘ inline = inline).

mod testgen;

use mapal_interp::{Outcome, RValue, RunResult, eval_call, run};
use mapal_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};
use mapal_rewrite::{INLINE_MAX_BODY, PassId, analyze_inline, replay, rewrite, rewrite_with};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use testgen::{Prog, build, prog_strategy};

const BUDGET: u64 = 200_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// Count `Call` morphisms in a graph.
fn call_count(ir: &CategoryIr) -> usize {
    ir.morphisms()
        .filter(|(_, m)| matches!(m.op, Operation::Call(_)))
        .count()
}

/// Count morphisms with a given payload-less op.
fn op_count(ir: &CategoryIr, op: Operation) -> usize {
    ir.morphisms().filter(|(_, m)| m.op == op).count()
}

/// An i32 scalar `RValue`.
fn scalar(x: i32) -> RValue {
    RValue::Scalar(Value::I32(x))
}

fn arr(xs: &[i32]) -> RValue {
    RValue::array(xs.iter().map(|&x| scalar(x)).collect())
}

fn arr_ty(size: u64) -> Ty {
    Ty::Array {
        elem: Box::new(Ty::i32()),
        size,
    }
}

/// R1's `≈` on whole-program runs (same shape as `property.rs`).
fn approx(a: &RunResult, b: &RunResult) -> bool {
    match (&a.outcome, &b.outcome) {
        (Outcome::Done(x), Outcome::Done(y)) => x == y && a.output == b.output,
        (Outcome::Trapped(_), Outcome::Trapped(_)) => true,
        (Outcome::Diverged, Outcome::Diverged) => true,
        _ => false,
    }
}

/// A Named `x -> x <op> x`, writing Return directly.
fn self_binop_fn(b: &mut IrBuilder, name: &str, op: Operation) -> mapal_ir::FuncId {
    let f = b
        .declare(FuncKind::Named, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    fb.binop(op, x, x, Dest::Ret { slot: None }, L).unwrap();
    fb.finish().unwrap();
    f
}

// --- two-fn program inlines to one graph ------------------------------------

#[test]
fn two_fn_program_inlines_to_one_graph() {
    // double(x) = x + x; main(x) = double(x) * double(x)  ⇒  x=3 → 36
    let mut b = IrBuilder::new();
    let double = self_binop_fn(&mut b, "double", Operation::Add);
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        let p = fb
            .call(double, x, Dest::Fresh(Some("p".into())), L)
            .unwrap();
        let q = fb
            .call(double, x, Dest::Fresh(Some("q".into())), L)
            .unwrap();
        fb.binop(Operation::Mul, p, q, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(call_count(&ir), 2);

    let plan = analyze_inline(&ir);
    assert_eq!(plan.inline.len(), 2, "both call sites strip");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(call_count(&out), 0, "no Call morphisms survive");
    assert_eq!(out.funcs().count(), 1, "orphan callee dropped — one graph");

    let before = eval_call(&ir, main, scalar(3), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(3), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(36)));
    assert_eq!(after, before);
}

// --- cost-model policy: the cap ----------------------------------------------

/// A Named `x -> x+1+1+…` with `n` Adds (3 morphisms each) + one `Output`.
fn add_chain_fn(b: &mut IrBuilder, name: &str, n: u32) -> mapal_ir::FuncId {
    let f = b
        .declare(FuncKind::Named, name, Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let one = fb.constant(Value::I32(1), L).unwrap();
    let mut acc = x;
    for _ in 0..n {
        acc = fb
            .binop(Operation::Add, acc, one, Dest::Fresh(None), L)
            .unwrap();
    }
    fb.output(acc, None, L).unwrap();
    fb.finish().unwrap();
    f
}

#[test]
fn oversized_callee_stays_a_call() {
    // big(x) = x + 86: 86 Adds × 3 morphisms + Output = 259 > INLINE_MAX_BODY.
    let mut b = IrBuilder::new();
    let big = add_chain_fn(&mut b, "big", 86);
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        fb.call(big, x, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert!(
        ir.func(big).unwrap().morphisms.len() as u32 > INLINE_MAX_BODY,
        "test shape: callee body over the cap"
    );

    let plan = analyze_inline(&ir);
    assert!(plan.inline.is_empty(), "over the cap ⇒ kept as a call");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(
        call_count(&out),
        1,
        "the kept call survives (region boundary)"
    );
    assert_eq!(out.funcs().count(), 2, "the callee is still referenced");

    let before = eval_call(&ir, main, scalar(3), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(3), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(89)));
    assert_eq!(after, before);
}

#[test]
fn callee_at_the_cap_inlines() {
    // at_cap(x) = x + 85: 85 Adds × 3 morphisms + Output = 256 = INLINE_MAX_BODY
    // (the policy is `≤`).
    let mut b = IrBuilder::new();
    let at_cap = add_chain_fn(&mut b, "at_cap", 85);
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        fb.call(at_cap, x, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(
        ir.func(at_cap).unwrap().morphisms.len() as u32,
        INLINE_MAX_BODY,
        "test shape: callee body exactly at the cap"
    );

    let plan = analyze_inline(&ir);
    assert_eq!(plan.inline.len(), 1, "≤ INLINE_MAX_BODY ⇒ stripped");

    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(call_count(&out), 0);
    let before = eval_call(&ir, main, scalar(3), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(3), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(88)));
    assert_eq!(after, before);
}

// --- entry + cycle guards -----------------------------------------------------

#[test]
fn entry_is_never_inlined() {
    // main(x) = x + 1; helper(x) = main(x) * 2 (uncalled — the analysis pin is
    // what matters: the `Call(entry)` edge must not be marked). A *live*
    // caller of the entry would close a Call cycle (the cycle guard's row), so
    // the entry rule only ever fires on dead code — belt-and-braces per the
    // plan's policy statement.
    let mut b = IrBuilder::new();
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let helper = b
        .declare(FuncKind::Named, "helper", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    {
        let mut fb = b.build_fn(helper).unwrap();
        let x = fb.input();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let m = fb.call(main, x, Dest::Fresh(None), L).unwrap();
        fb.binop(Operation::Mul, m, two, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(call_count(&ir), 1);

    let plan = analyze_inline(&ir);
    assert!(plan.inline.is_empty(), "the entry fn is never inlined");
}

// --- diamond-shared callee: per-site duplication -------------------------------

#[test]
fn diamond_shared_callee_inlines_per_site() {
    // sq(x) = x * x; main(x) = sq(x) + sq(x)  ⇒  x=3 → 18. Two call sites of
    // the SAME callee: one body copy per site (duplication is the documented
    // policy — strip is not free, the cap is the guard).
    let mut b = IrBuilder::new();
    let sq = self_binop_fn(&mut b, "sq", Operation::Mul);
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        let p = fb.call(sq, x, Dest::Fresh(Some("p".into())), L).unwrap();
        let q = fb.call(sq, x, Dest::Fresh(Some("q".into())), L).unwrap();
        fb.binop(Operation::Add, p, q, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    assert_eq!(op_count(&ir, Operation::Mul), 1, "one Mul before the strip");

    let out = replay(&ir, &analyze_inline(&ir)).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(call_count(&out), 0);
    assert_eq!(out.funcs().count(), 1);
    assert_eq!(
        op_count(&out, Operation::Mul),
        2,
        "one body copy per call site"
    );

    let before = eval_call(&ir, main, scalar(3), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(3), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(18)));
    assert_eq!(after, before);
}

// --- substitution shapes -------------------------------------------------------

#[test]
fn loop_bearing_callee_stays_a_call() {
    // countdown(x) { s = x; while s > 0 { s = s - 1 }; s }  ⇒ 0 for x ≥ 0;
    // the loop exits to the callee's Return (the LoopExit redirect row).
    // main(x) = countdown(x) + x  ⇒  x.
    let mut b = IrBuilder::new();
    let countdown = b
        .declare(FuncKind::Named, "countdown", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(countdown).unwrap();
        let x = fb.input();
        let lh = fb.begin_loop(x, L).unwrap();
        let s = fb.merge_of(&lh);
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let cond = fb
            .binop(Operation::Gt, s, zero, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let next = fb
            .binop(Operation::Sub, s, one, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, next, cond, L).unwrap();
        fb.loop_exit(&lh, s, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        let c = fb
            .call(countdown, x, Dest::Fresh(Some("c".into())), L)
            .unwrap();
        fb.binop(Operation::Add, c, x, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let plan = analyze_inline(&ir);
    assert!(plan.inline.is_empty(), "loop-bearing callee is not planned");
    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(call_count(&out), 1);
    assert_eq!(out.funcs().count(), 2);

    let before = eval_call(&ir, main, scalar(5), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(5), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(5)));
    assert_eq!(after, before);
}

#[test]
fn tuple_returning_callee_inlines() {
    // swap(p) = (p.1, p.0) — the callee's Return is written by two slot
    // writes. main(x) = swap((x, x + 1)).0  ⇒  x + 1.
    let pair_ty = Ty::Tuple(vec![Ty::i32(), Ty::i32()]);
    let mut b = IrBuilder::new();
    let swap = b
        .declare(FuncKind::Named, "swap", pair_ty.clone(), pair_ty.clone(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(swap).unwrap();
        let p = fb.input();
        let a = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
        let c = fb.proj(p, 1, Dest::Fresh(None), L).unwrap();
        fb.output(c, Some(0), L).unwrap();
        fb.output(a, Some(1), L).unwrap();
        fb.finish().unwrap();
    }
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let xp1 = fb
            .binop(Operation::Add, x, one, Dest::Fresh(None), L)
            .unwrap();
        let pair = fb.pack(&[x, xp1], Dest::Fresh(None), L).unwrap();
        let t = fb
            .call(swap, pair, Dest::Fresh(Some("t".into())), L)
            .unwrap();
        fb.proj(t, 0, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let out = replay(&ir, &analyze_inline(&ir)).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(call_count(&out), 0);
    assert_eq!(out.funcs().count(), 1);

    let before = eval_call(&ir, main, scalar(3), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(3), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(4)));
    assert_eq!(after, before);
}

#[test]
fn call_writing_return_inlines() {
    // inc(x) = x + 1; main(x) = call inc(x) → ret  (a Dest::Ret call site —
    // the callee's Return writers replay as main's own).
    let mut b = IrBuilder::new();
    let inc = b
        .declare(FuncKind::Named, "inc", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(inc).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let main = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let x = fb.input();
        fb.call(inc, x, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();

    let out = replay(&ir, &analyze_inline(&ir)).unwrap();
    assert!(validate(&out).is_empty());
    assert_eq!(call_count(&out), 0);
    assert_eq!(out.funcs().count(), 1);

    let before = eval_call(&ir, main, scalar(4), BUDGET);
    let after = eval_call(&out, out.entry(), scalar(4), BUDGET);
    assert_eq!(before, Outcome::Done(scalar(5)));
    assert_eq!(after, before);
}

#[test]
fn default_rewrite_inlines_call_inside_map_body() {
    // helper(x) = x + 1; body(x) = helper(x); main(a) = map(body, a).
    let mut b = IrBuilder::new();
    let helper = b
        .declare(FuncKind::Named, "helper", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(helper).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let body = b
        .declare(FuncKind::MapBody, "body", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(body).unwrap();
        let x = fb.input();
        fb.call(helper, x, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let main = b
        .declare(FuncKind::Named, "main", arr_ty(3), arr_ty(3), L)
        .unwrap();
    {
        let mut fb = b.build_fn(main).unwrap();
        let input = fb.input();
        fb.map(body, input, Dest::Ret { slot: None }, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(main).unwrap();
    let before = eval_call(&ir, main, arr(&[1, 2, 3]), BUDGET);

    let out = rewrite(ir).ir;
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    assert_eq!(call_count(&out), 0, "the MapBody Call is stripped");
    assert_eq!(out.funcs().count(), 2, "only main + MapBody remain");
    assert!(
        out.morphisms()
            .any(|(_, m)| matches!(m.op, Operation::Map { .. })),
        "the collection output remains a Map"
    );
    assert_eq!(before, Outcome::Done(arr(&[2, 3, 4])));
    assert_eq!(
        eval_call(&out, out.entry(), arr(&[1, 2, 3]), BUDGET),
        before
    );
}

// --- loop-bearing matmul4 policy shape -------------------------------------------

/// Lower an in-Core example (the `golden.rs` path) into sealed IR.
fn lower_example(name: &str) -> CategoryIr {
    let path = format!(
        "{}/../../examples/{}.mapal",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let po = mapal_syntax::parse(&src);
    assert!(
        po.diagnostics.is_empty(),
        "{name}: parse {:?}",
        po.diagnostics
    );
    mapal_lower::lower(&src, &po.program).unwrap_or_else(|d| panic!("{name}: lower {d:?}"))
}

#[test]
fn matmul4_loops_lift_then_callees_inline() {
    // R-LF removes `cell`'s k-loop, R-LM removes `matmul`'s t-loop, then the
    // next fixpoint round strips both now-loop-free Call boundaries.
    let ir = lower_example("matmul4_loop");
    let before = run(&ir, BUDGET);
    assert!(
        matches!(before.outcome, Outcome::Done(_)),
        "raw matmul4 runs"
    );

    assert!(
        analyze_inline(&ir).inline.is_empty(),
        "raw loop callees stay Calls"
    );
    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty(), "{:?}", validate(&res.ir));
    assert_eq!(call_count(&res.ir), 0, "both lifted callees inline");
    assert_eq!(
        res.ir
            .funcs()
            .flat_map(|(f, _)| res.ir.loop_structure(f))
            .count(),
        0,
        "all loop SCCs are gone"
    );
    assert!(
        res.ir.morphisms().any(|(_, m)| {
            let Operation::Map { body, .. } = m.op else {
                return false;
            };
            res.ir
                .func(body)
                .expect("MapBody")
                .morphisms
                .iter()
                .any(|&bm| matches!(res.ir.morphism(bm).unwrap().op, Operation::Fold { .. }))
        }),
        "the final graph contains a Map whose body contains the lifted Fold"
    );
    assert_eq!(run(&res.ir, BUDGET), before);
}

// --- determinism + idempotence (directed) ---------------------------------------

#[test]
fn inline_is_deterministic_and_idempotent() {
    // The diamond program, built fresh per run (CategoryIr is not Clone).
    let build = || {
        let mut b = IrBuilder::new();
        let sq = self_binop_fn(&mut b, "sq", Operation::Mul);
        let main = b
            .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(main).unwrap();
            let x = fb.input();
            let p = fb.call(sq, x, Dest::Fresh(Some("p".into())), L).unwrap();
            let q = fb.call(sq, x, Dest::Fresh(Some("q".into())), L).unwrap();
            fb.binop(Operation::Add, p, q, Dest::Ret { slot: None }, L)
                .unwrap();
            fb.finish().unwrap();
        }
        b.seal(main).unwrap()
    };

    let res1 = rewrite_with(build(), &[PassId::Inline]);
    let res2 = rewrite_with(build(), &[PassId::Inline]);
    assert_eq!(
        res1.ir.to_mermaid(),
        res2.ir.to_mermaid(),
        "L2: same graph → byte-identical inlined graph"
    );

    let again = rewrite_with(res1.ir, &[PassId::Inline]);
    assert!(
        again.report.applied.is_empty(),
        "inline ∘ inline = inline: {:?}",
        again.report.applied
    );
}

// --- R1 proptest battery (raw vs inlined vs the interp oracle) ------------------

fn check_closed(prog: &Prog) -> Result<(), TestCaseError> {
    let orig = build(prog);
    let before = run(&orig.ir, BUDGET);

    let res = rewrite_with(build(prog).ir, &[PassId::Inline]);
    prop_assert!(
        validate(&res.ir).is_empty(),
        "invalid: {:?}",
        validate(&res.ir)
    );
    let after = run(&res.ir, BUDGET);
    prop_assert!(approx(&after, &before), "{before:?} !≈ {after:?}");

    // Determinism: an independent build+inline → byte-identical mermaid.
    let res2 = rewrite_with(build(prog).ir, &[PassId::Inline]);
    prop_assert_eq!(
        res.ir.to_mermaid(),
        res2.ir.to_mermaid(),
        "nondeterministic"
    );

    // Idempotence: a second inline of the fixpoint applies nothing.
    let again = rewrite_with(res2.ir, &[PassId::Inline]);
    prop_assert!(
        again.report.applied.is_empty(),
        "not idempotent: {:?}",
        again.report.applied
    );
    Ok(())
}

fn check_open(prog: &Prog) -> Result<(), TestCaseError> {
    let orig = build(prog);
    let befores: Vec<Outcome> = orig
        .args
        .iter()
        .map(|a| eval_call(&orig.ir, orig.entry, a.clone(), BUDGET))
        .collect();

    let res = rewrite_with(build(prog).ir, &[PassId::Inline]);
    prop_assert!(validate(&res.ir).is_empty());
    for (arg, before) in orig.args.iter().zip(&befores) {
        let after = eval_call(&res.ir, res.ir.entry(), arg.clone(), BUDGET);
        prop_assert!(after == *before, "{before:?} !≈ {after:?}");
    }

    let res2 = rewrite_with(build(prog).ir, &[PassId::Inline]);
    prop_assert_eq!(
        res.ir.to_mermaid(),
        res2.ir.to_mermaid(),
        "nondeterministic"
    );
    let again = rewrite_with(res2.ir, &[PassId::Inline]);
    prop_assert!(
        again.report.applied.is_empty(),
        "not idempotent: {:?}",
        again.report.applied
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn closed_inline(prog in prog_strategy(false, false)) {
        check_closed(&prog)?;
    }

    #[test]
    fn closed_inline_trap_free(prog in prog_strategy(true, false)) {
        check_closed(&prog)?;
    }

    #[test]
    fn open_inline(prog in prog_strategy(false, true)) {
        check_open(&prog)?;
    }

    #[test]
    fn open_inline_trap_free(prog in prog_strategy(true, true)) {
        check_open(&prog)?;
    }
}
