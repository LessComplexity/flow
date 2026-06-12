//! Scale bench (DESIGN §16 item 7): build + seal + `to_mermaid` + `sccs` on
//! synthetic **chains** (straight-line, deep) and **grids** (wide fanout-reduce,
//! many edges per object) at 1k / 10k / 100k morphisms. Numbers recorded in
//! STATUS. Mirrors flow-syntax's bench file style (criterion, `harness = false`).

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use flow_ir::{Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value};

/// Build a straight-line `i32` add chain of `n` binops, then seal. Deep, narrow:
/// `n` objects on the critical path (the J1 / topo depth stressor).
fn build_chain(n: usize) -> flow_ir::CategoryIr {
    let loc = SourceLoc::new(0, 0);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), loc)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let one = fb.constant(Value::I32(1), loc).unwrap();
        let mut acc = fb.constant(Value::I32(0), loc).unwrap();
        for i in 0..n {
            let dest = if i + 1 == n {
                Dest::Ret { slot: None }
            } else {
                Dest::Fresh(None)
            };
            acc = fb
                .binop(flow_ir::Operation::Add, acc, one, dest, loc)
                .unwrap();
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

/// Build a balanced reduction **grid**: `~n/2` leaf constants reduced pairwise
/// up a binary tree of `Add`s to a single root. Wide and shallow (log depth):
/// exercises adjacency-heavy graphs where many objects have fanout/fanin, the
/// complement to the deep chain. Total binops ≈ leaves − 1 ≈ `n/2`.
fn build_grid(n: usize) -> flow_ir::CategoryIr {
    let loc = SourceLoc::new(0, 0);
    // Each Add contributes 2 Pair edges + 1 Add ≈ 3 morphisms; a leaf const is an
    // object, not a morphism. Pick a leaf count so build+seal touches ~n edges.
    let leaves = (n / 3).max(2);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), loc)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let mut level: Vec<flow_ir::ObjectId> = (0..leaves)
            .map(|_| fb.constant(Value::I32(1), loc).unwrap())
            .collect();
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            let pairs = level.len() / 2;
            for k in 0..pairs {
                let lhs = level[2 * k];
                let rhs = level[2 * k + 1];
                let last_pair = level.len() == 2 && k == pairs - 1;
                let dest = if last_pair {
                    Dest::Ret { slot: None }
                } else {
                    Dest::Fresh(None)
                };
                next.push(fb.binop(Operation::Add, lhs, rhs, dest, loc).unwrap());
            }
            // Carry an odd straggler up unchanged.
            if level.len() % 2 == 1 {
                next.push(level[level.len() - 1]);
            }
            level = next;
        }
        fb.finish().unwrap();
    }
    b.seal(f).unwrap()
}

fn bench_ir_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("ir_scale");
    for &n in &[1_000usize, 10_000, 100_000] {
        // --- chains ---
        group.bench_with_input(BenchmarkId::new("chain_build_seal", n), &n, |bch, &n| {
            bch.iter(|| build_chain(n));
        });
        let ir = build_chain(n);
        let entry = ir.entry();
        group.bench_with_input(BenchmarkId::new("chain_to_mermaid", n), &n, |bch, _| {
            bch.iter(|| ir.to_mermaid());
        });
        group.bench_with_input(BenchmarkId::new("chain_sccs", n), &n, |bch, _| {
            bch.iter(|| ir.sccs(entry));
        });

        // --- grids ---
        group.bench_with_input(BenchmarkId::new("grid_build_seal", n), &n, |bch, &n| {
            bch.iter(|| build_grid(n));
        });
        let gir = build_grid(n);
        let gentry = gir.entry();
        group.bench_with_input(BenchmarkId::new("grid_to_mermaid", n), &n, |bch, _| {
            bch.iter(|| gir.to_mermaid());
        });
        group.bench_with_input(BenchmarkId::new("grid_sccs", n), &n, |bch, _| {
            bch.iter(|| gir.sccs(gentry));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_ir_scale);
criterion_main!(benches);
