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

use mapal_ir::{ObjectId, Value};

use crate::eval::{EvalCtx, eval_morphism};
use crate::value::{Abort, RValue};

/// Run the loop whose merge is `merge` (interp DESIGN §4). Writes the loop's
/// exit object(s) into `env` and returns. Diverges (E1) under budget exhaustion.
///
/// The per-merge layout is [`mapal_ir::CategoryIr::loop_plan`] (DESIGN §3, BL7 —
/// the one source of truth). `.expect()` is sound: the supported pipeline only
/// runs the driver on canonical loops (lower OQ7 never generates another shape),
/// so a `None` here is an unreachable-class interpreter condition.
pub(crate) fn run_loop(ctx: &mut EvalCtx, merge: ObjectId, budget: &mut u64) -> Result<(), Abort> {
    let plan = ctx.ir.loop_plan(ctx.f, merge).unwrap_or_else(|| {
        unreachable!("out-of-M1 loop: non-canonical merge in supported pipeline")
    });

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
