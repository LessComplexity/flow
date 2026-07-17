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
    CategoryIr, Dest, FuncId, FuncKind, IrBuilder, MorphismId, ObjectId, ObjectKind, Operation,
};

use crate::plan::{FusionSpec, RewritePlan};

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

    // A single global old→new object map: object ids are unique graph-wide and
    // every edge stays within one function (I6), so one map is unambiguous.
    let mut remap: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();

    for (fid, _) in ir.funcs() {
        if !live.contains_key(fid) {
            continue;
        }
        replay_fn(&mut b, ir, plan, fid, &fmap, &fused, &mut remap);
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
            &mut fb, ir, plan, fmap, fused, remap, &mut done, &obj_order, o, ret_obj,
        );
    }

    // Phase B — Return writers (DESIGN §1.1: `Dest::Ret` canonical form).
    for &m in ir.in_edges(ret_obj) {
        reconstruct_return_writer(&mut fb, ir, plan, fmap, fused, remap, m);
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
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    done: &mut SecondaryMap<ObjectId, bool>,
    obj_order: &[ObjectId],
    o: ObjectId,
    ret_obj: ObjectId,
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
                fb, ir, plan, fmap, fused, remap, done, obj_order, o, ret_obj,
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
            reconstruct_temp(fb, ir, plan, fmap, fused, remap, o);
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
    remap: &mut SecondaryMap<ObjectId, ObjectId>,
    done: &mut SecondaryMap<ObjectId, bool>,
    obj_order: &[ObjectId],
    merge: ObjectId,
    ret_obj: ObjectId,
) {
    let merge_obj = ir.object(merge).expect("merge");

    // The SCC set: its objects are body objects (plus the merge and back route).
    let scc: SecondaryMap<ObjectId, bool> = {
        let mut m = SecondaryMap::new();
        for s in scc_containing(ir, ir.owner(merge), merge) {
            m.insert(s, true);
        }
        m
    };

    // init → begin_loop. The init is loop-invariant (topo defers LoopEnter), so
    // it is already mapped.
    let init = enter_source(ir, merge).expect("loop enter source");
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
        reconstruct_temp(fb, ir, plan, fmap, fused, remap, o);
        done.insert(o, true);
    }

    // Back edge: the LoopBack route packs (next_state, cond).
    let back_route = back_source(ir, merge).expect("loop back route");
    let bf = slot_feeders(ir, back_route);
    let back_loc = back_morph_loc(ir, merge);
    fb.loop_back(
        &lh,
        feed(plan, remap, bf[0]),
        feed(plan, remap, bf[1]),
        back_loc,
    )
    .expect("replay: loop_back");

    // Exit edge: the single LoopExit reachable from the merge, leaving the SCC.
    let (exit_route, exit_tgt, exit_loc) = exit_of(ir, merge, &scc).expect("loop exit");
    let ef = slot_feeders(ir, exit_route);
    let tgt_obj = ir.object(exit_tgt).expect("exit target");
    let dest = if exit_tgt == ret_obj {
        Dest::Ret { slot: None }
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
    if exit_tgt != ret_obj {
        remap.insert(exit_tgt, exit_new);
        done.insert(exit_tgt, true);
    }

    fb.end_loop(lh).expect("replay: end_loop");
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

    // A single op edge defines this object.
    let m = ir.morphism(ins[0]).expect("op edge");
    let new = emit_op(fb, ir, plan, fmap, remap, m.op, m.source, dest, m.loc);
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
        Call(g) => fb
            .call(fmap[g], feed(plan, remap, src), dest, loc)
            .expect("replay: call"),
        Map { body } => fb
            .map(fmap[body], feed(plan, remap, src), dest, loc)
            .expect("replay: map"),
        Fold { body } => fb
            .fold(fmap[body], feed(plan, remap, src), dest, loc)
            .expect("replay: fold"),
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
            emit_op(
                fb,
                ir,
                plan,
                fmap,
                remap,
                op,
                morph.source,
                Dest::Ret { slot: None },
                morph.loc,
            );
        }
    }
}

// --- recipe classification helpers ----------------------------------------

/// Whether `op` reads its source as an **internally-packed** tuple (the builder
/// mints that product atomically). These sources are never materialized.
fn reads_packed_source(op: Operation) -> bool {
    use Operation::*;
    matches!(
        op,
        Add | Sub
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
            | Print { .. }
            | LoopBack
            | LoopExit
    )
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

/// Feeder lookup: resolve `alias` transitively, then map to the new id (DESIGN
/// §1.1 id remap). Present by the time it is read (alias points earlier in topo,
/// feeders precede consumers).
fn feed(plan: &RewritePlan, remap: &SecondaryMap<ObjectId, ObjectId>, o: ObjectId) -> ObjectId {
    let r = plan.resolve_alias(o);
    remap[r]
}

// --- loop structure helpers -----------------------------------------------

/// The source object of `merge`'s `LoopEnter` in-edge (the loop init).
fn enter_source(ir: &CategoryIr, merge: ObjectId) -> Option<ObjectId> {
    ir.in_edges(merge).iter().find_map(|&m| {
        let mo = ir.morphism(m).unwrap();
        (mo.op == Operation::LoopEnter).then_some(mo.source)
    })
}

/// The source (route) object of `merge`'s `LoopBack` in-edge.
fn back_source(ir: &CategoryIr, merge: ObjectId) -> Option<ObjectId> {
    ir.in_edges(merge).iter().find_map(|&m| {
        let mo = ir.morphism(m).unwrap();
        (mo.op == Operation::LoopBack).then_some(mo.source)
    })
}

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

/// The single `LoopExit` for the loop whose merge is `merge`: the exit route
/// (its source), the exit target, and the morphism loc. Attribution is by
/// route-feeder membership in THIS merge's SCC (the interp driver's rule) —
/// never by reachability, which would attribute a downstream loop's exit to an
/// upstream merge in a two-sequential-loop function (S12).
fn exit_of(
    ir: &CategoryIr,
    _merge: ObjectId,
    scc: &SecondaryMap<ObjectId, bool>,
) -> Option<(ObjectId, ObjectId, flow_ir::SourceLoc)> {
    for (_, m) in ir.morphisms() {
        if m.op != Operation::LoopExit {
            continue;
        }
        if scc.contains_key(m.target) {
            continue;
        }
        if route_fed_by(ir, m.source, scc) {
            return Some((m.source, m.target, m.loc));
        }
    }
    None
}

/// Whether `route`'s slot feeders include an object of `scc` (the interp
/// driver's exit-attribution rule).
fn route_fed_by(ir: &CategoryIr, route: ObjectId, scc: &SecondaryMap<ObjectId, bool>) -> bool {
    ir.in_edges(route).iter().any(|&pm| {
        let pmo = ir.morphism(pm).unwrap();
        matches!(pmo.op, Operation::Pair { .. }) && scc.contains_key(pmo.source)
    })
}

/// The SCC (as an object list) that contains `o`, within function `f`.
fn scc_containing(ir: &CategoryIr, f: FuncId, o: ObjectId) -> Vec<ObjectId> {
    for comp in ir.sccs(f) {
        if comp.contains(&o) {
            return comp;
        }
    }
    vec![o]
}

// --- canonicity gate (R6) -------------------------------------------------

/// Whether every loop in `ir` is the canonical quartet: one merge per SCC, one
/// back and one exit per merge (DESIGN §5, R6). A non-canonical shape makes
/// `replay` return [`ReplayError::NonCanonicalLoop`].
pub fn is_canonical(ir: &CategoryIr) -> bool {
    for (f, _) in ir.funcs() {
        for lscc in ir.loop_structure(f) {
            if lscc.merges.len() != 1 {
                return false;
            }
            let merge = lscc.merges[0];
            let backs = ir
                .in_edges(merge)
                .iter()
                .filter(|&&m| ir.morphism(m).unwrap().op == Operation::LoopBack)
                .count();
            if backs != 1 {
                return false;
            }
            let scc: SecondaryMap<ObjectId, bool> = {
                let mut m = SecondaryMap::new();
                for &s in &lscc.objects {
                    m.insert(s, true);
                }
                m
            };
            let exits = ir
                .morphisms()
                .filter(|(_, m)| {
                    m.op == Operation::LoopExit
                        && !scc.contains_key(m.target)
                        && route_fed_by(ir, m.source, &scc)
                })
                .count();
            if exits != 1 {
                return false;
            }
        }
    }
    true
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
            Operation::Map { body } | Operation::Fold { body } => (morph.target, body),
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
            Operation::Call(g) | Operation::Map { body: g } | Operation::Fold { body: g } => {
                stack.push(g)
            }
            _ => {}
        }
    }
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
    inline_body(&mut hb, ir, fmap, spec.g, r1, Redirect::Return);
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
            fb, ir, &plan, fmap, &no_fuse, &mut local, &mut done, &obj_order, o, ret_obj,
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
/// `Map{g}` edge; its source `mid` was produced by a `Map{f}` whose source is the
/// original array — that array (already replayed) is the new map's input.
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
    let mid = ir.morphism(m_g).expect("g-edge").source;
    let m_f = ir.in_edges(mid)[0];
    let arr = ir.morphism(m_f).expect("f-edge").source;
    fb.map(h, feed(plan, remap, arr), dest, loc)
        .expect("replay: fused map")
}
