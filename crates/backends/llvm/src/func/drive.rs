//! the function drivers: `emit`, `emit_parallel`, `emit_task`, the `topo_order` walks
//!
//! Split out of the former single-file `func.rs` (S41); behaviour is byte-identical.

use super::*;

impl<'a> FnEmit<'a> {
    /// Emit the function body: prologue store, the topo walk, epilogue return.
    pub fn emit(mut self) -> String {
        let fd = self.ir.func(self.f).expect("func resolves");
        let ret_ty = self.obj_ty(fd.output);
        let fname = self.fnames[self.f].clone();

        // Only the entry function's frame is once-per-program (plan-s29
        // composition rule 4); the parallel entry goes through `emit_parallel`,
        // so reaching here as the entry IS the sequential flavor.
        self.heap_ok = self.f == self.ir.entry();

        let in_llt = self.prepare_storage();
        self.allocate_local_slots(&in_llt);

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
                if self.perf_timing {
                    self.line("call void @mapal_perf_end()");
                }
                self.heap_teardown();
                self.line(format!("ret {t} {v}"));
                t
            }
            None => {
                if self.perf_timing {
                    self.line("call void @mapal_perf_end()");
                }
                self.heap_teardown();
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
        let clean = self.attrs.clean(self.f) && !self.runtime_write && !self.perf_timing;
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
        let perf_begin = if self.perf_timing {
            "  call void @mapal_perf_begin()\n"
        } else {
            ""
        };
        format!(
            "define internal {sig_ret} @{fname}({param}){fn_attrs} {{\nentry:\n{}{}{}}}\n",
            perf_begin, self.allocas, self.body
        )
    }

    pub(crate) fn emit_parallel(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        plan: &PathPlan,
        perf_timing: bool,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        move_panel: Option<(u64, u64)>,
        profile: &'static TargetProfile,
    ) -> String {
        let mut host = FnEmit::new(
            ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, move_panel, profile,
        );
        host.perf_timing = perf_timing;
        // The host runs once per program, so its blocks may go to the arena.
        host.heap_ok = true;
        let fd = ir.func(f).expect("func resolves");
        let in_llt = host.prepare_storage();
        let frame = host.build_frame_layout(&in_llt, plan);
        host.frame = Some(frame.clone());
        // The parallel flavor packs every array into ONE `%Frame`, so THAT is
        // the block that blows the stack — heap-lower it as a unit. Tasks take
        // `ptr %frame` either way, and every field access is a `getelementptr
        // %Frame, ptr %frame, …`, so nothing below this line changes.
        let frame_bytes = llt_bytes(&frame.struct_llt());
        if host.heap_block(frame_bytes) {
            let align = llt_align(&frame.struct_llt());
            host.allocas.push_str(&format!(
                "  %frame = call ptr @mapal_rt_alloc(i64 {frame_bytes}, i64 {align})\n"
            ));
        } else {
            host.allocas.push_str("  %frame = alloca %Frame\n");
        }
        host.materialize_frame_slots();
        host.allocate_frame_packs();

        let topo = ir.topo_order(f);
        let mut assigned = SecondaryMap::new();
        let mut pinned = SecondaryMap::new();
        for (task_id, task) in plan.tasks.iter().enumerate() {
            let members: &[MorphismId] = match &task.kind {
                TaskKind::Split { site, .. } => std::slice::from_ref(site),
                TaskKind::Seq { morphisms } => morphisms,
            };
            for &m in members {
                assigned.insert(m, ());
            }
            if task.pinned {
                let first = *members.first().expect("task has a member");
                let first_topo = topo
                    .iter()
                    .position(|&candidate| candidate == first)
                    .expect("task member is in topo") as u32;
                pinned.insert(
                    first,
                    PinnedEmit {
                        task: task_id,
                        topo: first_topo,
                        len: task.deps.len(),
                    },
                );
            }
        }
        let mut checkpoints = SecondaryMap::new();
        for (ordinal, checkpoint) in plan
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.topo != u32::MAX)
            .enumerate()
        {
            let site = topo[checkpoint.topo as usize];
            let injection = checkpoint_injection(ir, site, &assigned, &topo);
            checkpoints.insert(
                injection,
                CheckpointEmit {
                    ordinal,
                    topo: checkpoint.topo,
                    len: checkpoint.wait.len(),
                },
            );
        }
        let mut pre_loop: SecondaryMap<MorphismId, Vec<CheckpointEmit>> = SecondaryMap::new();
        for scc in ir.loop_structure(f) {
            let mut objects: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            for &o in &scc.objects {
                objects.insert(o, ());
            }
            let mut members: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
            for &m in &topo {
                let morph = ir.morphism(m).expect("morphism resolves");
                if objects.contains_key(morph.source) || objects.contains_key(morph.target) {
                    members.insert(m, ());
                }
            }
            for &merge in &scc.merges {
                if let Some(plan) = ir.loop_plan(f, merge) {
                    for &m in plan
                        .decide_order
                        .iter()
                        .chain(&plan.advance_order)
                        .chain(&plan.exits)
                    {
                        members.insert(m, ());
                    }
                }
            }
            // A checkpoint site inside this loop ⟹ the loop is effectful (a
            // print is token-bearing), hence host-emitted; hoist its wait+check
            // to the loop's first LoopEnter.
            let first_enter = topo.iter().copied().find(|&m| {
                members.contains_key(m)
                    && matches!(
                        ir.morphism(m).expect("morphism resolves").op,
                        Operation::LoopEnter
                    )
            });
            let Some(first_enter) = first_enter else {
                continue;
            };
            for (ordinal, checkpoint) in plan
                .checkpoints
                .iter()
                .filter(|checkpoint| checkpoint.topo != u32::MAX)
                .enumerate()
            {
                if members.contains_key(topo[checkpoint.topo as usize]) {
                    let emit = CheckpointEmit {
                        ordinal,
                        topo: checkpoint.topo,
                        len: checkpoint.wait.len(),
                    };
                    if let Some(list) = pre_loop.get_mut(first_enter) {
                        list.push(emit);
                    } else {
                        pre_loop.insert(first_enter, vec![emit]);
                    }
                }
            }
        }
        host.host = Some(HostEmit {
            checkpoints,
            pinned,
            pre_loop,
        });

        if let Some(t) = &in_llt
            && let Some(slot) = host.slot(fd.input)
        {
            host.line(format!("store {t} %arg, ptr {slot}"));
        }
        host.line(format!(
            "%h = call ptr @mapal_par_begin(i32 {})",
            plan.tasks.len()
        ));
        for (task_id, task) in plan.tasks.iter().enumerate() {
            let (kind, n) = match &task.kind {
                // step 3b fallout: `path_plan` cut this task from the GRAPH,
                // where the producer is a real morphism making a real array.
                // The backend then decided nothing reads that array and emitted
                // no body. Registering it `Split` anyway would have the pool
                // slice a million-element range across every core to run a
                // function with zero instructions — free at MAPAL_PAR=1, one
                // dispatch per core plus a join at width. Register it as a
                // single unsplittable unit instead; the dep edges are untouched,
                // so dependents still wait on it and nothing renumbers.
                TaskKind::Split { site, .. }
                    if host
                        .ir
                        .morphism(*site)
                        .is_some_and(|m| host.elided_arrays.contains_key(m.target)) =>
                {
                    (0, 1)
                }
                TaskKind::Split { site, n }
                    if host
                        .tile_plan
                        .as_ref()
                        .and_then(|plan| plan.sites.get(*site))
                        .is_some_and(packing_site)
                        && host.packing =>
                {
                    (0, *n)
                }
                TaskKind::Split { n, .. } => (1, *n),
                TaskKind::Seq { morphisms } => (0, morphisms.len().max(1) as u64),
            };
            // plan-s32 step 2: the region's slice sizing, decided here because
            // both inputs are compile-time facts — the recorded reuse structure
            // and the profile's tile factors. The lane count is NOT an input;
            // the runtime multiplies `oversub` by however many lanes it has.
            let (min_slice, oversub) = match &task.kind {
                TaskKind::Split { site, .. } => host
                    .tile_plan
                    .as_ref()
                    .and_then(|plan| plan.sites.get(*site))
                    .map_or((0, 0), |site| host.slice_sizing(site)),
                TaskKind::Seq { .. } => (0, 0),
            };
            host.line(format!(
                "call void @mapal_par_task(ptr %h, i32 {task_id}, i32 {kind}, ptr @task{task_id}, i64 {n}, i32 {}, i64 {min_slice}, i32 {oversub}, i32 0)",
                task.rank
            ));
        }
        for (task_id, task) in plan.tasks.iter().enumerate() {
            if task.pinned {
                host.line(format!("call void @mapal_par_pin(ptr %h, i32 {task_id})"));
            }
        }
        for (after, task) in plan.tasks.iter().enumerate() {
            for &before in &task.deps {
                host.line(format!(
                    "call void @mapal_par_dep(ptr %h, i32 {before}, i32 {after})"
                ));
            }
        }
        host.line("call void @mapal_par_launch(ptr %h, ptr %frame)");
        host.walk_filtered(&assigned, false);
        host.line("call void @mapal_par_finish(ptr %h)");

        let ret_ty = host.obj_ty(fd.output);
        let sig_ret = match lower_ty(&ret_ty) {
            Some(t) => {
                let slot = host.slot(fd.output).expect("non-void return has a slot");
                let value = host.tmp();
                host.line(format!("{value} = load {t}, ptr {slot}"));
                if host.perf_timing {
                    host.line("call void @mapal_perf_end()");
                }
                host.heap_teardown();
                host.line(format!("ret {t} {value}"));
                t
            }
            None => {
                if host.perf_timing {
                    host.line("call void @mapal_perf_end()");
                }
                host.heap_teardown();
                host.line("ret void");
                "void".into()
            }
        };
        let param = in_llt
            .as_ref()
            .map(|t| format!("{t} %arg"))
            .unwrap_or_default();

        let mut out = frame.definition();
        out.push('\n');
        for (ordinal, checkpoint) in plan
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.topo != u32::MAX)
            .enumerate()
        {
            out.push_str(&wait_global(
                &format!("ckpt{ordinal}_entries"),
                &checkpoint.wait,
            ));
        }
        for (task_id, task) in plan.tasks.iter().enumerate() {
            if task.pinned {
                let wait = task
                    .deps
                    .iter()
                    .map(|&task| WaitEntry {
                        task,
                        threshold: None,
                    })
                    .collect::<Vec<_>>();
                out.push_str(&wait_global(&format!("pin{task_id}_entries"), &wait));
            }
        }
        if plan
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.topo != u32::MAX)
            || plan.tasks.iter().any(|task| task.pinned)
        {
            out.push('\n');
        }

        for (task_id, task) in plan.tasks.iter().enumerate() {
            out.push_str(&Self::emit_task(
                ir, f, fnames, strings, attrs, &frame, task_id, task, tiling, packing, contract,
                kc_nest, move_panel, profile,
            ));
            out.push('\n');
        }
        let perf_begin = if host.perf_timing {
            "  call void @mapal_perf_begin()\n"
        } else {
            ""
        };
        out.push_str(&format!(
            "define internal {sig_ret} @{}({param}) {{\nentry:\n{}{}{}{}{}\n",
            fnames[f], perf_begin, host.allocas, host.frame_geps, host.body, "}"
        ));
        out
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_task(
        ir: &'a CategoryIr,
        f: FuncId,
        fnames: &'a SecondaryMap<FuncId, String>,
        strings: &'a SecondaryMap<ObjectId, StrGlobal>,
        attrs: &'a FnAttrs,
        frame: &FrameLayout,
        task_id: usize,
        task: &mapal_ir::Task,
        tiling: bool,
        packing: bool,
        contract: bool,
        kc_nest: bool,
        move_panel: Option<(u64, u64)>,
        profile: &'static TargetProfile,
    ) -> String {
        if let TaskKind::Split { site: m, n } = &task.kind
            && let Some(site) = tiling
                .then(|| ir.tile_plan(f))
                .and_then(|plan| plan.sites.get(*m).cloned())
            && packing
            && packing_site(&site)
        {
            let mut slice = FnEmit::new(
                ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, move_panel,
                profile,
            );
            slice.guard_flavor = GuardFlavor::Task;
            slice.split_range = true;
            slice.prepare_storage();
            slice.frame = Some(frame.clone());
            let mut members = SecondaryMap::new();
            members.insert(*m, ());
            slice.walk_filtered(&members, true);
            slice.line("ret void");
            let slice_fn = format!(
                "define internal void @task{task_id}_slice(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
                slice.allocas, slice.frame_geps, slice.body
            );

            // plan-s43-parallel-bpack: the pack is its own range-parameterized
            // function over j-tiles, so the wrapper can dispatch it across the
            // pool instead of running it inline on one lane. S43 §4c measured
            // the inline version at 16.349 ms of a 54.164 ms threaded N=4096
            // wall — 30.2%, thread-count-independent, pure Amdahl.
            let source = ir.morphism(*m).expect("tile map resolves").source;
            let mut pack = FnEmit::new(
                ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, move_panel,
                profile,
            );
            pack.guard_flavor = GuardFlavor::Task;
            pack.split_range = true;
            pack.prepare_storage();
            pack.frame = Some(frame.clone());
            let packed = pack.packed_buffer(*m, &site);
            pack.emit_pack_copy(source, &site, &packed);
            pack.line("ret void");
            let pack_fn = format!(
                "define internal void @task{task_id}_pack(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
                pack.allocas, pack.frame_geps, pack.body
            );

            let mut emit = FnEmit::new(
                ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, move_panel,
                profile,
            );
            emit.guard_flavor = GuardFlavor::Task;
            emit.prepare_storage();
            emit.frame = Some(frame.clone());
            // RUN-ONCE IS PRESERVED AND IS NOT THE SAME PROPERTY AS RUN-SERIAL.
            // The outer registration is still `kind = 0` (drive.rs:266-275), so
            // this wrapper body executes exactly once — S27's run-once wrapper,
            // "help-first finish ⇒ width-1 sound". What changes is only the
            // WIDTH of the pack inside it.
            //
            // Two sequential nested runs, not one handle with a dep edge: a
            // dep-unlocked task is scheduled `Placement::Local(lane)`
            // (mapal-rt `complete_slice`), which would put every matmul slice on
            // one deque and skip the rank-sorted `Placement::Seed` the matmul
            // gets today. `mapal_par_finish` is `help_until(remaining == 0)`, so
            // the first run's return is the happens-before edge from every pack
            // store to every matmul load, and the second dispatch stays exactly
            // what it was. The extra handle costs one Box and a wake, once per
            // program run.
            let tile_j = profile.tile_j(&site.elem);
            let tiles = site.c.div_ceil(tile_j);
            let pack_handle = emit.tmp();
            emit.line(format!("{pack_handle} = call ptr @mapal_par_begin(i32 1)"));
            // The pack's quantum is ONE j-tile, not the matmul's `ti·t·c` — a
            // different axis in different units, so `slice_sizing` is not reused.
            // `slice_elems = 0` would be the silent-nothing bug: `slice_ranges`
            // falls to `min(T, ceil(n/4096))`, which at 256 tiles is ONE slice
            // and the pack stays serial with nothing failing.
            emit.line(format!(
                "call void @mapal_par_task(ptr {pack_handle}, i32 0, i32 1, ptr @task{task_id}_pack, i64 {tiles}, i32 {}, i64 1, i32 4, i32 0)",
                task.rank
            ));
            emit.line(format!(
                "call void @mapal_par_launch(ptr {pack_handle}, ptr %frame)"
            ));
            emit.line(format!("call void @mapal_par_finish(ptr {pack_handle})"));
            let handle = emit.tmp();
            emit.line(format!("{handle} = call ptr @mapal_par_begin(i32 1)"));
            // The packed flavor slices HERE, in the nested dispatch — the outer
            // task is the pack wrapper and is never split — so this is the call
            // the region's sizing belongs on.
            let (min_slice, oversub) = emit.slice_sizing(&site);
            emit.line(format!(
                "call void @mapal_par_task(ptr {handle}, i32 0, i32 1, ptr @task{task_id}_slice, i64 {n}, i32 {}, i64 {min_slice}, i32 {oversub}, i32 0)",
                task.rank
            ));
            emit.line(format!(
                "call void @mapal_par_launch(ptr {handle}, ptr %frame)"
            ));
            emit.line(format!("call void @mapal_par_finish(ptr {handle})"));
            emit.line("ret void");
            let wrapper_fn = format!(
                "define internal void @task{task_id}(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
                emit.allocas, emit.frame_geps, emit.body
            );
            return format!("{slice_fn}\n{pack_fn}\n{wrapper_fn}");
        }

        let mut emit = FnEmit::new(
            ir, f, fnames, strings, attrs, tiling, packing, contract, kc_nest, move_panel, profile,
        );
        emit.guard_flavor = GuardFlavor::Task;
        emit.split_range = matches!(task.kind, TaskKind::Split { .. });
        emit.watermark = match &task.kind {
            TaskKind::Split { .. } => false,
            TaskKind::Seq { morphisms } => !morphisms.iter().any(|&m| {
                matches!(
                    ir.morphism(m).expect("morphism resolves").op,
                    Operation::Fold { .. } | Operation::LoopEnter
                )
            }),
        };
        emit.prepare_storage();
        emit.frame = Some(frame.clone());

        let mut members = SecondaryMap::new();
        match &task.kind {
            TaskKind::Split { site, .. } => {
                members.insert(*site, ());
            }
            TaskKind::Seq { morphisms } => {
                for &m in morphisms {
                    members.insert(m, ());
                }
            }
        }
        emit.walk_filtered(&members, true);
        emit.line("ret void");
        format!(
            "define internal void @task{task_id}(i64 %lo, i64 %hi, ptr %frame) {{\nentry:\n{}{}{}}}\n",
            emit.allocas, emit.frame_geps, emit.body
        )
    }

    /// The capture count of the Map/Fold site whose body is `self.f` (unique —
    /// lower mints one fresh body fn per lambda site, ADR-0027). `0` when no
    /// site references the fn (a dead body), which is the identity lowering.
    pub(super) fn body_captures(&self) -> u32 {
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

    pub(super) fn walk(&mut self) {
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
                Operation::LoopEnter => {
                    if self.gated.contains_key(m) {
                        // plan-s40: arm-owned loop — its Phi drives it.
                        continue;
                    }
                    crate::loops::emit_loop(self, morph.target)
                }
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue; // driver-owned
                    }
                    if self.gated.contains_key(m) {
                        continue; // plan-s39: fired only from its Phi's branch
                    }
                    self.emit_morphism(m);
                }
            }
        }
    }

    pub(super) fn walk_filtered(
        &mut self,
        members: &SecondaryMap<MorphismId, ()>,
        include_members: bool,
    ) {
        let mut in_scc: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for scc in self.ir.loop_structure(self.f) {
            for &merge in &scc.merges {
                if let Some(plan) = self.ir.loop_plan(self.f, merge) {
                    for &m in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                        owned.insert(m, ());
                    }
                }
            }
            for o in scc.objects {
                in_scc.insert(o, ());
            }
        }

        for m in self.ir.topo_order(self.f) {
            if let Some(pin) = self
                .host
                .as_ref()
                .and_then(|host| host.pinned.get(m))
                .cloned()
            {
                self.line(format!(
                    "call void @mapal_par_wait(ptr %h, ptr @pin{}_entries, i32 {})",
                    pin.task, pin.len
                ));
                self.line(format!(
                    "call void @mapal_par_check(ptr %h, i64 {})",
                    pin.topo
                ));
                self.line(format!(
                    "call void @mapal_par_run_pinned(ptr %h, i32 {})",
                    pin.task
                ));
            }

            if members.contains_key(m) != include_members {
                continue;
            }
            let morph = self.ir.morphism(m).expect("morphism resolves");
            match morph.op {
                Operation::LoopEnter => {
                    if self.gated.contains_key(m) {
                        // plan-s40: arm-owned loop — its Phi drives it, and
                        // path_plan folded it into the Phi's Seq task, so no
                        // pre-loop checkpoint belongs here either.
                        continue;
                    }
                    if let Some(list) = self
                        .host
                        .as_ref()
                        .and_then(|host| host.pre_loop.get(m))
                        .cloned()
                    {
                        for c in &list {
                            self.line(format!(
                                "call void @mapal_par_wait(ptr %h, ptr @ckpt{}_entries, i32 {})",
                                c.ordinal, c.len
                            ));
                            self.line(format!(
                                "call void @mapal_par_check(ptr %h, i64 {})",
                                c.topo
                            ));
                        }
                    }
                    crate::loops::emit_loop(self, morph.target)
                }
                Operation::LoopBack | Operation::LoopExit => {}
                _ => {
                    if owned.contains_key(m)
                        || in_scc.contains_key(morph.source)
                        || in_scc.contains_key(morph.target)
                    {
                        continue;
                    }
                    if self.gated.contains_key(m) {
                        continue; // plan-s39: fired only from its Phi's branch
                    }
                    self.emit_morphism(m);
                }
            }
        }
    }

    pub(super) fn emit_checkpoint(&mut self, m: MorphismId) {
        let Some(checkpoint) = self
            .host
            .as_ref()
            .and_then(|host| host.checkpoints.get(m))
            .cloned()
        else {
            return;
        };
        self.line(format!(
            "call void @mapal_par_wait(ptr %h, ptr @ckpt{}_entries, i32 {})",
            checkpoint.ordinal, checkpoint.len
        ));
        self.line(format!(
            "call void @mapal_par_check(ptr %h, i64 {})",
            checkpoint.topo
        ));
    }
}
