//! The shared, plan-driven replayer (DESIGN §1.1). One graph constructor for
//! every pass: it walks each function's `topo_order`, classifies every object by
//! its *recipe*, and rebuilds an equivalent object through the public
//! [`IrBuilder`]. Well-formedness of the output is by construction; `validate()`
//! is a redundant independent check (R2).
//!
//! Identity replay — `replay(ir, &RewritePlan::new())` — is the WP1 soundness
//! anchor: it must reproduce an interp-equal, validate-clean graph on the 10
//! in-Core examples.
//!
//! **Recipe classification** (DESIGN §1.1 table): composite builder primitives
//! (`binop`, `phi`, `index`, `zip`, `print`, loop routes) mint their internal
//! `Pair` products atomically, so those products are *not* re-materialized — the
//! primitive call rebuilds them from the slot feeders. A product with any
//! explicit consumer (or ≥2 consumers) *is* materialized via `pack`.

use slotmap::SecondaryMap;

use flow_ir::{
    CategoryIr, Dest, FuncId, FuncKind, IrBuilder, MorphismId, ObjectId, ObjectKind, Operation, Ty,
    Value,
};

use crate::plan::{FusionSpec, LiftKind, LiftSpec, RewritePlan};

/// Why replay declined to rebuild (DESIGN §1.1 / R6). The only user-relevant
/// classification failure; the driver turns it into the whole-graph identity
/// path (`skipped_non_canonical`). Builder rejections during a well-formed
/// rebuild are internal bugs and panic instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// A function contains a loop shape outside the canonical quartet
    /// (multi-merge SCC, multiple backs, or multiple exits per merge).
    NonCanonicalLoop,
}

/// Where a replayed body's Return value lands (DESIGN §1.1 + the `inline`
/// pass). Normal fn replay writes the replayed fn's own Return ([`RetDest::Own`]);
/// an inlined call site redirects the callee's Return writers to the call
/// target's fresh object.
enum RetDest {
    /// Return writers target the enclosing fn's own Return object.
    Own,
    /// An inlined call whose target is a `Temporary`: the callee's Return
    /// value defines one fresh object under this name, handed back to the
    /// caller for the `remap` insertion.
    Fresh(Option<String>),
}

/// Rebuild `ir` under `plan` (DESIGN §1.1). Returns a fresh sealed graph, or
/// [`ReplayError::NonCanonicalLoop`] if any function's loop shape is outside the
/// interp-M1 canonical quartet (R6 — the driver takes the whole-graph identity
/// path).
pub fn replay(ir: &CategoryIr, plan: &RewritePlan) -> Result<CategoryIr, ReplayError> {
    if !is_canonical(ir) {
        return Err(ReplayError::NonCanonicalLoop);
    }
    debug_assert!(
        plan.is_consistent(ir),
        "replay: plan violates §1.2 P1/P2 consistency"
    );

    let mut b = IrBuilder::new();

    // Function-level DCE (§1.1/§3.1): only functions reachable from the entry
    // over the *post-plan* reference graph survive. A fused pair drops the two
    // original bodies (inlined into a synthesized `h`) when nothing else refers
    // to them; uncalled Named fns fall out too (RW7). Sound — the oracle never
    // evaluates unreferenced functions.
    let live = live_functions(ir, plan);

    // Declare every live function first (declare-before-reference: Call/Map/Fold
    // may target a fn defined later). Preserve kind/name/tys/loc.
    let mut fmap: SecondaryMap<FuncId, FuncId> = SecondaryMap::new();
    for (fid, def) in ir.funcs() {
        if !live.contains_key(fid) {
            continue;
        }
        let in_ty = ir.object(def.input).expect("fn input").ty.clone();
        let out_ty = ir.object(def.output).expect("fn output").ty.clone();
        let nf = b
            .declare(def.kind, &def.name, in_ty, out_ty, def.loc)
            .expect("replay: declare rejected a well-formed function");
        fmap.insert(fid, nf);
    }

    // Synthesize one fused `MapBody` per live fusion (DESIGN §4), built while no
    // `FnBuilder` is active. Keyed by the g-edge so replay swaps `Map{g}` for
    // `Map{h}` on the original array.
    let mut fused: SecondaryMap<MorphismId, FuncId> = SecondaryMap::new();
    let mut fused_n = 0u32;
    for (m_g, spec) in plan.fuse.iter() {
        let container = ir.owner(ir.morphism(m_g).expect("fused g-edge").source);
        if !live.contains_key(container) {
            continue;
        }
        let h = synthesize_fused(&mut b, ir, &fmap, m_g, spec, &mut fused_n);
        fused.insert(m_g, h);
    }

    // Synthesize one collection body per lifted loop before any enclosing
    // function is built; the replacement Map/Fold then references it exactly
    // like a source-authored body.
    let mut lifted: SecondaryMap<ObjectId, FuncId> = SecondaryMap::new();
    let mut lifted_n = 0u32;
    for (merge, spec) in plan.lift.iter() {
        let container = ir.owner(merge);
        if !live.contains_key(container) {
            continue;
        }
        let body = synthesize_lifted_body(&mut b, ir, &fmap, merge, spec, &mut lifted_n);
        lifted.insert(merge, body);
    }

    // A single global old→new object map: object ids are unique graph-wide and
    // every edge stays within one function (I6), so one map is unambiguous.
    let mut remap: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();

    for (fid, _) in ir.funcs() {
        if !live.contains_key(fid) {
            continue;
        }
        replay_fn(&mut b, ir, plan, fid, &fmap, &fused, &lifted, &mut remap);
    }

    let entry = fmap[ir.entry()];
    Ok(b.seal(entry)
        .expect("replay: seal rejected a builder-built graph"))
}

/// Rebuild one function's body (DESIGN §1.1). Phase A reconstructs body objects
/// in topo order; Phase B wires the Return writers.
#[allow(clippy::too_many_arguments)]
fn replay_fn(
    b: &mut IrBuilder,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fid: FuncId,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
) {
    let def = ir.func(fid).expect("fn def");
    let ret_obj = def.output;
    let obj_order = object_topo_order(ir, fid);

    let mut fb = b
        .build_fn(fmap[fid])
        .expect("replay: build_fn rejected a declared function");

    // The input Parameter maps to the builder's own input object.
    remap.insert(def.input, fb.input());

    let mut done: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

    // Phase A — body objects (constants, materialized products, op temporaries,
    // loop quartets). The Return object and its direct-writer primitives are
    // deferred to Phase B.
    for &o in &obj_order {
        if done.get(o).copied().unwrap_or(false) {
            continue;
        }
        reconstruct_a(
            &mut fb,
            ir,
            plan,
            fmap,
            fused,
            lifted,
            remap,
            &mut done,
            &obj_order,
            o,
            ret_obj,
            &RetDest::Own,
        );
    }

    // Phase B — Return writers (DESIGN §1.1: `Dest::Ret` canonical form).
    for &m in ir.in_edges(ret_obj) {
        reconstruct_return_writer(&mut fb, ir, plan, fmap, fused, lifted, remap, m);
    }

    fb.finish()
        .expect("replay: finish rejected a rebuilt function");
}

/// Phase A dispatch for one object.
#[allow(clippy::too_many_arguments)]
fn reconstruct_a(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    done: &mut SecondaryMap<ObjectId, bool>,
    obj_order: &[ObjectId],
    o: ObjectId,
    ret_obj: ObjectId,
    ret_dest: &RetDest,
) {
    if o == ret_obj {
        done.insert(o, true);
        return;
    }
    let obj = ir.object(o).expect("object");
    match obj.kind {
        ObjectKind::Parameter => {
            // Pre-mapped to fb.input().
            done.insert(o, true);
        }
        ObjectKind::Constant => {
            let v = obj.value.clone().expect("constant value");
            let new = fb.constant(v, obj.loc).expect("replay: constant");
            remap.insert(o, new);
            done.insert(o, true);
        }
        ObjectKind::LoopMerge => {
            reconstruct_loop(
                fb, ir, plan, fmap, fused, lifted, remap, done, obj_order, o, ret_obj, ret_dest,
            );
        }
        ObjectKind::Return => {
            // Only reachable for a foreign Return; the owning one is ret_obj.
            done.insert(o, true);
        }
        ObjectKind::Temporary => {
            if plan.drop.contains_key(o) || plan.alias.contains_key(o) {
                done.insert(o, true);
                return;
            }
            if let Some(v) = plan.constify.get(o) {
                let new = fb.constant(v.clone(), obj.loc).expect("replay: constify");
                remap.insert(o, new);
                done.insert(o, true);
                return;
            }
            if is_internal_pack(ir, o) {
                // Rebuilt internally by its consuming primitive; not materialized.
                done.insert(o, true);
                return;
            }
            reconstruct_temp(fb, ir, plan, fmap, fused, lifted, remap, o);
            done.insert(o, true);
        }
    }
}

/// Reconstruct one loop as a unit (DESIGN §1.1 quartet). Canonical shapes only —
/// [`is_canonical`] has already gated `replay`, so this loop's SCC has exactly
/// one merge, one back, one exit.
#[allow(clippy::too_many_arguments)]
fn reconstruct_loop(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    done: &mut SecondaryMap<ObjectId, bool>,
    obj_order: &[ObjectId],
    merge: ObjectId,
    ret_obj: ObjectId,
    ret_dest: &RetDest,
) {
    if let Some(spec) = plan.lift.get(merge) {
        reconstruct_lifted_loop(
            fb,
            ir,
            plan,
            lifted[merge],
            spec,
            remap,
            done,
            merge,
            ret_obj,
            ret_dest,
        );
        return;
    }

    let merge_obj = ir.object(merge).expect("merge");

    // One source of truth for the loop's attribution (DESIGN §3, BL7 —
    // flow-ir's loop_plan). `replay` is gated by `is_canonical`, so `Some` here.
    let lp = ir
        .loop_plan(ir.owner(merge), merge)
        .expect("canonical loop (is_canonical gated)");

    // The SCC set: its objects are body objects (plus the merge and back route).
    let scc: SecondaryMap<ObjectId, bool> = {
        let mut m = SecondaryMap::new();
        for &s in &lp.scc_objects {
            m.insert(s, true);
        }
        m
    };

    // init → begin_loop. The init is loop-invariant (topo defers LoopEnter), so
    // it is already mapped.
    let init = lp.init;
    let lh = fb
        .begin_loop(feed(plan, remap, init), merge_obj.loc)
        .expect("replay: begin_loop");
    remap.insert(merge, fb.merge_of(&lh));
    done.insert(merge, true);

    // Body objects, in the global topo suborder restricted to this SCC.
    for &o in obj_order {
        if o == merge || done.get(o).copied().unwrap_or(false) {
            continue;
        }
        if !scc.contains_key(o) {
            continue;
        }
        if plan.drop.contains_key(o) || plan.alias.contains_key(o) {
            done.insert(o, true);
            continue;
        }
        if is_internal_pack(ir, o) {
            // The back/exit route objects live here — rebuilt by loop_back /
            // loop_exit from their slot feeders.
            done.insert(o, true);
            continue;
        }
        reconstruct_temp(fb, ir, plan, fmap, fused, lifted, remap, o);
        done.insert(o, true);
    }

    // Back edge: the LoopBack route packs (next_state, cond).
    let back_route = lp.back_route;
    let bf = slot_feeders(ir, back_route);
    let back_loc = back_morph_loc(ir, merge);
    fb.loop_back(
        &lh,
        feed(plan, remap, bf[0]),
        feed(plan, remap, bf[1]),
        back_loc,
    )
    .expect("replay: loop_back");

    // Exit edge: the single attributed LoopExit, leaving the SCC.
    let exit_route = lp.exit_route;
    let exit_morph = ir.morphism(lp.exits[0]).expect("loop exit");
    let (exit_tgt, exit_loc) = (exit_morph.target, exit_morph.loc);
    let ef = slot_feeders(ir, exit_route);
    let tgt_obj = ir.object(exit_tgt).expect("exit target");
    let dest = if exit_tgt == ret_obj {
        match ret_dest {
            RetDest::Own => Dest::Ret { slot: None },
            // Inlined: the exit mints the call target's object instead of
            // writing the callee's (nonexistent) Return.
            RetDest::Fresh(name) => Dest::Fresh(name.clone()),
        }
    } else {
        Dest::Fresh(tgt_obj.name.clone())
    };
    let exit_new = fb
        .loop_exit(
            &lh,
            feed(plan, remap, ef[0]),
            feed(plan, remap, ef[1]),
            dest,
            exit_loc,
        )
        .expect("replay: loop_exit");
    if exit_tgt == ret_obj {
        // Inlined: record the exit object as the callee's Return value —
        // `inline_return` picks it up.
        if matches!(ret_dest, RetDest::Fresh(_)) {
            remap.insert(ret_obj, exit_new);
        }
    } else {
        remap.insert(exit_tgt, exit_new);
        done.insert(exit_tgt, true);
    }

    fb.end_loop(lh).expect("replay: end_loop");
}

/// Replace one planned loop SCC with `const K → Iota(K) → Map/Fold`, wiring the
/// collection result to the old `LoopExit` target.
#[allow(clippy::too_many_arguments)]
fn reconstruct_lifted_loop(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    body: FuncId,
    spec: &LiftSpec,
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    done: &mut SecondaryMap<ObjectId, bool>,
    merge: ObjectId,
    ret_obj: ObjectId,
    ret_dest: &RetDest,
) {
    let lp = ir
        .loop_plan(ir.owner(merge), merge)
        .expect("lift plan keys a canonical loop");
    let exit = ir.morphism(lp.exits[0]).expect("lifted loop exit");
    let target = exit.target;
    let target_obj = ir.object(target).expect("lifted loop exit target");
    let dest = if target == ret_obj {
        match ret_dest {
            RetDest::Own => Dest::Ret { slot: None },
            RetDest::Fresh(name) => Dest::Fresh(name.clone()),
        }
    } else {
        Dest::Fresh(target_obj.name.clone())
    };

    let count = fb
        .constant(Value::I32(spec.count), exit.loc)
        .expect("lift: count constant");
    let items = fb
        .iota(count, Dest::Fresh(None), exit.loc)
        .expect("lift: iota");
    let captures: Vec<_> = spec
        .captures
        .iter()
        .map(|&o| feed(plan, remap, o))
        .collect();
    let result = match spec.kind {
        LiftKind::Fold { seed, .. } => fb
            .fold_captured(
                body,
                &captures,
                feed(plan, remap, seed),
                items,
                dest,
                exit.loc,
            )
            .expect("lift: fold_captured"),
        LiftKind::Map => fb
            .map_captured(body, &captures, items, dest, exit.loc)
            .expect("lift: map_captured"),
    };

    for &o in &lp.scc_objects {
        done.insert(o, true);
    }
    done.insert(lp.back_route, true);
    done.insert(lp.exit_route, true);
    done.insert(merge, true);
    if target == ret_obj {
        if matches!(ret_dest, RetDest::Fresh(_)) {
            remap.insert(ret_obj, result);
        }
    } else {
        remap.insert(target, result);
        done.insert(target, true);
    }
}

/// Reconstruct a materialized-product or op-defined `Temporary` as a fresh
/// object (DESIGN §1.1). Names and locs preserved; `Dest::Fresh` throughout —
/// Return writes are handled separately (Phase B).
fn reconstruct_temp(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    o: ObjectId,
) {
    let obj = ir.object(o).expect("temp");
    let name = obj.name.clone();
    let dest = Dest::Fresh(name);
    let ins = ir.in_edges(o);

    // A product object: all in-edges are `Pair`. Rebuild via the matching packer
    // from its slot feeders (declaration order = slot order).
    if !ins.is_empty()
        && ins
            .iter()
            .all(|&m| matches!(ir.morphism(m).unwrap().op, Operation::Pair { .. }))
    {
        let comps: Vec<ObjectId> = slot_feeders(ir, o)
            .into_iter()
            .map(|s| feed(plan, remap, s))
            .collect();
        let new = match &obj.ty {
            flow_ir::Ty::Tuple(_) => fb.pack(&comps, dest, obj.loc),
            flow_ir::Ty::Struct { .. } => fb.pack_struct(obj.ty.clone(), &comps, dest, obj.loc),
            flow_ir::Ty::Array { .. } => fb.pack_array(&comps, dest, obj.loc),
            _ => unreachable!("product object with non-product ty"),
        }
        .expect("replay: pack");
        remap.insert(o, new);
        return;
    }

    // A fused `Map{g}` edge (DESIGN §4): emit `Map{h}` on the ORIGINAL array.
    if let Some(&h) = fused.get(ins[0]) {
        let new = emit_fused_map(fb, ir, plan, remap, h, ins[0], dest, obj.loc);
        remap.insert(o, new);
        return;
    }

    // An inlined `Call` edge (region-emission plan Move 1): substitute the
    // callee's body; its Return value defines this object.
    if plan.inline.contains_key(ins[0]) {
        let new = inline_call(
            fb,
            ir,
            plan,
            fmap,
            fused,
            lifted,
            remap,
            ins[0],
            RetDest::Fresh(obj.name.clone()),
        );
        remap.insert(o, new);
        return;
    }

    // A single op edge defines this object.
    let m = ir.morphism(ins[0]).expect("op edge");
    let new = emit_op(
        fb, ir, plan, fmap, remap, m.op, m.source, m.target, dest, m.loc,
    );
    remap.insert(o, new);
}

/// Emit one op primitive with feeders resolved through the internal pack (for
/// packed-source ops) or directly (DESIGN §1.1). Returns the result object.
#[allow(clippy::too_many_arguments)]
fn emit_op(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    remap: &SecondaryMap<ObjectId, ObjectId>,
    op: Operation,
    src: ObjectId,
    target: ObjectId,
    dest: Dest,
    loc: flow_ir::SourceLoc,
) -> ObjectId {
    use Operation::*;
    match op {
        Add | Sub | Mul | Div | Mod | Eq | Neq | Lt | Gt | Le | Ge | And | Or => {
            let f = slot_feeders(ir, src);
            fb.binop(
                op,
                feed(plan, remap, f[0]),
                feed(plan, remap, f[1]),
                dest,
                loc,
            )
            .expect("replay: binop")
        }
        Neg | Not => fb
            .unop(op, feed(plan, remap, src), dest, loc)
            .expect("replay: unop"),
        Widen => {
            let out_ty = ir.object(target).expect("replay: Widen target").ty.clone();
            fb.widen(feed(plan, remap, src), out_ty, dest, loc)
                .expect("replay: widen")
        }
        Proj { index } => fb
            .proj(feed(plan, remap, src), index, dest, loc)
            .expect("replay: proj"),
        Index => {
            let f = slot_feeders(ir, src);
            fb.index(feed(plan, remap, f[0]), feed(plan, remap, f[1]), dest, loc)
                .expect("replay: index")
        }
        Phi => {
            let f = slot_feeders(ir, src);
            fb.phi(
                feed(plan, remap, f[0]),
                feed(plan, remap, f[1]),
                feed(plan, remap, f[2]),
                dest,
                loc,
            )
            .expect("replay: phi")
        }
        Zip => {
            let f = slot_feeders(ir, src);
            fb.zip(feed(plan, remap, f[0]), feed(plan, remap, f[1]), dest, loc)
                .expect("replay: zip")
        }
        Enumerate => fb
            .enumerate(feed(plan, remap, src), dest, loc)
            .expect("replay: enumerate"),
        Iota => fb
            .iota(feed(plan, remap, src), dest, loc)
            .expect("replay: iota"),
        Fill => fb
            // The EXISTING (already-replayed) internal tuple, never the sugar
            // `fill(x, n)` — the sugar mints a fresh tuple, which resurrects
            // every CSE'd duplicate and breaks the fixpoint (S21).
            .fill_from(feed(plan, remap, src), dest, loc)
            .expect("replay: fill"),
        Update => {
            let f = slot_feeders(ir, src);
            fb.update(
                feed(plan, remap, f[0]),
                feed(plan, remap, f[1]),
                feed(plan, remap, f[2]),
                dest,
                loc,
            )
            .expect("replay: update")
        }
        Call(g) => fb
            .call(fmap[g], feed(plan, remap, src), dest, loc)
            .expect("replay: call"),
        Map { body, captures: 0 } => fb
            .map(fmap[body], feed(plan, remap, src), dest, loc)
            .expect("replay: map"),
        Map { body, captures } => {
            // ADR-0027: the source is the product (c₁…cₖ, arr) — re-thread the
            // leading capture components, never silently rebuild as k=0.
            let f = slot_feeders(ir, src);
            let (caps, data) = split_capture_source(&f, captures as usize, 1, "Map");
            let caps: Vec<ObjectId> = caps.iter().map(|&c| feed(plan, remap, c)).collect();
            fb.map_captured(fmap[body], &caps, feed(plan, remap, data[0]), dest, loc)
                .expect("replay: map_captured")
        }
        Fold { body, captures: 0 } => fb
            .fold(fmap[body], feed(plan, remap, src), dest, loc)
            .expect("replay: fold"),
        Fold { body, captures } => {
            // ADR-0027: the source is (c₁…cₖ, acc, arr) — captures first, then
            // the historical (acc, arr) tail.
            let f = slot_feeders(ir, src);
            let (caps, data) = split_capture_source(&f, captures as usize, 2, "Fold");
            let caps: Vec<ObjectId> = caps.iter().map(|&c| feed(plan, remap, c)).collect();
            fb.fold_captured(
                fmap[body],
                &caps,
                feed(plan, remap, data[0]),
                feed(plan, remap, data[1]),
                dest,
                loc,
            )
            .expect("replay: fold_captured")
        }
        Print { newline } => {
            // `print`/`println` take (token, value) and always mint a fresh
            // IoToken result (no Dest); `dest` is unused here (Print never
            // targets Return directly).
            let f = slot_feeders(ir, src);
            let (tok, val) = (feed(plan, remap, f[0]), feed(plan, remap, f[1]));
            if newline {
                fb.println(tok, val, loc).expect("replay: println")
            } else {
                fb.print(tok, val, loc).expect("replay: print")
            }
        }
        TimeMs => {
            // `time` (plan-time-builtin) takes the bare token (no packed
            // source) and mints the fresh `(IoToken, f64)` pair; `dest` is
            // unused, exactly as for Print. No rule rewrites it — it is
            // effectful, so replay only has to rebuild it faithfully.
            fb.time_ms(feed(plan, remap, src), loc)
                .expect("replay: time_ms")
        }
        Pair { .. } | Output | LoopEnter | LoopBack | LoopExit => {
            unreachable!("emit_op: {op:?} is not an object-defining primitive")
        }
    }
}

/// Phase B — wire one Return in-edge (DESIGN §1.1). `Output`/`Pair` are existing
/// objects fed via `output()`; a primitive edge targets Return directly and is
/// rebuilt with `Dest::Ret{None}`. `LoopExit` was already emitted by the loop
/// quartet (skipped here).
#[allow(clippy::too_many_arguments)]
fn reconstruct_return_writer(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &SecondaryMap<ObjectId, ObjectId>,
    m: MorphismId,
) {
    let morph = ir.morphism(m).expect("ret in-edge");
    // A fused `Map{g}` writing straight to Return (DESIGN §4).
    if let Some(&h) = fused.get(m) {
        emit_fused_map(
            fb,
            ir,
            plan,
            remap,
            h,
            m,
            Dest::Ret { slot: None },
            morph.loc,
        );
        return;
    }
    match morph.op {
        Operation::Output => {
            fb.output(feed(plan, remap, morph.source), None, morph.loc)
                .expect("replay: output full");
        }
        Operation::Pair { slot, .. } => {
            fb.output(feed(plan, remap, morph.source), Some(slot), morph.loc)
                .expect("replay: output slot");
        }
        // Emitted by the loop quartet with Dest::Ret{None}; not a Phase-B writer.
        Operation::LoopExit => {}
        // A value-producing primitive targeting Return directly (Dest::Ret{None}).
        op => {
            // An inlined `Call` writing Return directly (region-emission plan
            // Move 1): the callee's Return writers replay as this fn's own.
            if plan.inline.contains_key(m) {
                inline_call(fb, ir, plan, fmap, fused, lifted, remap, m, RetDest::Own);
                return;
            }
            emit_op(
                fb,
                ir,
                plan,
                fmap,
                remap,
                op,
                morph.source,
                morph.target,
                Dest::Ret { slot: None },
                morph.loc,
            );
        }
    }
}

// --- call inlining (region-emission plan Move 1) ----------------------------

/// Inline one planned `Call` edge: replay the callee's body verbatim into the
/// caller's builder with the callee's Parameter mapped to the call's (already
/// replayed) source object and the callee's Return writers redirected per
/// `ret_dest`. The callee's objects get fresh ids in builder emission order
/// (L2: same graph → same inlined graph); the callee's own planned Calls
/// recurse through [`reconstruct_temp`]. Returns the object holding the
/// callee's Return value (meaningful only for [`RetDest::Fresh`]).
#[allow(clippy::too_many_arguments)]
fn inline_call(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    fused: &SecondaryMap<MorphismId, FuncId>,
    lifted: &SecondaryMap<ObjectId, FuncId>,
    remap: &SecondaryMap<ObjectId, ObjectId>,
    m: MorphismId,
    ret_dest: RetDest,
) -> ObjectId {
    let morph = ir.morphism(m).expect("inline call edge");
    let Operation::Call(g) = morph.op else {
        unreachable!("inline: plan channel keys a non-Call edge")
    };
    let def = ir.func(g).expect("inline callee");
    let ret_obj = def.output;
    let obj_order = object_topo_order(ir, g);

    // A callee-local id map (the caller's map is read only for the call
    // source): object ids are unique graph-wide and each inlined copy is fully
    // wired before returning, so one local map per site is unambiguous — a
    // diamond-shared callee simply gets one fresh copy per site (the
    // documented duplication policy).
    let mut local: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();
    let arg = feed(plan, remap, morph.source);
    local.insert(def.input, arg);
    let mut done: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

    // Phase A — body objects, exactly as `replay_fn` (loop quartets included).
    for &o in &obj_order {
        if done.get(o).copied().unwrap_or(false) {
            continue;
        }
        reconstruct_a(
            fb, ir, plan, fmap, fused, lifted, &mut local, &mut done, &obj_order, o, ret_obj,
            &ret_dest,
        );
    }

    // Phase B — the callee's Return writers, redirected.
    inline_return(fb, ir, plan, fmap, &local, m, ret_obj, ret_dest, arg)
}

/// Phase B of an inlined call: wire the callee's Return writers per `ret_dest`
/// ([`RetDest::Own`]: they replay as the enclosing fn's own Return writes;
/// [`RetDest::Fresh`]: the Return value is captured in one fresh object).
/// Returns the object holding the Return value (`Fresh`), or `arg` as a dummy
/// (`Own` — the writes already landed on the enclosing fn's Return).
#[allow(clippy::too_many_arguments)]
fn inline_return(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    fmap: &SecondaryMap<FuncId, FuncId>,
    local: &SecondaryMap<ObjectId, ObjectId>,
    call: MorphismId,
    ret_obj: ObjectId,
    ret_dest: RetDest,
    arg: ObjectId,
) -> ObjectId {
    let writers = ir.in_edges(ret_obj);
    let call_loc = ir.morphism(call).expect("call").loc;

    // A slot-wise return (a product written by `Pair` slot writes).
    if !writers.is_empty()
        && writers
            .iter()
            .all(|&w| matches!(ir.morphism(w).unwrap().op, Operation::Pair { .. }))
    {
        let comps: Vec<ObjectId> = slot_feeders(ir, ret_obj)
            .into_iter()
            .map(|s| feed(plan, local, s))
            .collect();
        return match ret_dest {
            RetDest::Fresh(name) => {
                let ty = &ir.object(ret_obj).expect("ret").ty;
                match ty {
                    flow_ir::Ty::Tuple(_) => fb.pack(&comps, Dest::Fresh(name), call_loc),
                    flow_ir::Ty::Struct { .. } => {
                        fb.pack_struct(ty.clone(), &comps, Dest::Fresh(name), call_loc)
                    }
                    flow_ir::Ty::Array { .. } => fb.pack_array(&comps, Dest::Fresh(name), call_loc),
                    _ => unreachable!("inline: slot-written Return with non-product ty"),
                }
                .expect("inline: pack")
            }
            RetDest::Own => {
                for &w in writers {
                    let mo = ir.morphism(w).expect("ret writer");
                    let Operation::Pair { slot, .. } = mo.op else {
                        unreachable!("inline: mixed Return writers")
                    };
                    fb.output(feed(plan, local, mo.source), Some(slot), mo.loc)
                        .expect("inline: output slot");
                }
                arg
            }
        };
    }

    // The canonical single full-value writer (Output / primitive / LoopExit).
    assert!(
        writers.len() == 1,
        "inline: callee Return has a non-canonical writer set"
    );
    let mo = ir.morphism(writers[0]).expect("ret writer");
    match mo.op {
        // Emitted by the loop quartet with the redirect dest; the Fresh case
        // recorded the exit object as the Return value (`reconstruct_loop`).
        Operation::LoopExit => match ret_dest {
            RetDest::Fresh(_) => local[ret_obj],
            RetDest::Own => arg,
        },
        Operation::Output => match ret_dest {
            RetDest::Fresh(_) => feed(plan, local, mo.source),
            RetDest::Own => {
                fb.output(feed(plan, local, mo.source), None, mo.loc)
                    .expect("inline: output");
                arg
            }
        },
        op => {
            let dest = match ret_dest {
                RetDest::Fresh(name) => Dest::Fresh(name),
                RetDest::Own => Dest::Ret { slot: None },
            };
            emit_op(
                fb, ir, plan, fmap, local, op, mo.source, mo.target, dest, mo.loc,
            )
        }
    }
}

// --- recipe classification helpers ----------------------------------------

/// Whether `op` reads its source as an **internally-packed** tuple (the builder
/// mints that product atomically). These sources are never materialized.
///
/// ADR-0027: a `Map`/`Fold` with `captures > 0` qualifies — `map_captured` /
/// `fold_captured` mint the `(c₁…cₖ, …)` source product atomically. The k=0
/// forms keep the historical shapes (Map's bare array; Fold's materialized
/// `(acc, arr)` product). Keep in sync with `graph_rewrites`' copy — CSE
/// representatives must never be packs replay treats as internal.
fn reads_packed_source(op: Operation) -> bool {
    use Operation::*;
    match op {
        Add
        | Sub
        | Mul
        | Div
        | Mod
        | Eq
        | Neq
        | Lt
        | Gt
        | Le
        | Ge
        | And
        | Or
        | Phi
        | Index
        | Zip
        | Update
        | Print { .. }
        | LoopBack
        | LoopExit => true,
        Map { captures, .. } | Fold { captures, .. } => captures > 0,
        _ => false,
    }
}

/// Whether `o` is an internal pack (a product consumed **only** as one
/// packed-source primitive's source) — not materialized (DESIGN §1.1). A product
/// with an explicit consumer or ≥2 consumers falls through to `false` and is
/// materialized.
fn is_internal_pack(ir: &CategoryIr, o: ObjectId) -> bool {
    if ir.object(o).map(|x| x.kind) != Some(ObjectKind::Temporary) {
        return false;
    }
    let outs = ir.out_edges(o);
    if outs.len() != 1 {
        return false;
    }
    let m = ir.morphism(outs[0]).unwrap();
    m.source == o && reads_packed_source(m.op)
}

/// The slot feeders of a product object, in slot order (declaration = slot
/// order, load-bearing for non-commutative ops; DESIGN §1.1).
fn slot_feeders(ir: &CategoryIr, product: ObjectId) -> Vec<ObjectId> {
    let mut v: Vec<(u32, ObjectId)> = ir
        .in_edges(product)
        .iter()
        .filter_map(|&m| {
            let mo = ir.morphism(m).unwrap();
            if let Operation::Pair { slot, .. } = mo.op {
                Some((slot, mo.source))
            } else {
                None
            }
        })
        .collect();
    v.sort_by_key(|(s, _)| *s);
    v.into_iter().map(|(_, s)| s).collect()
}

/// Split a captured `Map`/`Fold` source product's slot feeders into the
/// leading `k` capture components and the trailing data inputs (`data` = 1
/// for `Map`: the array; `data` = 2 for `Fold`: acc + array). The builder
/// pins the source arity at construction (R2), so a mismatch here means a
/// malformed graph reached replay — an internal bug, named loudly. Never a
/// `debug_assert`: over-arity would silently truncate in release, and a
/// non-Pair-fed source (a short feeder list, since [`slot_feeders`] filters
/// to `Pair` edges) would panic as an unmarked index-OOB.
fn split_capture_source<'a, T>(
    feeders: &'a [T],
    k: usize,
    data: usize,
    op: &str,
) -> (&'a [T], &'a [T]) {
    assert!(
        feeders.len() == k + data,
        "replay: {op}{{k={k}}} source product arity expected {}, found {}",
        k + data,
        feeders.len()
    );
    feeders.split_at(k)
}

/// Feeder lookup: resolve `alias` transitively, then map to the new id (DESIGN
/// §1.1 id remap). Present by the time it is read (alias points earlier in topo,
/// feeders precede consumers).
fn feed(plan: &RewritePlan, remap: &SecondaryMap<ObjectId, ObjectId>, o: ObjectId) -> ObjectId {
    let r = plan.resolve_alias(o);
    remap
        .get(r)
        .copied()
        .unwrap_or_else(|| panic!("replay: feeder {r:?} (from {o:?}) is not mapped"))
}

// --- loop structure helpers -----------------------------------------------

/// The loc of `merge`'s `LoopBack` morphism.
fn back_morph_loc(ir: &CategoryIr, merge: ObjectId) -> flow_ir::SourceLoc {
    ir.in_edges(merge)
        .iter()
        .find_map(|&m| {
            let mo = ir.morphism(m).unwrap();
            (mo.op == Operation::LoopBack).then_some(mo.loc)
        })
        .expect("loop back morphism")
}

// --- canonicity gate (R6) -------------------------------------------------

/// Whether every loop in `ir` is the canonical quartet: one merge per SCC, one
/// back and one exit per merge (DESIGN §5, R6). A non-canonical shape makes
/// `replay` return [`ReplayError::NonCanonicalLoop`].
///
/// Delegates the per-merge attribution to [`flow_ir::CategoryIr::loop_plan`]
/// (DESIGN §3, BL7 — one source of truth): a merge is canonical iff its SCC has
/// exactly one merge and `loop_plan` yields `Some` (which itself encodes the
/// one-back / one-attributed-exit conditions).
pub fn is_canonical(ir: &CategoryIr) -> bool {
    ir.funcs().all(|(f, _)| {
        ir.loop_structure(f)
            .iter()
            .all(|lscc| lscc.merges.len() == 1 && ir.loop_plan(f, lscc.merges[0]).is_some())
    })
}

// --- object topo order ----------------------------------------------------

/// A topological object order for function `f` — feeders before consumers, loop
/// back-edges non-gating, `LoopMerge` complete on its `LoopEnter` (DESIGN §1.1).
/// Derived from `ir.topo_order`, which already defers `LoopEnter` so every
/// loop-invariant feeder precedes the header.
fn object_topo_order(ir: &CategoryIr, f: FuncId) -> Vec<ObjectId> {
    let objs: Vec<ObjectId> = ir
        .objects()
        .filter(|(id, _)| ir.try_owner(*id) == Some(f))
        .map(|(id, _)| id)
        .collect();

    // Gating in-edge count per object (mirrors the builder's completion rule).
    let mut remaining: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
    for &o in &objs {
        let kind = ir.object(o).unwrap().kind;
        let count = match kind {
            ObjectKind::Parameter | ObjectKind::Constant => 0,
            ObjectKind::LoopMerge => ir
                .in_edges(o)
                .iter()
                .filter(|&&m| ir.morphism(m).unwrap().op == Operation::LoopEnter)
                .count() as u32,
            _ => ir.in_edges(o).len() as u32,
        };
        remaining.insert(o, count);
    }

    let mut order: Vec<ObjectId> = Vec::new();
    for &o in &objs {
        if remaining[o] == 0 {
            order.push(o);
        }
    }

    // Drive completion off the morphism topo order; LoopBack never gates.
    for m in ir.topo_order(f) {
        let morph = ir.morphism(m).unwrap();
        if morph.op == Operation::LoopBack {
            continue;
        }
        let tgt = morph.target;
        if ir.try_owner(tgt) != Some(f) {
            continue;
        }
        let r = remaining[tgt];
        if r > 0 {
            let nr = r - 1;
            remaining.insert(tgt, nr);
            if nr == 0 {
                order.push(tgt);
            }
        }
    }

    order
}

// --- function-level DCE (DESIGN §1.1 / §3.1) ------------------------------

/// The functions reachable from the entry over the *post-plan* reference graph.
/// A fused `Map{g}` contributes the two bodies' own refs (they are inlined into a
/// synthesized `h`) rather than `f`/`g`; a dropped or aliased Call/Map/Fold
/// contributes nothing. Uncalled fns fall out (RW7).
fn live_functions(ir: &CategoryIr, plan: &RewritePlan) -> SecondaryMap<FuncId, ()> {
    let mut live: SecondaryMap<FuncId, ()> = SecondaryMap::new();
    let mut stack = vec![ir.entry()];
    while let Some(f) = stack.pop() {
        if live.contains_key(f) {
            continue;
        }
        live.insert(f, ());
        push_refs(ir, plan, f, &mut stack);
    }
    live
}

/// Push the functions a live function `f` references under the plan.
fn push_refs(ir: &CategoryIr, plan: &RewritePlan, f: FuncId, stack: &mut Vec<FuncId>) {
    let def = ir.func(f).expect("live fn");
    for &m in &def.morphisms {
        let morph = ir.morphism(m).expect("morph");
        let (target, body) = match morph.op {
            Operation::Call(g) => (morph.target, g),
            Operation::Map { body, .. } | Operation::Fold { body, .. } => (morph.target, body),
            _ => continue,
        };
        // A dropped/aliased defining edge is not replayed ⇒ no reference.
        if plan.drop.contains_key(target) || plan.alias.contains_key(target) {
            continue;
        }
        if let Some(spec) = plan.fuse.get(m) {
            // Fused: `h` inlines both bodies verbatim, referencing their own
            // Call/Map/Fold targets via fmap. `f`/`g` themselves are not kept.
            push_raw_refs(ir, spec.f, stack);
            push_raw_refs(ir, spec.g, stack);
        } else if plan.inline.contains_key(m) {
            // Inlined (Move 1): the callee is not referenced — but the inline
            // copy's own Call/Map/Fold targets are, so walk the callee's refs
            // plan-aware. Inline edges form a DAG (the pass's cycle guard), so
            // the descent terminates.
            push_refs(ir, plan, body, stack);
        } else {
            stack.push(body);
        }
    }
}

/// Push a body's raw Call/Map/Fold targets — used when the body is inlined into a
/// fused `h` (the inline copy references the originals through fmap).
fn push_raw_refs(ir: &CategoryIr, fid: FuncId, stack: &mut Vec<FuncId>) {
    let def = ir.func(fid).expect("body fn");
    for &m in &def.morphisms {
        match ir.morphism(m).expect("morph").op {
            Operation::Call(g)
            | Operation::Map { body: g, .. }
            | Operation::Fold { body: g, .. } => stack.push(g),
            _ => {}
        }
    }
}

// --- loop lifting synthesis ------------------------------------------------

/// Build the captured Map/Fold body requested by one [`LiftSpec`]. This is the
/// fused-body machinery pattern applied to a loop cone: map special leaves to
/// body parameters, replay the selected original objects through the same
/// primitive emitter, then write the cone root to the synthesized Return.
fn synthesize_lifted_body(
    b: &mut IrBuilder,
    ir: &CategoryIr,
    fmap: &SecondaryMap<FuncId, FuncId>,
    merge: ObjectId,
    spec: &LiftSpec,
    n: &mut u32,
) -> FuncId {
    let loc = ir.object(merge).expect("lift merge").loc;
    let mut inputs: Vec<Ty> = spec
        .captures
        .iter()
        .map(|&o| ir.object(o).expect("lift capture").ty.clone())
        .collect();
    let (kind, prefix) = match spec.kind {
        LiftKind::Fold { accumulator, .. } => {
            inputs.push(ir.object(accumulator).expect("lift accumulator").ty.clone());
            (FuncKind::FoldBody, "lift_fold")
        }
        LiftKind::Map => (FuncKind::MapBody, "lift_map"),
    };
    inputs.push(Ty::i32());
    let input_ty = if kind == FuncKind::MapBody && inputs.len() == 1 {
        Ty::i32()
    } else {
        Ty::Tuple(inputs)
    };
    let output_ty = ir
        .object(spec.body_result)
        .expect("lift body result")
        .ty
        .clone();
    let name = format!("{prefix}${n}");
    *n += 1;
    let body = b
        .declare(kind, &name, input_ty, output_ty, loc)
        .expect("lift: declare body");

    let mut fb = b.build_fn(body).expect("lift: build body");
    let input = fb.input();
    let mut local: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();
    if kind == FuncKind::MapBody && spec.captures.is_empty() {
        local.insert(spec.counter, input);
    } else {
        let mut slot = 0u32;
        for &capture in &spec.captures {
            let p = fb
                .proj(input, slot, Dest::Fresh(None), loc)
                .expect("lift: capture projection");
            local.insert(capture, p);
            slot += 1;
        }
        if let LiftKind::Fold { accumulator, .. } = spec.kind {
            let p = fb
                .proj(input, slot, Dest::Fresh(None), loc)
                .expect("lift: accumulator projection");
            local.insert(accumulator, p);
            slot += 1;
        }
        let item = fb
            .proj(input, slot, Dest::Fresh(None), loc)
            .expect("lift: item projection");
        local.insert(spec.counter, item);
    }

    let mut members: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    for &o in &spec.body_objects {
        members.insert(o, ());
        let obj = ir.object(o).expect("lift body object");
        if obj.kind == ObjectKind::Constant {
            let c = fb
                .constant(obj.value.clone().expect("constant"), obj.loc)
                .expect("lift: body constant");
            local.insert(o, c);
        }
    }

    let empty = RewritePlan::new();
    let no_fuse: SecondaryMap<MorphismId, FuncId> = SecondaryMap::new();
    let no_lift: SecondaryMap<ObjectId, FuncId> = SecondaryMap::new();
    for o in object_topo_order(ir, ir.owner(merge)) {
        if o == spec.body_result || !members.contains_key(o) || local.contains_key(o) {
            continue;
        }
        let obj = ir.object(o).expect("lift body object");
        match obj.kind {
            ObjectKind::Temporary if is_internal_pack(ir, o) => {}
            ObjectKind::Temporary => {
                reconstruct_temp(&mut fb, ir, &empty, fmap, &no_fuse, &no_lift, &mut local, o)
            }
            _ => unreachable!("lift analysis admitted an unmapped body leaf"),
        }
    }
    emit_lifted_return(&mut fb, ir, fmap, spec.body_result, &local);
    fb.finish().expect("lift: finish body");
    body
}

/// Emit the lifted cone root directly to the synthesized Return when it has a
/// defining primitive. This preserves the lower-canonical body shape consumed
/// by `tile_plan` (final Add/Fold targets Return, rather than `tmp → Output`).
fn emit_lifted_return(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    fmap: &SecondaryMap<FuncId, FuncId>,
    root: ObjectId,
    local: &SecondaryMap<ObjectId, ObjectId>,
) {
    if let Some(&value) = local.get(root) {
        fb.output(value, None, ir.object(root).expect("lift root").loc)
            .expect("lift: body output");
        return;
    }
    let obj = ir.object(root).expect("lift root");
    let ins = ir.in_edges(root);
    if !ins.is_empty()
        && ins
            .iter()
            .all(|&m| matches!(ir.morphism(m).unwrap().op, Operation::Pair { .. }))
    {
        let comps: Vec<_> = slot_feeders(ir, root)
            .into_iter()
            .map(|o| local[o])
            .collect();
        match &obj.ty {
            Ty::Tuple(_) => fb.pack(&comps, Dest::Ret { slot: None }, obj.loc),
            Ty::Struct { .. } => {
                fb.pack_struct(obj.ty.clone(), &comps, Dest::Ret { slot: None }, obj.loc)
            }
            Ty::Array { .. } => fb.pack_array(&comps, Dest::Ret { slot: None }, obj.loc),
            _ => unreachable!("lift: Pair-defined non-product root"),
        }
        .expect("lift: body return pack");
        return;
    }
    let [m] = ins else {
        unreachable!("lift: unmapped body root without one definition")
    };
    let morph = ir.morphism(*m).expect("lift root definer");
    emit_op(
        fb,
        ir,
        &RewritePlan::new(),
        fmap,
        local,
        morph.op,
        morph.source,
        morph.target,
        Dest::Ret { slot: None },
        morph.loc,
    );
}

// --- map fusion synthesis (DESIGN §4) -------------------------------------

/// How a fused body writes its Return: `Fresh` captures the value in a new object
/// (`f`, whose result feeds `g`); `Return` writes it into `h`'s own Return (`g`).
enum Redirect {
    Fresh,
    Return,
}

/// Synthesize one fused `MapBody` `h = g ∘ f` (DESIGN §4): declare it, inline
/// `f`'s body (param ↦ `h.input`, Return captured as `r₁`), then `g`'s body
/// (param ↦ `r₁`, Return ↦ `h`'s Return). Returns the new body's `FuncId`.
/// Preconditions (single full-value Return writer, loop-free) are the analysis's
/// contract; a violation here is an internal bug and panics.
///
/// ADR-0027: with `captures = k > 0` both bodies take `(c₁…cₖ, x)` — `h`'s
/// input carries the shared captures, so `g`'s parameter is re-packed as
/// `(π₀ h.in … πₖ₋₁ h.in, r₁)` rather than bare `r₁`.
fn synthesize_fused(
    b: &mut IrBuilder,
    ir: &CategoryIr,
    fmap: &SecondaryMap<FuncId, FuncId>,
    m_g: MorphismId,
    spec: &FusionSpec,
    n: &mut u32,
) -> FuncId {
    let loc = ir.morphism(m_g).expect("g-edge").loc;
    let f_def = ir.func(spec.f).expect("f body");
    let g_def = ir.func(spec.g).expect("g body");
    let in_ty = ir.object(f_def.input).expect("f in").ty.clone();
    let out_ty = ir.object(g_def.output).expect("g out").ty.clone();
    let name = format!("fused${n}");
    *n += 1;
    let h = b
        .declare(FuncKind::MapBody, &name, in_ty, out_ty, loc)
        .expect("replay: declare fused body");

    let mut hb = b.build_fn(h).expect("replay: build fused body");
    let h_in = hb.input();
    let r1 = inline_body(&mut hb, ir, fmap, spec.f, h_in, Redirect::Fresh);
    let g_param = if spec.captures == 0 {
        r1
    } else {
        let mut comps: Vec<ObjectId> = Vec::with_capacity(spec.captures as usize + 1);
        for i in 0..spec.captures {
            let p = hb
                .proj(h_in, i, Dest::Fresh(None), loc)
                .expect("replay: fused capture proj");
            comps.push(p);
        }
        comps.push(r1);
        hb.pack(&comps, Dest::Fresh(None), loc)
            .expect("replay: fused g-param pack")
    };
    inline_body(&mut hb, ir, fmap, spec.g, g_param, Redirect::Return);
    hb.finish().expect("replay: finish fused body");
    h
}

/// Inline one loop-free, single-full-value-Return body into the fused builder
/// (DESIGN §4). `param_new` is the object its Parameter maps to; `redirect`
/// decides how its Return value lands. Returns the object holding the Return
/// value (meaningful only for `Redirect::Fresh`). The copy is verbatim — an empty
/// plan, no nested fusion (a later fixpoint round reaches nested opportunities).
fn inline_body(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    fmap: &SecondaryMap<FuncId, FuncId>,
    body: FuncId,
    param_new: ObjectId,
    redirect: Redirect,
) -> ObjectId {
    let plan = RewritePlan::new();
    let no_fuse: SecondaryMap<MorphismId, FuncId> = SecondaryMap::new();
    let no_lift: SecondaryMap<ObjectId, FuncId> = SecondaryMap::new();

    let def = ir.func(body).expect("inline body");
    let ret_obj = def.output;
    let obj_order = object_topo_order(ir, body);

    let mut local: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();
    local.insert(def.input, param_new);
    let mut done: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

    for &o in &obj_order {
        if done.get(o).copied().unwrap_or(false) {
            continue;
        }
        reconstruct_a(
            fb,
            ir,
            &plan,
            fmap,
            &no_fuse,
            &no_lift,
            &mut local,
            &mut done,
            &obj_order,
            o,
            ret_obj,
            &RetDest::Own,
        );
    }

    // The single full-value Return writer (precondition): either an `Output` from
    // an existing body object, or a value-producing primitive into Return.
    let m = ir.in_edges(ret_obj)[0];
    let morph = ir.morphism(m).expect("ret writer");
    match redirect {
        Redirect::Fresh => match morph.op {
            Operation::Output => feed(&plan, &local, morph.source),
            op => emit_op(
                fb,
                ir,
                &plan,
                fmap,
                &local,
                op,
                morph.source,
                morph.target,
                Dest::Fresh(None),
                morph.loc,
            ),
        },
        Redirect::Return => {
            match morph.op {
                Operation::Output => {
                    fb.output(feed(&plan, &local, morph.source), None, morph.loc)
                        .expect("replay: fused output");
                }
                op => {
                    emit_op(
                        fb,
                        ir,
                        &plan,
                        fmap,
                        &local,
                        op,
                        morph.source,
                        morph.target,
                        Dest::Ret { slot: None },
                        morph.loc,
                    );
                }
            }
            param_new
        }
    }
}

/// Emit the fused `Map{h}` on the ORIGINAL array (DESIGN §4). `m_g` is the old
/// `Map{g}` edge; its mapped array `mid` was produced by a `Map{f}` — `f`'s
/// mapped array (already replayed) is the fused map's array input.
///
/// ADR-0027: with `captures = k > 0` the analysis has guaranteed both maps read
/// the identical capture objects, so the fused map re-threads `f`'s leading
/// source components with [`flow_ir::FnBuilder::map_captured`]; the fused body's
/// capture count is `k`.
#[allow(clippy::too_many_arguments)]
fn emit_fused_map(
    fb: &mut flow_ir::FnBuilder<'_>,
    ir: &CategoryIr,
    plan: &RewritePlan,
    remap: &SecondaryMap<ObjectId, ObjectId>,
    h: FuncId,
    m_g: MorphismId,
    dest: Dest,
    loc: flow_ir::SourceLoc,
) -> ObjectId {
    let g_morph = ir.morphism(m_g).expect("g-edge");
    let Operation::Map { captures, .. } = g_morph.op else {
        unreachable!("emit_fused_map: fused key is not a Map edge")
    };
    let k = captures as usize;
    // g's mapped array (= f's result): the bare source for k=0, else the source
    // product's last slot.
    let mid = if k == 0 {
        g_morph.source
    } else {
        let feeders = slot_feeders(ir, g_morph.source);
        let (_, data) = split_capture_source(&feeders, k, 1, "Map");
        data[0]
    };
    let m_f = ir.in_edges(mid)[0];
    let f_src = ir.morphism(m_f).expect("f-edge").source;
    if k == 0 {
        fb.map(h, feed(plan, remap, f_src), dest, loc)
            .expect("replay: fused map")
    } else {
        let f = slot_feeders(ir, f_src);
        let (caps, data) = split_capture_source(&f, k, 1, "Map");
        let caps: Vec<ObjectId> = caps.iter().map(|&c| feed(plan, remap, c)).collect();
        fb.map_captured(h, &caps, feed(plan, remap, data[0]), dest, loc)
            .expect("replay: fused map_captured")
    }
}

#[cfg(test)]
mod tests {
    use super::split_capture_source;

    #[test]
    fn split_capture_source_partitions_caps_and_data() {
        let (caps, data) = split_capture_source(&[10, 20, 30], 2, 1, "Map");
        assert_eq!(caps, &[10, 20]);
        assert_eq!(data, &[30]);
        let (caps, data) = split_capture_source(&[10, 20, 30, 40], 2, 2, "Fold");
        assert_eq!(caps, &[10, 20]);
        assert_eq!(data, &[30, 40]);
    }

    #[test]
    #[should_panic(expected = "replay: Map{k=2} source product arity expected 3, found 4")]
    fn split_capture_source_over_arity_is_loud() {
        // Over-arity must NOT silently truncate in release (ADR-0027 review
        // major #8 — the old `debug_assert` let the extra slot drop).
        let f = [10, 20, 30, 40];
        let _ = split_capture_source(&f, 2, 1, "Map");
    }

    #[test]
    #[should_panic(expected = "replay: Fold{k=1} source product arity expected 3, found 2")]
    fn split_capture_source_under_arity_is_loud() {
        // A non-Pair-fed source yields a SHORT feeder list (`slot_feeders`
        // filters to Pair edges) — the same check names that invariant too.
        let f = [10, 20];
        let _ = split_capture_source(&f, 1, 2, "Fold");
    }
}
