//! per-object slots, name minting, loads/stores, heap + packed buffers, trap sites
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    pub fn new(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        profile: &'static TargetProfile,
    ) -> Self {
        let mut gsites = SecondaryMap::new();
        let mut gated = SecondaryMap::new();
        for site in ir.guard_plan(f).into_iter().filter(GuardSite::gated) {
            for &m in site.on_true.own.iter().chain(site.on_false.own.iter()) {
                gated.insert(m, ());
            }
            gsites.insert(site.phi, site);
        }
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
            gsites,
            gated,
            elided_updates: SecondaryMap::new(),
            update_aliases: SecondaryMap::new(),
            frame: None,
            frame_geps: String::new(),
            guard_flavor: GuardFlavor::Host,
            split_range: false,
            watermark: false,
            host: None,
            runtime_write: false,
            perf_timing: false,
            tile_plan: tiling.then(|| ir.tile_plan(f)),
            elem: ir.elem_plan(f),
            elided_arrays: SecondaryMap::new(),
            packing,
            contract,
            kc_nest,
            profile,
            heap_ok: false,
            heap_used: false,
        }
    }

    pub(crate) fn set_perf_timing(&mut self, perf_timing: bool) {
        self.perf_timing = perf_timing;
    }

    pub(crate) fn set_task_body_site(&mut self, topo: u32) {
        self.guard_flavor = GuardFlavor::TaskBody(topo);
    }

    pub(super) fn fresh(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    pub(super) fn tmp(&mut self) -> String {
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

    pub(super) fn slot(&mut self, o: ObjectId) -> Option<String> {
        if let Some(slot) = self.slots.get(o) {
            return Some(slot.clone());
        }
        let field = self.frame.as_ref()?.fields.get(o)?.clone();
        if let Some(slot) = self.slots.get(field.owner) {
            let slot = slot.clone();
            self.slots.insert(o, slot.clone());
            return Some(slot);
        }
        let slot = format!("%o{}", field.ordinal);
        self.frame_geps.push_str(&format!(
            "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n",
            field.index
        ));
        self.slots.insert(field.owner, slot.clone());
        self.slots.insert(o, slot.clone());
        Some(slot)
    }

    pub(super) fn obj_ty(&self, o: ObjectId) -> Ty {
        self.ir.object(o).expect("object resolves").ty.clone()
    }

    // --- operand materialization -----------------------------------------

    /// Load the whole value of object `o`: a literal for a (scalar) constant, a
    /// `load` from its slot otherwise. `None` if `o` is erased / a `Str`.
    pub(super) fn load_whole(&mut self, o: ObjectId) -> Option<(String, String)> {
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
    pub(super) fn component_ptr(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
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
    pub(super) fn load_component(&mut self, agg: ObjectId, k: u32) -> Option<(String, String)> {
        let (cllt, ptr) = self.component_ptr(agg, k)?;
        let v = self.tmp();
        self.line(format!("{v} = load {cllt}, ptr {ptr}"));
        Some((cllt, v))
    }

    /// Whether this backend consumes `ElemSrc::Apply` (recomputing a
    /// classifiable `Map` producer at the read site). Off: see the refusal arm in
    /// [`FnEmit::emit_elem`] for the measurement. Legality lives in mapal-ir;
    /// this constant is the CPU backend's profitability answer, and it is `false`.
    const APPLY_INLINE: bool = false;

    /// Materialize `out[i]` from its element law (`mapal_ir::ElemSrc`) instead
    /// of loading it, at loop index `iv`. Returns `(llvm type, value operand)`,
    /// or `None` when the law cannot be realized here — in which case the caller
    /// keeps the load, which is always correct.
    ///
    /// This is the backend half of plan-s37-stage-structure: mapal-ir says what
    /// the element *is* (legality, machine-independent); the decision to inline
    /// it rather than read memory is the backend's, because the right answer
    /// differs per target. On this one it is unconditional for the bodyless
    /// laws — at most two loads and a `trunc`, never worse than the load it
    /// replaces.
    ///
    /// `Broadcast` is emitted inside the loop rather than hoisted by hand: the
    /// value is loop-invariant and LICM lifts it, and hand-hoisting would mean
    /// threading a pre-header through every caller for no measured gain.
    pub(super) fn emit_elem(
        &mut self,
        law: &ElemSrc,
        elem_ty: &Ty,
        iv: &str,
    ) -> Option<(String, String)> {
        match law {
            // `iota`: the element is the index. ADR-0029 pins the element type
            // to `i32`, so the source-of-truth check is the type, not the tag —
            // a wider iota would need its own conversion and must not silently
            // truncate.
            ElemSrc::Index => {
                if *elem_ty != Ty::i32() {
                    return None;
                }
                let v = self.tmp();
                self.line(format!("{v} = trunc i64 {iv} to i32"));
                Some(("i32".to_string(), v))
            }
            ElemSrc::Broadcast { source, slot } => self.load_component(*source, *slot),
            ElemSrc::Load { source, slot } => {
                let arr_ty = match slot {
                    None => self.obj_ty(*source),
                    Some(k) => self.obj_ty(*source).component_ty(*k).cloned()?,
                };
                let (cllt, _) = array_parts(&arr_ty);
                let arr_llt = lower_ty(&arr_ty)?;
                let ptr = self.array_operand_ptr(*source, *slot)?;
                let ep = self.tmp();
                self.line(format!(
                    "{ep} = getelementptr {arr_llt}, ptr {ptr}, i64 0, i64 {iv}"
                ));
                let v = self.tmp();
                self.line(format!("{v} = load {cllt}, ptr {ep}"));
                Some((cllt, v))
            }
            // `zip` / `enumerate`: build the pair in registers instead of
            // reading it back out of a materialized array of structs.
            ElemSrc::Pair(a, b) => {
                let a_ty = elem_ty.component_ty(0).cloned()?;
                let b_ty = elem_ty.component_ty(1).cloned()?;
                let (a_llt, a_v) = self.emit_elem(a, &a_ty, iv)?;
                let (b_llt, b_v) = self.emit_elem(b, &b_ty, iv)?;
                let pair_llt = lower_ty(elem_ty)?;
                let p0 = self.tmp();
                self.line(format!(
                    "{p0} = insertvalue {pair_llt} poison, {a_llt} {a_v}, 0"
                ));
                let p1 = self.tmp();
                self.line(format!(
                    "{p1} = insertvalue {pair_llt} {p0}, {b_llt} {b_v}, 1"
                ));
                Some((pair_llt, p1))
            }
            // A classifiable `Map` producer: recompute its element by calling
            // the same body the producer calls, on the recursively-built inner
            // element. Nothing is spliced or merged — this is two calls in one
            // loop, which is why capture identity is not required.
            //
            // REFUSED ON THIS TARGET, on measurement (plan-s37-stage-structure
            // Table B — profitability is the backend's call, and this is the
            // backend making it). Recomputing a producer body is only a win
            // when arithmetic is cheaper than the load it replaces. On a CPU
            // with the array already materialized it is not: enabling this arm
            // put two extra calls inside saxpy's timed loop — `fn1` and `fn2`
            // regenerating `x[i]` and `y0[i]` from the index instead of reading
            // them — and cost 0.72x at one thread (0.4731 -> 0.6586 ms min).
            // Gather, the shape it was built for, came out at 1.17x min but
            // 0.97x median: inside noise.
            //
            // The FACT stays in mapal-ir because it is true and machine-
            // independent; only this consumer declines. A bandwidth-bound
            // target where registers are cheap should reach a different verdict
            // — that asymmetry is the whole reason the decision lives here and
            // not in the query. Re-enable behind an op-count test against an L2
            // round trip, with a measurement that moves a published cell.
            ElemSrc::Apply { array, .. } if !Self::APPLY_INLINE => {
                // Decline, but degrade to reading the producer's materialized
                // output rather than failing: a refusal nested inside a `Pair`
                // must not collapse the pair back to an array-of-structs read.
                let arr = *array;
                self.emit_elem(
                    &ElemSrc::Load {
                        source: arr,
                        slot: None,
                    },
                    elem_ty,
                    iv,
                )
            }
            ElemSrc::Apply {
                body,
                source,
                captures,
                inner,
                array: _,
            } => {
                let src_ty = self.obj_ty(*source);
                let inner_arr_ty = if *captures == 0 {
                    src_ty.clone()
                } else {
                    src_ty.component_ty(*captures).cloned()?
                };
                let inner_elem_ty = inner_arr_ty.component_ty(0).cloned()?;
                let (in_llt, in_v) = self.emit_elem(inner, &inner_elem_ty, iv)?;
                let out_llt = lower_ty(elem_ty)?;
                let callee = self.fnames[*body].clone();
                let arg = if *captures == 0 {
                    format!("{in_llt} {in_v}")
                } else {
                    let arg_ty = self.obj_ty(self.ir.func(*body)?.input);
                    let arg_llt = lower_body_input_ty(&arg_ty, *captures)?;
                    self.body_call_arg(*source, *captures, &arg_ty, &arg_llt, &[(&in_llt, &in_v)])
                };
                let v = self.tmp();
                self.line(format!("{v} = call {out_llt} @{callee}({arg})"));
                Some((out_llt, v))
            }
        }
    }

    /// Store `(llt, val)` into object `o`'s slot, if it has one.
    pub(super) fn store_obj(&mut self, o: ObjectId, llt: &str, val: &str) {
        if let Some(slot) = self.slot(o) {
            self.line(format!("store {llt} {val}, ptr {slot}"));
        }
    }

    /// A fresh scratch alloca of `llt` in the entry block; returns its ptr name.
    pub(super) fn scratch(&mut self, llt: &str) -> String {
        let name = format!("%s{}", self.fresh());
        self.allocas.push_str(&format!("  {name} = alloca {llt}\n"));
        name
    }

    /// Does a block of `bytes` belong in the arena rather than the stack
    /// (plan-s29 emission item 4)? Records the teardown debt when it does.
    pub(super) fn heap_block(&mut self, bytes: u64) -> bool {
        let heap = self.heap_ok && bytes >= self.profile.heap_min_bytes;
        self.heap_used |= heap;
        heap
    }

    /// One named entry-block allocation of `llt`: today's `alloca` (with the
    /// explicit `align` when the site wants one), or an arena block once it
    /// crosses [`TargetProfile::heap_min_bytes`]. An `alloca` and a `mapal_rt_alloc` result are
    /// both just a `ptr`, so every `getelementptr {llt}, ptr …` consumer is
    /// unchanged — the swap is invisible below this line.
    pub(super) fn entry_alloc(&mut self, name: &str, llt: &str, align: Option<u64>) {
        let bytes = llt_bytes(llt);
        let text = if self.heap_block(bytes) {
            let align = align.unwrap_or_else(|| llt_align(llt));
            format!("  {name} = call ptr @mapal_rt_alloc(i64 {bytes}, i64 {align})\n")
        } else {
            match align {
                Some(align) => format!("  {name} = alloca {llt}, align {align}\n"),
                None => format!("  {name} = alloca {llt}\n"),
            }
        };
        self.allocas.push_str(&text);
    }

    /// Release the arena, if this emitter filled any of it. Emitted once, at
    /// the last point that can read arena memory (plan-s29 composition rule 4).
    pub(super) fn heap_teardown(&mut self) {
        if self.heap_used {
            self.line("call void @mapal_rt_free_all()");
        }
    }

    pub(super) fn packed_type(profile: &TargetProfile, site: &TileSite) -> String {
        let tile_j = profile.tile_j(&site.elem);
        let tiles = site.c.div_ceil(tile_j);
        let elems = site
            .k
            .checked_mul(tiles)
            .and_then(|n| n.checked_mul(tile_j))
            .expect("packed tile size fits u64");
        format!(
            "[{elems} x {}]",
            lower_ty(&site.elem).expect("tile element lowers")
        )
    }

    pub(super) fn packed_buffer(&mut self, m: MorphismId, site: &TileSite) -> PackedBuffer {
        let llt = Self::packed_type(self.profile, site);
        if let Some(field) = self
            .frame
            .as_ref()
            .and_then(|frame| frame.packed.get(m))
            .cloned()
        {
            let slot = format!("%pack_field{}", field.ordinal);
            let ptr = format!("%packed{}", field.ordinal);
            self.frame_geps.push_str(&format!(
                "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n  {ptr} = load ptr, ptr {slot}\n",
                field.index
            ));
            PackedBuffer { ptr, llt }
        } else {
            let ptr = format!("%s{}", self.fresh());
            self.entry_alloc(&ptr, &llt, Some(64));
            PackedBuffer { ptr, llt }
        }
    }

    pub(super) fn allocate_frame_packs(&mut self) {
        let Some(frame) = &self.frame else {
            return;
        };
        let Some(plan) = &self.tile_plan else {
            return;
        };
        let packs = frame
            .packed
            .iter()
            .map(|(m, field)| (m, field.clone(), plan.sites[m].clone()))
            .collect::<Vec<_>>();
        for (_, field, site) in packs {
            let llt = Self::packed_type(self.profile, &site);
            let ptr = format!("%pack{}", field.ordinal);
            let slot = format!("%pack_field{}", field.ordinal);
            self.entry_alloc(&ptr, &llt, Some(64));
            self.frame_geps.push_str(&format!(
                "  {slot} = getelementptr %Frame, ptr %frame, i32 0, i32 {}\n  store ptr {ptr}, ptr {slot}\n",
                field.index
            ));
        }
    }

    /// Copy one row-invariant b operand to packed[j-tile][k][lane], padding
    /// the final panel's dead lanes with zero.
    pub(super) fn emit_pack_copy(
        &mut self,
        source: ObjectId,
        site: &TileSite,
        packed: &PackedBuffer,
    ) {
        debug_assert!(packing_site(site));
        let source_ty = self.obj_ty(source);
        let b_ty = source_ty
            .component_ty(site.b.slot)
            .cloned()
            .expect("tile b array");
        let b_llt = lower_ty(&b_ty).expect("tile b lowers");
        let elem_llt = lower_ty(&site.elem).expect("tile element lowers");
        let b_ptr = self
            .array_operand_ptr(source, Some(site.b.slot))
            .expect("tile b ptr");
        let tile_j = self.profile.tile_j(&site.elem);
        let tiles = site.c.div_ceil(tile_j);
        let panel_elems = site.k * tile_j;
        let jt_ctr = self.scratch("i64");
        let k_ctr = self.scratch("i64");
        let lane_ctr = self.scratch("i64");
        let (jt_head, jt_body, jt_done) = (self.label(), self.label(), self.label());
        let (k_head, k_body, k_done) = (self.label(), self.label(), self.label());
        let (lane_head, lane_body, lane_done) = (self.label(), self.label(), self.label());
        let (load, pad, store_done) = (self.label(), self.label(), self.label());

        // plan-s43-parallel-bpack §2: the j-tile axis is the pack's OWN split
        // axis. Panels are disjoint by construction (`panel_base = jt·k·tile_j`,
        // reads shared read-only), so any cut at a whole `jt` is sound. At
        // `split_range == false` this is character-identical to the `0`/`tiles`
        // literals it replaces, which is what keeps the sequential inline pack
        // (`bulk.rs`) byte-for-byte unchanged.
        let (jt_lo, jt_hi) = self.bulk_bounds(tiles);
        self.line(format!("store i64 {jt_lo}, ptr {jt_ctr}"));
        self.line(format!("br label %{jt_head}"));
        self.label_line(&jt_head);
        let jt = self.tmp();
        self.line(format!("{jt} = load i64, ptr {jt_ctr}"));
        let all_tiles = self.tmp();
        self.line(format!("{all_tiles} = icmp uge i64 {jt}, {jt_hi}"));
        self.line(format!(
            "br i1 {all_tiles}, label %{jt_done}, label %{jt_body}"
        ));
        self.label_line(&jt_body);
        let j0 = self.tmp();
        self.line(format!("{j0} = mul i64 {jt}, {tile_j}"));
        let panel_base = self.tmp();
        self.line(format!("{panel_base} = mul i64 {jt}, {panel_elems}"));
        self.line(format!("store i64 0, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));

        self.label_line(&k_head);
        let k = self.tmp();
        self.line(format!("{k} = load i64, ptr {k_ctr}"));
        let all_k = self.tmp();
        self.line(format!("{all_k} = icmp uge i64 {k}, {}", site.k));
        self.line(format!("br i1 {all_k}, label %{k_done}, label %{k_body}"));
        self.label_line(&k_body);
        let packed_k = self.tmp();
        self.line(format!("{packed_k} = mul i64 {k}, {tile_j}"));
        let packed_row = self.tmp();
        self.line(format!("{packed_row} = add i64 {panel_base}, {packed_k}"));
        self.line(format!("store i64 0, ptr {lane_ctr}"));
        self.line(format!("br label %{lane_head}"));

        self.label_line(&lane_head);
        let lane = self.tmp();
        self.line(format!("{lane} = load i64, ptr {lane_ctr}"));
        let all_lanes = self.tmp();
        self.line(format!("{all_lanes} = icmp uge i64 {lane}, {tile_j}"));
        self.line(format!(
            "br i1 {all_lanes}, label %{lane_done}, label %{lane_body}"
        ));
        self.label_line(&lane_body);
        let j = self.tmp();
        self.line(format!("{j} = add i64 {j0}, {lane}"));
        let packed_index = self.tmp();
        self.line(format!("{packed_index} = add i64 {packed_row}, {lane}"));
        let packed_ptr = self.tmp();
        self.line(format!(
            "{packed_ptr} = getelementptr {}, ptr {}, i64 0, i64 {packed_index}",
            packed.llt, packed.ptr
        ));
        let live = self.tmp();
        self.line(format!("{live} = icmp ult i64 {j}, {}", site.c));
        self.line(format!("br i1 {live}, label %{load}, label %{pad}"));

        self.label_line(&load);
        let b_index = self
            .emit_tile_index(
                (site.b.base != 0).then(|| site.b.base.to_string()),
                &[(site.b.ck, k.as_str()), (1, j.as_str())],
            )
            .expect("tile b has lane term");
        let b_elem_ptr = self.tmp();
        self.line(format!(
            "{b_elem_ptr} = getelementptr {b_llt}, ptr {b_ptr}, i64 0, i64 {b_index}"
        ));
        let value = self.tmp();
        self.line(format!("{value} = load {elem_llt}, ptr {b_elem_ptr}"));
        self.line(format!("store {elem_llt} {value}, ptr {packed_ptr}"));
        self.line(format!("br label %{store_done}"));

        self.label_line(&pad);
        self.line(format!(
            "store {elem_llt} zeroinitializer, ptr {packed_ptr}"
        ));
        self.line(format!("br label %{store_done}"));
        self.label_line(&store_done);
        let lane_next = self.tmp();
        self.line(format!("{lane_next} = add i64 {lane}, 1"));
        self.line(format!("store i64 {lane_next}, ptr {lane_ctr}"));
        self.line(format!("br label %{lane_head}"));

        self.label_line(&lane_done);
        let k_next = self.tmp();
        self.line(format!("{k_next} = add i64 {k}, 1"));
        self.line(format!("store i64 {k_next}, ptr {k_ctr}"));
        self.line(format!("br label %{k_head}"));
        self.label_line(&k_done);
        let jt_next = self.tmp();
        self.line(format!("{jt_next} = add i64 {jt}, 1"));
        self.line(format!("store i64 {jt_next}, ptr {jt_ctr}"));
        self.line(format!("br label %{jt_head}"));
        self.label_line(&jt_done);
    }

    /// The local slot type for a Pair-built staging product. Array components
    /// consumed only as addresses by collection/index/call ops are `ptr`
    /// fields (S20 #6/#8); value-observable components retain their ABI type.
    pub(super) fn lower_slot_ty(&self, o: ObjectId, ty: &Ty) -> Option<String> {
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
    pub(super) fn pointer_only_array_component(&self, agg: ObjectId, k: u32) -> bool {
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
    pub(super) fn field_ptr(
        &mut self,
        base: &str,
        agg_ty: &Ty,
        agg_llt: &str,
        k: u32,
    ) -> Option<String> {
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
    pub(super) fn emit_memcpy(&mut self, dst: &str, src: &str, llt: &str) {
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
    pub(super) fn field_store(
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
    pub(super) fn array_operand_ptr(&mut self, source: ObjectId, k: Option<u32>) -> Option<String> {
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
    pub(super) fn trap_if(&mut self, cond: &str, kind: u32) {
        let trap = self.label();
        let cont = self.label();
        self.line(format!("br i1 {cond}, label %{trap}, label %{cont}"));
        self.label_line(&trap);
        self.line(format!("call void @mapal_trap(i32 {kind})"));
        self.line("unreachable");
        self.label_line(&cont);
    }

    pub(super) fn task_site(&self, m: MorphismId) -> u32 {
        match self.guard_flavor {
            GuardFlavor::Host => unreachable!("host guard has no task site"),
            GuardFlavor::TaskBody(topo) => topo,
            GuardFlavor::Task => self
                .ir
                .topo_order(self.f)
                .iter()
                .position(|&candidate| candidate == m)
                .expect("task morphism is in entry topo") as u32,
        }
    }

    pub(super) fn record_trap(&mut self, m: MorphismId, kind: u32) {
        let topo = self.task_site(m);
        self.line(format!("call void @mapal_par_trap(i64 {topo}, i32 {kind})"));
        self.runtime_write = true;
    }

    pub(super) fn local_trap_site(&self, m: MorphismId) -> bool {
        let morph = self.ir.morphism(m).expect("morphism resolves");
        match morph.op {
            Operation::Div | Operation::Mod => {
                matches!(self.obj_ty(morph.target), Ty::Int { .. })
            }
            Operation::Index => !self.bp.proven(m),
            Operation::Update => true,
            _ => false,
        }
    }

    pub(super) fn emit_watermark(&mut self, m: MorphismId) {
        let topo = self.task_site(m);
        self.line(format!("call void @mapal_par_watermark(i64 {topo})"));
        self.runtime_write = true;
    }

    /// The accumulator's flat offset for one (subrow, lane): `acc_base + r*stride + lane`.
    ///
    /// One helper for what were four copies of the same four-armed decision —
    /// the inline `if r == 0` at the seed and store loops (`acc_base` absent),
    /// the four-arm match inside `emit_tile_lane_loop`, and the KC nest's own
    /// `emit_tile_kc_acc_lane` (`acc_base` present). They were character-identical
    /// per arm, which is why this extraction is a provable no-op on emitted text:
    /// each arm mints exactly the temporaries it minted before, in the same order.
    /// That matters more than usual here — `fresh()` is a SINGLE ordinal counter
    /// feeding `tmp()`, `label()` and `scratch()` alike, so one extra or reordered
    /// mint renames every subsequent value in the function and rewrites the entry
    /// block as well.
    ///
    /// `None` is the direct nest, whose accumulator starts at 0 and whose `r == 0`
    /// case needs no arithmetic at all; `Some(base)` is the KC nest, addressing
    /// into a panel-offset accumulator.
    pub(super) fn emit_acc_lane(
        &mut self,
        lane: &str,
        acc_base: Option<&str>,
        r: u64,
        stride: u64,
    ) -> String {
        match (acc_base, r) {
            (None, 0) => lane.to_owned(),
            (None, _) => {
                let offset = self.tmp();
                self.line(format!("{offset} = add i64 {lane}, {}", r * stride));
                offset
            }
            (Some(base), 0) => {
                let offset = self.tmp();
                self.line(format!("{offset} = add i64 {lane}, {base}"));
                offset
            }
            (Some(base), _) => {
                let based = self.tmp();
                self.line(format!("{based} = add i64 {base}, {}", r * stride));
                let offset = self.tmp();
                self.line(format!("{offset} = add i64 {lane}, {based}"));
                offset
            }
        }
    }

    /// This row's live column window, clipped to the task's flat range:
    /// `[max(0, lo - row0), min(C, hi - row0))`.
    ///
    /// A task owns a flat `[lo, hi)` over `rows*C` elements, so the first and
    /// last rows it touches are partial. Both comparisons are **signed** on
    /// purpose — `lo - row0` is negative for every row after the first, and an
    /// unsigned compare would read that as enormous and clip to `0` wrongly.
    ///
    /// Was written out three times, character-identical, in the boundary row,
    /// the row-split and the KC boundary row. Extracted rather than left
    /// duplicated because it always leads its caller and always mints the same
    /// six temporaries in the same order, so hoisting it cannot move the shared
    /// ordinal counter that names every subsequent value.
    pub(super) fn emit_row_window(
        &mut self,
        site: &TileSite,
        lo: &str,
        hi: &str,
        row0: &str,
    ) -> (String, String) {
        let jw_lo_raw = self.tmp();
        self.line(format!("{jw_lo_raw} = sub i64 {lo}, {row0}"));
        let jw_lo_negative = self.tmp();
        self.line(format!("{jw_lo_negative} = icmp slt i64 {jw_lo_raw}, 0"));
        let jw_lo = self.tmp();
        self.line(format!(
            "{jw_lo} = select i1 {jw_lo_negative}, i64 0, i64 {jw_lo_raw}"
        ));
        let jw_hi_raw = self.tmp();
        self.line(format!("{jw_hi_raw} = sub i64 {hi}, {row0}"));
        let jw_hi_past_c = self.tmp();
        self.line(format!(
            "{jw_hi_past_c} = icmp sgt i64 {jw_hi_raw}, {}",
            site.c
        ));
        let jw_hi = self.tmp();
        self.line(format!(
            "{jw_hi} = select i1 {jw_hi_past_c}, i64 {}, i64 {jw_hi_raw}",
            site.c
        ));
        (jw_lo, jw_hi)
    }

    pub(super) fn bulk_bounds(&self, n: u64) -> (String, String) {
        if self.split_range {
            ("%lo".into(), "%hi".into())
        } else {
            ("0".into(), n.to_string())
        }
    }

    // --- the walk ---------------------------------------------------------
}
