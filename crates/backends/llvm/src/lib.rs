//! LLVM backend: emits textual LLVM IR (`.ll`) piped to `clang` (ADR-0020 §1;
//! backend-llvm DESIGN). The entry point [`emit`] realizes the functor
//! `F_LLVM : Mapal-Cat → LLVM-Cat` as a String — the translation unit **is** the
//! artifact (ADR-0020). Semantics are the interpreter oracle's by construction:
//! every `Print`/trap routes through the shared `mapal-rt` runtime (ADR-0020 §2),
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
mod profile;
mod reuse;
mod ty;

pub use crate::profile::TargetProfile;

use mapal_ir::{CategoryIr, FuncId, MorphismId, Operation, PathPlan, SourceLoc, TaskKind};
use slotmap::SecondaryMap;

use crate::func::{FnAttrs, FnEmit, packing_site};
use crate::module::{
    HEAP_DECLS, PAR_DECLS, PERF_DECLS, PREFETCH_DECL, RT_DECLS, collect_str_globals,
    emit_main_wrapper, emit_str_globals,
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

/// Emission options. [`emit`] delegates to these product defaults.
#[derive(Clone, Copy, Debug)]
pub struct EmitOpts {
    /// Bracket `mapal_main` with the mapal-rt compute timer.
    pub perf_timing: bool,
    /// Tile recognized matmul-shaped map sites.
    pub tiling: bool,
    /// Pack tiled two-dimensional right-hand operands.
    pub packing: bool,
    /// Use single-rounding FMA contraction on the product face; the default
    /// conformance face stays bit-exact.
    pub contract: bool,
    /// Split deep packed tile sites into k-panels — the OpenBLAS (jc, kc, ic)
    /// nest with an A-panel pack and partial sums parked in `out`.
    ///
    /// **Default OFF: measured a 3× LOSS** on M4 Pro at 1024 f32 (S29 —
    /// `fma` 59.8 ms on / 19.8 ms off; the parking traffic outweighs the A
    /// re-read it removes at this size and cache hierarchy). Kept and tested
    /// because the lever was designed against BOX-scale traffic (16 GB of A
    /// re-reads at 4096 on zen3) where it has not yet been measured. A pure
    /// performance tailor in ADR-0032's sense: bit-exact either way, which the
    /// differential suite enforces.
    pub kc_nest: bool,
    /// The machine facts the emitter tiles against, selected **by name**
    /// (plan-s31-target-profiles): `generic` (the default — today's literals,
    /// byte-identical), `apple-m`, `zen3`. Nothing probes the host; a box run
    /// names `zen3` rather than inheriting whatever machine the build happened
    /// on, so emission stays reproducible and cross-compilable.
    ///
    /// A pure ADR-0032 D4 performance tailor: every profile field is
    /// value-invariant, which the differential suite enforces by running under
    /// a non-default profile.
    pub target: &'static str,
}

impl Default for EmitOpts {
    fn default() -> Self {
        Self {
            perf_timing: false,
            tiling: true,
            packing: true,
            contract: false,
            kc_nest: false,
            target: "generic",
        }
    }
}

/// Emit one LLVM translation unit for `ir` (ADR-0020 §1). `Ok` is the `.ll`
/// text; `Err(Unsupported)` for a non-canonical loop (L3).
pub fn emit(ir: &CategoryIr) -> Result<String, EmitError> {
    emit_with_opts(ir, &EmitOpts::default())
}

/// [`emit`] with options (see [`EmitOpts`]).
pub fn emit_with_opts(ir: &CategoryIr, opts: &EmitOpts) -> Result<String, EmitError> {
    // Machine facts, by name (plan-s31-target-profiles rule 3). An unknown name
    // is an error, never a silent fall back to `generic` — a typo that quietly
    // emits the default profile's numbers is the exact failure the table exists
    // to remove.
    let Some(profile) = profile::resolve(opts.target) else {
        return Err(EmitError::Internal(format!(
            "unknown target profile `{}`; known: {}",
            opts.target,
            profile::names()
        )));
    };

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

    // Deterministic function names: entry → @mapal_main, others → @fn{ordinal}.
    let mut fnames: SecondaryMap<mapal_ir::FuncId, String> = SecondaryMap::new();
    let entry = ir.entry();
    for (ord, (id, _)) in ir.funcs().enumerate() {
        let name = if id == entry {
            "mapal_main".to_string()
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
    let prefetch = opts.tiling
        && opts.packing
        && ir.funcs().any(|(f, _)| {
            ir.tile_plan(f)
                .sites
                .iter()
                .any(|(_, site)| packing_site(site))
        });

    // Functions first: the arena declarations are gated on the emitted text
    // (below), so the bodies have to exist before the header is assembled.
    let mut funcs = String::new();
    for (id, _) in ir.funcs() {
        if id == entry && parallel {
            funcs.push_str(&FnEmit::emit_parallel(
                ir,
                id,
                &fnames,
                &strings,
                &attrs,
                path_plan.as_ref().expect("parallel plan"),
                opts.perf_timing,
                opts.tiling,
                opts.packing,
                opts.contract,
                opts.kc_nest,
                profile,
            ));
        } else {
            let mut fe = FnEmit::new(
                ir,
                id,
                &fnames,
                &strings,
                &attrs,
                opts.tiling,
                opts.packing,
                opts.contract,
                opts.kc_nest,
                profile,
            );
            if id == entry {
                fe.set_perf_timing(opts.perf_timing);
            }
            if let Some(&site) = body_sites.get(id) {
                fe.set_task_body_site(site);
            }
            funcs.push_str(&fe.emit());
        }
        funcs.push('\n');
    }

    let mut out = String::new();
    out.push_str("; mapal-backend-llvm emitted module\n");
    out.push_str(RT_DECLS);
    if prefetch {
        out.push_str(PREFETCH_DECL);
    }
    if opts.perf_timing {
        out.push_str(PERF_DECLS);
    }
    if parallel {
        out.push_str(PAR_DECLS);
    }
    // ponytail: gate the arena declarations on the emitted call itself, not on
    // a re-derived predicate — the call IS the requirement, so the two cannot
    // drift the way a `prefetch`-style pre-pass could. (PAR/PERF must gate on a
    // predicate: they are decided before any body exists.)
    if funcs.contains("@mapal_rt_alloc") {
        out.push_str(HEAP_DECLS);
    }
    out.push('\n');

    let sg = emit_str_globals(&strings);
    if !sg.is_empty() {
        out.push_str(&sg);
        out.push('\n');
    }

    out.push_str(&funcs);
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
