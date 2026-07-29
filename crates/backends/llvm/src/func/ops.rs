//! the op table — `emit_morphism` and the scalar operations it dispatches to
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// Emit one morphism (DESIGN §2 op table). Called by the straight-line walk
    /// and by the loop driver for decide/advance cones.
    pub(crate) fn emit_morphism(&mut self, m: MorphismId) {
        self.emit_checkpoint(m);
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
                self.emit_arith(m, source, target, op);
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
                if let Some(site) = self.gsites.get(m).cloned() {
                    // plan-s39: the condition picks the arm and only that
                    // arm's work runs. The cond's Pair edge (slot 2) is
                    // unconditional and already fired; each branch emits its
                    // arm's own-list, then lands the staged value in the
                    // target's slot.
                    let (_, c) = self.load_component(source, 2).expect("phi cond");
                    let bt = self.label();
                    let bf = self.label();
                    let bj = self.label();
                    self.line(format!("br i1 {c}, label %{bt}, label %{bf}"));
                    for (arm, blk, slot) in
                        [(&site.on_true, &bt, 0u32), (&site.on_false, &bf, 1u32)]
                    {
                        self.label_line(blk);
                        for &g in &arm.own {
                            let gm = self.ir.morphism(g).expect("morphism resolves");
                            if gm.op == Operation::LoopEnter {
                                // plan-s40: the handle stands for its whole
                                // loop unit — the driver CFG is emitted inside
                                // this branch.
                                crate::loops::emit_loop(self, gm.target);
                            } else {
                                self.emit_morphism(g);
                            }
                        }
                        if matches!(self.obj_ty(target), Ty::Array { .. }) {
                            let p = self
                                .array_operand_ptr(source, Some(slot))
                                .expect("phi arm array ptr");
                            let dst = self.slot(target).expect("phi array target slot");
                            let llt = lower_ty(&self.obj_ty(target)).expect("phi array lowers");
                            self.emit_memcpy(&dst, &p, &llt);
                        } else {
                            let (tllt, v) =
                                self.load_component(source, slot).expect("phi arm value");
                            self.store_obj(target, &tllt, &v);
                        }
                        self.line(format!("br label %{bj}"));
                    }
                    self.label_line(&bj);
                } else if matches!(self.obj_ty(target), Ty::Array { .. }) {
                    // Hand-built (non-builder) triple: strict select over both
                    // computed arms.
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
            Operation::Map { body, captures } => self.emit_map(m, source, target, body, captures),
            Operation::Fold { body, captures } => self.emit_fold(source, target, body, captures),
            Operation::Index => self.emit_index(m, source, target),
            Operation::Update => self.emit_update(m, source, target),
            // step 3b: an array every consumer rebuilds from its law is never
            // read, so the store loop that fills it is dead. Skipping it here
            // (rather than trusting DCE) also drops its `%Frame` field, which
            // DCE cannot do — the frame is one object shared across tasks.
            Operation::Zip | Operation::Enumerate | Operation::Iota | Operation::Fill
                if self.elided_arrays.contains_key(target) => {}
            Operation::Zip => self.emit_zip(source, target),
            Operation::Enumerate => self.emit_enumerate(source, target),
            Operation::Iota => self.emit_iota(source, target),
            Operation::Fill => self.emit_fill(source, target),
            Operation::Print { newline } => self.emit_print(source, newline),
            Operation::TimeMs => self.emit_time_ms(target),
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
        if self.watermark && self.local_trap_site(m) {
            self.emit_watermark(m);
        }
    }

    pub(super) fn emit_arith(
        &mut self,
        m: MorphismId,
        source: ObjectId,
        target: ObjectId,
        op: Operation,
    ) {
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
                if !zero_dead && !matches!(self.guard_flavor, GuardFlavor::Host) {
                    self.emit_task_div(m, target, op, &llt, &a, &b, signed, min_dead);
                    return;
                }
                if !zero_dead {
                    // Zero guard → mapal_trap(div_zero).
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_task_div(
        &mut self,
        m: MorphismId,
        target: ObjectId,
        op: Operation,
        llt: &str,
        a: &str,
        b: &str,
        signed: bool,
        min_dead: bool,
    ) {
        let zero = self.tmp();
        self.line(format!("{zero} = icmp eq {llt} {b}, 0"));
        let bad = self.label();
        let good = self.label();
        let done = self.label();
        self.line(format!("br i1 {zero}, label %{bad}, label %{good}"));
        self.label_line(&bad);
        self.record_trap(m, 0);
        self.line(format!("br label %{done}"));
        self.label_line(&good);

        let (good_value, good_block) = if signed && !min_dead {
            let min = int_min(llt);
            let minus_one = self.tmp();
            self.line(format!("{minus_one} = icmp eq {llt} {b}, -1"));
            let is_min = self.tmp();
            self.line(format!("{is_min} = icmp eq {llt} {a}, {min}"));
            let overflow = self.tmp();
            self.line(format!("{overflow} = and i1 {minus_one}, {is_min}"));
            let wrap = self.label();
            let normal = self.label();
            let wrapped = self.label();
            self.line(format!("br i1 {overflow}, label %{wrap}, label %{normal}"));
            self.label_line(&wrap);
            self.line(format!("br label %{wrapped}"));
            self.label_line(&normal);
            let real = self.tmp();
            let instruction = if op == Operation::Div { "sdiv" } else { "srem" };
            self.line(format!("{real} = {instruction} {llt} {a}, {b}"));
            self.line(format!("br label %{wrapped}"));
            self.label_line(&wrapped);
            let value = self.tmp();
            let wrap_value = if op == Operation::Div { min } else { "0" };
            self.line(format!(
                "{value} = phi {llt} [{wrap_value}, %{wrap}], [{real}, %{normal}]"
            ));
            (value, wrapped)
        } else {
            let instruction = match (signed, op) {
                (true, Operation::Div) => "sdiv",
                (true, Operation::Mod) => "srem",
                (false, Operation::Div) => "udiv",
                (false, Operation::Mod) => "urem",
                _ => unreachable!(),
            };
            let value = self.tmp();
            self.line(format!("{value} = {instruction} {llt} {a}, {b}"));
            (value, good)
        };
        self.line(format!("br label %{done}"));
        self.label_line(&done);
        let value = self.tmp();
        self.line(format!(
            "{value} = phi {llt} [0, %{bad}], [{good_value}, %{good_block}]"
        ));
        self.store_obj(target, llt, &value);
    }

    pub(super) fn emit_compare(&mut self, source: ObjectId, target: ObjectId, op: Operation) {
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
    /// observably read-only (Mapal value semantics; functional `Update` copies
    /// to a fresh alloca), so the address is observably the inline array, and
    /// no array bytes cross the call boundary. The lowering is per-signature
    /// (`lower_named_input_ty`), so every call site agrees with the callee's
    /// `FnEmit` by construction — host paths and body fns alike. Scalar-only
    /// arguments keep the by-value form byte-identical.
    pub(super) fn emit_call(&mut self, source: ObjectId, target: ObjectId, g: FuncId) {
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

    pub(super) fn emit_print(&mut self, source: ObjectId, newline: bool) {
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
                "call void @mapal_print_str(ptr {name}, i64 {len}, i1 zeroext {nl})"
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

    /// `time` (plan-time-builtin): the monotonic clock read. The source token
    /// is ordering-only and erases, and the `(IoToken, f64)` target residual-
    /// lowers to the bare `double` (ty.rs `lower_ty`: a one-component residual
    /// IS its component) — so the call result is the target object's value and
    /// no pair is materialized. Emission position in the block IS the ordering
    /// the token models.
    pub(super) fn emit_time_ms(&mut self, target: ObjectId) {
        let r = self.tmp();
        self.line(format!("{r} = call double @mapal_time_ms()"));
        self.store_obj(target, "double", &r);
    }

    /// The source object of the `Pair{slot==k}` edge feeding aggregate `agg`.
    pub(super) fn pair_source(&self, agg: ObjectId, k: u32) -> Option<ObjectId> {
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
    pub(super) fn update_in_place_source(&self, m: MorphismId) -> Option<ObjectId> {
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

    pub(super) fn emit_index(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
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
            if !matches!(self.guard_flavor, GuardFlavor::Host) {
                let oob = self.index_oob(&i64idx, size);
                let bad = self.label();
                let good = self.label();
                let done = self.label();
                self.line(format!("br i1 {oob}, label %{bad}, label %{good}"));
                self.label_line(&bad);
                self.record_trap(m, 1);
                self.line(format!("br label %{done}"));
                self.label_line(&good);
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {i64idx}"
                ));
                let loaded = self.tmp();
                self.line(format!("{loaded} = load {elem_llt}, ptr {ep}"));
                self.line(format!("br label %{done}"));
                self.label_line(&done);
                let value = self.tmp();
                self.line(format!(
                    "{value} = phi {elem_llt} [zeroinitializer, %{bad}], [{loaded}, %{good}]"
                ));
                self.store_obj(target, &elem_llt, &value);
                return;
            }
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

    pub(super) fn emit_update(&mut self, m: MorphismId, source: ObjectId, target: ObjectId) {
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
        if !matches!(self.guard_flavor, GuardFlavor::Host) {
            if let Some(src_arr_ptr) = copy_from {
                self.line(format!(
                    "call void @llvm.memcpy.p0.p0.i64(ptr {tgt_slot}, ptr {src_arr_ptr}, i64 ptrtoint (ptr getelementptr ({arr_llt}, ptr null, i64 1) to i64), i1 false)"
                ));
            }
            let oob = self.index_oob(&i64idx, size);
            let bad = self.label();
            let good = self.label();
            let done = self.label();
            self.line(format!("br i1 {oob}, label %{bad}, label %{good}"));
            self.label_line(&bad);
            self.record_trap(m, 1);
            self.line(format!("br label %{done}"));
            self.label_line(&good);
            let ep = self.tmp();
            self.line(format!(
                "{ep} = getelementptr {arr_llt}, ptr {tgt_slot}, i64 0, i64 {i64idx}"
            ));
            let (vllt, val) = self.load_component(source, 2).expect("update value");
            self.line(format!("store {vllt} {val}, ptr {ep}"));
            self.line(format!("br label %{done}"));
            self.label_line(&done);
            return;
        }
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
    pub(super) fn load_index(&mut self, agg: ObjectId, k: u32, idx_ty: &Ty) -> String {
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
    pub(super) fn guard_index(&mut self, i64idx: &str, size: u64) {
        // The extension already erased signedness; but the operand's original
        // signedness decided zext vs sext. A zero-extended value is ≥ 0, so the
        // signed two-sided check is always correct on the i64 form.
        let oob = self.index_oob(i64idx, size);
        self.trap_if(&oob, 1);
    }

    pub(super) fn index_oob(&mut self, i64idx: &str, size: u64) -> String {
        let lo = self.tmp();
        self.line(format!("{lo} = icmp slt i64 {i64idx}, 0"));
        let hi = self.tmp();
        self.line(format!("{hi} = icmp sge i64 {i64idx}, {size}"));
        let oob = self.tmp();
        self.line(format!("{oob} = or i1 {lo}, {hi}"));
        oob
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
    pub(super) fn body_call_arg(
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
}
