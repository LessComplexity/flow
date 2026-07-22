//! `emit_fn` (DESIGN §2/§5): entry-block allocas, the `topo_order` walk, and the
//! full op table over per-object stack slots. One `alloca` per materialized
//! object; a morphism loads its operand slots, computes, stores its target slot
//! — the piecewise functor application (§8.5).

use flow_ir::{
    BoundsProof, CategoryIr, FuncId, FuncKind, LastUsePlan, MorphismId, ObjectId, ObjectKind,
    Operation, Ty, Value,
};
use slotmap::SecondaryMap;

use crate::module::StrGlobal;
use crate::ty::{
    erased_index, lower_body_input_ty, lower_named_input_ty, lower_ty, residual_arity,
};

/// Truthful fn attributes (trap-aware; suggestions #7): the conservative
/// syntactic capability set. A fn is **clean** — pure by construction — when it
/// has no integer `Div`/`Mod` (zero/`MIN/-1` guards call `flow_trap`), no
/// trap-capable `Index`/`Update` (an unproven `Index`'s bounds guard calls
/// `flow_trap`; an S20 `bounds_proof`-proven `Index` can never fire — its guard
/// is elided, so it does not count), no `Print`/token use (the token-threaded
/// `flow_print_*` externs are not readonly), and — transitively, callerward
/// over `Call` and `Map`/`Fold` body edges — every callee is equally clean (the
/// spirit of the CUDA backend's `kernel.rs:TrapCaps` fixpoint).
///
/// A clean fn reads memory (by-ref array arguments) but never writes state
/// visible to its caller, never unwinds, and — unless its closure can loop —
/// always returns: clean fns get `readonly nounwind`, plus `willreturn` when
/// the transitive closure has no `LoopEnter` and no call cycle (a possibly
/// infinite loop or unbounded recursion would make `willreturn` a lie — UB).
/// Unclean fns get nothing. The rule is syntactic on the op set apart from the
/// `Index` proof, like `TrapCaps`: an integer `Div`/`Mod` counts even when a
/// future elision could remove its guard — keeping the fn attribute-free is
/// harmless, and the two passes stay independently simple.
pub(crate) struct FnAttrs {
    clean: SecondaryMap<FuncId, bool>,
    loopy: SecondaryMap<FuncId, bool>,
}

impl FnAttrs {
    pub(crate) fn analyze(ir: &CategoryIr) -> FnAttrs {
        let mut clean: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut loopy: SecondaryMap<FuncId, bool> = SecondaryMap::new();
        let mut edges: SecondaryMap<FuncId, Vec<FuncId>> = SecondaryMap::new();
        for (f, fd) in ir.funcs() {
            let bp = ir.bounds_proof(f);
            let mut unclean = false;
            let mut has_loop = false;
            for &m in &fd.morphisms {
                let morph = ir.morphism(m).expect("morphism resolves");
                match morph.op {
                    Operation::Div | Operation::Mod => {
                        let src_ty = &ir.object(morph.source).expect("source resolves").ty;
                        if matches!(src_ty.component_ty(0), Some(Ty::Int { .. })) {
                            // #13 credit (S20): a literal non-zero, non-−1
                            // constant divisor cannot trap — the guard is dead.
                            let safe = const_int_operand(ir, morph.source, 1)
                                .is_some_and(|v| v != 0 && v != -1);
                            if !safe {
                                unclean = true;
                            }
                        }
                    }
                    // A proven `Index` can never fire (S20 `bounds_proof`) —
                    // its guard is elided in emission, so it is not
                    // trap-capable and does not count (a proven one fails the
                    // guard and lands on the wildcard arm). `Update` stays
                    // counted (conservative this wave): its OOB class differs
                    // only if proven too.
                    Operation::Index if !bp.proven(m) => {
                        unclean = true;
                    }
                    Operation::Update | Operation::Print { .. } => {
                        unclean = true;
                    }
                    Operation::LoopEnter => has_loop = true,
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
            // Token use without a `Print` (a token threaded through in/out or a
            // product): conservative — the fn may reach `flow_print_*` through
            // any path, so it stays attribute-free.
            if !unclean {
                unclean = ir
                    .objects()
                    .any(|(id, obj)| ir.try_owner(id) == Some(f) && ty_has_token(&obj.ty));
            }
            clean.insert(f, !unclean);
            loopy.insert(f, has_loop);
        }
        // A call/body cycle can recurse without bound — not `willreturn` even
        // when clean. Seed `loopy` with every fn that can reach itself, then
        // propagate callerward with the same fixpoint.
        for (f, _) in ir.funcs() {
            if reaches_self(ir, &edges, f) {
                loopy.insert(f, true);
            }
        }
        loop {
            let mut changed = false;
            for (f, _) in ir.funcs() {
                for g in edges.get(f).cloned().unwrap_or_default() {
                    if clean.get(f).copied().unwrap_or(false)
                        && !clean.get(g).copied().unwrap_or(false)
                    {
                        clean.insert(f, false);
                        changed = true;
                    }
                    if !loopy.get(f).copied().unwrap_or(false)
                        && loopy.get(g).copied().unwrap_or(false)
                    {
                        loopy.insert(f, true);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        FnAttrs { clean, loopy }
    }

    /// `f` is in the clean set (`false` for unknown ids — a sealed graph's own
    /// fns are all present).
    pub(crate) fn clean(&self, f: FuncId) -> bool {
        self.clean.get(f).copied().unwrap_or(false)
    }

    /// `f`'s transitive closure can loop or recurse — withhold `willreturn`
    /// (`true` for unknown ids: the conservative answer).
    pub(crate) fn loopy(&self, f: FuncId) -> bool {
        self.loopy.get(f).copied().unwrap_or(true)
    }
}

/// `true` when `f` can reach itself through `edges` (a call/body cycle).
fn reaches_self(ir: &CategoryIr, edges: &SecondaryMap<FuncId, Vec<FuncId>>, f: FuncId) -> bool {
    let mut seen: SecondaryMap<FuncId, ()> = SecondaryMap::new();
    let mut stack = edges.get(f).cloned().unwrap_or_default();
    while let Some(g) = stack.pop() {
        if g == f {
            return true;
        }
        if seen.insert(g, ()).is_some() {
            continue;
        }
        if ir.func(g).is_some() {
            stack.extend(edges.get(g).cloned().unwrap_or_default());
        }
    }
    false
}

/// Does `ty` mention `IoToken` anywhere (top level or nested in a product)?
fn ty_has_token(ty: &Ty) -> bool {
    match ty {
        Ty::IoToken => true,
        Ty::Tuple(ts) => ts.iter().any(ty_has_token),
        Ty::Struct { fields, .. } => fields.iter().any(|(_, t)| ty_has_token(t)),
        _ => false,
    }
}

/// Per-function emission state (DESIGN Dat `FnCtx`). `slots` is partial — erased
/// (token/unit/str) objects have no slot.
pub(crate) struct FnEmit<'a> {
    pub ir: &'a CategoryIr,
    pub f: FuncId,
    pub fnames: &'a SecondaryMap<FuncId, String>,
    pub strings: &'a SecondaryMap<ObjectId, StrGlobal>,
    /// The module's attribute-capability pre-pass (suggestions #7): clean fns
    /// get `readonly nounwind` (+ `willreturn` when loop-free), unclean fns
    /// nothing.
    pub attrs: &'a FnAttrs,
    pub slots: SecondaryMap<ObjectId, String>,
    pub allocas: String,
    pub body: String,
    pub next: u32,
    /// By-ref array input state: `(input object, by-ref prefix k, by-ref
    /// input-struct text)` — the first-`k` Array components of the fn input
    /// arrive as `ptr`. For a Map/Fold body fn `k` is the site's capture count
    /// (ADR-0027, suggestions #6); for a `Named` fn `k = u32::MAX` — every
    /// top-level Array component (BL5 by-ref call args, suggestions #8).
    byref: Option<(ObjectId, u32, String)>,
    /// The objects holding a forwarded array pointer (`alloca ptr`): array
    /// objects produced by `Proj{index < k}` from the by-ref input, plus the
    /// input object itself when a Named fn's whole input is one bare Array.
    ptr_resident: SecondaryMap<ObjectId, ()>,
    /// The fn's last-use plan (docs/components/ir/plans/plan-last-use.md §2) —
    /// the single source of dead/escape/carried facts for the `Update` memcpy
    /// elision (suggestions #2); never re-derived locally.
    lup: LastUsePlan,
    /// The fn's bounds-proof plan (flow-ir `algo.rs:bounds_proof`, S20) — the
    /// provably-in-bounds `Index` set backing the guard elision: a proven
    /// `Index` can never fire, so `emit_index` drops its trap guard (just the
    /// GEP+load); everything unproven keeps today's guard byte-identical.
    bp: BoundsProof,
    /// `Update` targets whose whole-array memcpy is elided (rule 4's in-place
    /// write): they share the dead source array's slot, so the entry-block
    /// pass mints no alloca for them; `emit_update` inserts the shared slot.
    elided_updates: SecondaryMap<ObjectId, ()>,
}

impl<'a> FnEmit<'a> {
    pub fn new(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
    ) -> Self {
        FnEmit {
            ir,
            f,
            fnames,
            strings,
            attrs,
            slots: SecondaryMap::new(),
            allocas: String::new(),
            body: String::new(),
            next: 0,
            byref: None,
            ptr_resident: SecondaryMap::new(),
            lup: ir.last_use_plan(f),
            bp: ir.bounds_proof(f),
            elided_updates: SecondaryMap::new(),
        }
    }

    fn fresh(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    fn tmp(&mut self) -> String {
        format!("%t{}", self.fresh())
    }

    /// A fresh block label (bare, no `%`).
    pub(crate) fn label(&mut self) -> String {
        format!("bb{}", self.fresh())
    }

    /// Append one indented body instruction line.
    pub(crate) fn line(&mut self, s: impl AsRef<str>) {
        self.body.push_str("  ");
        self.body.push_str(s.as_ref());
        self.body.push('\n');
    }

    /// Append a bare block label line (`name:`), no indent.
    pub(crate) fn label_line(&mut self, name: &str) {
        self.body.push_str(name);
        self.body.push_str(":\n");
    }

    fn slot(&self, o: ObjectId) -> Option<String> {
        self.slots.get(o).cloned()
    }

    fn obj_ty(&self, o: ObjectId) -> Ty {
        self.ir.object(o).expect("object resolves").ty.clone()
    }

    // --- operand materialization -----------------------------------------

    /// Load the whole value of object `o`: a literal for a (scalar) constant, a
    /// `load` from its slot otherwise. `None` if `o` is erased / a `Str`.
    fn load_whole(&mut self, o: ObjectId) -> Option<(String, String)> {
        let obj = self.ir.object(o).expect("object resolves");
        if obj.kind == ObjectKind::Constant {
            return match &obj.value {
                Some(Value::Str(_)) | None => None,
                Some(v) => Some((lower_ty(&obj.ty)?, const_literal(v))),
            };
        }
        let llt = lower_ty(&obj.ty)?;
        let slot = self.slot(o)?;
        if self.ptr_resident.contains_key(o) {
            // By-ref capture: load the array through the forwarded pointer.
            // The deep copy is observably the inline value (read-only
            // capture semantics), so escaping uses are unchanged.
            let p = self.tmp();
            self.line(format!("{p} = load ptr, ptr {slot}"));
            let v = self.tmp();
            self.line(format!("{v} = load {llt}, ptr {p}"));
            return Some((llt, v));
        }
        // The by-ref input product's WHOLE value (an escaping use — `Pair`
        // into an ordinary product, `Output`): its first-`bk` Array fields
        // hold `ptr`s, so assemble the by-value whole in a scratch of the
        // ordinary type — each by-ref array component deep-copied through its
        // forwarded pointer, every other component copied inline.
        let bk = match &self.byref {
            Some((input, bk, text)) if *input == o && *text != llt => Some(*bk),
            _ => None,
        };
        if let Some(bk) = bk {
            let buf = self.scratch(&llt);
            for k in 0..product_arity(&obj.ty) {
                if k < bk && matches!(obj.ty.component_ty(k), Some(Ty::Array { .. })) {
                    let cllt = lower_ty(obj.ty.component_ty(k)?)?;
                    let src = self.array_operand_ptr(o, Some(k))?;
                    let dst = self.field_ptr(&buf, &obj.ty, &llt, k)?;
                    self.emit_memcpy(&dst, &src, &cllt);
                } else if let Some((cllt, cval)) = self.load_component(o, k) {
                    self.field_store(&buf, &obj.ty, &llt, k, &cllt, &cval);
                }
            }
            let v = self.tmp();
            self.line(format!("{v} = load {llt}, ptr {buf}"));
            return Some((llt, v));
        }
        let v = self.tmp();
        self.line(format!("{v} = load {llt}, ptr {slot}"));
        Some((llt, v))
    }

    /// A pointer to component `k` of the aggregate object `agg` (GEP or the bare
    /// slot). `None` if component `k` is erased or `agg` has no slot.
    fn component_ptr(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let agg_ty = self.obj_ty(agg);
        match &agg_ty {
            Ty::Tuple(_) | Ty::Struct { .. } => {
                let comp_ty = agg_ty.component_ty(k)?;
                let agg_slot = self.slot(agg)?;
                // The by-ref body input GEPs against the by-ref struct text; a
                // first-k Array field holds the capture `ptr`, not the array.
                let (cllt, agg_llt) = match &self.byref {
                    Some((input, bk, text)) if *input == agg => (
                        if k < *bk && matches!(comp_ty, Ty::Array { .. }) {
                            "ptr".into()
                        } else {
                            lower_ty(comp_ty)?
                        },
                        text.clone(),
                    ),
                    _ => (
                        if self.pointer_only_array_component(agg, k) {
                            "ptr".into()
                        } else {
                            lower_ty(comp_ty)?
                        },
                        self.lower_slot_ty(agg, &agg_ty)?,
                    ),
                };
                if residual_arity(&agg_ty) == 1 {
                    Some((cllt, agg_slot)) // bare: the slot IS the component
                } else {
                    let eidx = erased_index(&agg_ty, k)?;
                    let ptr = self.tmp();
                    self.line(format!(
                        "{ptr} = getelementptr {agg_llt}, ptr {agg_slot}, i32 0, i32 {eidx}"
                    ));
                    Some((cllt, ptr))
                }
            }
            Ty::Array { elem, .. } => {
                let cllt = lower_ty(elem)?;
                let agg_slot = self.slot(agg)?;
                let agg_llt = lower_ty(&agg_ty)?;
                let ptr = self.tmp();
                self.line(format!(
                    "{ptr} = getelementptr {agg_llt}, ptr {agg_slot}, i64 0, i64 {k}"
                ));
                Some((cllt, ptr))
            }
            _ => None,
        }
    }

    /// Load component `k` of aggregate `agg`. `None` if erased.
    fn load_component(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let (cllt, ptr) = self.component_ptr(agg, k)?;
        let v = self.tmp();
        self.line(format!("{v} = load {cllt}, ptr {ptr}"));
        Some((cllt, v))
    }

    /// Store `(llt, val)` into object `o`'s slot, if it has one.
    fn store_obj(&mut self, o: ObjectId, llt: &str, val: &str) {
        if let Some(slot) = self.slot(o) {
            self.line(format!("store {llt} {val}, ptr {slot}"));
        }
    }

    /// A fresh scratch alloca of `llt` in the entry block; returns its ptr name.
    fn scratch(&mut self, llt: &str) -> String {
        let name = format!("%s{}", self.fresh());
        self.allocas.push_str(&format!("  {name} = alloca {llt}\n"));
        name
    }

    /// The local slot type for a Pair-built staging product. Array components
    /// consumed only as addresses by collection/index/call ops are `ptr`
    /// fields (S20 #6/#8); value-observable components retain their ABI type.
    fn lower_slot_ty(&self, o: ObjectId, ty: &Ty) -> Option<String> {
        let components: Vec<&Ty> = match ty {
            Ty::Tuple(ts) => ts.iter().collect(),
            Ty::Struct { fields, .. } => fields.iter().map(|(_, ty)| ty).collect(),
            _ => return lower_ty(ty),
        };
        let kept: Vec<String> = components
            .iter()
            .enumerate()
            .filter_map(|(k, ty)| {
                if self.pointer_only_array_component(o, k as u32) {
                    Some("ptr".into())
                } else {
                    lower_ty(ty)
                }
            })
            .collect();
        match kept.len() {
            0 => None,
            1 => Some(kept.into_iter().next().unwrap()),
            _ => Some(format!("{{ {} }}", kept.join(", "))),
        }
    }

    /// A Pair array field is representation-only when every use reads that
    /// component as an address. Such fields stage a pointer, never array bytes.
    fn pointer_only_array_component(&self, agg: ObjectId, k: u32) -> bool {
        if !matches!(self.obj_ty(agg).component_ty(k), Some(Ty::Array { .. }))
            || self.pair_source(agg, k).is_none()
        {
            return false;
        }
        let uses = self.ir.out_edges(agg);
        !uses.is_empty()
            && uses.iter().all(
                |&m| match self.ir.morphism(m).expect("morphism resolves").op {
                    Operation::Index | Operation::Update => k == 0,
                    Operation::Zip => k <= 1,
                    Operation::Map { captures, .. } => k <= captures,
                    Operation::Fold { captures, .. } => k < captures || k == captures + 1,
                    Operation::Call(_) => true,
                    _ => false,
                },
            )
    }

    /// Address field `k` of raw aggregate storage. Erasure remapping is based
    /// on the source type; replacing an Array with `ptr` keeps it materialized.
    fn field_ptr(&mut self, base: &str, agg_ty: &Ty, agg_llt: &str, k: u32) -> Option<String> {
        if residual_arity(agg_ty) == 1 {
            return Some(base.to_string());
        }
        let eidx = erased_index(agg_ty, k)?;
        let p = self.tmp();
        self.line(format!(
            "{p} = getelementptr {agg_llt}, ptr {base}, i32 0, i32 {eidx}"
        ));
        Some(p)
    }

    /// Copy an aggregate value between allocas without creating aggregate SSA
    /// (ADR-0021's Update pattern). An exact identity is already in place.
    fn emit_memcpy(&mut self, dst: &str, src: &str, llt: &str) {
        if dst == src {
            return;
        }
        self.line(format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {src}, i64 ptrtoint (ptr getelementptr ({llt}, ptr null, i64 1) to i64), i1 false)"
        ));
    }

    /// Store `(vllt, val)` into field `k` of a raw aggregate pointer of ty
    /// `agg_ty`, whose lowered text is `agg_llt` (the by-ref input struct for a
    /// capturing body call, `lower_ty` otherwise — the GEP offsets differ).
    /// Used by the collection loops, which build products in scratch.
    fn field_store(
        &mut self,
        base: &str,
        agg_ty: &Ty,
        agg_llt: &str,
        k: u32,
        vllt: &str,
        val: &str,
    ) {
        let ptr = self
            .field_ptr(base, agg_ty, agg_llt, k)
            .expect("kept field");
        self.line(format!("store {vllt} {val}, ptr {ptr}"));
    }

    /// The base address of an op's array operand — component `k` of the `source`
    /// product (`Some(k)`), or the bare `source` object itself (`None`, the
    /// no-capture map source). When the array reaches the op from a ptr-resident
    /// by-ref capture (the `Pair` feeder, or `source` itself), the forwarded
    /// `load ptr` is the address — the op reads the caller's array directly
    /// instead of the inline deep copy. When `source` IS the by-ref fn input
    /// product, its Array field holds the forwarded `ptr` — load it. Anything
    /// else is the ordinary slot/component address, as before.
    fn array_operand_ptr(&mut self, source: ObjectId, k: Option<u32>) -> Option<String> {
        let feeder = match k {
            None => Some(source),
            Some(k) => self.pair_source(source, k),
        };
        if let Some(f) = feeder
            && self.ptr_resident.contains_key(f)
        {
            let slot = self.slot(f)?;
            let p = self.tmp();
            self.line(format!("{p} = load ptr, ptr {slot}"));
            return Some(p);
        }
        if let Some(f) = feeder
            && matches!(self.obj_ty(f), Ty::Array { .. })
            && let Some(slot) = self.slot(f)
        {
            return Some(slot);
        }
        match k {
            None => self.slot(source),
            Some(k) => {
                let byref_field = matches!(&self.byref, Some((input, bk, _))
                    if *input == source
                        && k < *bk
                        && matches!(self.obj_ty(source).component_ty(k), Some(Ty::Array { .. })));
                if byref_field {
                    let (_, fp) = self.component_ptr(source, k)?;
                    let p = self.tmp();
                    self.line(format!("{p} = load ptr, ptr {fp}"));
                    return Some(p);
                }
                Some(self.component_ptr(source, k)?.1)
            }
        }
    }

    // --- traps ------------------------------------------------------------

    /// Branch to a trap block when `cond` is true; continue otherwise
    /// (`kind`: 0 = div_zero, 1 = index_oob — DESIGN §1).
    fn trap_if(&mut self, cond: &str, kind: u32) {
        let trap = self.label();
        let cont = self.label();
        self.line(format!("br i1 {cond}, label %{trap}, label %{cont}"));
        self.label_line(&trap);
        self.line(format!("call void @flow_trap(i32 {kind})"));
        self.line("unreachable");
        self.label_line(&cont);
    }

    // --- the walk ---------------------------------------------------------

    /// Emit the function body: prologue store, the topo walk, epilogue return.
    pub fn emit(mut self) -> String {
        let fd = self.ir.func(self.f).expect("func resolves");
        let in_ty = self.obj_ty(fd.input);
        let ret_ty = self.obj_ty(fd.output);
        let fname = self.fnames[self.f].clone();

        // By-ref array input lowering. ADR-0027 by-reference array captures
        // (suggestions #6): a Map/Fold body fn's first-k Array input components
        // arrive as `ptr`; `k` comes from the unique Map/Fold site with body ==
        // self.f (lower mints one fresh body fn per lambda site). By-reference
        // array call arguments (BL5 amendment, suggestions #8): a Named fn's
        // EVERY top-level Array input component arrives as `ptr` (`k =
        // u32::MAX` — no leading-captures convention). Both rest on the same
        // read-only argument: capture semantics / Flow value semantics
        // (functional `Update` copies to a fresh alloca) make the pointer
        // observably identical to the inline array.
        let (bk, btext) = match fd.kind {
            FuncKind::MapBody | FuncKind::FoldBody => {
                let k = self.body_captures();
                (k, lower_body_input_ty(&in_ty, k))
            }
            FuncKind::Named => (u32::MAX, lower_named_input_ty(&in_ty)),
        };
        if let Some(text) = btext {
            self.byref = Some((fd.input, bk, text));
            // A bare-Array Named input arrives as `ptr` itself: the input
            // object's slot is the `alloca ptr` holding the argument pointer
            // (the whole object is ptr-resident, not just its Projs).
            if bk == u32::MAX && matches!(&in_ty, Ty::Array { .. }) {
                self.ptr_resident.insert(fd.input, ());
            }
            let owned_ids: Vec<ObjectId> = self
                .ir
                .objects()
                .filter(|(id, _)| self.ir.try_owner(*id) == Some(self.f))
                .map(|(id, _)| id)
                .collect();
            for id in owned_ids {
                for &m in self.ir.in_edges(id) {
                    let morph = self.ir.morphism(m).expect("morphism resolves");
                    if let Operation::Proj { index } = morph.op
                        && morph.source == fd.input
                        && index < bk
                        && matches!(in_ty.component_ty(index), Some(Ty::Array { .. }))
                    {
                        self.ptr_resident.insert(id, ());
                    }
                }
            }
        }
        let in_llt = match &self.byref {
            Some((_, _, text)) => Some(text.clone()),
            None => lower_ty(&in_ty),
        };

        // Last-use Update elision (suggestions #2; plan-last-use §2 rule 4):
        // the target of an in-place-able Update shares its dead source array's
        // slot — mint no alloca for it below (`emit_update` re-derives the
        // same verdict through `update_in_place_source` and inserts the shared
        // slot when it fires).
        let mut elided = Vec::new();
        for &m in &fd.morphisms {
            if self.update_in_place_source(m).is_some() {
                elided.push(self.ir.morphism(m).expect("morphism resolves").target);
            }
        }
        for t in elided {
            self.elided_updates.insert(t, ());
        }

        // Entry-block allocas: one per materialized (non-constant, non-erased)
        // object. Constants are literals; erased objects have no slot. A
        // ptr-resident object's slot is the `alloca ptr` holding its forwarded
        // capture pointer; the by-ref body input's slot is the by-ref struct.
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
            // An elided Update target has no slot of its own; like an erased
            // object it still consumes its ordinal (name stability, L2).
            if self.elided_updates.contains_key(*id) {
                ord += 1;
                continue;
            }
            let llt = if self.ptr_resident.contains_key(*id) {
                Some("ptr".into())
            } else if Some(*id) == self.byref.as_ref().map(|(input, _, _)| *input) {
                in_llt.clone()
            } else {
                self.lower_slot_ty(*id, ty)
            };
            if let Some(llt) = llt {
                let name = format!("%o{ord}");
                self.slots.insert(*id, name.clone());
                self.allocas.push_str(&format!("  {name} = alloca {llt}\n"));
            }
            ord += 1;
        }

        // Prologue: store the incoming parameter into its slot.
        if let Some(t) = &in_llt
            && let Some(ps) = self.slot(fd.input)
        {
            self.line(format!("store {t} %arg, ptr {ps}"));
        }

        // The topo walk (DESIGN §2/§3).
        self.walk();

        // Epilogue: return the output slot (or void).
        let sig_ret = match lower_ty(&ret_ty) {
            Some(t) => {
                let os = self.slot(fd.output).expect("non-void return has a slot");
                let v = self.tmp();
                self.line(format!("{v} = load {t}, ptr {os}"));
                self.line(format!("ret {t} {v}"));
                t
            }
            None => {
                self.line("ret void");
                "void".to_string()
            }
        };

        // Truthful fn attributes (suggestions #7; `FnAttrs`): clean fns are
        // `readonly nounwind` (+ `willreturn` when the closure can't loop or
        // recurse); a clean fn's bare-`ptr` by-ref array parameter additionally
        // carries `noalias nocapture readonly` (the callee never writes through
        // it, never lets it escape, and — the single pointer argument — it
        // aliases nothing else the fn accesses). Unclean fns stay bare.
        let clean = self.attrs.clean(self.f);
        let param = match &in_llt {
            Some(t) if clean && t == "ptr" && self.byref.is_some() => {
                format!("{t} noalias nocapture readonly %arg")
            }
            Some(t) => format!("{t} %arg"),
            None => String::new(),
        };
        let fn_attrs = if clean {
            if self.attrs.loopy(self.f) {
                " readonly nounwind"
            } else {
                " readonly nounwind willreturn"
            }
        } else {
            ""
        };
        format!(
            "define internal {sig_ret} @{fname}({param}){fn_attrs} {{\nentry:\n{}{}}}\n",
            self.allocas, self.body
        )
    }

    /// The capture count of the Map/Fold site whose body is `self.f` (unique —
    /// lower mints one fresh body fn per lambda site, ADR-0027). `0` when no
    /// site references the fn (a dead body), which is the identity lowering.
    fn body_captures(&self) -> u32 {
        for (_, m) in self.ir.morphisms() {
            match m.op {
                Operation::Map { body, captures } | Operation::Fold { body, captures }
                    if body == self.f =>
                {
                    return captures;
                }
                _ => {}
            }
        }
        0
    }

    fn walk(&mut self) {
        // Driver-owned morphisms: everything in a loop plan's decide/advance
        // cones, plus anything incident to an SCC object. Plan membership is the
        // precise rule — an exit-only payload chain (e.g. a computed exit value,
        // or an exit-arm Print) leaves the SCC but still belongs to the decide
        // cone; skipping by SCC incidence alone would re-emit it after the loop
        // (dead recompute for values, a DOUBLE side effect for Print).
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
                Operation::LoopEnter => crate::loops::emit_loop(self, morph.target),
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue; // driver-owned
                    }
                    self.emit_morphism(m);
                }
            }
        }
    }

    /// Emit one morphism (DESIGN §2 op table). Called by the straight-line walk
    /// and by the loop driver for decide/advance cones.
    pub(crate) fn emit_morphism(&mut self, m: MorphismId) {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        let op = morph.op;
        let source = morph.source;
        let target = morph.target;

        match op {
            Operation::Pair { slot, .. } => {
                if matches!(self.obj_ty(source), Ty::Array { .. }) {
                    let sllt = lower_ty(&self.obj_ty(source)).expect("array lowers");
                    let src = self
                        .array_operand_ptr(source, None)
                        .expect("Pair array source ptr");
                    let (dllt, dst) = self
                        .component_ptr(target, slot)
                        .expect("Pair array target ptr");
                    if dllt == "ptr" {
                        self.line(format!("store ptr {src}, ptr {dst}"));
                    } else {
                        self.emit_memcpy(&dst, &src, &sllt);
                    }
                } else if let Some((sllt, sval)) = self.load_whole(source)
                    && let Some((_, ptr)) = self.component_ptr(target, slot)
                {
                    self.line(format!("store {sllt} {sval}, ptr {ptr}"));
                }
            }
            Operation::Proj { index } => {
                if matches!(
                    self.obj_ty(source).component_ty(index),
                    Some(Ty::Array { .. })
                ) {
                    let src = self
                        .array_operand_ptr(source, Some(index))
                        .expect("Proj array source ptr");
                    let dst = self.slot(target).expect("Proj array target slot");
                    if self.ptr_resident.contains_key(target) {
                        self.line(format!("store ptr {src}, ptr {dst}"));
                    } else {
                        let llt = lower_ty(&self.obj_ty(target)).expect("Proj array lowers");
                        self.emit_memcpy(&dst, &src, &llt);
                    }
                } else if let Some((cllt, val)) = self.load_component(source, index) {
                    self.store_obj(target, &cllt, &val);
                }
            }
            Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
                self.emit_arith(source, target, op);
            }
            Operation::Neg => {
                let (llt, val) = self.load_whole(source).expect("neg operand");
                let r = self.tmp();
                if is_float(&self.obj_ty(source)) {
                    self.line(format!("{r} = fneg {llt} {val}"));
                } else {
                    self.line(format!("{r} = sub {llt} 0, {val}"));
                }
                self.store_obj(target, &llt, &r);
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
                let (_, a) = self.load_component(source, 0).expect("logic a");
                let (_, b) = self.load_component(source, 1).expect("logic b");
                let iop = if op == Operation::And { "and" } else { "or" };
                let r = self.tmp();
                self.line(format!("{r} = {iop} i1 {a}, {b}"));
                self.store_obj(target, "i1", &r);
            }
            Operation::Not => {
                let (_, val) = self.load_whole(source).expect("not operand");
                let r = self.tmp();
                self.line(format!("{r} = xor i1 {val}, true"));
                self.store_obj(target, "i1", &r);
            }
            Operation::Widen => {
                let (sllt, val) = self.load_whole(source).expect("widen operand");
                let tllt = lower_ty(&self.obj_ty(target)).expect("widen target");
                let cvt = match (self.obj_ty(source), self.obj_ty(target)) {
                    (Ty::Int { bits: 32, .. }, Ty::Int { bits: 64, .. }) => "sext",
                    (Ty::Int { bits: 32, .. }, Ty::Float { .. }) => "sitofp",
                    (Ty::Float { bits: 32 }, Ty::Float { bits: 64 }) => "fpext",
                    _ => unreachable!("invalid Widen pair passed validation"),
                };
                let r = self.tmp();
                self.line(format!("{r} = {cvt} {sllt} {val} to {tllt}"));
                self.store_obj(target, &tllt, &r);
            }
            Operation::Phi => {
                if matches!(self.obj_ty(target), Ty::Array { .. }) {
                    let t = self
                        .array_operand_ptr(source, Some(0))
                        .expect("phi then array ptr");
                    let e = self
                        .array_operand_ptr(source, Some(1))
                        .expect("phi else array ptr");
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let r = self.tmp();
                    self.line(format!("{r} = select i1 {c}, ptr {t}, ptr {e}"));
                    let dst = self.slot(target).expect("phi array target slot");
                    let llt = lower_ty(&self.obj_ty(target)).expect("phi array lowers");
                    self.emit_memcpy(&dst, &r, &llt);
                } else {
                    let (tllt, t) = self.load_component(source, 0).expect("phi then");
                    let (_, e) = self.load_component(source, 1).expect("phi else");
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let r = self.tmp();
                    self.line(format!("{r} = select i1 {c}, {tllt} {t}, {tllt} {e}"));
                    self.store_obj(target, &tllt, &r);
                }
            }
            Operation::Call(g) => self.emit_call(source, target, g),
            Operation::Map { body, captures } => self.emit_map(source, target, body, captures),
            Operation::Fold { body, captures } => self.emit_fold(source, target, body, captures),
            Operation::Index => self.emit_index(m, source, target),
            Operation::Update => self.emit_update(m, source, target),
            Operation::Zip => self.emit_zip(source, target),
            Operation::Enumerate => self.emit_enumerate(source, target),
            Operation::Iota => self.emit_iota(source, target),
            Operation::Fill => self.emit_fill(source, target),
            Operation::Print { newline } => self.emit_print(source, newline),
            Operation::Output => {
                if matches!(self.obj_ty(source), Ty::Array { .. }) {
                    let src = self
                        .array_operand_ptr(source, None)
                        .expect("Output array source ptr");
                    let dst = self.slot(target).expect("Output array target slot");
                    let llt = lower_ty(&self.obj_ty(source)).expect("Output array lowers");
                    self.emit_memcpy(&dst, &src, &llt);
                } else if let Some((llt, val)) = self.load_whole(source) {
                    self.store_obj(target, &llt, &val);
                }
            }
            Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
                unreachable!("loop ops are driver-owned")
            }
        }
    }

    fn emit_arith(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
        let opty = self
            .obj_ty(source)
            .component_ty(0)
            .cloned()
            .expect("arith ty");
        let (llt, a) = self.load_component(source, 0).expect("arith a");
        let (_, b) = self.load_component(source, 1).expect("arith b");

        if is_float(&opty) {
            let fop = match op {
                Operation::Add => "fadd",
                Operation::Sub => "fsub",
                Operation::Mul => "fmul",
                Operation::Div => "fdiv",
                Operation::Mod => "frem",
                _ => unreachable!(),
            };
            let r = self.tmp();
            self.line(format!("{r} = {fop} {llt} {a}, {b}"));
            self.store_obj(target, &llt, &r);
            return;
        }

        let signed = matches!(opty, Ty::Int { signed: true, .. });
        match op {
            Operation::Add | Operation::Sub | Operation::Mul => {
                let iop = match op {
                    Operation::Add => "add",
                    Operation::Sub => "sub",
                    Operation::Mul => "mul",
                    _ => unreachable!(),
                };
                let r = self.tmp();
                self.line(format!("{r} = {iop} {llt} {a}, {b}")); // no nsw/nuw (wraps, L1)
                self.store_obj(target, &llt, &r);
            }
            Operation::Div | Operation::Mod => {
                // #13 constant-divisor credit (S20): a literal non-zero divisor
                // makes the zero guard dead; a literal non-(−1) makes the
                // MIN/−1 guard dead (the oracle's behavior is identical — the
                // checks cannot fire).
                let dconst = const_int_operand(self.ir, source, 1);
                let zero_dead = matches!(dconst, Some(v) if v != 0);
                let min_dead = matches!(dconst, Some(v) if v != -1);
                if !zero_dead {
                    // Zero guard → flow_trap(div_zero).
                    let z = self.tmp();
                    self.line(format!("{z} = icmp eq {llt} {b}, 0"));
                    self.trap_if(&z, 0);
                }

                if signed && !min_dead {
                    // MIN/-1 guard: Div ⇒ MIN, Mod ⇒ 0 (wrapping_div/rem parity).
                    let min = int_min(&llt);
                    let m1 = self.tmp();
                    self.line(format!("{m1} = icmp eq {llt} {b}, -1"));
                    let ismin = self.tmp();
                    self.line(format!("{ismin} = icmp eq {llt} {a}, {min}"));
                    let ov = self.tmp();
                    self.line(format!("{ov} = and i1 {m1}, {ismin}"));
                    let lov = self.label();
                    let lnorm = self.label();
                    let ldone = self.label();
                    self.line(format!("br i1 {ov}, label %{lov}, label %{lnorm}"));
                    self.label_line(&lov);
                    let ovval = if op == Operation::Div { min } else { "0" };
                    self.store_obj(target, &llt, ovval);
                    self.line(format!("br label %{ldone}"));
                    self.label_line(&lnorm);
                    let sop = if op == Operation::Div { "sdiv" } else { "srem" };
                    let r = self.tmp();
                    self.line(format!("{r} = {sop} {llt} {a}, {b}"));
                    self.store_obj(target, &llt, &r);
                    self.line(format!("br label %{ldone}"));
                    self.label_line(&ldone);
                } else {
                    let sop = match (signed, op) {
                        (true, Operation::Div) => "sdiv",
                        (true, Operation::Mod) => "srem",
                        (false, Operation::Div) => "udiv",
                        (false, Operation::Mod) => "urem",
                        _ => unreachable!(),
                    };
                    let r = self.tmp();
                    self.line(format!("{r} = {sop} {llt} {a}, {b}"));
                    self.store_obj(target, &llt, &r);
                }
            }
            _ => unreachable!(),
        }
    }

    fn emit_compare(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
        let opty = self
            .obj_ty(source)
            .component_ty(0)
            .cloned()
            .expect("cmp ty");
        let (llt, a) = self.load_component(source, 0).expect("cmp a");
        let (_, b) = self.load_component(source, 1).expect("cmp b");
        let r = self.tmp();
        if is_float(&opty) {
            let pred = match op {
                Operation::Eq => "oeq",
                Operation::Neq => "une",
                Operation::Lt => "olt",
                Operation::Gt => "ogt",
                Operation::Le => "ole",
                Operation::Ge => "oge",
                _ => unreachable!(),
            };
            self.line(format!("{r} = fcmp {pred} {llt} {a}, {b}"));
        } else {
            let signed = matches!(opty, Ty::Int { signed: true, .. });
            let pred = match op {
                Operation::Eq => "eq",
                Operation::Neq => "ne",
                Operation::Lt => sign_pred(signed, "slt", "ult"),
                Operation::Gt => sign_pred(signed, "sgt", "ugt"),
                Operation::Le => sign_pred(signed, "sle", "ule"),
                Operation::Ge => sign_pred(signed, "sge", "uge"),
                _ => unreachable!(),
            };
            self.line(format!("{r} = icmp {pred} {llt} {a}, {b}"));
        }
        self.store_obj(target, "i1", &r);
    }

    /// A Named call (BL5 amendment, suggestions #8): top-level Array
    /// (components of the) argument go **by reference** — array parameters are
    /// observably read-only (Flow value semantics; functional `Update` copies
    /// to a fresh alloca), so the address is observably the inline array, and
    /// no array bytes cross the call boundary. The lowering is per-signature
    /// (`lower_named_input_ty`), so every call site agrees with the callee's
    /// `FnEmit` by construction — host paths and body fns alike. Scalar-only
    /// arguments keep the by-value form byte-identical.
    fn emit_call(&mut self, source: ObjectId, target: ObjectId, g: FuncId) {
        let callee = self.fnames[g].clone();
        let cfd = self.ir.func(g).expect("callee");
        let in_ty = self.obj_ty(cfd.input);
        let out_ty = self.obj_ty(cfd.output);
        let arg = match lower_named_input_ty(&in_ty) {
            None => String::new(),
            Some(_) if !has_top_array(&in_ty) => {
                let (llt, val) = self.load_whole(source).expect("call arg");
                format!("{llt} {val}")
            }
            Some(arg_llt) if arg_llt == "ptr" => {
                // A single surviving array argument: the bare source's address,
                // or the one array component's.
                let addr = if matches!(&in_ty, Ty::Array { .. }) {
                    self.array_operand_ptr(source, None)
                } else {
                    let k = (0u32..)
                        .find(|&k| matches!(in_ty.component_ty(k), Some(Ty::Array { .. })))
                        .expect("the surviving array component");
                    self.array_operand_ptr(source, Some(k))
                };
                format!("ptr {}", addr.expect("call array arg"))
            }
            Some(arg_llt) => {
                // The product argument, assembled in scratch: Array components
                // store their address, everything else its value — the
                // `body_call_arg` template with every component a "capture".
                self.body_call_arg(source, product_arity(&in_ty), &in_ty, &arg_llt, &[])
            }
        };
        match lower_ty(&out_ty) {
            None => self.line(format!("call void @{callee}({arg})")),
            Some(rty) => {
                let r = self.tmp();
                self.line(format!("{r} = call {rty} @{callee}({arg})"));
                self.store_obj(target, &rty, &r);
            }
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
            // Str comes only from a literal (I9s): the slot-1 Pair source is a
            // Str constant with a private global.
            let p_obj = self.pair_source(source, 1).expect("str print source");
            let g = self.strings.get(p_obj).expect("str global");
            let len = g.bytes.len();
            let name = g.name.clone();
            self.line(format!(
                "call void @flow_print_str(ptr {name}, i64 {len}, i1 zeroext {nl})"
            ));
            return;
        }
        let (func, ze, tystr) = print_dispatch(&pty);
        let (_, val) = self.load_component(source, 1).expect("print value");
        // Param attr goes *after* the type in a call arg (`i8 zeroext %v`), like
        // the trailing newline `i1 zeroext` — attr-before-type is invalid LLVM.
        let ze = if ze { "zeroext " } else { "" };
        self.line(format!(
            "call void @{func}({tystr} {ze}{val}, i1 zeroext {nl})"
        ));
    }

    /// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`.
    fn pair_source(&self, agg: ObjectId, k: u32) -> Option<ObjectId> {
        for &m in self.ir.in_edges(agg) {
            let morph = self.ir.morphism(m).expect("morphism");
            if let Operation::Pair { slot, .. } = morph.op
                && slot == k
            {
                return Some(morph.source);
            }
        }
        None
    }

    /// Rule 4's in-place legality for the `Update` morphism `m` (plan-last-use
    /// §2; suggestions #2): the source array object, when the plan proves it
    /// `dead_after` this Update — every use ranked at/before it under rule 1's
    /// decide < `LoopExit` < advance < `LoopBack`, ¬escapes, ¬carried — so the
    /// whole-array memcpy may be skipped and the target may SHARE the source's
    /// slot (the element store lands in place; in the loop-carried matmul4
    /// shape the write goes straight into the merge's own storage and the
    /// back-edge copy becomes an identity). A ptr-resident (by-ref) source is
    /// never eligible — the store would land in caller memory (the plan's
    /// rule 2 already marks borrowed inputs escaping; this check is the
    /// emitted-text-level second line). `None` keeps the fresh-alloca copy.
    fn update_in_place_source(&self, m: MorphismId) -> Option<ObjectId> {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        if morph.op != Operation::Update {
            return None;
        }
        let arr = self.pair_source(morph.source, 0)?;
        if self.ptr_resident.contains_key(arr)
            || self.ir.object(arr).expect("object resolves").kind == ObjectKind::Constant
        {
            return None;
        }
        let idx = self.lup.position(m)?;
        self.lup.dead_after(arr, idx).then_some(arr)
    }

    fn emit_index(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("index array");
        let (elem_llt, size) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("array lowers");
        let arr_ptr = self
            .array_operand_ptr(source, Some(0))
            .expect("index array ptr");
        let idx_ty = src_ty.component_ty(1).cloned().expect("index i");
        let i64idx = self.load_index(source, 1, &idx_ty);
        // Guard elision (S20 `bounds_proof`; the vectorization unlock): the
        // plan proves the index statically inside `[0, size)` — the trap is
        // dead, so a proven `Index` emits just the GEP+load. Everything
        // unproven keeps the two-sided guard byte-identical.
        if !self.bp.proven(m) {
            self.guard_index(&i64idx, size);
        }
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {i64idx}"
        ));
        let v = self.tmp();
        self.line(format!("{v} = load {elem_llt}, ptr {ep}"));
        self.store_obj(target, &elem_llt, &v);
    }

    fn emit_update(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let arr_ty = src_ty.component_ty(0).cloned().expect("update array");
        let (_elem_llt, size) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("array lowers");
        let idx_ty = src_ty.component_ty(1).cloned().expect("update i");
        // Last-use elision (suggestions #2; plan-last-use §2 rule 4): a dead
        // source shares its slot with the target — the memcpy is skipped and
        // the element store below lands in place. The copy path emits
        // byte-identical text to before.
        let (tgt_slot, copy_from) = match self.update_in_place_source(m) {
            Some(arr) => {
                let slot = self.slot(arr).expect("in-place update source slot");
                self.slots.insert(target, slot.clone());
                (slot, None)
            }
            None => (
                self.slot(target).expect("update target slot"),
                Some(
                    self.array_operand_ptr(source, Some(0))
                        .expect("update src ptr"),
                ),
            ),
        };
        let i64idx = self.load_index(source, 1, &idx_ty);
        self.guard_index(&i64idx, size);
        if let Some(src_arr_ptr) = copy_from {
            // memcpy source array → target (fresh array; ADR-0021). Size via the
            // gep-null sizeof constant expr (handles element padding).
            self.line(format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {tgt_slot}, ptr {src_arr_ptr}, i64 ptrtoint (ptr getelementptr ({arr_llt}, ptr null, i64 1) to i64), i1 false)"
            ));
        }
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {arr_llt}, ptr {tgt_slot}, i64 0, i64 {i64idx}"
        ));
        let (vllt, val) = self.load_component(source, 2).expect("update value");
        self.line(format!("store {vllt} {val}, ptr {ep}"));
    }

    /// Load index component `k`, zero/sign-extended to i64 per its ty (S13
    /// type-directed rule: u8 zext, signed sext).
    fn load_index(&mut self, agg: ObjectId, k: u32, idx_ty: &Ty) -> String {
        let (illt, idx) = self.load_component(agg, k).expect("index operand");
        if illt == "i64" {
            return idx;
        }
        let ext = if matches!(idx_ty, Ty::Int { signed: false, .. }) {
            "zext"
        } else {
            "sext"
        };
        let e = self.tmp();
        self.line(format!("{e} = {ext} {illt} {idx} to i64"));
        e
    }

    /// Trap when the i64 index is out of `[0, size)`. Unsigned (u8) indices skip
    /// the lower bound (they zero-extend, never negative — S13).
    fn guard_index(&mut self, i64idx: &str, size: u64) {
        // The extension already erased signedness; but the operand's original
        // signedness decided zext vs sext. A zero-extended value is ≥ 0, so the
        // signed two-sided check is always correct on the i64 form.
        let lo = self.tmp();
        self.line(format!("{lo} = icmp slt i64 {i64idx}, 0"));
        let hi = self.tmp();
        self.line(format!("{hi} = icmp sge i64 {i64idx}, {size}"));
        let oob = self.tmp();
        self.line(format!("{oob} = or i1 {lo}, {hi}"));
        self.trap_if(&oob, 1);
    }

    /// Assemble a capturing map/fold body's call operand (ADR-0027): the
    /// `(c₁…cₖ, rest…)` product in a fresh scratch — capture components `0..k`
    /// loaded from the op's source product (the broadcast edges), then the
    /// per-iteration `rest` components (`elem` for map; `acc, elem` for fold).
    /// Returns the `{llt} {val}` call operand. Erasure applies as usual: an
    /// erased component has no representation (`field_store` remaps via
    /// `erased_index`; a lowered capture is never erased — L1605).
    ///
    /// An Array capture travels **by reference** (suggestions #6): its scratch
    /// field is a `ptr` (matching the body fn's by-ref signature) holding the
    /// array's address — the forwarded capture pointer when the `Pair` feeder
    /// is itself ptr-resident (the transitive fold-in-map case), the source
    /// product's component address otherwise. No array bytes move per call.
    /// (`emit_call` reuses this template for a Named call's whole argument —
    /// every component a "capture", no `rest` — suggestions #8.)
    fn body_call_arg(
        &mut self,
        source: ObjectId,
        captures: u32,
        arg_ty: &Ty,
        arg_llt: &str,
        rest: &[(&str, &str)],
    ) -> String {
        let buf = self.scratch(arg_llt);
        for i in 0..captures {
            if matches!(arg_ty.component_ty(i), Some(Ty::Array { .. })) {
                let addr = self
                    .array_operand_ptr(source, Some(i))
                    .expect("body capture addr");
                self.field_store(&buf, arg_ty, arg_llt, i, "ptr", &addr);
            } else {
                let (cllt, cval) = self.load_component(source, i).expect("body capture");
                self.field_store(&buf, arg_ty, arg_llt, i, &cllt, &cval);
            }
        }
        for (j, &(rllt, rval)) in rest.iter().enumerate() {
            self.field_store(&buf, arg_ty, arg_llt, captures + j as u32, rllt, rval);
        }
        let whole = self.tmp();
        self.line(format!("{whole} = load {arg_llt}, ptr {buf}"));
        format!("{arg_llt} {whole}")
    }

    fn emit_map(&mut self, source: ObjectId, target: ObjectId, body: FuncId, captures: u32) {
        let src_ty = self.obj_ty(source);
        // The mapped array: the bare source (k=0) or the source product's last
        // component (k>0 — ADR-0027: source `(c₁…cₖ, [T; n])`, captures leading).
        let (arr_ty, arr_ptr) = if captures == 0 {
            let ptr = self.array_operand_ptr(source, None).expect("map src slot");
            (src_ty, ptr)
        } else {
            let arr_ty = src_ty.component_ty(captures).cloned().expect("map array");
            let ptr = self
                .array_operand_ptr(source, Some(captures))
                .expect("map array ptr");
            (arr_ty, ptr)
        };
        let (tllt, n) = array_parts(&arr_ty);
        let src_arr_llt = lower_ty(&arr_ty).expect("map src lowers");
        let tgt_ty = self.obj_ty(target);
        let (ullt, _) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("map tgt lowers");
        let tgt_slot = self.slot(target).expect("map tgt slot");
        let callee = self.fnames[body].clone();
        let ctr = self.scratch("i64");

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {src_arr_llt}, ptr {arr_ptr}, i64 0, i64 {iv}"
        ));
        let e = self.tmp();
        self.line(format!("{e} = load {tllt}, ptr {ep}"));
        // The body call's argument: the bare element (k=0) or the assembled
        // `(c₁…cₖ, elem)` product (k>0), per the body fn's input ty — with
        // Array captures by reference, matching the body fn's signature.
        let arg = if captures == 0 {
            format!("{tllt} {e}")
        } else {
            let arg_ty = self.obj_ty(self.ir.func(body).expect("map body").input);
            let arg_llt = lower_body_input_ty(&arg_ty, captures).expect("map body input lowers");
            self.body_call_arg(source, captures, &arg_ty, &arg_llt, &[(&tllt, &e)])
        };
        let r = self.tmp();
        self.line(format!("{r} = call {ullt} @{callee}({arg})"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store {ullt} {r}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    fn emit_fold(&mut self, source: ObjectId, target: ObjectId, body: FuncId, captures: u32) {
        let src_ty = self.obj_ty(source);
        // ADR-0027: source `(c₁…cₖ, Acc, [T; n])` (k=0: `(Acc, [T; n])`) — the
        // accumulator is component k, the folded array component k+1.
        let arr_ty = src_ty
            .component_ty(captures + 1)
            .cloned()
            .expect("fold array");
        let (tllt, n) = array_parts(&arr_ty);
        let arr_llt = lower_ty(&arr_ty).expect("fold array lowers");
        let (acc_llt, acc0) = self.load_component(source, captures).expect("fold acc");
        let arr_ptr = self
            .array_operand_ptr(source, Some(captures + 1))
            .expect("fold array ptr");

        let callee = self.fnames[body].clone();
        let pair_ty = self.obj_ty(self.ir.func(body).expect("fold body").input);
        let pair_llt = lower_body_input_ty(&pair_ty, captures).expect("fold pair lowers");

        let accslot = self.scratch(&acc_llt);
        let ctr = self.scratch("i64");
        self.line(format!("store {acc_llt} {acc0}, ptr {accslot}"));
        self.line(format!("store i64 0, ptr {ctr}"));

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {iv}"
        ));
        let e = self.tmp();
        self.line(format!("{e} = load {tllt}, ptr {ep}"));
        let a = self.tmp();
        self.line(format!("{a} = load {acc_llt}, ptr {accslot}"));
        // The step call's argument: the `(c₁…cₖ, acc, elem)` product (k=0:
        // `(acc, elem)`), assembled in scratch per the body fn's input ty.
        let arg = self.body_call_arg(
            source,
            captures,
            &pair_ty,
            &pair_llt,
            &[(&acc_llt, &a), (&tllt, &e)],
        );
        let na = self.tmp();
        self.line(format!("{na} = call {acc_llt} @{callee}({arg})"));
        self.line(format!("store {acc_llt} {na}, ptr {accslot}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
        let fin = self.tmp();
        self.line(format!("{fin} = load {acc_llt}, ptr {accslot}"));
        self.store_obj(target, &acc_llt, &fin);
    }

    fn emit_zip(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let a_ty = src_ty.component_ty(0).cloned().expect("zip a");
        let b_ty = src_ty.component_ty(1).cloned().expect("zip b");
        let (allt, n) = array_parts(&a_ty);
        let (bllt, _) = array_parts(&b_ty);
        let a_arr_llt = lower_ty(&a_ty).expect("zip a lowers");
        let b_arr_llt = lower_ty(&b_ty).expect("zip b lowers");
        let a_ptr = self.array_operand_ptr(source, Some(0)).expect("zip a ptr");
        let b_ptr = self.array_operand_ptr(source, Some(1)).expect("zip b ptr");

        let tgt_ty = self.obj_ty(target);
        let elem_ty = tgt_ty.component_ty(0).cloned().expect("zip elem");
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("zip tgt lowers");
        let tgt_slot = self.slot(target).expect("zip tgt slot");
        let ctr = self.scratch("i64");

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let ea = {
            let p = self.tmp();
            self.line(format!(
                "{p} = getelementptr {a_arr_llt}, ptr {a_ptr}, i64 0, i64 {iv}"
            ));
            let v = self.tmp();
            self.line(format!("{v} = load {allt}, ptr {p}"));
            v
        };
        let eb = {
            let p = self.tmp();
            self.line(format!(
                "{p} = getelementptr {b_arr_llt}, ptr {b_ptr}, i64 0, i64 {iv}"
            ));
            let v = self.tmp();
            self.line(format!("{v} = load {bllt}, ptr {p}"));
            v
        };
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        let elem_llt = lower_ty(&elem_ty).expect("zip elem lowers");
        self.field_store(&dp, &elem_ty, &elem_llt, 0, &allt, &ea);
        self.field_store(&dp, &elem_ty, &elem_llt, 1, &bllt, &eb);
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    fn emit_enumerate(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let (allt, n) = array_parts(&src_ty);
        let src_arr_llt = lower_ty(&src_ty).expect("enum src lowers");
        let src_slot = self.array_operand_ptr(source, None).expect("enum src ptr");

        let tgt_ty = self.obj_ty(target);
        let elem_ty = tgt_ty.component_ty(0).cloned().expect("enum elem");
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("enum tgt lowers");
        let tgt_slot = self.slot(target).expect("enum tgt slot");
        let ctr = self.scratch("i64");

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let idx32 = self.tmp();
        self.line(format!("{idx32} = trunc i64 {iv} to i32"));
        let ep = self.tmp();
        self.line(format!(
            "{ep} = getelementptr {src_arr_llt}, ptr {src_slot}, i64 0, i64 {iv}"
        ));
        let ea = self.tmp();
        self.line(format!("{ea} = load {allt}, ptr {ep}"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        let elem_llt = lower_ty(&elem_ty).expect("enum elem lowers");
        self.field_store(&dp, &elem_ty, &elem_llt, 0, "i32", &idx32);
        self.field_store(&dp, &elem_ty, &elem_llt, 1, &allt, &ea);
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    /// `Iota` (ADR-0029): `out[i] = (i32)i`. The count is the (builder-minted)
    /// constant object; `n` rides the target type (validate ties them), so no
    /// source read is needed. Trap-free by construction.
    fn emit_iota(&mut self, _source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("iota tgt lowers");
        let tgt_slot = self.slot(target).expect("iota tgt slot");
        let ctr = self.scratch("i64");

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let idx32 = self.tmp();
        self.line(format!("{idx32} = trunc i64 {iv} to i32"));
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store i32 {idx32}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    /// `Fill` (ADR-0029): `out[i] = x` — the internal (x, count) pair feeds
    /// the value; `n` rides the target type (validate ties them). Trap-free.
    fn emit_fill(&mut self, source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("fill tgt lowers");
        let tgt_slot = self.slot(target).expect("fill tgt slot");
        let (vllt, v) = self.load_component(source, 0).expect("fill value");
        let ctr = self.scratch("i64");

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 0, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {n}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        let dp = self.tmp();
        self.line(format!(
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {iv}"
        ));
        self.line(format!("store {vllt} {v}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    // --- loop-driver hooks (used by loops.rs) ----------------------------

    /// Copy object `from`'s whole value into object `to`'s slot (init→merge,
    /// next→merge). No-op if either side is erased.
    pub(crate) fn copy_obj(&mut self, from: ObjectId, to: ObjectId) {
        if matches!(self.obj_ty(from), Ty::Array { .. }) {
            let src = self
                .array_operand_ptr(from, None)
                .expect("array copy source ptr");
            let dst = self.slot(to).expect("array copy target slot");
            let llt = lower_ty(&self.obj_ty(from)).expect("array copy lowers");
            self.emit_memcpy(&dst, &src, &llt);
        } else if let Some((llt, val)) = self.load_whole(from) {
            self.store_obj(to, &llt, &val);
        }
    }

    /// Copy component `k` of aggregate `route` into object `to`'s slot
    /// (exit payload → exit object). No-op if erased.
    pub(crate) fn copy_component(&mut self, route: ObjectId, k: u32, to: ObjectId) {
        if matches!(self.obj_ty(route).component_ty(k), Some(Ty::Array { .. })) {
            let src = self
                .array_operand_ptr(route, Some(k))
                .expect("array route source ptr");
            let dst = self.slot(to).expect("array route target slot");
            let llt = lower_ty(&self.obj_ty(to)).expect("array route lowers");
            self.emit_memcpy(&dst, &src, &llt);
        } else if let Some((llt, val)) = self.load_component(route, k) {
            self.store_obj(to, &llt, &val);
        }
    }

    /// Load component `k` of `route` as a bare operand (the loop guard bool).
    pub(crate) fn load_route_component(&mut self, route: ObjectId, k: u32) -> String {
        self.load_component(route, k).expect("route component").1
    }
}

// --- free helpers ---------------------------------------------------------

/// The slot-`k` feeder of a product object (the free form of
/// `FnEmit::pair_source`, for the analysis passes — mirror of cuda
/// kernel.rs's free helper).
fn pair_source_ir(ir: &CategoryIr, agg: ObjectId, k: u32) -> Option<ObjectId> {
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

/// A literal int operand (the constant behind a pair slot), for the #13
/// constant-divisor credit (S20). `None` when not a literal int.
fn const_int_operand(ir: &CategoryIr, source: ObjectId, k: u32) -> Option<i128> {
    let obj = ir
        .object(pair_source_ir(ir, source, k)?)
        .expect("object resolves");
    if obj.kind != flow_ir::ObjectKind::Constant {
        return None;
    }
    match &obj.value {
        Some(flow_ir::Value::I32(n)) => Some(*n as i128),
        Some(flow_ir::Value::I64(n)) => Some(*n as i128),
        Some(flow_ir::Value::U8(n)) => Some(*n as i128),
        _ => None,
    }
}

fn is_float(ty: &Ty) -> bool {
    matches!(ty, Ty::Float { .. })
}

/// Does `ty` carry a top-level Array — the ty itself, or a direct product
/// component (nested products-in-products do NOT count: they stay by value,
/// suggestions #8's recorded limitation)?
fn has_top_array(ty: &Ty) -> bool {
    match ty {
        Ty::Array { .. } => true,
        Ty::Tuple(ts) => ts.iter().any(|t| matches!(t, Ty::Array { .. })),
        Ty::Struct { fields, .. } => fields.iter().any(|(_, t)| matches!(t, Ty::Array { .. })),
        _ => false,
    }
}

/// The direct component count of a Tuple/Struct (0 for anything else).
fn product_arity(ty: &Ty) -> u32 {
    match ty {
        Ty::Tuple(ts) => ts.len() as u32,
        Ty::Struct { fields, .. } => fields.len() as u32,
        _ => 0,
    }
}

fn sign_pred(signed: bool, s: &'static str, u: &'static str) -> &'static str {
    if signed { s } else { u }
}

/// `(elem_llt, size)` of an `Array` ty.
fn array_parts(ty: &Ty) -> (String, u64) {
    match ty {
        Ty::Array { elem, size } => (lower_ty(elem).expect("array elem lowers"), *size),
        _ => unreachable!("expected an array ty"),
    }
}

fn int_min(llt: &str) -> &'static str {
    match llt {
        "i32" => "-2147483648",
        "i64" => "-9223372036854775808",
        "i8" => "-128",
        _ => unreachable!("non-Core int width in Div/Mod"),
    }
}

/// `(flow_rt_func, needs_zeroext, llvm_ty)` for a printable scalar.
fn print_dispatch(ty: &Ty) -> (&'static str, bool, &'static str) {
    match ty {
        Ty::Int { bits: 32, .. } => ("flow_print_i32", false, "i32"),
        Ty::Int { bits: 64, .. } => ("flow_print_i64", false, "i64"),
        Ty::Int { bits: 8, .. } => ("flow_print_u8", true, "i8"),
        Ty::Bool => ("flow_print_bool", true, "i1"),
        Ty::Float { bits: 32 } => ("flow_print_f32", false, "float"),
        Ty::Float { bits: 64 } => ("flow_print_f64", false, "double"),
        _ => unreachable!("non-printable Print operand"),
    }
}

/// The LLVM constant text for a scalar `Value` (floats as the 16-hex-digit form
/// LLVM uses for both `float` and `double`).
fn const_literal(v: &Value) -> String {
    match v {
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::F32(x) => format!("0x{:016X}", (*x as f64).to_bits()),
        Value::F64(x) => format!("0x{:016X}", x.to_bits()),
        Value::Str(_) => unreachable!("Str is not a scalar operand"),
    }
}
