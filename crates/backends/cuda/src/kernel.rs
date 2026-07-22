//! `kernel.rs` (DESIGN §1 execution mapping, §2 memory model, §3 device trap
//! flag; BC3–BC5, BC8): the device half of the backend. Three pieces:
//!
//! - **BC8 qualifier analysis** ([`Qualifiers`]) — the three-case rule over
//!   the call graph (§1): token-bearing ⇒ host-only; token-free ∧ reachable
//!   from any map/fold body ⇒ device-visible; token-free ∧ NOT body-reachable
//!   ⇒ host-only (a `__device__` instantiation of a fn containing launch-form
//!   ops would need CDP/rdc — HW-1's illegal-CUDA case, out at M3). Of the
//!   body-reachable token-free fns, the ones with **no launch-form ops
//!   anywhere** (transitively), no array-typed return, and no transitive
//!   call to a Twin fn get a single `__host__ __device__` definition; the
//!   rest get a host definition **plus** a `__device__` twin (the inline
//!   form). An array-typed return forces the twin: a single source cannot
//!   both return a device buffer handle (host) and write a per-thread local
//!   result (device, out-param convention) — and Twin-ness propagates
//!   callerward through calls, or the caller's two-site definition would
//!   call the Twin's host definition on the device pass (F4).
//! - **Kernel emission** ([`emit_kernel`], [`emit_kernel_set`]) — one
//!   `__global__` per launch-form array-bulk op site, one-thread-per-element,
//!   256 threads/block, 64-bit index arithmetic (BC3), **deduplicated by
//!   structural shape**: each unique kernel text is emitted once (under the
//!   first site's name) and launched per site — the site's args ride the
//!   launch (suggestions.md #17). Site names: `k{f_ord}_{site_ord}` — `f_ord`
//!   is the fn's ordinal in `ir.funcs()` (the `fn{f_ord}` scheme's ordinal),
//!   and `site_ord` counts the fn's launch-form bulk-op sites in
//!   `topo_order`.
//! - **`__device__` twins** ([`DevEmit`]) — the inline form: the per-thread
//!   sequential op table for body-reachable fns that need device code
//!   distinct from their host definition. Twin names: `d_{fname}`.
//!
//! **The trap-pointer convention (§3).** The device trap flag rides every
//! trap-CAPABLE kernel launch and device-side call as a trailing
//! `unsigned int* trap` argument — #14's [`TrapCaps`] pre-pass drops the
//! convention where no guard can fire (a host `static` global is not
//! addressable from device code, so the flag pointer is threaded
//! explicitly). In-kernel and in-twin guards do `*trap = kind + 1; return …;`
//! — the flag is cudaMemset-zeroed (0 = quiescent), so it stores the
//! flow-rt kind **plus one** (div_zero ⇒ `1u`, index_oob ⇒ `2u`); the host
//! readback decodes with `flow_trap(kind - 1)` (module.rs). A bare-kind
//! store would collide: div_zero's 0 would read back as "no trap". After a
//! device-side `Call` to a capable fn the caller emits `if (*trap) return
//! …;` to unwind to the launching kernel (the oracle's first-trap-wins
//! order, §3). `__host__ __device__` fns take the same trailing parameter
//! when capable — host callers pass the host global `d_trap`, unused on the
//! host pass because host guards call `flow_trap` directly.
//!
//! **The width rule (§3, llvm `func.rs:589–619` ported).** An `Index`/`Update`
//! index operand is extended to `int64_t` *per its `Ty`* — in C++ the
//! conversion `(int64_t)v` is value-preserving for both unsigned (zero-ext)
//! and signed (sign-ext) operands, so one cast realizes llvm's zext/sext
//! split. The bounds check is then a **signed two-sided compare**,
//! `idx < 0 || idx >= (int64_t)n` — never against `size_t` (the usual
//! arithmetic conversions would re-unsigned a negative operand into a huge
//! positive one).
//!
//! **Kernel geometry (BC3).** Elementwise kernels (Map/Zip/Enumerate/Update)
//! use `unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x +
//! threadIdx.x; if (i < n) …` with grid = `ceil(n/256)` computed in 64-bit —
//! `n` is a compile-time constant (it lives in `Ty::Array`), so `n` and the
//! grid expression are baked into the text as `u64` literals. Iota/Fill carry
//! that constant as a `long long n` launch argument and use the same BC3 grid.
//! Top-level `Index` and `Fold` launch `<<<1, 1>>>` (BC4's single thread).

use flow_ir::{
    BoundsProof, CategoryIr, EmissionClass, EmissionPlan, FuncId, FuncKind, Morphism, MorphismId,
    ObjectId, ObjectKind, Operation, SourceLoc, Ty, Value,
};
use slotmap::SecondaryMap;
use std::collections::HashMap;

use crate::EmitError;
use crate::func::{const_literal, in_place_update, is_float, unsigned_twin};
use crate::ty::{
    erased_index, lower_ty, residual_arity, residual_contains_array,
    tree_contains_product_with_array,
};

// ---------------------------------------------------------------------------
// BC8 qualifier analysis
// ---------------------------------------------------------------------------

/// The BC8 case of one fn (§1). `HostOnly` fns exist only as WP2 host
/// definitions; `HostDevice` fns get a single `__host__ __device__`
/// definition (pure-scalar token-free); `Twin` fns get the host definition
/// plus a `d_{fname}` `__device__` twin emitted by [`DevEmit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FnQual {
    HostOnly,
    HostDevice,
    Twin,
}

/// Per-fn BC8 qualifiers, computed once per module.
pub(crate) struct Qualifiers {
    map: SecondaryMap<FuncId, FnQual>,
}

impl Qualifiers {
    /// The qualifier of `f` (`HostOnly` for unknown ids — a sealed graph's
    /// own fns are all present).
    pub(crate) fn get(&self, f: FuncId) -> FnQual {
        self.map.get(f).copied().unwrap_or(FnQual::HostOnly)
    }

    /// The device-visible name of `g`: the twin for [`FnQual::Twin`], the
    /// plain name for [`FnQual::HostDevice`]. Panics on `HostOnly` — a
    /// token-free body-reachable fn's callees are always device-visible
    /// (token introduction is signature-synthesis-only, ADR-0013 law i, so a
    /// token-free caller implies a token-free, hence body-reachable, callee).
    pub(crate) fn device_name(&self, fnames: &SecondaryMap<FuncId, String>, g: FuncId) -> String {
        match self.get(g) {
            FnQual::Twin => format!("d_{}", fnames[g]),
            FnQual::HostDevice => fnames[g].clone(),
            FnQual::HostOnly => unreachable!("HostOnly fn called from device code"),
        }
    }

    /// The three-case analysis (§1, BC8). One pass over the call graph:
    /// seeds body-reachability from the `MapBody`/`FoldBody` fns, then it
    /// flows calleeward over `Call` edges while **Twin-ness** (a direct
    /// launch-form op, or an array-typed return — the out-param convention
    /// forces the twin) flows **callerward** over `Call` edges, both by
    /// fixpoint (robust to recursion). Callerward propagation of the
    /// array-return rule matters (F4): a body-reachable pure-scalar fn
    /// calling an array-returning Twin must be a Twin itself — as
    /// `__host__ __device__` its single definition would call the callee's
    /// HOST definition on the device pass (illegal CUDA).
    pub(crate) fn analyze(ir: &CategoryIr) -> Qualifiers {
        let mut map: SecondaryMap<FuncId, FnQual> = SecondaryMap::new();

        // Direct facts per fn.
        let mut body_reachable: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        // (bulk ∨ array-return), seeded direct; the fixpoint closes it
        // callerward — after the fixpoint this is "needs the Twin form".
        let mut needs_twin: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut calls: SecondaryMap<FuncId, Vec<FuncId>> = SecondaryMap::new();
        for (f, fd) in ir.funcs() {
            body_reachable.insert(f, fd.kind != FuncKind::Named);
            let mut direct = false;
            for &m in &fd.morphisms {
                let morph = ir.morphism(m).expect("morphism resolves");
                match morph.op {
                    Operation::Map { .. }
                    | Operation::Zip
                    | Operation::Enumerate
                    | Operation::Iota
                    | Operation::Fill
                    | Operation::Fold { .. }
                    | Operation::Index
                    | Operation::Update => direct = true,
                    // An array construction site (pack_array literal): the host
                    // form cudaMalloc+cudaMemcpy's, so it is a launch-form op.
                    Operation::Pair { .. } => {
                        let tty = &ir.object(morph.target).expect("target resolves").ty;
                        if matches!(tty, Ty::Array { .. }) {
                            direct = true;
                        }
                    }
                    Operation::Call(g) => {
                        let mut v = calls.get(f).cloned().unwrap_or_default();
                        v.push(g);
                        calls.insert(f, v);
                    }
                    _ => {}
                }
            }
            let out_ty = &ir.object(fd.output).expect("output resolves").ty;
            // An array-typed return forces the twin (the out-param
            // convention) — but only when the array LOWERS: an erased
            // array (Array{Unit}) has no device buffer, the convention is
            // moot, and a single `__host__ __device__` void definition
            // serves both sites (F6).
            needs_twin.insert(
                f,
                direct || (matches!(out_ty, Ty::Array { .. }) && lower_ty(out_ty).is_some()),
            );
        }

        // Fixpoint: body-reachability flows calleeward over Call edges (a
        // caller of a body-reachable fn is NOT body-reachable — the
        // direction is from bodies outward to what bodies call); Twin-ness
        // flows callerward (a body-reachable caller of a Twin needs the
        // device instantiation of its callee — hence its own twin).
        loop {
            let mut changed = false;
            for (f, _) in ir.funcs() {
                let callees = calls.get(f).cloned().unwrap_or_default();
                for g in callees {
                    if body_reachable[f] && !body_reachable.get(g).copied().unwrap_or(false) {
                        body_reachable.insert(g, true);
                        changed = true;
                    }
                    if needs_twin.get(g).copied().unwrap_or(false) && !needs_twin[f] {
                        needs_twin.insert(f, true);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        for (f, fd) in ir.funcs() {
            let in_ty = &ir.object(fd.input).expect("input resolves").ty;
            let out_ty = &ir.object(fd.output).expect("output resolves").ty;
            let token = flow_ir::ty_contains_token(in_ty) || flow_ir::ty_contains_token(out_ty);
            let qual = if token || !body_reachable.get(f).copied().unwrap_or(false) {
                // (i) token-bearing ⇒ host-only; (iii) token-free ∧ NOT
                // body-reachable ⇒ host-only (regardless of bulk ops — no
                // device instantiation is ever needed).
                FnQual::HostOnly
            } else if needs_twin.get(f).copied().unwrap_or(false) {
                // (ii) token-free ∧ body-reachable ∧ (launch-form ops ∨
                // array return, transitively through calls) ⇒ twins. An
                // array return forces the twin: the single-definition form
                // cannot serve a device buffer handle on the host and a
                // per-thread local result on the device (out-param
                // convention).
                FnQual::Twin
            } else {
                // (iv) pure-scalar token-free, body-reachable: one
                // `__host__ __device__` definition serves both sites.
                FnQual::HostDevice
            };
            map.insert(f, qual);
        }
        Qualifiers { map }
    }
}

// ---------------------------------------------------------------------------
// #14 trap-capability pre-pass
// ---------------------------------------------------------------------------

/// Per-fn DEVICE trap capability (suggestions.md #14), computed once per
/// module: can the fn's device code (its `__device__` twin or
/// `__host__ __device__` definition, transitively through the calls it
/// makes) set the trap flag? Conservative: any **integer** `Div`/`Mod` (the
/// §3 zero guard), any `Update` or unproven `Index` (the §3 bounds guards)
/// reachable ⇒ capable. Float `Div`/`Mod` never guard (ADR-0013's S13: ÷0 is
/// ±inf/NaN). The S20 refinement: an `Index` the fn's [`BoundsProof`] clears
/// (provably in `[0, n)`) can never fire its guard — it is NOT a trap
/// source, exactly as if the guard were elided; anything unknown/wrapping/
/// loop-carried stays capable (the analysis's `None` answer keeps today's
/// behavior). The device call graph is `Call` edges plus `Map`/`Fold`
/// **body** edges (a twin's inline map/fold loops call the body's device
/// form); capability flows callerward by fixpoint (robust to recursion).
/// The rule is purely syntactic on the op set: an integer `Div`/`Mod` counts
/// even when #13's constant-divisor elision removes its guard (a `t / 4`
/// keeps its fn capable) — keeping the param where no guard survives is
/// harmless, and the two passes stay independently simple.
///
/// A trap-FREE fn or kernel drops the uniform trap convention: no
/// `unsigned int* trap` parameter, no trap argument at its call sites, no
/// post-call `if (*trap)` check, and — for a kernel — no
/// `trap_check_after_launch()` after its launch (fewer host syncs; this is
/// the perf-visible part). Wherever any guard can fire the convention is
/// kept verbatim, so class parity and first-trap-wins are preserved: a
/// dropped post-call check is dead by construction (the callee cannot set
/// the flag), and every capable call keeps its own check, in order.
pub(crate) struct TrapCaps {
    map: SecondaryMap<FuncId, bool>,
    /// The per-fn bounds proofs behind the Index rule (the S20 deduced
    /// query), computed once here — `site` reads them back by the site's
    /// owner fn instead of re-deriving the analysis per query (BL7).
    proofs: SecondaryMap<FuncId, BoundsProof>,
}

impl TrapCaps {
    pub(crate) fn analyze(ir: &CategoryIr) -> TrapCaps {
        let mut map: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut proofs: SecondaryMap<FuncId, BoundsProof> = SecondaryMap::new();
        let mut edges: SecondaryMap<FuncId, Vec<FuncId>> = SecondaryMap::new();
        for (f, fd) in ir.funcs() {
            let proof = ir.bounds_proof(f);
            let mut capable = false;
            for &m in &fd.morphisms {
                let morph = ir.morphism(m).expect("morphism resolves");
                match morph.op {
                    Operation::Div | Operation::Mod => {
                        let src_ty = &ir.object(morph.source).expect("source resolves").ty;
                        if matches!(src_ty.component_ty(0), Some(Ty::Int { .. })) {
                            // #13 credit (S20): a literal non-zero, non-−1
                            // constant divisor cannot trap — the guard elision
                            // already proves the check dead; capability credits it.
                            let safe = const_int_operand(ir, morph.source, 1)
                                .is_some_and(|v| v != 0 && v != -1);
                            if !safe {
                                capable = true;
                            }
                        }
                    }
                    // S20: only an UNPROVEN Index can fire its bounds guard.
                    Operation::Index if !proof.proven(m) => capable = true,
                    // Update stays conservative this wave (no proof query).
                    Operation::Update => capable = true,
                    // ADR-0029: scalar Widen is total and trap-free.
                    Operation::Widen => {}
                    Operation::Call(g) => {
                        let mut v = edges.get(f).cloned().unwrap_or_default();
                        v.push(g);
                        edges.insert(f, v);
                    }
                    Operation::Map { body, .. } | Operation::Fold { body, .. } => {
                        let mut v = edges.get(f).cloned().unwrap_or_default();
                        v.push(body);
                        edges.insert(f, v);
                    }
                    _ => {}
                }
            }
            map.insert(f, capable);
            proofs.insert(f, proof);
        }
        loop {
            let mut changed = false;
            for (f, _) in ir.funcs() {
                if map.get(f).copied().unwrap_or(false) {
                    continue;
                }
                for g in edges.get(f).cloned().unwrap_or_default() {
                    if map.get(g).copied().unwrap_or(false) {
                        map.insert(f, true);
                        changed = true;
                        break;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        TrapCaps { map, proofs }
    }

    /// The capability of `f` (`false` for unknown ids — a sealed graph's own
    /// fns are all present).
    pub(crate) fn get(&self, f: FuncId) -> bool {
        self.map.get(f).copied().unwrap_or(false)
    }

    /// The trap capability of one launch-form bulk-op site — the kernel-side
    /// half of the pre-pass: `Map`/`Fold` ride their body fn's capability
    /// (the per-thread/per-step body call is the only trap source); an
    /// UNPROVEN `Index` / any `Update` guards in-kernel (§3) — a
    /// `bounds_proof`-proven `Index` (S20) can never fire its guard, so the
    /// site is trap-free (no proof cached for the owner fn answers
    /// conservatively: capable); `Zip`/`Enumerate` never trap.
    pub(crate) fn site(&self, ir: &CategoryIr, m: MorphismId) -> bool {
        let morph = ir.morphism(m).expect("morphism resolves");
        match morph.op {
            Operation::Map { body, .. } | Operation::Fold { body, .. } => self.get(body),
            Operation::Index => {
                let owner = ir.owner(morph.target);
                !self.proofs.get(owner).map(|p| p.proven(m)).unwrap_or(false)
            }
            Operation::Update => true,
            // ADR-0029: Iota/Fill are trap-free by construction (like Zip/Enumerate).
            Operation::Zip | Operation::Enumerate | Operation::Iota | Operation::Fill => false,
            _ => unreachable!("not a bulk-op site"),
        }
    }
}

// ---------------------------------------------------------------------------
// F3's recorded Unsupported cell: arrays embedded in products on device
// ---------------------------------------------------------------------------

/// The element tys a launch-form bulk-op site's kernel loads from / stores
/// into global memory (the F3 cell's domain): a product-with-array element
/// would put a local-memory or buffer-interior pointer into a global buffer.
fn site_element_tys(ir: &CategoryIr, morph: &Morphism) -> Vec<Ty> {
    let ty_of = |o: ObjectId| ir.object(o).expect("object resolves").ty.clone();
    let elem_of = |t: &Ty| match t {
        Ty::Array { elem, .. } => Some((**elem).clone()),
        _ => None,
    };
    let src = ty_of(morph.source);
    let tgt = ty_of(morph.target);
    let mut out: Vec<Ty> = Vec::new();
    match morph.op {
        Operation::Map { captures, .. } => {
            // ADR-0027: k>0 ⇒ the mapped array is the source product's last
            // component; the leading captures ride as kernel parameters
            // (ordinary dataflow — never an escaping element type).
            let arr = if captures == 0 {
                Some(&src)
            } else {
                src.component_ty(captures)
            };
            if let Some(at) = arr {
                out.extend(elem_of(at));
            }
            out.extend(elem_of(&tgt));
        }
        Operation::Enumerate => {
            out.extend(elem_of(&src));
            out.extend(elem_of(&tgt));
        }
        Operation::Iota | Operation::Fill => {
            out.extend(elem_of(&tgt));
        }
        Operation::Zip => {
            // The source is the (A, B) pair of input arrays; the target's
            // element is the (EA, EB) product the kernel stores.
            for k in 0..2 {
                if let Some(at) = src.component_ty(k) {
                    out.extend(elem_of(at));
                }
            }
            out.extend(elem_of(&tgt));
        }
        Operation::Update => {
            // The array's element and the replacement value.
            if let Some(at) = src.component_ty(0) {
                out.extend(elem_of(at));
            }
            if let Some(vt) = src.component_ty(2) {
                out.push(vt.clone());
            }
        }
        Operation::Index => {
            // The array's element IS the result (device buffer or readback).
            if let Some(at) = src.component_ty(0) {
                out.extend(elem_of(at));
            }
        }
        Operation::Fold { captures, .. } => {
            // The accumulator and the array's element (ADR-0027: the
            // captures shift them to components k and k+1).
            if let Some(acc) = src.component_ty(captures) {
                out.push(acc.clone());
            }
            if let Some(at) = src.component_ty(captures + 1) {
                out.extend(elem_of(at));
            }
        }
        _ => {}
    }
    out
}

/// The F3 cell (DESIGN §5's recorded `Unsupported`, "never a silent
/// miscompile"): reject a product whose residual contains an array in any
/// DEVICE value context that escapes per-thread storage —
///
/// - **a kernel's element type** (every launch-form bulk-op site's global-
///   memory loads/stores — incl. arrays OF such products and a consuming
///   `Index` across launches), and
/// - **a Twin/`__host__ __device__` fn's return type** (a pointer-field
///   struct returned by value on the device pass, or written through the
///   array out-param into the caller's buffer),
///
/// plus **device-side locals** of the product type itself — minus the
/// shapes that provably stay in per-thread storage: the input parameter
/// (arrives by value — the fold body's `(acc, e)` pair) and the transient
/// operand aggregates of bulk ops / calls (their fields are gathered
/// field-wise by the op; the struct itself is never stored to global
/// memory). ADR-0027 adds the same shape once more: a product used **only
/// by `Proj` edges whose surviving array components are all projected
/// out** (the captured map/fold source's feeder wire — the lower projects
/// the seed and the captured handles into the op's source product) is
/// destructured field-wise exactly like a bulk-op source: the struct never
/// escapes per-thread storage and its array content leaves only as
/// ordinary handles. Host-side products holding handles are NOT the cell
/// (their `T*` fields are host copies of device pointers — the F2 shape),
/// and neither are arrays OF scalar products, products of scalar products,
/// or scalar products.
pub(crate) fn check_device_product_arrays(
    ir: &CategoryIr,
    quals: &Qualifiers,
) -> Result<(), EmitError> {
    const CELL: &str = "arrays embedded in products on device";
    let cell = |loc: SourceLoc| EmitError::Unsupported {
        feature: CELL.into(),
        loc,
    };
    for (f, fd) in ir.funcs() {
        for site in collect_sites(ir, f) {
            let morph = ir.morphism(site.m).expect("morphism resolves");
            if site_element_tys(ir, morph)
                .iter()
                .any(tree_contains_product_with_array)
            {
                return Err(cell(morph.loc));
            }
        }
        if quals.get(f) == FnQual::HostOnly {
            continue; // host-only fns hold handles, never device values
        }
        let out_obj = ir.object(fd.output).expect("output resolves");
        if tree_contains_product_with_array(&out_obj.ty) {
            return Err(cell(out_obj.loc));
        }
        // The transient operand aggregates of this fn's bulk ops / calls:
        // consumed field-wise by the op, never stored (the in-twin Index
        // pair, the in-twin fold's (seed, array) pair, a device call's
        // by-value argument struct).
        let mut transient: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for &m in &fd.morphisms {
            let morph = ir.morphism(m).expect("morphism resolves");
            if is_bulk_op(morph.op) || matches!(morph.op, Operation::Call(_)) {
                transient.insert(morph.source, ());
            }
        }
        for (o, obj) in ir.objects() {
            if ir.try_owner(o) != Some(f)
                || o == fd.input
                || o == fd.output
                || obj.kind == ObjectKind::Constant
                || transient.contains_key(o)
            {
                continue;
            }
            if residual_contains_array(&obj.ty) && !proj_destructured(ir, o, &obj.ty) {
                return Err(cell(obj.loc));
            }
        }
    }
    Ok(())
}

/// The ADR-0027 transient shape: is object `o` (typed `ty`, a product whose
/// residual contains an array) used **only** by `Proj` edges, with every
/// surviving array-typed component among the projected indices? Then the
/// struct is destructured field-wise and never escapes per-thread storage
/// (its array content leaves only as `{T}*` handle copies), exactly like a
/// bulk-op source aggregate. A product with no out-edges, a non-Proj use, or
/// an unprojected array component keeps the recorded cell.
fn proj_destructured(ir: &CategoryIr, o: ObjectId, ty: &Ty) -> bool {
    let edges = ir.out_edges(o);
    if edges.is_empty() {
        return false;
    }
    let mut projected: Vec<u32> = Vec::new();
    for &m in edges {
        let morph = ir.morphism(m).expect("morphism resolves");
        match morph.op {
            Operation::Proj { index } => projected.push(index),
            _ => return false,
        }
    }
    let mut k = 0u32;
    while let Some(comp) = ty.component_ty(k) {
        if lower_ty(comp).is_some() && matches!(comp, Ty::Array { .. }) && !projected.contains(&k) {
            return false;
        }
        k += 1;
    }
    true
}

// ---------------------------------------------------------------------------
// Launch-form op sites
// ---------------------------------------------------------------------------

/// One launch-form array-bulk op site: the morphism and its kernel's name.
pub(crate) struct Site {
    pub m: MorphismId,
    pub kname: String,
}

/// Is `op` a launch-form array-bulk op (one kernel per site)?
fn is_bulk_op(op: Operation) -> bool {
    matches!(
        op,
        Operation::Map { .. }
            | Operation::Zip
            | Operation::Enumerate
            | Operation::Iota
            | Operation::Fill
            | Operation::Fold { .. }
            | Operation::Index
            | Operation::Update
    )
}

/// The fn's launch-form bulk-op sites in `topo_order`, named
/// `k{f_ord}_{site_ord}`. Sites inside a canonical loop's decide/advance
/// cones are included (WP4): the loop driver emits their host launches
/// through the same op table, once per iteration at runtime.
pub(crate) fn collect_sites(ir: &CategoryIr, f: FuncId) -> Vec<Site> {
    let f_ord = ir.funcs().position(|(id, _)| id == f).expect("fn resolves");
    let mut out = Vec::new();
    for m in ir.topo_order(f) {
        let morph = ir.morphism(m).expect("morphism resolves");
        if is_bulk_op(morph.op) {
            let kname = format!("k{f_ord}_{}", out.len());
            out.push(Site { m, kname });
        }
    }
    out
}

/// The fns whose HOST definition can execute: the entry plus everything
/// reachable from it through `Call` edges (fixpoint; robust to recursion).
/// Map/fold bodies are invoked by kernels, not host calls, so a fn outside
/// this set has no host caller at runtime.
pub(crate) fn host_reachable(ir: &CategoryIr) -> SecondaryMap<FuncId, bool> {
    let mut live: SecondaryMap<FuncId, bool> = SecondaryMap::new();
    live.insert(ir.entry(), true);
    loop {
        let mut changed = false;
        for (f, fd) in ir.funcs() {
            if !live.get(f).copied().unwrap_or(false) {
                continue;
            }
            for &m in &fd.morphisms {
                let morph = ir.morphism(m).expect("morphism resolves");
                if let Operation::Call(g) = morph.op
                    && !live.get(g).copied().unwrap_or(false)
                {
                    live.insert(g, true);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    live
}

/// The dead-host-twin predicate (suggestions.md #12): a [`FnQual::Twin`] fn
/// with no host caller is invoked only from device code (kernels and twins
/// name its `d_` twin), so its host definition — and the launch-form kernels
/// only that definition launches — are dead text. Skipping them changes no
/// launch and no behavior; the twin (device section) is unaffected.
pub(crate) fn dead_host_twin(
    quals: &Qualifiers,
    live: &SecondaryMap<FuncId, bool>,
    f: FuncId,
) -> bool {
    quals.get(f) == FnQual::Twin && !live.get(f).copied().unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Shared shape helpers
// ---------------------------------------------------------------------------

/// The innermost element ty of a (possibly nested) array (DESIGN §5 flat
/// aggregate). Identity for non-arrays.
fn flat_base(ty: &Ty) -> &Ty {
    let mut t = ty;
    while let Ty::Array { elem, .. } = t {
        t = elem;
    }
    t
}

/// The flat element count of `ty`: the product of array sizes along the
/// nesting (1 for non-arrays).
pub(crate) fn flat_count(ty: &Ty) -> u64 {
    let mut n = 1u64;
    let mut t = ty;
    while let Ty::Array { elem, size } = t {
        n *= size;
        t = elem;
    }
    n
}

/// The pinned per-thread local-array budget (DESIGN §5's documented cell:
/// "if it exceeds the per-thread local-memory budget the honest cell is a
/// documented `Unsupported`, recorded here and in STATUS — never
/// improvised"). A per-thread local array — a twin's produced local
/// (`{T} oK[{flat}]`, §2 item 8) or the fold kernel's array-acc copy
/// (`{T} acc[{m}];`, BC4) — whose nominal flat byte size exceeds this is
/// rejected, not emitted. 16 KiB sits far above the corpus (sepia's
/// `[Pixel; 16]` is 192 B) and far below the local-memory wall.
pub(crate) const MAX_LOCAL_ARRAY_BYTES: u64 = 16384;

/// The nominal (unpadded) byte size of a flat element: the scalar width;
/// a product sums its surviving components (an array component rides as a
/// handle — pointer width). Erased ⇒ `None` (no storage to budget). This
/// is a deterministic budget measure, not the ABI `sizeof` (padding is the
/// target compiler's business).
fn nominal_sizeof(ty: &Ty) -> Option<u64> {
    match ty {
        Ty::Int { bits, .. } | Ty::Float { bits } => Some(*bits as u64 / 8),
        Ty::Bool => Some(1),
        Ty::Unit | Ty::Str | Ty::IoToken => None,
        Ty::Array { .. } => Some(8), // a handle field
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let mut sum = 0u64;
            let mut k = 0u32;
            while let Some(c) = ty.component_ty(k) {
                if lower_ty(c).is_some() {
                    sum += nominal_sizeof(c)?;
                }
                k += 1;
            }
            Some(sum)
        }
    }
}

/// The nominal flat byte size of `ty` as a per-thread local array —
/// `flat_count(ty) * nominal_sizeof(base)` — or `None` if the flat base is
/// erased (no local exists to budget).
pub(crate) fn local_array_bytes(ty: &Ty) -> Option<u64> {
    Some(flat_count(ty) * nominal_sizeof(flat_base(ty))?)
}

/// `x` rounded up to a multiple of `a` (a power of two).
pub(crate) fn align_up(x: u64, a: u64) -> u64 {
    (x + a - 1) & !(a - 1)
}

/// The C-layout byte size of `ty`'s lowered form — the arena measure
/// (plan-smart-arenas §3: offsets must be ABI-exact numerics, and
/// [`nominal_sizeof`] explicitly undercounts padded products). Scalars at
/// width; a product lays out its surviving components by C struct rules —
/// each component at its own alignment, the struct tail-padded to its widest
/// component's alignment — except residual-1, which lowers to the **bare**
/// surviving component (no wrapper, no padding). Erased ⇒ `None`. An array
/// ty contributes a handle field (pointer width) — reachable only as a
/// product component; buffer capacities go through [`buffer_bytes_of`] (flat
/// base × flat count). The emitted `static_assert`s (module.rs) pin these
/// sizes against nvcc's `sizeof` on the box leg.
pub(crate) fn abi_sizeof(ty: &Ty) -> Option<u64> {
    match ty {
        Ty::Int { bits, .. } | Ty::Float { bits } => Some(*bits as u64 / 8),
        Ty::Bool => Some(1),
        Ty::Unit | Ty::Str | Ty::IoToken => None,
        Ty::Array { .. } => Some(8), // a handle field
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let mut comps: Vec<&Ty> = Vec::new();
            let mut k = 0u32;
            while let Some(c) = ty.component_ty(k) {
                if lower_ty(c).is_some() {
                    comps.push(c);
                }
                k += 1;
            }
            match comps.as_slice() {
                [] => None,
                [only] => abi_sizeof(only), // residual-1: the bare component
                _ => {
                    let mut off = 0u64;
                    let mut max_align = 1u64;
                    for c in comps {
                        let a = abi_alignof(c)?;
                        off = align_up(off, a) + abi_sizeof(c)?;
                        max_align = max_align.max(a);
                    }
                    Some(align_up(off, max_align)) // tail padding
                }
            }
        }
    }
}

/// The C-layout alignment of `ty`'s lowered form: scalars at width, a handle
/// at pointer width, a product at its widest surviving component's alignment
/// (residual-1: the bare component's). Erased ⇒ `None`.
fn abi_alignof(ty: &Ty) -> Option<u64> {
    match ty {
        Ty::Int { bits, .. } | Ty::Float { bits } => Some(*bits as u64 / 8),
        Ty::Bool => Some(1),
        Ty::Unit | Ty::Str | Ty::IoToken => None,
        Ty::Array { .. } => Some(8),
        Ty::Tuple(_) | Ty::Struct { .. } => {
            let mut max_align: Option<u64> = None;
            let mut k = 0u32;
            while let Some(c) = ty.component_ty(k) {
                if lower_ty(c).is_some() {
                    let a = abi_alignof(c)?;
                    max_align = Some(max_align.map_or(a, |m: u64| m.max(a)));
                }
                k += 1;
            }
            max_align
        }
    }
}

/// The F7 budget check: over-budget ⇒ the documented `Unsupported` cell
/// with the measured size in the feature string.
pub(crate) fn check_local_array_budget(ty: &Ty, loc: SourceLoc) -> Result<(), EmitError> {
    if let Some(bytes) = local_array_bytes(ty)
        && bytes > MAX_LOCAL_ARRAY_BYTES
    {
        return Err(EmitError::Unsupported {
            feature: format!(
                "per-thread local array over {MAX_LOCAL_ARRAY_BYTES} bytes ({bytes} bytes)"
            ),
            loc,
        });
    }
    Ok(())
}

/// The launch-form side of the F7 budget: a launch-form `Fold` with an
/// array-typed acc copies it into a per-thread local in the single-thread
/// kernel (`{ct} acc[{m}];`, BC4) — check every fold site. (The inline
/// form's array acc lives in the twin's produced local — checked there.)
pub(crate) fn check_fold_acc_budgets(ir: &CategoryIr) -> Result<(), EmitError> {
    for (f, _) in ir.funcs() {
        for site in collect_sites(ir, f) {
            let morph = ir.morphism(site.m).expect("morphism resolves");
            let Operation::Fold { captures, .. } = morph.op else {
                continue;
            };
            let src_ty = &ir.object(morph.source).expect("source resolves").ty;
            // ADR-0027: the acc is the source product's component k.
            let acc_ty = src_ty.component_ty(captures).expect("fold acc");
            if matches!(acc_ty, Ty::Array { .. }) {
                check_local_array_budget(acc_ty, morph.loc)?;
            }
        }
    }
    Ok(())
}

/// The lowered C++ type of `ty`'s flat base element.
pub(crate) fn flat_base_ct(ty: &Ty) -> Option<String> {
    lower_ty(flat_base(ty))
}

/// `(elem_ty, size)` of an `Array` ty (cloned — the llvm `array_parts`).
pub(crate) fn array_parts(ty: &Ty) -> (Ty, u64) {
    match ty {
        Ty::Array { elem, size } => ((**elem).clone(), *size),
        _ => unreachable!("expected an array ty"),
    }
}

/// The cudaMalloc byte-count text for a device buffer holding `ty` (an array
/// ty): `sizeof({base}) * {flat}ULL`.
pub(crate) fn buffer_bytes(ty: &Ty) -> String {
    format!(
        "sizeof({}) * {}ULL",
        flat_base_ct(ty).expect("array base lowers"),
        flat_count(ty)
    )
}

/// The arena-capacity measure of a device buffer holding `ty` (an array ty):
/// the ABI-exact flat base size × the flat count — the numeric twin of
/// [`buffer_bytes`]'s text (`sizeof(base) * flat`, pinned equal by the
/// emitted `static_assert`s on the box leg).
pub(crate) fn buffer_bytes_of(ty: &Ty) -> Option<u64> {
    Some(abi_sizeof(flat_base(ty))? * flat_count(ty))
}

/// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`.
/// Whether `e` needs no parentheses when nested into a larger expression:
/// an identifier / literal / member path (`in.f3`, `o7`), or one already
/// wrapped by a single balanced outer `(...)` pair (WP-B/WP-C inlining).
pub(crate) fn is_atomic_expr(e: &str) -> bool {
    if !e.is_empty()
        && e.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return true;
    }
    if e.starts_with('(') && e.ends_with(')') {
        let mut depth = 0i32;
        for (i, c) in e.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return i == e.len() - 1;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

pub(crate) fn pair_source(ir: &CategoryIr, agg: ObjectId, k: u32) -> Option<ObjectId> {
    for &m in ir.in_edges(agg) {
        let morph = ir.morphism(m).expect("morphism resolves");
        if let Operation::Pair { slot, .. } = morph.op
            && slot == k
        {
            return Some(morph.source);
        }
    }
    None
}

/// The element-load expression for index `i` of a flat buffer `base` whose
/// element ty is `elem`: `base[i]` for scalars/products (AoS); the sub-array
/// pointer `base + (i * m)` for array elements (the flat-nested seam, §5).
fn elem_expr(base: &str, i: &str, elem: &Ty) -> String {
    if matches!(elem, Ty::Array { .. }) {
        format!("({base} + ({i} * {}ULL))", flat_count(elem))
    } else {
        format!("{base}[{i}]")
    }
}

/// The width-rule extension (§3): `(int64_t){expr}` — value-preserving for
/// both signed and unsigned operands, realizing llvm's zext/sext split.
pub(crate) fn extend_index(expr: &str) -> String {
    format!("(int64_t){expr}")
}

/// The signed two-sided bounds guard (§3): `idx < 0 || idx >= (int64_t)n`.
fn bounds_cond(idx64: &str, n: u64) -> String {
    format!("{idx64} < 0 || {idx64} >= (int64_t){n}")
}

/// The grid expression for an `n`-element elementwise kernel (BC3, 64-bit).
pub(crate) fn grid_expr(n: u64) -> String {
    format!("(unsigned int)(({n}ULL + 255ULL) / 256ULL)")
}

/// The number of `Pair` in-edges per array-typed object of `f` — the
/// pack_array literal sites. The construction is emitted at the object's
/// **last** Pair edge in `topo_order`: every element's producer precedes its
/// Pair edge, and every use of the literal follows all of them (topo
/// guarantee), so the last edge is the one point where all elements are
/// materialized and no use has happened.
pub(crate) fn literal_pair_counts(ir: &CategoryIr, f: FuncId) -> SecondaryMap<ObjectId, usize> {
    let mut out: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
    let fd = ir.func(f).expect("func resolves");
    for &m in &fd.morphisms {
        let morph = ir.morphism(m).expect("morphism resolves");
        if let Operation::Pair { .. } = morph.op {
            let tty = &ir.object(morph.target).expect("target resolves").ty;
            if matches!(tty, Ty::Array { .. }) {
                let cur = out.get(morph.target).copied().unwrap_or(0);
                out.insert(morph.target, cur + 1);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Kernel emission
// ---------------------------------------------------------------------------

/// The body-input argument assembly for a map/fold body call (ADR-0027 —
/// shared by the launch form's kernels and the inline form's per-thread
/// loops): the `(c₁…cₖ, original…)` product under the residual-erasure
/// remap, the captures as the leading fields. `parts` is one optional
/// expression per body-input component (`None` = an erased component, which
/// has no field to store). Returns the declaration/store lines (the caller
/// emits them at its indent) plus the argument expression: the assembled
/// struct for residual ≥ 2, the bare surviving component for residual-1,
/// `None` for residual-0.
fn assemble_body_arg(pair_ty: &Ty, parts: &[Option<String>]) -> (Vec<String>, Option<String>) {
    if residual_arity(pair_ty) >= 2 {
        let pct = lower_ty(pair_ty).expect("body input lowers");
        let mut lines = vec![format!("{pct} pair;")];
        for (slot, part) in parts.iter().enumerate() {
            if let (Some(eidx), Some(expr)) = (erased_index(pair_ty, slot as u32), part) {
                lines.push(format!("pair.f{eidx} = {expr};"));
            }
        }
        (lines, Some("pair".to_string()))
    } else {
        // Residual ≤ 1: the bare survivor (if any) is the whole argument.
        (Vec::new(), parts.iter().flatten().next().cloned())
    }
}

/// One `__global__` definition for `site` (deterministic text, no host slot
/// names — parameters are formal; the host launch in func.rs maps actuals
/// positionally onto the same list). A trap-FREE site (#14, `caps.site`)
/// drops the `unsigned int* trap` parameter — the launch then passes no
/// `d_trap` and skips the readback (func.rs's `launch_and_check`).
pub(crate) fn emit_kernel(
    ir: &CategoryIr,
    site: &Site,
    fnames: &SecondaryMap<FuncId, String>,
    quals: &Qualifiers,
    caps: &TrapCaps,
) -> String {
    let morph = ir.morphism(site.m).expect("morphism resolves");
    let k = Kernel {
        ir,
        name: &site.kname,
        fnames,
        quals,
        caps,
        capable: caps.site(ir, site.m),
        out: String::new(),
    };
    match morph.op {
        Operation::Map { body, captures } => {
            k.map_kernel(morph.source, morph.target, body, captures)
        }
        Operation::Zip => k.zip_kernel(morph.source, morph.target),
        Operation::Enumerate => k.enumerate_kernel(morph.source, morph.target),
        Operation::Iota => k.iota_kernel(morph.target),
        Operation::Fill => k.fill_kernel(morph.target),
        Operation::Update => k.update_kernel(morph.source, morph.target),
        Operation::Index => k.index_kernel(morph.source, morph.target),
        Operation::Fold { body, captures } => {
            k.fold_kernel(morph.source, morph.target, body, captures)
        }
        _ => unreachable!("not a bulk-op site"),
    }
}

/// The module's deduplicated kernel table (suggestions.md #17 — "kernel
/// shape dedup"): each structurally unique kernel is emitted ONCE and
/// launched per site. The shape key is the kernel's emitted text minus its
/// name — the op kind, parameter/element types, `n`/flat counts, and the
/// body fn's device name (where applicable) all ride the text, so text
/// equality IS structural equality. Deterministic (L2): fns iterate in
/// `ir.funcs()` order, sites in `collect_sites` order; the first site of a
/// shape lends its `k{f_ord}_{site_ord}` name to the surviving definition,
/// and the dedup map is only ever looked up, never iterated.
pub(crate) struct KernelSet {
    /// The deduplicated `__global__` definitions, in first-appearance order.
    pub text: String,
    /// fn → (site morphism → the launch's kernel name — the shape's
    /// survivor, not necessarily the site's own name).
    pub names: SecondaryMap<FuncId, HashMap<MorphismId, String>>,
}

/// Emit every fn's launch-form bulk-op sites as one deduplicated kernel
/// table (#17). [`FnEmit`](crate::func::FnEmit) consumes `names` so each
/// site's launch names the surviving definition. Sites of a dead host twin
/// (#12) contribute nothing: the only launcher of those kernels is the
/// fn's skipped host definition.
pub(crate) fn emit_kernel_set(
    ir: &CategoryIr,
    fnames: &SecondaryMap<FuncId, String>,
    quals: &Qualifiers,
    caps: &TrapCaps,
    live: &SecondaryMap<FuncId, bool>,
) -> KernelSet {
    let mut text = String::new();
    let mut names: SecondaryMap<FuncId, HashMap<MorphismId, String>> = SecondaryMap::new();
    // Shape key (definition text minus the name) → the surviving kernel name.
    let mut survivors: HashMap<String, String> = HashMap::new();
    for (f, _) in ir.funcs() {
        if dead_host_twin(quals, live, f) {
            continue; // #12: no host definition ⇒ no launches ⇒ no kernels
        }
        for site in collect_sites(ir, f) {
            let emitted = emit_kernel(ir, &site, fnames, quals, caps);
            // The name appears only in the `__global__ void {name}(` header —
            // kernel bodies name only body fns and `trap`, never the kernel.
            let key = &emitted[emitted.find('(').expect("kernel header")..];
            let kname = match survivors.get(key) {
                Some(survivor) => survivor.clone(),
                None => {
                    survivors.insert(key.to_string(), site.kname.clone());
                    text.push_str(&emitted);
                    site.kname.clone()
                }
            };
            let mut per_fn = names.get(f).cloned().unwrap_or_default();
            per_fn.insert(site.m, kname);
            names.insert(f, per_fn);
        }
    }
    KernelSet { text, names }
}

struct Kernel<'a> {
    ir: &'a CategoryIr,
    name: &'a str,
    fnames: &'a SecondaryMap<FuncId, String>,
    quals: &'a Qualifiers,
    caps: &'a TrapCaps,
    /// The site's own trap capability (#14): a trap-free kernel carries no
    /// `unsigned int* trap` parameter.
    capable: bool,
    out: String,
}

impl<'a> Kernel<'a> {
    fn obj_ty(&self, o: ObjectId) -> Ty {
        self.ir.object(o).expect("object resolves").ty.clone()
    }

    fn line(&mut self, indent: usize, s: impl AsRef<str>) {
        for _ in 0..indent {
            self.out.push_str("  ");
        }
        self.out.push_str(s.as_ref());
        self.out.push('\n');
    }

    /// `__global__ void k…(params) {` + the 64-bit thread-index prologue
    /// where `threaded` (elementwise kernels), else just the header.
    fn header(&mut self, params: &[String], threaded: bool) {
        self.out.push_str(&format!(
            "__global__ void {}({}) {{\n",
            self.name,
            params.join(", ")
        ));
        if threaded {
            self.line(
                1,
                "unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;",
            );
        }
    }

    /// The device call to body fn `g`: name + `trap` threading — the trap
    /// argument rides only when the callee can trap (#14); a trap-free
    /// callee takes no trap parameter (twin_signature/fn_signature).
    fn body_call(&self, g: FuncId, arg: Option<String>, dest: Option<String>) -> String {
        let name = self.quals.device_name(self.fnames, g);
        let mut args: Vec<String> = Vec::new();
        if let Some(a) = arg {
            args.push(a);
        }
        if let Some(d) = dest {
            args.push(d);
        }
        if self.caps.get(g) {
            args.push("trap".into());
        }
        format!("{name}({})", args.join(", "))
    }

    /// The trailing trap parameter, pushed iff this site is trap-capable
    /// (#14 — a trap-free kernel's launch passes no `d_trap`).
    fn push_trap_param(&self, params: &mut Vec<String>) {
        if self.capable {
            params.push("unsigned int* trap".into());
        }
    }

    /// The elementwise prologue shared by Map/Zip/Enumerate: kernel params
    /// `(out?, in…, trap)` — erased operands are omitted (erased-element
    /// arrays have no representation).
    fn elementwise_header(&mut self, params: &[String], n: u64) {
        self.header(params, true);
        self.line(1, format!("if (i < {n}ULL) {{"));
    }

    fn map_kernel(
        mut self,
        source: ObjectId,
        target: ObjectId,
        body: FuncId,
        captures: u32,
    ) -> String {
        let src_ty = self.obj_ty(source);
        // ADR-0027: k>0 ⇒ the source is the product `(c₁…cₖ, [T; n])` — the
        // mapped array is its last component; k=0 ⇒ the bare array (every
        // pre-existing program).
        let arr_ty = if captures == 0 {
            src_ty.clone()
        } else {
            src_ty
                .component_ty(captures)
                .cloned()
                .expect("map array component")
        };
        let (elem, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        let (uelem, _) = array_parts(&tgt_ty);
        let arr_ct = lower_ty(&arr_ty);
        let tgt_ct = lower_ty(&tgt_ty);

        let mut params: Vec<String> = Vec::new();
        if let Some(ct) = &tgt_ct {
            params.push(format!("{ct} out"));
        }
        if let Some(ct) = &arr_ct {
            params.push(format!("{ct} in"));
        }
        // ADR-0027: the captured buffers/scalars ride as extra kernel
        // parameters, positionally after the existing operands, before the
        // trap pointer. An array capture is a plain handle (`{base}* cap{j}`);
        // a scalar capture is a by-value parameter — both are per-thread
        // reads of the same device storage.
        let mut cap_args: Vec<Option<String>> = Vec::new();
        for j in 0..captures {
            let cap_ty = src_ty.component_ty(j).cloned().expect("map capture");
            if let Some(ct) = lower_ty(&cap_ty) {
                params.push(format!("{ct} cap{j}"));
                cap_args.push(Some(format!("cap{j}")));
            } else {
                cap_args.push(None);
            }
        }
        self.push_trap_param(&mut params);
        self.elementwise_header(&params, n);

        // The body argument: k=0 ⇒ the bare element value (or nothing, if
        // the element is erased); k>0 ⇒ the `(c₁…cₖ, elem)` body-input
        // product assembled with the captures as leading fields. The store:
        // the out element, or nothing. Array-typed body results ride the
        // twin out-param convention (the body fn is necessarily a Twin then
        // — array return).
        let elem_arg = if lower_ty(&elem).is_some() {
            Some(elem_expr("in", "i", &elem))
        } else {
            None
        };
        let arg = if captures == 0 {
            elem_arg
        } else {
            let pair_ty = self.obj_ty(self.ir.func(body).expect("body resolves").input);
            let mut parts = cap_args.clone();
            parts.push(elem_arg);
            let (decl, arg) = assemble_body_arg(&pair_ty, &parts);
            for l in decl {
                self.line(2, l);
            }
            arg
        };
        if lower_ty(&uelem).is_none() {
            let call = self.body_call(body, arg, None);
            self.line(2, format!("{call};"));
        } else if matches!(uelem, Ty::Array { .. }) {
            let dest = format!("(out + (i * {}ULL))", flat_count(&uelem));
            let call = self.body_call(body, arg, Some(dest));
            self.line(2, format!("{call};"));
        } else {
            let call = self.body_call(body, arg, None);
            self.line(2, format!("out[i] = {call};"));
        }
        self.line(1, "}");
        self.out.push_str("}\n");
        self.out
    }

    fn zip_kernel(mut self, source: ObjectId, target: ObjectId) -> String {
        let src_ty = self.obj_ty(source);
        let a_ty = src_ty.component_ty(0).cloned().expect("zip a");
        let b_ty = src_ty.component_ty(1).cloned().expect("zip b");
        let (a_elem, n) = array_parts(&a_ty);
        let (b_elem, _) = array_parts(&b_ty);
        let tgt_ty = self.obj_ty(target);
        let (elem, _) = array_parts(&tgt_ty); // the (A, B) product

        let mut params: Vec<String> = Vec::new();
        if let Some(ct) = lower_ty(&tgt_ty) {
            params.push(format!("{ct} out"));
        }
        if let Some(ct) = lower_ty(&a_ty) {
            params.push(format!("{ct} a"));
        }
        if let Some(ct) = lower_ty(&b_ty) {
            params.push(format!("{ct} b"));
        }
        self.push_trap_param(&mut params);
        self.elementwise_header(&params, n);

        // Per surviving component k of the element product: store the source
        // element (or sub-array pointer) into the erased-index field.
        self.zip_store(&elem, 0, &a_elem, "a");
        self.zip_store(&elem, 1, &b_elem, "b");
        self.line(1, "}");
        self.out.push_str("}\n");
        self.out
    }

    /// One component store of the Zip elementwise body; no-op when the
    /// component is erased.
    fn zip_store(&mut self, elem: &Ty, k: u32, comp_elem: &Ty, arr: &str) {
        let Some(eidx) = erased_index(elem, k) else {
            return;
        };
        let lvalue = if residual_arity(elem) == 1 {
            "out[i]".to_string()
        } else {
            format!("out[i].f{eidx}")
        };
        let rval = elem_expr(arr, "i", comp_elem);
        self.line(2, format!("{lvalue} = {rval};"));
    }

    fn enumerate_kernel(mut self, source: ObjectId, target: ObjectId) -> String {
        let src_ty = self.obj_ty(source);
        let (a_elem, n) = array_parts(&src_ty);
        let tgt_ty = self.obj_ty(target);
        let (elem, _) = array_parts(&tgt_ty); // the (i32, A) product

        let mut params: Vec<String> = Vec::new();
        if let Some(ct) = lower_ty(&tgt_ty) {
            params.push(format!("{ct} out"));
        }
        if let Some(ct) = lower_ty(&src_ty) {
            params.push(format!("{ct} a"));
        }
        self.push_trap_param(&mut params);
        self.elementwise_header(&params, n);

        // The i32 index IS the thread index (cast, no transfer — §1).
        let bare = residual_arity(&elem) == 1;
        if let Some(eidx) = erased_index(&elem, 0) {
            let lvalue = if bare {
                "out[i]".to_string()
            } else {
                format!("out[i].f{eidx}")
            };
            self.line(2, format!("{lvalue} = (int32_t)i;"));
        }
        if let Some(eidx) = erased_index(&elem, 1) {
            let lvalue = if bare {
                "out[i]".to_string()
            } else {
                format!("out[i].f{eidx}")
            };
            let rval = elem_expr("a", "i", &a_elem);
            self.line(2, format!("{lvalue} = {rval};"));
        }
        self.line(1, "}");
        self.out.push_str("}\n");
        self.out
    }

    fn iota_kernel(mut self, target: ObjectId) -> String {
        let tgt_ty = self.obj_ty(target);
        // Elem ctype derived, not hardcoded: the builder pins `[i32; n]`
        // today, but the ADR-0029 `[i64; n]` annotation form must not
        // silently truncate here if it ever lands.
        let (elem, _) = array_parts(&tgt_ty);
        let elem_ct = lower_ty(&elem).expect("iota elem lowers");
        let mut params = vec![format!(
            "{} out",
            lower_ty(&tgt_ty).expect("iota target lowers")
        )];
        params.push("long long n".into());
        self.header(&params, false);
        self.line(
            1,
            "long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;",
        );
        self.line(1, format!("if (i < n) out[i] = ({elem_ct})i;"));
        self.out.push_str("}\n");
        self.out
    }

    fn fill_kernel(mut self, target: ObjectId) -> String {
        let tgt_ty = self.obj_ty(target);
        let (elem, _) = array_parts(&tgt_ty);
        let mut params = Vec::new();
        if let Some(ct) = lower_ty(&tgt_ty) {
            params.push(format!("{ct} out"));
        }
        params.push("long long n".into());
        if let Some(ct) = lower_ty(&elem) {
            params.push(format!("{ct} x"));
        }
        self.header(&params, false);
        self.line(
            1,
            "long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;",
        );
        if lower_ty(&elem).is_some() {
            self.line(1, "if (i < n) out[i] = x;");
        }
        self.out.push_str("}\n");
        self.out
    }

    fn update_kernel(mut self, source: ObjectId, _target: ObjectId) -> String {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
        let val_ty = src_ty.component_ty(2).cloned().expect("update val");
        let (elem, n) = array_parts(&arr_ty);
        let flat = flat_count(&arr_ty);

        // BC5: full-copy kernel + bounds guard (index_oob ⇒ store 2u, the
        // kind+1 encoding; width rule §3). The guard runs in every thread
        // (uniform, benign same-value stores); the index arrives as int64_t
        // (extended at the launch, §3).
        let mut params: Vec<String> = Vec::new();
        let arr_ct = lower_ty(&arr_ty);
        if arr_ct.is_some() {
            params.push(format!("{}* out", flat_base_ct(&arr_ty).unwrap()));
            params.push(format!("{}* src", flat_base_ct(&arr_ty).unwrap()));
        }
        params.push("int64_t idx".into());
        if lower_ty(&val_ty).is_some() {
            let vct = flat_base_ct(&val_ty).expect("update val lowers");
            if matches!(val_ty, Ty::Array { .. }) {
                params.push(format!("{vct}* val"));
            } else {
                params.push(format!("{vct} val"));
            }
        }
        self.push_trap_param(&mut params);

        self.header(&params, false);
        let cond = bounds_cond("idx", n);
        self.line(1, format!("if ({cond}) {{ *trap = 2u; return; }}"));
        if arr_ct.is_some() {
            self.line(
                1,
                "unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;",
            );
            self.line(1, format!("if (i < {flat}ULL) {{"));
            if matches!(elem, Ty::Array { .. }) {
                // Nested: slot i/m of the outer is replaced by the whole
                // sub-array `val` (flat copy, m cells per outer element).
                let m = flat_count(&elem);
                self.line(
                    2,
                    format!("out[i] = ((int64_t)(i / {m}ULL) == idx) ? val[i % {m}ULL] : src[i];"),
                );
            } else {
                self.line(2, "out[i] = ((int64_t)i == idx) ? val : src[i];");
            }
            self.line(1, "}");
        }
        self.out.push_str("}\n");
        self.out
    }

    fn index_kernel(mut self, source: ObjectId, target: ObjectId) -> String {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("index array");
        let (elem, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);

        // 1-launch kernel (host launches <<<1, 1>>>): guard → write the
        // element into a device result buffer. Scalar elements land in a
        // 1-cell buffer the host memcpy's D→H; array elements copy into a
        // fresh device buffer (in-kernel device-to-device, §1 op table).
        let mut params: Vec<String> = Vec::new();
        let tgt_ct = lower_ty(&tgt_ty);
        if tgt_ct.is_some() {
            // The result is always a device buffer: 1 cell for scalar
            // elements, flat m cells for array elements (§1 op table).
            params.push(format!("{}* result", flat_base_ct(&tgt_ty).unwrap()));
        }
        if let Some(ct) = lower_ty(&arr_ty) {
            params.push(format!("{ct} arr"));
        }
        params.push("int64_t idx".into());
        self.push_trap_param(&mut params);

        self.header(&params, false);
        // The §3 bounds guard rides only a trap-CAPABLE site (#14/S20): a
        // proven-in-bounds Index is trap-free — no flag to set, dead text.
        if self.capable {
            let cond = bounds_cond("idx", n);
            self.line(1, format!("if ({cond}) {{ *trap = 2u; return; }}"));
        }
        if tgt_ct.is_some() {
            if matches!(elem, Ty::Array { .. }) {
                let m = flat_count(&elem);
                self.line(
                    1,
                    format!("for (unsigned long long j = 0; j < {m}ULL; j++) {{"),
                );
                self.line(
                    2,
                    format!("result[j] = arr[((unsigned long long)idx * {m}ULL) + j];"),
                );
                self.line(1, "}");
            } else {
                self.line(1, "*result = arr[(unsigned long long)idx];");
            }
        }
        self.out.push_str("}\n");
        self.out
    }

    fn fold_kernel(
        mut self,
        source: ObjectId,
        target: ObjectId,
        body: FuncId,
        captures: u32,
    ) -> String {
        let src_ty = self.obj_ty(source);
        // ADR-0027: the source product is `(c₁…cₖ, Acc, [T; n])` — the
        // captures shift the acc to component k and the array to k+1 (k=0:
        // today's `(Acc, [T; n])`).
        let acc_ty = src_ty.component_ty(captures).cloned().expect("fold acc");
        let arr_ty = src_ty
            .component_ty(captures + 1)
            .cloned()
            .expect("fold array");
        let (elem, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        // The body input product `(c₁…cₖ, Acc, T)` — the oracle's per-step
        // assembly extended with the leading captures (ADR-0027).
        let pair_ty = self.obj_ty(self.ir.func(body).expect("body resolves").input);

        let acc_ct = lower_ty(&acc_ty);
        let arr_ct = lower_ty(&arr_ty);
        let mut params: Vec<String> = Vec::new();
        if lower_ty(&tgt_ty).is_some() {
            // Scalar acc ⇒ a 1-cell buffer; array acc ⇒ the flat result
            // buffer (stays on device, §1).
            params.push(format!("{}* result", flat_base_ct(&tgt_ty).unwrap()));
        }
        match &acc_ty {
            Ty::Array { .. } => {
                // Erased-element acc (Array{Unit}): no representation — the
                // acc0 parameter is omitted; the fold loop still runs (F6,
                // launch-form degradation). (`acc_ct` of an array IS the
                // `{base}*` pointer text.)
                if let Some(ct) = &acc_ct {
                    params.push(format!("{ct} acc0"));
                }
            }
            _ => {
                if let Some(ct) = &acc_ct {
                    params.push(format!("{ct} acc0"));
                }
            }
        }
        if arr_ct.is_some() {
            params.push(format!("{}* arr", flat_base_ct(&arr_ty).unwrap()));
        }
        // ADR-0027: the captured buffers/scalars as extra kernel parameters,
        // positionally after the existing operands, before the trap pointer.
        let mut cap_args: Vec<Option<String>> = Vec::new();
        for j in 0..captures {
            let cap_ty = src_ty.component_ty(j).cloned().expect("fold capture");
            if let Some(ct) = lower_ty(&cap_ty) {
                params.push(format!("{ct} cap{j}"));
                cap_args.push(Some(format!("cap{j}")));
            } else {
                cap_args.push(None);
            }
        }
        self.push_trap_param(&mut params);

        // BC4: the single-thread kernel — the oracle's left-sequential loop
        // over device memory, acc in per-thread local storage (§5). An
        // erased-element acc has no storage anywhere (F6): no acc local, no
        // writeback — the loop still calls the body per element.
        self.header(&params, false);
        let array_acc = matches!(acc_ty, Ty::Array { .. }) && acc_ct.is_some();
        if array_acc {
            let m = flat_count(&acc_ty);
            let ct = flat_base_ct(&acc_ty).expect("array acc base lowers");
            self.line(1, format!("{ct} acc[{m}];"));
            self.line(
                1,
                format!("for (unsigned long long j = 0; j < {m}ULL; j++) {{"),
            );
            self.line(2, "acc[j] = acc0[j];");
            self.line(1, "}");
        } else if let Some(ct) = &acc_ct {
            self.line(1, format!("{ct} acc = acc0;"));
        }
        self.line(
            1,
            format!("for (unsigned long long i = 0; i < {n}ULL; i++) {{"),
        );
        // The per-step argument: the captures (leading fields), then the
        // local scalar / per-thread local array acc (decays to the pointer
        // field for an array acc), then the element — the body input's
        // `(c₁…cₖ, Acc, T)` shape under the residual-erasure remap.
        let acc_expr = if lower_ty(&acc_ty).is_some() {
            Some("acc".to_string())
        } else {
            None
        };
        let elem_arg = if lower_ty(&elem).is_some() {
            Some(elem_expr("arr", "i", &elem))
        } else {
            None
        };
        let mut parts = cap_args.clone();
        parts.push(acc_expr);
        parts.push(elem_arg);
        let (decl, arg) = assemble_body_arg(&pair_ty, &parts);
        for l in decl {
            self.line(2, l);
        }
        let dest = if array_acc {
            Some("acc".to_string())
        } else {
            None
        };
        let call = self.body_call(body, arg, dest);
        if array_acc {
            self.line(2, format!("{call};"));
        } else if acc_ct.is_some() {
            self.line(2, format!("acc = {call};"));
        } else {
            self.line(2, format!("{call};"));
        }
        // The per-step trap check (first-trap-wins, §3) — only when the body
        // can trap (#14); a trap-free body leaves the flag untouched.
        if self.caps.get(body) {
            self.line(2, "if (*trap) return;");
        }
        self.line(1, "}");
        if array_acc {
            let m = flat_count(&acc_ty);
            self.line(
                1,
                format!("for (unsigned long long j = 0; j < {m}ULL; j++) {{"),
            );
            self.line(2, "result[j] = acc[j];");
            self.line(1, "}");
        } else if lower_ty(&tgt_ty).is_some() {
            self.line(1, "*result = acc;");
        }
        self.out.push_str("}\n");
        self.out
    }
}

// ---------------------------------------------------------------------------
// Device-side signatures
// ---------------------------------------------------------------------------

/// The `__device__` twin's signature text (no body):
/// `static __device__ {ret} d_{name}({in}, unsigned int* trap)`, or
/// `static __device__ void d_{name}({in}, {T}* out, unsigned int* trap)` for
/// an array-typed return (the out-param convention — a twin's arrays are
/// per-thread locals and cannot be returned by pointer). The convention
/// applies only when the array LOWERS (F6): an erased array return
/// (Array{Unit}) has no buffer to write and lowers to plain `void`. A
/// trap-FREE twin (#14, `caps`) drops the trailing trap parameter — its
/// callers then pass no trap argument and emit no post-call check.
pub(crate) fn twin_signature(
    ir: &CategoryIr,
    f: FuncId,
    fnames: &SecondaryMap<FuncId, String>,
    caps: &TrapCaps,
) -> String {
    let fd = ir.func(f).expect("func resolves");
    let in_ty = &ir.object(fd.input).expect("input resolves").ty;
    let out_ty = &ir.object(fd.output).expect("output resolves").ty;
    let name = format!("d_{}", fnames[f]);
    let mut params: Vec<String> = Vec::new();
    if let Some(t) = lower_ty(in_ty) {
        params.push(format!("{t} in"));
    }
    if matches!(out_ty, Ty::Array { .. }) && lower_ty(out_ty).is_some() {
        params.push(format!("{}* out", flat_base_ct(out_ty).unwrap()));
        if caps.get(f) {
            params.push("unsigned int* trap".into());
        }
        return format!("static __device__ void {name}({})", params.join(", "));
    }
    if caps.get(f) {
        params.push("unsigned int* trap".into());
    }
    let ret = lower_ty(out_ty).unwrap_or_else(|| "void".into());
    format!("static __device__ {ret} {name}({})", params.join(", "))
}

// ---------------------------------------------------------------------------
// DevEmit — the inline form: `__device__` twins (§1, BC8 case ii)
// ---------------------------------------------------------------------------

/// The inline-form emitter: one `__device__` twin per [`FnQual::Twin`] fn —
/// the per-thread sequential op table (§1's right-hand column), which *is*
/// the oracle's evaluation order per thread. Scalar ops emit the same text
/// as the host table (BC2 unsigned-cast wrapping, BC7 strict Phi, `&`/`|`);
/// the divergences are exactly §1's in-body column: `Div`/`Mod`/`Index`/
/// `Update` guards set `*trap = kind` and return; `Map`/`Zip`/`Enumerate`/
/// `Fold` are per-thread sequential loops; array literals are per-thread
/// local initializers (§2 item 8); `Call` is a direct `__device__` call —
/// with the trap pointer threaded and a post-call trap check only when the
/// callee can trap (#14, [`TrapCaps`]; a trap-free fn drops the trailing
/// trap parameter from its twin signature, and its callers pass nothing).
///
/// **Array materialization on device.** An array value *produced* inside the
/// twin (a literal, a Map/Zip/Enumerate/Update/Index/Fold result, a call
/// result, or the fn's Return) is a per-thread local C array `{T} oK[{flat}]`
/// — statically bounded, `n` from the `Ty` (§5). Every other array-typed
/// object (parameters, `Proj`/`Phi` results, **loop merges and exit
/// objects**) is a plain `{T}*` handle; handle copies are pointer
/// assignments (shallow — safe because mutation is either absent or
/// plan-proven: `Update` copies by default, and an in-place `Update`
/// (plan-last-use §2 rule 4, [`in_place_update`]) writes only a buffer the
/// last-use plan proves dead — its old value unobservable). The one place a
/// local array is *assigned into* is elementwise; whole-array "moves" are
/// pointer assignments and happen only between handles. An in-placed twin
/// `Update` declares no produced local for its target: the per-thread copy
/// is skipped, the store lands in the source array, and the target's slot
/// aliases the source's from that point on.
///
/// **Loops in bodies (WP4, §1's inline-form cell).** A body fn may contain
/// a canonical loop (lower emits each body with fresh loop state); the twin
/// emits it as a per-thread sequential `while (true)` quartet under the
/// same `loop_plan` gate — decide cone → guard → advance cone → back edge,
/// with the same driver-ownership skip in the walk. A carried array's merge
/// is a handle (never a produced local), so the back edge is the host
/// driver's pointer swap verbatim; the one cell-wise copy is an exit
/// payload landing in a produced local array (the fn's array Return).
pub(crate) struct DevEmit<'a> {
    ir: &'a CategoryIr,
    f: FuncId,
    fnames: &'a SecondaryMap<FuncId, String>,
    quals: &'a Qualifiers,
    caps: &'a TrapCaps,
    slots: SecondaryMap<ObjectId, String>,
    /// Array-typed objects materialized as per-thread local C arrays (the
    /// produced set above); any other array object is a `{T}*` handle.
    produced: SecondaryMap<ObjectId, ()>,
    /// In-placed `Update` targets (plan-last-use §2 rule 4, computed in
    /// [`DevEmit::new`] from the fn's deduced `last_use_plan` — never
    /// re-derived): the source array dies at the update, so the per-thread
    /// copy is skipped and the store lands in the source's storage; no
    /// produced local is declared for the target (its slot aliases the
    /// source's at the site). Lookup-only (L2).
    in_place: SecondaryMap<ObjectId, ()>,
    /// Literal sites: total Pair in-edges per array object, and how many the
    /// walk has passed (the construction emits at the last one).
    lit_total: SecondaryMap<ObjectId, usize>,
    lit_seen: SecondaryMap<ObjectId, usize>,
    /// The fn's bounds-proof plan (the S20 deduced query, computed once at
    /// construction alongside `last_use` — the BL7 pattern): an
    /// `Operation::Index` it proves statically in-bounds can never fire the
    /// §3 bounds guard, so [`DevEmit::emit_index`] drops the dead guard text
    /// and emits the plain per-thread read (the extension temp stays).
    proof: BoundsProof,
    /// The minimal-emission classification (plan-minimal-emission WP-B):
    /// Named objects keep hoisted `o{ord}` locals + statement assignment;
    /// Inline objects live as memoized expression strings (`exprs`);
    /// Dissolved products never materialize — consumers read their field
    /// sources through `component_expr`.
    plan: EmissionPlan,
    /// Backend-forced Named overrides on top of the plan: a `Call` target
    /// must stay a statement so the §3 post-call `if (*trap)` check keeps
    /// its position (the query is backend-agnostic and cannot know the trap
    /// protocol; recorded in the plan doc as a WP-B as-built note).
    force_named: SecondaryMap<ObjectId, ()>,
    /// Memoized expressions for Inline/Dissolved-consumed values — one
    /// string per object, referenced exactly once (R-NODUP holds because
    /// `Inline ⇔ effective count = 1` in the query).
    exprs: SecondaryMap<ObjectId, String>,
    decls: String,
    body: String,
    next: u32,
    /// `return;` / `return {ret}{};` — the guard early-return for this fn.
    ret_default: String,
    /// Base indent added to every `line` level: 0 normally, 1 inside a
    /// loop's while body (the WP4 twin quartet).
    base: usize,
}

impl<'a> DevEmit<'a> {
    pub(crate) fn new(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        quals: &'a Qualifiers,
        caps: &'a TrapCaps,
    ) -> Self {
        let fd = ir.func(f).expect("func resolves");
        let out_ty = &ir.object(fd.output).expect("output resolves").ty;

        let mut produced: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for &m in &fd.morphisms {
            let morph = ir.morphism(m).expect("morphism resolves");
            let produces_array = match morph.op {
                Operation::Map { .. }
                | Operation::Zip
                | Operation::Enumerate
                | Operation::Iota
                | Operation::Fill
                | Operation::Update
                | Operation::Call(_) => {
                    matches!(
                        ir.object(morph.target).expect("target").ty,
                        Ty::Array { .. }
                    )
                }
                Operation::Index | Operation::Fold { .. } => {
                    matches!(
                        ir.object(morph.target).expect("target").ty,
                        Ty::Array { .. }
                    )
                }
                Operation::Pair { .. } => {
                    matches!(
                        ir.object(morph.target).expect("target").ty,
                        Ty::Array { .. }
                    )
                }
                _ => false,
            };
            if produces_array {
                produced.insert(morph.target, ());
            }
        }
        if matches!(out_ty, Ty::Array { .. }) {
            produced.insert(fd.output, ());
        }

        // In-place Update targets (plan-last-use §2 rule 4): the deduced
        // query answers once per site, at construction (the BL7 pattern —
        // deduced, never re-derived at the site).
        let last_use = ir.last_use_plan(f);
        let mut in_place: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for &m in &fd.morphisms {
            let morph = ir.morphism(m).expect("morphism resolves");
            if morph.op != Operation::Update {
                continue;
            }
            let src_ty = ir.object(morph.source).expect("source resolves").ty.clone();
            let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
            if lower_ty(&arr_ty).is_none() {
                continue; // erased element: the guard is the whole op
            }
            let src = pair_source(ir, morph.source, 0).expect("update src");
            if in_place_update(ir, &last_use, f, m, src) {
                in_place.insert(morph.target, ());
            }
        }

        let ret_default = match lower_ty(out_ty) {
            Some(ct) if !matches!(out_ty, Ty::Array { .. }) => format!("return {ct}{{}};"),
            _ => "return;".to_string(),
        };

        let plan = ir.emission_plan(f);
        let mut force_named: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for &m in &fd.morphisms {
            let morph = ir.morphism(m).expect("morphism resolves");
            if matches!(morph.op, Operation::Call(_)) {
                force_named.insert(morph.target, ());
            }
        }
        // Product-typed Inline (plan §2 as-built note 2): Phase 1 takes the
        // plan's sanctioned fallback — one local name — instead of a braced
        // aggregate literal (the Pair arm assigns its fields normally). The
        // exhibits are unaffected: their wrapper products are Dissolved.
        for (id, obj) in ir.objects() {
            if ir.try_owner(id) == Some(f)
                && obj.ty.product_arity().is_some()
                && plan.class(id).is_some_and(|c| c.is_inline())
            {
                force_named.insert(id, ());
            }
        }

        DevEmit {
            ir,
            f,
            fnames,
            quals,
            caps,
            slots: SecondaryMap::new(),
            produced,
            in_place,
            lit_total: literal_pair_counts(ir, f),
            lit_seen: SecondaryMap::new(),
            proof: ir.bounds_proof(f),
            plan,
            force_named,
            exprs: SecondaryMap::new(),
            decls: String::new(),
            body: String::new(),
            next: 0,
            ret_default,
            base: 0,
        }
    }

    /// The effective class: the deduced plan, overridden Named where the
    /// backend's statement protocol demands it (Call targets).
    fn cls(&self, o: ObjectId) -> EmissionClass {
        if self.force_named.contains_key(o) {
            return EmissionClass::Named;
        }
        self.plan.class(o).unwrap_or(EmissionClass::Named)
    }

    fn dissolved(&self, o: ObjectId) -> bool {
        self.cls(o).is_dissolved()
    }

    /// Inline OR Dissolved: no local, no declaration — the value lives as an
    /// expression (or, for a dissolved product, as its fields' expressions).
    fn expr_only(&self, o: ObjectId) -> bool {
        !self.cls(o).is_named()
    }

    fn fresh(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    fn tmp(&mut self) -> String {
        format!("t{}", self.fresh())
    }

    fn line(&mut self, indent: usize, s: impl AsRef<str>) {
        for _ in 0..(indent + self.base) {
            self.body.push_str("  ");
        }
        self.body.push_str(s.as_ref());
        self.body.push('\n');
    }

    fn slot(&self, o: ObjectId) -> Option<String> {
        self.slots.get(o).cloned()
    }

    fn obj_ty(&self, o: ObjectId) -> Ty {
        self.ir.object(o).expect("object resolves").ty.clone()
    }

    // --- operand materialization (host-table rules, func.rs) --------------

    /// The whole value of `o` as a C++ expression: constant literal,
    /// memoized Inline expression, or slot name (an array slot name decays
    /// to a pointer where needed).
    fn load_whole(&mut self, o: ObjectId) -> Option<(String, String)> {
        let obj = self.ir.object(o).expect("object resolves");
        if obj.kind == ObjectKind::Constant {
            return match &obj.value {
                Some(Value::Str(_)) | None => None,
                Some(v) => Some((lower_ty(&obj.ty)?, const_literal(v))),
            };
        }
        let ct = lower_ty(&obj.ty)?;
        if let Some(e) = self.exprs.get(o) {
            return Some((ct, e.clone()));
        }
        let slot = self.slot(o)?;
        Some((ct, slot))
    }

    /// Component `k` of aggregate `agg` under the residual-erasure remap
    /// (identical to the host rule). A DISSOLVED aggregate never
    /// materializes: the component resolves through the Pair edge to the
    /// field source's own expression (WP-B — the wrapper text disappears).
    fn component_expr(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let agg_ty = self.obj_ty(agg);
        match &agg_ty {
            Ty::Tuple(_) | Ty::Struct { .. } => {
                let comp_ty = agg_ty.component_ty(k)?.clone();
                let cct = lower_ty(&comp_ty)?;
                if self.dissolved(agg) {
                    let src = pair_source(self.ir, agg, k)?;
                    let (_, val) = self.load_whole(src)?;
                    return Some((cct, val));
                }
                let agg_slot = self.slot(agg)?;
                if residual_arity(&agg_ty) == 1 {
                    Some((cct, agg_slot))
                } else {
                    let eidx = erased_index(&agg_ty, k)?;
                    Some((cct, format!("{agg_slot}.f{eidx}")))
                }
            }
            _ => None,
        }
    }

    /// Route a produced value: a Named object takes its one statement
    /// assignment; an Inline object memoizes the (parenthesized) expression
    /// and emits nothing (WP-B).
    fn store_obj(&mut self, o: ObjectId, expr: &str) {
        if self.expr_only(o) {
            let wrapped = if is_atomic_expr(expr) {
                expr.to_string()
            } else {
                format!("({expr})")
            };
            self.exprs.insert(o, wrapped);
            return;
        }
        if let Some(slot) = self.slot(o) {
            self.line(1, format!("{slot} = {expr};"));
        }
    }

    // --- the walk ----------------------------------------------------------

    /// Emit the twin: declarations, prologue, the topo walk, epilogue.
    pub(crate) fn emit(mut self) -> Result<String, EmitError> {
        let fd = self.ir.func(self.f).expect("func resolves");
        let in_ty = self.obj_ty(fd.input);
        let out_ty = self.obj_ty(fd.output);

        // Hoisted declarations (`o{ord}`): scalars/products as values,
        // produced arrays as per-thread locals, other arrays as handles.
        let mut ord = 0u32;
        let owned: Vec<(ObjectId, ObjectKind, Ty)> = self
            .ir
            .objects()
            .filter(|(id, _)| self.ir.try_owner(*id) == Some(self.f))
            .map(|(id, obj)| (id, obj.kind, obj.ty.clone()))
            .collect();
        for (id, kind, ty) in &owned {
            if *kind == ObjectKind::Constant {
                continue;
            }
            if lower_ty(ty).is_none() {
                ord += 1;
                continue;
            }
            // R-ONENAME: the parameter IS a variable — the input object's
            // slot is the literal `in` (no `o0 = in;` copy, no declaration).
            if *id == fd.input {
                self.slots.insert(*id, "in".to_string());
                ord += 1;
                continue;
            }
            // WP-B: Inline/Dissolved values own no local (never arrays —
            // the query keeps arrays Named; the guard is belt-and-braces).
            if !matches!(ty, Ty::Array { .. }) && self.expr_only(*id) {
                ord += 1;
                continue;
            }
            let name = format!("o{ord}");
            if let Ty::Array { .. } = ty {
                if self.in_place.contains_key(*id) {
                    // In-placed Update target: no produced local — the site's
                    // store lands in the SOURCE array's storage and the
                    // target's slot aliases it (inserted at the site).
                    ord += 1;
                    continue;
                }
                if self.produced.contains_key(*id) {
                    // F7: the per-thread local-array budget (the documented
                    // cell — over-budget is Unsupported, never improvised).
                    check_local_array_budget(
                        ty,
                        self.ir.object(*id).expect("object resolves").loc,
                    )?;
                    let base = flat_base_ct(ty).expect("array base lowers");
                    self.decls
                        .push_str(&format!("  {base} {name}[{}];\n", flat_count(ty)));
                } else {
                    let ct = lower_ty(ty).expect("array lowers");
                    self.decls.push_str(&format!("  {ct} {name};\n"));
                }
            } else {
                let ct = lower_ty(ty).expect("lowers");
                self.decls.push_str(&format!("  {ct} {name};\n"));
            }
            self.slots.insert(*id, name);
            ord += 1;
        }

        // No prologue copy: the input object's slot IS `in` (R-ONENAME).
        let _ = in_ty;

        // Driver-owned morphisms: everything in a loop plan's decide/advance
        // cones, plus anything incident to an SCC object — DESIGN §1's
        // walk-skip paragraph, the llvm rule (its func.rs:252–290) carried
        // verbatim, computed once. Plan membership is the precise rule: an
        // exit-only payload chain leaves the SCC but still belongs to the
        // decide cone; SCC incidence alone would re-emit it after the loop.
        let mut in_scc: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for scc in self.ir.loop_structure(self.f) {
            for &mg in &scc.merges {
                if let Some(plan) = self.ir.loop_plan(self.f, mg) {
                    for &mo in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                        owned.insert(mo, ());
                    }
                }
            }
            for o in scc.objects {
                in_scc.insert(o, ());
            }
        }

        for m in self.ir.topo_order(self.f) {
            let morph = self.ir.morphism(m).expect("morphism resolves");
            match morph.op {
                Operation::LoopEnter => self.emit_loop(morph.target)?,
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue; // driver-owned
                    }
                    self.emit_morphism(m)?;
                }
            }
        }

        // Epilogue: scalar returns by value; an array Return is copied into
        // the caller's `out` buffer (the out-param convention).
        match lower_ty(&out_ty) {
            Some(_) if !matches!(out_ty, Ty::Array { .. }) => {
                let os = self.slot(fd.output).expect("non-void return has a slot");
                self.line(1, format!("return {os};"));
            }
            Some(_) => {
                let os = self.slot(fd.output).expect("array return has a local");
                self.line(
                    1,
                    format!(
                        "for (unsigned long long j = 0; j < {}ULL; j++) {{",
                        flat_count(&out_ty)
                    ),
                );
                self.line(2, format!("out[j] = {os}[j];"));
                self.line(1, "}");
                self.line(1, "return;");
            }
            None => self.line(1, "return;"),
        }

        let sig = twin_signature(self.ir, self.f, self.fnames, self.caps);
        Ok(format!("{sig} {{\n{}{}}}\n", self.decls, self.body))
    }

    /// A canonical loop in the inline form (DESIGN §1's `loops` row, right
    /// column): the host driver's guard-first quartet (loops.rs) as
    /// per-thread sequential device code — decide cone → guard → advance
    /// cone → back edge. The merge is always a handle (never a produced
    /// local array), so init/back-edge copies are pointer swaps for carried
    /// arrays, exactly as on the host; the one cell-wise copy is an exit
    /// payload landing in a produced local array (the fn's array Return —
    /// the epilogue copies it to the caller's `out`).
    fn emit_loop(&mut self, merge: ObjectId) -> Result<(), EmitError> {
        let plan = self
            .ir
            .loop_plan(self.f, merge)
            .expect("canonical loop (gated by emit's L3 capability check)");

        // Entry: init → merge local (pointer copy for a carried array).
        if let Some((_, val)) = self.load_whole(plan.init) {
            self.store_obj(plan.merge, &val);
        }

        self.line(1, "while (true) {");
        self.base += 1;
        // Decide/exit cone: guard cond + exit-route payload, every iteration.
        for &mo in &plan.decide_order {
            self.emit_morphism(mo)?;
        }
        let (_, cond) = self
            .component_expr(plan.exit_route, 1)
            .expect("loop guard cond");
        self.line(1, format!("if (!{cond}) {{ break; }}"));
        // Advance cone: the next-state, unreachable on the exit step.
        for &mo in &plan.advance_order {
            self.emit_morphism(mo)?;
        }
        // Back edge: one copy, sequenced last (the parallel assignment —
        // the route is a distinct local from the merge, so no temporaries).
        if let Some((_, val)) = self.component_expr(plan.back_route, 0) {
            self.store_obj(plan.merge, &val);
        }
        self.base -= 1;
        self.line(1, "}");

        // Exit: the payload materializes exactly once, here. A produced
        // (local-array) exit object takes the payload cell-wise; a handle
        // takes the pointer.
        for &ex in &plan.exits {
            let (route, tgt) = {
                let m = self.ir.morphism(ex).expect("exit morphism");
                (m.source, m.target)
            };
            if matches!(self.obj_ty(tgt), Ty::Array { .. }) && self.produced.contains_key(tgt) {
                if let Some((_, payload)) = self.component_expr(route, 0) {
                    let n = flat_count(&self.obj_ty(tgt));
                    let jv = self.tmp();
                    let tgt_slot = self.slot(tgt).expect("produced exit local");
                    self.line(
                        1,
                        format!("for (unsigned long long {jv} = 0; {jv} < {n}ULL; {jv}++) {{"),
                    );
                    self.line(2, format!("{tgt_slot}[{jv}] = {payload}[{jv}];"));
                    self.line(1, "}");
                }
            } else if let Some((_, val)) = self.component_expr(route, 0) {
                self.store_obj(tgt, &val);
            }
        }
        Ok(())
    }

    fn emit_morphism(&mut self, m: MorphismId) -> Result<(), EmitError> {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        let op = morph.op;
        let source = morph.source;
        let target = morph.target;

        match op {
            Operation::Pair { slot, .. } => {
                if matches!(self.obj_ty(target), Ty::Array { .. }) {
                    // Body-local array literal (§2 item 8): per-thread local
                    // initializer, emitted at the last Pair edge.
                    let seen = self.lit_seen.get(target).copied().unwrap_or(0) + 1;
                    self.lit_seen.insert(target, seen);
                    if seen == self.lit_total.get(target).copied().unwrap_or(0) {
                        self.emit_literal(target);
                    }
                } else if self.dissolved(target) {
                    // WP-B: a dissolved product never materializes — its
                    // consumers read the field sources through
                    // `component_expr`; the assembly text disappears.
                } else if let Some((_, sval)) = self.load_whole(source)
                    && let Some((_, lvalue)) = self.component_expr(target, slot)
                {
                    self.line(1, format!("{lvalue} = {sval};"));
                }
            }
            Operation::Proj { index } => {
                if let Some((_, val)) = self.component_expr(source, index) {
                    self.store_obj(target, &val);
                }
            }
            Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
                self.emit_arith(source, target, op);
            }
            Operation::Neg => {
                let (ct, val) = self.load_whole(source).expect("neg operand");
                if is_float(&self.obj_ty(source)) {
                    // `-({val})`: a folded negative constant would print
                    // `--1.5e0` — the decrement operator, ill-formed here.
                    self.store_obj(target, &format!("-({val})"));
                } else {
                    let uct = unsigned_twin(&self.obj_ty(source));
                    self.store_obj(target, &format!("({ct})(0 - ({uct}){val})"));
                }
            }
            Operation::Eq
            | Operation::Neq
            | Operation::Lt
            | Operation::Gt
            | Operation::Le
            | Operation::Ge => {
                self.emit_compare(source, target, op);
            }
            Operation::And | Operation::Or => {
                let (_, a) = self.component_expr(source, 0).expect("logic a");
                let (_, b) = self.component_expr(source, 1).expect("logic b");
                let sym = if op == Operation::And { "&" } else { "|" };
                self.store_obj(target, &format!("{a} {sym} {b}"));
            }
            Operation::Not => {
                let (_, val) = self.load_whole(source).expect("not operand");
                self.store_obj(target, &format!("!{val}"));
            }
            Operation::Widen => {
                let (_, val) = self.load_whole(source).expect("widen operand");
                let target_ct = lower_ty(&self.obj_ty(target)).expect("widen target");
                self.store_obj(target, &format!("({target_ct})({val})"));
            }
            Operation::Phi => {
                // BC7 strict select over precomputed temporaries (host rule).
                let (tct, t) = self.component_expr(source, 0).expect("phi then");
                let (_, e) = self.component_expr(source, 1).expect("phi else");
                let (_, c) = self.component_expr(source, 2).expect("phi cond");
                let tt = self.tmp();
                let te = self.tmp();
                self.line(1, format!("{tct} {tt} = {t};"));
                self.line(1, format!("{tct} {te} = {e};"));
                self.store_obj(target, &format!("{c} ? {tt} : {te}"));
            }
            Operation::Call(g) => self.emit_call(source, target, g),
            Operation::Iota => self.emit_iota(target),
            Operation::Fill => self.emit_fill(source, target),
            Operation::Map { body, captures } => self.emit_map(source, target, body, captures),
            Operation::Zip => self.emit_zip(source, target),
            Operation::Enumerate => self.emit_enumerate(source, target),
            Operation::Fold { body, captures } => self.emit_fold(source, target, body, captures),
            Operation::Index => self.emit_index(m, source, target),
            Operation::Update => self.emit_update(source, target),
            Operation::Print { .. } => {
                unreachable!("token-free twins contain no Print (E2)")
            }
            Operation::Output => self.emit_output(source, target),
            Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
                unreachable!("loop ops are driver-owned")
            }
        }
        Ok(())
    }

    fn emit_arith(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
        let opty = self
            .obj_ty(source)
            .component_ty(0)
            .cloned()
            .expect("arith ty");
        let (ct, a) = self.component_expr(source, 0).expect("arith a");
        let (_, b) = self.component_expr(source, 1).expect("arith b");

        if is_float(&opty) {
            let expr = match op {
                Operation::Add => format!("{a} + {b}"),
                Operation::Sub => format!("{a} - {b}"),
                Operation::Mul => format!("{a} * {b}"),
                Operation::Div => format!("{a} / {b}"),
                Operation::Mod if matches!(opty, Ty::Float { bits: 32 }) => {
                    format!("fmodf({a}, {b})")
                }
                Operation::Mod => format!("fmod({a}, {b})"),
                _ => unreachable!(),
            };
            self.store_obj(target, &expr);
            return;
        }

        let signed = matches!(opty, Ty::Int { signed: true, .. });
        match op {
            Operation::Add | Operation::Sub | Operation::Mul => {
                let sym = match op {
                    Operation::Add => "+",
                    Operation::Sub => "-",
                    Operation::Mul => "*",
                    _ => unreachable!(),
                };
                let uct = unsigned_twin(&opty);
                self.store_obj(target, &format!("({ct})(({uct}){a} {sym} ({uct}){b})"));
            }
            Operation::Div | Operation::Mod => {
                // #13: a literal non-zero constant divisor makes the zero
                // guard dead by construction (the oracle's behavior is
                // identical — the guard can never fire); a constant ≠ −1
                // makes the MIN/−1 value guard dead. A literal 0 keeps the
                // guard (it always fires, exactly as the oracle traps).
                let const_div = const_int_operand(self.ir, source, 1);
                if !matches!(const_div, Some(v) if v != 0) {
                    // §3: the device zero guard stores div_zero (kind+1 ⇒ 1u)
                    // and returns.
                    let ret = self.ret_default.clone();
                    self.line(1, format!("if ({b} == 0) {{ *trap = 1u; {ret} }}"));
                }
                if signed && !matches!(const_div, Some(v) if v != -1) {
                    // Guarded form: the target is Named by the query (a
                    // guarded producer), so the slot exists.
                    let slot = self.slot(target).expect("div/mod result slot");
                    let min = int_min(&ct);
                    let sym = if op == Operation::Div { "/" } else { "%" };
                    let ovval = if op == Operation::Div { min } else { "0" };
                    self.line(1, format!("if (({b} == -1) && ({a} == {min})) {{"));
                    self.line(2, format!("{slot} = {ovval};"));
                    self.line(1, "} else {");
                    self.line(2, format!("{slot} = {a} {sym} {b};"));
                    self.line(1, "}");
                } else {
                    let sym = if op == Operation::Div { "/" } else { "%" };
                    self.store_obj(target, &format!("{a} {sym} {b}"));
                }
            }
            _ => unreachable!(),
        }
    }

    fn emit_compare(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
        let (_, a) = self.component_expr(source, 0).expect("cmp a");
        let (_, b) = self.component_expr(source, 1).expect("cmp b");
        let sym = match op {
            Operation::Eq => "==",
            Operation::Neq => "!=",
            Operation::Lt => "<",
            Operation::Gt => ">",
            Operation::Le => "<=",
            Operation::Ge => ">=",
            _ => unreachable!(),
        };
        self.store_obj(target, &format!("{a} {sym} {b}"));
    }

    fn emit_call(&mut self, source: ObjectId, target: ObjectId, g: FuncId) {
        let out_ty = self.obj_ty(self.ir.func(g).expect("callee resolves").output);
        let arg = self.load_whole(source).map(|(_, val)| val);
        let name = self.quals.device_name(self.fnames, g);
        let mut args: Vec<String> = Vec::new();
        if let Some(a) = arg {
            args.push(a);
        }
        // Array return ⇒ the callee is a Twin (the qualifier rule) and the
        // result is written into the caller's local via the out-param. An
        // erased array return has no buffer (F6): the callee's device form
        // is plain void and the call stores nothing.
        let array_ret = matches!(out_ty, Ty::Array { .. }) && lower_ty(&out_ty).is_some();
        if array_ret {
            let dest = self.slot(target).expect("array call result local");
            args.push(dest);
        }
        // #14: the trap argument and the post-call check ride only when the
        // callee can trap — a trap-free callee leaves the flag untouched, so
        // both are dead by construction (first-trap-wins is preserved by the
        // checks kept after capable calls, in order).
        let capable = self.caps.get(g);
        if capable {
            args.push("trap".into());
        }
        let call = format!("{name}({})", args.join(", "));
        let ret = self.ret_default.clone();
        if array_ret {
            self.line(1, format!("{call};"));
        } else if lower_ty(&out_ty).is_some() {
            let slot = self.slot(target).expect("call result slot");
            self.line(1, format!("{slot} = {call};"));
        } else {
            self.line(1, format!("{call};"));
        }
        if capable {
            self.line(1, format!("if (*trap) {ret}"));
        }
    }

    /// A per-thread sequential Map loop (§1 inline form): the target local
    /// was declared in the prologue; the body call is trap-checked per
    /// element (the oracle's in-order first trap). ADR-0027: a capturing
    /// map's source is the `(c₁…cₖ, [T; n])` product — the captured
    /// components are per-thread reads of the same buffers/scalars, passed
    /// as the leading fields of the body-input argument.
    fn emit_map(&mut self, source: ObjectId, target: ObjectId, body: FuncId, captures: u32) {
        let arr_obj = if captures == 0 {
            source
        } else {
            pair_component(self.ir, source, captures)
        };
        let arr_ty = self.obj_ty(arr_obj);
        let (elem, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        let (uelem, _) = array_parts(&tgt_ty);
        let iv = self.tmp();

        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        // An erased source array (Array{Unit}) has no slot: the body
        // argument is omitted, and the per-thread loop still runs — the
        // launch form omits the kernel parameter the same way (F6). (The
        // residual expect is the invariant elem-lowers ⟹ source-lowers ⟹
        // a slot exists.)
        let src = self.slot(arr_obj);
        let elem_arg = if lower_ty(&elem).is_some() {
            Some(elem_expr(
                src.as_deref().expect("map source slot"),
                &iv,
                &elem,
            ))
        } else {
            None
        };
        // ADR-0027: the capture components of the source product, then the
        // element — the `(c₁…cₖ, elem)` body-input shape.
        let arg = if captures == 0 {
            elem_arg
        } else {
            let pair_ty = self.obj_ty(self.ir.func(body).expect("body resolves").input);
            let mut parts: Vec<Option<String>> = Vec::new();
            for j in 0..captures {
                // WP-B (R-NODUP): read the capture THROUGH the source
                // product's field — a Named product gives `oN.fK` (one
                // reference), never a second copy of an Inline feeder's
                // expression; a dissolved product resolves to the field
                // source's expression, which then counts as its one use.
                parts.push(self.component_expr(source, j).map(|(_, v)| v));
            }
            parts.push(elem_arg);
            let (decl, arg) = assemble_body_arg(&pair_ty, &parts);
            for l in decl {
                self.line(2, l);
            }
            arg
        };
        let name = self.quals.device_name(self.fnames, body);
        let mut args: Vec<String> = Vec::new();
        if let Some(a) = arg {
            args.push(a);
        }
        // An erased array-typed body result has no buffer (F6): no out
        // param, no store — the body call stands alone.
        let array_ret = matches!(uelem, Ty::Array { .. }) && lower_ty(&uelem).is_some();
        let tgt = self.slot(target);
        if array_ret {
            let dest = format!(
                "({} + ({iv} * {}ULL))",
                tgt.clone().expect("map target local"),
                flat_count(&uelem)
            );
            args.push(dest);
        }
        // #14: trap argument + per-element check only for a trap-capable
        // body (the oracle's in-order first trap needs no check where no
        // guard can fire).
        let capable = self.caps.get(body);
        if capable {
            args.push("trap".into());
        }
        let call = format!("{name}({})", args.join(", "));
        let ret = self.ret_default.clone();
        if array_ret {
            self.line(2, format!("{call};"));
        } else if lower_ty(&uelem).is_some() {
            let t = tgt.expect("map target local");
            self.line(2, format!("{t}[{iv}] = {call};"));
        } else {
            self.line(2, format!("{call};"));
        }
        if capable {
            self.line(2, format!("if (*trap) {ret}"));
        }
        self.line(1, "}");
    }

    fn emit_zip(&mut self, source: ObjectId, target: ObjectId) {
        let Some(tgt) = self.slot(target) else {
            return; // fully-erased element product: no data, no traps
        };
        let src_ty = self.obj_ty(source);
        let a_ty = src_ty.component_ty(0).cloned().expect("zip a");
        let b_ty = src_ty.component_ty(1).cloned().expect("zip b");
        let (a_elem, n) = array_parts(&a_ty);
        let (b_elem, _) = array_parts(&b_ty);
        let (elem, _) = array_parts(&self.obj_ty(target));
        // An erased input array (e.g. Array{Unit}) has no slot; its element
        // component is erased in the output product too, so it stores nothing.
        let a = self.slot(pair_component(self.ir, source, 0));
        let b = self.slot(pair_component(self.ir, source, 1));
        let iv = self.tmp();
        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        let bare = residual_arity(&elem) == 1;
        for (k, comp_elem, arr) in [(0u32, &a_elem, &a), (1u32, &b_elem, &b)] {
            if let (Some(eidx), Some(arr)) = (erased_index(&elem, k), arr) {
                let lvalue = if bare {
                    format!("{tgt}[{iv}]")
                } else {
                    format!("{tgt}[{iv}].f{eidx}")
                };
                let rval = elem_expr(arr, &iv, comp_elem);
                self.line(2, format!("{lvalue} = {rval};"));
            }
        }
        self.line(1, "}");
    }

    fn emit_enumerate(&mut self, source: ObjectId, target: ObjectId) {
        let Some(tgt) = self.slot(target) else { return };
        let src_ty = self.obj_ty(source);
        let (a_elem, n) = array_parts(&src_ty);
        let (elem, _) = array_parts(&self.obj_ty(target));
        // An erased source array (Array{Unit}) has no slot (F6): the i32
        // index component still stores per element; the element component
        // is erased from the output product too, so it stores nothing (the
        // launch form omits the parameter the same way).
        let a = self.slot(source);
        let iv = self.tmp();
        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        let bare = residual_arity(&elem) == 1;
        if let Some(eidx) = erased_index(&elem, 0) {
            let lvalue = if bare {
                format!("{tgt}[{iv}]")
            } else {
                format!("{tgt}[{iv}].f{eidx}")
            };
            self.line(2, format!("{lvalue} = (int32_t){iv};"));
        }
        if let Some(eidx) = erased_index(&elem, 1) {
            let lvalue = if bare {
                format!("{tgt}[{iv}]")
            } else {
                format!("{tgt}[{iv}].f{eidx}")
            };
            // eidx exists ⟹ the element lowers ⟹ the source array lowers
            // ⟹ it has a slot (the invariant, not a reachable panic).
            let arr = a.as_deref().expect("enumerate source slot");
            let rval = elem_expr(arr, &iv, &a_elem);
            self.line(2, format!("{lvalue} = {rval};"));
        }
        self.line(1, "}");
    }

    fn emit_iota(&mut self, target: ObjectId) {
        let Some(tgt) = self.slot(target) else { return };
        // Elem ctype derived — same guard as `iota_kernel`.
        let (elem, n) = array_parts(&self.obj_ty(target));
        let elem_ct = lower_ty(&elem).expect("iota elem lowers");
        let iv = self.tmp();
        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        self.line(2, format!("{tgt}[{iv}] = ({elem_ct}){iv};"));
        self.line(1, "}");
    }

    fn emit_fill(&mut self, source: ObjectId, target: ObjectId) {
        let Some(tgt) = self.slot(target) else { return };
        let (_, n) = array_parts(&self.obj_ty(target));
        let (_, val) = self.component_expr(source, 0).expect("fill value");
        let iv = self.tmp();
        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        self.line(2, format!("{tgt}[{iv}] = {val};"));
        self.line(1, "}");
    }

    /// The width-rule extension alone: materialize `(int64_t)idx` into a
    /// temp and return its name. This is the whole device form of a
    /// `bounds_proof`-PROVEN `Index` (S20): the proof puts the index
    /// statically in `[0, n)`, so the §3 guard can never fire and is dead
    /// text — the read below rides the same temp either way.
    fn extend_only(&mut self, idx_expr: &str) -> String {
        let i64v = self.tmp();
        self.line(1, format!("int64_t {i64v} = {};", extend_index(idx_expr)));
        i64v
    }

    /// The width-rule extension + signed two-sided bounds guard, device form:
    /// materialize `(int64_t)idx` into a temp, then `idx < 0 || idx >= n`
    /// stores index_oob (kind+1 ⇒ 2u) and returns (§3). Returns the temp name.
    fn guard_index(&mut self, idx_expr: &str, n: u64) -> String {
        let i64v = self.extend_only(idx_expr);
        let cond = bounds_cond(&i64v, n);
        let ret = self.ret_default.clone();
        self.line(1, format!("if ({cond}) {{ *trap = 2u; {ret} }}"));
        i64v
    }

    fn emit_index(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("index array");
        let (elem, n) = array_parts(&arr_ty);
        let (_, idx) = self.component_expr(source, 1).expect("index idx operand");
        // S20: proven in-bounds ⇒ the §3 guard is dead text (trap-freedom is
        // exact) — extension temp only; unproven keeps the guard verbatim.
        let i64v = if self.proof.proven(m) {
            self.extend_only(&idx)
        } else {
            self.guard_index(&idx, n)
        };
        if lower_ty(&self.obj_ty(target)).is_none() {
            return; // erased element: the guard is the whole op
        }
        let arr = self
            .slot(pair_component(self.ir, source, 0))
            .expect("index array slot");
        if matches!(elem, Ty::Array { .. }) {
            // Array element: fresh per-thread local sub-copy (array targets
            // are always Named — the query keeps arrays out of Inline).
            let tgt = self.slot(target).expect("index target");
            let m = flat_count(&elem);
            let jv = self.tmp();
            self.line(
                1,
                format!("for (unsigned long long {jv} = 0; {jv} < {m}ULL; {jv}++) {{"),
            );
            self.line(
                2,
                format!("{tgt}[{jv}] = {arr}[((unsigned long long){i64v} * {m}ULL) + {jv}];"),
            );
            self.line(1, "}");
        } else {
            // A proven Index is guard-free, so its scalar result may be
            // Inline — route through store_obj (WP-B); an unproven one is
            // Named by the query (guarded producer) and stays a statement.
            self.store_obj(target, &format!("{arr}[(unsigned long long){i64v}]"));
        }
    }

    fn emit_update(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
        let (elem, n) = array_parts(&arr_ty);
        let (_, idx) = self.component_expr(source, 1).expect("update idx operand");
        let i64v = self.guard_index(&idx, n);
        let in_place = self.in_place.contains_key(target);
        let tgt = if in_place {
            None
        } else {
            let Some(t) = self.slot(target) else {
                return; // erased element: the guard is the whole op
            };
            Some(t)
        };
        let src = self
            .slot(pair_component(self.ir, source, 0))
            .expect("update src");
        let (_, val) = self.component_expr(source, 2).expect("update value");
        let dst = match &tgt {
            Some(tgt) => {
                // §1 inline column: per-thread local copy, then the store.
                let iv = self.tmp();
                self.line(
                    1,
                    format!(
                        "for (unsigned long long {iv} = 0; {iv} < {}ULL; {iv}++) {{",
                        flat_count(&arr_ty)
                    ),
                );
                self.line(2, format!("{tgt}[{iv}] = {src}[{iv}];"));
                self.line(1, "}");
                tgt.clone()
            }
            // In place (plan-last-use §2 rule 4): the source array is dead
            // at this update — the copy is elided and the store lands in the
            // source's storage; the target aliases it from here on (no
            // produced local was declared for it).
            None => src.clone(),
        };
        if matches!(elem, Ty::Array { .. }) {
            let m = flat_count(&elem);
            let jv = self.tmp();
            self.line(
                1,
                format!("for (unsigned long long {jv} = 0; {jv} < {m}ULL; {jv}++) {{"),
            );
            self.line(
                2,
                format!("{dst}[((unsigned long long){i64v} * {m}ULL) + {jv}] = {val}[{jv}];"),
            );
            self.line(1, "}");
        } else {
            self.line(1, format!("{dst}[(unsigned long long){i64v}] = {val};"));
        }
        if in_place {
            self.slots.insert(target, dst);
        }
    }

    /// A per-thread sequential Fold loop — the oracle verbatim (acc slot 0,
    /// `(acc, e)` per step, in-order), acc in per-thread local storage (§5).
    /// An array-typed acc lives directly in the target's local array.
    /// ADR-0027: a capturing fold's source is `(c₁…cₖ, Acc, [T; n])` — the
    /// captures (per-thread reads of the same buffers/scalars) lead the
    /// body-input argument.
    fn emit_fold(&mut self, source: ObjectId, target: ObjectId, body: FuncId, captures: u32) {
        let src_ty = self.obj_ty(source);
        let acc_ty = src_ty.component_ty(captures).cloned().expect("fold acc");
        let arr_ty = src_ty
            .component_ty(captures + 1)
            .cloned()
            .expect("fold array");
        let (elem, n) = array_parts(&arr_ty);
        let pair_ty = self.obj_ty(self.ir.func(body).expect("body resolves").input);
        // An erased array (Array{Unit}) has no slot; its element is erased
        // from the pair too, so the loop only re-calls the body.
        let arr = self.slot(pair_component(self.ir, source, captures + 1));
        // An erased-element acc (Array{Unit}) has no storage anywhere (F6):
        // treated like an erased scalar acc — the loop still calls the body.
        let array_acc = matches!(acc_ty, Ty::Array { .. }) && lower_ty(&acc_ty).is_some();

        // acc storage: array acc ⇒ the target local; scalar acc ⇒ a temp
        // initialized from the seed; erased acc ⇒ none.
        let acc_name = if array_acc {
            let tgt = self.slot(target).expect("fold target local");
            let (_, seed) = self.component_expr(source, captures).expect("fold seed");
            let jv = self.tmp();
            let m = flat_count(&acc_ty);
            self.line(
                1,
                format!("for (unsigned long long {jv} = 0; {jv} < {m}ULL; {jv}++) {{"),
            );
            self.line(2, format!("{tgt}[{jv}] = {seed}[{jv}];"));
            self.line(1, "}");
            Some(tgt)
        } else if let Some(ct) = lower_ty(&acc_ty) {
            let (_, seed) = self.component_expr(source, captures).expect("fold seed");
            let t = self.tmp();
            self.line(1, format!("{ct} {t} = {seed};"));
            Some(t)
        } else {
            None
        };

        let iv = self.tmp();
        self.line(
            1,
            format!("for (unsigned long long {iv} = 0; {iv} < {n}ULL; {iv}++) {{"),
        );
        // The (c₁…cₖ, acc, e) assembly under the residual remap — the
        // captures are the leading fields of the body-input product.
        let acc_expr = if lower_ty(&acc_ty).is_some() {
            acc_name.clone()
        } else {
            None
        };
        let earg = if lower_ty(&elem).is_some() {
            Some(elem_expr(
                arr.as_deref().expect("fold array slot"),
                &iv,
                &elem,
            ))
        } else {
            None
        };
        let mut parts: Vec<Option<String>> = Vec::new();
        for j in 0..captures {
            // WP-B (R-NODUP): through the source product's field — see
            // emit_map's capture loop.
            parts.push(self.component_expr(source, j).map(|(_, v)| v));
        }
        parts.push(acc_expr);
        parts.push(earg);
        let (decl, arg) = assemble_body_arg(&pair_ty, &parts);
        for l in decl {
            self.line(2, l);
        }
        let name = self.quals.device_name(self.fnames, body);
        let mut args: Vec<String> = Vec::new();
        if let Some(a) = arg {
            args.push(a);
        }
        if array_acc {
            args.push(acc_name.clone().unwrap());
        }
        // #14: trap argument + per-step check only for a trap-capable body.
        let capable = self.caps.get(body);
        if capable {
            args.push("trap".into());
        }
        let call = format!("{name}({})", args.join(", "));
        let ret = self.ret_default.clone();
        if array_acc {
            self.line(2, format!("{call};"));
        } else if acc_name.is_some() {
            self.line(2, format!("{} = {call};", acc_name.clone().unwrap()));
        } else {
            self.line(2, format!("{call};"));
        }
        if capable {
            self.line(2, format!("if (*trap) {ret}"));
        }
        self.line(1, "}");
        // Scalar acc lands in the target slot; array acc is already home.
        if !array_acc
            && acc_name.is_some()
            && let Some(tgt) = self.slot(target)
        {
            self.line(1, format!("{tgt} = {};", acc_name.unwrap()));
        }
    }

    /// A body-local array literal (§2 item 8): per-element stores into the
    /// per-thread local array; nested elements copy cellwise from the
    /// sub-array handle.
    fn emit_literal(&mut self, target: ObjectId) {
        let tgt = match self.slot(target) {
            Some(t) => t,
            None => return, // erased element type: no representation
        };
        let arr_ty = self.obj_ty(target);
        let (elem, n) = array_parts(&arr_ty);
        let nested = matches!(elem, Ty::Array { .. });
        let m = flat_count(&elem);
        for k in 0..n {
            let src = pair_source(self.ir, target, k as u32).expect("literal element");
            let Some((_, expr)) = self.load_whole(src) else {
                continue; // erased element
            };
            if nested {
                let jv = self.tmp();
                self.line(
                    1,
                    format!("for (unsigned long long {jv} = 0; {jv} < {m}ULL; {jv}++) {{"),
                );
                self.line(2, format!("{tgt}[{} + {jv}] = {expr}[{jv}];", k * m));
                self.line(1, "}");
            } else {
                self.line(1, format!("{tgt}[{k}] = {expr};"));
            }
        }
    }

    fn emit_output(&mut self, source: ObjectId, target: ObjectId) {
        if matches!(self.obj_ty(target), Ty::Array { .. }) {
            // An array Return: copy the source's cells into the fn's Return
            // local; the epilogue copies it to the caller's `out`.
            if let (Some(tgt), Some((_, src))) = (self.slot(target), self.load_whole(source)) {
                let m = flat_count(&self.obj_ty(target));
                let jv = self.tmp();
                self.line(
                    1,
                    format!("for (unsigned long long {jv} = 0; {jv} < {m}ULL; {jv}++) {{"),
                );
                self.line(2, format!("{tgt}[{jv}] = {src}[{jv}];"));
                self.line(1, "}");
            }
        } else if let Some((_, val)) = self.load_whole(source) {
            self.store_obj(target, &val);
        }
    }
}

/// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`
/// (kernel-side operand gathering for Zip/Index/Update/Fold products).
fn pair_component(ir: &CategoryIr, agg: ObjectId, k: u32) -> ObjectId {
    pair_source(ir, agg, k).expect("aggregate component resolves")
}

/// The static integer value of arith operand `k` of a Div/Mod source pair,
/// when the operand arrives as a literal `Pair` edge from a Constant object
/// — suggestions.md #13's constant-divisor knowledge. `None` for every
/// other shape (fn parameters, computed operands, floats, non-pair-fed
/// sources): the guards then emit as usual.
pub(crate) fn const_int_operand(ir: &CategoryIr, source: ObjectId, k: u32) -> Option<i128> {
    let obj = ir
        .object(pair_source(ir, source, k)?)
        .expect("object resolves");
    if obj.kind != ObjectKind::Constant {
        return None;
    }
    match &obj.value {
        Some(Value::I32(n)) => Some(*n as i128),
        Some(Value::I64(n)) => Some(*n as i128),
        Some(Value::U8(n)) => Some(*n as i128),
        _ => None,
    }
}

/// The `<cstdint>` MIN macro for a signed int C++ type (mirrors func.rs's
/// host guard).
fn int_min(ct: &str) -> &'static str {
    match ct {
        "int8_t" => "INT8_MIN",
        "int16_t" => "INT16_MIN",
        "int32_t" => "INT32_MIN",
        "int64_t" => "INT64_MIN",
        _ => unreachable!("non-Core int width in Div/Mod"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use flow_ir::{Dest, IrBuilder};

    const L: SourceLoc = SourceLoc { start: 0, end: 0 };

    fn lower_src(src: &str) -> CategoryIr {
        let po = flow_syntax::parse(src);
        assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
        flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
    }

    fn emit_src(src: &str) -> String {
        crate::emit(&lower_src(src)).unwrap()
    }

    fn build_example(name: &str) -> CategoryIr {
        let path = format!(
            "{}/../../../examples/{name}.flow",
            env!("CARGO_MANIFEST_DIR")
        );
        let src = std::fs::read_to_string(&path).unwrap();
        lower_src(&src)
    }

    /// The fnames map, built exactly as lib.rs's emit does.
    fn fnames_of(ir: &CategoryIr) -> SecondaryMap<FuncId, String> {
        let mut fnames: SecondaryMap<FuncId, String> = SecondaryMap::new();
        let entry = ir.entry();
        for (ord, (id, _)) in ir.funcs().enumerate() {
            fnames.insert(
                id,
                if id == entry {
                    "flow_main".to_string()
                } else {
                    format!("fn{ord}")
                },
            );
        }
        fnames
    }

    /// The device section of an emitted module (twin definitions) — from the
    /// first `__device__` to the first `__global__` (kernels follow it).
    fn twin_slice(cu: &str) -> &str {
        let start = cu.find("static __device__").expect("a device twin");
        let end = cu[start..]
            .find("__global__")
            .map(|e| start + e)
            .unwrap_or(cu.len());
        &cu[start..end]
    }

    /// The qualifier of the one fn named `name`.
    fn qual_of(ir: &CategoryIr, quals: &Qualifiers, name: &str) -> FnQual {
        for (id, fd) in ir.funcs() {
            if fd.name == name {
                return quals.get(id);
            }
        }
        panic!("no fn named {name}");
    }

    // --- abi_sizeof (the arena measure — plan-smart-arenas §3) --------------

    fn tup(ts: Vec<Ty>) -> Ty {
        Ty::Tuple(ts)
    }

    fn arr(elem: Ty, size: u64) -> Ty {
        Ty::Array {
            elem: Box::new(elem),
            size,
        }
    }

    #[test]
    fn abi_sizeof_scalars_at_width() {
        assert_eq!(abi_sizeof(&Ty::i32()), Some(4));
        assert_eq!(abi_sizeof(&Ty::i64()), Some(8));
        assert_eq!(abi_sizeof(&Ty::u8()), Some(1));
        assert_eq!(abi_sizeof(&Ty::f32()), Some(4));
        assert_eq!(abi_sizeof(&Ty::f64()), Some(8));
        assert_eq!(abi_sizeof(&Ty::Bool), Some(1));
        // Erased types have no representation to size.
        assert_eq!(abi_sizeof(&Ty::Unit), None);
        assert_eq!(abi_sizeof(&Ty::IoToken), None);
        assert_eq!(abi_sizeof(&Ty::Str), None);
    }

    #[test]
    fn abi_sizeof_products_follow_c_layout() {
        // The drift case: (i32, bool) is 4 + 1 with tail padding to the
        // widest alignment (4) ⇒ 8 — nominal_sizeof says 5.
        let padded = tup(vec![Ty::i32(), Ty::Bool]);
        assert_eq!(nominal_sizeof(&padded), Some(5));
        assert_eq!(abi_sizeof(&padded), Some(8));
        // (f64, i32): 8 + 4, tail-padded to 8 ⇒ 16 (nominal 12).
        assert_eq!(abi_sizeof(&tup(vec![Ty::f64(), Ty::i32()])), Some(16));
        // (i32, f64): 4 + pad4 + 8 ⇒ 16 either way round.
        assert_eq!(abi_sizeof(&tup(vec![Ty::i32(), Ty::f64()])), Some(16));
        // (i32, i32): no padding anywhere ⇒ 8, nominal agrees.
        assert_eq!(abi_sizeof(&tup(vec![Ty::i32(), Ty::i32()])), Some(8));
        // sepia's Pixel (u8 × 3): alignment 1, no padding ⇒ 3.
        assert_eq!(
            abi_sizeof(&tup(vec![Ty::u8(), Ty::u8(), Ty::u8()])),
            Some(3)
        );
        // A handle field rides at pointer width: (i64, [f32; 4]) ⇒ 16.
        assert_eq!(
            abi_sizeof(&tup(vec![Ty::i64(), arr(Ty::f32(), 4)])),
            Some(16)
        );
        // Nested: ((i32, i32), bool) = 8 + 1, tail pad to 4 ⇒ 12.
        let nested = tup(vec![tup(vec![Ty::i32(), Ty::i32()]), Ty::Bool]);
        assert_eq!(abi_sizeof(&nested), Some(12));
    }

    #[test]
    fn abi_sizeof_residual_one_is_the_bare_component() {
        // A residual-1 product lowers to its surviving component — no
        // wrapper, no tail padding (the ty.rs residual rule).
        assert_eq!(abi_sizeof(&tup(vec![Ty::IoToken, Ty::i32()])), Some(4));
        assert_eq!(abi_sizeof(&tup(vec![Ty::Bool, Ty::Unit])), Some(1));
        // Fully erased ⇒ no representation at all.
        assert_eq!(abi_sizeof(&tup(vec![Ty::Unit, Ty::IoToken])), None);
    }

    #[test]
    fn buffer_bytes_of_is_flat_base_times_flat_count() {
        // The numeric twin of buffer_bytes' `sizeof(base) * flat` text.
        assert_eq!(buffer_bytes_of(&arr(Ty::f64(), 16)), Some(128));
        // Nested arrays peel to the flat base.
        assert_eq!(buffer_bytes_of(&arr(arr(Ty::i32(), 2), 3)), Some(24));
        // An array of a padded product: 8 B element × 16.
        let pixel_prod = arr(tup(vec![Ty::i32(), Ty::Bool]), 16);
        assert_eq!(buffer_bytes_of(&pixel_prod), Some(128));
        assert_eq!(
            buffer_bytes(&pixel_prod),
            "sizeof(FlowProd_int32_t_bool) * 16ULL"
        );
        // Erased base ⇒ no buffer.
        assert_eq!(buffer_bytes_of(&arr(Ty::Unit, 4)), None);
    }

    #[test]
    fn align_up_rounds_to_powers_of_two() {
        assert_eq!(align_up(0, 256), 0);
        assert_eq!(align_up(1, 256), 256);
        assert_eq!(align_up(256, 256), 256);
        assert_eq!(align_up(257, 256), 512);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
    }

    // --- BC8 qualifier matrix ----------------------------------------------

    #[test]
    fn qualifier_token_bearing_is_host_only() {
        let ir = lower_src("fn main() {\n    7 -> println;\n}\n");
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "main"), FnQual::HostOnly);
    }

    #[test]
    fn qualifier_pure_scalar_not_body_reachable_is_host_only() {
        // abs-like: token-free, no bulk ops, called only from main — plain
        // host fn, NO __host__ __device__ (keeps scalar programs WP2-stable).
        let src = "fn f(x: i32) -> i32 {\n    x + 1 -> ret;\n}\n\
                   fn main() {\n    5 -> f -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "f"), FnQual::HostOnly);
        let cu = crate::emit(&ir).unwrap();
        assert!(!cu.contains("__host__ __device__"), "{cu}");
        assert!(cu.contains("static int32_t fn0(int32_t in)"), "{cu}");
    }

    #[test]
    fn qualifier_body_reachable_pure_scalar_is_host_device() {
        // sepia: clamp is token-free, no bulk ops, called from the map body.
        let ir = build_example("sepia");
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "clamp"), FnQual::HostDevice);
        assert_eq!(qual_of(&ir, &quals, "main"), FnQual::HostOnly);
        // The map/fold bodies are pure-scalar: single definitions.
        assert_eq!(qual_of(&ir, &quals, "main::map@0"), FnQual::HostDevice);
        assert_eq!(qual_of(&ir, &quals, "main::fold@1"), FnQual::HostDevice);
    }

    #[test]
    fn qualifier_body_reachable_bulk_is_twin() {
        // The map body constructs a literal (a launch-form op) ⇒ twin.
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x + 1][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "main::map@0"), FnQual::Twin);
        // The twin exists; no __host__ __device__ definition for it.
        let cu = crate::emit(&ir).unwrap();
        assert!(cu.contains("static __device__ int32_t d_fn"), "{cu}");
    }

    #[test]
    fn qualifier_array_return_body_is_twin_even_without_bulk_ops() {
        // The fold body returns the acc array by pass-through — no bulk op,
        // but an array return forces the twin (out-param convention).
        let src = "fn main() {\n    \
                   ([0, 0], [[1, 2], [3, 4]]) -> fold { acc, row -> acc } -> r;\n    \
                   r[0] -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "main::fold@0"), FnQual::Twin);
    }

    #[test]
    fn qualifier_bulk_is_transitive_through_calls() {
        // h contains no bulk op but calls g, which folds ⇒ both twins.
        let src = "fn g(a: [i32; 2]) -> i32 {\n    (0, a) -> fold { acc, x -> acc + x } -> ret;\n}\n\
                   fn h(a: [i32; 2]) -> i32 {\n    a -> g -> ret;\n}\n\
                   fn main() {\n    [[1, 2], [3, 4]] -> map { row -> row -> h } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "g"), FnQual::Twin);
        assert_eq!(qual_of(&ir, &quals, "h"), FnQual::Twin);
        assert_eq!(qual_of(&ir, &quals, "main"), FnQual::HostOnly);
        // The inner fold body is pure-scalar: host+device single.
        assert_eq!(qual_of(&ir, &quals, "g::fold@0"), FnQual::HostDevice);
    }

    #[test]
    fn qualifier_twin_propagates_callerward_through_calls() {
        // F4: g is a Twin (array return, no bulk ops); f is body-reachable,
        // pure-scalar, and CALLS g. The array-return Twin rule applied only
        // at decision time would classify f HostDevice — and its single
        // `__host__ __device__` definition would call g's HOST definition on
        // the device pass (nvcc hard error). The fixpoint must propagate
        // (bulk ∨ array-return) callerward through Call edges.
        //
        // The lens-B repro verbatim (classification only — its `(b, 5) -> p`
        // pair is a device-side product-with-array local, the F3 cell, so
        // the module no longer emits; the nvcc hard error is gone either
        // way):
        let repro = "fn g(a: [i32; 2]) -> [i32; 2] {\n    a -> ret;\n}\n\
                     fn f(a: [i32; 2]) -> i32 {\n    a -> g -> b;\n    (b, 5) -> p;\n    p.1 -> ret;\n}\n\
                     fn main() {\n    \
                     [1, 2] -> map { x -> [x, x + 1] -> f -> r;\n        r -> ret\n    } -> rs;\n    \
                     rs[0] -> println;\n}\n";
        let ir = lower_src(repro);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "g"), FnQual::Twin);
        assert_eq!(qual_of(&ir, &quals, "f"), FnQual::Twin);

        // The F3-clean variant (f consumes nothing from b): emits, and f's
        // device twin calls g's TWIN (d_g), never g's host definition.
        let src = "fn g(a: [i32; 2]) -> [i32; 2] {\n    a -> ret;\n}\n\
                   fn f(a: [i32; 2]) -> i32 {\n    a -> g -> b;\n    b -> g -> c;\n    7 -> ret;\n}\n\
                   fn main() {\n    \
                   [1, 2] -> map { x -> [x, x + 1] -> f -> r;\n        r -> ret\n    } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(qual_of(&ir, &quals, "g"), FnQual::Twin);
        assert_eq!(qual_of(&ir, &quals, "f"), FnQual::Twin);
        let fnames = fnames_of(&ir);
        let (mut d_f, mut d_g) = (String::new(), String::new());
        for (id, fd) in ir.funcs() {
            match fd.name.as_str() {
                "f" => d_f = format!("d_{}", fnames[id]),
                "g" => d_g = format!("d_{}", fnames[id]),
                _ => {}
            }
        }
        let cu = crate::emit(&ir).unwrap();
        let twin = twin_slice(&cu);
        // f's twin DEFINITION: the `{d_f}(` occurrence whose param list
        // closes into ` {` (prototypes and call sites end with `;`).
        let needle = format!("{d_f}(");
        let f_start = twin
            .match_indices(&needle)
            .find(|(i, _)| {
                let after = &twin[i + needle.len()..];
                after
                    .find(')')
                    .is_some_and(|p| after[p + 1..].trim_start().starts_with('{'))
            })
            .map(|(i, _)| i)
            .expect("f twin def");
        let f_end = twin[f_start..].find("\n}\n").unwrap() + f_start;
        let f_twin = &twin[f_start..f_end];
        assert!(
            f_twin.contains(&format!("{d_g}(")),
            "f's twin calls g's twin:\n{f_twin}"
        );
    }

    // --- #12: dead host-side twins -------------------------------------------

    #[test]
    fn dead_host_twin_skips_host_definition_and_kernels() {
        // The map body constructs a literal (bulk ⇒ Twin) and is called only
        // from the map kernel — no host path. Its host definition and its
        // Index site's kernel are dead text (#12): main's map + index sites
        // are the only kernels.
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x + 1][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let ir = lower_src(src);
        let body = ir
            .funcs()
            .find(|(_, fd)| fd.kind == FuncKind::MapBody)
            .map(|(id, _)| id)
            .expect("a map body");
        let live = host_reachable(&ir);
        assert!(dead_host_twin(&Qualifiers::analyze(&ir), &live, body));
        let cu = crate::emit(&ir).unwrap();
        // The twin (device form) survives; the host definition is gone.
        assert!(cu.contains("static __device__ int32_t d_fn"), "{cu}");
        assert!(!cu.contains("static int32_t fn"), "{cu}");
        // main's map + index sites only (the body's host-side Index emits
        // no kernel — before #17/#12 it was the duplicate k1_0).
        assert_eq!(cu.matches("__global__ void").count(), 2, "{cu}");
        // Launch count unchanged: one map launch, one index launch.
        let main_start = cu.find("static void flow_main() {").unwrap();
        let main_def = &cu[main_start..];
        assert_eq!(main_def.matches("<<<").count(), 2, "{main_def}");
    }

    #[test]
    fn host_called_twin_keeps_host_definition_and_kernels() {
        // g is body-reachable (called from the map body) AND host-called
        // (from main) ⇒ Twin with a live host path: the host definition and
        // its fold-site kernel stay (#12's boundary).
        let src = "fn g(a: [i32; 2]) -> i32 {\n    (0, a) -> fold { acc, x -> acc + x } -> ret;\n}\n\
                   fn main() {\n    [1, 2] -> map { x -> [x, x + 1] -> g } -> rs;\n    \
                   [3, 4] -> g -> s;\n    rs[0] -> println;\n    s -> println;\n}\n";
        let ir = lower_src(src);
        let g = ir
            .funcs()
            .find(|(_, fd)| fd.name == "g")
            .map(|(id, _)| id)
            .expect("g");
        let quals = Qualifiers::analyze(&ir);
        let live = host_reachable(&ir);
        assert_eq!(quals.get(g), FnQual::Twin);
        assert!(!dead_host_twin(&quals, &live, g));
        let cu = crate::emit(&ir).unwrap();
        // Both definitions of g: the host fn and the device twin.
        assert!(cu.contains("static __device__ int32_t d_fn"), "{cu}");
        assert!(cu.contains("static int32_t fn"), "{cu}");
        // g's host definition launches its fold kernel (main calls g).
        let gname = cu
            .match_indices("static int32_t fn")
            .next()
            .map(|(i, _)| i)
            .expect("g host def");
        let g_def = &cu[gname..];
        assert!(g_def.contains("<<<1, 1>>>"), "{g_def}");
    }

    // --- #14: trap-capability pre-pass ---------------------------------------

    #[test]
    fn trap_caps_int_div_capable_float_div_not() {
        // An integer Div is a §3 zero guard ⇒ capable — except a literal
        // non-zero, non-−1 constant divisor, which cannot trap (the S20 #13
        // credit); a float Div never guards (ADR-0013's S13: ÷0 is ±inf/NaN)
        // ⇒ trap-free.
        let body_caps = |src: &str| {
            let ir = lower_src(src);
            let caps = TrapCaps::analyze(&ir);
            ir.funcs()
                .find(|(_, fd)| fd.kind == FuncKind::MapBody)
                .map(|(id, _)| caps.get(id))
                .expect("a map body")
        };
        assert!(body_caps(
            "fn main() {\n    [1, 2] -> map { x -> x / 2 -> y; y -> z; z / y } -> rs;\n    rs[0] -> println;\n}\n"
        ));
        assert!(!body_caps(
            "fn main() {\n    [1, 2] -> map { x -> x / 2 } -> rs;\n    rs[0] -> println;\n}\n"
        ));
        assert!(!body_caps(
            "fn main() {\n    [1.0, 2.0] -> map { x -> x / 2.0 } -> rs;\n    rs[0] -> println;\n}\n"
        ));
    }

    #[test]
    fn widen_is_trap_free_device_scalar_with_no_kernel_site() {
        let src = "fn main() {\n    2 -> iota -> a;\n    a -> map { x -> x -> widen_f64 } -> b;\n    b[1] -> println;\n}\n";
        let ir = lower_src(src);
        let body = ir
            .funcs()
            .find(|(_, fd)| fd.kind == FuncKind::MapBody)
            .map(|(id, _)| id)
            .expect("map body");
        assert!(!TrapCaps::analyze(&ir).get(body));
        let cu = crate::emit(&ir).unwrap();
        assert!(cu.contains("__host__ __device__ double"), "{cu}");
        assert!(cu.contains("(double)("), "{cu}");
        // Iota + Map + Index only: Widen is scalar and owns no kernel site.
        assert_eq!(cu.matches("__global__ void").count(), 3, "{cu}");
    }

    #[test]
    fn trap_caps_flows_callerward_through_calls() {
        // g has the unproven Index (capable — `a[u]`'s index is a load out
        // of a parameter array: statically unknown, so the S20 proof can't
        // clear it); the map body only CALLS g ⇒ capable transitively; the
        // map site rides the body (TrapCaps::site), so the kernel keeps the
        // full convention — while the float-div sibling kernel trims it.
        let src = "fn g(a: [i32; 2]) -> i32 {\n    a[0] -> u;\n    a[u] -> ret;\n}\n\
                   fn main() {\n    [1, 2] -> map { x -> [x, x] -> g } -> rs;\n    \
                   rs[4 % 3] -> println;\n}\n";
        let ir = lower_src(src);
        let caps = TrapCaps::analyze(&ir);
        let g = ir
            .funcs()
            .find(|(_, fd)| fd.name == "g")
            .map(|(id, _)| id)
            .expect("g");
        let body = ir
            .funcs()
            .find(|(_, fd)| fd.kind == FuncKind::MapBody)
            .map(|(id, _)| id)
            .expect("a map body");
        assert!(caps.get(g));
        assert!(caps.get(body)); // callerward through the Call edge
        // The site rule: the map site rides its capable body.
        let site_m = collect_sites(&ir, body).first().map(|s| s.m);
        let main = ir.entry();
        let main_sites = collect_sites(&ir, main);
        assert!(site_m.is_none()); // the body's sites are inline-form
        assert!(
            caps.site(&ir, main_sites[0].m),
            "the map kernel rides the capable body"
        );
        // The Index site itself: `4 % 3` is statically [0,2] ⊄ [0,2) — the
        // S20 proof can't clear it, so the site stays capable too.
        assert!(caps.site(&ir, main_sites[1].m));
        let cu = crate::emit(&ir).unwrap();
        // Both launches keep the §3 check (map body + index site capable).
        assert_eq!(cu.matches("trap_check_after_launch();").count(), 2, "{cu}");
    }

    #[test]
    fn trap_free_float_body_trims_kernel_param_arg_and_check() {
        // The whole trim on one module (#14): a trap-free float-div body —
        // the kernel drops the trap parameter, the launch drops `d_trap`,
        // and no readback follows. The unproven Index sibling keeps
        // everything (`4 % 3` is statically [0,2] ⊄ [0,2): the S20 proof
        // can't clear it — a proven constant readback would trim too).
        let src = "fn main() {\n    [1.0, 2.0] -> map { x -> x / 2.0 } -> rs;\n    \
                   rs[4 % 3] -> println;\n}\n";
        let cu = emit_src(src);
        let kstart = cu.find("__global__ void k0_0(").expect("map kernel");
        let kend = cu[kstart..].find("\n}\n").unwrap() + kstart;
        let kern = &cu[kstart..kend];
        assert!(!kern.contains("trap"), "trap-free kernel text:\n{kern}");
        // One launch (the Index) keeps d_trap + the readback.
        assert_eq!(cu.matches(", d_trap);").count(), 1, "{cu}");
        assert_eq!(cu.matches("trap_check_after_launch();").count(), 1, "{cu}");
        // Launch count is a per-site property: unchanged.
        assert_eq!(cu.matches("<<<").count(), 2, "{cu}");
    }

    #[test]
    fn proven_index_trims_guard_param_arg_and_check_unproven_keeps() {
        // The S20 refinement end to end: a `bounds_proof`-proven Index site
        // (the constant readback `a[0]`) is trap-free — its kernel drops the
        // §3 guard AND the trap parameter, its launch drops `d_trap` and the
        // readback — while the unproven sibling (`a[17 % 5]`, statically
        // [0,4] ⊄ [0,4)) keeps the full convention verbatim. The two
        // otherwise-identical Index sites now emit TWO kernels (#17's shape
        // key carries the guard/parameter text — proven and unproven are
        // different shapes); launch count is unchanged.
        let src = "fn main() {\n    [1, 2, 3, 4] -> a: [i32; 4];\n    \
                   a[0] -> x;\n    a[17 % 5] -> y;\n    x + y -> println;\n}\n";
        let ir = lower_src(src);
        let caps = TrapCaps::analyze(&ir);
        let main = ir.entry();
        let sites = collect_sites(&ir, main);
        assert!(!caps.site(&ir, sites[0].m), "a[0]: proven ⇒ trap-free");
        assert!(caps.site(&ir, sites[1].m), "a[17 % 5]: unproven ⇒ capable");
        assert!(caps.get(main)); // the unproven site holds main capable
        let cu = crate::emit(&ir).unwrap();
        // The trap-free Index kernel: no guard text, no trap parameter.
        let free = "__global__ void k0_0(int32_t* result, int32_t* arr, int64_t idx) {";
        assert!(cu.contains(free), "{cu}");
        let fstart = cu.find(free).unwrap();
        let fend = cu[fstart..].find("\n}\n").unwrap() + fstart;
        let fkern = &cu[fstart..fend];
        assert!(!fkern.contains("trap"), "{fkern}");
        assert!(
            fkern.contains("*result = arr[(unsigned long long)idx];"),
            "{fkern}"
        );
        // The capable sibling keeps guard + parameter, verbatim.
        assert!(
            cu.contains(
                "__global__ void k0_1(int32_t* result, int32_t* arr, int64_t idx, unsigned int* trap) {"
            ),
            "{cu}"
        );
        assert!(
            cu.contains("if (idx < 0 || idx >= (int64_t)4) { *trap = 2u; return; }"),
            "{cu}"
        );
        // Launches: two, unchanged — one drops `d_trap`, one keeps it.
        assert_eq!(cu.matches("<<<").count(), 2, "{cu}");
        assert_eq!(cu.matches(", d_trap);").count(), 1, "{cu}");
        assert_eq!(cu.matches("trap_check_after_launch();").count(), 1, "{cu}");
        assert_eq!(cu.matches("__global__ void").count(), 2, "{cu}");
    }

    // --- kernel text shapes --------------------------------------------------

    #[test]
    fn elementwise_kernel_shape_and_grid() {
        let cu = emit_src(
            "fn main() {\n    [1, 2, 3, 4] -> a: [i32; 4];\n    \
             a -> map { x -> x * 2 } -> b;\n    b[17 % 5] -> println;\n}\n",
        );
        // BC3: 64-bit thread index, bounds-guarded, n baked as u64.
        assert!(
            cu.contains(
                "unsigned long long i = (unsigned long long)blockIdx.x * blockDim.x + threadIdx.x;"
            ),
            "{cu}"
        );
        assert!(cu.contains("if (i < 4ULL) {"), "{cu}");
        // Grid = ceil(n/256) computed in 64-bit, at the launch.
        assert!(
            cu.contains("<<<(unsigned int)((4ULL + 255ULL) / 256ULL), 256>>>"),
            "{cu}"
        );
        // The map kernel calls the body — the body is trap-free (#14), so
        // no trap pointer rides the kernel or the call.
        assert!(cu.contains("out[i] = fn1(in[i]);"), "{cu}");
        // The launch's Index sibling CAN trap (`17 % 5` is statically
        // [0,4] ⊄ [0,4) — the S20 proof can't clear it): the §3 check
        // follows it.
        assert!(cu.contains("trap_check_after_launch();"), "{cu}");
    }

    #[test]
    fn enumerate_kernel_index_is_thread_index() {
        let ir = build_example("zip_demo");
        let cu = crate::emit(&ir).unwrap();
        assert!(cu.contains("out[i].f0 = (int32_t)i;"), "{cu}");
        assert!(cu.contains("out[i].f1 = a[i];"), "{cu}");
    }

    #[test]
    fn iota_fill_host_kernels_are_64_bit_trap_free_and_deduped() {
        let cu = emit_src(
            "fn main() {\n    4 -> iota -> a;\n    5 -> iota -> b;\n    \
             (7, 4) -> fill -> c;\n    (8, 5) -> fill -> d;\n}\n",
        );
        assert_eq!(cu.matches("__global__ void").count(), 2, "{cu}");
        assert!(
            cu.contains("__global__ void k0_0(int32_t* out, long long n) {"),
            "{cu}"
        );
        assert!(
            cu.contains("long long i = (long long)blockIdx.x * blockDim.x + threadIdx.x;"),
            "{cu}"
        );
        assert!(cu.contains("if (i < n) out[i] = (int32_t)i;"), "{cu}");
        assert!(
            cu.contains("__global__ void k0_2(int32_t* out, long long n, int32_t x) {"),
            "{cu}"
        );
        assert!(cu.contains("if (i < n) out[i] = x;"), "{cu}");
        assert_eq!(cu.matches("k0_0<<<").count(), 2, "{cu}");
        assert_eq!(cu.matches("k0_2<<<").count(), 2, "{cu}");
        assert!(!cu.contains("unsigned int* trap"), "{cu}");
        assert!(!cu.contains("trap_check_after_launch();"), "{cu}");
    }

    #[test]
    fn update_kernel_full_copy_and_guard() {
        let src = "fn main() {\n    mut a: [i32; 4] <- [1, 2, 3, 4];\n    \
                   a[1] <- 9;\n    a[1] -> println;\n}\n";
        let cu = emit_src(src);
        // BC5: the full-copy kernel + bounds guard (index_oob ⇒ 2u, §3's
        // kind+1 flag encoding; width rule).
        assert!(
            cu.contains("if (idx < 0 || idx >= (int64_t)4) { *trap = 2u; return; }"),
            "{cu}"
        );
        assert!(
            cu.contains("out[i] = ((int64_t)i == idx) ? val : src[i];"),
            "{cu}"
        );
        // The host launch passes the extended index + the value by value.
        assert!(cu.contains(", (int64_t)1, 9, d_trap);"), "{cu}");
    }

    #[test]
    fn bounds_guard_width_rule_never_size_t() {
        // u8 (unsigned → zext semantics) and i64 (signed → sext semantics)
        // both realize the width rule as the value-preserving `(int64_t)`
        // conversion + the signed two-sided compare; never size_t. The
        // boundary-OOB constant `3` (size is 3) is exactly the index the
        // S20 proof can never clear — the guard must stay where it fires.
        for idx_decl in ["3 -> i: u8;", "3 -> i: i64;", "3 -> i;"] {
            let src = format!(
                "fn main() {{\n    [1, 2, 3] -> a: [i32; 3];\n    {idx_decl}\n    \
                 a[i] -> println;\n}}\n"
            );
            let cu = emit_src(&src);
            assert!(
                cu.contains("idx < 0 || idx >= (int64_t)3"),
                "idx_decl {idx_decl}:\n{cu}"
            );
            assert!(!cu.contains("(size_t)"), "{cu}");
        }
    }

    #[test]
    fn index_kernel_scalar_readback_d2h() {
        let ir = build_example("vector_add");
        let cu = crate::emit(&ir).unwrap();
        // 1-cell device buffer (a fn-zone member — arena pointer init, #18),
        // <<<1, 1>>> launch, D→H into the host local.
        assert!(cu.contains("int32_t* t"), "{cu}");
        assert!(cu.contains("= (int32_t*)(arena0 + "), "{cu}");
        assert!(cu.contains("<<<1, 1>>>"), "{cu}");
        // The index readback specifically — the prelude's trap-flag memcpy
        // would satisfy a bare "cudaMemcpyDeviceToHost" in every module.
        assert!(
            cu.contains("cudaMemcpyDeviceToHost), \"cudaMemcpy(index)\""),
            "{cu}"
        );
        assert!(
            cu.contains("*result = arr[(unsigned long long)idx];"),
            "{cu}"
        );
    }

    #[test]
    fn index_kernel_array_elem_copies_in_kernel() {
        let src = "fn main() {\n    [[1, 2], [3, 4]] -> m: [[i32; 2]; 2];\n    \
                   m[1] -> row: [i32; 2];\n    row[0] -> println;\n}\n";
        let cu = emit_src(src);
        // Array element: fresh device buffer + in-kernel device-to-device copy.
        assert!(
            cu.contains("for (unsigned long long j = 0; j < 2ULL; j++) {"),
            "{cu}"
        );
        assert!(
            cu.contains("result[j] = arr[((unsigned long long)idx * 2ULL) + j];"),
            "{cu}"
        );
        // No D→H readback for the array result (it stays on device); the
        // only D→H is the scalar row[0] index + the trap-flag reads.
        assert!(!cu.contains("cudaMemcpy(row"), "{cu}");
    }

    #[test]
    fn fold_kernel_single_thread_oracle_order() {
        let ir = build_example("vector_add");
        let cu = crate::emit(&ir).unwrap();
        // BC4: single-thread kernel, the oracle loop verbatim — acc slot 0,
        // (acc, e) per step, in-order. The fold body is trap-free (#14):
        // no trap parameter and no per-step check (vector_add's only
        // would-be trap sources are its two Index sites — constant
        // readbacks the S20 proof clears, so they trim too).
        assert!(cu.contains("int32_t acc = acc0;"), "{cu}");
        assert!(cu.contains("pair.f0 = acc;"), "{cu}");
        assert!(cu.contains("pair.f1 = arr[i];"), "{cu}");
        assert!(cu.contains("acc = fn2(pair);"), "{cu}");
        assert!(cu.contains("*result = acc;"), "{cu}");
        // Scalar acc: 1-cell buffer + D→H readback at the site.
        assert!(cu.contains("\"cudaMemcpy(fold)\""), "{cu}");
    }

    #[test]
    fn fold_kernel_array_acc_stays_on_device() {
        let src = "fn main() {\n    \
                   ([0, 0], [[1, 2], [3, 4]]) -> fold { acc, row -> acc } -> r: [i32; 2];\n    \
                   r[1] -> println;\n}\n";
        let cu = emit_src(src);
        // §5: acc in per-thread local storage (statically bounded).
        assert!(cu.contains("int32_t acc[2];"), "{cu}");
        assert!(cu.contains("acc[j] = acc0[j];"), "{cu}");
        // Array acc ⇒ twin body with the out-param convention. The body is
        // a pure passthrough — trap-free (#14): no trap argument.
        assert!(cu.contains("d_fn"), "{cu}");
        assert!(cu.contains(", acc);"), "{cu}");
        // The result is the device buffer — written back in-kernel.
        assert!(cu.contains("result[j] = acc[j];"), "{cu}");
    }

    // --- literal uploads ------------------------------------------------------

    #[test]
    fn literal_upload_static_const_one_memcpy() {
        let ir = build_example("vector_add");
        let cu = crate::emit(&ir).unwrap();
        assert!(
            cu.contains("static const int32_t lit0[16] = { 0, 1, 2, 3, 4, 5, 6, 7"),
            "{cu}"
        );
        // Exactly one H→D memcpy per literal (two literals here) — BC11.
        assert_eq!(cu.matches("cudaMemcpyHostToDevice").count(), 2, "{cu}");
        // Both buffers are fn-zone members (#18): arena pointer inits, and
        // the zone release replaces the per-buffer frees.
        assert!(cu.contains("o2 = (int32_t*)(arena0 + 0ULL);"), "{cu}");
        assert!(cu.contains("o3 = (int32_t*)(arena0 + 256ULL);"), "{cu}");
        assert!(!cu.contains("cudaFree(o2)"), "{cu}");
        assert!(cu.contains("cudaFree(arena0)"), "{cu}");
    }

    #[test]
    fn literal_computed_elements_plain_local() {
        // sepia's [Pixel; 16]: struct-valued elements — plain local data
        // array (not static const), same one-memcpy shape.
        let ir = build_example("sepia");
        let cu = crate::emit(&ir).unwrap();
        assert!(
            cu.contains("FlowProd_float_float_float lit0[16] = { o2,"),
            "{cu}"
        );
        assert!(!cu.contains("static const FlowProd"), "{cu}");
        assert_eq!(cu.matches("cudaMemcpyHostToDevice").count(), 1, "{cu}");
    }

    #[test]
    fn literal_nested_elements_device_to_device() {
        let src = "fn main() {\n    [[1, 2], [3, 4]] -> m: [[i32; 2]; 2];\n    \
                   m[1] -> row: [i32; 2];\n    row[0] -> println;\n}\n";
        let cu = emit_src(src);
        // The inner arrays upload H→D (each is a scalar literal); the outer
        // is a flat buffer filled by device-to-device copies.
        assert_eq!(cu.matches("cudaMemcpyDeviceToDevice").count(), 2, "{cu}");
        // The outer buffer is a fn-zone member (#18): after the two inner
        // literals' slots, at the compile-time offset (ABI 4 B × 4 flat).
        assert!(cu.contains("o4 = (int32_t*)(arena0 + 512ULL);"), "{cu}");
        assert!(
            cu.contains("cudaMemcpy(o4 + 0ULL, o2, sizeof(int32_t) * 2ULL"),
            "{cu}"
        );
    }

    // --- inline form (device twins) -------------------------------------------

    #[test]
    fn twin_inline_map_is_per_thread_sequential_loop() {
        // A map body containing a nested map: the twin runs the inner map as
        // a per-thread sequential loop; the outer kernel calls the twin.
        // Both bodies are trap-free (#14): no trap threading anywhere.
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x] -> map { y -> y + 1 } } -> rs;\n    \
                   rs[0][0] -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("static __device__"), "{cu}");
        // The twin's inner-map loop, no trap references at all (#14).
        let twin = twin_slice(&cu);
        assert!(twin.contains("for (unsigned long long"), "{twin}");
        assert!(!twin.contains("trap"), "{twin}");
        // The outer kernel calls the twin with the out-offset destination.
        assert!(cu.contains("d_fn"), "{cu}");
    }

    #[test]
    fn twin_iota_fill_are_local_loops() {
        let src = r#"
fn main() {
    [1, 2] -> map { x ->
        4 -> iota -> a;
        (x, 4) -> fill -> b;
        a[1] + b[2]
    } -> rs;
    rs[0] -> println;
}
"#;
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        assert_eq!(twin.matches("for (unsigned long long").count(), 2, "{twin}");
        assert!(twin.contains("] = (int32_t)"), "{twin}");
        assert!(twin.contains("] = o"), "{twin}");
        assert!(!twin.contains("<<<"), "{twin}");
    }

    #[test]
    fn twin_body_local_literal_initializer() {
        // §2 item 8: a body-local array literal is a per-thread local
        // initializer, never a host static.
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x + 1][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        // Per-thread local array + per-element stores.
        assert!(twin.contains("[2];"), "{twin}");
        assert!(twin.contains("[0] = "), "{twin}");
        assert!(twin.contains("[1] = "), "{twin}");
        // The host static+memcpy mechanism does NOT appear in the twin.
        assert!(!twin.contains("cudaMemcpy"), "{twin}");
        assert!(!twin.contains("static const"), "{twin}");
    }

    #[test]
    fn twin_index_guard_sets_flag_and_returns() {
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x + 1][x] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        // The device bounds guard: extend to int64_t, two-sided compare,
        // set the flag + return (§3) — never flow_trap on device.
        assert!(twin.contains("int64_t t"), "{twin}");
        assert!(twin.contains("< 0 ||"), "{twin}");
        assert!(twin.contains("*trap = 2u; return int32_t{};"), "{twin}");
        assert!(!twin.contains("flow_trap"), "{twin}");
    }

    #[test]
    fn twin_proven_index_elides_guard() {
        // The S20 row-1 half, in-twin: the constant-index read
        // `[x, x + 1][0]` is provably in-bounds (0 < 2) — the §3 guard is
        // dead text, so the twin emits the extension temp + the plain
        // per-thread read and nothing else. With its only trap source
        // elided the twin is trap-free (#14): no trap parameter, no flag
        // references at all (the unproven sibling above keeps everything).
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x + 1][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        assert!(twin.contains("int64_t t"), "{twin}");
        assert!(!twin.contains("< 0 ||"), "{twin}");
        assert!(twin.contains("[(unsigned long long)t"), "{twin}");
        assert!(!twin.contains("trap"), "{twin}");
    }

    #[test]
    fn twin_div_guard_sets_flag_and_returns() {
        // The body has a literal (bulk ⇒ twin) AND a Div: the device zero
        // guard stores the div_zero encoding (1u) and returns.
        let src = "fn main() {\n    [10, 0] -> map { x -> [100 / x, x][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        assert!(
            twin.contains("== 0) { *trap = 1u; return int32_t{}; }"),
            "{twin}"
        );
    }

    #[test]
    fn twin_float_neg_of_negative_constant_is_parenthesized() {
        // The twin (inline-form) Neg has the same rule as the host table
        // (F5): `-({val})`, never the ill-formed `--1.5e0`.
        let src = "fn main() {\n    \
                   [1.0, 2.0] -> map { x ->\n        \
                       -1.5 -> c;\n        \
                       -c -> d;\n        \
                       [d, x][0] -> ret\n    \
                   } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        // WP-B: the Neg may inline into its consumer — the pin is the
        // parenthesized form itself, wherever it lands, and the absence of
        // the ill-formed `--` ANYWHERE in the twin.
        assert!(!twin.contains("--"), "{twin}");
        assert!(twin.contains("-(-1.5e0)"), "{twin}");
    }

    #[test]
    fn trap_flag_stores_encode_kind_plus_one() {
        // §3's flag encoding: the flag is cudaMemset-zeroed (0 = quiescent),
        // so a guard stores kind + 1 — div_zero ⇒ 1u, index_oob ⇒ 2u — and
        // the host readback decodes with flow_trap(kind - 1) back to the
        // flow-rt kinds 0/1. A bare-kind store would collide: div_zero's 0
        // would read back as "no trap" (the R1 class cross).
        let src = "fn main() {\n    [10, 0] -> map { x -> [100 / x, x][x] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        // Exact store texts: div_zero ⇒ 1u, index_oob ⇒ 2u.
        assert!(
            twin.contains("== 0) { *trap = 1u; return int32_t{}; }"),
            "{twin}"
        );
        assert!(
            twin.contains(") { *trap = 2u; return int32_t{}; }"),
            "{twin}"
        );
        // The host decode maps 1→0 (div_zero) and 2→1 (index_oob).
        assert!(cu.contains("flow_trap(kind - 1);"), "{cu}");
    }

    #[test]
    fn host_device_fn_div_guard_is_ifdef_split() {
        // A pure-scalar body with a Div: single __host__ __device__
        // definition; the guard is compiled per-site by the preprocessor.
        let src = "fn main() {\n    [10, 0] -> map { x -> 100 / x } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("static __host__ __device__"), "{cu}");
        assert!(cu.contains("#ifdef __CUDA_ARCH__"), "{cu}");
        assert!(cu.contains("*d_trap = 1u; return int32_t{};"), "{cu}");
        assert!(cu.contains("#else"), "{cu}");
        assert!(cu.contains("flow_trap(0);"), "{cu}");
        assert!(cu.contains("#endif"), "{cu}");
    }

    #[test]
    fn device_call_trap_check_after_call() {
        // The nested-map twin calls the inner body per element and checks
        // the flag after each call (first-trap-wins, §3) — the inner body
        // here is trap-CAPABLE (the Div's zero guard), so the full trap
        // convention rides (#14 trims it only for trap-free callees).
        let src = "fn main() {\n    [1, 2] -> map { x -> [x, x] -> map { y -> y / x } } -> rs;\n    \
                   rs[0][0] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        assert!(twin.contains(", trap);"), "{twin}");
        assert!(twin.contains("if (*trap) return;"), "{twin}");
    }

    #[test]
    fn twin_signature_array_return_uses_out_param() {
        let ir = lower_src(
            "fn main() {\n    ([0, 0], [[1, 2], [3, 4]]) -> fold { acc, row -> acc } -> r;\n    \
             r[0] -> println;\n}\n",
        );
        let fnames = fnames_of(&ir);
        let caps = TrapCaps::analyze(&ir);
        for (id, fd) in ir.funcs() {
            if fd.kind == FuncKind::FoldBody {
                let sig = twin_signature(&ir, id, &fnames, &caps);
                // The body is a pure passthrough — trap-free (#14), so the
                // trailing trap parameter is trimmed.
                assert_eq!(
                    sig,
                    "static __device__ void d_fn1(FlowProd_int32_tp_int32_tp in, int32_t* out)"
                );
            }
        }
    }

    // --- module-level invariants ----------------------------------------------

    #[test]
    fn emission_order_device_before_kernels_before_host() {
        let ir = build_example("vector_add");
        let cu = crate::emit(&ir).unwrap();
        let dev = cu.find("__device__").expect("device fn");
        let kern = cu.find("__global__").expect("kernel");
        let proto = cu.find("static void flow_main();").expect("host proto");
        let def = cu.find("static void flow_main() {").expect("host def");
        assert!(dev < kern, "device fns before kernels");
        assert!(kern < proto, "kernels before host prototypes");
        assert!(proto < def, "prototypes before definitions");
    }

    #[test]
    fn vector_add_launch_count_and_trap_checks() {
        let ir = build_example("vector_add");
        let cu = crate::emit(&ir).unwrap();
        // zip + map + 2 index + fold = 5 launches (launch count is a
        // per-site property — unchanged by #17's dedup, #14's trim, and the
        // S20 proof refinement).
        assert_eq!(cu.matches("<<<").count(), 5, "{cu}");
        // §3's check-after-EVERY-launch, trimmed by #14 to the sites that
        // can trap — now zero: vector_add's only would-be trap sources are
        // its two Index sites, and both are constant readbacks (`c[0]`,
        // `c[15]`) the S20 bounds proof clears (0 < 16, 15 < 16) ⇒
        // trap-free, no readback follows any launch.
        assert_eq!(cu.matches("trap_check_after_launch();").count(), 0, "{cu}");
    }

    #[test]
    fn host_device_fn_called_from_both_sites() {
        // sq is called from the map body (device) AND from main (host): one
        // definition serves both. sq is trap-FREE (#14: `x * x` has no
        // guard), so its two-site signature takes no trap pointer and every
        // call site passes nothing — the definition shape is otherwise the
        // BC8 (iv) single-definition rule.
        let src = "fn sq(x: i32) -> i32 {\n    x * x -> ret;\n}\n\
                   fn main() {\n    [1, 2] -> map { x -> x -> sq } -> rs;\n    \
                   3 -> sq -> s;\n    rs[0] -> println;\n    s -> println;\n}\n";
        let cu = emit_src(src);
        assert!(
            cu.contains("static __host__ __device__ int32_t fn0(int32_t in)"),
            "{cu}"
        );
        // The kernel calls the map body; the map body (itself a HostDevice
        // fn) calls sq — and so does main; nobody threads a trap pointer.
        assert!(cu.contains("out[i] = fn2(in[i]);"), "{cu}");
        // WP-C (R-ONENAME): the HostDevice body calls sq with the param
        // read in place — no extraction local.
        assert!(cu.contains("= fn0(in);"), "{cu}");
        let main_start = cu.find("static void flow_main() {").unwrap();
        let host = &cu[main_start..];
        assert!(host.contains("fn0(3)"), "{host}");
        // Exactly one definition of sq (no twin).
        assert_eq!(cu.matches("fn0(int32_t in").count(), 2, "{cu}"); // proto + def
    }

    #[test]
    fn emit_twice_byte_equal() {
        for name in ["vector_add", "zip_demo", "sepia"] {
            let a = crate::emit(&build_example(name)).unwrap();
            let b = crate::emit(&build_example(name)).unwrap();
            assert_eq!(a, b, "{name}: emit is not byte-deterministic");
        }
    }

    // --- F3: the recorded Unsupported cell — arrays embedded in products ---
    //
    // Deep-value semantics break under the handle model (DESIGN §5): a
    // product whose residual contains an array lowers to a struct with a
    // `T*` field, and on device that field can only hold a per-thread
    // local-memory or buffer-interior pointer — both dangle the moment the
    // struct escapes into global memory. The honest move is the recorded
    // cell, never a silent miscompile.

    /// The F3 cell, asserted on `emit`.
    fn assert_product_array_cell(src: &str) {
        let ir = lower_src(src);
        let e = crate::emit(&ir).unwrap_err();
        assert!(
            matches!(
                e,
                EmitError::Unsupported { ref feature, .. }
                    if feature == "arrays embedded in products on device"
            ),
            "expected the F3 Unsupported cell, got {e:?}"
        );
    }

    #[test]
    fn map_body_returning_product_with_array_is_unsupported() {
        // Repro 1: the map's output element type is a product containing an
        // array — the elementwise kernel would store per-thread local
        // pointers into the global output buffer.
        assert_product_array_cell(
            "fn main() {\n    [1, 2] -> map { x -> ([x, x], x) } -> rs;\n    \
             rs[0] -> p;\n    p.1 -> println;\n}\n",
        );
    }

    #[test]
    fn map_body_local_product_with_array_is_unsupported() {
        // Repro 1b: the body only PASSES the product through a device-side
        // local (scalar return) — the twin's local struct still has no
        // honest storage for the array field.
        assert_product_array_cell(
            "fn main() {\n    [1, 2] -> map { x ->\n        \
             ([x, x], x) -> p;\n        p.1 -> ret\n    } -> rs;\n    \
             rs[0] -> println;\n}\n",
        );
    }

    #[test]
    fn zip_over_nested_arrays_is_unsupported() {
        // Repro 2: zip producing ([i32;2], [i32;2]) elements — the kernel
        // would store buffer-INTERIOR pointers into global memory; a
        // consuming Index kernel would dereference them across launches
        // (after the pointee buffers' frees, or as garbage).
        assert_product_array_cell(
            "fn main() {\n    \
             ([[1, 2], [3, 4]], [[5, 6], [7, 8]]) -> zip -> z;\n    \
             z[0] -> p;\n    p.0 -> a;\n    a[0] -> println;\n}\n",
        );
    }

    #[test]
    fn host_device_fn_product_with_array_return_is_unsupported() {
        // Repro 3: a body-reachable pure fn (no bulk ops, param passthrough)
        // returning a product-with-array — classified HostDevice, its single
        // definition would return a pointer-field struct BY VALUE on the
        // device pass.
        let src = "fn g(a: [i32; 2]) -> ([i32; 2], i32) {\n    (a, 5) -> ret;\n}\n\
                   fn main() {\n    \
                   [1, 2] -> map { x -> [x, x + 1] -> g -> p;\n        p.1 -> ret\n    } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        assert_product_array_cell(src);
    }

    #[test]
    fn product_array_cell_does_not_fire_on_supported_shapes() {
        // Host-side products holding handles are fine (the struct fields
        // are host copies of device pointers — F2's mk shape).
        let mk = "fn mk(x: i32) -> ([i32; 2], i32) {\n    [1, 2] -> a;\n    (a, x) -> ret;\n}\n\
                  fn main() {\n    5 -> mk -> p;\n    p.0 -> arr;\n    arr[1] -> println;\n}\n";
        assert!(crate::emit(&lower_src(mk)).is_ok(), "host-side product");
        // Arrays OF products (sepia's [Pixel; 16] — products of scalars).
        assert!(
            crate::emit(&build_example("sepia")).is_ok(),
            "array of scalar products"
        );
        // Products of products without arrays.
        let nested = "fn main() {\n    5 -> x;\n    ((x, x), true) -> p;\n    \
                      p.0 -> q;\n    q.0 -> println;\n}\n";
        assert!(
            crate::emit(&lower_src(nested)).is_ok(),
            "nested scalar products"
        );
        // The array-acc fold's (acc, e) pair: a product-with-array passed
        // BY VALUE to the body twin — registers only, never global memory.
        let fold = "fn main() {\n    \
                    ([0, 0], [[1, 2], [3, 4]]) -> fold { acc, row -> acc } -> r: [i32; 2];\n    \
                    r[1] -> println;\n}\n";
        assert!(
            crate::emit(&lower_src(fold)).is_ok(),
            "the by-value (acc, e) pair"
        );
        // Scalar products on device (the ordinary map/zip shapes).
        assert!(
            crate::emit(&build_example("vector_add")).is_ok(),
            "scalar products"
        );
    }

    // --- WP4: loops in bodies (inline-form quartets) -------------------------

    #[test]
    fn body_fn_scalar_loop_is_host_device_quartet() {
        // §1's inline-form cell: a canonical loop inside a map body lowers
        // as a top-level loop of the synthesized body fn. A pure-scalar
        // body is BC8 case (iv): ONE `__host__ __device__` definition —
        // the quartet is plain C++, valid on both passes.
        let src = "fn main() {\n    \
                       [1, 2, 3, 4] -> a: [i32; 4];\n    \
                       a -> map { x ->\n    \
                           mut i: i32 <- 0;\n    \
                           mut acc: i32 <- x;\n    \
                           loop {\n    \
                               (i < 2) -> {\n    \
                                   -true-> { acc + 10 -> acc; i + 1 -> i; -> loop; }\n    \
                                   -false-> acc -> r;\n    \
                               }\n    \
                           }\n    \
                           r -> ret\n    \
                       } -> rs;\n    \
                       rs[1] -> println;\n}\n";
        let cu = emit_src(src);
        // The body fn's single two-site definition carries the quartet.
        let def_start = cu
            .find("static __host__ __device__ int32_t fn")
            .expect("a HostDevice body fn:\n{cu}");
        let def_end = cu[def_start..].find("\n}\n").unwrap() + def_start;
        let def = &cu[def_start..def_end];
        assert_eq!(def.matches("while (true) {").count(), 1, "{def}");
        assert!(def.contains(") { break; }"), "{def}");
        // Guard-first: the break precedes the advance-cone add and the back
        // edge; the exit copy follows the loop close.
        let brk = def.find(") { break; }").unwrap();
        let add = def.find("(uint32_t)").expect("advance arith:\n{def}");
        assert!(brk < add, "guard-first: break before advance cone:\n{def}");
    }

    #[test]
    fn twin_body_loop_is_per_thread_sequential_quartet() {
        // §1's inline-form cell, Twin case: the body contains a launch-form
        // op (the `[10, 20]` literal + Index), so BC8 makes it a twin; its
        // canonical loop is emitted as a per-thread sequential quartet. The
        // speculative `[10, 20][i]` read lives in the advance cone — on the
        // exit step (i = 2) it never executes (guard-first, ADR-0016).
        // (An ARRAY-carried loop exit in a body is rejected by the lower —
        // L1201 — before it can reach the backend; the scalar-carried shape
        // is the one the surface language produces today.)
        let src = "fn main() {\n    \
                       [1, 2] -> map { x ->\n    \
                           mut i: i32 <- 0;\n    \
                           mut acc: i32 <- x;\n    \
                           loop {\n    \
                               (i < 2) -> {\n    \
                                   -true-> { acc + [10, 20][i] -> acc; i + 1 -> i; -> loop; }\n    \
                                   -false-> acc -> r;\n    \
                               }\n    \
                           }\n    \
                           r -> ret\n    \
                       } -> rs;\n    \
                       rs[1] -> println;\n}\n";
        let cu = emit_src(src);
        let twin = twin_slice(&cu);
        assert_eq!(twin.matches("while (true) {").count(), 1, "{twin}");
        assert!(twin.contains(") { break; }"), "{twin}");
        // The loop-invariant literal is constructed once (outside the loop,
        // per-thread); the Index's int64 guard + read is inside the advance
        // cone — after the break (guard-first).
        let lit = twin
            .find("[2];")
            .expect("per-thread literal local:\n{twin}");
        let wh = twin.find("while (true) {").unwrap();
        assert!(lit < wh, "loop-invariant literal outside the loop:\n{twin}");
        let brk = twin.find(") { break; }").unwrap();
        let idx = twin.find("int64_t").expect("index guard temp:\n{twin}");
        assert!(
            brk < idx,
            "guard-first: the speculative Index is post-guard:\n{twin}"
        );
        // The device bounds guard for the in-loop Index (index_oob ⇒ 2u).
        assert!(twin.contains("*trap = 2u;"), "{twin}");
    }

    // --- F6: erased-element arrays degrade gracefully (never a panic) ------
    //
    // The surface cannot write a `()` literal (P0001), so the erased-element
    // shapes are built with IrBuilder: a MapBody `i32 → Unit` (zero writers,
    // I-RET) mapped over an i32 literal yields a `[Unit; n]` value, which
    // has no runtime representation (L4).

    fn arr_ty(elem: Ty, size: u64) -> Ty {
        Ty::Array {
            elem: Box::new(elem),
            size,
        }
    }

    /// `m1: i32 → Unit` — the Unit-array factory body (no morphisms).
    fn declare_unit_body(b: &mut IrBuilder) -> FuncId {
        let m1 = b
            .declare(FuncKind::MapBody, "m1", Ty::i32(), Ty::Unit, L)
            .unwrap();
        {
            let fb = b.build_fn(m1).unwrap();
            fb.finish().unwrap();
        }
        m1
    }

    #[test]
    fn fold_erased_acc_degrades_gracefully() {
        // F6 sites func.rs:844 + kernel.rs:686: a launch-form fold whose acc
        // is an erased-element array ([Unit;2]) — no acc0 kernel parameter,
        // no seed argument at the launch, no result buffer; the kernel still
        // launches and runs the body per element. (This body is a pure
        // passthrough — trap-free, so #14 drops the trap threading; a
        // trap-capable body would keep it.) The fold body's return is the
        // erased
        // array: with no device buffer to write, the out-param convention
        // is moot, so the body classifies HostDevice (void on both sites).
        let mut b = IrBuilder::new();
        let m1 = declare_unit_body(&mut b);
        let fb_id = b
            .declare(
                FuncKind::FoldBody,
                "fb",
                Ty::Tuple(vec![arr_ty(Ty::Unit, 2), Ty::i32()]),
                arr_ty(Ty::Unit, 2),
                L,
            )
            .unwrap();
        {
            let mut fb = b.build_fn(fb_id).unwrap();
            let i = fb.input();
            let acc = fb.proj(i, 0, Dest::Fresh(None), L).unwrap();
            fb.output(acc, None, L).unwrap();
            fb.finish().unwrap();
        }
        let main = b
            .declare(FuncKind::Named, "main", Ty::Unit, Ty::Unit, L)
            .unwrap();
        {
            let mut fb = b.build_fn(main).unwrap();
            let c1 = fb.constant(Value::I32(1), L).unwrap();
            let c2 = fb.constant(Value::I32(2), L).unwrap();
            let a = fb.pack_array(&[c1, c2], Dest::Fresh(None), L).unwrap();
            let u = fb.map(m1, a, Dest::Fresh(None), L).unwrap();
            let src = fb.pack(&[u, a], Dest::Fresh(None), L).unwrap();
            let _r = fb.fold(fb_id, src, Dest::Fresh(None), L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(main).unwrap();
        let cu = crate::emit(&ir).unwrap();
        // No acc0 anywhere (kernel param, host seed arg, or local).
        assert!(!cu.contains("acc0"), "{cu}");
        // The fold kernel still launches and runs the oracle loop, calling
        // the (void) body per element. The body is trap-free (#14): no trap
        // argument on the call and no per-step check.
        assert!(cu.contains("<<<1, 1>>>"), "{cu}");
        assert!(
            cu.contains("for (unsigned long long i = 0; i < 2ULL; i++) {"),
            "{cu}"
        );
        assert!(cu.contains("(arr[i]);"), "{cu}");
        assert!(!cu.contains("if (*trap)"), "{cu}");
    }

    /// The main wrapper shared by the inline-form tests: `[1,2] -> map mb ->
    /// rs; rs[0] -> ret` (`mb` already built and finished).
    fn seal_main_over(mut b: IrBuilder, mb: FuncId) -> CategoryIr {
        let main = b
            .declare(FuncKind::Named, "main", Ty::Unit, Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(main).unwrap();
            let c1 = fb.constant(Value::I32(1), L).unwrap();
            let c2 = fb.constant(Value::I32(2), L).unwrap();
            let a = fb.pack_array(&[c1, c2], Dest::Fresh(None), L).unwrap();
            let rs = fb.map(mb, a, Dest::Fresh(None), L).unwrap();
            let z = fb.constant(Value::I32(0), L).unwrap();
            let r = fb.index(rs, z, Dest::Fresh(None), L).unwrap();
            fb.output(r, None, L).unwrap();
            fb.finish().unwrap();
        }
        b.seal(main).unwrap()
    }

    #[test]
    fn twin_map_erased_source_degrades_gracefully() {
        // F6 site kernel.rs:1289: the inline map's SOURCE is the erased
        // [Unit;2] — the twin must omit the body argument but still run the
        // per-thread loop (the launch form omits the parameter the same
        // way). m2: Unit → i32, a constant body.
        let mut b = IrBuilder::new();
        let m1 = declare_unit_body(&mut b);
        let m2 = b
            .declare(FuncKind::MapBody, "m2", Ty::Unit, Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(m2).unwrap();
            let c = fb.constant(Value::I32(5), L).unwrap();
            fb.output(c, None, L).unwrap();
            fb.finish().unwrap();
        }
        let mb = b
            .declare(FuncKind::MapBody, "mb", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(mb).unwrap();
            let x = fb.input();
            // A body-local literal: the bulk op that makes mb a Twin.
            let arr = fb.pack_array(&[x, x], Dest::Fresh(None), L).unwrap();
            let u = fb.map(m1, arr, Dest::Fresh(None), L).unwrap();
            let w = fb.map(m2, u, Dest::Fresh(None), L).unwrap();
            // Index by the element `x` itself ([1,2] statically ⊄ [0,2)) —
            // the S20 proof can't clear it, so the twin keeps its §3 guard
            // (a proven constant index would elide it).
            let r = fb.index(w, x, Dest::Fresh(None), L).unwrap();
            fb.output(r, None, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = seal_main_over(b, mb);
        let cu = crate::emit(&ir).unwrap();
        let twin = twin_slice(&cu);
        // The per-thread loop over the erased source runs (n = 2), storing
        // the body result. The constant body is trap-free (#14): its call
        // passes no trap argument — though the twin itself stays
        // trap-capable (its unproven Index guard) and keeps the parameter.
        assert!(
            twin.contains("for (unsigned long long") && twin.contains("< 2ULL;"),
            "{twin}"
        );
        assert!(twin.contains("= fn1();"), "{twin}");
        assert!(twin.contains("*trap = 2u;"), "{twin}");
    }

    #[test]
    fn twin_enumerate_erased_source_degrades_gracefully() {
        // F6 site kernel.rs:1364: enumerate over the erased [Unit;2] — the
        // i32 index component (the thread index) still stores per element;
        // the erased element component stores nothing.
        let mut b = IrBuilder::new();
        let m1 = declare_unit_body(&mut b);
        let mb = b
            .declare(FuncKind::MapBody, "mb", Ty::i32(), Ty::i32(), L)
            .unwrap();
        {
            let mut fb = b.build_fn(mb).unwrap();
            let x = fb.input();
            // A body-local literal: the bulk op that makes mb a Twin.
            let arr = fb.pack_array(&[x, x], Dest::Fresh(None), L).unwrap();
            let u = fb.map(m1, arr, Dest::Fresh(None), L).unwrap();
            let e = fb.enumerate(u, Dest::Fresh(None), L).unwrap();
            let z = fb.constant(Value::I32(0), L).unwrap();
            let p = fb.index(e, z, Dest::Fresh(None), L).unwrap();
            let r = fb.proj(p, 0, Dest::Fresh(None), L).unwrap();
            fb.output(r, None, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = seal_main_over(b, mb);
        let cu = crate::emit(&ir).unwrap();
        let twin = twin_slice(&cu);
        assert!(
            twin.contains("for (unsigned long long") && twin.contains("< 2ULL;"),
            "{twin}"
        );
        // The index store survives: `{tgt}[{iv}] = (int32_t){iv};`.
        assert!(twin.contains("= (int32_t)"), "{twin}");
    }

    // --- F7: the per-thread local-array budget (documented cell) -----------

    /// A map body constructing an `n`-element body-local literal — the twin
    /// materializes it as a per-thread local C array (§2 item 8).
    fn twin_local_array_src(n: usize) -> String {
        let elems = vec!["x"; n].join(", ");
        format!(
            "fn main() {{\n    [1, 2] -> map {{ x -> [{elems}] -> big;\n    big[0] -> ret\n    }} -> rs;\n    rs[0] -> println;\n}}\n"
        )
    }

    /// A launch-form fold with an `n`-element array acc — the single-thread
    /// fold kernel copies it into a per-thread local (BC4).
    fn fold_acc_array_src(n: usize) -> String {
        let elems = vec!["0"; n].join(", ");
        format!(
            "fn main() {{\n    ([{elems}], [[1, 2], [3, 4]]) -> fold {{ acc, row -> acc }} -> r;\n    r[0] -> println;\n}}\n"
        )
    }

    #[test]
    fn twin_local_array_at_budget_emits_over_is_unsupported() {
        // 4096 i32 = 16384 B: exactly at the budget — emits.
        let cu = crate::emit(&lower_src(&twin_local_array_src(4096))).unwrap();
        assert!(cu.contains("int32_t o"), "{cu}");
        // 4097 i32 = 16388 B: over — the documented Unsupported cell.
        let e = crate::emit(&lower_src(&twin_local_array_src(4097))).unwrap_err();
        assert!(
            matches!(
                e,
                EmitError::Unsupported { ref feature, .. }
                    if feature == "per-thread local array over 16384 bytes (16388 bytes)"
            ),
            "expected the F7 cell, got {e:?}"
        );
    }

    #[test]
    fn fold_kernel_acc_at_budget_emits_over_is_unsupported() {
        // 4096 i32 = 16384 B: exactly at the budget — emits (the kernel
        // carries the acc in per-thread local storage).
        let cu = crate::emit(&lower_src(&fold_acc_array_src(4096))).unwrap();
        assert!(cu.contains("int32_t acc[4096];"), "{cu}");
        // 4097 i32 = 16388 B: over — the documented Unsupported cell.
        let e = crate::emit(&lower_src(&fold_acc_array_src(4097))).unwrap_err();
        assert!(
            matches!(
                e,
                EmitError::Unsupported { ref feature, .. }
                    if feature == "per-thread local array over 16384 bytes (16388 bytes)"
            ),
            "expected the F7 cell, got {e:?}"
        );
    }

    // --- ADR-0027: captures — map/fold bodies read enclosing bindings ------

    /// The one fn of `kind` in the module.
    fn fn_of_kind(ir: &CategoryIr, kind: FuncKind) -> FuncId {
        ir.funcs()
            .find(|(_, fd)| fd.kind == kind)
            .map(|(id, _)| id)
            .expect("a fn of the kind")
    }

    #[test]
    fn capturing_map_threads_capture_through_kernel_launch_and_body_call() {
        // ADR-0027 launch form: the captured scalar is an extra kernel
        // parameter (positionally after the existing operands, before the
        // trap pointer); the launch passes the source product's capture
        // component; the per-thread body call gets it as the leading
        // body-input field.
        let src = "fn main() {\n    3 -> scale;\n    [1, 2, 3] -> a;\n    \
                   a -> map { x -> x * scale } -> b;\n    b[1] -> println;\n}\n";
        let ir = lower_src(src);
        // The F3 cell must not trip: the captured source product is ordinary
        // dataflow (the transient-aggregate exemption covers it), and the
        // pure-scalar body classifies HostDevice exactly as before (BC8 iv).
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(
            quals.get(fn_of_kind(&ir, FuncKind::MapBody)),
            FnQual::HostDevice
        );
        let cu = crate::emit(&ir).unwrap();
        // The kernel's extra parameter, after `in`. The body (`x * scale`)
        // is trap-free (#14): no trailing trap parameter.
        assert!(
            cu.contains("__global__ void k0_0(int32_t* out, int32_t* in, int32_t cap0) {"),
            "{cu}"
        );
        // The body call's capture argument: the leading field of the
        // assembled `(scale, x)` body input.
        assert!(cu.contains("pair.f0 = cap0;"), "{cu}");
        assert!(cu.contains("pair.f1 = in[i];"), "{cu}");
        assert!(cu.contains("out[i] = fn1(pair);"), "{cu}");
        // The launch's extra argument (the constant capture folds to `3`),
        // after the `in` operand — no `d_trap` (trap-free site, #14).
        assert!(
            cu.contains("k0_0<<<(unsigned int)((3ULL + 255ULL) / 256ULL), 256>>>(o4, o2, 3);"),
            "{cu}"
        );
    }

    #[test]
    fn capturing_fold_threads_capture_through_kernel_launch_and_body_call() {
        // The Fold sibling: source `(base, acc, [T; n])` with the acc shifted
        // to component 1 — same parameter/launch/body-call threading.
        let src = "fn main() {\n    10 -> base;\n    [1, 2, 3] -> a;\n    \
                   (0, a) -> fold { acc, x -> acc + x * base } -> r;\n    r -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(
            quals.get(fn_of_kind(&ir, FuncKind::FoldBody)),
            FnQual::HostDevice
        );
        let cu = crate::emit(&ir).unwrap();
        // The body (`acc + x * base`) is trap-free (#14): no trap parameter.
        assert!(
            cu.contains(
                "__global__ void k0_0(int32_t* result, int32_t acc0, int32_t* arr, int32_t cap0) {"
            ),
            "{cu}"
        );
        // The `(base, acc, x)` assembly: the capture leads, then the acc
        // local, then the element — the ADR-0027 body-input shape.
        assert!(cu.contains("pair.f0 = cap0;"), "{cu}");
        assert!(cu.contains("pair.f1 = acc;"), "{cu}");
        assert!(cu.contains("pair.f2 = arr[i];"), "{cu}");
        assert!(cu.contains("acc = fn1(pair);"), "{cu}");
        // WP-C: the fold seed (a constant) inlines into the launch arg.
        assert!(cu.contains("k0_0<<<1, 1>>>(t1, 0, o5, 10);"), "{cu}");
    }

    #[test]
    fn capturing_map_with_bulk_body_classifies_twin_and_inlines_captures() {
        // ADR-0027 Q3 (transitive captures) + the inline form: the outer map
        // body contains a capturing fold (a launch-form op) ⇒ the outer body
        // classifies Twin exactly as before (captures don't change BC8); the
        // inner fold's per-thread loop passes the through-captured scalar as
        // the leading body-input field; the inner fold's own body stays a
        // pure-scalar HostDevice. The F3 cell stays silent over the captured
        // wiring (the `emit` unwraps).
        let src = "fn main() {\n    10 -> scale;\n    [[1, 2], [3, 4]] -> m;\n    \
                   m -> map { row -> (0, row) -> fold { acc, x -> acc + x * scale } } -> rs;\n    \
                   rs[1] -> println;\n}\n";
        let ir = lower_src(src);
        let quals = Qualifiers::analyze(&ir);
        assert_eq!(quals.get(fn_of_kind(&ir, FuncKind::MapBody)), FnQual::Twin);
        assert_eq!(
            quals.get(fn_of_kind(&ir, FuncKind::FoldBody)),
            FnQual::HostDevice
        );
        let cu = crate::emit(&ir).unwrap();
        // The outer kernel: the capture is the extra parameter and the
        // leading body-input field; the element is the sub-array pointer.
        // Every body in the chain is trap-free (#14): no trap parameters,
        // no trap arguments, no per-step checks.
        assert!(
            cu.contains("__global__ void k0_0(int32_t* out, int32_t* in, int32_t cap0) {"),
            "{cu}"
        );
        assert!(cu.contains("pair.f0 = cap0;"), "{cu}");
        assert!(cu.contains("pair.f1 = (in + (i * 2ULL));"), "{cu}");
        assert!(cu.contains("out[i] = d_fn2(pair);"), "{cu}");
        assert!(
            cu.contains("k0_0<<<(unsigned int)((2ULL + 255ULL) / 256ULL), 256>>>(o6, o4, 10);"),
            "{cu}"
        );
        // The twin's per-thread fold loop: the through-captured `scale` is a
        // per-thread read of the twin's input field, leading the inner fold
        // body's `(scale, acc, x)` argument.
        let twin = twin_slice(&cu);
        assert!(
            twin.contains("for (unsigned long long t1 = 0; t1 < 2ULL; t1++) {"),
            "{twin}"
        );
        // WP-B (R-NODUP): the through-captured scalar is read through the
        // Named fold-operand product's field — one reference, no extraction
        // local, no re-computation (the per-iteration re-read collapses at
        // WP-D hoisting).
        assert!(twin.contains("pair.f0 = o7.f0;"), "{twin}");
        assert!(twin.contains("pair.f1 = t0;"), "{twin}");
        assert!(twin.contains("pair.f2 = o6[t1];"), "{twin}");
        assert!(twin.contains("t0 = fn1(pair);"), "{twin}");
    }
}
