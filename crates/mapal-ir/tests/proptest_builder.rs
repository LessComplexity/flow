//! The headline property (DESIGN §16 item 3 / P2 definition of done):
//! interleaved valid + deliberately-invalid builder call sequences, then seal —
//! **if seal returns Ok(ir) then validate(&ir) is empty**. Plus a positive
//! generator (well-typed programs) that always seals and validates clean, and a
//! determinism property (§16 item 6).

use mapal_ir::{
    Dest, FuncKind, IrBuilder, Operation, SourceLoc, Ty, Value, lint_mermaid, validate,
};
use proptest::prelude::*;

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

/// One step the interleaved generator can attempt. Many are deliberately
/// type-unsafe (wrong arities, type confusions, token misuse on a token-free
/// fn, loops left unclosed); the builder must reject each per its documented
/// conditions, and the headline property is that a successful `seal` still
/// validates clean.
#[derive(Clone, Debug)]
enum Step {
    ConstI32(i32),
    ConstBool(bool),
    /// Add two pool objects at the given (wrapping) indices.
    Add(usize, usize),
    /// Lt two pool objects.
    Lt(usize, usize),
    /// And two pool objects.
    And(usize, usize),
    /// Pack two pool objects into a tuple.
    Pack(usize, usize),
    /// Proj index 0 of a pool object.
    Proj0(usize),
    /// Neg a pool object.
    Neg(usize),
    /// Phi(a, b, cond).
    Phi(usize, usize, usize),
    /// `print(token, value)` — always ill-typed here (no token in an i32 fn),
    /// so this exercises the rejection path and can never corrupt the graph.
    Print(usize, usize),
    /// Pack two pool objects into a `[T; 2]` array (only succeeds when both
    /// share an elem ty) — feedstock for `Zip`/`Enum`.
    MakeArr(usize, usize),
    /// `zip(a, b)` — succeeds only when both are same-size arrays (ADR-0018).
    Zip(usize, usize),
    /// `enumerate(a)` — succeeds only when `a` is an array (ADR-0018).
    Enum(usize),
    /// `update(a, i, v)` — succeeds only when `a` is an array, `i` integer, and
    /// `v` matches the elem ty (ADR-0021).
    Update(usize, usize, usize),
    /// Open a loop seeded from a pool object (leaving it possibly unclosed).
    BeginLoop(usize),
    /// Back-edge the most recently opened loop with a pool object as next-state.
    LoopBackLast(usize, usize),
    /// Exit + close the most recently opened loop.
    EndLoopLast(usize, usize),
}

fn step_strategy() -> impl Strategy<Value = Step> {
    prop_oneof![
        any::<i32>().prop_map(Step::ConstI32),
        any::<bool>().prop_map(Step::ConstBool),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Add(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Lt(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::And(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Pack(a, b)),
        any::<usize>().prop_map(Step::Proj0),
        any::<usize>().prop_map(Step::Neg),
        (any::<usize>(), any::<usize>(), any::<usize>()).prop_map(|(a, b, c)| Step::Phi(a, b, c)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Print(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::MakeArr(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::Zip(a, b)),
        any::<usize>().prop_map(Step::Enum),
        (any::<usize>(), any::<usize>(), any::<usize>())
            .prop_map(|(a, b, c)| Step::Update(a, b, c)),
        any::<usize>().prop_map(Step::BeginLoop),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::LoopBackLast(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Step::EndLoopLast(a, b)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Interleaved valid/invalid calls: every error is one of the documented
    /// conditions (we don't assert which), and a successful seal validates clean.
    #[test]
    fn interleaved_calls_seal_implies_valid(steps in prop::collection::vec(step_strategy(), 0..40)) {
        let mut b = IrBuilder::new();
        // main : i32 -> i32 (a permissive scalar fn so most ops type-check).
        let f = b.declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L).unwrap();
        let mut pool: Vec<mapal_ir::ObjectId>;
        {
            let mut fb = b.build_fn(f).unwrap();
            pool = vec![fb.input()];
            // Stack of open loop handles (some sequences leave loops unclosed —
            // `finish` then reports OpenLoop and the program does not seal).
            let mut loops: Vec<mapal_ir::LoopHandle> = Vec::new();
            let pick = |pool: &Vec<mapal_ir::ObjectId>, i: usize| pool[i % pool.len().max(1)];
            for step in steps {
                if pool.is_empty() {
                    break;
                }
                let res: Result<mapal_ir::ObjectId, _> = match step {
                    Step::ConstI32(v) => fb.constant(Value::I32(v), L),
                    Step::ConstBool(v) => fb.constant(Value::Bool(v), L),
                    Step::Add(a, c) => {
                        fb.binop(Operation::Add, pick(&pool, a), pick(&pool, c), Dest::Fresh(None), L)
                    }
                    Step::Lt(a, c) => {
                        fb.binop(Operation::Lt, pick(&pool, a), pick(&pool, c), Dest::Fresh(None), L)
                    }
                    Step::And(a, c) => {
                        fb.binop(Operation::And, pick(&pool, a), pick(&pool, c), Dest::Fresh(None), L)
                    }
                    Step::Pack(a, c) => fb.pack(&[pick(&pool, a), pick(&pool, c)], Dest::Fresh(None), L),
                    Step::Proj0(a) => fb.proj(pick(&pool, a), 0, Dest::Fresh(None), L),
                    Step::Neg(a) => fb.unop(Operation::Neg, pick(&pool, a), Dest::Fresh(None), L),
                    Step::Phi(a, c, d) => {
                        fb.phi(pick(&pool, a), pick(&pool, c), pick(&pool, d), Dest::Fresh(None), L)
                    }
                    Step::Print(t, v) => fb.print(pick(&pool, t), pick(&pool, v), L),
                    Step::MakeArr(a, c) => {
                        fb.pack_array(&[pick(&pool, a), pick(&pool, c)], Dest::Fresh(None), L)
                    }
                    Step::Zip(a, c) => {
                        fb.zip(pick(&pool, a), pick(&pool, c), Dest::Fresh(None), L)
                    }
                    Step::Enum(a) => fb.enumerate(pick(&pool, a), Dest::Fresh(None), L),
                    Step::Update(a, i, v) => fb.update(
                        pick(&pool, a),
                        pick(&pool, i),
                        pick(&pool, v),
                        Dest::Fresh(None),
                        L,
                    ),
                    Step::BeginLoop(a) => {
                        match fb.begin_loop(pick(&pool, a), L) {
                            Ok(lh) => {
                                let m = fb.merge_of(&lh);
                                loops.push(lh);
                                Ok(m)
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Step::LoopBackLast(s, c) => {
                        if let Some(lh) = loops.last() {
                            fb.loop_back(lh, pick(&pool, s), pick(&pool, c), L).map(|()| pool[0])
                        } else {
                            Ok(pool[0])
                        }
                    }
                    Step::EndLoopLast(v, c) => {
                        if let Some(lh) = loops.last() {
                            // Try to record an exit so end_loop can succeed, then close.
                            let _ = fb.loop_exit(lh, pick(&pool, v), pick(&pool, c), Dest::Fresh(None), L);
                            let lh = loops.pop().unwrap();
                            let _ = fb.end_loop(lh);
                        }
                        Ok(pool[0])
                    }
                };
                if let Ok(id) = res {
                    // Avoid re-pushing the sentinel pool[0] for no-op steps.
                    if !pool.contains(&id) {
                        pool.push(id);
                    }
                }
            }
            // Always write the input (i32) to ret to satisfy I-RET; ignore the
            // result (a half-built loop makes the chain ill-formed regardless).
            let _ = fb.output(pool[0], None, L);
            // finish may fail (e.g. an unclosed loop → OpenLoop); ignore.
            let _ = fb.finish();
        }
        // seal: if Ok, validate must be clean.
        if let Ok(ir) = b.seal(f) {
            let viol = validate(&ir);
            prop_assert!(viol.is_empty(), "seal Ok but validate found {viol:?}");
        }
    }
}

// A positive generator: build well-typed i32 chains of varying length; always
// seals and validates clean, and dumps lint clean.
proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// Well-typed chains always seal, validate clean, and lint clean.
    #[test]
    fn positive_chains_seal_and_validate(ops in prop::collection::vec(0u8..3, 1..30)) {
        let mut b = IrBuilder::new();
        let f = b.declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L).unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            let mut acc = fb.input();
            let n = ops.len();
            for (i, op) in ops.iter().enumerate() {
                let dest = if i + 1 == n { Dest::Ret { slot: None } } else { Dest::Fresh(None) };
                let operation = match op { 0 => Operation::Add, 1 => Operation::Sub, _ => Operation::Mul };
                acc = fb.binop(operation, acc, one, dest, L).unwrap();
            }
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        prop_assert!(validate(&ir).is_empty());
        let dump = ir.to_mermaid();
        prop_assert!(lint_mermaid(&dump).is_empty());
    }
}

/// A `Str`-bearing ty generator: the headline property must cover declared
/// input/output tys that smuggle `Str` into a param/return position (the F1
/// blocker — those reach the graph without going through a packer). Property:
/// declaring `id : T -> T`, building a bare-move body, and sealing either errors
/// (at declare or seal) OR produces a graph that validates clean — never a
/// "seal Ok but validate dirty" graph.
fn str_bearing_ty() -> impl Strategy<Value = Ty> {
    let leaf = prop_oneof![Just(Ty::Str), Just(Ty::i32()), Just(Ty::Bool)];
    leaf.prop_recursive(3, 8, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 2..4).prop_map(Ty::Tuple),
            (inner.clone(), 1u64..4).prop_map(|(e, n)| Ty::Array {
                elem: Box::new(e),
                size: n
            }),
            prop::collection::vec(inner, 1..3).prop_map(|fs| Ty::Struct {
                name: "S".into(),
                fields: fs
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| (format!("f{i}"), t))
                    .collect(),
            }),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// Str-bearing declared tys: seal Ok ⇒ validate empty (F1 / DESIGN §9 I9s).
    #[test]
    fn str_in_declared_ty_seal_implies_valid(ty in str_bearing_ty()) {
        let mut b = IrBuilder::new();
        // The only Str-bearing ty that can legitimately seal is one with NO Str
        // at all (the generator also emits Str-free tys); when Str is present in
        // a param/return position the builder must reject at declare or seal.
        let Ok(f) = b.declare(FuncKind::Named, "id", ty.clone(), ty.clone(), L) else {
            return Ok(()); // rejected at intake — fine.
        };
        {
            let mut fb = b.build_fn(f).unwrap();
            let x = fb.input();
            // Bare full-value move x -> ret (ty == output ty).
            let _ = fb.output(x, None, L);
            let _ = fb.finish();
        }
        if let Ok(ir) = b.seal(f) {
            let viol = validate(&ir);
            prop_assert!(viol.is_empty(), "seal Ok but validate found {viol:?} for ty {ty:?}");
        }
    }
}

/// Determinism (DESIGN §16 item 6; I12): building the same program twice yields
/// byte-identical `to_mermaid` AND identical `topo_order` / `sccs`. Two fresh
/// `IrBuilder` instances driven by an identical construction order issue
/// identical slotmap keys (append-only, no removals), so the key-bearing
/// `topo_order` / `sccs` results compare equal verbatim — this is the I12
/// guarantee verified, not assumed from slotmap's contract. We use a loop body
/// so the `sccs` comparison ranges over a non-trivial SCC, not just singletons.
#[test]
fn determinism_same_program_same_output() {
    fn build() -> (
        String,
        Vec<mapal_ir::MorphismId>,
        Vec<Vec<mapal_ir::ObjectId>>,
    ) {
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
        let order = ir.topo_order(f);
        let sccs = ir.sccs(f);
        (ir.to_mermaid(), order, sccs)
    }
    let (m1, o1, s1) = build();
    let (m2, o2, s2) = build();
    assert_eq!(m1, m2, "byte-identical mermaid");
    assert_eq!(
        o1, o2,
        "identical topo_order (same morphism keys, same order)"
    );
    assert_eq!(s1, s2, "identical sccs (same object keys, same partition)");
    // The sccs comparison is non-vacuous: a counting loop has one non-trivial SCC.
    assert!(
        s1.iter().any(|c| c.len() > 1),
        "loop yields a non-trivial SCC"
    );
}
