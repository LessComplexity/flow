//! `flow-check`: the completion of the pipeline's partial functor
//! (DESIGN `docs/components/check/DESIGN.md`; ADR-0003). Consumes the sealed,
//! `validate()`-clean [`flow_ir::CategoryIr`] **plus** the
//! [`flow_syntax::Program`] tree and emits structured diagnostics only,
//! discharging exactly the obligations interp assumes (interp §9, IN3):
//!
//! - **Pass 1 — Return exclusivity** (T0101, `exclusivity`): at most one
//!   full-value writer per Return (§3), so interp's "takes the writer that
//!   fires" is vacuous-safe.
//! - **Pass 2 — E2 effect legality** (T0201, `effects`): no effectful morphism
//!   inside a parallel-fanout branch (§4), the node-kind-keyed walk (`Fanout`
//!   opens the illegal context; `SeqBlock` recurses sticky — ADR-0019).
//!
//! Determinism is sacred (C-check-5): no `HashMap` anywhere; exclusivity
//! findings in `funcs()` insertion order, then effect findings in tree walk
//! order. Two runs on equal inputs yield an identical `Vec<Diagnostic>`.

mod diag;
mod effects;
mod exclusivity;

// `diag` (TCode + the diag constructor) stays crate-private, mirroring
// flow-lower (only `pub fn lower` is exported; LCode/diag are internal). The
// passes reach it via `crate::diag::...`; nothing external consumes it.

use flow_ir::{CategoryIr, validate};
use flow_syntax::{Diagnostic, Program};

/// Check a sealed, `validate()`-clean program (DESIGN §1).
///
/// Empty vec = accept. Non-empty = reject; diagnostics are
/// [`flow_syntax::Diagnostic`] with `severity: Error`, `fix: None`.
///
/// `source`, `program`, and `ir` must be the same source's `parse`/`lower`
/// (the standard `parse → lower → check` pipeline). Mismatched inputs are a
/// caller bug (debug-territory).
///
/// Boundary (CK2): `debug_assert!(validate(ir).is_empty())` — a dirty sealed
/// graph is unconstructible through the public builder, so there is no
/// release-mode re-validation and no T-code for dirty input.
pub fn check(source: &str, program: &Program, ir: &CategoryIr) -> Vec<Diagnostic> {
    debug_assert!(
        validate(ir).is_empty(),
        "check's precondition is a validate-clean graph (CK2): {:?}",
        validate(ir)
    );

    let mut out = Vec::new();
    // Exclusivity findings first (funcs() insertion order), then effects
    // findings (tree walk order) — the C-check-5 determinism order.
    exclusivity::check(ir, &mut out);
    effects::check(source, program, ir, &mut out);
    out
}
