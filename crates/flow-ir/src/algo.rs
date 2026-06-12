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

        // Process ready objects in a FIFO worklist; ties broken by the order
        // objects were discovered, which is insertion order.
        let mut cursor = 0;
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
