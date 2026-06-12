//! `validate()` — the independent oracle (DESIGN §11).
//!
//! A from-scratch pass over a sealed [`CategoryIr`] that re-derives the
//! invariants **without sharing code with the builder's checks**. Its purpose is
//! (a) the property test's oracle — *every graph the public API produces
//! validates clean* — and (b) a debug-assert hook for future passes.
//!
//! **Independence is scoped honestly** (DESIGN §11): each invariant splits into a
//! *graph-shape clause* (validate-checkable from the sealed graph alone) and,
//! for some, an *API-discipline / provenance clause* (builder-only). validate()
//! certifies exactly the graph-shape clauses. The provenance clauses it cannot
//! check are listed below.
//!
//! Clauses validate() **cannot** independently certify (provenance only):
//! - I2: that each edge arose from the builder's per-call typing dispatch — but
//!   the *resulting* §5.1 shape is re-derived here from tys.
//! - I7: that `constant()` is the only setter of `value` — but the
//!   `value.is_some() ⇔ kind==Constant ∧ value.ty()==ty` shape *is* checked.
//! - I8: the "via the ret-write API" provenance — reduced here to the
//!   graph-shape clause "`op == Output` ⇒ `target.kind == Return` ∧ source ty =
//!   target ty".
//! - The I4 loop-fork *origin* (a `LoopHandle`) — re-expressed structurally via
//!   SCC membership.

use slotmap::SecondaryMap;

use crate::graph::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation};
use crate::ty::{self, Ty};

/// A well-formedness violation found by [`validate`] (DESIGN §11). Mirrors
/// `IrError` but carries ids instead of build context — no `Display` (C3).
#[derive(Clone, Debug, PartialEq)]
pub enum IrViolation {
    /// An object's in-edge set does not match any I3 shape.
    BadInEdges(ObjectId),
    /// An edge violates the §5.1 typing table.
    BadEdgeType(MorphismId),
    /// A `Constant` with `value` shape wrong, or a non-Constant with a value.
    BadConstant(ObjectId),
    /// A Parameter/Constant with a non-empty in-edge set.
    SourceHasInEdges(ObjectId),
    /// A Return object with out-edges (must be a sink).
    ReturnHasOutEdges(ObjectId),
    /// `op == Output` whose target is not a Return, or ty mismatch.
    BadOutput(MorphismId),
    /// I4: a token-bearing object with >1 token consumer outside the loop fork.
    TokenNotLinear(ObjectId),
    /// I4b: a token-bearing non-Return object with no token consumer.
    TokenDropped(ObjectId),
    /// I4: a `Phi` selecting a token-bearing ty.
    TokenInPhi(MorphismId),
    /// I4: a token inside a Map/Fold body.
    TokenInBody(FuncId),
    /// §8: a loop-carried token with a token-free exit payload.
    TokenNotEscaping(ObjectId),
    /// I9s: `Str` in a product/array outside the print pair.
    StrOutsidePrint(ObjectId),
    /// Two `Struct` tys sharing a name with different fields.
    StructNameConflict(String),
    /// A cross-function morphism (source/target owners differ).
    CrossFunctionEdge(MorphismId),
    /// An object/morphism with no owner.
    Unowned(ObjectId),
    /// I5: a `LoopMerge` whose SCC placement is malformed.
    BadLoop(ObjectId),
    /// I6: a cycle in the function reference graph.
    RecursiveReference(Vec<FuncId>),
    /// I-RET: mixed full/slot writers, missing slot, etc.
    BadReturn(ObjectId),
    /// I10: a ty deeper than `MAX_TY_DEPTH`.
    TyTooDeep(ObjectId),
    /// I9: a non-Core ty on some object.
    NonCoreType(ObjectId),
}

/// Re-derive every graph-shape invariant on a sealed graph (DESIGN §11). An
/// empty vec means well-formed.
pub fn validate(ir: &CategoryIr) -> Vec<IrViolation> {
    let mut v = Vec::new();
    check_ownership(ir, &mut v);
    check_objects(ir, &mut v);
    check_edges(ir, &mut v);
    check_in_edge_shapes(ir, &mut v);
    check_tokens(ir, &mut v);
    check_str(ir, &mut v);
    check_struct_names(ir, &mut v);
    check_returns(ir, &mut v);
    check_loops(ir, &mut v);
    check_references(ir, &mut v);
    v
}

/// I6 cross-function + every object/morphism owned.
fn check_ownership(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (id, _) in ir.objects() {
        if ir.object(id).is_some() && ir.try_owner(id).is_none() {
            v.push(IrViolation::Unowned(id));
        }
    }
    for (mid, m) in ir.morphisms() {
        let so = ir.try_owner(m.source);
        let to = ir.try_owner(m.target);
        if so.is_none() || to.is_none() || so != to {
            v.push(IrViolation::CrossFunctionEdge(mid));
        }
    }
}

/// I7 constant shape, I9/I10 ty intake re-derivation, Return-is-sink,
/// Parameter/Constant in-edge-free.
fn check_objects(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (id, obj) in ir.objects() {
        if !ty::ty_depth_ok(&obj.ty) {
            v.push(IrViolation::TyTooDeep(id));
            continue;
        }
        if !ty_is_core(&obj.ty) {
            v.push(IrViolation::NonCoreType(id));
        }
        // I7: value.is_some() ⇔ kind==Constant, and value.ty()==ty.
        match (&obj.value, obj.kind) {
            (Some(val), ObjectKind::Constant) => {
                if val.ty() != obj.ty {
                    v.push(IrViolation::BadConstant(id));
                }
            }
            (Some(_), _) => v.push(IrViolation::BadConstant(id)),
            (None, ObjectKind::Constant) => v.push(IrViolation::BadConstant(id)),
            (None, _) => {}
        }
        // Parameter/Constant must have no in-edges.
        if matches!(obj.kind, ObjectKind::Parameter | ObjectKind::Constant)
            && !ir.in_edges(id).is_empty()
        {
            v.push(IrViolation::SourceHasInEdges(id));
        }
        // Return is a sink.
        if obj.kind == ObjectKind::Return && !ir.out_edges(id).is_empty() {
            v.push(IrViolation::ReturnHasOutEdges(id));
        }
    }
}

/// I2 (graph-shape clause) + I8: re-derive the §5.1 typing table from tys.
fn check_edges(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (mid, m) in ir.morphisms() {
        let (sty, tty) = match (ir.object(m.source), ir.object(m.target)) {
            (Some(s), Some(t)) => (&s.ty, &t.ty),
            _ => {
                v.push(IrViolation::BadEdgeType(mid));
                continue;
            }
        };
        if !edge_type_ok(ir, m, sty, tty) {
            v.push(IrViolation::BadEdgeType(mid));
        }
        // I8 graph-shape clause: Output ⇒ target is Return ∧ source ty == target ty.
        if m.op == Operation::Output {
            let tgt_kind = ir.object(m.target).map(|o| o.kind);
            if tgt_kind != Some(ObjectKind::Return) || sty != tty {
                v.push(IrViolation::BadOutput(mid));
            }
        }
    }
}

/// I2: whether a single edge satisfies its §5.1 row, derived only from tys.
fn edge_type_ok(ir: &CategoryIr, m: &crate::graph::Morphism, sty: &Ty, tty: &Ty) -> bool {
    match m.op {
        Operation::Pair { slot, arity } => {
            // target is a product of the given arity; source matches slot ty.
            // `product_arity()` is `u64` (F4/SND-3); the `Pair` field is `u32`,
            // so a > u32::MAX-arity array can never have a matching `Pair` edge.
            match tty.product_arity() {
                Some(a) if a == arity as u64 && slot < arity => {
                    tty.component_ty(slot).map(|c| c == sty).unwrap_or(false)
                }
                _ => false,
            }
        }
        Operation::Proj { index } => {
            matches!(sty, Ty::Tuple(_) | Ty::Struct { .. })
                && sty.component_ty(index).map(|c| c == tty).unwrap_or(false)
        }
        Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
            two_tuple(sty)
                .map(|(a, b)| a == b && a.is_numeric() && tty == a)
                .unwrap_or(false)
        }
        Operation::Neg => sty.is_numeric() && sty == tty,
        Operation::Eq | Operation::Neq => two_tuple(sty)
            .map(|(a, b)| a == b && (a.is_numeric() || *a == Ty::Bool) && *tty == Ty::Bool)
            .unwrap_or(false),
        Operation::Lt | Operation::Gt | Operation::Le | Operation::Ge => two_tuple(sty)
            .map(|(a, b)| a == b && a.is_numeric() && *tty == Ty::Bool)
            .unwrap_or(false),
        Operation::And | Operation::Or => two_tuple(sty)
            .map(|(a, b)| *a == Ty::Bool && *b == Ty::Bool && *tty == Ty::Bool)
            .unwrap_or(false),
        Operation::Not => *sty == Ty::Bool && *tty == Ty::Bool,
        Operation::Phi => match sty {
            Ty::Tuple(ts) if ts.len() == 3 => {
                ts[0] == ts[1]
                    && ts[2] == Ty::Bool
                    && &ts[0] == tty
                    && !ty::ty_contains_token(&ts[0])
            }
            _ => false,
        },
        Operation::Call(g) => match ir.func(g) {
            Some(def) => {
                ir.object(def.input).map(|o| &o.ty) == Some(sty)
                    && ir.object(def.output).map(|o| &o.ty) == Some(tty)
            }
            None => false,
        },
        Operation::Map { body } => match (sty, tty, ir.func(body)) {
            (Ty::Array { elem: se, size: sn }, Ty::Array { elem: te, size: tn }, Some(def)) => {
                sn == tn
                    && ir.object(def.input).map(|o| &o.ty) == Some(&**se)
                    && ir.object(def.output).map(|o| &o.ty) == Some(&**te)
            }
            _ => false,
        },
        Operation::Fold { body } => match (sty, ir.func(body)) {
            (Ty::Tuple(ts), Some(def)) if ts.len() == 2 => {
                if let Ty::Array { elem, .. } = &ts[1] {
                    let body_in = ir.object(def.input).map(|o| o.ty.clone());
                    let body_out = ir.object(def.output).map(|o| o.ty.clone());
                    let want_in = Ty::Tuple(vec![ts[0].clone(), (**elem).clone()]);
                    body_in == Some(want_in) && body_out.as_ref() == Some(&ts[0]) && tty == &ts[0]
                } else {
                    false
                }
            }
            _ => false,
        },
        Operation::Index => match sty {
            Ty::Tuple(ts) if ts.len() == 2 => match &ts[0] {
                Ty::Array { elem, .. } => ts[1].is_integer() && &**elem == tty,
                _ => false,
            },
            _ => false,
        },
        Operation::Print => match sty {
            Ty::Tuple(ts) if ts.len() == 2 => {
                ts[0] == Ty::IoToken && ts[1].is_printable() && *tty == Ty::IoToken
            }
            _ => false,
        },
        Operation::LoopEnter => {
            sty == tty && ir.object(m.target).map(|o| o.kind) == Some(ObjectKind::LoopMerge)
        }
        Operation::LoopBack => match sty {
            Ty::Tuple(ts) if ts.len() == 2 => {
                ts[1] == Ty::Bool
                    && &ts[0] == tty
                    && ir.object(m.target).map(|o| o.kind) == Some(ObjectKind::LoopMerge)
            }
            _ => false,
        },
        Operation::LoopExit => match sty {
            Ty::Tuple(ts) if ts.len() == 2 => ts[1] == Ty::Bool && &ts[0] == tty,
            _ => false,
        },
        Operation::Output => sty == tty,
    }
}

/// I3: re-derive the one-definition rule from in-edge sets.
fn check_in_edge_shapes(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (id, obj) in ir.objects() {
        let ins = ir.in_edges(id);
        let ok = match obj.kind {
            ObjectKind::Parameter | ObjectKind::Constant => ins.is_empty(),
            ObjectKind::LoopMerge => {
                // 1 LoopEnter + ≥1 LoopBack, nothing else.
                let mut enters = 0;
                let mut backs = 0;
                let mut other = 0;
                for &mid in ins {
                    match ir.morphism(mid).unwrap().op {
                        Operation::LoopEnter => enters += 1,
                        Operation::LoopBack => backs += 1,
                        _ => other += 1,
                    }
                }
                enters == 1 && backs >= 1 && other == 0
            }
            ObjectKind::Return => true, // I-RET handled separately.
            ObjectKind::Temporary => {
                // (b) one value-producing definer, OR (c) exactly `arity` Pair
                // edges with distinct slots (product object).
                let all_pair = !ins.is_empty()
                    && ins
                        .iter()
                        .all(|&mid| matches!(ir.morphism(mid).unwrap().op, Operation::Pair { .. }));
                if all_pair {
                    product_pair_shape_ok(ir, &obj.ty, ins)
                } else {
                    ins.len() == 1
                }
            }
        };
        if !ok {
            v.push(IrViolation::BadInEdges(id));
        }
    }
}

/// Whether a product object's `Pair` in-edges cover exactly its arity with
/// distinct slots.
fn product_pair_shape_ok(ir: &CategoryIr, ty: &Ty, ins: &[MorphismId]) -> bool {
    // `product_arity()` is `u64` (F4/SND-3); `Pair` slot/arity fields are `u32`.
    // A product whose arity exceeds the in-edge count (incl. any > u32::MAX-arity
    // array) cannot be exactly covered, so the count check rejects it.
    let arity = match ty.product_arity() {
        Some(a) => a,
        None => return false,
    };
    if ins.len() as u64 != arity {
        return false;
    }
    let mut seen: Vec<u32> = Vec::with_capacity(ins.len());
    for &mid in ins {
        if let Operation::Pair { slot, arity: a } = ir.morphism(mid).unwrap().op {
            if a as u64 != arity || (slot as u64) >= arity || seen.contains(&slot) {
                return false;
            }
            seen.push(slot);
        } else {
            return false;
        }
    }
    true
}

/// I4 + I4b: token linearity (with the structural loop-fork exception) and the
/// token sink rule, all SCC-derived.
fn check_tokens(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    // Precompute per-function SCC membership for the loop-fork exception.
    let scc_of = build_scc_membership(ir);

    for (id, obj) in ir.objects() {
        if !ty::ty_contains_token(&obj.ty) {
            continue;
        }
        // Token-bearing consumers: out-edges whose target ty contains a token.
        let mut token_consumers: Vec<MorphismId> = Vec::new();
        for &mid in ir.out_edges(id) {
            let tgt = ir.morphism(mid).unwrap().target;
            if ir
                .object(tgt)
                .map(|o| ty::ty_contains_token(&o.ty))
                .unwrap_or(false)
            {
                token_consumers.push(mid);
            }
        }

        // I4b token sink: zero token consumers ⇒ must be the owning Return.
        if token_consumers.is_empty() {
            let owner = ir.owner(id);
            let is_return = ir.func(owner).map(|d| d.output) == Some(id);
            if !is_return {
                v.push(IrViolation::TokenDropped(id));
            }
            continue;
        }

        // I4 linearity: at most one token consumer, except the loop fork.
        if token_consumers.len() > 1 && !is_loop_fork(ir, &scc_of, id, &token_consumers) {
            v.push(IrViolation::TokenNotLinear(id));
        }
    }

    // Phi may not select a token-bearing ty.
    for (mid, m) in ir.morphisms() {
        if m.op == Operation::Phi
            && let Some(o) = ir.object(m.target)
            && ty::ty_contains_token(&o.ty)
        {
            v.push(IrViolation::TokenInPhi(mid));
        }
    }

    // Map/Fold bodies must be token-free.
    for (fid, def) in ir.funcs() {
        if matches!(
            def.kind,
            crate::graph::FuncKind::MapBody | crate::graph::FuncKind::FoldBody
        ) {
            let in_tok = ir
                .object(def.input)
                .map(|o| ty::ty_contains_token(&o.ty))
                .unwrap_or(false);
            let out_tok = ir
                .object(def.output)
                .map(|o| ty::ty_contains_token(&o.ty))
                .unwrap_or(false);
            let any_tok = def.morphisms.iter().any(|&m| {
                let mm = ir.morphism(m).unwrap();
                ir.object(mm.source)
                    .map(|o| ty::ty_contains_token(&o.ty))
                    .unwrap_or(false)
            });
            if in_tok || out_tok || any_tok {
                v.push(IrViolation::TokenInBody(fid));
            }
        }
    }
}

/// The structural loop fork (DESIGN §8/I4): an object inside a loop SCC with
/// exactly two token consumers, both `Pair` edges into route objects, one route
/// consumed by a `LoopBack` into a merge whose SCC contains the object, the
/// other by a `LoopExit` whose source is in that SCC.
fn is_loop_fork(
    ir: &CategoryIr,
    scc_of: &SecondaryMap<ObjectId, usize>,
    obj: ObjectId,
    consumers: &[MorphismId],
) -> bool {
    if consumers.len() != 2 {
        return false;
    }
    let obj_scc = match scc_of.get(obj) {
        Some(&s) => s,
        None => return false,
    };
    // Both consumers must be Pair edges; classify each consumer's forward cone
    // (the back route reaches a LoopBack into the SCC; the exit route reaches a
    // LoopExit leaving it). When U is a product, the fork is several edges
    // upstream of the route, so we trace cones rather than one hop.
    let mut has_back = false;
    let mut has_exit = false;
    for &mid in consumers {
        let m = ir.morphism(mid).unwrap();
        if !matches!(m.op, Operation::Pair { .. }) {
            return false;
        }
        match cone_classify(ir, scc_of, m.target, obj_scc) {
            Some(true) => has_back = true,
            Some(false) => has_exit = true,
            None => {}
        }
    }
    has_back && has_exit
}

/// Classify a forward cone from `start`: `Some(true)` if it reaches a `LoopBack`
/// into `scc`, `Some(false)` if it reaches a `LoopExit` leaving `scc`, else
/// `None` (iterative BFS; J1). Independent local copy (no builder code shared).
fn cone_classify(
    ir: &CategoryIr,
    scc_of: &SecondaryMap<ObjectId, usize>,
    start: ObjectId,
    scc: usize,
) -> Option<bool> {
    let mut seen: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
    let mut queue: Vec<ObjectId> = vec![start];
    seen.insert(start, true);
    let mut found_exit = false;
    while let Some(o) = queue.pop() {
        for &m in ir.out_edges(o) {
            let mm = ir.morphism(m).unwrap();
            match mm.op {
                Operation::LoopBack if scc_of.get(mm.target) == Some(&scc) => return Some(true),
                Operation::LoopExit if scc_of.get(mm.target) != Some(&scc) => found_exit = true,
                _ => {}
            }
            let t = mm.target;
            if !seen.get(t).copied().unwrap_or(false) {
                seen.insert(t, true);
                queue.push(t);
            }
        }
    }
    if found_exit { Some(false) } else { None }
}

/// I9s: `Str` may appear only as a Constant ty or the second component of a
/// `(IoToken, Str)` print pair. Re-derived from product membership.
fn check_str(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (id, obj) in ir.objects() {
        match &obj.ty {
            // A bare Str object is fine only if it is a Constant (the literal).
            Ty::Str => {
                if obj.kind != ObjectKind::Constant {
                    // A Str-typed temporary is only legitimate as the value
                    // feeding a print pair — but as a standalone object that is
                    // still just a Str scalar (the Constant). A non-constant Str
                    // scalar would be a lowering bug; flag it.
                    v.push(IrViolation::StrOutsidePrint(id));
                }
            }
            // The print pair: (IoToken, Str) is allowed; any other product
            // containing Str is not.
            Ty::Tuple(ts) => {
                if ts.iter().any(ty::ty_contains_str) {
                    let is_print_pair = ts.len() == 2 && ts[0] == Ty::IoToken && ts[1] == Ty::Str;
                    if !is_print_pair {
                        v.push(IrViolation::StrOutsidePrint(id));
                    }
                }
            }
            other => {
                if ty::ty_contains_str(other) {
                    v.push(IrViolation::StrOutsidePrint(id));
                }
            }
        }
    }
}

/// StructNameConflict: all `Struct`s sharing a name must have identical fields.
fn check_struct_names(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    let mut seen: Vec<(String, Vec<(String, Ty)>)> = Vec::new();
    for (_, obj) in ir.objects() {
        let mut stack: Vec<&Ty> = vec![&obj.ty];
        while let Some(t) = stack.pop() {
            match t {
                Ty::Struct { name, fields } => {
                    if let Some((_, prev)) = seen.iter().find(|(n, _)| n == name) {
                        if prev != fields {
                            v.push(IrViolation::StructNameConflict(name.clone()));
                        }
                    } else {
                        seen.push((name.clone(), fields.clone()));
                    }
                    for (_, inner) in fields {
                        stack.push(inner);
                    }
                }
                Ty::Tuple(ts) => stack.extend(ts.iter()),
                Ty::Array { elem, .. } => stack.push(elem),
                _ => {}
            }
        }
    }
}

/// I-RET: Return in-edge completeness, re-derived per function.
fn check_returns(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (_, def) in ir.funcs() {
        let ret = def.output;
        let out_ty = match ir.object(ret) {
            Some(o) => o.ty.clone(),
            None => continue,
        };
        let mut full = 0usize;
        let mut slots: Vec<u32> = Vec::new();
        for &mid in ir.in_edges(ret) {
            match ir.morphism(mid).unwrap().op {
                Operation::Pair { slot, .. } => slots.push(slot),
                _ => full += 1,
            }
        }
        if full > 0 && !slots.is_empty() {
            v.push(IrViolation::BadReturn(ret));
            continue;
        }
        if !slots.is_empty() {
            match out_ty.product_arity() {
                None => v.push(IrViolation::BadReturn(ret)),
                Some(arity) => {
                    slots.sort_unstable();
                    let dup = slots.windows(2).any(|w| w[0] == w[1]);
                    // Completeness against the true `u64` arity without
                    // truncation: the deduplicated `u32` slots must be exactly
                    // `0..arity`. A > u32::MAX-arity product (not slot-
                    // addressable) is correctly flagged missing (F4/SND-3).
                    let complete = !dup
                        && (slots.len() as u64) == arity
                        && slots.iter().enumerate().all(|(k, &s)| s as u64 == k as u64);
                    if dup || !complete {
                        v.push(IrViolation::BadReturn(ret));
                    }
                }
            }
        } else if full == 0 && out_ty != Ty::Unit {
            v.push(IrViolation::BadReturn(ret));
        }
    }
}

/// I5 + loop-carried-token escape, SCC-derived per function.
fn check_loops(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    for (f, _) in ir.funcs() {
        let comps = ir.sccs(f);
        let mut scc_of: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
        for (i, comp) in comps.iter().enumerate() {
            for &o in comp {
                scc_of.insert(o, i);
            }
        }
        let nontrivial = |i: usize| -> bool {
            comps[i].len() > 1
                || comps[i]
                    .first()
                    .map(|&o| {
                        ir.out_edges(o)
                            .iter()
                            .any(|&m| ir.morphism(m).unwrap().target == o)
                    })
                    .unwrap_or(false)
        };

        for (id, obj) in ir.objects() {
            if ir.owner(id) != f || obj.kind != ObjectKind::LoopMerge {
                continue;
            }
            let merge_scc = match scc_of.get(id) {
                Some(&s) => s,
                None => {
                    v.push(IrViolation::BadLoop(id));
                    continue;
                }
            };
            if !nontrivial(merge_scc) {
                v.push(IrViolation::BadLoop(id));
                continue;
            }
            let mut backs = 0;
            let mut bad = false;
            for &mid in ir.in_edges(id) {
                let m = ir.morphism(mid).unwrap();
                match m.op {
                    Operation::LoopEnter if scc_of.get(m.source) == Some(&merge_scc) => {
                        bad = true;
                    }
                    Operation::LoopBack => {
                        backs += 1;
                        // Per-edge I5 on the carried STATE, not the route object:
                        // the route `(next_state, cond)` is always inside the SCC
                        // via its `cond` slot edge, so test the route's slot-0
                        // `Pair` source (the next_state). DESIGN §9 I5.
                        match loopback_state_source(ir, m.source) {
                            Some(s) if scc_of.get(s) == Some(&merge_scc) => {}
                            _ => bad = true,
                        }
                    }
                    _ => {}
                }
            }
            if backs == 0 {
                bad = true;
            }
            // ≥1 LoopExit belonging to this loop, target outside; token escape.
            // The exit route is a downward leaf (not in the SCC), so "belongs"
            // means its route has a Pair in-edge from an object in the SCC.
            let u_has_token = ty::ty_contains_token(&obj.ty);
            let mut exits = 0;
            for (_, m) in ir.morphisms() {
                if m.op != Operation::LoopExit {
                    continue;
                }
                if !reachable(ir, id, m.source) {
                    continue;
                }
                if scc_of.get(m.target) == Some(&merge_scc) {
                    continue;
                }
                exits += 1;
                if u_has_token {
                    let b_ty = &ir.object(m.target).unwrap().ty;
                    if !ty::ty_contains_token(b_ty) {
                        v.push(IrViolation::TokenNotEscaping(id));
                    }
                }
            }
            if exits == 0 {
                bad = true;
            }
            if bad {
                v.push(IrViolation::BadLoop(id));
            }
        }
    }
}

/// I6: function reference graph acyclic (iterative DFS; J1).
fn check_references(ir: &CategoryIr, v: &mut Vec<IrViolation>) {
    let func_ids: Vec<FuncId> = ir.funcs().map(|(id, _)| id).collect();
    let mut color: SecondaryMap<FuncId, u8> = SecondaryMap::new();
    for &f in &func_ids {
        color.insert(f, 0);
    }
    let refs_of = |f: FuncId| -> Vec<FuncId> {
        let mut out = Vec::new();
        if let Some(def) = ir.func(f) {
            for &m in &def.morphisms {
                if let Some(mm) = ir.morphism(m) {
                    match mm.op {
                        Operation::Call(g)
                        | Operation::Map { body: g }
                        | Operation::Fold { body: g } => out.push(g),
                        _ => {}
                    }
                }
            }
        }
        out
    };
    for &root in &func_ids {
        if color[root] != 0 {
            continue;
        }
        let mut path: Vec<FuncId> = Vec::new();
        struct Frame {
            refs: Vec<FuncId>,
            cursor: usize,
            node: FuncId,
        }
        let mut stack: Vec<Frame> = vec![Frame {
            refs: refs_of(root),
            cursor: 0,
            node: root,
        }];
        color.insert(root, 1);
        path.push(root);
        while let Some(frame) = stack.last_mut() {
            if frame.cursor < frame.refs.len() {
                let w = frame.refs[frame.cursor];
                frame.cursor += 1;
                match color.get(w).copied().unwrap_or(2) {
                    0 => {
                        color.insert(w, 1);
                        path.push(w);
                        stack.push(Frame {
                            refs: refs_of(w),
                            cursor: 0,
                            node: w,
                        });
                    }
                    1 => {
                        let start = path.iter().position(|&x| x == w).unwrap_or(0);
                        v.push(IrViolation::RecursiveReference(path[start..].to_vec()));
                        return;
                    }
                    _ => {}
                }
            } else {
                color.insert(frame.node, 2);
                path.pop();
                stack.pop();
            }
        }
    }
}

// --- small local helpers (no builder code shared) -------------------------

/// Per-object SCC index across all functions (each function's SCCs are disjoint
/// by ownership, so a single map is unambiguous).
fn build_scc_membership(ir: &CategoryIr) -> SecondaryMap<ObjectId, usize> {
    let mut scc_of: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
    let mut base = 0usize;
    for (f, _) in ir.funcs() {
        let comps = ir.sccs(f);
        for (i, comp) in comps.iter().enumerate() {
            for &o in comp {
                scc_of.insert(o, base + i);
            }
        }
        base += comps.len();
    }
    scc_of
}

/// Whether `to` is forward-reachable from `from` (iterative BFS; J1). Used to
/// attribute a `LoopExit`'s route to its merge.
fn reachable(ir: &CategoryIr, from: ObjectId, to: ObjectId) -> bool {
    let mut seen: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
    let mut queue: Vec<ObjectId> = vec![from];
    seen.insert(from, true);
    while let Some(o) = queue.pop() {
        if o == to {
            return true;
        }
        for &m in ir.out_edges(o) {
            let t = ir.morphism(m).unwrap().target;
            if !seen.get(t).copied().unwrap_or(false) {
                seen.insert(t, true);
                queue.push(t);
            }
        }
    }
    false
}

/// The carried-state object of a `LoopBack` route (DESIGN §7): the source of the
/// route's slot-0 `Pair` in-edge (the `next_state`; slot 1 is the `cond`).
/// `route` is the `LoopBack` morphism's source. `None` if the route is not the
/// canonical `(next_state, cond)` shape. Re-derived independently of the builder
/// (DESIGN §11).
fn loopback_state_source(ir: &CategoryIr, route: ObjectId) -> Option<ObjectId> {
    for &mid in ir.in_edges(route) {
        let m = ir.morphism(mid)?;
        if let Operation::Pair { slot: 0, .. } = m.op {
            return Some(m.source);
        }
    }
    None
}

/// The two component tys of a 2-tuple, else `None`.
fn two_tuple(ty: &Ty) -> Option<(&Ty, &Ty)> {
    match ty {
        Ty::Tuple(ts) if ts.len() == 2 => Some((&ts[0], &ts[1])),
        _ => None,
    }
}

/// Iterative Core ty check (mirrors the builder's intake, encoded once here as
/// the §5.1-adjacent data rule — no shared helper, per §11 independence).
fn ty_is_core(ty: &Ty) -> bool {
    let mut stack: Vec<&Ty> = vec![ty];
    while let Some(t) = stack.pop() {
        match t {
            Ty::Int { bits, signed } => {
                if !matches!((bits, signed), (32, true) | (64, true) | (8, false)) {
                    return false;
                }
            }
            Ty::Float { bits } => {
                if *bits != 32 && *bits != 64 {
                    return false;
                }
            }
            Ty::Bool | Ty::Unit | Ty::Str | Ty::IoToken => {}
            Ty::Tuple(ts) => {
                if ts.len() < 2 {
                    return false;
                }
                stack.extend(ts.iter());
            }
            Ty::Struct { fields, .. } => stack.extend(fields.iter().map(|(_, t)| t)),
            Ty::Array { elem, size } => {
                if *size < 1 {
                    return false;
                }
                stack.push(elem);
            }
        }
    }
    true
}
