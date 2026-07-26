//! ADR-0027 — the testgen generator's capture-producing shapes, pinned one
//! example each: the step emits the expected `captures > 0` op, the built IR
//! is canonical and validate-clean, the interp oracle computes the
//! hand-derived value, and the full rewrite pipeline preserves the run
//! (R1/R2). A deterministic smoke sweep closes the file: the random strategy
//! really produces capture programs in both modes (the property battery and
//! the backend differential sweeps only see them if the distribution puts
//! them there).

mod testgen;

use mapal_interp::{Outcome, RValue, run};
use mapal_ir::{CategoryIr, Operation, Value, validate};
use mapal_rewrite::{analyze_map_fusion, is_canonical, replay, rewrite};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;
use testgen::{NestBody, Prog, Step, build, prog_strategy};

const BUDGET: u64 = 200_000;

/// A closed, pure, trap-free program script with empty body pools (each test
/// fills the pools it exercises). `main` runs after the generator's seed
/// constants, so `pool.i32s` starts as `[1, 2, 3]` and `ret`/`cap` indices
/// count from there.
fn closed_prog(main: Vec<Step>) -> Prog {
    Prog {
        trap_free: true,
        open: false,
        effectful: false,
        helpers: vec![],
        map_bodies: vec![],
        fold_bodies: vec![],
        map_cap_bodies: vec![],
        map_acap_bodies: vec![],
        fold_cap_bodies: vec![],
        nest_bodies: vec![],
        main,
        prints: vec![],
        ret: 0,
        args: vec![0],
    }
}

/// The capture counts of every `Map`/`Fold` edge in the graph, in morphism
/// order — `(map captures, fold captures)`.
fn capture_counts(ir: &CategoryIr) -> (Vec<u32>, Vec<u32>) {
    let mut maps = Vec::new();
    let mut folds = Vec::new();
    for (_, m) in ir.morphisms() {
        match m.op {
            Operation::Map { captures, .. } => maps.push(captures),
            Operation::Fold { captures, .. } => folds.push(captures),
            _ => {}
        }
    }
    (maps, folds)
}

/// Build + run the closed script: the IR must be canonical and validate-clean
/// (a generator that emits an ill-typed step is a bug — this is where it
/// surfaces), and the full rewrite must preserve the run (R1/R2).
fn check_closed(p: &Prog) -> Outcome {
    let built = build(p);
    assert!(is_canonical(&built.ir), "generated program not canonical");
    assert!(
        validate(&built.ir).is_empty(),
        "generated ir invalid: {:?}",
        validate(&built.ir)
    );
    let before = run(&built.ir, BUDGET).outcome;

    let res = rewrite(build(p).ir);
    assert!(
        validate(&res.ir).is_empty(),
        "rewritten invalid: {:?}",
        validate(&res.ir)
    );
    let after = run(&res.ir, BUDGET).outcome;
    assert_eq!(after, before, "rewrite changed the run");
    before
}

fn i32_outcome(k: i32) -> Outcome {
    Outcome::Done(RValue::Scalar(Value::I32(k)))
}

// --- map with a scalar capture ----------------------------------------------

#[test]
fn map_scalar_capture() {
    // cap = 10; a = [1,2,3]; map((c, x) -> c + x, [cap], a) = [11,12,13].
    let mut p = closed_prog(vec![
        Step::ConstI32(10),                   // i32s: [1,2,3, 10]
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::MapCapScalar {
            arr: 0,
            cap: 3,
            body: 0,
        }, // arrs[1] = [11,12,13]
        Step::Index { arr: 1, idx: 2 },       // i32s[4] = 13
    ]);
    p.map_cap_bodies = vec![vec![Step::Bin { op: 0, a: 0, b: 1 }]]; // cap + elem
    p.ret = 4;

    let (maps, _) = capture_counts(&build(&p).ir);
    assert_eq!(maps, vec![1], "one captured map, captures = 1");
    assert_eq!(check_closed(&p), i32_outcome(13));
}

// --- map with an array capture ----------------------------------------------

#[test]
fn map_array_capture() {
    // cap = [4,5,6]; a = [1,2,3]; map((c, x) -> x * c[1], [cap], a) = [5,10,15].
    let mut p = closed_prog(vec![
        Step::ConstI32(4),
        Step::ConstI32(5),
        Step::ConstI32(6),                    // i32s: [1,2,3, 4,5,6]
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::MakeArray { a: 3, b: 4, c: 5 }, // arrs[1] = [4,5,6]
        Step::MapCapArray {
            arr: 0,
            cap: 1,
            body: 0,
        }, // arrs[2] = [5,10,15]
        Step::Index { arr: 2, idx: 2 },       // i32s[6] = 15
    ]);
    // Body pool: [elem, cap[0], cap[1], cap[2]] → elem * cap[1].
    p.map_acap_bodies = vec![vec![Step::Bin { op: 2, a: 0, b: 2 }]];
    p.ret = 6;

    let (maps, _) = capture_counts(&build(&p).ir);
    assert_eq!(maps, vec![1], "one array-captured map");
    assert_eq!(check_closed(&p), i32_outcome(15));
}

// --- fold with a scalar capture ----------------------------------------------

#[test]
fn fold_scalar_capture() {
    // cap = 10; a = [1,2,3]; fold((c, acc, x) -> acc + c * x, [cap], 1, a):
    // 1 + 10*1 = 11 → 11 + 10*2 = 31 → 31 + 10*3 = 61.
    let mut p = closed_prog(vec![
        Step::ConstI32(10),                   // i32s: [1,2,3, 10]
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::FoldCapScalar {
            arr: 0,
            seed: 0,
            cap: 3,
            body: 0,
        }, // i32s[4] = 61
    ]);
    p.fold_cap_bodies = vec![vec![
        Step::Bin { op: 2, a: 0, b: 2 }, // cap * elem
        Step::Bin { op: 0, a: 1, b: 3 }, // acc + (cap * elem)
    ]];
    p.ret = 4;

    let (_, folds) = capture_counts(&build(&p).ir);
    assert_eq!(folds, vec![1], "one captured fold");
    assert_eq!(check_closed(&p), i32_outcome(61));
}

// --- fold nested in a map body, capturing across two levels -------------------

#[test]
fn map_nested_fold_two_level_capture() {
    // cap = [4,5,6]; a = [1,2,3]; map over a with body (c, e):
    //   s = fold((e, acc, x) -> acc + x + e, [e], e, c)   // inner captures e
    //   e * s
    // e=1: 1+4+1=6 → 6+5+1=12 → 12+6+1=19 → 1*19 = 19
    // e=2: 2+4+2=8 → 8+5+2=15 → 15+6+2=23 → 2*23 = 46
    // e=3: 3+4+3=10 → 10+5+3=18 → 18+6+3=27 → 3*27 = 81
    let mut p = closed_prog(vec![
        Step::ConstI32(4),
        Step::ConstI32(5),
        Step::ConstI32(6),                    // i32s: [1,2,3, 4,5,6]
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::MakeArray { a: 3, b: 4, c: 5 }, // arrs[1] = [4,5,6]
        Step::MapNestFold {
            arr: 0,
            cap: 1,
            body: 0,
        }, // arrs[2] = [19,46,81]
        Step::Index { arr: 2, idx: 1 },       // i32s[6] = 46
    ]);
    p.nest_bodies = vec![NestBody {
        // Inner fold body pool [cap=e, acc, x]: acc + x, then + e.
        inner: vec![
            Step::Bin { op: 0, a: 1, b: 2 },
            Step::Bin { op: 0, a: 3, b: 0 },
        ],
        // Outer pool [e, s]: e * s.
        outer: vec![Step::Bin { op: 2, a: 0, b: 1 }],
    }];
    p.ret = 6;

    let (maps, folds) = capture_counts(&build(&p).ir);
    assert_eq!(maps, vec![1], "the outer map captures the array");
    assert_eq!(folds, vec![1], "the inner fold captures the outer element");
    assert_eq!(check_closed(&p), i32_outcome(46));
}

// --- a loop-carried scalar captured by a map body ----------------------------

#[test]
fn loop_carried_capture() {
    // a = [1,2,3]; i = loop(5) → 5; map((c, x) -> c + x, [i], a) = [6,7,8].
    let mut p = closed_prog(vec![
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::LoopCapMap {
            k: 5,
            arr: 0,
            body: 0,
        }, // i32s[3] = 5; arrs[1] = [6,7,8]
        Step::Index { arr: 1, idx: 2 },       // i32s[4] = 8
    ]);
    p.map_cap_bodies = vec![vec![Step::Bin { op: 0, a: 0, b: 1 }]]; // cap + elem
    p.ret = 4;

    let built = build(&p);
    let (maps, _) = capture_counts(&built.ir);
    assert_eq!(maps, vec![1], "the map captures the loop exit value");
    assert_eq!(check_closed(&p), i32_outcome(8));
}

// --- chained array-captured maps with an identical capture FUSE ---------------

#[test]
fn chained_array_capture_maps_fuse() {
    // cap = [1,2,3]; a = [1,2,3]; two chained maps of the same body
    // ((c, x) -> x + c[0]) reading the SAME capture object — fusion fires and
    // the fused map keeps captures = 1 (the array-typed capture replay path).
    let mut p = closed_prog(vec![
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[0] = [1,2,3]
        Step::MakeArray { a: 0, b: 1, c: 2 }, // arrs[1] = [1,2,3] (capture)
        Step::MapCapArray {
            arr: 0,
            cap: 1,
            body: 0,
        }, // arrs[2] = [2,3,4]
        Step::MapCapArray {
            arr: 2,
            cap: 1,
            body: 0,
        }, // arrs[3] = [3,4,5]
        Step::Index { arr: 3, idx: 1 },       // i32s[3] = 4
    ]);
    // Body pool: [elem, cap[0], cap[1], cap[2]] → elem + cap[0].
    p.map_acap_bodies = vec![vec![Step::Bin { op: 0, a: 0, b: 1 }]];
    p.ret = 3;

    let ir = build(&p).ir;
    let (maps, _) = capture_counts(&ir);
    assert_eq!(maps, vec![1, 1], "two array-captured maps");
    let plan = analyze_map_fusion(&ir);
    assert_eq!(
        plan.fuse.len(),
        1,
        "identical capture object — fusion fires"
    );
    let out = replay(&ir, &plan).unwrap();
    assert!(validate(&out).is_empty(), "{:?}", validate(&out));
    let (maps, _) = capture_counts(&out);
    assert_eq!(maps, vec![1], "the fused map keeps the shared capture");
    assert_eq!(run(&out, BUDGET).outcome, i32_outcome(4));

    assert_eq!(check_closed(&p), i32_outcome(4));
}

// --- the random strategy really produces capture programs ---------------------

#[test]
fn random_programs_include_captures() {
    let mut runner = TestRunner::deterministic();
    let mut programs_with_caps = 0usize;
    let mut captured_maps = 0usize;
    let mut captured_folds = 0usize;
    for (trap_free, open) in [(false, false), (true, false), (false, true), (true, true)] {
        let strat = prog_strategy(trap_free, open);
        for _ in 0..32 {
            let prog = strat.new_tree(&mut runner).unwrap().current();
            let built = build(&prog);
            assert!(
                validate(&built.ir).is_empty(),
                "generated ir invalid: {:?}",
                validate(&built.ir)
            );
            assert!(is_canonical(&built.ir), "generated program not canonical");
            let (maps, folds) = capture_counts(&built.ir);
            let cm = maps.iter().filter(|&&k| k > 0).count();
            let cf = folds.iter().filter(|&&k| k > 0).count();
            if cm + cf > 0 {
                programs_with_caps += 1;
            }
            captured_maps += cm;
            captured_folds += cf;
        }
    }
    assert!(
        programs_with_caps >= 8 && captured_maps >= 8 && captured_folds >= 2,
        "capture shapes too rare: {programs_with_caps} programs, {captured_maps} maps, {captured_folds} folds"
    );
}
