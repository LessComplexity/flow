//! R-LF / R-LM — lift canonical counted loops to captured `Fold` / `Map`.
//!
//! Analysis consumes [`flow_ir::CategoryIr::loop_plan`] as the sole loop-shape
//! oracle. v1 recognizes exactly the ratified forms:
//! - `(counter, acc)`, `counter = 0; counter < K; counter += 1`, `K >= 1`,
//!   pure accumulator cone, one exit carrying `acc`;
//! - `(counter, out)` in either product slot, the same counter, one identity-
//!   indexed `Update(out, counter, value)`, `len(out) == K`, one exit carrying
//!   `out`.
//!
//! Everything else is an empty plan entry: zero-trip loops stay loops because
//! Core has no empty arrays.

use slotmap::SecondaryMap;

use flow_ir::{
    CategoryIr, LoopPlan, MorphismId, ObjectId, ObjectKind, Operation, Ty, Value, ty_contains_token,
};

use crate::plan::{LiftKind, LiftSpec, RewritePlan};

/// Analyze all canonical loops for R-LF / R-LM.
pub fn analyze_lift(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    for (f, _) in ir.funcs() {
        for scc in ir.loop_structure(f) {
            if scc.merges.len() != 1 {
                continue;
            }
            let merge = scc.merges[0];
            let Some(lp) = ir.loop_plan(f, merge) else {
                continue;
            };
            if let Some(spec) = analyze_loop(ir, &lp) {
                plan.lift.insert(merge, spec);
            }
        }
    }
    plan
}

fn analyze_loop(ir: &CategoryIr, lp: &LoopPlan) -> Option<LiftSpec> {
    for &o in &lp.scc_objects {
        if ty_contains_token(&ir.object(o)?.ty) {
            return None;
        }
    }
    let Ty::Tuple(state_tys) = &ir.object(lp.merge)?.ty else {
        return None;
    };
    if state_tys.len() != 2 {
        return None;
    }

    let state = state_components(ir, lp.merge)?;
    let init = exact_feeders(ir, lp.init, 2)?;
    let back = exact_feeders(ir, lp.back_route, 2)?;
    let next_state = back[0];
    if !lp.product_targets.contains(&next_state) {
        return None;
    }
    let next = exact_feeders(ir, next_state, 2)?;
    let exit = exact_feeders(ir, lp.exit_route, 2)?;
    if back[1] != exit[1] {
        return None;
    }

    let guard = single_def(ir, exit[1])?;
    if guard.op != Operation::Lt || !lp.decide_order.contains(&guard.id) {
        return None;
    }
    let guard_args = exact_feeders(ir, guard.source, 2)?;
    let count = const_i32(ir, guard_args[1])?;
    if count < 1 {
        return None;
    }

    let counter_slot = (0..2).find(|&slot| {
        state[slot] == guard_args[0]
            && const_i32(ir, init[slot]) == Some(0)
            && is_plus_one(ir, lp, state[slot], next[slot])
    })?;
    let value_slot = 1 - counter_slot;
    let counter = state[counter_slot];
    let carried = state[value_slot];
    if exit[0] != carried {
        return None;
    }

    let spec = if let Ty::Array { size, .. } = &state_tys[value_slot] {
        let count_u64 = u64::try_from(count).ok()?;
        if *size != count_u64 {
            return None;
        }
        analyze_map(ir, lp, counter, carried, next[value_slot], count)
    } else {
        analyze_fold(
            ir,
            lp,
            counter,
            carried,
            init[value_slot],
            next[value_slot],
            count,
        )
    }?;
    covers_loop_body(
        ir,
        lp,
        &spec,
        state,
        guard.source,
        guard.target,
        next_state,
        next[counter_slot],
        next[value_slot],
    )
    .then_some(spec)
}

fn analyze_fold(
    ir: &CategoryIr,
    lp: &LoopPlan,
    counter: ObjectId,
    accumulator: ObjectId,
    seed: ObjectId,
    result: ObjectId,
    count: i32,
) -> Option<LiftSpec> {
    let (captures, body_objects) = body_cone(ir, lp, result, &[counter, accumulator], &[])?;
    Some(LiftSpec {
        kind: LiftKind::Fold { accumulator, seed },
        counter,
        count,
        captures,
        body_objects,
        body_result: result,
    })
}

fn analyze_map(
    ir: &CategoryIr,
    lp: &LoopPlan,
    counter: ObjectId,
    collection: ObjectId,
    updated: ObjectId,
    count: i32,
) -> Option<LiftSpec> {
    let updates: Vec<_> = lp
        .advance_order
        .iter()
        .filter(|&&m| ir.morphism(m).is_some_and(|x| x.op == Operation::Update))
        .collect();
    if updates.len() != 1 {
        return None;
    }
    let update = single_def(ir, updated)?;
    if update.op != Operation::Update || update.id != *updates[0] {
        return None;
    }
    let args = exact_feeders(ir, update.source, 3)?;
    if args[0] != collection || args[1] != counter {
        return None;
    }
    let result = args[2];
    let (captures, body_objects) = body_cone(ir, lp, result, &[counter], &[collection])?;
    Some(LiftSpec {
        kind: LiftKind::Map,
        counter,
        count,
        captures,
        body_objects,
        body_result: result,
    })
}

/// Backward slice of the synthesized body. In-SCC definitions must belong to
/// the advance phase and be token-free/pure; external non-constants become
/// captures. `special` objects become body parameters, while any `forbidden`
/// dependency rejects the rule (R-LM's c-free value cone).
fn body_cone(
    ir: &CategoryIr,
    lp: &LoopPlan,
    root: ObjectId,
    special: &[ObjectId],
    forbidden: &[ObjectId],
) -> Option<(Vec<ObjectId>, Vec<ObjectId>)> {
    let mut in_scc: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    for &o in &lp.scc_objects {
        in_scc.insert(o, ());
    }
    let mut advance: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    for &m in &lp.advance_order {
        advance.insert(m, ());
    }

    let mut seen: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    let mut captured: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if seen.contains_key(o) {
            continue;
        }
        if forbidden.contains(&o) {
            return None;
        }
        let obj = ir.object(o)?;
        if ty_contains_token(&obj.ty) {
            return None;
        }
        seen.insert(o, ());
        if special.contains(&o) || obj.kind == ObjectKind::Constant {
            continue;
        }
        if !in_scc.contains_key(o) {
            let ins = ir.in_edges(o);
            if obj.kind == ObjectKind::Parameter
                || ins.is_empty()
                || !ins.iter().all(|&m| {
                    ir.morphism(m)
                        .is_some_and(|morph| cloneable_invariant(ir, morph))
                })
            {
                captured.insert(o, ());
                continue;
            }
            for &m in ins {
                stack.push(ir.morphism(m)?.source);
            }
            continue;
        }
        if obj.kind == ObjectKind::LoopMerge {
            return None;
        }
        let ins = ir.in_edges(o);
        if ins.is_empty() {
            return None;
        }
        for &m in ins {
            if !advance.contains_key(m) {
                return None;
            }
            let morph = ir.morphism(m)?;
            if matches!(
                morph.op,
                Operation::Print { .. }
                    | Operation::LoopEnter
                    | Operation::LoopBack
                    | Operation::LoopExit
            ) {
                return None;
            }
            stack.push(morph.source);
        }
    }

    let body_objects: Vec<_> = ir
        .objects()
        .filter_map(|(o, _)| seen.contains_key(o).then_some(o))
        .collect();
    let captures = body_objects
        .iter()
        .copied()
        .filter(|&o| captured.contains_key(o))
        .collect();
    Some((captures, body_objects))
}

/// Loop-invariant scalar/structural derivations that are safe to copy into the
/// synthesized body. Parameter projections are the capture boundary: copying
/// through them would capture the whole caller tuple and hide affine fields
/// from downstream recognizers.
fn cloneable_invariant(ir: &CategoryIr, morph: &flow_ir::Morphism) -> bool {
    if matches!(morph.op, Operation::Proj { .. })
        && ir.object(morph.source).map(|o| o.kind) == Some(ObjectKind::Parameter)
    {
        return false;
    }
    matches!(
        morph.op,
        Operation::Pair { .. }
            | Operation::Proj { .. }
            | Operation::Add
            | Operation::Sub
            | Operation::Mul
            | Operation::Neg
            | Operation::Eq
            | Operation::Neq
            | Operation::Lt
            | Operation::Gt
            | Operation::Le
            | Operation::Ge
            | Operation::And
            | Operation::Or
            | Operation::Not
            | Operation::Phi
            | Operation::Widen
    )
}

/// Every per-iteration morphism must be either selected into the body cone or
/// one of the exact counter/guard/state-route scaffolding edges. Otherwise the
/// lift would strand a counter-dependent object (or silently drop observable
/// trapping work) when it retires the SCC.
#[allow(clippy::too_many_arguments)]
fn covers_loop_body(
    ir: &CategoryIr,
    lp: &LoopPlan,
    spec: &LiftSpec,
    state: [ObjectId; 2],
    guard_source: ObjectId,
    guard: ObjectId,
    next_state: ObjectId,
    next_counter: ObjectId,
    next_value: ObjectId,
) -> bool {
    let mut allowed: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    let mut allow_object = |o| {
        for &m in ir.in_edges(o) {
            allowed.insert(m, ());
        }
    };

    for &o in &spec.body_objects {
        allow_object(o);
    }
    for o in [
        state[0],
        state[1],
        guard_source,
        guard,
        lp.exit_route,
        next_state,
        next_counter,
        lp.back_route,
    ] {
        allow_object(o);
    }
    if matches!(spec.kind, LiftKind::Map) {
        allow_object(next_value);
        let Some(update) = single_def(ir, next_value) else {
            return false;
        };
        allow_object(update.source);
    }
    let Some(counter_add) = single_def(ir, next_counter) else {
        return false;
    };
    allow_object(counter_add.source);

    lp.decide_order
        .iter()
        .chain(&lp.advance_order)
        .all(|m| allowed.contains_key(*m))
}

fn state_components(ir: &CategoryIr, merge: ObjectId) -> Option<[ObjectId; 2]> {
    let mut slots = [None, None];
    for &m in ir.out_edges(merge) {
        let morph = ir.morphism(m)?;
        let Operation::Proj { index } = morph.op else {
            continue;
        };
        let slot = usize::try_from(index).ok()?;
        if slot >= 2 || slots[slot].replace(morph.target).is_some() {
            return None;
        }
    }
    Some([slots[0]?, slots[1]?])
}

fn is_plus_one(ir: &CategoryIr, lp: &LoopPlan, counter: ObjectId, next: ObjectId) -> bool {
    let Some(add) = single_def(ir, next) else {
        return false;
    };
    add.op == Operation::Add
        && lp.advance_order.contains(&add.id)
        && exact_feeders(ir, add.source, 2)
            .is_some_and(|f| f[0] == counter && const_i32(ir, f[1]) == Some(1))
}

fn single_def(ir: &CategoryIr, o: ObjectId) -> Option<&flow_ir::Morphism> {
    let [m] = ir.in_edges(o) else {
        return None;
    };
    ir.morphism(*m)
}

fn exact_feeders(ir: &CategoryIr, product: ObjectId, arity: usize) -> Option<Vec<ObjectId>> {
    let mut slots = vec![None; arity];
    for &m in ir.in_edges(product) {
        let morph = ir.morphism(m)?;
        let Operation::Pair {
            slot,
            arity: edge_arity,
        } = morph.op
        else {
            return None;
        };
        if edge_arity as usize != arity {
            return None;
        }
        let slot = slot as usize;
        if slot >= arity || slots[slot].replace(morph.source).is_some() {
            return None;
        }
    }
    slots.into_iter().collect()
}

fn const_i32(ir: &CategoryIr, o: ObjectId) -> Option<i32> {
    match ir.object(o)?.value.as_ref()? {
        Value::I32(v) => Some(*v),
        _ => None,
    }
}
