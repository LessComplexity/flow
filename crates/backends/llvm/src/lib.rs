//! LLVM backend: emits textual LLVM IR (`.ll`) piped to `clang` (ADR-0020 §1;
//! backend-llvm DESIGN). The entry point [`emit`] realizes the functor
//! `F_LLVM : Flow-Cat → LLVM-Cat` as a String — the translation unit **is** the
//! artifact (ADR-0020). Semantics are the interpreter oracle's by construction:
//! every `Print`/trap routes through the shared `flow-rt` runtime (ADR-0020 §2),
//! integer arithmetic wraps, `Div`/`Mod`/`Index`/`Update` are guarded (an S20
//! `bounds_proof`-proven `Index` elides its statically-dead guard), and floats
//! are IEEE at width.
//!
//! Emission is deterministic (L2): all names come from per-function ordinals and
//! a rising counter, never slotmap bits. The one capability gap (L3) is the
//! non-canonical loop shape (multi-merge SCC), rejected as
//! [`EmitError::Unsupported`] — the same scope boundary as interp M1 / rewrite.

mod func;
mod loops;
mod module;
mod ty;

use flow_ir::{CategoryIr, FuncId, MorphismId, Operation, PathPlan, SourceLoc, TaskKind};
use slotmap::SecondaryMap;

use crate::func::{FnAttrs, FnEmit};
use crate::module::{
    PAR_DECLS, RT_DECLS, collect_str_globals, emit_main_wrapper, emit_str_globals,
};

/// A structured, renderer-free emission error (ADR-0020 §1; C3).
#[derive(Clone, Debug, PartialEq)]
pub enum EmitError {
    /// A capability-matrix rejection (the ✋ cell): a feature outside the
    /// realized set — here, the non-canonical (multi-merge) loop shape (L3).
    Unsupported { feature: String, loc: SourceLoc },
    /// An internal invariant violation (should not occur for sealed IR).
    Internal(String),
}

/// Emit one LLVM translation unit for `ir` (ADR-0020 §1). `Ok` is the `.ll`
/// text; `Err(Unsupported)` for a non-canonical loop (L3).
pub fn emit(ir: &CategoryIr) -> Result<String, EmitError> {
    // Capability gate (L3): canonical loops only — every SCC has exactly one
    // merge and a well-formed `loop_plan`. Same predicate as rewrite's
    // `is_canonical` / interp M1 (BL6/BL7).
    for (f, _) in ir.funcs() {
        for lscc in ir.loop_structure(f) {
            let canonical = lscc.merges.len() == 1 && ir.loop_plan(f, lscc.merges[0]).is_some();
            if !canonical {
                let loc = lscc
                    .merges
                    .first()
                    .and_then(|&m| ir.object(m))
                    .map(|o| o.loc)
                    .unwrap_or(SourceLoc { start: 0, end: 0 });
                return Err(EmitError::Unsupported {
                    feature: "nested loops".into(),
                    loc,
                });
            }
        }
    }

    let strings = collect_str_globals(ir);

    // Suggestions #7's attribute-capability pre-pass (func.rs `FnAttrs`):
    // clean fns get `readonly nounwind` (+ `willreturn`), unclean fns nothing.
    let attrs = FnAttrs::analyze(ir);

    // Deterministic function names: entry → @flow_main, others → @fn{ordinal}.
    let mut fnames: SecondaryMap<flow_ir::FuncId, String> = SecondaryMap::new();
    let entry = ir.entry();
    for (ord, (id, _)) in ir.funcs().enumerate() {
        let name = if id == entry {
            "flow_main".to_string()
        } else {
            format!("fn{ord}")
        };
        fnames.insert(id, name);
    }

    // ponytail: malformed/cyclic plans stay sequential; delete this fallback
    // when path_plan's total-DAG contract is enforced before backend entry.
    let path_plan = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ir.path_plan(entry)))
        .ok()
        .filter(path_plan_is_acyclic);
    let parallel = path_plan
        .as_ref()
        .is_some_and(|plan| !plan.is_single_path());
    let body_sites = if parallel {
        parallel_body_sites(ir, entry, path_plan.as_ref().expect("parallel plan"))
    } else {
        SecondaryMap::new()
    };

    let mut out = String::new();
    out.push_str("; flow-backend-llvm emitted module\n");
    out.push_str(RT_DECLS);
    if parallel {
        out.push_str(PAR_DECLS);
    }
    out.push('\n');

    let sg = emit_str_globals(&strings);
    if !sg.is_empty() {
        out.push_str(&sg);
        out.push('\n');
    }

    for (id, _) in ir.funcs() {
        if id == entry && parallel {
            out.push_str(&FnEmit::emit_parallel(
                ir,
                id,
                &fnames,
                &strings,
                &attrs,
                path_plan.as_ref().expect("parallel plan"),
            ));
        } else {
            let mut fe = FnEmit::new(ir, id, &fnames, &strings, &attrs);
            if let Some(&site) = body_sites.get(id) {
                fe.set_task_body_site(site);
            }
            out.push_str(&fe.emit());
        }
        out.push('\n');
    }

    out.push_str(&emit_main_wrapper(ir));
    Ok(out)
}

fn path_plan_is_acyclic(plan: &PathPlan) -> bool {
    let mut remaining = plan
        .tasks
        .iter()
        .map(|task| task.deps.len())
        .collect::<Vec<_>>();
    let mut ready = remaining
        .iter()
        .enumerate()
        .filter_map(|(task, &deps)| (deps == 0).then_some(task))
        .collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < ready.len() {
        let before = ready[cursor];
        cursor += 1;
        for (after, task) in plan.tasks.iter().enumerate() {
            if task.deps.contains(&before) {
                remaining[after] -= 1;
                if remaining[after] == 0 {
                    ready.push(after);
                }
            }
        }
    }
    ready.len() == plan.tasks.len()
}

/// Map every Map/Fold body reached from a parallel entry site to that site's
/// topo position. Lowering mints body functions per site, so the map is
/// single-valued; nested bodies inherit the outer entry site's trap position.
fn parallel_body_sites(
    ir: &CategoryIr,
    entry: FuncId,
    plan: &PathPlan,
) -> SecondaryMap<FuncId, u32> {
    let topo = ir.topo_order(entry);
    let mut topo_pos: SecondaryMap<MorphismId, u32> = SecondaryMap::new();
    for (i, &m) in topo.iter().enumerate() {
        topo_pos.insert(m, i as u32);
    }

    let mut out = SecondaryMap::new();
    for task in &plan.tasks {
        let members: &[MorphismId] = match &task.kind {
            TaskKind::Split { site, .. } => std::slice::from_ref(site),
            TaskKind::Seq { morphisms } => morphisms,
        };
        for &m in members {
            if let Operation::Map { body, .. } | Operation::Fold { body, .. } =
                ir.morphism(m).expect("morphism resolves").op
            {
                mark_body_closure(ir, body, topo_pos[m], &mut out);
            }
        }
    }
    out
}

fn mark_body_closure(
    ir: &CategoryIr,
    body: FuncId,
    site_topo: u32,
    out: &mut SecondaryMap<FuncId, u32>,
) {
    if out.insert(body, site_topo).is_some() {
        return;
    }
    for &m in &ir.func(body).expect("body resolves").morphisms {
        if let Operation::Map { body, .. } | Operation::Fold { body, .. } =
            ir.morphism(m).expect("morphism resolves").op
        {
            mark_body_closure(ir, body, site_topo, out);
        }
    }
}
