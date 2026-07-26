//! Scale bench (DESIGN §8.6): `rewrite` of synthetic chains / grids at 1k / 10k /
//! 100k morphisms. Numbers recorded in the WP report. Mirrors
//! `mapal-ir/benches/ir_scale.rs` (criterion, `harness = false`).
//!
//! Inputs are **parameter-seeded** (not all-constant like `ir_scale`): an
//! all-constant chain would const-fold one level per round, needing O(n) rounds
//! past `MAX_ROUNDS`. Instead:
//! - `chain` — a param-seeded add chain that rewrites to nothing: measures the
//!   four analysis scans (the realistic per-graph cost when little folds).
//! - `grid_cse` — a balanced add-tree over copies of the same param: identical
//!   subtrees collapse under CSE in one pass, measuring CSE + one full replay.

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use mapal_ir::{CategoryIr, Dest, FuncKind, IrBuilder, ObjectId, Operation, SourceLoc, Ty, Value};
use mapal_rewrite::rewrite;

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// A param-seeded add chain of `n` binops: `acc = param; acc += 1` × n. Nothing
/// folds/CSEs/DCEs — `rewrite` returns after four O(V+E) analysis scans.
fn build_chain(n: usize) -> CategoryIr {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let mut acc = fb.input();
        let one = fb.constant(Value::I32(1), L).unwrap();
        for i in 0..n {
            let dest = if i + 1 == n {
                Dest::Ret { slot: None }
            } else {
                Dest::Fresh(None)
            };
            acc = fb.binop(Operation::Add, acc, one, dest, L).unwrap();
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

/// A balanced add-tree over `~n/3` copies of the single param: every subtree is
/// `param + param` at each level, so CSE collapses all duplicates in one pass.
fn build_grid_cse(n: usize) -> CategoryIr {
    let leaves = (n / 3).max(2);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let p = fb.input();
        let mut level: Vec<ObjectId> = (0..leaves).map(|_| p).collect();
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let pairs = level.len() / 2;
            for k in 0..pairs {
                let last = level.len() == 2 && k == pairs - 1;
                let dest = if last {
                    Dest::Ret { slot: None }
                } else {
                    Dest::Fresh(None)
                };
                next.push(
                    fb.binop(Operation::Add, level[2 * k], level[2 * k + 1], dest, L)
                        .unwrap(),
                );
            }
            if level.len() % 2 == 1 {
                next.push(level[level.len() - 1]);
            }
            level = next;
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

fn bench_rewrite_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("rewrite_scale");
    for &n in &[1_000usize, 10_000, 100_000] {
        group.bench_with_input(BenchmarkId::new("chain", n), &n, |bch, &n| {
            bch.iter_batched(|| build_chain(n), rewrite, BatchSize::SmallInput);
        });
        group.bench_with_input(BenchmarkId::new("grid_cse", n), &n, |bch, &n| {
            bch.iter_batched(|| build_grid_cse(n), rewrite, BatchSize::SmallInput);
        });
    }
    group.finish();
}

criterion_group!(benches, bench_rewrite_scale);
criterion_main!(benches);
