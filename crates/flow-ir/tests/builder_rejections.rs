//! Integration-level rejection matrix (DESIGN §16 item 1) and token properties
//! (§16 item 5), all through the public builder API. The in-crate split-file
//! unit tests (`src/builder/tests.rs`) cover every `IrError` variant; this file
//! re-checks the reviewer-named holes at integration scope and adds the token
//! reachability-ordering / linearity properties.

use flow_ir::{Dest, FuncKind, IrBuilder, IrError, Operation, SourceLoc, Ty, Value, validate};

const L: SourceLoc = SourceLoc { start: 0, end: 0 };

// --- the §16 reviewer-named holes -----------------------------------------

#[test]
fn binop_eq_bool_ok_but_lt_bool_rejects() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", Ty::Bool, Ty::Bool, L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let t = fb.constant(Value::Bool(true), L).unwrap();
    assert!(fb.binop(Operation::Eq, x, t, Dest::Fresh(None), L).is_ok());
    assert!(matches!(
        fb.binop(Operation::Lt, x, t, Dest::Fresh(None), L),
        Err(IrError::TypeMismatch { .. })
    ));
}

#[test]
fn pack_of_str_constant_rejects() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(
            FuncKind::Named,
            "m",
            Ty::i32(),
            Ty::Tuple(vec![Ty::Str, Ty::i32()]),
            L,
        )
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let s = fb.constant(Value::Str("x".into()), L).unwrap();
    assert_eq!(
        fb.pack(&[s, x], Dest::Fresh(None), L),
        Err(IrError::StrOutsidePrint)
    );
}

#[test]
fn pack_singleton_rejects() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    assert_eq!(
        fb.pack(&[x], Dest::Fresh(None), L),
        Err(IrError::SingletonTuple)
    );
}

#[test]
fn ret_slot_on_scalar_rejects() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let one = fb.constant(Value::I32(1), L).unwrap();
    assert_eq!(
        fb.binop(Operation::Add, x, one, Dest::Ret { slot: Some(0) }, L),
        Err(IrError::RetNotProduct)
    );
}

#[test]
fn phi_with_token_branch_rejects() {
    let pair = Ty::Tuple(vec![Ty::IoToken, Ty::i32()]);
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", pair.clone(), pair, L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let x = fb.input();
    let c = fb.constant(Value::Bool(true), L).unwrap();
    assert_eq!(
        fb.phi(x, x, c, Dest::Fresh(None), L),
        Err(IrError::TokenInPhi)
    );
}

#[test]
fn two_pixel_decls_with_different_fields_reject_at_seal() {
    let p1 = Ty::Struct {
        name: "Pixel".into(),
        fields: vec![("r".into(), Ty::u8())],
    };
    let p2 = Ty::Struct {
        name: "Pixel".into(),
        fields: vec![("r".into(), Ty::i32())],
    };
    let mut b = IrBuilder::new();
    let f = b.declare(FuncKind::Named, "f", p1.clone(), p1, L).unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let inp = fb.input();
        fb.output(inp, None, L).unwrap();
        fb.finish().unwrap();
    }
    let g = b.declare(FuncKind::Named, "g", p2.clone(), p2, L).unwrap();
    {
        let mut fb = b.build_fn(g).unwrap();
        let inp = fb.input();
        fb.output(inp, None, L).unwrap();
        fb.finish().unwrap();
    }
    assert_eq!(b.seal(f).unwrap_err(), IrError::StructNameConflict);
}

#[test]
fn dangling_final_token_rejects() {
    // A token produced but never threaded to Return → a token error at seal.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::Unit, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s = fb.constant(Value::Str("x".into()), L).unwrap();
        let _t1 = fb.print(t0, s, L).unwrap(); // t1 is dropped, never reaches ret.
        fb.finish().unwrap();
    }
    // Output is Unit (no ret writer needed), so the only violation is the
    // dangling token (I4b) → TokenDropped.
    assert_eq!(b.seal(f).unwrap_err(), IrError::TokenDropped);
}

// --- token properties (DESIGN §16 item 5) ---------------------------------

#[test]
fn token_chain_ends_at_return() {
    // Every token chain runs Parameter → … → Return (I4b). A clean two-print
    // program validates, and the final token feeds Return.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s = fb.constant(Value::Str("hi".into()), L).unwrap();
        let t1 = fb.print(t0, s, L).unwrap();
        fb.output(t1, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    assert!(validate(&ir).is_empty());
    // The Return object has a token-bearing in-edge (the final token); the
    // token chain runs Parameter → … → Return (I4b).
    let ret = ir.func(f).unwrap().output;
    assert!(!ir.in_edges(ret).is_empty());
    assert_eq!(ir.object(ret).unwrap().ty, Ty::IoToken);
}

#[test]
fn two_prints_are_reachability_ordered() {
    // Two Print morphisms in one function are reachability-ordered: the second's
    // token source is reachable from the first's token output (E2 structural
    // determinism). We verify the second print's token input is the first's
    // output token.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s1 = fb.constant(Value::Str("a".into()), L).unwrap();
        let t1 = fb.print(t0, s1, L).unwrap();
        let s2 = fb.constant(Value::Str("b".into()), L).unwrap();
        let t2 = fb.print(t1, s2, L).unwrap();
        fb.output(t2, None, L).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).unwrap();
    // Collect the two Print morphisms; the second's token-pair source-0 must be
    // the first's output object (the chain is serialized by dataflow).
    let prints: Vec<_> = ir
        .func(f)
        .unwrap()
        .morphisms
        .iter()
        .filter(|&&m| matches!(ir.morphism(m).unwrap().op, Operation::Print { .. }))
        .copied()
        .collect();
    assert_eq!(prints.len(), 2);
    let first_out = ir.morphism(prints[0]).unwrap().target;
    // The second print's pair (source) has a Pair 0/2 in-edge from first_out.
    let second_pair = ir.morphism(prints[1]).unwrap().source;
    let feeds_token = ir.in_edges(second_pair).iter().any(|&m| {
        let mm = ir.morphism(m).unwrap();
        matches!(mm.op, Operation::Pair { slot: 0, .. }) && mm.source == first_out
    });
    assert!(feeds_token, "second print consumes the first print's token");
}

// --- review fix-round regressions -----------------------------------------

/// F1: a `Str`-bearing declared input/output ty reaches the graph without going
/// through a packer, so `seal` (not just `validate`) must reject it — otherwise
/// the headline "seal Ok ⇒ validate empty" property is violated (DESIGN §9 I9s /
/// §16 item 3). Covers bare `Str` and `Str` nested in a tuple / struct / array.
#[test]
fn str_bearing_declared_ty_rejected_at_seal() {
    fn build_and_seal(io_ty: Ty) -> Result<(), IrError> {
        let mut b = IrBuilder::new();
        let f = b.declare(FuncKind::Named, "id", io_ty.clone(), io_ty, L)?;
        {
            let mut fb = b.build_fn(f).unwrap();
            let x = fb.input();
            fb.output(x, None, L).unwrap();
            fb.finish().unwrap();
        }
        b.seal(f).map(|_| ())
    }

    // Bare Str param/return.
    assert_eq!(build_and_seal(Ty::Str), Err(IrError::StrOutsidePrint));
    // Str inside a tuple (not the print pair).
    assert_eq!(
        build_and_seal(Ty::Tuple(vec![Ty::i32(), Ty::Str])),
        Err(IrError::StrOutsidePrint)
    );
    // Str inside a struct field.
    assert_eq!(
        build_and_seal(Ty::Struct {
            name: "S".into(),
            fields: vec![("a".into(), Ty::Str)],
        }),
        Err(IrError::StrOutsidePrint)
    );
    // Str as an array element.
    assert_eq!(
        build_and_seal(Ty::Array {
            elem: Box::new(Ty::Str),
            size: 2,
        }),
        Err(IrError::StrOutsidePrint)
    );
}

/// F2 / SND-1: a second `LoopBack` whose carried state comes from OUTSIDE the
/// loop SCC must be `LoopBackOutsideScc` — even when it shares the real in-SCC
/// loop guard `cond`. The route `(state, cond)` is always pulled into the SCC by
/// the `cond` slot edge, so the per-edge I5 check must inspect the carried STATE
/// (the route's slot-0 source), not the route object (DESIGN §9 I5).
#[test]
fn second_loopback_from_const_with_real_cond_rejects() {
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
        // The REAL in-SCC guard: i < 10.
        let cond = fb
            .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, inext, cond, L).unwrap();
        // Second back edge: state is a CONSTANT (in-degree 0, outside any cycle)
        // but the cond is the genuine in-SCC guard.
        let seven = fb.constant(Value::I32(7), L).unwrap();
        fb.loop_back(&lh, seven, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    assert_eq!(b.seal(f).unwrap_err(), IrError::LoopBackOutsideScc);
}

/// F2 control: a legitimate single-back-edge counting loop (carried state from
/// inside the SCC) still seals and validates clean — the I5 fix must not reject
/// well-formed loops.
#[test]
fn canonical_counting_loop_still_seals() {
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
            .binop(Operation::Lt, merge, ten, Dest::Fresh(None), L)
            .unwrap();
        let one = fb.constant(Value::I32(1), L).unwrap();
        let inext = fb
            .binop(Operation::Add, merge, one, Dest::Fresh(None), L)
            .unwrap();
        fb.loop_back(&lh, inext, cond, L).unwrap();
        fb.loop_exit(&lh, merge, cond, Dest::Ret { slot: None }, L)
            .unwrap();
        fb.end_loop(lh).unwrap();
        fb.finish().unwrap();
    }
    let ir = b.seal(f).expect("canonical loop seals");
    assert!(validate(&ir).is_empty());
}

/// F4 / SND-3: an array output ty with `size > u32::MAX` is not slot-addressable
/// (the `Pair { arity: u32 }` field cannot name its slots). A ret-slot write must
/// be rejected with `SlotOutOfRange` rather than silently truncating the arity to
/// `u32` and passing the `k < arity` bounds check (DESIGN §3 keeps `Array.size`
/// a `u64`).
#[test]
fn ret_slot_into_oversize_array_rejects() {
    let mut b = IrBuilder::new();
    let out = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: (u32::MAX as u64) + 6, // 4_294_967_301
    };
    let f = b
        .declare(FuncKind::Named, "main", Ty::i32(), out, L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let inp = fb.input();
    // Slot 4 would pass the truncated arity (5) but is meaningless for a
    // ~4.3e9-element array; must reject.
    assert_eq!(fb.output(inp, Some(4), L), Err(IrError::SlotOutOfRange));
}

/// F4 boundary: `product_arity()` reports the TRUE `u64` size without truncating
/// to `u32` (the bug that let an out-of-bounds slot pass `k < arity`). An array
/// just above `u32::MAX` keeps its real arity; the previous `*size as u32` cast
/// would have wrapped `(u32::MAX as u64) + 6` to `5`.
#[test]
fn array_product_arity_is_untruncated_u64() {
    let over = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: (u32::MAX as u64) + 6,
    };
    assert_eq!(over.product_arity(), Some((u32::MAX as u64) + 6));
    // Not slot-addressable: the u32 view is None (would-have-truncated to 5).
    assert_eq!(over.product_arity_u32(), None);

    // Exactly u32::MAX is the largest slot-addressable array.
    let at = Ty::Array {
        elem: Box::new(Ty::i32()),
        size: u32::MAX as u64,
    };
    assert_eq!(at.product_arity(), Some(u32::MAX as u64));
    assert_eq!(at.product_arity_u32(), Some(u32::MAX));
}

/// SND-2: cross-builder id mixing is unsupported (DESIGN §10). A `FuncId` minted
/// by one builder collides with the same slotmap key in another builder, so the
/// "versioned-key defense" the prose claims does NOT hold across distinct
/// builders. This test PINS the actual behavior — passing a foreign `FuncId` to
/// `call` resolves to whatever local function shares that key — so the false
/// rationale is not silently relied on downstream. (See the report `notes`: the
/// faithful fix per the do-not-modify-DESIGN constraint is to pin the documented
/// UB rather than rewrite the spec text.)
#[test]
fn cross_builder_funcid_mixing_is_unsupported_ub() {
    // Builder B declares two fns so its second FuncId is the 2nd slotmap key.
    let mut bb = IrBuilder::new();
    let _bg = bb
        .declare(FuncKind::Named, "g", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let b_second = bb
        .declare(FuncKind::Named, "g2", Ty::i32(), Ty::i32(), L)
        .unwrap();

    // Builder A declares main + helper; helper shares b_second's key.
    let mut ba = IrBuilder::new();
    let main = ba
        .declare(FuncKind::Named, "main", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let helper = ba
        .declare(FuncKind::Named, "helper", Ty::i32(), Ty::i32(), L)
        .unwrap();
    // The collision is the whole point: foreign and local keys are equal.
    assert_eq!(
        b_second, helper,
        "slotmap re-issues identical keys across builders (the documented hazard)"
    );

    // Passing B's FuncId to A's `call` resolves to A's `helper` (same key) — NOT
    // an UnknownFunction. This is UB per DESIGN §10; the test pins it so the
    // false "versioned-key" claim cannot be trusted as a real defense.
    {
        let mut fb = ba.build_fn(main).unwrap();
        let x = fb.input();
        let r = fb.call(b_second, x, Dest::Ret { slot: None }, L);
        assert!(
            r.is_ok(),
            "foreign FuncId silently resolves to the colliding local fn (documented UB)"
        );
        fb.finish().unwrap();
    }
    {
        let mut fb = ba.build_fn(helper).unwrap();
        let x = fb.input();
        fb.output(x, None, L).unwrap();
        fb.finish().unwrap();
    }
    // The graph even seals clean — confirming the hole the finding describes.
    let ir = ba
        .seal(main)
        .expect("seals (UB: wrong-but-well-formed call edge)");
    assert!(validate(&ir).is_empty());
}

/// TY-1: a zero-field `Ty::Struct` is product `1` whose literal mints a Temporary
/// with ZERO in-edges — `seal` returned Ok while `validate` flagged `BadInEdges`,
/// breaking the headline "seal Ok ⇒ validate empty" property (DESIGN §16 item 3).
///
/// Layer 1 (intake / the I9 path): a zero-field struct ty is `NonCoreType` wherever
/// a ty enters the builder, so it is unconstructible at the source. We assert this
/// from BOTH `declare` entry points (input and output ty), since both flow through
/// the same `intake_ty`.
#[test]
fn zero_field_struct_ty_rejected_at_intake() {
    let empty = Ty::Struct {
        name: "Empty".into(),
        fields: vec![],
    };
    // As a declared INPUT ty.
    let mut b = IrBuilder::new();
    assert_eq!(
        b.declare(FuncKind::Named, "a", empty.clone(), Ty::Unit, L),
        Err(IrError::NonCoreType)
    );
    // As a declared OUTPUT ty.
    let mut b = IrBuilder::new();
    assert_eq!(
        b.declare(FuncKind::Named, "b", Ty::Unit, empty.clone(), L),
        Err(IrError::NonCoreType)
    );
    // Nested inside a tuple (the iterative intake walk reaches it).
    let mut b = IrBuilder::new();
    assert_eq!(
        b.declare(
            FuncKind::Named,
            "c",
            Ty::Tuple(vec![Ty::i32(), empty]),
            Ty::Unit,
            L
        ),
        Err(IrError::NonCoreType)
    );
}

/// TY-1 layer 2 (defense in depth): `pack_struct` with zero components is
/// `EmptyProduct`, mirroring `pack`. Layer 1 makes a zero-FIELD struct ty
/// unconstructible, so a well-typed `(zero-field ty, &[])` pair is unreachable
/// through intake; this guard instead fires on the empty COMPONENT list directly,
/// independent of the named struct's field count — so it is reachable and tested
/// with a fielded struct ty (the empty component list is reported before the
/// arity mismatch).
#[test]
fn pack_struct_zero_components_is_empty_product() {
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "m", Ty::i32(), Ty::i32(), L)
        .unwrap();
    let mut fb = b.build_fn(f).unwrap();
    let sty = Ty::Struct {
        name: "P".into(),
        fields: vec![("a".into(), Ty::i32())],
    };
    assert_eq!(
        fb.pack_struct(sty, &[], Dest::Fresh(None), L),
        Err(IrError::EmptyProduct)
    );
}

#[test]
fn token_double_consumption_is_unconstructible() {
    // Reusing a token in two independent prints → the second makes the token
    // non-linear; seal rejects with TokenNotLinear.
    let mut b = IrBuilder::new();
    let f = b
        .declare(FuncKind::Named, "main", Ty::IoToken, Ty::IoToken, L)
        .unwrap();
    {
        let mut fb = b.build_fn(f).unwrap();
        let t0 = fb.input();
        let s1 = fb.constant(Value::Str("a".into()), L).unwrap();
        let s2 = fb.constant(Value::Str("b".into()), L).unwrap();
        // Both prints consume t0 — double use.
        let t1 = fb.print(t0, s1, L).unwrap();
        let _t2 = fb.print(t0, s2, L).unwrap();
        fb.output(t1, None, L).unwrap();
        fb.finish().unwrap();
    }
    assert_eq!(b.seal(f).unwrap_err(), IrError::TokenNotLinear);
}
