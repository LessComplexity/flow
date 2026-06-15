//! Interp scale bench (interp DESIGN §11.7): run the fueled evaluator on a
//! synthetic **deep loop** (a counting loop driven N iterations) and a **large
//! map** (`[i32; N] -> map { x -> x + 1 }`) at a couple of sizes. Numbers
//! recorded in STATUS. Mirrors flow-ir's bench file style (criterion,
//! `harness = false`). Correctness gates this — the suite is green first.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use flow_interp::{Outcome, RValue, eval_call};
use flow_ir::{CategoryIr, Dest, FuncId, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

fn loc() -> SourceLoc {
    SourceLoc::empty_at(0)
}

/// `fn count_to(limit: i32) -> i32` — a loop carrying `i` that increments while
/// `i < limit`, returning `i` (= `limit`). Drives the guard-first driver N
/// iterations: the deep-loop stressor.
fn build_counting_loop() -> (CategoryIr, FuncId) {
    let mut ib = IrBuilder::new();
    let f = ib
        .declare(FuncKind::Named, "count_to", Ty::i32(), Ty::i32(), loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(f).unwrap();
        let limit = fb.input();
        let zero = fb.constant(Value::I32(0), loc()).unwrap();
        let lh = fb.begin_loop(zero, loc()).unwrap();
        let merge = fb.merge_of(&lh);
        let cond = fb
            .binop(Operation::Lt, merge, limit, Dest::Fresh(None), loc())
            .unwrap();
        let one = fb.constant(Value::I32(1), loc()).unwrap();
        let next = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), loc())
            .unwrap();
        fb.loop_back(&lh, next, cond, loc()).unwrap();
        let exit = fb
            .loop_exit(&lh, merge, cond, Dest::Fresh(None), loc())
            .unwrap();
        fb.output(exit, None, loc()).unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = ib.seal(f).unwrap();
    (ir, f)
}

/// `fn bump_all(xs: [i32; N]) -> [i32; N]` — `xs -> map { x -> x + 1 }`. The
/// large-map stressor: N element applications of the body.
fn build_map(n: u64) -> (CategoryIr, FuncId, RValue) {
    let mut ib = IrBuilder::new();
    let arr_ty = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: n,
    };
    let body = ib
        .declare(FuncKind::MapBody, "bump", Ty::i32(), Ty::i32(), loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(body).unwrap();
        let x = fb.input();
        let one = fb.constant(Value::I32(1), loc()).unwrap();
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: None }, loc())
            .unwrap();
        fb.finish().unwrap();
    }
    let f = ib
        .declare(FuncKind::Named, "bump_all", arr_ty.clone(), arr_ty, loc())
        .unwrap();
    {
        let mut fb = ib.build_fn(f).unwrap();
        let xs = fb.input();
        fb.map(body, xs, Dest::Ret { slot: None }, loc()).unwrap();
        fb.finish().unwrap();
    }
    let ir = ib.seal(f).unwrap();
    let arg = RValue::Array(
        (0..n)
            .map(|i| RValue::Scalar(Value::I32(i as i32)))
            .collect(),
    );
    (ir, f, arg)
}

fn bench_interp_scale(c: &mut Criterion) {
    let (loop_ir, loop_f) = build_counting_loop();
    let mut g = c.benchmark_group("deep_loop");
    for &iters in &[1_000i32, 10_000] {
        g.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter(|| {
                let out = eval_call(
                    &loop_ir,
                    loop_f,
                    RValue::Scalar(Value::I32(iters)),
                    u64::MAX,
                );
                assert!(matches!(out, Outcome::Done(_)));
            });
        });
    }
    g.finish();

    let mut g = c.benchmark_group("large_map");
    for &n in &[1_000u64, 10_000] {
        let (ir, f, arg) = build_map(n);
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let out = eval_call(&ir, f, arg.clone(), u64::MAX);
                assert!(matches!(out, Outcome::Done(_)));
            });
        });
    }
    g.finish();
}

criterion_group!(benches, bench_interp_scale);
criterion_main!(benches);
