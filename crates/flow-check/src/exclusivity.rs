//! Pass 1 — Return exclusivity (T0101; DESIGN §3).
//!
//! For each `(f, fd)` in `ir.funcs()` (insertion order) the **full-value
//! writers** of `fd.output` are its non-`Pair` in-edges (the slot form writes
//! via `Pair{slot,arity}` edges and is per-slot single-writer by validate's
//! I-RET arms; classification mirrors `validate::check_returns`). With more than
//! one full-value writer at most one can fire at runtime, breaking interp's IN3
//! assumption — so every writer beyond the first (walk order) is a T0101
//! (CK3, strict; no loop-exit exception in v1).

use flow_ir::{CategoryIr, Operation};
use flow_syntax::{Diagnostic, SourceLoc};

use crate::diag::{TCode, diag};

/// Convert an IR span to a syntax span — field-identical, distinct types (the
/// IR keeps its own `SourceLoc` to depend on nothing; ir/loc.rs D8).
fn to_syntax(loc: flow_ir::SourceLoc) -> SourceLoc {
    SourceLoc {
        start: loc.start,
        end: loc.end,
    }
}

/// Run pass 1 over `ir`, appending any T0101 diagnostics in `funcs()` order.
pub fn check(ir: &CategoryIr, out: &mut Vec<Diagnostic>) {
    for (_, fd) in ir.funcs() {
        let ret = fd.output;
        // Full-value writers = non-Pair in-edges of the Return object; the slot
        // form (Pair edges) is per-slot single-writer by validate's I-RET shape.
        let mut full_value_writers = ir.in_edges(ret).iter().filter(|&&mid| {
            ir.morphism(mid)
                .map(|m| !matches!(m.op, Operation::Pair { .. }))
                .unwrap_or(false)
        });

        // The first writer is legitimate; each one beyond it is a T0101.
        let _ = full_value_writers.next();
        for &mid in full_value_writers {
            let loc = ir.morphism(mid).expect("ret in-edge exists").loc;
            out.push(diag(
                TCode::MultipleReturnWriters,
                to_syntax(loc),
                "multiple full-value writers to this function's return; at most \
                 one may fire per run (exactly one return is permitted)",
            ));
        }
    }
}
