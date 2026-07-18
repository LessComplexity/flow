//! Graph algorithms (DESIGN §13): iterative Tarjan SCC, Kahn topological order,
//! and the `loop_structure` backend predicate.
//!
//! Everything here is `O(V + E)` and **recursion-free** (J1): Tarjan uses an
//! explicit work stack, Kahn is a worklist. The §16 deep-graph test (100k-object
//! chain) exercises the no-stack-overflow guarantee.

use slotmap::SecondaryMap;

use crate::graph::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation};

/// A non-trivial loop SCC and its merge objects (DESIGN §7; the backend
/// capability predicate). One entry per non-trivial SCC.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopScc {
    /// All objects in the SCC, in deterministic order.
    pub objects: Vec<ObjectId>,
    /// The `LoopMerge` objects within this SCC (≥1; >1 for nested loops).
    pub merges: Vec<ObjectId>,
}

/// The per-merge loop attribution the interp driver, rewrite replayer, and the
/// backend-llvm emitter all derive from one graph (DESIGN §3, BL7 — the one
/// source of truth for the rule whose two hand-maintained copies both regressed
/// in S12). Produced by [`CategoryIr::loop_plan`], which returns `None` for any
/// non-canonical shape (multi-merge SCC, ≠1 `LoopBack`, ≠1 attributed
/// `LoopExit`) — so `is_canonical`-style gates read `.is_some()` and the interp
/// / backend drivers, which only ever run on gated-canonical loops, `.expect()`.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopPlan {
    /// The `LoopMerge` object this plan is for.
    pub merge: ObjectId,
    /// `source(LoopEnter → merge)` — the init value object (loop-invariant).
    pub init: ObjectId,
    /// `source(LoopBack → merge)` — the `(next_state, cond)` route object.
    pub back_route: ObjectId,
    /// The attributed `LoopExit` morphisms (canonical: exactly one).
    pub exits: Vec<MorphismId>,
    /// `source(the unique attributed LoopExit)` — the `(payload, cond)` route.
    pub exit_route: ObjectId,
    /// The SCC's objects (membership set; deterministic order).
    pub scc_objects: Vec<ObjectId>,
    /// Decide phase (ADR-0016): body morphisms whose target is backward-reachable
    /// from `exit_route` — the cond + exit-route feeders, run every iteration.
    pub decide_order: Vec<MorphismId>,
    /// Advance phase: the rest of the body (next-state), skipped on the exit step.
    pub advance_order: Vec<MorphismId>,
    /// Product objects assembled by `Pair` edges across the body (reset/iter).
    pub product_targets: Vec<ObjectId>,
}

impl CategoryIr {
    /// The objects owned by function `f`, in deterministic order.
    fn func_objects(&self, f: FuncId) -> Vec<ObjectId> {
        self.objects
            .iter()
            .filter(|(id, _)| self.owner.get(*id) == Some(&f))
            .map(|(id, _)| id)
            .collect()
    }

    /// Strongly-connected components of function `f`'s object subgraph, via
    /// **iterative** Tarjan (DESIGN §13). Edges are `f`'s morphisms; every SCC
    /// is returned, trivial (single-object, no self-loop) ones included.
    ///
    /// Order: Tarjan emits SCCs in reverse-topological order; within an SCC,
    /// objects are in the order Tarjan pops them. Deterministic given the
    /// insertion-ordered adjacency.
    pub fn sccs(&self, f: FuncId) -> Vec<Vec<ObjectId>> {
        let nodes = self.func_objects(f);

        // Tarjan bookkeeping in SecondaryMaps (no HashMap; I12).
        let mut index: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
        let mut lowlink: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
        let mut on_stack: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
        let mut visited: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

        let mut next_index: u32 = 0;
        let mut tarjan_stack: Vec<ObjectId> = Vec::new();
        let mut sccs: Vec<Vec<ObjectId>> = Vec::new();

        // Explicit DFS frame: the node, and a cursor into its out-edges.
        struct Frame {
            node: ObjectId,
            edge_cursor: usize,
        }

        for &root in &nodes {
            if visited.get(root).copied().unwrap_or(false) {
                continue;
            }
            let mut call_stack: Vec<Frame> = vec![Frame {
                node: root,
                edge_cursor: 0,
            }];

            // Initialize root on first push.
            index.insert(root, next_index);
            lowlink.insert(root, next_index);
            next_index += 1;
            visited.insert(root, true);
            on_stack.insert(root, true);
            tarjan_stack.push(root);

            while let Some(frame) = call_stack.last_mut() {
                let v = frame.node;
                let out = self.out_edges(v);
                let mut recursed = false;

                while frame.edge_cursor < out.len() {
                    let m = out[frame.edge_cursor];
                    frame.edge_cursor += 1;
                    let w = self.morphisms[m].target;
                    // Only follow edges that stay within f's object set.
                    if self.owner.get(w) != Some(&f) {
                        continue;
                    }
                    if !visited.get(w).copied().unwrap_or(false) {
                        // "Recurse" into w.
                        index.insert(w, next_index);
                        lowlink.insert(w, next_index);
                        next_index += 1;
                        visited.insert(w, true);
                        on_stack.insert(w, true);
                        tarjan_stack.push(w);
                        call_stack.push(Frame {
                            node: w,
                            edge_cursor: 0,
                        });
                        recursed = true;
                        break;
                    } else if on_stack.get(w).copied().unwrap_or(false) {
                        let iw = index[w];
                        let lv = lowlink[v];
                        if iw < lv {
                            lowlink.insert(v, iw);
                        }
                    }
                }

                if recursed {
                    continue;
                }

                // Done with v's edges: if it is an SCC root, pop the component.
                if lowlink[v] == index[v] {
                    let mut comp = Vec::new();
                    loop {
                        let w = tarjan_stack.pop().expect("tarjan stack underflow");
                        on_stack.insert(w, false);
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(comp);
                }

                // Pop v's frame and propagate lowlink to the parent.
                call_stack.pop();
                if let Some(parent) = call_stack.last() {
                    let p = parent.node;
                    let lv = lowlink[v];
                    let lp = lowlink[p];
                    if lv < lp {
                        lowlink.insert(p, lv);
                    }
                }
            }
        }

        sccs
    }

    /// A topological order of `f`'s morphisms (DESIGN §13), Kahn's algorithm
    /// over the morphisms with **`LoopBack` edges excluded as gating** (but
    /// still emitted — interp needs them).
    ///
    /// Completion rule: Parameters/Constants start complete; a product object
    /// completes when all `arity` slot edges are emitted; a `LoopMerge`
    /// completes on its `LoopEnter` alone (header-first); every other object
    /// completes on its one definer. `LoopExit` edges are ordinary gating edges.
    /// `LoopBack` is appended after its source completes, never gating. Ties
    /// break by insertion order (deterministic).
    ///
    /// **`LoopEnter` edges are deferred**: a ready `LoopEnter` is released only
    /// when no other morphism is ready. This guarantees every morphism not
    /// (transitively) gated by a loop merge — i.e. every loop-invariant
    /// computation, however many hops from its sources — precedes the loop
    /// header in the order. The interp loop driver and straight-line backends
    /// rely on this: they read loop-invariant operands when the header fires
    /// (S12 fix; previously a multi-hop invariant like `x * 2` inside a loop
    /// body was ordered after its `LoopEnter` and read-before-write).
    pub fn topo_order(&self, f: FuncId) -> Vec<MorphismId> {
        let nodes = self.func_objects(f);

        // For each object, how many in-edges must fire before it "completes".
        // (b)/(c)/(d)/(e) of I3: product objects need all arity Pair edges;
        // LoopMerge needs only the LoopEnter; others need their single definer;
        // Parameters/Constants need zero.
        let mut remaining: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
        for &o in &nodes {
            let obj = &self.objects[o];
            let count = match obj.kind {
                ObjectKind::Parameter | ObjectKind::Constant => 0,
                ObjectKind::LoopMerge => {
                    // Completes on the single LoopEnter edge alone.
                    self.in_edges(o)
                        .iter()
                        .filter(|&&m| self.morphisms[m].op == Operation::LoopEnter)
                        .count() as u32
                }
                _ => {
                    // Temporary/Return: count the gating in-edges. LoopBack edges
                    // never target these (they target LoopMerge), so every in-edge
                    // here gates.
                    self.in_edges(o).len() as u32
                }
            };
            remaining.insert(o, count);
        }

        // Objects that are complete and ready to release their out-edges.
        let mut ready: Vec<ObjectId> = Vec::new();
        for &o in &nodes {
            if remaining[o] == 0 {
                ready.push(o);
            }
        }

        let mut order: Vec<MorphismId> = Vec::new();
        let mut released: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

        // LoopEnter edges whose source is complete, awaiting a drained worklist
        // (the deferral rule above). FIFO in source-release order (deterministic).
        let mut deferred_enters: Vec<MorphismId> = Vec::new();
        let mut dcursor = 0;

        // Process ready objects in a FIFO worklist; ties broken by the order
        // objects were discovered, which is insertion order. When the worklist
        // drains, release the next deferred LoopEnter and continue.
        let mut cursor = 0;
        loop {
            while cursor < ready.len() {
                let o = ready[cursor];
                cursor += 1;
                if released.get(o).copied().unwrap_or(false) {
                    continue;
                }
                released.insert(o, true);

                for &m in self.out_edges(o) {
                    let morph = &self.morphisms[m];
                    if morph.op == Operation::LoopBack {
                        // Emitted but never gates: append once its source is complete.
                        order.push(m);
                        continue;
                    }
                    if morph.op == Operation::LoopEnter {
                        // Deferred: released only when nothing else is ready.
                        deferred_enters.push(m);
                        continue;
                    }
                    // Ordinary gating edge: emit it, then decrement the target.
                    order.push(m);
                    let tgt = morph.target;
                    if self.owner.get(tgt) != Some(&f) {
                        continue;
                    }
                    let r = remaining[tgt];
                    if r > 0 {
                        let nr = r - 1;
                        remaining.insert(tgt, nr);
                        if nr == 0 {
                            ready.push(tgt);
                        }
                    }
                }
            }

            // Worklist drained: release the next deferred LoopEnter, if any.
            if dcursor < deferred_enters.len() {
                let m = deferred_enters[dcursor];
                dcursor += 1;
                order.push(m);
                let tgt = self.morphisms[m].target;
                if self.owner.get(tgt) == Some(&f) {
                    let r = remaining[tgt];
                    if r > 0 {
                        let nr = r - 1;
                        remaining.insert(tgt, nr);
                        if nr == 0 {
                            ready.push(tgt);
                        }
                    }
                }
                continue;
            }
            break;
        }

        order
    }

    /// Loop regions recovered by SCC (DESIGN §7/§13): one [`LoopScc`] per
    /// non-trivial SCC, each tagged with its `LoopMerge` objects.
    ///
    /// A trivial SCC (one object, no self-loop) is not a loop and is excluded.
    pub fn loop_structure(&self, f: FuncId) -> Vec<LoopScc> {
        let mut out = Vec::new();
        for comp in self.sccs(f) {
            let nontrivial = comp.len() > 1 || self.has_self_loop(comp[0]);
            if !nontrivial {
                continue;
            }
            let mut merges: Vec<ObjectId> = comp
                .iter()
                .copied()
                .filter(|&o| self.objects[o].kind == ObjectKind::LoopMerge)
                .collect();
            // Deterministic order for merges (by insertion within the comp).
            merges.sort_by_key(|&o| self.object_seq(o));
            let mut objects = comp;
            objects.sort_by_key(|&o| self.object_seq(o));
            out.push(LoopScc { objects, merges });
        }
        out
    }

    /// The per-merge loop attribution for the loop headed by `merge` in function
    /// `f` (DESIGN §3, BL7). Returns `None` for any non-canonical shape: `merge`
    /// not in a nontrivial SCC, a multi-merge SCC, ≠1 `LoopEnter`/`LoopBack` into
    /// `merge`, or ≠1 `LoopExit` attributed to this merge's SCC.
    ///
    /// **Exit attribution** is by *route-feeder membership* (the S12 rule): a
    /// `LoopExit` belongs to this merge iff its route object is `Pair`-fed by an
    /// object of THIS merge's SCC — never per-function union or reachability,
    /// which mis-attribute a downstream loop's exit to an upstream merge in a
    /// two-sequential-loop function.
    ///
    /// **Decide/advance split** (ADR-0016): the decide cone is every body
    /// morphism whose target is backward-reachable from `exit_route` (cond +
    /// exit-route feeders, incl. exit-feeding effects); the advance cone is the
    /// rest (next-state), unreachable on the exit step.
    pub fn loop_plan(&self, f: FuncId, merge: ObjectId) -> Option<LoopPlan> {
        // Single-merge SCC (M1 canonical shape) — THIS merge's SCC only, never
        // the per-function union.
        let scc = self
            .loop_structure(f)
            .into_iter()
            .find(|s| s.merges.contains(&merge))?;
        if scc.merges.len() != 1 {
            return None;
        }
        let in_scc: SecondaryMap<ObjectId, ()> = {
            let mut m = SecondaryMap::new();
            for &o in &scc.objects {
                m.insert(o, ());
            }
            m
        };

        // init / back_route from the merge's in-edges.
        let mut init = None;
        let mut back_routes: Vec<ObjectId> = Vec::new();
        for &m in self.in_edges(merge) {
            let morph = &self.morphisms[m];
            match morph.op {
                Operation::LoopEnter => {
                    if init.is_some() {
                        return None;
                    }
                    init = Some(morph.source);
                }
                Operation::LoopBack => back_routes.push(morph.source),
                _ => {}
            }
        }
        let init = init?;
        if back_routes.len() != 1 {
            return None;
        }
        let back_route = back_routes[0];

        // exits: LoopExit morphisms whose route object is fed (slot-Pair source)
        // by an in-SCC value — attribute by route-feeder membership.
        let mut exits: Vec<MorphismId> = Vec::new();
        for (mid, morph) in self.morphisms() {
            if self.try_owner(morph.source) != Some(f) {
                continue;
            }
            if morph.op != Operation::LoopExit {
                continue;
            }
            let route = morph.source;
            let fed_by_scc = self.in_edges(route).iter().any(|&pm| {
                let pmo = &self.morphisms[pm];
                matches!(pmo.op, Operation::Pair { .. }) && in_scc.contains_key(pmo.source)
            });
            if fed_by_scc {
                exits.push(mid);
            }
        }
        if exits.len() != 1 {
            return None;
        }
        let exit_route = self.morphisms[exits[0]].source;

        // body_order: topo_order filtered to morphisms incident to SCC(merge) OR
        // feeding a route object, excluding loop ops. Route objects sit outside
        // the SCC, but every Pair edge assembling them must re-fire each iteration
        // — including loop-invariant feeds whose source is outside the SCC — so
        // membership-by-SCC alone would drop them.
        let body_order: Vec<MorphismId> = self
            .topo_order(f)
            .into_iter()
            .filter(|&m| {
                let morph = &self.morphisms[m];
                if matches!(
                    morph.op,
                    Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit
                ) {
                    return false;
                }
                in_scc.contains_key(morph.source)
                    || in_scc.contains_key(morph.target)
                    || morph.target == back_route
                    || morph.target == exit_route
            })
            .collect();

        // Decide set (ADR-0016): objects backward-reachable within body_order's
        // edges from exit_route. Fixpoint over the (small) body.
        let mut in_d: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        in_d.insert(exit_route, ());
        loop {
            let mut changed = false;
            for &m in &body_order {
                let morph = &self.morphisms[m];
                if in_d.contains_key(morph.target) && !in_d.contains_key(morph.source) {
                    in_d.insert(morph.source, ());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut decide_order = Vec::new();
        let mut advance_order = Vec::new();
        for &m in &body_order {
            let morph = &self.morphisms[m];
            if in_d.contains_key(morph.target) {
                decide_order.push(m);
            } else {
                advance_order.push(m);
            }
        }

        // Product targets assembled by Pair edges across the whole body.
        let mut product_targets: Vec<ObjectId> = Vec::new();
        for &m in &body_order {
            let morph = &self.morphisms[m];
            if matches!(morph.op, Operation::Pair { .. })
                && !product_targets.contains(&morph.target)
            {
                product_targets.push(morph.target);
            }
        }

        Some(LoopPlan {
            merge,
            init,
            back_route,
            exits,
            exit_route,
            scc_objects: scc.objects,
            decide_order,
            advance_order,
            product_targets,
        })
    }

    /// Whether `o` has an edge to itself (the only way a 1-object SCC is a loop).
    fn has_self_loop(&self, o: ObjectId) -> bool {
        self.out_edges(o)
            .iter()
            .any(|&m| self.morphisms[m].target == o)
    }

    /// A stable sequence number for an object (its position in insertion order),
    /// used to make `loop_structure` output order independent of Tarjan's
    /// pop order.
    fn object_seq(&self, o: ObjectId) -> usize {
        self.objects
            .iter()
            .position(|(id, _)| id == o)
            .unwrap_or(usize::MAX)
    }
}
