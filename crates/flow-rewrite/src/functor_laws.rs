//! Layer 1 (DESIGN §4) — map fusion + `map(id)` elimination. Pure analysis:
//! `&CategoryIr → RewritePlan`. The fusion body synthesis itself lives in
//! [`replay`](crate::replay); this pass only decides *what* to fuse/eliminate and
//! fills the `fuse` / `drop` / `alias` channels.
//!
//! Justifying law: the `List` functor law `map g ∘ map f = map (g ∘ f)`,
//! `map id = id` (category-ir §6.1.1). Preconditions (§1.2 P1/P2/P3): keys are
//! non-SCC `Temporary` objects; both fused bodies are transitively loop-free
//! (divergence guard, RW10) and lower-canonical (single full-value Return writer).
//!
//! ADR-0027 scoping: fusion fires only when both maps read the **identical**
//! capture objects (same ObjectIds, same order — the common chained-map case);
//! `map(id)` forwards only a capture-free map. Differing capture sets are
//! recorded headroom (the union re-threading is sound, not implemented).

use slotmap::SecondaryMap;

use flow_ir::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation};

use crate::graph_rewrites::is_pure;
use crate::plan::{FusionSpec, RewritePlan};

/// Analyze `ir` for the two layer-1 functor laws (DESIGN §4).
///
/// - `map g ∘ map f → map (g ∘ f)`: a `Map{f}` edge whose result array's **only**
///   consumer is a `Map{g}` edge. Emits `fuse[g_edge] = {f, g}` + `drop[mid]`.
///   ADR-0027: only when both maps read the identical capture objects (same
///   ids, same order); the fused map keeps the shared set.
/// - `map(id) → id`: a capture-free `Map` whose body is the identity (single
///   `Output` param→ret). Emits `alias[map_target] = arr` (P1: only a
///   `Temporary` target, never a Return writer).
pub fn analyze_map_fusion(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    let scc = scc_membership(ir);

    // Overlapping chains fuse at most once per pass (the driver's fixpoint reaches
    // the rest across rounds); a claimed morphism/object is off-limits.
    let mut claimed_m: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    let mut claimed_o: SecondaryMap<ObjectId, ()> = SecondaryMap::new();

    for (m_g, morph) in ir.morphisms() {
        // --- map(id) → id ------------------------------------------------
        // ADR-0027: only a capture-free identity map forwards its result — a
        // captured "identity" body returns (c₁…cₖ, elem), which is not the
        // identity on the array (and the alias target would mistype).
        if let Operation::Map {
            body, captures: 0, ..
        } = morph.op
            && is_identity_body(ir, body)
        {
            let map_target = morph.target;
            let arr = morph.source;
            // P1: only a `Temporary` target may be forwarded (never a Return
            // writer — dropping it fails I-RET). P2: key and value share SCC
            // membership — require both outside every loop SCC.
            if ir.object(map_target).map(|o| o.kind) == Some(ObjectKind::Temporary)
                && scc.get(map_target).is_none()
                && scc.get(arr).is_none()
                && !plan.alias.contains_key(map_target)
            {
                plan.alias.insert(map_target, arr);
                continue;
            }
            // Otherwise fall through: an identity map targeting Return may still
            // fuse if its source is itself a fusable `Map{f}` output.
        }

        // --- map g ∘ map f → map (g ∘ f) ---------------------------------
        let (g_body, g_caps) = match morph.op {
            Operation::Map { body, captures } => (body, captures),
            _ => continue,
        };
        // ADR-0027: g's mapped array plus its capture objects — for k>0 the
        // source is the product (c₁…cₖ, arr). A malformed arity never fuses.
        let Some((g_cap_objs, mid)) = map_source_parts(ir, morph.source, g_caps) else {
            continue;
        };
        // k>0: g's source product must be consumed ONLY by this edge — fusion
        // drops `mid`, which a shared product would still read.
        if g_caps > 0 && ir.out_edges(morph.source).len() != 1 {
            continue;
        }

        // The intermediate array must be a non-SCC `Temporary` (P2 drop key),
        // produced by exactly one `Map{f}`, and consumed ONLY by this `Map{g}`.
        if ir.object(mid).map(|o| o.kind) != Some(ObjectKind::Temporary)
            || scc.get(mid).is_some()
            || plan.alias.contains_key(mid)
            || ir.out_edges(mid).len() != 1
        {
            continue;
        }
        let mid_ins = ir.in_edges(mid);
        if mid_ins.len() != 1 {
            continue;
        }
        let f_morph = ir.morphism(mid_ins[0]).expect("mid in-edge");
        let (f_body, f_caps) = match f_morph.op {
            Operation::Map { body, captures } => (body, captures),
            _ => continue, // mid not produced by a Map — nothing to fuse.
        };
        // ADR-0027: fuse only when both maps read the *identical* capture
        // objects (same ids, same order) — the common chained-map case. The
        // union re-threading of differing capture sets is provably sound but
        // recorded headroom, not implemented.
        let Some((f_cap_objs, _)) = map_source_parts(ir, f_morph.source, f_caps) else {
            continue;
        };
        if f_cap_objs != g_cap_objs {
            continue;
        }

        // P3: both bodies transitively loop-free (a per-element reorder must not
        // flip Diverged ↔ Trapped). v1: each body single full-value Return writer.
        if !is_loop_free_fn(ir, f_body)
            || !is_loop_free_fn(ir, g_body)
            || !single_full_return_writer(ir, f_body)
            || !single_full_return_writer(ir, g_body)
        {
            continue;
        }

        // Non-overlap with an already-chosen fusion this pass.
        if claimed_m.contains_key(mid_ins[0])
            || claimed_m.contains_key(m_g)
            || claimed_o.contains_key(mid)
        {
            continue;
        }

        plan.fuse.insert(
            m_g,
            FusionSpec {
                f: f_body,
                g: g_body,
                captures: g_caps,
            },
        );
        plan.drop.insert(mid, ());
        claimed_m.insert(mid_ins[0], ());
        claimed_m.insert(m_g, ());
        claimed_o.insert(mid, ());
    }

    plan
}

/// Whether `body` **denotes the identity morphism**: its Return has a single
/// `Output` writer sourced from the body's own input Parameter, *and* every other
/// morphism it owns is pure and total.
///
/// The second half is not belt-and-braces — it is the law's precondition. The
/// oracle evaluates a body's whole graph (which is why DCE pins impure dead cones
/// live, R4), so a body that returns its parameter *and* computes a dead `x / x`
/// denotes `id ∘ trap : A ⇀ A`, not `id_A`. Eliminating that map deletes the
/// trap — a proptest counterexample did exactly that (S34,
/// `plans/plan-s34-identity-map-trap.md`): `Trapped(DivZero)` became `Done(0)`.
///
/// `Pair` is admitted as structural (a product build has no observable of its
/// own). `Widen`/`Iota`/`Fill` are total and would be admissible, but they are
/// outside [`is_pure`]; refusing a legal rewrite costs an optimisation, allowing
/// an illegal one costs the guarantee.
fn is_identity_body(ir: &CategoryIr, body: FuncId) -> bool {
    let def = match ir.func(body) {
        Some(d) => d,
        None => return false,
    };
    let ins = ir.in_edges(def.output);
    if ins.len() != 1 {
        return false;
    }
    let m = ir.morphism(ins[0]).expect("ret in-edge");
    if m.op != Operation::Output || m.source != def.input {
        return false;
    }
    def.morphisms.iter().all(|&mm| {
        let op = ir.morphism(mm).expect("body morphism").op;
        matches!(op, Operation::Output | Operation::Pair { .. }) || is_pure(op)
    })
}

/// Whether `body`'s Return has exactly one writer and it is a full-value writer
/// (not a `Pair{slot}` slot write) — the lower-canonical shape fusion needs.
fn single_full_return_writer(ir: &CategoryIr, body: FuncId) -> bool {
    let def = match ir.func(body) {
        Some(d) => d,
        None => return false,
    };
    let ins = ir.in_edges(def.output);
    ins.len() == 1
        && !matches!(
            ir.morphism(ins[0]).expect("ret in-edge").op,
            Operation::Pair { .. }
        )
}

/// Whether `root` is transitively loop-free (§3.1 "total" notion, RW10): neither
/// it nor any function reachable from it via `Call`/`Map`/`Fold` contains a loop.
/// The reference graph is acyclic (I6), so the closure terminates.
fn is_loop_free_fn(ir: &CategoryIr, root: FuncId) -> bool {
    let mut seen: SecondaryMap<FuncId, ()> = SecondaryMap::new();
    let mut stack = vec![root];
    while let Some(f) = stack.pop() {
        if seen.contains_key(f) {
            continue;
        }
        seen.insert(f, ());
        if !ir.loop_structure(f).is_empty() {
            return false;
        }
        for &m in &ir.func(f).expect("fn").morphisms {
            match ir.morphism(m).expect("morph").op {
                Operation::Call(g)
                | Operation::Map { body: g, .. }
                | Operation::Fold { body: g, .. } => stack.push(g),
                _ => {}
            }
        }
    }
    true
}

/// Per-object membership in **any** non-trivial loop SCC across all functions
/// (each function's SCCs are disjoint by ownership). Present ⇒ loop-carried.
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

// --- ADR-0027 source-shape readers ------------------------------------------

/// A `Map` edge's operands (ADR-0027): its capture objects in slot order plus
/// the mapped array. `src` is the edge's source — the bare array for k=0, the
/// `(c₁…cₖ, arr)` product for k>0. `None` on a malformed product arity (the
/// graph would already fail validate; the analysis stays conservative).
fn map_source_parts(
    ir: &CategoryIr,
    src: ObjectId,
    captures: u32,
) -> Option<(Vec<ObjectId>, ObjectId)> {
    if captures == 0 {
        return Some((Vec::new(), src));
    }
    let f = slot_feeders(ir, src);
    let k = captures as usize;
    if f.len() != k + 1 {
        return None;
    }
    Some((f[..k].to_vec(), f[k]))
}

/// Slot feeders of a product, in slot order (declaration = slot order).
fn slot_feeders(ir: &CategoryIr, product: ObjectId) -> Vec<ObjectId> {
    let mut v: Vec<(u32, ObjectId)> = ir
        .in_edges(product)
        .iter()
        .filter_map(|&m| {
            let mo = ir.morphism(m).expect("in-edge");
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
