//! `flow-lower`: parse tree → sealed IR (DESIGN `docs/components/lower/DESIGN.md`).
//!
//! [`lower`] turns a **parse-clean** [`flow_syntax::Program`] into a **sealed**
//! [`flow_ir::CategoryIr`], or a list of lower-stage diagnostics (L-codes). It
//! runs five passes (DESIGN §2):
//!
//! - **A** type table (`tys`), **B** effects & call graph (`effects`),
//!   **C** declare + **D** per-fn typing (`typing`) and emission (`emit`),
//!   **E** seal.
//!
//! Totality (DESIGN §13): a clean tree + bounded recursion + fail-fast emission
//! ⇒ `lower` never panics or hangs. Determinism is sacred (DESIGN §11): no
//! `HashMap` iteration affects emission order — `BTreeMap`/`Vec` throughout.

mod diag;
mod effects;
mod emit;
mod scope;
mod typing;
mod tys;

use std::collections::{BTreeMap, BTreeSet};

use flow_syntax::{Diagnostic, Program};

use crate::diag::{LCode, diag};
use crate::tys::name_text;

/// Lower a parse-clean program to a sealed IR.
///
/// Precondition: `program` came from a `parse()` whose diagnostics were empty
/// (syntax J3: no Error nodes, no rejected-but-kept forms). Encountering such a
/// form anyway yields L1000, never a panic. Total: never panics, never hangs
/// (recursion is bounded by the parser's depth guard P0011; DESIGN §13).
pub fn lower(source: &str, program: &Program) -> Result<flow_ir::CategoryIr, Vec<Diagnostic>> {
    let mut diags: Vec<Diagnostic> = Vec::new();

    // Defensive: any Error item is an unclean tree (L1000).
    for item in &program.items {
        if let flow_syntax::Item::Error(sp) = item {
            diags.push(diag(LCode::UncleanTree, *sp, "error item reached lowering"));
        }
    }

    // --- structural fn checks (NoMain / MainShape / DuplicateFn / Reserved /
    //     DuplicateParam), collected.
    let mut fn_names: BTreeSet<String> = BTreeSet::new();
    let mut has_main = false;
    {
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for item in &program.items {
            if let flow_syntax::Item::Fn(fd) = item {
                let name = name_text(source, fd.name).to_string();
                // Reserved name: `fn print`, or a builtin scalar type name.
                if is_reserved(&name) {
                    diags.push(diag(
                        LCode::ReservedName,
                        fd.name.span,
                        format!("`{name}` is a reserved name"),
                    ));
                }
                if seen.contains_key(&name) {
                    diags.push(diag(
                        LCode::DuplicateFn,
                        fd.name.span,
                        format!("duplicate function `{name}`"),
                    ));
                } else {
                    seen.insert(name.clone(), ());
                    fn_names.insert(name.clone());
                }
                if name == "main" {
                    has_main = true;
                    if !fd.params.is_empty() || fd.ret_ty.is_some() {
                        diags.push(diag(
                            LCode::MainShape,
                            fd.name.span,
                            "`main` must have no parameters and no declared return",
                        ));
                    }
                }
                // Duplicate parameter names (L1006).
                let mut pseen: BTreeSet<String> = BTreeSet::new();
                for p in &fd.params {
                    let pn = name_text(source, p.name).to_string();
                    if !pseen.insert(pn.clone()) {
                        diags.push(diag(
                            LCode::DuplicateParam,
                            p.name.span,
                            format!("duplicate parameter `{pn}`"),
                        ));
                    }
                }
            }
        }
    }
    if !has_main {
        diags.push(diag(LCode::NoMain, program.span, "no `fn main`"));
    }

    // --- Pass A: type table.
    let (types, ty_diags) = tys::build(source, program);
    diags.extend(ty_diags);

    // --- Pass B: effects & call graph (L1008 aborts).
    let effects = match effects::analyze(source, program, &fn_names) {
        Ok(e) => e,
        Err(mut e) => {
            diags.append(&mut e);
            return Err(diags);
        }
    };

    // Resolve surface fn signatures (param/return tys) — feeds D1 and C.
    let fn_sigs = typing::build_fn_sigs(source, &types, &effects, program, &mut diags);

    // --- Pass D1: typing walk per fn (collected).
    let mut typing_info: BTreeMap<String, typing::TypeInfo> = BTreeMap::new();
    for item in &program.items {
        if let flow_syntax::Item::Fn(fd) = item {
            let name = name_text(source, fd.name).to_string();
            let (info, d) = typing::analyze_fn(source, &types, &effects, &fn_sigs, fd);
            diags.extend(d);
            // L1306: return completeness (D1; syntactic presence is sound for
            // Core — L1405 keeps `-> ret` out of Phi arms).
            let sig = fn_sigs.get(&name);
            let has_output = sig.map(|s| s.ret.is_some()).unwrap_or(false);
            let out_arity = sig
                .and_then(|s| s.ret.as_ref())
                .and_then(|t| t.product_arity());
            typing::check_return_completeness(source, fd, has_output, out_arity, &mut diags);
            typing_info.insert(name, info);
        }
    }

    // If the typing/structural passes found errors, stop before emission
    // (emission is fail-fast and assumes a well-typed-enough tree; DESIGN §1
    // LD12 — the typing pass is the multi-error surface).
    if !diags.is_empty() {
        return Err(diags);
    }

    // --- Pass C + D2/D3 + E: declare, emit, seal (fail-fast).
    emit::lower_program(source, program, &types, &effects, &fn_sigs, &typing_info)
}

/// The builtin print family (ADR-0015): `print` (raw) and `println` (newline).
/// One predicate so the many effect / typing / emit sites that special-case the
/// builtin cannot drift — adding `println` otherwise silently breaks every missed
/// call site (the FRAMEWORK §5 "one source of truth for a shared check" rule; this
/// helper was introduced precisely because `println` regressed an un-updated site).
pub(crate) fn is_print_builtin(name: &str) -> bool {
    matches!(name, "print" | "println")
}

/// Whether a name is reserved (L1009): `print`/`println`, or a builtin scalar type name.
fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "print" | "println" | "i32" | "i64" | "u8" | "f32" | "f64" | "bool"
    )
}
