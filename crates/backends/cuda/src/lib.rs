//! CUDA backend: emits textual CUDA C++ (`.cu`) compiled by `nvcc` on a
//! remote GPU box (ADR-0020 §1; backend-cuda DESIGN). The entry point
//! [`emit`] realizes the functor `F_CUDA : Mapal-Cat → CUDA-Cat` as a String —
//! the translation unit **is** the artifact (ADR-0020). Semantics are the
//! interpreter oracle's by construction: every `Print`/trap routes through
//! the shared `mapal-rt` runtime on the host (ADR-0020 §2), integer arithmetic
//! wraps (via unsigned casts — C++ signed overflow is UB, BC2),
//! `Div`/`Mod`/`Index`/`Update` are guarded, and floats are IEEE at width
//! (`-fmad=false`, BC9).
//!
//! Emission is deterministic (L2): all names come from per-function ordinals
//! and rising counters, never slotmap bits. The one capability gap (L3) is
//! the non-canonical loop shape (multi-merge SCC), rejected as
//! [`EmitError::Unsupported`] — the same scope boundary as interp M1 /
//! rewrite R6 / llvm BL6, gated by the same `mapal_ir::loop_plan` predicate.
//!
//! [`kernel`] is the array/device heart: launch-form array-bulk ops (fn top
//! level) become one `__global__` per STRUCTURAL SHAPE (`k{f_ord}_{s_ord}`
//! of the first site — #17's dedup, one definition launched per site) plus a
//! host launch with `trap_check_after_launch()` after every launch THAT CAN
//! TRAP (DESIGN §3 + #14's `TrapCaps` trim); body-reachable token-free fns
//! become `__device__` twins (`d_fnN`, the inline form) or single
//! `__host__ __device__` definitions per the BC8 three-case qualifier rule;
//! array literals upload as host data arrays + one H→D memcpy (§2, BC11).
//!
//! [`loops`] is the loop driver: a canonical loop at fn top level becomes
//! the host-driven guard-first quartet (decide cone → guard → advance cone →
//! back edge) per `mapal_ir::loop_plan`; the fn walk's driver-ownership skip
//! (decide∪advance ∪ SCC incidence, the llvm rule) keeps cone morphisms
//! single-emitted; a carried array's merge is a host handle whose back edge
//! is a pointer swap. The last-use plan (plan-last-use §2, suggestions #2 +
//! the BC5 amendment) sharpens both sides of that swap where it proves the
//! carried state dead: an `Update` writes IN PLACE (no fresh buffer — the
//! source handle is the kernel's `out`; `func::update_site` +
//! `in_place_update`, and the twin's per-thread copy is skipped in
//! `kernel::DevEmit`), and the back edge frees the merge's outgoing buffer
//! when its producer is a registered allocation (`func::emit_back_edge_frees`,
//! under the pointer-value init guard) — where the plan can't prove
//! (borrowed/escape), today's O(k·n) accumulate-to-fn-exit stays, the escape
//! epilogue value-guarded as always. Inside a body fn the same plan emits a
//! per-thread sequential quartet in the twin (kernel.rs's `DevEmit`).
//!
//! [`arena`] is the smart-arena deduced query (suggestions.md #18,
//! plan-smart-arenas v1.0): one FnScope zone per fn covers every
//! non-loop-cone buffer construction site — one arena `cudaMalloc` at fn
//! entry, per-site `arena0 + OFF` pointer inits (256 B-aligned, compile-time
//! constants), one zone release at fn exit vetoed by the pointer-range
//! escape test. Loop-cone sites keep per-buffer `cudaMalloc` (v1.1 debt).
//!
//! [`EmitOpts::perf_timing`] (suggestions.md #19a) wraps every launch in
//! CUDA events and prints machine-readable `MAPAL_PERF` lines — opt-in via
//! [`emit_with_opts`]; the default [`emit`] text is byte-identical without
//! it.

mod arena;
mod func;
mod kernel;
mod loops;
mod module;
mod ty;

use mapal_ir::{CategoryIr, FuncId, SourceLoc};
use slotmap::SecondaryMap;

use crate::func::{FnEmit, fn_signature};
use crate::kernel::{DevEmit, FnQual, Qualifiers, twin_signature};
use crate::module::{
    PRELUDE, collect_prod_structs, collect_str_globals, emit_main_wrapper, emit_prod_structs,
    emit_str_globals,
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

/// Emission options (suggestions.md #19 step a — kernel-time
/// instrumentation). All-off is the default: [`emit`] is [`emit_with_opts`]
/// at the defaults, and the default text is byte-identical to the
/// pre-options emitter (the differential and the goldens ride the default).
#[derive(Clone, Copy, Debug, Default)]
pub struct EmitOpts {
    /// Wrap every kernel launch in CUDA events: one event pair per launch
    /// site (created once per fn invocation), `cudaEventRecord(start)`
    /// before the launch, Record(stop)+Synchronize+ElapsedTime after —
    /// printing a machine-readable `MAPAL_PERF launch=<kernel> ms=<%.4f>`
    /// line per execution — plus a `MAPAL_PERF total ms=<%.4f>` line (the
    /// sum of the fn's launch times) at fn end. Trap checks are unchanged
    /// (the stop event is recorded BEFORE `trap_check_after_launch`); the
    /// elapsed sync adds a host sync only where #14 already skipped the
    /// trap readback — timing is inherently synchronizing.
    pub perf_timing: bool,
}

/// Emit one CUDA translation unit for `ir` (ADR-0020 §1). `Ok` is the `.cu`
/// text; `Err(Unsupported)` for a non-canonical loop (L3), for arrays
/// embedded in products in device value contexts (the F3 cell, DESIGN §5 —
/// a pointer-field struct cannot live in global memory under the handle
/// model; recorded, never a silent miscompile), or for a fn arena over
/// `ARENA_MAX_BYTES` (plan-smart-arenas rule 4 — the F7 precedent).
pub fn emit(ir: &CategoryIr) -> Result<String, EmitError> {
    emit_with_opts(ir, &EmitOpts::default())
}

/// [`emit`] with options (see [`EmitOpts`]).
pub fn emit_with_opts(ir: &CategoryIr, opts: &EmitOpts) -> Result<String, EmitError> {
    // Capability gate (L3): canonical loops only — every SCC has exactly one
    // merge and a well-formed `loop_plan`. Same predicate as rewrite's
    // `is_canonical` / interp M1 / llvm (BL6/BL7) — the one shared ceiling.
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
    let prods = collect_prod_structs(ir);
    let quals = Qualifiers::analyze(ir);

    // The F3 cell (DESIGN §5): arrays embedded in products in device value
    // contexts are a recorded Unsupported — never a silent miscompile.
    kernel::check_device_product_arrays(ir, &quals)?;
    // The F7 budget (DESIGN §5's documented cell): a launch-form fold's
    // array acc over the per-thread local budget is Unsupported. (Twin
    // produced locals are checked in DevEmit's declaration walk.)
    kernel::check_fold_acc_budgets(ir)?;

    // Deterministic function names: entry → mapal_main, others → fn{ordinal}
    // (the llvm scheme).
    let mut fnames: SecondaryMap<FuncId, String> = SecondaryMap::new();
    let entry = ir.entry();
    for (ord, (id, _)) in ir.funcs().enumerate() {
        let name = if id == entry {
            "mapal_main".to_string()
        } else {
            format!("fn{ord}")
        };
        fnames.insert(id, name);
    }

    let mut out = String::new();
    out.push_str("// mapal-backend-cuda emitted translation unit\n");
    out.push_str(
        "// build: nvcc -std=c++17 -fmad=true -arch=sm_89 prog.cu libmapal_rt.a -lpthread -ldl -lm -o prog\n",
    );
    out.push_str(
        "// DESIGN §4 (amended S24b, Sapir): -fmad=true is the product/perf default; conformance runs pin -fmad=false for oracle bit-parity. Host -march=native/-mfma stays forbidden in conformance.\n\n",
    );
    out.push_str(PRELUDE);

    let sg = emit_str_globals(&strings);
    if !sg.is_empty() {
        out.push('\n');
        out.push_str(&sg);
    }

    let ps = emit_prod_structs(&prods);
    if !ps.is_empty() {
        out.push('\n');
        out.push_str(&ps);
    }

    // #14's trap-capability pre-pass (kernel.rs `TrapCaps`): trap-free fns
    // and kernels drop the uniform trap convention (parameter, launch arg,
    // post-launch readback); capable code keeps it verbatim.
    let caps = kernel::TrapCaps::analyze(ir);

    // Kernels: one __global__ per STRUCTURALLY UNIQUE launch-form array-bulk
    // op shape (#17's dedup — the first site's name survives), before the
    // host definitions that launch them. Every site still launches (the
    // site's args ride the launch); FnEmit names the survivor per site.
    // Computed before the device section: HostDevice definitions are FnEmit
    // emissions too and consume the per-site survivor names. #12: a Twin fn
    // with no host caller contributes no sites — its (dead) host definition
    // was their only launcher.
    let live = kernel::host_reachable(ir);
    let kernels = kernel::emit_kernel_set(ir, &fnames, &quals, &caps, &live);

    // Device section (BC8): prototypes first (device fns may call each
    // other), then definitions — `__host__ __device__` singles for
    // pure-scalar body-reachable fns (case iv), `__device__` twins for
    // body-reachable fns with launch-form ops (case ii). A case-iv
    // definition doubles as the fn's host definition, so host code never
    // forward-references it.
    let mut dev_protos = String::new();
    let mut dev_defs = String::new();
    for (id, _) in ir.funcs() {
        match quals.get(id) {
            FnQual::HostDevice => {
                dev_protos.push_str(&fn_signature(ir, id, &fnames, FnQual::HostDevice, &caps));
                dev_protos.push_str(";\n");
            }
            FnQual::Twin => {
                dev_protos.push_str(&twin_signature(ir, id, &fnames, &caps));
                dev_protos.push_str(";\n");
            }
            FnQual::HostOnly => {}
        }
    }
    for (id, _) in ir.funcs() {
        match quals.get(id) {
            FnQual::HostDevice => {
                let mut fe = FnEmit::new(ir, id, &fnames, &strings, &quals, &caps, &kernels);
                fe.perf = opts.perf_timing;
                dev_defs.push_str(&fe.emit()?);
            }
            FnQual::Twin => {
                let de = DevEmit::new(ir, id, &fnames, &quals, &caps);
                dev_defs.push_str(&de.emit()?);
            }
            FnQual::HostOnly => {}
        }
    }
    if !dev_protos.is_empty() {
        out.push('\n');
        out.push_str(&dev_protos);
    }
    if !dev_defs.is_empty() {
        out.push('\n');
        out.push_str(&dev_defs);
    }

    // (computed above — the definitions precede the host fns that launch
    // them)
    if !kernels.text.is_empty() {
        out.push('\n');
        out.push_str(&kernels.text);
    }

    // Prototypes before definitions: C++ resolves calls at definition order,
    // the call graph may name a later fn. Case-iv fns are already defined in
    // the device section; their prototype (and only it) serves both sites.
    // #12: a Twin fn with no host caller gets no host prototype/definition —
    // device code names only its `d_` twin (dead-text deletion, no behavior
    // change: nothing on the host path could call it).
    out.push('\n');
    for (id, _) in ir.funcs() {
        if quals.get(id) == FnQual::HostDevice || kernel::dead_host_twin(&quals, &live, id) {
            continue;
        }
        out.push_str(&fn_signature(ir, id, &fnames, quals.get(id), &caps));
        out.push_str(";\n");
    }
    out.push('\n');

    for (id, _) in ir.funcs() {
        if quals.get(id) == FnQual::HostDevice || kernel::dead_host_twin(&quals, &live, id) {
            continue;
        }
        let mut fe = FnEmit::new(ir, id, &fnames, &strings, &quals, &caps, &kernels);
        fe.perf = opts.perf_timing;
        out.push_str(&fe.emit()?);
        out.push('\n');
    }

    out.push_str(&emit_main_wrapper(ir));
    Ok(out)
}
