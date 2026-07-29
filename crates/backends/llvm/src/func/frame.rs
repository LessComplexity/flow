//! storage preparation, elided arrays, local slots, the `%Frame` layout
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// Configure by-ref inputs and Update slot aliases. Returns the lowered
    /// incoming argument type.
    pub(super) fn prepare_storage(&mut self) -> Option<String> {
        let fd = self.ir.func(self.f).expect("func resolves");
        let in_ty = self.obj_ty(fd.input);
        let (bk, btext) = match fd.kind {
            FuncKind::MapBody | FuncKind::FoldBody => {
                let k = self.body_captures();
                (k, lower_body_input_ty(&in_ty, k))
            }
            FuncKind::Named => (u32::MAX, lower_named_input_ty(&in_ty)),
        };
        if let Some(text) = btext {
            self.byref = Some((fd.input, bk, text));
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

        for &m in &fd.morphisms {
            if let Some(source) = self.update_in_place_source(m) {
                let target = self.ir.morphism(m).expect("morphism resolves").target;
                self.elided_updates.insert(target, ());
                self.update_aliases.insert(target, source);
            }
        }
        self.mark_elided_arrays(&fd.morphisms.clone());
        in_llt
    }

    /// Step 3b: find arrays no one will ever load, because every consumer
    /// rebuilds the element from its law (`elem_plan`).
    ///
    /// Deliberately narrow. An array qualifies only when **every** out-edge is a
    /// `Map`/`Fold` reading it directly as the mapped/folded array — the
    /// capture-free shape. A captured consumer reaches its array through a
    /// `Pair` product, and following that chain is more analysis than the win
    /// justifies today; those arrays keep their buffer. Conservative in the safe
    /// direction: a missed elision costs a store pass, a wrong one dereferences
    /// a field that does not exist.
    pub(super) fn mark_elided_arrays(&mut self, morphisms: &[MorphismId]) {
        for &m in morphisms {
            let Some(morph) = self.ir.morphism(m) else {
                continue;
            };
            if !matches!(
                morph.op,
                Operation::Iota | Operation::Fill | Operation::Zip | Operation::Enumerate
            ) {
                continue;
            }
            let arr = morph.target;
            // The law must be one a consumer can actually build. `Apply` is
            // excluded because THIS backend declines it (see `APPLY_INLINE`)
            // and a declined `Apply` degrades to loading exactly this array.
            match self.elem.src(arr) {
                Some(ElemSrc::Index | ElemSrc::Broadcast { .. } | ElemSrc::Pair(..)) => {}
                _ => continue,
            }
            if self.ir.object(arr).map(|o| o.kind) != Some(ObjectKind::Temporary) {
                continue;
            }
            let consumers = self.ir.out_edges(arr);
            if consumers.is_empty() {
                continue;
            }
            let all_inline = consumers.iter().all(|&c| {
                let Some(cm) = self.ir.morphism(c) else {
                    return false;
                };
                match cm.op {
                    Operation::Map { captures: 0, .. } => cm.source == arr,
                    Operation::Fold { .. } | Operation::Map { .. } => false,
                    _ => false,
                }
            });
            if all_inline {
                self.elided_arrays.insert(arr, ());
            }
        }
    }

    pub(super) fn owned_objects(&self) -> Vec<(ObjectId, ObjectKind, Ty)> {
        self.ir
            .objects()
            .filter(|(id, _)| self.ir.try_owner(*id) == Some(self.f))
            .map(|(id, obj)| (id, obj.kind, obj.ty.clone()))
            .collect()
    }

    pub(super) fn slot_type(
        &self,
        id: ObjectId,
        ty: &Ty,
        in_llt: &Option<String>,
    ) -> Option<String> {
        if self.ptr_resident.contains_key(id) {
            Some("ptr".into())
        } else if Some(id) == self.byref.as_ref().map(|(input, _, _)| *input) {
            in_llt.clone()
        } else {
            self.lower_slot_ty(id, ty)
        }
    }

    pub(super) fn allocate_local_slots(&mut self, in_llt: &Option<String>) {
        let mut ord = 0u32;
        for (id, kind, ty) in self.owned_objects() {
            if kind == ObjectKind::Constant {
                continue;
            }
            if self.elided_updates.contains_key(id) || self.elided_arrays.contains_key(id) {
                ord += 1;
                continue;
            }
            if let Some(llt) = self.slot_type(id, &ty, in_llt) {
                let name = format!("%o{ord}");
                self.slots.insert(id, name.clone());
                self.entry_alloc(&name, &llt, None);
            }
            ord += 1;
        }
    }

    pub(super) fn build_frame_layout(
        &self,
        in_llt: &Option<String>,
        path_plan: &PathPlan,
    ) -> FrameLayout {
        let mut fields = SecondaryMap::new();
        let mut order = Vec::new();
        let mut ord = 0u32;
        for (id, kind, ty) in self.owned_objects() {
            if kind == ObjectKind::Constant {
                continue;
            }
            // step 3b: an array nobody loads needs no storage. Dropping the
            // FIELD is the part DCE cannot do for us — `%Frame` is one object
            // shared across every task, so an unread member still costs its
            // bytes in the allocation.
            if self.elided_updates.contains_key(id) || self.elided_arrays.contains_key(id) {
                ord += 1;
                continue;
            }
            if let Some(llt) = self.slot_type(id, &ty, in_llt) {
                let index = order.len() as u32;
                fields.insert(
                    id,
                    FrameField {
                        owner: id,
                        index,
                        ordinal: ord,
                        llt,
                    },
                );
                order.push(id);
            }
            ord += 1;
        }
        for (target, _) in self.update_aliases.iter() {
            let mut source = self.update_aliases[target];
            while let Some(&next) = self.update_aliases.get(source) {
                source = next;
            }
            let field = fields
                .get(source)
                .expect("elided Update source has a frame field")
                .clone();
            fields.insert(target, field);
        }
        let mut packed = SecondaryMap::new();
        if self.packing
            && let Some(tile_plan) = &self.tile_plan
        {
            for task in &path_plan.tasks {
                if let TaskKind::Split { site: m, .. } = &task.kind
                    && let Some(site) = tile_plan.sites.get(*m)
                    && packing_site(site)
                {
                    packed.insert(
                        *m,
                        PackedField {
                            index: (order.len() + packed.len()) as u32,
                            ordinal: packed.len() as u32,
                        },
                    );
                }
            }
        }
        FrameLayout {
            fields,
            order,
            packed,
        }
    }

    pub(super) fn materialize_frame_slots(&mut self) {
        let order = self.frame.as_ref().expect("frame layout").order.clone();
        for o in order {
            self.slot(o).expect("frame field resolves");
        }
    }
}
