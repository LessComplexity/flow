//! The guard-first loop driver (interp DESIGN §4; ADR-0016).
//!
//! A loop is one single-merge SCC. On each iteration the driver:
//!   1. writes `env[merge] := state`, resets the staging buffers of every
//!      product object assembled in the body,
//!   2. evaluates the **decide/exit cone** (the shared guard `cond` and the
//!      `LoopExit` payload, incl. exit-feeding effects like countdown's
//!      `println`),
//!   3. reads the guard; if it selects exit, copies the exit payload to the exit
//!      objects and stops **without** evaluating the continue-branch,
//!   4. otherwise evaluates the **advance set** (the next-state feeding
//!      `LoopBack`, where speculative traps like fir's `Index` live) and
//!      iterates.
//!
//! This guard-first split (ADR-0016) is why fir never indexes `coeffs[4]` on the
//! exit step `k = 4`, and why countdown still prints `0` on its `n = 0` exit
//! step.

use slotmap::SecondaryMap;

use flow_ir::{CategoryIr, FuncId, MorphismId, ObjectId, Operation, Value};

use crate::eval::{EvalCtx, eval_morphism};
use crate::value::{Abort, RValue};

/// Per-merge, structurally-derived loop layout (interp DESIGN §4).
struct LoopPlan {
    /// The `LoopMerge` object.
    merge: ObjectId,
    /// `source(LoopEnter → merge)` — the init value object.
    init: ObjectId,
    /// `source(LoopBack → merge)` — the `(next_state, cond)` route object.
    back_route: ObjectId,
    /// The attributed `LoopExit` morphisms (M1: exactly one).
    exits: Vec<MorphismId>,
    /// `exit_route` = source of the unique attributed `LoopExit`.
    exit_route: ObjectId,
    /// Decide phase: body morphisms whose target feeds the exit route.
    decide_order: Vec<MorphismId>,
    /// Advance phase: the rest of the body (next-state computation).
    advance_order: Vec<MorphismId>,
    /// Product objects assembled by `Pair` edges across both phases (reset/iter).
    product_targets: Vec<ObjectId>,
}

/// Run the loop whose merge is `merge` (interp DESIGN §4). Writes the loop's
/// exit object(s) into `env` and returns. Diverges (E1) under budget exhaustion.
pub(crate) fn run_loop(ctx: &mut EvalCtx, merge: ObjectId, budget: &mut u64) -> Result<(), Abort> {
    let plan = derive_plan(ctx.ir, ctx.f, merge);

    // state := env[init] (computed outside the SCC, already in env).
    let mut state = ctx
        .env
        .get(plan.init)
        .cloned()
        .unwrap_or_else(|| unreachable!("loop init value computed before driver"));

    loop {
        // env[merge] := state.
        ctx.env.insert(plan.merge, state.clone());

        // Reset the staging buffers of every product object in the body so
        // finalization re-triggers cleanly this iteration.
        for &p in &plan.product_targets {
            ctx.staging.remove(p);
        }

        // Decide/exit cone: build cond + the exit route (incl. exit-feeding
        // effects). Decrements budget per morphism.
        for &mo in &plan.decide_order {
            eval_morphism(ctx, mo, budget)?;
        }

        // cond := env[exit_route]@1 (the shared guard bool; D7).
        let cond = read_slot_bool(ctx, plan.exit_route, 1);

        if !cond {
            // EXIT — do NOT evaluate the continue-branch. Copy each exit
            // payload (route slot 0) to the exit object.
            for &ex in &plan.exits {
                let route = ctx.morph(ex).source;
                let target = ctx.morph(ex).target;
                let payload = slot_value(ctx, route, 0);
                ctx.env.insert(target, payload);
            }
            return Ok(());
        }

        // CONTINUE — build the next-state (the inr(U) arm).
        for &mo in &plan.advance_order {
            eval_morphism(ctx, mo, budget)?;
        }

        // state := env[back_route]@0 (the (next_state, cond) route's slot 0).
        state = slot_value(ctx, plan.back_route, 0);
    }
}

/// Read slot `k` of the aggregate `env[o]` as a value, cloned.
fn slot_value(ctx: &EvalCtx, o: ObjectId, k: u32) -> RValue {
    let v = ctx
        .env
        .get(o)
        .unwrap_or_else(|| unreachable!("route object built before read"));
    match v {
        RValue::Tuple(es) => es[k as usize].clone(),
        RValue::Struct { fields, .. } => fields[k as usize].1.clone(),
        RValue::Array(es) => es[k as usize].clone(),
        _ => unreachable!("route object is not a product"),
    }
}

/// Read slot `k` of `env[o]` as a bool.
fn read_slot_bool(ctx: &EvalCtx, o: ObjectId, k: u32) -> bool {
    match slot_value(ctx, o, k) {
        RValue::Scalar(Value::Bool(b)) => b,
        _ => unreachable!("guard slot is not a bool"),
    }
}

/// Derive the per-merge loop layout once (interp DESIGN §4). Asserts the M1
/// canonical shape (single merge, one LoopBack, one attributed LoopExit); any
/// other shape is out of M1 (lower OQ7 never generates one) and is an
/// unreachable-class interpreter condition for the supported pipeline.
fn derive_plan(ir: &CategoryIr, f: FuncId, merge: ObjectId) -> LoopPlan {
    // M1 single-merge assertion: this merge's SCC has exactly one merge.
    let scc = ir
        .loop_structure(f)
        .into_iter()
        .find(|s| s.merges.contains(&merge))
        .unwrap_or_else(|| unreachable!("merge is in some SCC"));
    assert!(
        scc.merges.len() == 1,
        "out-of-M1 loop: multi-merge SCC is unsupported"
    );

    // THIS merge's SCC only — never the per-function union. With two sequential
    // loops in one fn, the union attributed the second loop's exit (and body
    // morphisms) to the first merge's plan: the exit count tripped the M1 assert
    // on a legal Core program, and body_order would have owned foreign morphisms
    // (S12 fix; pinned by interp tests/loop_invariants.rs::two_sequential_loops
    // and lower-reachable per lower/DESIGN §8.5).
    let in_scc: SecondaryMap<ObjectId, ()> = {
        let mut m = SecondaryMap::new();
        for &o in &scc.objects {
            m.insert(o, ());
        }
        m
    };
    debug_assert!(
        in_scc.contains_key(merge),
        "the merge belongs to its own SCC"
    );

    // init / back_route from the merge's in-edges.
    let mut init = None;
    let mut back_routes: Vec<ObjectId> = Vec::new();
    for &m in ir.in_edges(merge) {
        let morph = ir.morphism(m).expect("sealed");
        match morph.op {
            Operation::LoopEnter => {
                assert!(init.is_none(), "out-of-M1 loop: multiple LoopEnter edges");
                init = Some(morph.source);
            }
            Operation::LoopBack => back_routes.push(morph.source),
            _ => {}
        }
    }
    let init = init.unwrap_or_else(|| unreachable!("merge has a LoopEnter"));
    assert!(
        back_routes.len() == 1,
        "out-of-M1 loop: expected exactly one LoopBack into the merge"
    );
    let back_route = back_routes[0];

    // exits: LoopExit morphisms whose route object is fed (slot-Pair source) by
    // an in-SCC value — attribute by route-feeder membership (interp DESIGN §4).
    let mut exits: Vec<MorphismId> = Vec::new();
    for (mid, morph) in ir.morphisms() {
        if ir.try_owner(morph.source) != Some(f) {
            continue;
        }
        if morph.op != Operation::LoopExit {
            continue;
        }
        let route = morph.source;
        let fed_by_scc = ir.in_edges(route).iter().any(|&pm| {
            let pmo = ir.morphism(pm).expect("sealed");
            matches!(pmo.op, Operation::Pair { .. }) && in_scc.contains_key(pmo.source)
        });
        if fed_by_scc {
            exits.push(mid);
        }
    }
    assert!(
        exits.len() == 1,
        "out-of-M1 loop: expected exactly one attributed LoopExit"
    );
    let exit_route = ir.morphism(exits[0]).expect("sealed").source;

    // body_order: topo_order filtered to morphisms incident to SCC(merge)
    // OR feeding a route object, excluding loop ops (interp DESIGN §4).
    //
    // The route objects (`back_route`, `exit_route`) sit *outside* the SCC, but
    // every `Pair` edge that assembles them must be re-fired each iteration so
    // the routes are repopulated before they are read — including loop-invariant
    // feeds whose source is a constant outside the SCC (e.g. a constant-`true`
    // guard, interp DESIGN §4 / §11.3). Membership-by-SCC alone would drop such
    // an edge (both endpoints outside the SCC), leaving the route unfinalized.
    let body_order: Vec<MorphismId> = ir
        .topo_order(f)
        .into_iter()
        .filter(|&m| {
            let morph = ir.morphism(m).expect("sealed");
            if matches!(
                morph.op,
                Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit
            ) {
                return false;
            }
            in_scc.contains_key(morph.source)
                || in_scc.contains_key(morph.target)
                || morph.target == back_route
                || morph.target == exit_route
        })
        .collect();

    // The decide/advance split (ADR-0016): D = objects backward-reachable within
    // body_order's edges from exit_route. Iterate to a fixpoint over the
    // (small) body.
    let mut in_d: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    in_d.insert(exit_route, ());
    loop {
        let mut changed = false;
        for &m in &body_order {
            let morph = ir.morphism(m).expect("sealed");
            if in_d.contains_key(morph.target) && !in_d.contains_key(morph.source) {
                in_d.insert(morph.source, ());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut decide_order = Vec::new();
    let mut advance_order = Vec::new();
    for &m in &body_order {
        let morph = ir.morphism(m).expect("sealed");
        if in_d.contains_key(morph.target) {
            decide_order.push(m);
        } else {
            advance_order.push(m);
        }
    }

    // Product targets assembled by Pair edges across the whole body (reset/iter).
    let mut product_targets: Vec<ObjectId> = Vec::new();
    for &m in &body_order {
        let morph = ir.morphism(m).expect("sealed");
        if matches!(morph.op, Operation::Pair { .. }) && !product_targets.contains(&morph.target) {
            product_targets.push(morph.target);
        }
    }

    LoopPlan {
        merge,
        init,
        back_route,
        exits,
        exit_route,
        decide_order,
        advance_order,
        product_targets,
    }
}
