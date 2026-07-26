//! The host-driven loop quartet (DESIGN §1's `loops` op-table row; ADR-0016
//! guard-first; llvm `loops.rs` ported to C++ statements). Only the
//! canonical single-merge shape reaches here — non-canonical (multi-merge
//! SCC) loops are rejected upstream in [`crate::emit`] as `Unsupported`
//! (L3), and the attribution is [`mapal_ir::CategoryIr::loop_plan`], the one
//! source of truth (BL7) the interp driver also consumes — never re-derived.
//!
//! The C++ shape mirrors the interp/llvm driver 1:1 over the fn's hoisted
//! locals (mirroring llvm's memory slots):
//!
//! ```text
//! <merge> = <init>;          // entry: init → merge local (pointer copy for
//!                            //   a carried array handle — §2's merge IS a
//!                            //   host handle variable)
//! while (true) {
//!     <decide-cone emissions>   // guard cond + exit-route assembly, run
//!                               //   EVERY iteration incl. the exit one
//!                               //   (ADR-0016 / countdown prints 0)
//!     if (!<exit_route@1>) { break; }   // guard-first: on the exit step the
//!                                       //   advance cone — incl. its kernel
//!                                       //   launches — never executes
//!     <advance-cone emissions>  // next-state + back-route assembly
//!     <merge> = <back_route@0>; // back edge: parallel assignment to the
//! }                             //   carried state (pointer swap for arrays)
//! <exit obj> = <exit_route@0>; // exit payload, materialized exactly once
//! ```
//!
//! **The back edge needs no temporaries** (the task's parallel-copy
//! question): llvm's single `copy_component` after the advance cone
//! transfers verbatim. The route object is a *distinct* local from the
//! merge (route objects sit outside the SCC), the advance cone fully
//! assembles it before the one back-edge assignment runs, and nothing in
//! the cones writes the merge's local (its only in-edges are
//! `LoopEnter`/`LoopBack`) — so sequencing realizes the parallel semantics:
//! every read of the old state (through projections materialized in the
//! cones) precedes the single struct/scalar/pointer copy. A carried array
//! swaps only the handle value. The outgoing buffer's lifetime is the
//! last-use plan's call (suggestions.md #2, plan-last-use §3): where the
//! plan proves the state dead past the swap and the next-instance producer
//! is a registered allocation, the back edge frees it under the
//! pointer-value init guard (`FnEmit::emit_back_edge_frees`); otherwise
//! both the old and new buffers stay in the allocation registry and are
//! freed at fn exit (§2's O(k·n), recorded — the conservative default).
//!
//! Trap semantics are unchanged in loops (DESIGN §3): cone emissions are
//! the ordinary op table — host scalar guards call `mapal_trap` directly,
//! and every kernel launch in a cone is followed by
//! `trap_check_after_launch()`, so a device trap on iteration `i` fires
//! before iteration `i+1`'s host code runs (first-trap-wins in launch
//! order, the oracle's evaluation order).

use mapal_ir::ObjectId;

use crate::EmitError;
use crate::func::FnEmit;

/// Emit the guard-first host quartet for the canonical loop headed by
/// `merge` (DESIGN §1 `loops` row, left column). Called from
/// [`FnEmit::walk`] at the `LoopEnter` site; the walk's driver-ownership
/// skip guarantees the cone morphisms emit nowhere else.
pub(crate) fn emit_loop(fe: &mut FnEmit, merge: ObjectId) -> Result<(), EmitError> {
    let plan = fe
        .ir
        .loop_plan(fe.f, merge)
        .expect("canonical loop (gated by emit's L3 capability check)");

    // Entry: init → merge local. The init is loop-invariant, already
    // materialized by the walk before the LoopEnter site (ir §13).
    fe.copy_obj(plan.init, plan.merge);

    fe.line("while (true) {");
    fe.indent += 1;

    // Decide/exit cone: guard cond + exit-route payload, every iteration.
    for &mo in &plan.decide_order {
        fe.emit_morphism(mo)?;
    }
    let cond = fe.route_component(plan.exit_route, 1);
    fe.line(format!("if (!{cond}) {{ break; }}"));

    // Advance cone: the next-state, unreachable on the exit step.
    for &mo in &plan.advance_order {
        fe.emit_morphism(mo)?;
    }
    // Back-edge freeing (suggestions.md #2, plan-last-use §3): free the
    // merge's outgoing buffers the plan proves dead past the swap — the
    // producer's per-iteration buffers no longer accumulate to fn exit.
    // Emits nothing where the plan can't prove (today's O(k·n) default).
    fe.emit_back_edge_frees(&plan);
    // Back edge: the parallel assignment to the carried state (see the
    // module doc — one copy, sequenced last; a pointer swap for arrays).
    fe.copy_component(plan.back_route, 0, plan.merge);

    fe.indent -= 1;
    fe.line("}");

    // Exit: copy the exit route's payload (slot 0) to each exit object —
    // exit-only payloads materialize exactly once, here (the walk skips
    // everything driver-owned).
    for &ex in &plan.exits {
        let (route, tgt) = {
            let m = fe.ir.morphism(ex).expect("exit morphism");
            (m.source, m.target)
        };
        fe.copy_component(route, 0, tgt);
    }
    Ok(())
}
