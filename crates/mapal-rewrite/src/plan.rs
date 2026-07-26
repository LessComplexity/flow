//! `RewritePlan` — the read-only analysis product every pass emits (DESIGN §1,
//! morphism table). One replayer, six plan channels: `alias` / `constify` /
//! `drop` / `fuse` / `inline` / `lift`. The plan is pure data; [`replay`](crate::replay)
//! is the only consumer.
//!
//! All maps are `SecondaryMap`/`Vec` keyed by the **input** graph's ids — no
//! `HashMap` in any output-affecting path (determinism D2).

use slotmap::SecondaryMap;

use mapal_ir::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Value};

/// A fused `Map` synthesis request (DESIGN §4). `map g ∘ map f → map (g ∘ f)`:
/// the `g`-edge is keyed in [`RewritePlan::fuse`]; `f`/`g` are the two bodies to
/// splice. Body synthesis itself lands in WP4 — WP1 only carries the datum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionSpec {
    /// The first (inner) map body — applied per element first.
    pub f: FuncId,
    /// The second (outer) map body — applied to `f`'s result.
    pub g: FuncId,
    /// The shared capture count of both maps (ADR-0027). Fusion is offered only
    /// when the two maps read the *identical* capture objects (same ids, same
    /// order); the fused map keeps them as its leading source components. `0`
    /// for every pre-ADR-0027 chained map.
    pub captures: u32,
}

/// Which collection operation replaces a canonical loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiftKind {
    /// R-LF: the second state component becomes the fold accumulator.
    Fold {
        /// The loop's accumulator component object.
        accumulator: ObjectId,
        /// The loop-invariant accumulator seed.
        seed: ObjectId,
    },
    /// R-LM: the second state component is replaced by a captured map.
    Map,
}

/// A loop→map/fold synthesis request keyed by its `LoopMerge`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiftSpec {
    /// R-LF or R-LM and its rule-specific operands.
    pub kind: LiftKind,
    /// The projected canonical counter object; becomes the collection item.
    pub counter: ObjectId,
    /// Static positive trip count (`1..=i32::MAX`).
    pub count: i32,
    /// Loop-invariant values threaded into the synthesized body, in stable
    /// object insertion order.
    pub captures: Vec<ObjectId>,
    /// The backward cone copied into the synthesized body. Membership only;
    /// replay walks the original deterministic object order.
    pub body_objects: Vec<ObjectId>,
    /// The cone root returned by the synthesized body.
    pub body_result: ObjectId,
}

/// The six plan channels (DESIGN §1 morphism table + `inline` + loop lifting).
/// Empty plan ⇒ identity replay (the WP1 anchor).
#[derive(Debug, Default)]
pub struct RewritePlan {
    /// `alias?`: consumers of the key read the value instead. Resolved
    /// transitively before replay; acyclic by construction (always points at an
    /// earlier-in-topo object).
    pub alias: SecondaryMap<ObjectId, ObjectId>,
    /// `constify?`: the object is re-materialized as a `Constant` with this
    /// value; its defining cone is not replayed for it.
    pub constify: SecondaryMap<ObjectId, Value>,
    /// `drop`: objects (with their defining morphisms) not replayed at all.
    pub drop: SecondaryMap<ObjectId, ()>,
    /// `fuse?`: a `Map` edge replaced by a fused `Map` (layer 1).
    pub fuse: SecondaryMap<MorphismId, FusionSpec>,
    /// `inline?`: a `Call` edge stripped by graph substitution (region-emission
    /// plan Move 1) — the callee's body is replayed into the caller with
    /// `input ↦ call source`, `output ↦ call target`. Keys are Call-edge ids;
    /// P1/P2 (object-keyed) do not apply.
    pub inline: SecondaryMap<MorphismId, ()>,
    /// `lift?`: a canonical loop SCC replaced by `Iota` + captured `Map`/`Fold`.
    /// Keys are the input graph's `LoopMerge` objects.
    pub lift: SecondaryMap<ObjectId, LiftSpec>,
}

impl RewritePlan {
    /// A fresh empty plan.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether every channel is empty — a round whose plan is empty is a no-op
    /// (DESIGN §5, by-value intake).
    pub fn is_empty(&self) -> bool {
        self.alias.is_empty()
            && self.constify.is_empty()
            && self.drop.is_empty()
            && self.fuse.is_empty()
            && self.inline.is_empty()
            && self.lift.is_empty()
    }

    /// Resolve an `alias` chain transitively to its terminal object (DESIGN §1).
    /// Acyclic by construction; the step cap is a defense against a malformed
    /// plan (never trusted to loop forever).
    pub fn resolve_alias(&self, o: ObjectId) -> ObjectId {
        let mut cur = o;
        // Chains are short (each hop points earlier in topo); the cap is a hard
        // ceiling, not an expected bound.
        for _ in 0..self.alias.len() + 1 {
            match self.alias.get(cur) {
                Some(&next) if next != cur => cur = next,
                _ => break,
            }
        }
        cur
    }

    /// P1 + P2 (DESIGN §1.2) — the plan-consistency laws every pass must honour,
    /// checked in debug builds before replay.
    ///
    /// - **P1**: `alias`/`constify`/`drop` key only `Temporary` objects (never
    ///   Parameter/Constant/Return/LoopMerge).
    /// - **P2**: `constify`/`drop` keys sit in **no** loop SCC; an `alias` key and
    ///   its value share SCC membership (both out, or both the same SCC).
    ///
    /// Always compiled (the `debug_assert!` that calls it type-checks in release
    /// builds — e.g. benches — even though it only *runs* in debug).
    pub fn is_consistent(&self, ir: &CategoryIr) -> bool {
        let scc_of = scc_membership(ir);

        // P1: keyed objects are Temporary only.
        let is_temp = |o: ObjectId| ir.object(o).map(|x| x.kind) == Some(ObjectKind::Temporary);
        for (o, _) in self.alias.iter() {
            if !is_temp(o) {
                return false;
            }
        }
        for (o, _) in self.constify.iter() {
            if !is_temp(o) {
                return false;
            }
        }
        for (o, _) in self.drop.iter() {
            if !is_temp(o) {
                return false;
            }
        }

        // P2: constify/drop keys in no SCC; alias key/value same SCC membership.
        for (o, _) in self.constify.iter() {
            if scc_of.get(o).is_some() {
                return false;
            }
        }
        for (o, _) in self.drop.iter() {
            if scc_of.get(o).is_some() {
                return false;
            }
        }
        for (k, &val) in self.alias.iter() {
            if scc_of.get(k).copied() != scc_of.get(val).copied() {
                return false;
            }
        }
        true
    }
}

/// Per-object non-trivial-SCC index across all functions (each function's SCCs
/// are disjoint by ownership). Only loop SCCs are recorded; a straight-line
/// object is absent (`None`). Helper for the P2 consistency check.
fn scc_membership(ir: &CategoryIr) -> SecondaryMap<ObjectId, usize> {
    let mut scc_of: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
    let mut next = 0usize;
    for (f, _) in ir.funcs() {
        for scc in ir.loop_structure(f) {
            for &o in &scc.objects {
                scc_of.insert(o, next);
            }
            next += 1;
        }
    }
    scc_of
}
