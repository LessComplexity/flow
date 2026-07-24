//! Region-emission plan Move 1 — strip `Call` morphisms by graph substitution.
//! Pure analysis: `&CategoryIr → RewritePlan`, filling only the `inline`
//! channel; the substitution itself lives in [`replay`](crate::replay) (the
//! callee's body is rebuilt into the caller with `input ↦ call source`,
//! `output ↦ call target`, fresh ids in builder emission order — L2).
//!
//! Functions are a human modularity construct; the optimizer's unit is the
//! flattened primitive dataflow graph. Semantics-preserving by construction
//! (substitution of equals); the R1 harness makes that a theorem under test.
//!
//! **Policy (plan §Move 1):** inline a call site iff
//!   - the callee's morphism count ≤ [`INLINE_MAX_BODY`] (strip is not free —
//!     a callee called at K sites is copied K times; the cap is a recorded
//!     constant, tuned by the perf harness, never semantics-bearing), ∧,
//!   - the callee is not the entry (the entry fn is never inlined), ∧,
//!   - the callee has no loop (inlining it into a caller loop can create nested
//!     SCCs, which lower never produces and the LLVM backend rejects; the
//!     future loop-to-fold lift makes these callees eligible), ∧,
//!   - no `Call` cycle (the builder rejects recursive Calls at seal —
//!     `IrError::RecursiveCall` — so the graph is a DAG by construction; if
//!     recursion ever arrives, cycle members stay as calls, plan §7).
//!
//! Calls inside `Map`/`Fold` bodies are stripped: those bodies are separate
//! functions, and the graph-wide morphism walk plans their call sites too.
//! Sites kept as calls remain region boundaries (the callee is its own region
//! graph).

use slotmap::SecondaryMap;

use flow_ir::{CategoryIr, FuncId, Operation};

use crate::plan::RewritePlan;

/// Policy cap (plan §Move 1, v2.0): a callee's body is inlined only when its
/// morphism count is ≤ this recorded, perf-harness-tuned 256-morphism limit;
/// the cap is never semantics-bearing.
pub const INLINE_MAX_BODY: u32 = 256;

/// Analyze `ir` for strippable `Call` edges (region-emission plan Move 1).
/// Every `Call(g)` morphism whose callee passes the module-header policy gets
/// an `inline[m] = ()` entry. Deterministic (insertion-order walk) and
/// idempotent: stripped calls are gone from the output, and kept calls
/// (oversized / entry / loop-bearing / cyclic) stay un-inlinable, so a second
/// pass plans nothing.
pub fn analyze_inline(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    let cyclic = call_cyclic(ir);
    for (m, morph) in ir.morphisms() {
        let Operation::Call(g) = morph.op else {
            continue;
        };
        if g == ir.entry() || !ir.loop_structure(g).is_empty() || cyclic.contains_key(g) {
            continue;
        }
        let def = ir.func(g).expect("call target");
        if def.morphisms.len() as u32 > INLINE_MAX_BODY {
            continue;
        }
        plan.inline.insert(m, ());
    }
    plan
}

/// Functions that can reach themselves over `Call` edges alone (plan §Move 1
/// cycle guard). Map/Fold bodies can't be `Call` targets, so they never close
/// a `Call` cycle — and the builder rejects recursive Calls at seal
/// (`IrError::RecursiveCall`), so this is empty for every graph the public
/// API can produce. It is the recorded policy for the recursion future: a
/// callee on a `Call` cycle stays a call, which also bounds replay's
/// inlining recursion.
fn call_cyclic(ir: &CategoryIr) -> SecondaryMap<FuncId, ()> {
    let mut cyclic: SecondaryMap<FuncId, ()> = SecondaryMap::new();
    for (f, _) in ir.funcs() {
        let mut seen: SecondaryMap<FuncId, ()> = SecondaryMap::new();
        let mut stack = call_targets(ir, f);
        while let Some(h) = stack.pop() {
            if h == f {
                cyclic.insert(f, ());
                break;
            }
            if seen.contains_key(h) {
                continue;
            }
            seen.insert(h, ());
            stack.extend(call_targets(ir, h));
        }
    }
    cyclic
}

/// The direct `Call` targets of function `f`.
fn call_targets(ir: &CategoryIr, f: FuncId) -> Vec<FuncId> {
    ir.func(f)
        .expect("fn")
        .morphisms
        .iter()
        .filter_map(|&m| match ir.morphism(m).expect("morph").op {
            Operation::Call(g) => Some(g),
            _ => None,
        })
        .collect()
}
