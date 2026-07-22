//! `FnEmit` (DESIGN §1 op table, **host column**): one host C++ function per
//! `FuncDef` — hoisted value locals, the `topo_order` walk, and the host op
//! table. Scalars and products materialize as plain C++ locals (`o{ord}` by
//! object ordinal, temps `t{n}` by rising counter — the llvm `f{i}o{j}`
//! scheme re-spelled); every morphism's result is materialized into its
//! object's local (SSA-ish, the llvm slot discipline). A slot name is just a
//! host variable, so device-handle locals (`T*`) ride the same map
//! unchanged.
//!
//! WP3 (kernel.rs's host half, live here): array construction sites emit the
//! §2 literal upload (host data array + one H→D memcpy); array-bulk op sites
//! emit the buffer allocation + kernel launch + `trap_check_after_launch()`
//! where the site can trap (§3 + #14's trim — provably trap-free launches
//! pass no flag and skip the readback), with `Index`/`Fold` scalar results
//! read back D→H. **The buffer allocation is #18's arena form (v1.0):** one
//! FnScope zone per fn ([`crate::arena`]) covers every non-loop-cone site —
//! one arena `cudaMalloc` at fn entry, per-site `arena0 + OFF` pointer inits
//! at the old malloc points, one zone release at fn exit under the
//! range-test escape veto — while loop-cone sites keep the per-buffer
//! `cudaMalloc` + allocation-registry entry (the single choke point is
//! [`FnEmit::alloc_buffer`]).
//! [`FnQual::HostDevice`] fns emit a single `__host__ __device__` definition
//! (BC8 iv): the signature threads the trap pointer (when the fn can trap,
//! #14) and the `Div`/`Mod` zero guard is `#ifdef __CUDA_ARCH__`-split —
//! both guards fold for literal non-zero constant divisors (#13).
//!
//! WP4 (loops): the walk skips driver-owned morphisms — `loop_plan`
//! decide∪advance membership ∪ SCC incidence (DESIGN §1's walk-skip
//! paragraph, the llvm rule `func.rs:252–290` carried verbatim) — and each
//! `LoopEnter` delegates to [`crate::loops::emit_loop`], the guard-first
//! host quartet. Cone emissions reuse this op table unchanged: launches
//! inside a loop iterate with the loop, and a carried array's merge is a
//! host handle whose back edge is a pointer swap (§2). Two consequences
//! live here: every device-pointer local is `nullptr`-initialized (a
//! cone-side allocation site may not execute on a zero-iteration loop, so
//! the free path must never read an indeterminate handle), and an
//! array-returning fn's escape guard compares pointer VALUES — per buffer
//! `if (buf != ret)` for registered (cone-site) buffers, and the zone
//! release's `escaped0` range test for arena members — the loop-exit escape
//! reaches the return through the exit copy, not an `Output`, so the
//! registry's name-based removal cannot see it (the buffer equal to the
//! returned handle escapes; the duty transfers to the caller, §2's
//! invariants preserved).
//!
//! #19a ([`FnEmit::perf`], suggestions.md #19 step a): with `perf_timing`
//! set, every launch is wrapped in CUDA events (`fev{i}_start/stop`, one
//! fn-scope pair per launch site, created once) — `Record(start)` before
//! the launch, `Record(stop)`+`Synchronize`+`ElapsedTime` after (the stop
//! BEFORE the trap check) — printing `FLOW_PERF launch=<kernel> ms=` per
//! execution, plus `FLOW_PERF total ms=` at fn end. Default off: the text
//! is byte-identical to the pre-options emitter.

use flow_ir::{
    CategoryIr, FuncId, LastUsePlan, LoopPlan, MorphismId, ObjectId, ObjectKind, Operation, Ty,
    Value,
};
use slotmap::SecondaryMap;
use std::collections::HashMap;

use crate::EmitError;
use crate::arena::{self, ArenaKey};
use crate::kernel::{self, FnQual, Qualifiers};
use crate::module::{StrGlobal, print_dispatch};
use crate::ty::{erased_index, lower_ty, residual_arity};

/// Per-function emission state (DESIGN Dat `FnCtx`). `slots` is partial —
/// erased (Unit/IoToken/Str) objects and constants have no slot (constants
/// fold into use sites as literals).
pub(crate) struct FnEmit<'a> {
    pub ir: &'a CategoryIr,
    pub f: FuncId,
    pub fnames: &'a SecondaryMap<FuncId, String>,
    pub strings: &'a SecondaryMap<ObjectId, StrGlobal>,
    /// The module's BC8 qualifier analysis (kernel.rs) and this fn's own
    /// case — `HostDevice` fns emit one `__host__ __device__` definition
    /// (trap-threading signature, `#ifdef __CUDA_ARCH__` Div/Mod guards).
    pub quals: &'a Qualifiers,
    pub qual: FnQual,
    /// The module's trap-capability pre-pass (kernel.rs, #14): trap-free
    /// kernels drop the trap parameter/launch arg/readback, and calls to
    /// trap-free `HostDevice` fns pass no `d_trap`.
    pub caps: &'a kernel::TrapCaps,
    /// Launch-form bulk-op sites: morphism → the launch's kernel name. The
    /// name is the site's dedup SURVIVOR (`kernel::emit_kernel_set`, #17):
    /// a site whose structural shape was already emitted launches the first
    /// occurrence's definition, not a `k{f_ord}_{s_ord}` of its own.
    pub sites: HashMap<MorphismId, String>,
    /// Array-literal sites: total Pair in-edges per array object and how many
    /// the walk has passed (construction emits at the last one — see
    /// kernel::literal_pair_counts).
    pub lit_total: SecondaryMap<ObjectId, usize>,
    pub lit_seen: SecondaryMap<ObjectId, usize>,
    pub slots: SecondaryMap<ObjectId, String>,
    /// Hoisted local declarations (fn top), one per materialized object —
    /// plus the fn-scope readback temps the WP3 `Index`/`Fold` sites hoist
    /// here during the walk (a cone-side temp must be fn-scope so the free
    /// at fn exit can name it).
    pub decls: String,
    pub body: String,
    pub next: u32,
    /// Base indent of `line` in 2-space levels: 1 at fn top level, raised to
    /// 2 by the loop driver (loops.rs) around the decide/advance cones.
    pub indent: usize,
    /// The allocation registry (DESIGN §2, allocation-based ownership): every
    /// device buffer THIS fn `cudaMalloc`'d, in allocation order — freed at
    /// fn exit by [`FnEmit::emit_frees`]. Parameters are borrowed (never
    /// registered, never freed here); `Output` removes an escaping buffer
    /// (the free duty transfers to the caller). **Arena members are not
    /// registered** (they are freed with their zone, not per buffer) — only
    /// loop-cone sites' per-buffer mallocs push here in v1.0.
    pub allocs: Vec<String>,
    /// The fn's arena plan (suggestions.md #18, plan-smart-arenas v1.0):
    /// one FnScope zone covering the fn's non-loop-cone buffer sites.
    /// Computed at [`FnEmit::emit`] start (deduced, never stored — the
    /// capacity guard is rule 4's compile-time `Unsupported`); `None` when
    /// the fn has no zone members.
    pub arena: Option<arena::ArenaPlan>,
    /// The fn's last-use plan (plan-last-use §2, the BL7 deduced query —
    /// computed once at construction, alongside the arena plan): the single
    /// source of dead/escape/carried facts for the in-place `Update`
    /// ([`in_place_update`], rule 4) and the back-edge freeing
    /// (`loops.rs`, suggestion #2). Representation-only: values, evaluation
    /// order, and trap behavior are unchanged.
    pub last_use: LastUsePlan,
    /// Kernel-time instrumentation (suggestions.md #19a): when set, every
    /// launch is wrapped in CUDA events (`FLOW_PERF launch=` per execution,
    /// `FLOW_PERF total ms=` at fn end). Default off — the text is then
    /// byte-identical to the pre-options emitter. Set post-construction by
    /// the [`crate::emit_with_opts`] path (keeps `new` at its W1 arity).
    pub perf: bool,
    /// Launch-site morphism → the site's event-pair ordinal (`fev{i}_start`
    /// / `fev{i}_stop`), in `collect_sites` order (deterministic). Populated
    /// at [`FnEmit::emit`] start when `perf` is set; lookup-only (L2).
    pub ev_ord: HashMap<MorphismId, usize>,
    /// Minimal-emission classification (plan-minimal-emission WP-C — the
    /// host/`__host__ __device__` lane of the same mechanism as `DevEmit`).
    pub plan: flow_ir::EmissionPlan,
    /// Backend-forced Named on top of the plan: `Call` targets (host callees
    /// trap via `flow_trap` inside — call position is semantic), every
    /// bulk-op target (launch/readback machinery needs an lvalue local),
    /// and product-typed Inline (local-name fallback).
    pub force_named: SecondaryMap<ObjectId, ()>,
    /// Memoized expressions for Inline/Dissolved values (see `DevEmit`).
    pub exprs: SecondaryMap<ObjectId, String>,
}

impl<'a> FnEmit<'a> {
    pub fn new(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        quals: &'a Qualifiers,
        caps: &'a kernel::TrapCaps,
        kernels: &'a kernel::KernelSet,
    ) -> Self {
        let sites = kernels.names.get(f).cloned().unwrap_or_default();
        let plan = ir.emission_plan(f);
        let mut force_named: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let fd = ir.func(f).expect("func resolves");
        for &m in &fd.morphisms {
            let morph = ir.morphism(m).expect("morphism resolves");
            match morph.op {
                Operation::Call(_)
                | Operation::Map { .. }
                | Operation::Zip
                | Operation::Enumerate
                | Operation::Iota
                | Operation::Fill
                | Operation::Fold { .. }
                | Operation::Index
                | Operation::Update => {
                    force_named.insert(morph.target, ());
                }
                _ => {}
            }
        }
        for (id, obj) in ir.objects() {
            if ir.try_owner(id) == Some(f)
                && obj.ty.product_arity().is_some()
                && plan.class(id).is_some_and(|c| c.is_inline())
            {
                force_named.insert(id, ());
            }
        }
        FnEmit {
            ir,
            f,
            fnames,
            strings,
            quals,
            qual: quals.get(f),
            caps,
            sites,
            lit_total: kernel::literal_pair_counts(ir, f),
            lit_seen: SecondaryMap::new(),
            slots: SecondaryMap::new(),
            decls: String::new(),
            body: String::new(),
            next: 0,
            indent: 1,
            allocs: Vec::new(),
            arena: None,
            last_use: ir.last_use_plan(f),
            perf: false,
            ev_ord: HashMap::new(),
            plan,
            force_named,
            exprs: SecondaryMap::new(),
        }
    }

    /// The effective class (WP-C): deduced plan, overridden Named where the
    /// host statement protocol demands it.
    fn cls(&self, o: ObjectId) -> flow_ir::EmissionClass {
        if self.force_named.contains_key(o) {
            return flow_ir::EmissionClass::Named;
        }
        self.plan.class(o).unwrap_or(flow_ir::EmissionClass::Named)
    }

    fn dissolved(&self, o: ObjectId) -> bool {
        self.cls(o).is_dissolved()
    }

    fn expr_only(&self, o: ObjectId) -> bool {
        !self.cls(o).is_named()
    }

    fn fresh(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    /// A fresh temporary name (`t{n}`).
    fn tmp(&mut self) -> String {
        format!("t{}", self.fresh())
    }

    /// Append one body line at the current indent level.
    pub(crate) fn line(&mut self, s: impl AsRef<str>) {
        for _ in 0..self.indent {
            self.body.push_str("  ");
        }
        self.body.push_str(s.as_ref());
        self.body.push('\n');
    }

    /// Append one body line one level deeper (inside an `if`/`else` block).
    fn line4(&mut self, s: impl AsRef<str>) {
        for _ in 0..=self.indent {
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

    // --- operand materialization -----------------------------------------

    /// The whole value of object `o` as a C++ expression: a literal for a
    /// (scalar) constant, the local's name otherwise. `None` if `o` is
    /// erased / a `Str` / a non-scalar constant (arrays are WP3).
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

    /// Component `k` of aggregate `agg` as a C++ (lvalue) expression, under
    /// the residual-erasure remap: residual-1 products are **bare** (the
    /// local IS the component); residual ≥ 2 products use the
    /// `.f{erased_index}` field. `None` if the component is erased or `agg`
    /// has no slot. Arrays have no component expression yet (WP3).
    fn component_expr(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let agg_ty = self.obj_ty(agg);
        match &agg_ty {
            Ty::Tuple(_) | Ty::Struct { .. } => {
                let comp_ty = agg_ty.component_ty(k)?.clone();
                let cct = lower_ty(&comp_ty)?; // erased component ⇒ None
                // WP-C: a dissolved product resolves through its Pair edge
                // to the field source's own expression.
                if self.dissolved(agg) {
                    let src = kernel::pair_source(self.ir, agg, k)?;
                    let (_, val) = self.load_whole(src)?;
                    return Some((cct, val));
                }
                let agg_slot = self.slot(agg)?;
                if residual_arity(&agg_ty) == 1 {
                    Some((cct, agg_slot)) // bare: the local IS the component
                } else {
                    let eidx = erased_index(&agg_ty, k)?;
                    Some((cct, format!("{agg_slot}.f{eidx}")))
                }
            }
            _ => None,
        }
    }

    /// Route a produced value (WP-C): Named takes its one statement
    /// assignment; Inline memoizes the (parenthesized) expression.
    fn store_obj(&mut self, o: ObjectId, expr: &str) {
        if self.expr_only(o) {
            let wrapped = if kernel::is_atomic_expr(expr) {
                expr.to_string()
            } else {
                format!("({expr})")
            };
            self.exprs.insert(o, wrapped);
            return;
        }
        if let Some(slot) = self.slot(o) {
            self.line(format!("{slot} = {expr};"));
        }
    }

    // --- loop copies (the loops.rs quartet's moves; llvm's names kept) ----

    /// Copy the whole value of `from` into `to`'s local (loop init → merge;
    /// a value copy for scalars/products, a pointer copy for a carried
    /// array handle). No-op if erased.
    pub(crate) fn copy_obj(&mut self, from: ObjectId, to: ObjectId) {
        if let Some((_, val)) = self.load_whole(from) {
            self.store_obj(to, &val);
        }
    }

    /// Copy component `k` of aggregate `route` into object `to`'s local
    /// (the back edge: `back_route` slot 0 → merge — a pointer swap for a
    /// carried array, §2; and the exit copy: `exit_route` slot 0 → exit
    /// object). No-op if erased.
    pub(crate) fn copy_component(&mut self, route: ObjectId, k: u32, to: ObjectId) {
        if let Some((_, val)) = self.component_expr(route, k) {
            self.store_obj(to, &val);
        }
    }

    /// Component `k` of `route` as a bare lvalue expression (the loop guard
    /// bool — `exit_route` slot 1).
    pub(crate) fn route_component(&mut self, route: ObjectId, k: u32) -> String {
        self.component_expr(route, k).expect("route component").1
    }

    /// Back-edge freeing (suggestions.md #2; plan-last-use §3's cuda row):
    /// at the back edge, free the merge's outgoing array buffers whose
    /// next-instance producer is a registered allocation — but only when the
    /// last-use plan proves the carried state dead past the swap
    /// (`dead_after(merge, position(LoopBack))`: rule 1's ranking puts every
    /// body use of the merge at/before the back edge, the merge does not
    /// escape, and it is not carried) and the init's component is not
    /// borrowed (where the plan says borrowed/escape, no free is emitted —
    /// the conservative default is today's accumulate-to-fn-exit). Emits
    /// nothing per component that fails either clause: an in-placed update
    /// target (not registered — the swap is an identity there), the
    /// unchanged-carried merge, an arena member, or a call result.
    ///
    /// Each free rides the pointer-VALUE guard `if (merge.fE != init.fE)` —
    /// the same comparison class as the fn-exit escape guard (which stays as
    /// the second line of defense): on the first iteration the merge still
    /// holds the init's buffer (an arena member or a borrowed handle — never
    /// the producer's registered buffer), so the guard skips it; later
    /// iterations hold the previous iteration's producer buffer — dead past
    /// the swap — freed here instead of leaking to fn exit. The producer's
    /// registry entry is untouched: the exit iteration's final instance is
    /// freed at fn exit under the escape value guard, as today.
    pub(crate) fn emit_back_edge_frees(&mut self, plan: &LoopPlan) {
        let back_pos = self
            .ir
            .in_edges(plan.merge)
            .iter()
            .filter(|&&m| self.ir.morphism(m).expect("morphism resolves").op == Operation::LoopBack)
            .filter_map(|&m| self.last_use.position(m))
            .min();
        let Some(back_pos) = back_pos else { return };
        if !self.last_use.dead_after(plan.merge, back_pos) {
            return; // the plan can't prove the state dead past the swap
        }
        let Some(state) = self.pair_source(plan.back_route, 0) else {
            return;
        };
        let merge_ty = self.obj_ty(plan.merge);
        let bare = matches!(merge_ty, Ty::Array { .. });
        for k in 0.. {
            // The component's next-instance producer (packed into the
            // back-route state), the init's matching component (the borrowed
            // veto), and the two guard lvalues.
            let (comp_array, producer, init_comp, merge_lv, init_lv) = if bare {
                if k > 0 {
                    break;
                }
                (
                    true,
                    state,
                    plan.init,
                    self.slot(plan.merge),
                    self.slot(plan.init),
                )
            } else {
                let Some(comp_ty) = merge_ty.component_ty(k) else {
                    break;
                };
                let array = matches!(comp_ty, Ty::Array { .. });
                let producer = self.pair_source(state, k);
                let init_comp = self.pair_source(plan.init, k);
                let merge_lv = self.component_expr(plan.merge, k).map(|(_, lv)| lv);
                let init_lv = self.component_expr(plan.init, k).map(|(_, lv)| lv);
                match (producer, init_comp) {
                    (Some(p), Some(i)) => (array, p, i, merge_lv, init_lv),
                    _ => continue,
                }
            };
            if !comp_array {
                continue; // scalars carry no buffer
            }
            // The registered-allocation clause: the producer must be a
            // per-buffer cone-site malloc (the registry is the compile-time
            // roster of those).
            let Some(producer_slot) = self.slot(producer) else {
                continue;
            };
            if !self.allocs.contains(&producer_slot) {
                continue;
            }
            // The borrowed veto: where the init's component is not a fresh
            // fn-owned buffer (a Parameter, or ptr-resident Proj/Phi/Call
            // provenance — the plan's borrowed/escape clause), emit no free
            // (today's behavior, O(k·n) elsewhere).
            if !fresh_owned_buffer(self.ir, init_comp) {
                continue;
            }
            let (Some(merge_lv), Some(init_lv)) = (merge_lv, init_lv) else {
                continue;
            };
            self.line(format!("if ({merge_lv} != {init_lv}) {{"));
            self.indent += 1;
            self.line(format!(
                "cu_check(cudaFree({merge_lv}), \"cudaFree({merge_lv})\");"
            ));
            self.indent -= 1;
            self.line("}");
        }
    }

    // --- allocation registry (DESIGN §2; WP3 hooks) -----------------------

    /// Record a device buffer this fn allocated (WP3's cudaMalloc sites push
    /// here, in order).
    pub(crate) fn register_alloc(&mut self, name: String) {
        self.allocs.push(name);
    }

    /// Drop a buffer from the registry without freeing — the `Output` escape
    /// move (the caller inherits the free duty). No-op for unregistered
    /// names (all scalars).
    pub(crate) fn remove_alloc(&mut self, name: &str) {
        self.allocs.retain(|a| a != name);
    }

    /// `cudaFree` every buffer still owned by this fn, in allocation order
    /// (the allocator frees at fn exit; escapes were removed by `Output`),
    /// then release the fn's arena zone. Emits nothing when the registry is
    /// empty and the fn has no zone.
    ///
    /// When the fn **returns a value with array components**, each free is
    /// value-guarded against EVERY array-typed component of the return value
    /// (WP4 + F2): a loop-carried buffer escapes through the loop's exit
    /// copy into the Return, and a locally-allocated buffer escapes as a
    /// returned struct's field — neither passes through an `Output`, so the
    /// registry's name-based removal cannot see it. The guard compares by
    /// pointer equality against [`escape_lvalues`] — the return local itself
    /// for a bare array return, `ret.f{e}` (recursively) for products — so
    /// the Phi/alias escape shapes are subsumed too: whichever buffer the
    /// return value points at is the escaping one, freed by the caller;
    /// every other allocation is freed here (§2's no-double-free /
    /// no-use-after-free invariants, by pointer value). Pointer locals are
    /// `nullptr`-initialized, so the guard never reads an indeterminate
    /// handle (a cone-side site may not have executed).
    ///
    /// **The zone release (plan-smart-arenas rule 3).** The arena is freed
    /// once, under the range-test veto: `escaped0` is the disjunction of the
    /// escape lvalues' pointer-range tests (`(char*)e >= (char*)arena0 &&
    /// (char*)e < (char*)arena0 + CAP`) — the same pointer-VALUE comparison
    /// class as the per-buffer guard, at zone granularity: an escaping
    /// buffer pins its whole zone (the caller inherits the bounded-leak
    /// duty, DESIGN §2 amendment (ii)'s shape). Cone-site buffers are not
    /// zone members; their per-buffer frees above are unchanged.
    fn emit_frees(&mut self) {
        let fd = self.ir.func(self.f).expect("func resolves");
        let out_ty = self
            .ir
            .object(fd.output)
            .expect("output resolves")
            .ty
            .clone();
        let escapes: Vec<String> = match self.slot(fd.output) {
            Some(ret) => escape_lvalues(&out_ty, &ret),
            None => Vec::new(),
        };
        for name in std::mem::take(&mut self.allocs) {
            if escapes.is_empty() {
                self.line(format!("cu_check(cudaFree({name}), \"cudaFree({name})\");"));
            } else {
                let cond = escapes
                    .iter()
                    .map(|e| format!("{name} != {e}"))
                    .collect::<Vec<_>>()
                    .join(" && ");
                self.line(format!("if ({cond}) {{"));
                self.line4(format!("cu_check(cudaFree({name}), \"cudaFree({name})\");"));
                self.line("}");
            }
        }
        let arena_cap = self.arena.as_ref().map(|p| p.capacity);
        if let Some(cap) = arena_cap {
            if escapes.is_empty() {
                self.line("cu_check(cudaFree(arena0), \"cudaFree(arena0)\");");
            } else {
                let tests = escapes
                    .iter()
                    .map(|e| {
                        format!(
                            "((char*){e} >= (char*)arena0 && (char*){e} < (char*)arena0 + {cap}ULL)"
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" || ");
                self.line(format!("bool escaped0 = {tests};"));
                self.line("if (!escaped0) {");
                self.line4("cu_check(cudaFree(arena0), \"cudaFree(arena0)\");");
                self.line("}");
            }
        }
    }

    // --- the walk ---------------------------------------------------------

    /// Emit the function: declarations, prologue, the topo walk, epilogue.
    pub fn emit(mut self) -> Result<String, EmitError> {
        let fd = self.ir.func(self.f).expect("func resolves");
        let in_ty = self.obj_ty(fd.input);
        let ret_ty = self.obj_ty(fd.output);

        // The arena plan (suggestions.md #18, plan-smart-arenas v1.0) —
        // deduced from the sealed graph, never stored; over ARENA_MAX_BYTES
        // is rule 4's compile-time Unsupported (the F7 precedent). Always
        // `None` for a HostDevice fn (the qualifier rule keeps launch-form
        // ops out of it), so no cudaMalloc ever reaches a __device__ pass.
        self.arena = arena::arena_plan(self.ir, self.f)?;

        // Hoisted declarations: one local per materialized (non-constant,
        // non-erased) object, `o{ord}` in deterministic object order — the
        // llvm ordinal scheme: constants consume no ordinal, erased
        // non-constants do. Device-handle locals (`T*`) are
        // `nullptr`-initialized: a loop-cone allocation site may not execute
        // (zero-iteration loop), and the free path must never read an
        // indeterminate handle.
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
            // R-ONENAME (WP-C): the parameter IS a variable — alias `in`,
            // no declaration, no prologue copy.
            if *id == fd.input && lower_ty(ty).is_some() {
                self.slots.insert(*id, "in".to_string());
                ord += 1;
                continue;
            }
            // Inline/Dissolved values own no local (arrays stay Named).
            if !matches!(ty, Ty::Array { .. }) && lower_ty(ty).is_some() && self.expr_only(*id) {
                ord += 1;
                continue;
            }
            if let Some(ct) = lower_ty(ty) {
                let name = format!("o{ord}");
                self.slots.insert(*id, name.clone());
                if ct.ends_with('*') {
                    self.decls.push_str(&format!("  {ct} {name} = nullptr;\n"));
                } else {
                    self.decls.push_str(&format!("  {ct} {name};\n"));
                }
            }
            ord += 1;
        }

        // The zone's base handle (plan §3): one arena per fn, malloc'd once
        // at fn entry; members get their `arena0 + OFF` pointer inits at the
        // points where today `malloc_buffer` runs.
        let arena_cap = self.arena.as_ref().map(|p| p.capacity);
        if arena_cap.is_some() {
            self.decls.push_str("  char* arena0 = nullptr;\n");
        }

        // #19a (perf_timing): one event pair per launch site (collect_sites
        // order — deterministic), created once per fn invocation; the
        // accumulators ride the fn as plain locals. Empty when off, so the
        // default text is byte-identical.
        if self.perf {
            for (i, site) in kernel::collect_sites(self.ir, self.f)
                .into_iter()
                .enumerate()
            {
                self.ev_ord.insert(site.m, i);
            }
        }
        let ev_count = self.ev_ord.len();
        if ev_count > 0 {
            for i in 0..ev_count {
                self.decls
                    .push_str(&format!("  cudaEvent_t fev{i}_start, fev{i}_stop;\n"));
            }
            self.decls
                .push_str("  float flow_perf_total = 0.0f;\n  float flow_perf_ms = 0.0f;\n");
        }

        // Prologue: the arena malloc, the event creates, then the parameter
        // moves into the input object's local.
        if let Some(cap) = arena_cap {
            self.line(format!(
                "cu_check(cudaMalloc((void**)&arena0, {cap}ULL), \"cudaMalloc(arena0)\");"
            ));
        }
        for i in 0..ev_count {
            self.line(format!(
                "cu_check(cudaEventCreate(&fev{i}_start), \"cudaEventCreate\");"
            ));
            self.line(format!(
                "cu_check(cudaEventCreate(&fev{i}_stop), \"cudaEventCreate\");"
            ));
        }
        // No `{o0} = in;` prologue copy — the input aliases `in` (WP-C).
        let _ = &in_ty;

        // The topo walk (DESIGN §1: one host statement per morphism; the
        // driver-ownership skip routes loop bodies to loops.rs's quartet).
        self.walk()?;

        // Epilogue: the FLOW_PERF total + event destroys (#19a), then free
        // this fn's device buffers (value-guarded for an array return — see
        // emit_frees), then return the output local.
        if ev_count > 0 {
            self.line("printf(\"FLOW_PERF total ms=%.4f\\n\", flow_perf_total);");
            for i in 0..ev_count {
                self.line(format!(
                    "cu_check(cudaEventDestroy(fev{i}_start), \"cudaEventDestroy\");"
                ));
                self.line(format!(
                    "cu_check(cudaEventDestroy(fev{i}_stop), \"cudaEventDestroy\");"
                ));
            }
        }
        self.emit_frees();
        match lower_ty(&ret_ty) {
            Some(_) => {
                let os = self.slot(fd.output).expect("non-void return has a slot");
                self.line(format!("return {os};"));
            }
            None => {
                self.line("return;");
            }
        }

        let sig = fn_signature(self.ir, self.f, self.fnames, self.qual, self.caps);
        Ok(format!("{sig} {{\n{}{}}}\n", self.decls, self.body))
    }

    fn walk(&mut self) -> Result<(), EmitError> {
        // Driver-owned morphisms: everything in a loop plan's decide/advance
        // cones, plus anything incident to an SCC object — DESIGN §1's
        // walk-skip paragraph, the llvm rule (its func.rs:252–290, the skip
        // itself :280–285) carried verbatim. Plan membership is the precise
        // rule: an exit-only payload chain (a computed exit value, an
        // exit-arm Print) leaves the SCC but still belongs to the decide
        // cone; skipping by SCC incidence alone would re-emit it after the
        // loop (dead recompute for values, a DOUBLE side effect for Print).
        // The loop driver emits the decide/advance cones; they emit nowhere
        // else.
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
                Operation::LoopEnter => crate::loops::emit_loop(self, morph.target)?,
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
        Ok(())
    }

    /// Emit one morphism (DESIGN §1 op table, host column). Called by the
    /// straight-line walk and by the loop driver for the decide/advance
    /// cones.
    pub(crate) fn emit_morphism(&mut self, m: MorphismId) -> Result<(), EmitError> {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        let op = morph.op;
        let source = morph.source;
        let target = morph.target;

        match op {
            Operation::Pair { slot, .. } => {
                if matches!(self.obj_ty(target), Ty::Array { .. }) {
                    // Array construction (pack_array literal): emitted once,
                    // at the target's last Pair edge in topo order (§2, BC11).
                    let seen = self.lit_seen.get(target).copied().unwrap_or(0) + 1;
                    self.lit_seen.insert(target, seen);
                    if seen == self.lit_total.get(target).copied().unwrap_or(0) {
                        self.emit_literal(target);
                    }
                } else if self.dissolved(target) {
                    // WP-C: dissolved wrapper — no materialization; consumers
                    // read the field sources through `component_expr`.
                } else if let Some((_, sval)) = self.load_whole(source)
                    && let Some((_, lvalue)) = self.component_expr(target, slot)
                {
                    self.line(format!("{lvalue} = {sval};"));
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
                    // BC2: wrapping_neg via the unsigned twin (INT_MIN neg is UB).
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
                // BC7: `&` / `|` on precomputed operands — never the
                // short-circuiting `&&` / `||`.
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
                // BC7 strict select: both arms are already computed (their
                // producers ran upstream) — the temps pin the llvm `select`
                // shape; a ternary over the arm *expressions* could skip the
                // untaken arm's trap.
                let (tct, t) = self.component_expr(source, 0).expect("phi then");
                let (_, e) = self.component_expr(source, 1).expect("phi else");
                let (_, c) = self.component_expr(source, 2).expect("phi cond");
                let tt = self.tmp();
                let te = self.tmp();
                self.line(format!("{tct} {tt} = {t};"));
                self.line(format!("{tct} {te} = {e};"));
                self.store_obj(target, &format!("{c} ? {tt} : {te}"));
            }
            Operation::Call(g) => self.emit_call(source, target, g),
            Operation::Print { newline } => self.emit_print(source, newline),
            Operation::Output => {
                if let Some((_, val)) = self.load_whole(source) {
                    self.store_obj(target, &val);
                }
                // Allocation-registry escape (DESIGN §2): a returned buffer's
                // free duty transfers to the caller. Inert in WP2.
                if let Some(s) = self.slot(source) {
                    self.remove_alloc(&s);
                }
            }
            Operation::Map { .. }
            | Operation::Zip
            | Operation::Enumerate
            | Operation::Iota
            | Operation::Fill
            | Operation::Fold { .. }
            | Operation::Index
            | Operation::Update => {
                let kname = self.sites.get(&m).expect("bulk site registered").clone();
                let capable = self.caps.site(self.ir, m);
                self.emit_bulk_site(m, &kname, source, target, op, capable);
            }
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
                // IEEE at width; ÷0 is ±inf/NaN, no trap (ADR-0013 S13).
                Operation::Div => format!("{a} / {b}"),
                // fmod — llvm's open `frem` parity question transfers (§4b).
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
                // BC2: unsigned-cast wrapping — C++ signed overflow is UB.
                let uct = unsigned_twin(&opty);
                self.store_obj(target, &format!("({ct})(({uct}){a} {sym} ({uct}){b})"));
            }
            Operation::Div | Operation::Mod => {
                // #13 (kernel.rs's const_int_operand): a literal non-zero
                // constant divisor makes the zero guard dead by construction
                // (oracle behavior identical — it can never fire); a
                // constant ≠ −1 makes the MIN/−1 value guard dead. A literal
                // 0 keeps the guard (it always fires, as the oracle traps).
                let const_div = kernel::const_int_operand(self.ir, source, 1);
                if !matches!(const_div, Some(v) if v != 0) {
                    if self.qual == FnQual::HostDevice {
                        // BC8 (iv): one definition compiled for both sites. The
                        // host pass traps directly; the device pass stores
                        // div_zero (kind+1 ⇒ 1u) in the flag (the `d_trap`
                        // parameter shadows the global — see fn_signature) and
                        // returns (§3).
                        let ret = self.ret_default();
                        self.line(format!("if ({b} == 0) {{"));
                        self.line4("#ifdef __CUDA_ARCH__");
                        self.line4(format!("*d_trap = 1u; {ret}"));
                        self.line4("#else");
                        self.line4("flow_trap(0);");
                        self.line4("#endif");
                        self.line("}");
                    } else {
                        // Zero guard → flow_trap(div_zero) on the host (kind 0).
                        self.line(format!("if ({b} == 0) {{ flow_trap(0); }}"));
                    }
                }
                if signed && !matches!(const_div, Some(v) if v != -1) {
                    // MIN/-1 → defined result (wrapping_div/rem parity):
                    // Div ⇒ MIN, Mod ⇒ 0 — a value guard, NOT a trap. The
                    // unguarded C++ `INT_MIN / -1` would be UB. Guarded ⇒
                    // the target is Named by the query — the slot exists.
                    let slot = self.slot(target).expect("div/mod result slot");
                    let min = int_min(&ct);
                    let sym = if op == Operation::Div { "/" } else { "%" };
                    let ovval = if op == Operation::Div { min } else { "0" };
                    self.line(format!("if (({b} == -1) && ({a} == {min})) {{"));
                    self.line4(format!("{slot} = {ovval};"));
                    self.line("} else {");
                    self.line4(format!("{slot} = {a} {sym} {b};"));
                    self.line("}");
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
        // One C++ operator per op: signedness rides the operand types; float
        // `==`/`!=`/`<`… match llvm's oeq/une/olt/… on unordered operands.
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
        let callee = self.fnames[g].clone();
        let out_ty = self.obj_ty(self.ir.func(g).expect("callee").output);
        let mut arg = match self.load_whole(source) {
            Some((_, val)) => val,
            None => String::new(),
        };
        // BC8 (iv): a `__host__ __device__` callee that CAN TRAP (#14)
        // takes the threaded trap pointer; host callers pass the host global
        // (unused on the host pass — its guards call flow_trap directly,
        // §3). A trap-free callee's signature has no trap parameter, so the
        // call passes nothing.
        if self.quals.get(g) == FnQual::HostDevice && self.caps.get(g) {
            arg = if arg.is_empty() {
                "d_trap".to_string()
            } else {
                format!("{arg}, d_trap")
            };
        }
        match lower_ty(&out_ty) {
            None => self.line(format!("{callee}({arg});")),
            Some(_) => self.store_obj(target, &format!("{callee}({arg})")),
        }
    }

    fn emit_print(&mut self, source: ObjectId, newline: bool) {
        let nl = if newline { "true" } else { "false" };
        let pty = self
            .obj_ty(source)
            .component_ty(1)
            .cloned()
            .expect("print printable");
        if pty == Ty::Str {
            // Str comes only from a literal (I9s): the slot-1 Pair source is
            // a Str constant with a private global; explicit length (the C
            // array's NUL terminator is not part of the payload).
            let p_obj = self.pair_source(source, 1).expect("str print source");
            let g = self.strings.get(p_obj).expect("str global");
            let len = g.bytes.len();
            let name = g.name.clone();
            self.line(format!(
                "flow_print_str((const uint8_t*){name}, {len}, {nl});"
            ));
            return;
        }
        let func = print_dispatch(&pty).expect("printable Print operand");
        let (_, val) = self.component_expr(source, 1).expect("print value");
        self.line(format!("{func}({val}, {nl});"));
    }

    /// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`.
    fn pair_source(&self, agg: ObjectId, k: u32) -> Option<ObjectId> {
        kernel::pair_source(self.ir, agg, k)
    }

    /// The guard early-return for this fn's return type (`return {ret}{};` or
    /// `return;`) — used by the `__host__ __device__` Div/Mod guard's device
    /// branch. HostDevice fns never return arrays (the qualifier rule), so
    /// `{ct}{}` is always a scalar/product value-init here.
    fn ret_default(&self) -> String {
        let fd = self.ir.func(self.f).expect("func resolves");
        let out_ty = &self.ir.object(fd.output).expect("output resolves").ty;
        match lower_ty(out_ty) {
            Some(ct) => format!("return {ct}{{}};"),
            None => "return;".to_string(),
        }
    }

    // --- WP3: array-bulk launch sites (kernel.rs owns the __global__ side) -

    /// `cudaMalloc` into the (already-declared) handle `slot` + register it
    /// in the allocation registry (DESIGN §2, allocation-based ownership).
    /// The per-buffer path — loop-cone sites in v1.0 (arena.rs); fn-zone
    /// members go through [`FnEmit::alloc_buffer`]'s pointer init instead.
    fn malloc_buffer(&mut self, slot: &str, bytes: &str) {
        self.line(format!(
            "cu_check(cudaMalloc((void**)&{slot}, {bytes}), \"cudaMalloc({slot})\");"
        ));
        self.register_alloc(slot.to_string());
    }

    /// Allocate one construction site's buffer (plan-smart-arenas §3 — the
    /// single choke point, malloc_buffer's arena form): a fn-zone member's
    /// device address is the compile-time `arena0 + OFF` pointer init (no
    /// `cudaMalloc` at the site, nothing registered — the zone frees it); a
    /// loop-cone site keeps the per-buffer `cudaMalloc` + registry entry
    /// (v1.0). Either way the line lands at exactly the point
    /// `malloc_buffer` ran today, so zero-iteration `nullptr` semantics and
    /// per-iteration re-init are preserved verbatim. `ptr_ct` is the slot's
    /// lowered pointer type (the cast target); `bytes` the per-buffer
    /// path's cudaMalloc byte-count text.
    fn alloc_buffer(&mut self, key: ArenaKey, slot: &str, ptr_ct: &str, bytes: &str) {
        match self.arena.as_ref().and_then(|p| p.offset(key)) {
            Some(off) => self.line(format!("{slot} = ({ptr_ct})(arena0 + {off}ULL);")),
            None => self.malloc_buffer(slot, bytes),
        }
    }

    /// One kernel launch + the §3 after-EVERY-launch trap check — the check
    /// rides only for a trap-CAPABLE site (#14): a trap-free kernel takes no
    /// trap pointer, so the launch passes no `d_trap` argument and no
    /// synchronizing readback follows (fewer syncs — the perf-visible part).
    /// Wherever any guard can fire the check-after-every-launch convention
    /// is kept verbatim (class parity, first-trap-wins).
    ///
    /// With #19a's `perf_timing` the launch is wrapped in CUDA events:
    /// `Record(start)` before it, `Record(stop)` + `Synchronize` +
    /// `ElapsedTime` after — the stop is recorded BEFORE the trap check, so
    /// the §3 convention is unchanged — printing one machine-readable
    /// `FLOW_PERF launch=<kernel> ms=<%.4f>` line per EXECUTION (a cone
    /// launch prints per iteration). The elapsed sync is the only added
    /// host sync, and only where #14 had skipped the readback.
    fn launch_and_check(
        &mut self,
        m: MorphismId,
        kname: &str,
        dims: &str,
        args: &[String],
        capable: bool,
    ) {
        let mut args = args.to_vec();
        if capable {
            args.push("d_trap".into());
        }
        let launch = format!("{kname}<<<{dims}>>>({});", args.join(", "));
        if self.perf {
            let i = *self.ev_ord.get(&m).expect("launch site has an event pair");
            self.line(format!(
                "cu_check(cudaEventRecord(fev{i}_start), \"cudaEventRecord\");"
            ));
            self.line(launch);
            self.line(format!(
                "cu_check(cudaEventRecord(fev{i}_stop), \"cudaEventRecord\");"
            ));
            self.line(format!(
                "cu_check(cudaEventSynchronize(fev{i}_stop), \"cudaEventSynchronize\");"
            ));
            self.line(format!(
                "cu_check(cudaEventElapsedTime(&flow_perf_ms, fev{i}_start, fev{i}_stop), \"cudaEventElapsedTime\");"
            ));
            self.line("flow_perf_total += flow_perf_ms;");
            self.line(format!(
                "printf(\"FLOW_PERF launch={kname} ms=%.4f\\n\", flow_perf_ms);"
            ));
        } else {
            self.line(launch);
        }
        if capable {
            self.line("trap_check_after_launch();");
        }
    }

    /// The host side of one launch-form array-bulk op site: allocate the
    /// output buffer (a fn-zone pointer init, or a per-buffer cudaMalloc for
    /// a cone site — [`FnEmit::alloc_buffer`]), gather operands, launch,
    /// trap-check. Argument order mirrors the kernel's parameter list
    /// positionally (kernel.rs `emit_kernel` is the definitional side);
    /// ADR-0027 captures are the source product's leading components, passed
    /// after the existing operands, before the trap pointer. `capable` is
    /// the site's trap capability (#14, `TrapCaps::site`): it decides the
    /// trailing `d_trap` argument and the post-launch check.
    fn emit_bulk_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        op: Operation,
        capable: bool,
    ) {
        match op {
            Operation::Map { captures, .. } => {
                self.map_site(m, kname, source, target, captures, capable)
            }
            Operation::Zip => self.zip_site(m, kname, source, target, capable),
            Operation::Enumerate => self.enumerate_site(m, kname, source, target, capable),
            Operation::Iota => self.iota_site(m, kname, source, target, capable),
            Operation::Fill => self.fill_site(m, kname, source, target, capable),
            Operation::Update => self.update_site(m, kname, source, target, capable),
            Operation::Index => self.index_site(m, kname, source, target, capable),
            Operation::Fold { captures, .. } => {
                self.fold_site(m, kname, source, target, captures, capable)
            }
            _ => unreachable!("not a bulk-op site"),
        }
    }

    /// The ADR-0027 capture operands of a map/fold site: the source product's
    /// first `captures` components, each as a launch argument (a device
    /// buffer handle for an array capture, a by-value scalar otherwise) —
    /// appended after the site's existing operands, before `d_trap`.
    fn push_capture_args(&mut self, args: &mut Vec<String>, source: ObjectId, captures: u32) {
        for j in 0..captures {
            let cap_obj = self.pair_source(source, j).expect("capture component");
            if let Some((_, v)) = self.load_whole(cap_obj) {
                args.push(v);
            }
        }
    }

    fn map_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        captures: u32,
        capable: bool,
    ) {
        // ADR-0027: k>0 ⇒ the mapped array is the source product's last
        // component; k=0 ⇒ the source IS the array.
        let arr_obj = if captures == 0 {
            source
        } else {
            self.pair_source(source, captures).expect("map array")
        };
        let arr_ty = self.obj_ty(arr_obj);
        let (_, n) = kernel::array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        let mut args: Vec<String> = Vec::new();
        if let Some(ptr_ct) = lower_ty(&tgt_ty) {
            let slot = self.slot(target).expect("map target slot");
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&tgt_ty),
            );
            args.push(slot);
        }
        if lower_ty(&arr_ty).is_some() {
            args.push(self.slot(arr_obj).expect("map source slot"));
        }
        self.push_capture_args(&mut args, source, captures);
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(n)),
            &args,
            capable,
        );
    }

    fn zip_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let src_ty = self.obj_ty(source);
        let a_ty = src_ty.component_ty(0).cloned().expect("zip a");
        let (_, n) = kernel::array_parts(&a_ty);
        let tgt_ty = self.obj_ty(target);
        let mut args: Vec<String> = Vec::new();
        if let Some(ptr_ct) = lower_ty(&tgt_ty) {
            let slot = self.slot(target).expect("zip target slot");
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&tgt_ty),
            );
            args.push(slot);
        }
        for k in 0..2 {
            let arr = self.pair_source(source, k).expect("zip input");
            if let Some(s) = self.slot(arr) {
                args.push(s);
            }
        }
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(n)),
            &args,
            capable,
        );
    }

    fn enumerate_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let src_ty = self.obj_ty(source);
        let (_, n) = kernel::array_parts(&src_ty);
        let tgt_ty = self.obj_ty(target);
        let mut args: Vec<String> = Vec::new();
        if let Some(ptr_ct) = lower_ty(&tgt_ty) {
            let slot = self.slot(target).expect("enumerate target slot");
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&tgt_ty),
            );
            args.push(slot);
        }
        if lower_ty(&src_ty).is_some() {
            args.push(self.slot(source).expect("enumerate source slot"));
        }
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(n)),
            &args,
            capable,
        );
    }

    fn iota_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = kernel::array_parts(&tgt_ty);
        let mut args = Vec::new();
        if let Some(ptr_ct) = lower_ty(&tgt_ty) {
            let slot = self.slot(target).expect("iota target slot");
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&tgt_ty),
            );
            args.push(slot);
        }
        let (_, count) = self.load_whole(source).expect("iota count");
        args.push(count);
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(n)),
            &args,
            capable,
        );
    }

    fn fill_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = kernel::array_parts(&tgt_ty);
        let mut args = Vec::new();
        if let Some(ptr_ct) = lower_ty(&tgt_ty) {
            let slot = self.slot(target).expect("fill target slot");
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&tgt_ty),
            );
            args.push(slot);
        }
        let count = self.pair_source(source, 1).expect("fill count");
        let (_, count) = self.load_whole(count).expect("fill count operand");
        args.push(count);
        let value = self.pair_source(source, 0).expect("fill value");
        if let Some((_, value)) = self.load_whole(value) {
            args.push(value);
        }
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(n)),
            &args,
            capable,
        );
    }

    fn update_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
        let flat = kernel::flat_count(&arr_ty);
        let mut args: Vec<String> = Vec::new();
        if let Some(ptr_ct) = lower_ty(&arr_ty) {
            let slot = self.slot(target).expect("update target slot");
            let src = self.pair_source(source, 0).expect("update src");
            let src_slot = self.slot(src).expect("update src slot");
            if in_place_update(self.ir, &self.last_use, self.f, m, src) {
                // In place (plan-last-use §2 rule 4): the source array dies
                // at this update — no fresh buffer; the target handle IS the
                // source handle. The element-write kernel is race-free on
                // aliased out/src (each thread touches disjoint indices),
                // the bounds guard is unchanged, and the old array value is
                // dead, so the mutation is unobservable. An arena-member
                // site keeps its offset reserved-but-unused (the recorded
                // v1 simplification — capacity is slightly over-reserved).
                self.line(format!("{slot} = {src_slot};"));
            } else {
                self.alloc_buffer(
                    ArenaKey::Obj(target),
                    &slot,
                    &ptr_ct,
                    &kernel::buffer_bytes(&arr_ty),
                );
            }
            args.push(slot);
            args.push(src_slot);
        }
        // §3 width rule: extend the index per its Ty to int64_t (the C++
        // conversion is value-preserving — llvm's zext/sext split realized).
        let idx_obj = self.pair_source(source, 1).expect("update idx");
        let (_, idx) = self.load_whole(idx_obj).expect("update idx operand");
        args.push(kernel::extend_index(&idx));
        let val_obj = self.pair_source(source, 2).expect("update val");
        if lower_ty(&self.obj_ty(val_obj)).is_some() {
            let (_, v) = self.load_whole(val_obj).expect("update val operand");
            args.push(v);
        }
        self.launch_and_check(
            m,
            kname,
            &format!("{}, 256", kernel::grid_expr(flat)),
            &args,
            capable,
        );
    }

    fn index_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        capable: bool,
    ) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("index array");
        let tgt_ty = self.obj_ty(target);
        let arr_obj = self.pair_source(source, 0).expect("index array");
        let idx_obj = self.pair_source(source, 1).expect("index idx");
        let (_, idx) = self.load_whole(idx_obj).expect("index idx operand");
        let mut args: Vec<String> = Vec::new();
        // The result buffer: a 1-cell device buffer for scalar elements (the
        // host memcpy's it D→H below — §2 item 5); an array-typed element
        // lands in a fresh device buffer that IS the target handle (§1).
        let mut cell: Option<String> = None;
        if let Some(ct) = lower_ty(&tgt_ty) {
            if matches!(tgt_ty, Ty::Array { .. }) {
                let slot = self.slot(target).expect("index target slot");
                self.alloc_buffer(
                    ArenaKey::Obj(target),
                    &slot,
                    &ct,
                    &kernel::buffer_bytes(&tgt_ty),
                );
                args.push(slot);
            } else {
                // The 1-cell readback buffer is declared at fn scope
                // (hoisted, nullptr-initialized): in a loop cone the site
                // iterates but the free at fn exit must still name it.
                let t = self.tmp();
                self.decls.push_str(&format!("  {ct}* {t} = nullptr;\n"));
                self.alloc_buffer(
                    ArenaKey::Cell(m),
                    &t,
                    &format!("{ct}*"),
                    &format!("sizeof({ct}) * 1ULL"),
                );
                args.push(t.clone());
                cell = Some(t);
            }
        }
        if lower_ty(&arr_ty).is_some() {
            args.push(self.slot(arr_obj).expect("index array slot"));
        }
        args.push(kernel::extend_index(&idx));
        self.launch_and_check(m, kname, "1, 1", &args, capable);
        if let Some(t) = cell {
            let ct = lower_ty(&tgt_ty).expect("index result lowers");
            let slot = self.slot(target).expect("index target slot");
            self.line(format!(
                "cu_check(cudaMemcpy(&{slot}, {t}, sizeof({ct}), cudaMemcpyDeviceToHost), \"cudaMemcpy(index)\");"
            ));
        }
    }

    fn fold_site(
        &mut self,
        m: MorphismId,
        kname: &str,
        source: ObjectId,
        target: ObjectId,
        captures: u32,
        capable: bool,
    ) {
        let src_ty = self.obj_ty(source);
        // ADR-0027: the source product is `(c₁…cₖ, Acc, [T; n])` — the
        // captures shift the acc to component k, the array to k+1.
        let acc_ty = src_ty.component_ty(captures).cloned().expect("fold acc");
        let arr_ty = src_ty
            .component_ty(captures + 1)
            .cloned()
            .expect("fold array");
        let tgt_ty = self.obj_ty(target);
        let acc_obj = self.pair_source(source, captures).expect("fold seed");
        let arr_obj = self.pair_source(source, captures + 1).expect("fold array");
        let mut args: Vec<String> = Vec::new();
        // Scalar acc ⇒ 1-cell device buffer + D→H readback (§2 item 6); an
        // array acc stays as the result buffer (§1 — the target handle).
        let mut cell: Option<String> = None;
        if let Some(ct) = lower_ty(&tgt_ty) {
            if matches!(acc_ty, Ty::Array { .. }) {
                let slot = self.slot(target).expect("fold target slot");
                self.alloc_buffer(
                    ArenaKey::Obj(target),
                    &slot,
                    &ct,
                    &kernel::buffer_bytes(&acc_ty),
                );
                args.push(slot);
            } else {
                // The 1-cell readback buffer is declared at fn scope
                // (hoisted, nullptr-initialized), like Index's.
                let t = self.tmp();
                self.decls.push_str(&format!("  {ct}* {t} = nullptr;\n"));
                self.alloc_buffer(
                    ArenaKey::Cell(m),
                    &t,
                    &format!("{ct}*"),
                    &format!("sizeof({ct}) * 1ULL"),
                );
                args.push(t.clone());
                cell = Some(t);
            }
        }
        match &acc_ty {
            Ty::Array { .. } => {
                // An erased-element acc (Array{Unit}) has no slot: the seed
                // argument is omitted, matching the kernel's omitted acc0
                // parameter — the launch still runs (F6).
                if let Some(s) = self.slot(acc_obj) {
                    args.push(s);
                }
            }
            _ => {
                if lower_ty(&acc_ty).is_some() {
                    let (_, v) = self.load_whole(acc_obj).expect("fold seed operand");
                    args.push(v);
                }
            }
        }
        if lower_ty(&arr_ty).is_some() {
            args.push(self.slot(arr_obj).expect("fold array slot"));
        }
        self.push_capture_args(&mut args, source, captures);
        self.launch_and_check(m, kname, "1, 1", &args, capable);
        if let Some(t) = cell {
            let ct = lower_ty(&tgt_ty).expect("fold result lowers");
            let slot = self.slot(target).expect("fold target slot");
            self.line(format!(
                "cu_check(cudaMemcpy(&{slot}, {t}, sizeof({ct}), cudaMemcpyDeviceToHost), \"cudaMemcpy(fold)\");"
            ));
        }
    }

    /// An array construction site (§2, BC11): a host data array + one H→D
    /// `cudaMemcpy`, per execution of the construction site. All-constant
    /// elements use a `static const` data array; computed elements a plain
    /// local (same one-memcpy shape); nested elements ride per-element
    /// device-to-device copies from the sub-array handles. The device buffer
    /// itself comes from [`FnEmit::alloc_buffer`] (a fn-zone pointer init,
    /// or a per-buffer cudaMalloc for a cone site).
    fn emit_literal(&mut self, target: ObjectId) {
        let Some(slot) = self.slot(target) else {
            return; // erased element type: no representation
        };
        let arr_ty = self.obj_ty(target);
        let ptr_ct = lower_ty(&arr_ty).expect("literal array lowers");
        let (elem, n) = kernel::array_parts(&arr_ty);

        if matches!(elem, Ty::Array { .. }) {
            self.alloc_buffer(
                ArenaKey::Obj(target),
                &slot,
                &ptr_ct,
                &kernel::buffer_bytes(&arr_ty),
            );
            let m = kernel::flat_count(&elem);
            let base = kernel::flat_base_ct(&arr_ty).expect("literal base lowers");
            for k in 0..n {
                let src = self.pair_source(target, k as u32).expect("literal element");
                if let Some((_, expr)) = self.load_whole(src) {
                    self.line(format!(
                        "cu_check(cudaMemcpy({slot} + {}ULL, {expr}, sizeof({base}) * {m}ULL, cudaMemcpyDeviceToDevice), \"cudaMemcpy(literal)\");",
                        k * m
                    ));
                }
            }
            return;
        }

        let mut elems: Vec<String> = Vec::new();
        let mut all_const = true;
        for k in 0..n {
            let src = self.pair_source(target, k as u32).expect("literal element");
            let is_const = matches!(
                self.ir.object(src).expect("element resolves").kind,
                ObjectKind::Constant
            );
            let (_, expr) = self.load_whole(src).expect("literal element operand");
            elems.push(expr);
            all_const &= is_const;
        }
        let ct = lower_ty(&elem).expect("literal element lowers");
        let data = format!("lit{}", self.fresh());
        let init = elems.join(", ");
        if all_const {
            self.line(format!("static const {ct} {data}[{n}] = {{ {init} }};"));
        } else {
            self.line(format!("{ct} {data}[{n}] = {{ {init} }};"));
        }
        self.alloc_buffer(
            ArenaKey::Obj(target),
            &slot,
            &ptr_ct,
            &format!("sizeof({data})"),
        );
        self.line(format!(
            "cu_check(cudaMemcpy({slot}, {data}, sizeof({data}), cudaMemcpyHostToDevice), \"cudaMemcpy(literal)\");"
        ));
    }
}

/// The C++ function signature text (no body): `static {ret} {name}({param})`.
/// Erased input ⇒ no parameter, erased output ⇒ `void` (the llvm rule). Used
/// for both the file-top prototypes and the definitions — C++ needs a
/// declaration before any call, so all fns are prototyped first. A
/// [`FnQual::HostDevice`] fn gets the single two-site definition:
/// `static __host__ __device__ {ret} {name}({param}, unsigned int* d_trap)`.
/// The trap parameter is named `d_trap` **deliberately shadowing the host
/// global**: inside the fn, `d_trap` then resolves to the parameter on the
/// device pass and to the threaded-through global on the host pass, so call
/// sites (`callee(arg, d_trap)`) and the Div/Mod guard's `*d_trap = 1u;`
/// are source-identical on both sites (§3's kind+1 flag encoding). #14: the
/// parameter rides only when the fn can trap (`caps`) — a trap-free fn's
/// callers pass nothing (func.rs `emit_call`, kernel.rs's device calls).
pub(crate) fn fn_signature(
    ir: &CategoryIr,
    f: FuncId,
    fnames: &SecondaryMap<FuncId, String>,
    qual: FnQual,
    caps: &kernel::TrapCaps,
) -> String {
    let fd = ir.func(f).expect("func resolves");
    let in_ty = &ir.object(fd.input).expect("input resolves").ty;
    let ret_ty = &ir.object(fd.output).expect("output resolves").ty;
    let ret = lower_ty(ret_ty).unwrap_or_else(|| "void".into());
    let param = match lower_ty(in_ty) {
        Some(t) => format!("{t} in"),
        None => String::new(),
    };
    if qual == FnQual::HostDevice {
        let params = match (param.is_empty(), caps.get(f)) {
            (true, true) => "unsigned int* d_trap".to_string(),
            (false, true) => format!("{param}, unsigned int* d_trap"),
            (true, false) => String::new(),
            (false, false) => param,
        };
        return format!("static __host__ __device__ {ret} {}({params})", fnames[f]);
    }
    format!("static {ret} {}({param})", fnames[f])
}

// --- free helpers ---------------------------------------------------------

/// In-place `Update` legality (plan-last-use §2 rule 4 + the consumer
/// composition pinned by flow-ir's `last_use_borrowed_init_is_never_dead`):
/// `Update(s, …)` may write into the source's buffer iff
///
/// 1. `dead_after(s, position(update))` — the plan's predicate: no use of
///    `s` at/after the update (rule 1's ranking puts the decide cone's reads
///    before the advance cone, so a guard read of the carried array still
///    converts), `s` does not escape, `s` is not carried. This alone decides
///    DIRECT sources: a `Parameter`/borrowed or escape-reaching source fails
///    it (the plan's `escapes`), a `Phi`/`Proj`/`Call` result may pass it —
///    so the remaining half is the consumer's, per the source's alias root
///    (the emission-level pointer copies):
///
/// 2. a **`LoopMerge` root** (the carried case — the source rides the merge's
///    current-state buffer) requires [`merge_family_dead`] (every same-field
///    current-state alias dies at the update — a later read of the same
///    buffer vetoes) AND [`owned_loop_init`] (the loop's init is an owned,
///    read-nowhere-else buffer — a borrowed `Parameter` init or a
///    ptr-resident/extra-used init vetoes: its buffer is the caller's, or is
///    still read — the full copy stays);
///
/// 3. **any other root** requires the source itself to be a fresh fn-owned
///    buffer ([`fresh_owned_buffer`]: a literal or bulk-op target — never a
///    `Proj`/`Phi`/`Call` result, whose storage aliases another object's or
///    may be borrowed).
///
/// Where the plan can't prove, the sites fall back to the full-copy Update
/// (today's behavior — the conservative default).
pub(crate) fn in_place_update(
    ir: &CategoryIr,
    plan: &LastUsePlan,
    f: FuncId,
    m: MorphismId,
    src: ObjectId,
) -> bool {
    let Some(pos) = plan.position(m) else {
        return false;
    };
    if !plan.dead_after(src, pos) {
        return false;
    }
    // The alias root: walk `Proj` in-edges source-ward (a `Proj` result is a
    // handle copy of the product's field — the same buffer), recording the
    // field path (innermost first).
    let mut root = src;
    let mut field_path: Vec<u32> = Vec::new();
    loop {
        let proj = ir.in_edges(root).iter().copied().find(|&e| {
            matches!(
                ir.morphism(e).expect("morphism resolves").op,
                Operation::Proj { .. }
            )
        });
        let Some(e) = proj else { break };
        let morph = ir.morphism(e).expect("morphism resolves");
        let Operation::Proj { index } = morph.op else {
            unreachable!()
        };
        field_path.push(index);
        root = morph.source;
    }
    if ir.object(root).expect("object resolves").kind == ObjectKind::LoopMerge {
        merge_family_dead(ir, plan, root, &field_path, pos) && owned_loop_init(ir, f, root)
    } else {
        field_path.is_empty() && fresh_owned_buffer(ir, src)
    }
}

/// The merge-family half of [`in_place_update`]'s rule 4: every use of the
/// source field's buffer — through the merge itself (whole-aggregate uses:
/// the exit assembly's handle copies, direct whole-merge operands) or
/// through same-field `Proj` views (the current-state aliases) — ranks
/// at/before the update. Other fields' projections name other buffers and
/// are irrelevant (a product state's scalar counter never vetoes the array
/// field). Positions and dead-ness are the plan's; the field selection is
/// the emitter's buffer structure.
fn merge_family_dead(
    ir: &CategoryIr,
    plan: &LastUsePlan,
    agg: ObjectId,
    field_path: &[u32],
    pos: u32,
) -> bool {
    for &e in ir.out_edges(agg) {
        let morph = ir.morphism(e).expect("morphism resolves");
        if let Operation::Proj { index } = morph.op {
            if !field_path.is_empty() && index != field_path[0] {
                continue; // a different field: another buffer.
            }
            // A current-state alias of the source's buffer: it and its
            // matching descendants must die at the update too (a read of
            // the old value after the write would observe the mutation).
            if !plan.dead_after(morph.target, pos)
                || !merge_family_dead(ir, plan, morph.target, &field_path[1..], pos)
            {
                return false;
            }
            continue;
        }
        // Whole-aggregate uses (Pair/LoopExit handle copies included) must
        // rank at/before the update — the plan's positions, not liveness.
        if plan.position(e).is_none_or(|p| p > pos) {
            return false;
        }
    }
    true
}

/// The borrowed/extra-used init veto ([`in_place_update`]'s consumer half —
/// flow-ir's pin: "a loop's borrowed init is never written in place"). Walks
/// the init's assembly cone (the init object plus everything it packs,
/// source-ward through `Pair` edges): every buffer in it must be fn-owned
/// and read nowhere but the ONE entry copy — a `Parameter` (borrowed), a
/// ptr-resident provenance (`Phi`/`Proj`/`Call` — the buffer aliases or may
/// be borrowed), an extra use (the value is read elsewhere, and iteration 1
/// writes the merge's buffer = the init's), or a second `LoopEnter` (two
/// loops sharing the init) all veto.
fn owned_loop_init(ir: &CategoryIr, f: FuncId, merge: ObjectId) -> bool {
    let Some(lp) = ir.loop_plan(f, merge) else {
        return false;
    };
    let mut cone: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    let mut stack = vec![lp.init];
    while let Some(o) = stack.pop() {
        if cone.contains_key(o) {
            continue;
        }
        cone.insert(o, ());
        for &e in ir.in_edges(o) {
            let morph = ir.morphism(e).expect("morphism resolves");
            if matches!(morph.op, Operation::Pair { .. }) {
                stack.push(morph.source);
            }
        }
    }
    let mut entries = 0;
    for (o, _) in cone.iter() {
        let obj = ir.object(o).expect("object resolves");
        if obj.kind == ObjectKind::Parameter {
            return false; // the borrowed handle (flow-ir's pin)
        }
        let buffer_bearing = matches!(obj.ty, Ty::Array { .. } | Ty::Tuple(_) | Ty::Struct { .. });
        if matches!(obj.ty, Ty::Array { .. }) && !fresh_owned_buffer(ir, o) {
            return false; // ptr-resident or borrowed provenance
        }
        if !buffer_bearing {
            continue; // scalars have no buffer — their other uses are reads of a copy
        }
        for &e in ir.out_edges(o) {
            let morph = ir.morphism(e).expect("morphism resolves");
            match morph.op {
                Operation::LoopEnter => entries += 1,
                Operation::Pair { .. } if cone.contains_key(morph.target) => {}
                _ => return false, // read/packed elsewhere, or a second consumer
            }
        }
    }
    entries == 1
}

/// A fresh fn-owned buffer: defined only by ops whose targets this backend
/// allocates itself (an array literal's `Pair` cone, or a bulk-op target) —
/// never a `Phi`/`Proj`/`Call` result (pointer-resident: aliases another
/// object's buffer, or may be the callee's borrowed argument).
fn fresh_owned_buffer(ir: &CategoryIr, o: ObjectId) -> bool {
    let obj = ir.object(o).expect("object resolves");
    if obj.kind != ObjectKind::Temporary {
        return false;
    }
    let in_edges = ir.in_edges(o);
    !in_edges.is_empty()
        && in_edges.iter().all(|&e| {
            matches!(
                ir.morphism(e).expect("morphism resolves").op,
                Operation::Pair { .. }
                    | Operation::Map { .. }
                    | Operation::Zip
                    | Operation::Enumerate
                    | Operation::Iota
                    | Operation::Fill
                    | Operation::Update
                    | Operation::Fold { .. }
                    | Operation::Index
            )
        })
}

/// The lvalues of every array-typed component of the value `base` (typed
/// `ty`) — the escape set for the fn-epilogue free guard (§2, F2). A bare
/// array contributes `base` itself; a product contributes each surviving
/// array-typed component's field lvalue (`base.f{erased_index}` — the ty.rs
/// residual remap), recursing into nested products; scalars and erased
/// components contribute nothing. Pointer equality against this set
/// subsumes the Phi/alias escape shapes (whichever buffer the return value
/// holds is the escaping one).
fn escape_lvalues(ty: &Ty, base: &str) -> Vec<String> {
    match ty {
        Ty::Array { .. } => vec![base.to_string()],
        Ty::Tuple(_) | Ty::Struct { .. } => {
            // A residual-1 product is bare: the local IS the single
            // surviving component (no `.f` field — the component_expr rule).
            let bare = residual_arity(ty) == 1;
            let mut out = Vec::new();
            let mut k = 0u32;
            while let Some(comp) = ty.component_ty(k) {
                if lower_ty(comp).is_some() {
                    let lbase = if bare {
                        base.to_string()
                    } else {
                        let eidx = erased_index(ty, k).expect("surviving component");
                        format!("{base}.f{eidx}")
                    };
                    out.extend(escape_lvalues(comp, &lbase));
                }
                k += 1;
            }
            out
        }
        _ => Vec::new(),
    }
}

pub(crate) fn is_float(ty: &Ty) -> bool {
    matches!(ty, Ty::Float { .. })
}

/// The unsigned int C++ type of the same width — BC2's cast target.
pub(crate) fn unsigned_twin(ty: &Ty) -> String {
    match ty {
        Ty::Int { bits, .. } => format!("uint{bits}_t"),
        _ => unreachable!("unsigned twin of non-int"),
    }
}

/// The `<cstdint>` MIN macro for a signed int C++ type (the MIN/-1 guard's
/// bound and the Div overflow value).
fn int_min(ct: &str) -> &'static str {
    match ct {
        "int8_t" => "INT8_MIN",
        "int16_t" => "INT16_MIN",
        "int32_t" => "INT32_MIN",
        "int64_t" => "INT64_MIN",
        _ => unreachable!("non-Core int width in Div/Mod"),
    }
}

/// The C++ constant text for a scalar `Value`. Ints render decimal except
/// `MIN` (the bare decimal `-…8` literal doesn't fit the C++ literal's type —
/// the `<cstdint>` macros instead). Floats render as round-tripping
/// scientific literals: Rust `{:e}` prints the shortest digits that
/// round-trip (the same value flow-rt's `Display` parity prints), and the
/// exponent keeps huge values like 1e300 floating literals (plain `Display`
/// would print 300 digits with no point — an *integer* literal, ill-formed).
/// f32 gets the `f` suffix (no double-rounding); NaN/±inf use the `<cmath>`
/// macros.
pub(crate) fn const_literal(v: &Value) -> String {
    match v {
        Value::I32(n) if *n == i32::MIN => "INT32_MIN".into(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) if *n == i64::MIN => "INT64_MIN".into(),
        Value::I64(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::F32(x) => float_literal(
            x.is_nan(),
            x.is_sign_negative(),
            x.is_infinite(),
            format!("{x:e}"),
            "f",
        ),
        Value::F64(x) => float_literal(
            x.is_nan(),
            x.is_sign_negative(),
            x.is_infinite(),
            format!("{x:e}"),
            "",
        ),
        Value::Str(_) => unreachable!("Str is not a scalar operand"),
    }
}

fn float_literal(is_nan: bool, neg: bool, is_inf: bool, digits: String, suffix: &str) -> String {
    if is_nan {
        return if neg { "(-NAN)".into() } else { "NAN".into() };
    }
    if is_inf {
        let h = if suffix == "f" {
            "HUGE_VALF"
        } else {
            "HUGE_VAL"
        };
        return if neg { format!("-{h}") } else { h.into() };
    }
    format!("{digits}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flow_ir::{Dest, FuncKind, IrBuilder, SourceLoc};

    const L: SourceLoc = SourceLoc { start: 0, end: 0 };

    fn lower_src(src: &str) -> CategoryIr {
        let po = flow_syntax::parse(src);
        assert!(po.diagnostics.is_empty(), "parse: {:?}", po.diagnostics);
        flow_lower::lower(src, &po.program).unwrap_or_else(|d| panic!("lower: {d:?}"))
    }

    fn emit_src(src: &str) -> String {
        crate::emit(&lower_src(src)).unwrap()
    }

    #[test]
    fn int_arith_wraps_via_unsigned_casts() {
        let cu = emit_src(
            "fn main() {\n    3 + 4 -> a;\n    10 - 3 -> s;\n    2 * 6 -> m;\n    \
             a + s + m -> t;\n    t -> println;\n}\n",
        );
        // BC2: every int op is `({cty})(({ucty})a {sym} ({ucty})b)`. WP-C:
        // the operand wrappers dissolved — constants inline as literals
        // inside the casts (the shape pin is unchanged).
        assert!(cu.contains("(int32_t)((uint32_t)"), "{cu}");
        assert!(cu.contains(" + (uint32_t)"), "{cu}");
        assert!(cu.contains(" - (uint32_t)"), "{cu}");
        assert!(cu.contains(" * (uint32_t)"), "{cu}");
        // Constants render as decimal literals, inline in the expressions.
        assert!(cu.contains("(uint32_t)3"), "{cu}");
        assert!(cu.contains("(uint32_t)10"), "{cu}");
    }

    #[test]
    fn neg_wraps_via_unsigned_cast() {
        let cu = emit_src("fn main() {\n    5 -> x;\n    -x -> y;\n    y -> println;\n}\n");
        assert!(cu.contains("(int32_t)(0 - (uint32_t)"), "{cu}");
    }

    #[test]
    fn div_mod_guards_host_shape() {
        let src = "fn f(a: i32, b: i32) -> i32 {\n    a / b -> q;\n    a % b -> r;\n    \
                   q + r -> ret;\n}\nfn main() {\n    (7, 3) -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        // Zero guard → direct host flow_trap(0) (DESIGN §3).
        assert_eq!(cu.matches("== 0) { flow_trap(0); }").count(), 2, "{cu}");
        // MIN/-1 value guards: Div ⇒ MIN, Mod ⇒ 0.
        assert_eq!(cu.matches("== -1) && (").count(), 2, "{cu}");
        // The overflow arms: Div stores INT32_MIN, Mod stores 0 (both inside
        // the `} else {` frame; the prelude's `unsigned int kind = 0;` is not
        // an overflow store).
        assert!(cu.contains("= INT32_MIN;\n  } else {"), "{cu}");
        assert!(cu.contains("= 0;\n  } else {"), "{cu}");
        // The division/modulo proper, guarded.
        assert!(cu.contains(" / "), "{cu}");
        assert!(cu.contains(" % "), "{cu}");
    }

    #[test]
    fn unsigned_div_has_only_the_zero_guard() {
        let src = "fn f(a: u8, b: u8) -> u8 {\n    a / b -> ret;\n}\n\
                   fn main() {\n    200 -> x: u8;\n    3 -> y: u8;\n    \
                   (x, y) -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("== 0) { flow_trap(0); }"), "{cu}");
        // Unsigned: no MIN/-1 guard anywhere.
        assert!(!cu.contains("== -1"), "{cu}");
    }

    // --- #13: constant-divisor guard elision --------------------------------

    #[test]
    fn const_divisor_elides_both_guards_host() {
        // Literal non-zero divisor (≠ −1): the zero guard and the MIN/−1
        // value guard are dead by construction — a plain division remains.
        let src = "fn f(a: i32) -> i32 {\n    a / 4 -> q;\n    a % 4 -> r;\n    \
                   q + r -> ret;\n}\nfn main() {\n    7 -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        assert!(!cu.contains("flow_trap(0)"), "{cu}");
        assert!(!cu.contains("== -1"), "{cu}");
        assert!(cu.contains(" / "), "{cu}");
        assert!(cu.contains(" % "), "{cu}");
    }

    #[test]
    fn const_zero_divisor_keeps_the_zero_guard() {
        // A literal 0 divisor: the guard ALWAYS fires (the oracle traps the
        // same way) — eliding is for provably-dead guards only.
        let src = "fn f(a: i32) -> i32 {\n    a / 0 -> ret;\n}\n\
                   fn main() {\n    7 -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("== 0) { flow_trap(0); }"), "{cu}");
    }

    #[test]
    fn const_neg_one_divisor_keeps_only_the_min_guard() {
        // A literal −1 divisor: the zero guard is dead (elided); the MIN/−1
        // value guard is LIVE (it decides on the dividend) and stays.
        let src = "fn f(a: i32) -> i32 {\n    a / -1 -> ret;\n}\n\
                   fn main() {\n    7 -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        assert!(!cu.contains("flow_trap(0)"), "{cu}");
        assert!(cu.contains("== -1) && ("), "{cu}");
        assert!(cu.contains("= INT32_MIN;"), "{cu}");
    }

    #[test]
    fn const_divisor_elides_guards_in_twin_and_host_device() {
        // Device paths (#13 applies to the twin AND the `__host__ __device__`
        // definition): a constant-divisor Div drops the device zero guard —
        // and with it the `#ifdef __CUDA_ARCH__` split.
        let src = "fn main() {\n    [10, 20] -> map { x -> x / 4 } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("static __host__ __device__"), "{cu}");
        assert!(!cu.contains("#ifdef __CUDA_ARCH__"), "{cu}");
        assert!(!cu.contains("*d_trap = 1u"), "{cu}");
        // The twin sibling (a body with a literal AND a constant-divisor
        // Div): no `*trap = 1u` in device code.
        let src = "fn main() {\n    [10, 20] -> map { x -> [x / 4, x][0] } -> rs;\n    \
                   rs[0] -> println;\n}\n";
        let cu = emit_src(src);
        assert!(!cu.contains("*trap = 1u"), "{cu}");
        assert!(cu.contains(" / "), "{cu}");
    }

    #[test]
    fn float_div_mod_never_had_guards() {
        // #13 touches nothing on the float path (÷0 is ±inf/NaN by
        // ADR-0013's S13 amendment — never a guard).
        let cu = emit_src(
            "fn main() {\n    7.5 / 2.0 -> d;\n    7.5 % 2.0 -> m;\n    \
                           d + m -> t;\n    t -> println;\n}\n",
        );
        // No guard CALLS (the prelude's flow_trap declaration is always
        // present).
        assert!(!cu.contains("flow_trap(0);"), "{cu}");
        assert!(!cu.contains("== 0) {"), "{cu}");
        assert!(cu.contains(" / "), "{cu}");
        assert!(cu.contains("fmod("), "{cu}");
    }

    #[test]
    fn float_arith_is_plain_and_mod_is_fmod() {
        let cu = emit_src("fn main() {\n    7.5 % 2.0 -> m;\n    m -> println;\n}\n");
        // f64 Mod → fmod; WP-C: operands and result inline (the wrapper
        // product dissolved, the single-consumer chain nests into print).
        assert!(cu.contains("fmod(7.5e0, 2e0)"), "{cu}");
        assert!(cu.contains("flow_print_f64("), "{cu}");
        let src = "fn main() {\n    7.5 -> x: f32;\n    2.0 -> y: f32;\n    \
                   x % y -> m;\n    m -> println;\n}\n";
        let cu = emit_src(src);
        assert!(cu.contains("fmodf("), "{cu}");
        assert!(cu.contains("flow_print_f32("), "{cu}");
    }

    #[test]
    fn float_neg_of_negative_constant_is_parenthesized() {
        // The lower folds `-1.5` into a Constant on the raw path; Neg must
        // emit `-({val})` — `--1.5e0` is ill-formed C++ (the decrement
        // operator).
        let cu = emit_src("fn main() {\n    -1.5 -> x;\n    -x -> y;\n    y -> println;\n}\n");
        // WP-C: the Neg may inline — pin the parenthesized form wherever
        // it lands; `--` never appears in an expression position (comment
        // banners legitimately contain `---`).
        assert!(!cu.contains("= --"), "{cu}");
        assert!(!cu.contains("(--"), "{cu}");
        assert!(cu.contains("-(-1.5e0)"), "{cu}");
    }

    #[test]
    fn phi_is_strict_select_over_temporaries() {
        let src = "fn abs(x: i32) -> i32 {\n    (x > 0) -> {\n        -true->  x;\n        \
                   -false-> x * -1;\n    } -> ret;\n}\n\
                   fn main() {\n    -7 -> abs -> r;\n    r -> println;\n}\n";
        let cu = emit_src(src);
        // Both arms materialize into temporaries before the ternary (BC7).
        assert!(cu.contains("int32_t t0 = "), "{cu}");
        assert!(cu.contains("int32_t t1 = "), "{cu}");
        assert!(cu.contains("? t0 : t1;"), "{cu}");
    }

    #[test]
    fn and_or_are_bitwise_never_short_circuit() {
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "main", Ty::Bool, Ty::Bool, L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let i = fb.input();
            let and = fb
                .binop(Operation::And, i, i, Dest::Fresh(None), L)
                .unwrap();
            fb.binop(Operation::Or, and, i, Dest::Ret { slot: None }, L)
                .unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let cu = crate::emit(&ir).unwrap();
        assert!(cu.contains(" & "), "{cu}");
        assert!(cu.contains(" | "), "{cu}");
        assert!(!cu.contains("&&"), "{cu}");
        assert!(!cu.contains("||"), "{cu}");
    }

    #[test]
    fn residual_one_pair_is_bare_no_struct() {
        let cu = emit_src("fn main() {\n    7 -> println;\n}\n");
        // The (IoToken, i32) print pair is residual-1: a bare int32_t local —
        // no FlowProd type anywhere in the module.
        assert!(!cu.contains("FlowProd"), "{cu}");
        assert!(cu.contains("flow_print_i32(o"), "{cu}");
    }

    #[test]
    fn residual_two_pair_uses_named_struct_fields() {
        let src = "fn f(a: i32, b: i32) -> i32 {\n    a + b -> ret;\n}\n\
                   fn main() {\n    (1, 2) -> f -> v;\n    v -> println;\n}\n";
        let cu = emit_src(src);
        assert!(
            cu.contains("struct FlowProd_int32_t_int32_t {\n    int32_t f0;\n    int32_t f1;\n};"),
            "{cu}"
        );
        // Pair writes and arith reads go through the .f{erased} fields.
        assert!(cu.contains(".f0 = "), "{cu}");
        assert!(cu.contains(".f1 = "), "{cu}");
        assert!(cu.contains(".f0 + (uint32_t)"), "{cu}");
    }

    #[test]
    fn u8_print_routes_to_flow_print_u8() {
        let cu = emit_src("fn main() {\n    200 -> x: u8;\n    x -> println;\n}\n");
        // A call (not the prelude's decl): flow_print_u8 on a local.
        assert!(cu.contains("flow_print_u8(o"), "{cu}");
        // No i32 print CALL (the prelude declares every print fn).
        assert!(!cu.contains("flow_print_i32(o"), "{cu}");
    }

    #[test]
    fn str_print_passes_pointer_and_explicit_len() {
        let cu = emit_src("fn main() {\n    \"hi\" -> print;\n}\n");
        assert!(cu.contains("static const char str0[] = \"hi\";"), "{cu}");
        assert!(
            cu.contains("flow_print_str((const uint8_t*)str0, 2, false);"),
            "{cu}"
        );
    }

    #[test]
    fn literals_render_at_width() {
        assert_eq!(const_literal(&Value::I32(7)), "7");
        assert_eq!(const_literal(&Value::I32(-7)), "-7");
        assert_eq!(const_literal(&Value::I32(i32::MIN)), "INT32_MIN");
        assert_eq!(const_literal(&Value::I64(i64::MIN)), "INT64_MIN");
        assert_eq!(const_literal(&Value::I64(-9)), "-9");
        assert_eq!(const_literal(&Value::U8(255)), "255");
        assert_eq!(const_literal(&Value::Bool(true)), "true");
        assert_eq!(const_literal(&Value::F64(f64::NAN)), "NAN");
        assert_eq!(const_literal(&Value::F64(f64::INFINITY)), "HUGE_VAL");
        assert_eq!(const_literal(&Value::F64(f64::NEG_INFINITY)), "-HUGE_VAL");
        assert_eq!(const_literal(&Value::F32(f32::INFINITY)), "HUGE_VALF");
    }

    #[test]
    fn float_literals_round_trip() {
        // Every finite literal parses back bit-exactly (Rust's parser is
        // correctly rounded, like the target C++ compilers).
        for x in [
            4080.0f64,
            5.375,
            -0.0,
            0.1,
            1e300,
            1e-300,
            f64::MAX,
            f64::MIN_POSITIVE,
        ] {
            let lit = const_literal(&Value::F64(x));
            let back: f64 = lit.parse().unwrap();
            assert_eq!(back.to_bits(), x.to_bits(), "f64 literal {lit} for {x}");
        }
        for x in [4080.0f32, 5.375, -0.0, 0.1, 1e30, f32::MAX] {
            let lit = const_literal(&Value::F32(x));
            let back: f32 = lit.trim_end_matches('f').parse().unwrap();
            assert_eq!(back.to_bits(), x.to_bits(), "f32 literal {lit} for {x}");
        }
    }

    /// WP2's placeholder is gone: vector_add now emits a complete module —
    /// kernels, launches, literal uploads, the whole WP3 surface.
    #[test]
    fn array_bulk_ops_emit_wp3() {
        let path = format!(
            "{}/../../../examples/vector_add.flow",
            env!("CARGO_MANIFEST_DIR")
        );
        let src = std::fs::read_to_string(&path).unwrap();
        let cu = crate::emit(&lower_src(&src)).unwrap();
        assert!(cu.contains("__global__"), "{cu}");
        // S20: vector_add's only would-be trap sources are its two constant
        // readbacks — the bounds proof clears both, so no launch is followed
        // by the §3 check at all.
        assert!(!cu.contains("trap_check_after_launch();"), "{cu}");
        assert!(cu.contains("cudaMemcpyHostToDevice"), "{cu}");
    }

    #[test]
    fn allocation_registry_skeleton() {
        // A trivial sealed main to host an FnEmit.
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "main", Ty::Unit, Ty::Unit, L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let i = fb.input();
            fb.output(i, None, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let mut fnames: SecondaryMap<FuncId, String> = SecondaryMap::new();
        fnames.insert(ir.entry(), "flow_main".to_string());
        let strings: SecondaryMap<ObjectId, StrGlobal> = SecondaryMap::new();
        let quals = crate::kernel::Qualifiers::analyze(&ir);
        let caps = crate::kernel::TrapCaps::analyze(&ir);
        let live = crate::kernel::host_reachable(&ir);
        let kernels = crate::kernel::emit_kernel_set(&ir, &fnames, &quals, &caps, &live);

        let mut fe = FnEmit::new(&ir, ir.entry(), &fnames, &strings, &quals, &caps, &kernels);
        fe.register_alloc("buf0".into());
        fe.register_alloc("buf1".into());
        fe.register_alloc("buf2".into());
        // The escape move removes exactly buf1; frees emit in allocation order.
        fe.remove_alloc("buf1");
        fe.remove_alloc("never_registered"); // no-op, no panic
        fe.emit_frees();
        assert_eq!(
            fe.body,
            "  cu_check(cudaFree(buf0), \"cudaFree(buf0)\");\n  \
             cu_check(cudaFree(buf2), \"cudaFree(buf2)\");\n"
        );
        // The registry is drained by emission.
        assert!(fe.allocs.is_empty());
    }

    #[test]
    fn empty_registry_emits_no_frees() {
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "main", Ty::Unit, Ty::Unit, L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let i = fb.input();
            fb.output(i, None, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let cu = crate::emit(&ir).unwrap();
        // WP2's scalar programs register nothing: no cudaFree in any fn body.
        assert!(!cu.contains("cudaFree(buf"), "{cu}");
        // The one cudaFree LINE is the trap-flag free in main.
        let free_lines: Vec<&str> = cu.lines().filter(|l| l.contains("cudaFree")).collect();
        assert_eq!(free_lines.len(), 1, "{cu}");
        assert!(free_lines[0].contains("d_trap"), "{cu}");
    }

    // --- F2: the return-escape guard covers every array-typed component ----

    /// The definition slice of the host fn whose signature contains `needle`.
    fn fn_def<'a>(cu: &'a str, needle: &str) -> &'a str {
        let start = cu.find(needle).expect("fn def");
        let end = cu[start..].find("\n}\n").map(|e| start + e).unwrap();
        &cu[start..end]
    }

    /// Frees emitted at fn-epilogue indent (unguarded — the value guard
    /// indents its free one level deeper). A bare free of a buffer that
    /// escapes through the return is the use-after-free (DESIGN §2).
    fn bare_free_lines(def: &str) -> Vec<&str> {
        def.lines()
            .filter(|l| l.starts_with("  cu_check(cudaFree("))
            .collect()
    }

    #[test]
    fn product_return_with_array_component_guards_free() {
        // F2 repro (a): mk returns a product holding the locally-allocated
        // array `a`. The name-based registry escape (Output) cannot see the
        // buffer inside the struct; the free must be value-guarded against
        // the returned struct's array field, or the caller gets a dangling
        // field (use-after-free). #18: the literal buffer is a zone member,
        // so the guard is the zone release's range-test veto (plan rule 3 —
        // the same pointer-VALUE comparison class, at zone granularity).
        let src = "fn mk(x: i32) -> ([i32; 2], i32) {\n    [1, 2] -> a;\n    (a, x) -> ret;\n}\n\
                   fn main() {\n    5 -> mk -> p;\n    p.0 -> arr;\n    arr[1] -> println;\n}\n";
        let cu = emit_src(src);
        let def = fn_def(&cu, "fn0(int32_t in) {");
        // The zone release is vetoed iff the return struct's array field
        // (.f0 by the erased-index remap) points into the arena.
        assert!(
            def.contains("(char*)o1.f0 >= (char*)arena0 && (char*)o1.f0 < (char*)arena0 + 256ULL"),
            "range-test veto against the return field:\n{def}"
        );
        assert!(def.contains("if (!escaped0) {"), "{def}");
        assert_eq!(
            bare_free_lines(def),
            Vec::<&str>::new(),
            "no buffer may be freed bare in an array-component return:\n{def}"
        );
        // The returned struct field still holds the handle past fn exit.
        assert!(def.contains("return o"), "{def}");
    }

    #[test]
    fn phi_selected_array_return_guards_free() {
        // F2 repro (b): a Phi over two locally-allocated buffers feeds the
        // return. The Output's name-based removal matches nothing in the
        // registry (the Phi allocated nothing); the selected buffer escapes
        // and must survive, the unselected one is freed — by pointer
        // equality against the return local. #18: both literals share the
        // fn zone, so the veto is one range test on the returned handle —
        // an escaping buffer pins its whole zone (rule 3's recorded
        // bounded-leak tradeoff for the unselected buffer).
        let src = "fn sel(c: bool) -> [i32; 2] {\n    \
                   c -> {\n        -true-> [1, 2];\n        -false-> [3, 4];\n    } -> ret;\n}\n\
                   fn main() {\n    true -> sel -> r;\n    r[0] -> println;\n}\n";
        let cu = emit_src(src);
        let def = fn_def(&cu, "fn0(bool in) {");
        // Both literal buffers are zone members (two pointer inits); the
        // single zone release is vetoed by the returned handle's range test.
        assert_eq!(def.matches("= (int32_t*)(arena0 + ").count(), 2, "{def}");
        assert_eq!(def.matches("cu_check(cudaFree(").count(), 1, "{def}");
        assert!(
            def.contains("(char*)o1 >= (char*)arena0 && (char*)o1 < (char*)arena0 + 512ULL"),
            "range-test veto on the returned handle:\n{def}"
        );
        assert_eq!(
            bare_free_lines(def),
            Vec::<&str>::new(),
            "no bare frees — the escaped buffer is runtime-selected:\n{def}"
        );
    }

    #[test]
    fn nested_product_return_guards_free_recursively() {
        // F2, nested shape: an array two products deep — the guard must
        // recurse to the inner field lvalue (.f0.f0 by the remap). #18: the
        // recursion now shows up inside the zone release's range test.
        let src = "fn mk(x: i32) -> (([i32; 2], i32), bool) {\n    \
                   [1, 2] -> a;\n    ((a, x), true) -> ret;\n}\n\
                   fn main() {\n    5 -> mk -> p;\n    p.0 -> q;\n    q.0 -> arr;\n    \
                   arr[1] -> println;\n}\n";
        let cu = emit_src(src);
        let def = fn_def(&cu, "fn0(int32_t in) {");
        assert!(
            def.contains("(char*)o1.f0.f0 >= (char*)arena0"),
            "the veto's range test recurses into nested products:\n{def}"
        );
        assert_eq!(
            bare_free_lines(def),
            Vec::<&str>::new(),
            "no bare frees:\n{def}"
        );
    }

    // --- plan-last-use §2 rule 4: the in-place Update ------------------------

    /// fn f(a: [i32; 4]) -> [i32; 4] { update(a, 0, 1) -> ret } — one Update
    /// whose source is the borrowed parameter itself.
    fn param_update_cu() -> String {
        let arr4 = Ty::Array {
            elem: Box::new(Ty::i32()),
            size: 4,
        };
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "main", arr4.clone(), arr4, L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let a = fb.input();
            let z = fb.constant(Value::I64(0), L).unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            fb.update(a, z, one, Dest::Ret { slot: None }, L).unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        crate::emit(&ir).unwrap()
    }

    #[test]
    fn in_place_update_borrowed_parameter_source_keeps_full_copy() {
        // Rule 4's first half: a Parameter source escapes (the plan's rule
        // 2) — never written in place. The target gets its own fn-zone
        // pointer init and the launch's out/src stay distinct handles.
        let cu = param_update_cu();
        assert!(
            cu.contains("= (int32_t*)(arena0 + 0ULL);"),
            "the full-copy target buffer (a zone member) stays:\n{cu}"
        );
        let def = fn_def(&cu, "flow_main(int32_t* in) {");
        // No in-place handle copy: the launch's out/src stay distinct
        // handles (the parameter's slot is only read, never aliased into).
        let launch = def
            .lines()
            .find(|l| l.contains("<<<"))
            .expect("the update launch");
        let args = launch.split(">>>(").nth(1).expect("launch args");
        let mut operands = args.split(',').map(str::trim);
        let out = operands.next().expect("out arg");
        let src = operands.next().expect("src arg");
        assert_ne!(out, src, "out/src stay distinct (full copy):\n{def}");
    }

    #[test]
    fn in_place_update_dead_literal_source_writes_in_place() {
        // A locally-built array read nowhere after the update (rule 4's
        // dead_after): the target handle IS the source handle — no pointer
        // init, no fresh buffer for the update.
        let arr4 = Ty::Array {
            elem: Box::new(Ty::i32()),
            size: 4,
        };
        let mut b = IrBuilder::new();
        let f = b
            .declare(FuncKind::Named, "main", Ty::Unit, arr4, L)
            .unwrap();
        {
            let mut fb = b.build_fn(f).unwrap();
            let one = fb.constant(Value::I32(1), L).unwrap();
            let two = fb.constant(Value::I32(2), L).unwrap();
            let three = fb.constant(Value::I32(3), L).unwrap();
            let four = fb.constant(Value::I32(4), L).unwrap();
            let z = fb
                .pack_array(&[one, two, three, four], Dest::Fresh(None), L)
                .unwrap();
            let idx = fb.constant(Value::I64(0), L).unwrap();
            let nine = fb.constant(Value::I32(9), L).unwrap();
            fb.update(z, idx, nine, Dest::Ret { slot: None }, L)
                .unwrap();
            fb.finish().unwrap();
        }
        let ir = b.seal(f).unwrap();
        let cu = crate::emit(&ir).unwrap();
        let def = fn_def(&cu, "flow_main() {");
        let launch = def
            .lines()
            .find(|l| l.contains("<<<"))
            .expect("the update launch");
        let args = launch.split(">>>(").nth(1).expect("launch args");
        let mut operands = args.split(',').map(str::trim);
        let out = operands.next().expect("out arg").to_string();
        let src = operands.next().expect("src arg").to_string();
        // In place: the target was assigned the source handle (no zone
        // pointer init for it), so out == src at the launch.
        assert!(
            def.contains(&format!("{out} = {src};")),
            "the in-place handle copy:\n{def}"
        );
        assert!(
            !def.contains(&format!("{out} = (int32_t*)(arena0")),
            "no fresh buffer for the update target:\n{def}"
        );
        // The literal's own buffer (the zone member the update writes) is
        // still there, and the launch/guard text is unchanged.
        assert!(def.contains("= (int32_t*)(arena0 + 0ULL);"), "{def}");
        assert!(def.contains("trap_check_after_launch();"), "{def}");
    }
}
