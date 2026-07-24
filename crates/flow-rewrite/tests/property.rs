//! DESIGN §8.2 (headline) + §8.5 (adversarial). For every generated program
//! (both strategies, both modes) × per-pass and full rewrite:
//!   - `run(rewritten) ≈ run(original)` per R1 (`≈`: `Done`+output byte-exact;
//!     `Trapped(_) ≈ Trapped(_)`; `Diverged ≈ Diverged`; classes never cross),
//!   - `validate(rewritten)` empty (R2),
//!   - determinism (same input ⇒ byte-identical `to_mermaid`),
//!   - idempotence (a second rewrite applies nothing).
//!
//! Plus hand-built adversarial cases: dead trapping code stays `Trapped`, two
//! independent traps stay `Trapped` under rebuild-order permutation, a float
//! `x + 0.0` is untouched, a divergent loop stays `Diverged`.

mod testgen;

use flow_interp::{Outcome, RunResult, TrapKind, eval_call, run};
use flow_ir::{CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, validate};
use flow_rewrite::{PassId, RewriteResult, is_canonical, rewrite, rewrite_with};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use testgen::{Prog, Step, build, prog_strategy};

const BUDGET: u64 = 200_000;
const L: SourceLoc = SourceLoc { start: 0, end: 0 };

const ALL: &[PassId] = &[
    PassId::ConstFold,
    PassId::Cse,
    PassId::Dce,
    PassId::MapFusion,
    PassId::Inline,
    PassId::LiftLoops,
];

/// R1's `≈` on whole-program runs.
fn approx(a: &RunResult, b: &RunResult) -> bool {
    match (&a.outcome, &b.outcome) {
        (Outcome::Done(x), Outcome::Done(y)) => x == y && a.output == b.output,
        (Outcome::Trapped(_), Outcome::Trapped(_)) => true,
        (Outcome::Diverged, Outcome::Diverged) => true,
        _ => false,
    }
}

/// R1's `≈` on a single `eval_call` outcome.
fn approx_outcome(a: &Outcome, b: &Outcome) -> bool {
    match (a, b) {
        (Outcome::Done(x), Outcome::Done(y)) => x == y,
        (Outcome::Trapped(_), Outcome::Trapped(_)) => true,
        (Outcome::Diverged, Outcome::Diverged) => true,
        _ => false,
    }
}

/// The full R1/R2/determinism/idempotence battery on one closed program.
fn check_closed(prog: &Prog) -> Result<(), TestCaseError> {
    let orig = build(prog);
    prop_assert!(
        is_canonical(&orig.ir),
        "generated program must be canonical"
    );
    let before = run(&orig.ir, BUDGET);
    prop_assert!(validate(&orig.ir).is_empty(), "generated ir invalid");

    // Per-pass: each pass alone preserves R1 + R2.
    for &pass in ALL {
        let res = rewrite_with(build(prog).ir, std::slice::from_ref(&pass));
        prop_assert!(
            validate(&res.ir).is_empty(),
            "{pass:?}: invalid: {:?}",
            validate(&res.ir)
        );
        let after = run(&res.ir, BUDGET);
        prop_assert!(approx(&after, &before), "{pass:?}: {before:?} !≈ {after:?}");
    }

    // Full rewrite: R1 + R2.
    let res = rewrite(build(prog).ir);
    prop_assert!(validate(&res.ir).is_empty(), "full: invalid");
    let after = run(&res.ir, BUDGET);
    prop_assert!(approx(&after, &before), "full: {before:?} !≈ {after:?}");

    check_determinism_idempotence(prog, &res)
}

/// The battery on one open pure fn, exercised on each random arg.
fn check_open(prog: &Prog) -> Result<(), TestCaseError> {
    let orig = build(prog);
    prop_assert!(is_canonical(&orig.ir));
    prop_assert!(validate(&orig.ir).is_empty());
    let befores: Vec<Outcome> = orig
        .args
        .iter()
        .map(|a| eval_call(&orig.ir, orig.entry, a.clone(), BUDGET))
        .collect();

    for &pass in ALL {
        let res = rewrite_with(build(prog).ir, std::slice::from_ref(&pass));
        prop_assert!(validate(&res.ir).is_empty(), "{pass:?}: invalid");
        for (arg, before) in orig.args.iter().zip(&befores) {
            let after = eval_call(&res.ir, res.ir.entry(), arg.clone(), BUDGET);
            prop_assert!(
                approx_outcome(&after, before),
                "{pass:?}: {before:?} !≈ {after:?}"
            );
        }
    }

    let res = rewrite(build(prog).ir);
    prop_assert!(validate(&res.ir).is_empty());
    for (arg, before) in orig.args.iter().zip(&befores) {
        let after = eval_call(&res.ir, res.ir.entry(), arg.clone(), BUDGET);
        prop_assert!(
            approx_outcome(&after, before),
            "full: {before:?} !≈ {after:?}"
        );
    }

    check_determinism_idempotence(prog, &res)
}

fn check_determinism_idempotence(prog: &Prog, res: &RewriteResult) -> Result<(), TestCaseError> {
    // Determinism: an independent rewrite of an identical build → same mermaid.
    let res2 = rewrite(build(prog).ir);
    prop_assert_eq!(
        res.ir.to_mermaid(),
        res2.ir.to_mermaid(),
        "nondeterministic"
    );

    // Idempotence: a second rewrite of the fixpoint applies nothing.
    let again = rewrite(build(prog).ir);
    let twice = rewrite(again.ir);
    prop_assert!(
        twice.report.applied.is_empty(),
        "not idempotent: {:?}",
        twice.report.applied
    );
    Ok(())
}

#[test]
fn sequential_generated_lifts_with_intervening_fold() {
    let prog = Prog {
        trap_free: true,
        open: false,
        effectful: false,
        helpers: vec![],
        map_bodies: vec![],
        fold_bodies: vec![],
        map_cap_bodies: vec![],
        map_acap_bodies: vec![],
        fold_cap_bodies: vec![vec![]],
        nest_bodies: vec![],
        main: vec![
            Step::LiftMap { arr: 0, cap: 0 },
            Step::Phi { t: 0, e: 0, c: 0 },
            Step::FoldCapScalar {
                arr: 0,
                seed: 0,
                cap: 0,
                body: 0,
            },
            Step::LiftFold {
                k: 0,
                seed: 0,
                cap: 34,
            },
        ],
        prints: vec![],
        ret: 0,
        args: vec![0],
    };
    check_closed(&prog).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn closed_default(prog in prog_strategy(false, false)) {
        check_closed(&prog)?;
    }

    #[test]
    fn closed_trap_free(prog in prog_strategy(true, false)) {
        check_closed(&prog)?;
    }

    #[test]
    fn open_default(prog in prog_strategy(false, true)) {
        check_open(&prog)?;
    }

    #[test]
    fn open_trap_free(prog in prog_strategy(true, true)) {
        check_open(&prog)?;
    }
}

// --- §8.5 adversarial (hand-built) ----------------------------------------

/// Dead code that traps stays `Trapped` end-to-end (R4): a dead `1 / 0` is
/// evaluated by the oracle (interp §2) and DCE must keep it.
#[test]
fn dead_trapping_div_stays_trapped() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        // Dead: result never reaches Return.
        let _dead = fb
            .binop(Operation::Div, one, zero, Dest::Fresh(None), L)
            .unwrap();
        let ret = fb.constant(Value::I32(42), L).unwrap();
        fb.output(ret, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(matches!(
        run(&ir, BUDGET).outcome,
        Outcome::Trapped(TrapKind::DivZero)
    ));

    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty());
    assert!(
        matches!(
            run(&res.ir, BUDGET).outcome,
            Outcome::Trapped(TrapKind::DivZero)
        ),
        "DCE dropped a dead trapping Div"
    );
}

/// Two independent traps: the run traps regardless of rebuild-order permutation
/// (`Trapped(k₁) ≈ Trapped(k₂)`).
#[test]
fn two_independent_traps_stay_trapped() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let _div = fb
            .binop(Operation::Div, one, zero, Dest::Fresh(None), L)
            .unwrap();
        let a0 = fb.constant(Value::I32(7), L).unwrap();
        let a1 = fb.constant(Value::I32(8), L).unwrap();
        let arr = fb.pack_array(&[a0, a1], Dest::Fresh(None), L).unwrap();
        let big = fb.constant(Value::I32(99), L).unwrap();
        let _oob = fb.index(arr, big, Dest::Fresh(None), L).unwrap();
        let ret = fb.constant(Value::I32(0), L).unwrap();
        fb.output(ret, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(matches!(run(&ir, BUDGET).outcome, Outcome::Trapped(_)));
    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty());
    assert!(matches!(run(&res.ir, BUDGET).outcome, Outcome::Trapped(_)));
}

/// A float `x + 0.0` is NOT an algebraic-identity rewrite (RW4: IEEE-unsound,
/// `-0.0 + 0.0 = 0.0`). ConstFold must find nothing on a pure open `f32 -> f32`.
#[test]
fn float_add_zero_untouched() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "f", Ty::f32(), Ty::f32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let x = fb.input();
        let z = fb.constant(Value::F32(0.0), L).unwrap();
        fb.binop(Operation::Add, x, z, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    let res = rewrite_with(ir, &[PassId::ConstFold]);
    assert!(
        res.report.applied.is_empty(),
        "ConstFold rewrote a float x+0.0: {:?}",
        res.report.applied
    );
    // And the run is unchanged where -0.0 would expose a wrong fold.
    let out = eval_call(&res.ir, res.ir.entry(), RValue_f32(-0.0), BUDGET);
    assert_eq!(out, Outcome::Done(RValue_f32(0.0)));
}

#[allow(non_snake_case)]
fn RValue_f32(x: f32) -> flow_interp::RValue {
    flow_interp::RValue::Scalar(Value::F32(x))
}

/// A hand-built non-terminating loop stays `Diverged` under rewrite.
#[test]
fn divergent_loop_stays_diverged() {
    let ir = divergent_loop();
    assert!(is_canonical(&ir));
    assert!(matches!(run(&ir, 5_000).outcome, Outcome::Diverged));
    let res = rewrite(ir);
    assert!(validate(&res.ir).is_empty());
    assert!(
        matches!(run(&res.ir, 5_000).outcome, Outcome::Diverged),
        "rewrite changed a divergent loop"
    );
}

/// `main() -> i32 { i = 0; while true { i += 1 } }` — canonical shape, never exits.
fn divergent_loop() -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let zero = fb.constant(Value::I32(0), L).unwrap();
        let tru = fb.constant(Value::Bool(true), L).unwrap();
        let lh = fb.begin_loop(zero, L).unwrap();
        let i = fb.merge_of(&lh);
        let one = fb.constant(Value::I32(1), L).unwrap();
        let next = fb
            .binop(Operation::Add, i, one, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, next, tru, L).unwrap(); // back on true = always
        fb.loop_exit(&lh, i, tru, Dest::Ret { slot: None }, L)
            .unwrap(); // exit on false = never
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}
