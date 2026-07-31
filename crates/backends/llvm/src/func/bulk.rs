//! the bulk collection ops: map, fold, zip, enumerate, iota, fill
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// S44's **move-panel traversal**: permute a bulk `map`'s loop counter so the
    /// iteration space is visited in `b × b` tiles of a `w`-wide 2-D geometry
    /// instead of in linear order. Returns the index to *use* on this trip; the
    /// loop counter, its bounds and its trip count are untouched.
    ///
    /// **A permutation of the counter, not a loop nest** — and that is the whole
    /// correctness argument. `perm` is a bijection of `[0, n)`, the parallel
    /// slices partition the counter, so the images of those slices still
    /// partition the outputs: every element is visited exactly once, by exactly
    /// one worker, and the values are bit-identical. It also means `%lo`/`%hi`
    /// need no special handling — a blocked *nest* would have to grow a head and
    /// a tail for the partial rows at a slice boundary, and this does not.
    ///
    /// ```text
    /// p  = ((rb·CB + cb)·B + dr)·B + dc      -- the counter, decomposed
    /// t  = (rb·B + dr)·W + cb·B + dc         -- the index it stands for
    /// ```
    ///
    /// Every divisor is a compile-time constant, so `-O2` turns the `udiv`/`urem`
    /// pair into shifts when `b` is a power of two.
    ///
    /// Declines — returning the counter unchanged — unless the panel divides the
    /// geometry both ways. A panel that does not tile the space would need a
    /// remainder arm, and the honest answer at that point is that this rung has
    /// nothing to say about that shape.
    fn move_panel_index(&mut self, iv: &str, n: u64) -> String {
        let Some((w, b)) = self.move_panel else {
            return iv.to_owned();
        };
        if w == 0 || b == 0 || n % w != 0 {
            return iv.to_owned();
        }
        let rows = n / w;
        if rows < 2 || !w.is_multiple_of(b) || !rows.is_multiple_of(b) {
            return iv.to_owned();
        }
        let col_blocks = w / b;
        let op = |emit: &mut Self, code: &str, lhs: &str, rhs: u64| {
            let t = emit.tmp();
            emit.line(format!("{t} = {code} i64 {lhs}, {rhs}"));
            t
        };
        let dc = op(self, "urem", iv, b);
        let q = op(self, "udiv", iv, b);
        let dr = op(self, "urem", &q, b);
        let q2 = op(self, "udiv", &q, b);
        let cb = op(self, "urem", &q2, col_blocks);
        let rb = op(self, "udiv", &q2, col_blocks);
        let row = op(self, "mul", &rb, b);
        let row = {
            let t = self.tmp();
            self.line(format!("{t} = add i64 {row}, {dr}"));
            t
        };
        let row = op(self, "mul", &row, w);
        let col = op(self, "mul", &cb, b);
        let idx = self.tmp();
        self.line(format!("{idx} = add i64 {row}, {col}"));
        let out = self.tmp();
        self.line(format!("{out} = add i64 {idx}, {dc}"));
        out
    }

    pub(super) fn emit_map(
        &mut self,
        m: MorphismId,
        source: ObjectId,
        target: ObjectId,
        body: FuncId,
        captures: u32,
    ) {
        if let Some(site) = self
            .tile_plan
            .as_ref()
            .and_then(|plan| plan.sites.get(m))
            // Non-conv k-split sites keep the untiled body-call fallback
            // (rule 3): the affine tile emission ignores `ksplit` and would
            // compute wrong addresses. Conv-shaped k-split sites pass through
            // to the unrolled micro-kernel.
            .filter(|site| (site.a.ksplit.is_none() && site.b.ksplit.is_none()) || conv_site(site))
            .cloned()
        {
            let packed = if self.packing && packing_site(&site) {
                let needs_pack = self
                    .frame
                    .as_ref()
                    .and_then(|frame| frame.packed.get(m))
                    .is_none();
                let packed = self.packed_buffer(m, &site);
                if needs_pack {
                    self.emit_pack_copy(source, &site, &packed);
                }
                Some(packed)
            } else {
                None
            };
            self.emit_tiled_map(source, target, &site, packed);
            return;
        }

        let src_ty = self.obj_ty(source);
        // The mapped array: the bare source (k=0) or the source product's last
        // component (k>0 — ADR-0027: source `(c₁…cₖ, [T; n])`, captures leading).
        // The pointer is taken LAZILY: an elided array (step 3b) has no
        // `%Frame` field, so asking for its slot would panic — and the whole
        // point is that this path never needs it.
        let (arr_ty, arr_slot) = if captures == 0 {
            (src_ty, None)
        } else {
            (
                src_ty.component_ty(captures).cloned().expect("map array"),
                Some(captures),
            )
        };
        let (tllt, n) = array_parts(&arr_ty);
        let tgt_ty = self.obj_ty(target);
        let (ullt, _) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("map tgt lowers");
        let tgt_slot = self.slot(target).expect("map tgt slot");
        let callee = self.fnames[body].clone();
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        // S44: the index this trip stands for. `move_panel` off (the default) or
        // declining returns `iv` itself, so the text below is character-identical
        // to what it has always been.
        let ix = self.move_panel_index(&iv, n);
        // plan-s37-stage-structure: if `elem_plan` knows what `arr[i]` IS, build
        // it here instead of reading it back out of memory. The intermediate
        // array is still emitted — this is the query, not a rewrite; whether the
        // buffer survives is a separate (backend-owned) decision. `None` keeps
        // the load, which is what every case did before and is always correct.
        let mapped = if captures == 0 {
            Some(source)
        } else {
            self.pair_source(source, captures)
        };
        let law = mapped.and_then(|o| self.elem.src(o)).cloned();
        let inlined = law
            .filter(|l| !matches!(l, ElemSrc::Load { .. }))
            .zip(arr_ty.component_ty(0).cloned())
            .and_then(|(l, elem_ty)| self.emit_elem(&l, &elem_ty, &ix));
        let e = match inlined {
            Some((_, v)) => v,
            None => {
                let src_arr_llt = lower_ty(&arr_ty).expect("map src lowers");
                let arr_ptr = self
                    .array_operand_ptr(source, arr_slot)
                    .expect("map src slot");
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {src_arr_llt}, ptr {arr_ptr}, i64 0, i64 {ix}"
                ));
                let e = self.tmp();
                self.line(format!("{e} = load {tllt}, ptr {ep}"));
                e
            }
        };
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
            "{dp} = getelementptr {tgt_arr_llt}, ptr {tgt_slot}, i64 0, i64 {ix}"
        ));
        self.line(format!("store {ullt} {r}, ptr {dp}"));
        let iv1 = self.tmp();
        self.line(format!("{iv1} = add i64 {iv}, 1"));
        self.line(format!("store i64 {iv1}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&ld);
    }

    pub(super) fn emit_fold(
        &mut self,
        source: ObjectId,
        target: ObjectId,
        body: FuncId,
        captures: u32,
    ) {
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
        let (lo, hi) = self.bulk_bounds(n);
        self.line(format!("store {acc_llt} {acc0}, ptr {accslot}"));
        self.line(format!("store i64 {lo}, ptr {ctr}"));

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
        self.line(format!("br i1 {done}, label %{ld}, label %{lb}"));
        self.label_line(&lb);
        // Same element-law consumption as `emit_map` (plan-s37-stage-structure):
        // a fold over an `iota`/`fill`/`zip` reads the law, not the array. The
        // accumulator chain is untouched — order and arity are exactly as
        // before, so the fold's value semantics cannot move.
        let folded = self.pair_source(source, captures + 1);
        let law = folded.and_then(|o| self.elem.src(o)).cloned();
        let inlined = law
            .filter(|l| !matches!(l, ElemSrc::Load { .. }))
            .zip(arr_ty.component_ty(0).cloned())
            .and_then(|(l, elem_ty)| self.emit_elem(&l, &elem_ty, &iv));
        let e = match inlined {
            Some((_, v)) => v,
            None => {
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {arr_ptr}, i64 0, i64 {iv}"
                ));
                let e = self.tmp();
                self.line(format!("{e} = load {tllt}, ptr {ep}"));
                e
            }
        };
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

    pub(super) fn emit_zip(&mut self, source: ObjectId, target: ObjectId) {
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
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
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

    pub(super) fn emit_enumerate(&mut self, source: ObjectId, target: ObjectId) {
        let src_ty = self.obj_ty(source);
        let (allt, n) = array_parts(&src_ty);
        let src_arr_llt = lower_ty(&src_ty).expect("enum src lowers");
        let src_slot = self.array_operand_ptr(source, None).expect("enum src ptr");

        let tgt_ty = self.obj_ty(target);
        let elem_ty = tgt_ty.component_ty(0).cloned().expect("enum elem");
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("enum tgt lowers");
        let tgt_slot = self.slot(target).expect("enum tgt slot");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
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
    pub(super) fn emit_iota(&mut self, _source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("iota tgt lowers");
        let tgt_slot = self.slot(target).expect("iota tgt slot");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
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
    pub(super) fn emit_fill(&mut self, source: ObjectId, target: ObjectId) {
        let tgt_ty = self.obj_ty(target);
        let (_, n) = array_parts(&tgt_ty);
        let tgt_arr_llt = lower_ty(&tgt_ty).expect("fill tgt lowers");
        let tgt_slot = self.slot(target).expect("fill tgt slot");
        let (vllt, v) = self.load_component(source, 0).expect("fill value");
        let ctr = self.scratch("i64");
        let (lo, hi) = self.bulk_bounds(n);

        let (lh, lb, ld) = (self.label(), self.label(), self.label());
        self.line(format!("store i64 {lo}, ptr {ctr}"));
        self.line(format!("br label %{lh}"));
        self.label_line(&lh);
        let iv = self.tmp();
        self.line(format!("{iv} = load i64, ptr {ctr}"));
        let done = self.tmp();
        self.line(format!("{done} = icmp uge i64 {iv}, {hi}"));
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
