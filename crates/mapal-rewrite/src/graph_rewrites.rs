//! Layer 4 (DESIGN §3) — dead-code elimination + common-subexpression
//! elimination. Pure analyses: `&CategoryIr → RewritePlan`.
//!
//! Both are **conservative**: DCE removes only dead cones of provably
//! non-trapping, non-diverging *pure* ops (a superset of §3.1's non-removable set
//! is pinned live — sound because keeping is always R1-safe, and trap-conservative
//! per R4); CSE merges duplicate pure `Temporary` computations (constant dedup is
//! **not** done — P1 forbids keying a `Constant`; see the crate report).
//!
//! Justifying law: graph liveness (DCE, trap-conservative R4) and "same op + same
//! source ⇒ same morphism" (CSE, category-ir §9.4). Every key is a non-SCC
//! `Temporary` (§1.2 P1/P2).

use std::collections::BTreeMap;

use slotmap::SecondaryMap;

use mapal_ir::{
    CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation, ty_contains_token, ty_label,
};

use crate::plan::RewritePlan;

// --- DCE (DESIGN §3.1) ----------------------------------------------------

/// Analyze `ir` for dead pure cones (DESIGN §3.1). `drop`s every non-SCC
/// `Temporary` unreachable (backward, over `in_edges`) from a **keep root**:
/// each function's Return, every loop-SCC object (RW11 — a pure dead loop is
/// kept), every non-pure op result (Div/Mod/Index/Call/Map/Fold/Print — possibly
/// trapping/diverging, R4), and every `LoopExit` target (the quartet rebuilds it
/// unconditionally). Everything dropped is therefore a dead **pure** op whose
/// removal cannot change the run's class.
pub fn analyze_dce(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    let scc = scc_membership(ir);

    let mut roots: Vec<ObjectId> = Vec::new();
    for (_, def) in ir.funcs() {
        roots.push(def.output);
    }

    // plan-s39: a gated `Phi` whose arm can trap is NOT droppable, even when
    // its result is dead. `Phi` is in `is_pure`, so DCE would delete it — but
    // R4 keeps the arm's `Div`, and with the guard gone that `Div` runs
    // unconditionally: `Done(1)` became `Trapped(DivZero)` (property
    // `closed_default`). The guard is what decides whether the trap fires, so
    // it inherits the trap-capability of the arms it gates and is pinned with
    // them. Dual to the other direction: exempting the ARM from R4 let DCE
    // delete a trap that must fire (`open_default`), which is why the arm
    // stays pinned too.
    // plan-s40 review find [3]/[6]: the pin keys on gated(), not can_trap
    // alone. An arm owning a trap-free loop is heavy-but-not-trap-capable;
    // dropping its dead Phi un-gates the loop (the SCC itself is pinned by
    // RW11 below), and the flat walk then drives it unconditionally — a
    // non-terminating untaken loop went Done raw, Diverged rewritten.
    for (f, _) in ir.funcs() {
        for site in ir.guard_plan(f) {
            if site.gated() {
                roots.push(ir.morphism(site.phi).expect("phi").target);
            }
        }
    }
    for (o, obj) in ir.objects() {
        if scc.contains_key(o) {
            roots.push(o); // loop machinery pinned live (RW11 / P2).
            continue;
        }
        if obj.kind != ObjectKind::Temporary {
            continue;
        }
        // A LoopExit target is materialized by the quartet regardless of `drop`;
        // pin it so its cone survives and the fixpoint never re-drops it.
        if ir
            .in_edges(o)
            .iter()
            .any(|&m| ir.morphism(m).expect("in").op == Operation::LoopExit)
        {
            roots.push(o);
            continue;
        }
        // A non-pure op may trap or diverge; keeping it (and its cone) preserves
        // the run's class even when its result is dead (R4).
        //
        // plan-s39: guard-arm work is NOT exempt from this, even though it only
        // fires when the condition picks its arm. A dead `Phi` is still walked
        // by the interpreter, so it still fires its chosen arm and can still
        // trap — exempting gated results let DCE turn `Trapped(DivZero)` into
        // `Done(0)` (property `open_default`). The orphan case that motivated
        // the exemption is handled where it belongs: ConstFold drops the losing
        // arm and the triple in the same plan when it resolves a condition.
        if let Some(m) = op_edge(ir, o)
            && !is_pure(ir.morphism(m).expect("op").op)
        {
            roots.push(o);
        }
    }

    // plan-s40 review find [5]: guard verdicts are a function of CONSUMER
    // SETS, and the interpreter executes dead code — so removing a dead
    // reader of an arm value changes what the raw program means: a value
    // with a dead second reader is strict, and with the reader dropped the
    // site gates, suppressing a trap the raw run fires (`Trapped(DivZero)`
    // raw, `Done` rewritten — the 1024-case hammer's DCE seed). DCE
    // therefore preserves every dead cone that can touch a verdict: pin each
    // dead `Temporary` SINK forward-reachable from the verdict cone (the
    // backward cone of every Phi triple, widened by the member targets of
    // any loop unit that cone touches, since unit joinability inspects all
    // of them — including the sink refusal). Pinning the SINK is what makes
    // the whole cone survive: replay materializes only read objects, so
    // pinning an interior product alone does not outlive the rebuild.
    // Phi-free functions pin nothing and keep byte-identical DCE.
    'verdict: {
        let mut cone: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut stack: Vec<ObjectId> = Vec::new();
        for (_, morph) in ir.morphisms() {
            if morph.op == Operation::Phi {
                stack.push(morph.source);
            }
        }
        if stack.is_empty() {
            // Phi-free graph: no verdicts to preserve — skip the halo,
            // forward and taint walks (compile-time A/B gate, S40).
            break 'verdict;
        }
        while let Some(o) = stack.pop() {
            if cone.insert(o, ()).is_some() {
                continue;
            }
            for &m in ir.in_edges(o) {
                stack.push(ir.morphism(m).expect("in").source);
            }
        }
        for (f, _) in ir.funcs() {
            for lscc in ir.loop_structure(f) {
                if !lscc.objects.iter().any(|o| cone.contains_key(*o)) {
                    continue;
                }
                let objs: SecondaryMap<ObjectId, ()> = {
                    let mut s = SecondaryMap::new();
                    for &o in &lscc.objects {
                        s.insert(o, ());
                    }
                    s
                };
                for (_, morph) in ir.morphisms() {
                    if objs.contains_key(morph.source) || objs.contains_key(morph.target) {
                        stack.push(morph.target);
                    }
                }
                while let Some(o) = stack.pop() {
                    if cone.insert(o, ()).is_some() {
                        continue;
                    }
                    for &m in ir.in_edges(o) {
                        stack.push(ir.morphism(m).expect("in").source);
                    }
                }
            }
        }
        // Forward closure of the cone, then pin its dead Temporary sinks —
        // but only TAINTED ones (a sink whose backward cone contains an
        // impure morphism: a trap, a call, loop machinery). A flip on a
        // pure-total value's ownership is unobservable — computed or skipped,
        // nobody can tell — so preserving a pure dead cone would change
        // surface emissions (sepia kept a dead float pack) for nothing.
        let mut fwd = cone;
        let mut stack: Vec<ObjectId> = fwd.iter().map(|(o, ())| o).collect();
        while let Some(o) = stack.pop() {
            for &m in ir.out_edges(o) {
                let t = ir.morphism(m).expect("out").target;
                if fwd.insert(t, ()).is_none() {
                    stack.push(t);
                }
            }
        }
        let tainted: SecondaryMap<ObjectId, ()> = {
            let mut t: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            let mut stack: Vec<ObjectId> = Vec::new();
            for (_, morph) in ir.morphisms() {
                // Observable-when-executed: partial ops, bodies/calls that may
                // hide them, effects, and loop machinery (divergence). NOT
                // `!is_pure` — that list omits `Pair`, and a staging edge is
                // not an observation.
                if matches!(
                    morph.op,
                    Operation::Div
                        | Operation::Mod
                        | Operation::Index
                        | Operation::Update
                        | Operation::Call(_)
                        | Operation::Map { .. }
                        | Operation::Fold { .. }
                        | Operation::Print { .. }
                        | Operation::TimeMs
                        | Operation::Output
                        | Operation::LoopEnter
                        | Operation::LoopBack
                        | Operation::LoopExit
                ) {
                    stack.push(morph.target);
                }
            }
            while let Some(o) = stack.pop() {
                if t.insert(o, ()).is_some() {
                    continue;
                }
                for &m in ir.out_edges(o) {
                    stack.push(ir.morphism(m).expect("out").target);
                }
            }
            t
        };
        for (o, obj) in ir.objects() {
            if obj.kind == ObjectKind::Temporary
                && ir.out_edges(o).is_empty()
                && fwd.contains_key(o)
                && tainted.contains_key(o)
            {
                roots.push(o);
            }
        }
    }

    let keep = backward_reachable(ir, &roots);

    for (o, obj) in ir.objects() {
        if obj.kind == ObjectKind::Temporary && !scc.contains_key(o) && !keep.contains_key(o) {
            plan.drop.insert(o, ());
        }
    }
    plan
}

/// Whether `op` is a pure, total value op: its only effect is its result, it
/// never traps and never diverges (DESIGN §3.1 "always removable" row). `Div`,
/// `Mod`, `Index`, `Call`, `Map`, `Fold`, `Print` are conservatively impure.
///
/// The crate's single "pure and total" predicate: DCE uses it to decide what a
/// dead cone may drop, and [`crate::functor_laws`] uses it to decide whether a
/// map body denotes the *identity morphism* — the same question, so the same
/// list. Two lists would drift, and a drift here deletes observable behaviour.
pub(crate) fn is_pure(op: Operation) -> bool {
    use Operation::*;
    matches!(
        op,
        Add | Sub
            | Mul
            | Neg
            | Eq
            | Neq
            | Lt
            | Gt
            | Le
            | Ge
            | And
            | Or
            | Not
            | Phi
            | Proj { .. }
            | Zip
            | Enumerate
    )
}

/// Objects backward-reachable from any root over `in_edges` (multi-source BFS;
/// J1). Backward-closed: a kept object's every feeder is kept, so no kept object
/// ever references a dropped one.
fn backward_reachable(ir: &CategoryIr, roots: &[ObjectId]) -> SecondaryMap<ObjectId, ()> {
    let mut keep: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    let mut stack: Vec<ObjectId> = Vec::new();
    for &r in roots {
        if keep.insert(r, ()).is_none() {
            stack.push(r);
        }
    }
    while let Some(o) = stack.pop() {
        for &m in ir.in_edges(o) {
            let s = ir.morphism(m).expect("in").source;
            if keep.insert(s, ()).is_none() {
                stack.push(s);
            }
        }
    }
    keep
}

// --- CSE (DESIGN §3.2) ----------------------------------------------------

/// Analyze `ir` for common subexpressions (DESIGN §3.2). One insertion-order walk
/// (a valid value-topo for non-SCC objects — feeders are built before consumers);
/// first occurrence wins, later duplicates get `alias[target] = first`. Excludes
/// non-`Temporary` targets, token-bearing tys, loop-SCC objects (cross-SCC merges
/// break R6), and internal packs (never materialized). Constant dedup is omitted
/// (P1 forbids keying a `Constant`).
pub fn analyze_cse(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    let scc = scc_membership(ir);
    let fseq = func_seq(ir);
    let seq = obj_seq(ir);

    // Pass-local value numbering: `merged[o]` = the surviving representative of
    // `o`; feeders are keyed through it (one hop — the representative is never
    // itself merged, first-wins).
    let mut merged: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();
    let mut seen: BTreeMap<String, ObjectId> = BTreeMap::new();

    for (o, obj) in ir.objects() {
        if obj.kind != ObjectKind::Temporary || scc.contains_key(o) {
            continue;
        }
        if ty_contains_token(&obj.ty) {
            continue; // I4: merging token producers could widen token out-degree.
        }
        if is_internal_pack(ir, o) {
            continue; // rebuilt by its consuming primitive; keying it is inert.
        }
        let key = match cse_key(ir, &merged, &seq, &fseq, o) {
            Some(k) => k,
            None => continue,
        };
        match seen.get(&key) {
            Some(&first) => {
                plan.alias.insert(o, first);
                merged.insert(o, first);
            }
            None => {
                seen.insert(key, o);
            }
        }
    }
    plan
}

/// A deterministic value-number key for `o`: its op (functions by `funcs()`
/// order, not a `FuncId` bit pattern), its value feeders resolved through the
/// pass-local merge map (by `objects()` order), and its result ty. `None` for an
/// object with no keyable recipe.
fn cse_key(
    ir: &CategoryIr,
    merged: &SecondaryMap<ObjectId, ObjectId>,
    seq: &SecondaryMap<ObjectId, u32>,
    fseq: &SecondaryMap<FuncId, u32>,
    o: ObjectId,
) -> Option<String> {
    let ty = ty_label(&ir.object(o)?.ty);
    let resolve = |f: ObjectId| -> u32 { seq[merged.get(f).copied().unwrap_or(f)] };

    // A materialized product (pack): key by kind-ty + slot feeders.
    if is_pack(ir, o) {
        let feeders: Vec<u32> = slot_feeders(ir, o).into_iter().map(resolve).collect();
        return Some(format!("pack|{ty}|{feeders:?}"));
    }
    // An op-defined temporary.
    let m = op_edge(ir, o)?;
    let morph = ir.morphism(m).expect("op");
    let feeders: Vec<u32> = value_feeders(ir, morph.op, morph.source)
        .into_iter()
        .map(resolve)
        .collect();
    Some(format!("{}|{ty}|{feeders:?}", op_tag(morph.op, fseq)))
}

/// A stable op tag: `Call`/`Map`/`Fold` encode the callee/body by `funcs()`
/// position; every other op by its `Debug` (payloads are plain scalars).
/// ADR-0027: the capture count is part of a Map/Fold's identity (`^k`); the
/// capture *objects* are keyed via `value_feeders` (the source product's slot
/// feeders), so maps differing only in their captures never share a key.
fn op_tag(op: Operation, fseq: &SecondaryMap<FuncId, u32>) -> String {
    match op {
        Operation::Call(g) => format!("Call{}", fseq[g]),
        Operation::Map { body, captures } => format!("Map{}^{}", fseq[body], captures),
        Operation::Fold { body, captures } => format!("Fold{}^{}", fseq[body], captures),
        other => format!("{other:?}"),
    }
}

/// The value operands of `op` whose source is `src`: the internal pack's slot
/// feeders for a packed-source op, else the single direct source.
fn value_feeders(ir: &CategoryIr, op: Operation, src: ObjectId) -> Vec<ObjectId> {
    if reads_packed_source(op) {
        slot_feeders(ir, src)
    } else {
        vec![src]
    }
}

// --- shared graph readers -------------------------------------------------

/// Per-object membership in any loop SCC (each function's SCCs disjoint by
/// ownership). Present ⇒ loop-carried.
fn scc_membership(ir: &CategoryIr) -> SecondaryMap<ObjectId, ()> {
    let mut set: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    for (f, _) in ir.funcs() {
        for lscc in ir.loop_structure(f) {
            for &o in &lscc.objects {
                set.insert(o, ());
            }
        }
    }
    set
}

/// Object → its `objects()` position (a stable id-independent sequence number).
fn obj_seq(ir: &CategoryIr) -> SecondaryMap<ObjectId, u32> {
    let mut m: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
    for (i, (o, _)) in ir.objects().enumerate() {
        m.insert(o, i as u32);
    }
    m
}

/// Function → its `funcs()` position.
fn func_seq(ir: &CategoryIr) -> SecondaryMap<FuncId, u32> {
    let mut m: SecondaryMap<FuncId, u32> = SecondaryMap::new();
    for (i, (f, _)) in ir.funcs().enumerate() {
        m.insert(f, i as u32);
    }
    m
}

/// The single op-defining in-edge of `o` (its one non-`Pair`/`Output`/loop
/// in-edge), or `None`.
fn op_edge(ir: &CategoryIr, o: ObjectId) -> Option<MorphismId> {
    let ins = ir.in_edges(o);
    if ins.len() != 1 {
        return None;
    }
    let m = ins[0];
    match ir.morphism(m).expect("in").op {
        Operation::Pair { .. }
        | Operation::LoopEnter
        | Operation::LoopBack
        | Operation::LoopExit
        | Operation::Output => None,
        _ => Some(m),
    }
}

/// Whether `o` is a pack-built product (≥1 in-edge, all `Pair`).
fn is_pack(ir: &CategoryIr, o: ObjectId) -> bool {
    let ins = ir.in_edges(o);
    !ins.is_empty()
        && ins
            .iter()
            .all(|&m| matches!(ir.morphism(m).expect("in").op, Operation::Pair { .. }))
}

/// Whether `o` is an internal pack (a product consumed only as one
/// packed-source primitive's source) — never materialized.
fn is_internal_pack(ir: &CategoryIr, o: ObjectId) -> bool {
    let outs = ir.out_edges(o);
    if outs.len() != 1 {
        return false;
    }
    let m = ir.morphism(outs[0]).expect("out");
    m.source == o && reads_packed_source(m.op)
}

/// Whether `op` reads its source as an internally-packed product (the builder
/// mints it atomically).
///
/// ADR-0027: a `Map`/`Fold` with `captures > 0` qualifies (the `(c₁…cₖ, …)`
/// source is minted by `map_captured`/`fold_captured`); k=0 keeps the
/// historical shapes. Keep in sync with `replay`'s copy — a pack replay treats
/// as internal must never become a CSE representative.
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

/// Slot feeders of a product, in slot order.
fn slot_feeders(ir: &CategoryIr, product: ObjectId) -> Vec<ObjectId> {
    let mut v: Vec<(u32, ObjectId)> = ir
        .in_edges(product)
        .iter()
        .filter_map(|&m| {
            let mo = ir.morphism(m).expect("in");
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
