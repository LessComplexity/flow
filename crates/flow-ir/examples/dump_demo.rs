//! Runnable demo: hand-build two graphs via the public `IrBuilder` API into one
//! sealed `CategoryIr`, then print a lint-clean Mermaid dump (DESIGN §7/§10/§14).

use flow_ir::{
    CategoryIr, Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, lint_mermaid,
};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// Build both graphs into one sealed `CategoryIr`.
fn build() -> CategoryIr {
    let mut b = IrBuilder::new();

    // (1) `fn f(data: i32) -> i32 { data * 2 -> + 5 -> ret; }`
    //     input ×2 (Pair const 2 + Mul), then +5 (Pair const 5 + Add) → Return.
    let f = b
        .declare(FuncKind::Named, "f", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let data = fb.input();
        let two = fb.constant(Value::I32(2), L).unwrap();
        let s1 = fb
            .binop(Operation::Mul, data, two, Dest::Fresh(None), L)
            .unwrap();
        let five = fb.constant(Value::I32(5), L).unwrap();
        // Canonical ret-write: the final primitive targets Return directly (§10).
        fb.binop(Operation::Add, s1, five, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.finish().unwrap();
    }

    // (2) `sum_to_n`-shaped loop (DESIGN §7, graph (d') construction reference):
    //     carried (i, acc) seeded (1, 0), guard i <= n (n = 10 const),
    //     back route carries (i+1, acc+i), exit reads acc from the MERGE view.
    let g = b
        .declare(FuncKind::Named, "sum_to_n", Ty::i32(), Ty::i32(), L)
        .unwrap();
    {
        let mut fb = b.build_fn(g).unwrap();
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
        // guard i <= n  (n is the constant 10 for the demo).
        let n = fb.constant(Value::I32(10), L).unwrap();
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
        // exit payload is the merge-view acc (Proj of merge, not the updated value).
        fb.loop_exit(&lh, acc, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }

    b.seal(f).unwrap()
}

fn main() {
    let ir = build();
    let dump = ir.to_mermaid();
    // Self-check: the demo asserts its own dump is a lint-clean Mermaid document.
    let lints = lint_mermaid(&dump);
    assert!(lints.is_empty(), "lint failures: {lints:?}");
    print!("{dump}");
}
