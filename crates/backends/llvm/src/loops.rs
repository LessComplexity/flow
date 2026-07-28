//! Loop CFG — ADR-0016 guard-first as control flow (DESIGN §3). The canonical
//! quartet only; non-canonical (multi-merge SCC) shapes are rejected upstream in
//! [`crate::emit`] as `Unsupported` (L3).
//!
//! The per-merge attribution is [`mapal_ir::CategoryIr::loop_plan`] — the one
//! source of truth (BL7) the interp driver also consumes. The CFG mirrors the
//! interp loop driver (interp `loops.rs`) 1:1 over memory slots:
//!
//! ```text
//! entry:   store init → merge.slot; br header
//! header:  <decide cone>; %c = <exit_route[1]>; br %c, advance, exit
//! advance: <advance cone>; store back_route[0] → merge.slot; br header
//! exit:    store exit_route[0] → exit obj slot; br after
//! after:   … straight-line continues …
//! ```

use mapal_ir::ObjectId;

use crate::func::FnEmit;

/// Emit the guard-first CFG for the loop headed by `merge` (DESIGN §3).
pub(crate) fn emit_loop(fe: &mut FnEmit, merge: ObjectId) {
    let plan = fe
        .ir
        .loop_plan(fe.f, merge)
        .expect("canonical loop (gated by emit's capability check)");

    // entry: init → merge.slot (init is loop-invariant, already computed by the
    // topo LoopEnter deferral — ir §13).
    fe.copy_obj(plan.init, plan.merge);

    let header = fe.label();
    let advance = fe.label();
    let exitb = fe.label();
    let after = fe.label();

    fe.line(format!("br label %{header}"));
    fe.label_line(&header);

    // Decide/exit cone: the guard cond + exit-route payload, run every iteration
    // (incl. the exit one — ADR-0016 / countdown prints 0). Guard-arm-owned
    // morphisms are skipped exactly as in the flat walk (plan-s40, interp
    // parity) — their in-cone Phi fires the chosen arm.
    for &mo in &plan.decide_order {
        if fe.gated.contains_key(mo) {
            continue;
        }
        fe.emit_morphism(mo);
    }
    let cond = fe.load_route_component(plan.exit_route, 1);
    fe.line(format!("br i1 {cond}, label %{advance}, label %{exitb}"));

    // Advance cone: the next-state (unreachable on the exit step — guard-first).
    fe.label_line(&advance);
    for &mo in &plan.advance_order {
        if fe.gated.contains_key(mo) {
            continue;
        }
        fe.emit_morphism(mo);
    }
    fe.copy_component(plan.back_route, 0, plan.merge);
    fe.line(format!("br label %{header}"));

    // Exit: copy each exit route's payload (slot 0) to its exit object.
    fe.label_line(&exitb);
    for &ex in &plan.exits {
        let (route, tgt) = {
            let m = fe.ir.morphism(ex).expect("exit morphism");
            (m.source, m.target)
        };
        fe.copy_component(route, 0, tgt);
    }
    fe.line(format!("br label %{after}"));
    fe.label_line(&after);
}
