//! Graph algorithms (DESIGN §13): iterative Tarjan SCC, Kahn topological order,
//! and the `loop_structure` backend predicate.
//!
//! Everything here is `O(V + E)` and **recursion-free** (J1): Tarjan uses an
//! explicit work stack, Kahn is a worklist. The §16 deep-graph test (100k-object
//! chain) exercises the no-stack-overflow guarantee.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use slotmap::SecondaryMap;

use crate::graph::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation};
use crate::ty::{Ty, Value, ty_contains_token};

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

/// The per-fn bounds-proof plan (the deduced query
/// `bounds_proof : IR × FuncId → BoundsProof` — the BL7 pattern alongside
/// [`loop_plan`] / [`last_use_plan`]). Interval analysis over the fn's object
/// graph, answering "is this `Index` provably in `[0, n)`?" so the backends
/// can drop dead trap guards in counted loops (the S20 kernel-gap finding:
/// guards inside the map/fold inner loops block unrolling/vectorization).
/// Conservative by construction: any unknown/wraparound/unsupported shape is
/// NOT proven, so today's guards stay (zero behavior change where unproven).
#[derive(Clone, Debug, PartialEq)]
pub struct BoundsProof {
    /// The `Operation::Index` morphisms proven statically in-bounds.
    proven: SecondaryMap<MorphismId, ()>,
}

impl BoundsProof {
    /// Whether `m` (an `Operation::Index` morphism) is provably in-bounds.
    pub fn proven(&self, m: MorphismId) -> bool {
        self.proven.contains_key(m)
    }
}

/// A [`PathPlan`] task identifier. Task ids are indices into
/// [`PathPlan::tasks`].
pub type TaskId = usize;

/// One schedulable unit in a function's path plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// Whether the task is element-range splittable or sequential.
    pub kind: TaskKind,
    /// Tasks whose produced objects this task consumes.
    pub deps: Vec<TaskId>,
    /// Saturating critical-path weight to a sink.
    pub rank: u32,
    /// Earliest topo position of a trap-capable morphism in this task.
    pub trap_min: Option<u32>,
    /// Whether a trap-capable pure named call pins this task to the host spine.
    pub pinned: bool,
}

/// The execution shape of a [`Task`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// One element-range-splittable bulk operation.
    Split {
        /// The bulk morphism.
        site: MorphismId,
        /// Its statically known element count.
        n: u64,
    },
    /// Morphisms that execute sequentially, in topo order.
    Seq {
        /// The task's morphisms.
        morphisms: Vec<MorphismId>,
    },
}

/// One task-progress requirement at a host-spine checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitEntry {
    /// The task whose progress gates the checkpoint.
    pub task: TaskId,
    /// `Some(w)` accepts a decided watermark ≥ `w` or task completion;
    /// `None` requires task completion.
    pub threshold: Option<u32>,
}

/// A host-spine observation point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// The token operation's topo position, or `u32::MAX` for function exit.
    pub topo: u32,
    /// Task progress required before the host may pass this point.
    pub wait: Vec<WaitEntry>,
}

/// The deterministic task DAG and host-spine checkpoints for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathPlan {
    /// Schedulable pure tasks. Task ids are vector indices.
    pub tasks: Vec<Task>,
    /// Token-operation checkpoints in topo order, followed by function exit.
    pub checkpoints: Vec<Checkpoint>,
}

/// One loop's complete driver region (plan-s40): SCC-incident morphisms,
/// machinery, and the `loop_plan` cones — exactly the set the flat walk hands
/// the driver. Private to [`CategoryIr::guard_plan`]: an arm owns a unit
/// atomically, represented in its own-list by the `LoopEnter` handle(s) alone;
/// internals never enter an own-list (the driver fires them).
struct LoopUnit {
    members: SecondaryMap<MorphismId, ()>,
    /// The `LoopEnter` handle(s) — what stands for the unit in an own-list.
    enters: Vec<MorphismId>,
    /// Drivable: single merge with a `loop_plan`, machinery unshared. A
    /// non-canonical unit never joins an arm (it runs unconditionally, which
    /// is always safe) — and `LiftLoops` consumes `loop_plan` facts too, so a
    /// shape the driver cannot run is a shape the rewriter cannot lift:
    /// refused raw ⇒ refused rewritten, no stability hole.
    canonical: bool,
    /// Any member trap-capable — the handle carries the whole unit's flag.
    can_trap: bool,
}

/// One arm of a Phi-position guard (plan-s39). `own` is the arm's exclusive
/// work: the morphisms whose every path to an observable root passes through
/// this arm's `Pair` edge — they fire only when the condition picks this arm.
///
/// Ownership is DIRECT: a nested guard's own morphisms belong to the nested
/// site (whose `phi` stays in this list as the gate that fires it), so the
/// own-lists of all sites partition the gated morphisms — each has exactly one
/// innermost owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardArm {
    /// The arm's value object (the triple's `Pair` slot 0/1 feeder).
    pub value: ObjectId,
    /// The boundary `Pair` edge writing `value` into the Phi triple.
    pub edge: MorphismId,
    /// Direct exclusive work in topo order; `edge` is last. Empty ⇒ the value
    /// is computed unconditionally (a parameter, constant, or shared object)
    /// and there is nothing to gate.
    pub own: Vec<MorphismId>,
    /// Whether the arm's TRANSITIVE exclusive work (this list plus any nested
    /// site's) holds a trap-capable morphism: integer `Div`/`Mod`, unproven
    /// `Index`, `Update`, or a `Call`/`Map`/`Fold` whose body is trap-capable.
    /// An ungated realization (today's `select`) is sound only when false.
    pub can_trap: bool,
    /// Whether the arm's exclusive work is worth skipping: it holds a bulk
    /// operation or a call. Scalar arithmetic is not — computing `x * -1`
    /// costs less than branching around it, and inside a map body a branch
    /// would cost the whole element loop its vectorization.
    pub heavy: bool,
}

/// A Phi-position guard site (plan-s39): the condition and the two arms'
/// exclusive work. An arm's work runs only if the condition picks that arm;
/// a Phi the query does not report (a non-builder triple shape, or an arm
/// reaching loop machinery) keeps today's strict both-arms semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardSite {
    /// The `Phi` morphism.
    pub phi: MorphismId,
    /// The `Bool` condition object (the triple's `Pair` slot 2 feeder).
    pub cond: ObjectId,
    pub on_true: GuardArm,
    pub on_false: GuardArm,
}

impl GuardSite {
    /// Whether consumers must gate this site. Legality first: a trap-capable
    /// arm can never be evaluated ungated, and no cost argument buys it. Then
    /// cost: a `heavy` arm is worth branching around. Everything else — two
    /// arms of scalar arithmetic — stays ungated, because for a total arm of a
    /// few register ops, gating and computing are the same work and the branch
    /// is the more expensive of the two. Every arm's own-list ends with its
    /// boundary `Pair` edge, so an "empty" arm is never literally empty; this
    /// predicate, not emptiness, is the test.
    pub fn gated(&self) -> bool {
        self.on_true.can_trap || self.on_false.can_trap || self.on_true.heavy || self.on_false.heavy
    }
}

/// Map/fold sites whose iteration space may be tiled without changing any
/// per-cell operation order.
#[derive(Clone, Debug, PartialEq)]
pub struct TilePlan {
    /// Recognized `Operation::Map` morphisms and their complete emission data.
    pub sites: SecondaryMap<MorphismId, TileSite>,
}

/// One affine array read in a tiled map/fold site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRead {
    pub slot: u32,
    pub base: u64,
    pub ci: u64,
    pub ck: u64,
    pub clane: u64,
    /// The `k÷div`/`k%div` decomposition when this read's address is affine in
    /// the fold's derived axes instead of raw `k` (`ck == 0` then). `None` on
    /// plain affine reads.
    pub ksplit: Option<TileKSplit>,
}

/// The `k = kq·div + kr` decomposition of a fold's counted axis: the read's
/// address is affine in the derived variables `k÷div` (coefficient `cq`) and
/// `k%div` (coefficient `cr`) — conv2d's `k/3`,`k%3` window taps. The same
/// derived-var move the map body gets for `(t÷C, t%C)`, one level down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileKSplit {
    /// The literal divisor shared by the fold body's `Div`/`Mod` pair.
    pub div: u64,
    /// Address coefficient on `k÷div`.
    pub cq: u64,
    /// Address coefficient on `k%div`.
    pub cr: u64,
}

/// Affine address coefficients over the tile axes:
/// `base + ci·i + clane·lane + ck·k + cq·(k÷div) + cr·(k%div)`.
#[derive(Clone, Copy, Default)]
struct TileAffine {
    base: u64,
    ci: u64,
    clane: u64,
    ck: u64,
    cq: u64,
    cr: u64,
}

impl TileAffine {
    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            base: self.base.checked_add(other.base)?,
            ci: self.ci.checked_add(other.ci)?,
            clane: self.clane.checked_add(other.clane)?,
            ck: self.ck.checked_add(other.ck)?,
            cq: self.cq.checked_add(other.cq)?,
            cr: self.cr.checked_add(other.cr)?,
        })
    }

    fn scale(self, by: u64) -> Option<Self> {
        Some(Self {
            base: self.base.checked_mul(by)?,
            ci: self.ci.checked_mul(by)?,
            clane: self.clane.checked_mul(by)?,
            ck: self.ck.checked_mul(by)?,
            cq: self.cq.checked_mul(by)?,
            cr: self.cr.checked_mul(by)?,
        })
    }
}

/// Everything a backend needs to emit one recognized tiled map site.
#[derive(Clone, Debug, PartialEq)]
pub struct TileSite {
    pub rows: u64,
    pub c: u64,
    pub k: u64,
    pub a: TileRead,
    pub b: TileRead,
    pub seed: Value,
    pub elem: Ty,
    pub mul_a_first: bool,
    pub add_acc_first: bool,
}

impl PathPlan {
    /// Whether scheduling can be skipped entirely.
    pub fn is_single_path(&self) -> bool {
        self.tasks.len() <= 1
            && matches!(
                self.checkpoints.as_slice(),
                [Checkpoint { topo: u32::MAX, wait }]
                    if wait.len() == self.tasks.len()
                        && wait.iter().enumerate().all(|(task, entry)|
                            entry.task == task && entry.threshold.is_none())
            )
    }
}

/// How an owned, non-constant, non-token object participates in minimal
/// emission. Derived only from the sealed graph by
/// [`CategoryIr::emission_plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmissionClass {
    /// A structural product whose fields flow directly to primitive operands
    /// or projections; the product itself never materializes.
    Dissolved,
    /// A pure, guard-free value with exactly one effective consumer.
    Inline,
    /// A boundary, guarded producer, or value without exactly one consumer.
    Named,
}

impl EmissionClass {
    /// Whether this is [`Self::Dissolved`].
    pub fn is_dissolved(self) -> bool {
        self == Self::Dissolved
    }

    /// Whether this is [`Self::Inline`].
    pub fn is_inline(self) -> bool {
        self == Self::Inline
    }

    /// Whether this is [`Self::Named`].
    pub fn is_named(self) -> bool {
        self == Self::Named
    }
}

/// The minimal-emission classification for one function. Constants and
/// token-carrying objects are absent because neither materializes.
#[derive(Clone, Debug, PartialEq)]
pub struct EmissionPlan {
    class: SecondaryMap<ObjectId, EmissionClass>,
}

impl EmissionPlan {
    /// The object's class, or `None` when it is not owned by the queried
    /// function, is a constant, or carries a token.
    pub fn class(&self, o: ObjectId) -> Option<EmissionClass> {
        self.class.get(o).copied()
    }
}

/// A value range in the unsigned lattice (negative-capable shapes bail to
/// unknown — wrapping ints mean a `Sub` past zero invalidates intervals).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rng {
    /// A value in `[lo, hi]`.
    Int(u64, u64),
    /// A `(i32, X)` enumerate element over `n`: its `.0` ∈ `[0, n)`.
    EnumIdx(u64),
}

/// The per-element law of an array object: **what `out[i]` is, as a closed
/// function of the loop index** (plan-s37-stage-structure §3).
///
/// An array whose law is known does not have to exist. `iota` writes a million
/// `i32`s so a consuming `map` can read them back, but `ix[i]` *is* `i` — the
/// array is a stored copy of a deduced morphism (FRAMEWORK §5), paid for with a
/// store pass, a load pass, and an opaque memory object LLVM refuses to
/// vectorize through (`cannot identify array bounds`).
///
/// This type says only what the element **is**. Whether to use that instead of
/// loading — recompute vs. keep in memory, and whether inlining blows a cache,
/// register or occupancy budget — is a per-target question the **backend** owns:
/// on a GPU bandwidth is scarce and registers are cheap, on a CPU it is a body's
/// cycle count against an L2 round trip, on an FPGA it is BRAM against silicon
/// area. Same graph, three answers, so there is deliberately no cost, op-count
/// or read-count field here (ADR-0032: mapal-ir never learns a machine fact).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElemSrc {
    /// `out[i] = i` — the loop index itself (`Iota`).
    Index,
    /// `out[i] = x` for a loop-invariant `x` (`Fill`): component `slot` of
    /// `source`, which the emitter already loads once above the loop head.
    Broadcast { source: ObjectId, slot: u32 },
    /// **The cut.** No closed form, so the array materializes and the element is
    /// loaded — exactly today's behavior, which is why cutting is always sound.
    /// `(source, slot)` is the pair `array_operand_ptr` / `load_component`
    /// already take, so consuming this needs no new pointer plumbing.
    Load { source: ObjectId, slot: Option<u32> },
    /// Structural pairing with no arithmetic of its own: `Zip` is
    /// `Pair(·, ·)` and `Enumerate` is `Pair(Index, ·)` — which is why
    /// `Enumerate` needs no constructor here (ADR-0018 already calls `Zip` the
    /// canonical iso `Aⁿ × Bⁿ ≅ (A×B)ⁿ`, and `enumerate a ≅ zip(iota n, a)`).
    Pair(Box<ElemSrc>, Box<ElemSrc>),
    /// `out[i] = body(inner(i))` — a `Map` whose body is **classifiable**.
    ///
    /// The other four producers are bodyless: their op tag is the entire
    /// formula, zero degrees of freedom, total and trap-free by construction.
    /// `Map` is not their peer — it carries an arbitrary `FuncId` that may
    /// trap, diverge, or print. So it enters the family only behind a gate
    /// (see [`CategoryIr::elem_law`]): the body must be trap-free, loop-free
    /// and effect-free, because a consumer that recomputes it would otherwise
    /// be free to delete a trap, turn `Trapped` into `Diverged`, or reorder an
    /// observable — the S34 failure class one level up.
    ///
    /// Capture identity is deliberately **not** required: nothing is spliced.
    /// The consumer emits a call to the same body, so this is two calls in one
    /// loop rather than a merged body.
    Apply {
        body: FuncId,
        source: ObjectId,
        captures: u32,
        inner: Box<ElemSrc>,
        /// The producer's own materialized output array. A consumer that
        /// DECLINES to recompute (profitability is the backend's call) reads
        /// this instead — so a declined sub-law degrades to a load rather than
        /// poisoning the law that contains it. Without it, refusing an `Apply`
        /// nested inside a `Pair` would collapse the whole pair back to an
        /// array-of-structs read, silently undoing the `Zip` win.
        array: ObjectId,
    },
}

/// The per-fn element-law plan — the deduced query
/// `elem : IR × FuncId → ElemPlan`, the BL7 pattern alongside [`tile_plan`] and
/// [`last_use_plan`] (plan-s37-stage-structure).
///
/// **Only non-[`ElemSrc::Load`] laws are recorded.** Absence means "load it",
/// which is the status quo — so, like every query here, saying nothing is always
/// safe and a consumer that ignores the plan entirely is still correct.
///
/// Non-destructive by design: this records that an intermediate array *could* be
/// skipped and never removes it. The graph, the count `Constant` that
/// `validate`'s `IotaCountMismatch` ties, and every downstream recognizer's
/// witness all survive — which is what leaves the elide-vs-materialize decision
/// with the backend, and what keeps a deliberate materialization (S27 rung-3
/// packing) expressible.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElemPlan {
    src: SecondaryMap<ObjectId, ElemSrc>,
}

impl ElemPlan {
    /// The element law of `arr`, or `None` when it must be loaded.
    pub fn src(&self, arr: ObjectId) -> Option<&ElemSrc> {
        self.src.get(arr)
    }

    /// How many arrays in the fn have a closed-form element law.
    pub fn len(&self) -> usize {
        self.src.len()
    }

    /// Whether no array in the fn has one.
    pub fn is_empty(&self) -> bool {
        self.src.is_empty()
    }
}

/// The per-fn last-use plan (docs/components/ir/plans/plan-last-use.md §2 —
/// the deduced query `last_use : IR × FuncId → LastUsePlan`, the BL7 pattern
/// alongside [`loop_plan`]): per-object death positions plus the escape and
/// loop-carried classifications the backend consumers (in-place `Update`,
/// back-edge freeing, arena coloring) read through [`LastUsePlan::dead_after`]
/// and the accessors. Total and deterministic on any sealed fn (rules 5-6):
/// non-canonical loops contribute no carried set and no position adjustment,
/// so consumers fall back to today's behavior on `None`-shaped answers.

#[derive(Clone, Debug, PartialEq)]
pub struct LastUsePlan {
    /// Rule 1's oracle-order position per morphism: `topo_order`, with every
    /// canonical loop's morphisms re-ranked **decide < `LoopExit` < advance <
    /// `LoopBack`** within the topo slots they already occupied (the
    /// guard-first quartet's execution order, ADR-0016; a global permutation —
    /// non-loop morphisms keep their topo positions).
    position: SecondaryMap<MorphismId, u32>,
    /// The greatest adjusted position of any use of the object, counting a
    /// `Pair`/`Phi` edge as a use at the product's own last-use position —
    /// rule 2's retention "through Pair fields and Phi arms" applied to
    /// liveness: a packed handle lives as long as the product holding it.
    /// Absent for objects with no uses.
    use_pos: SecondaryMap<ObjectId, u32>,
    /// Rule 2's escape set (conservative): every `Parameter`, plus anything
    /// reaching the fn's output — but for the loop's own carried state (the
    /// merge, its `Proj` views, the back-route `Pair` cone) the path through
    /// that loop's own `LoopExit` does NOT count (the per-iteration release
    /// valve: the escaping final instance is protected via the exit OBJECT,
    /// which is not exempt). Every other escape path (a different loop's
    /// exit excepted only when it, too, is the carrier's own) counts.
    escapes: SecondaryMap<ObjectId, bool>,
    /// `carried_by` (rule 3): object → the `LoopMerge` its value crosses a
    /// `LoopBack` into — the back-route `Pair` cone, loop-body objects only
    /// (a loop-invariant route feeder is not carried).
    carried: SecondaryMap<ObjectId, ObjectId>,
}

impl LastUsePlan {
    /// The morphism's adjusted position (rule 1's oracle-order ranking).
    /// `Some` for every morphism of the fn.
    pub fn position(&self, m: MorphismId) -> Option<u32> {
        self.position.get(m).copied()
    }

    /// `death` (plan §2): the greatest adjusted position of any use of `o`.
    /// `None` (⊥) for escaping objects (rule 2 — never dead inside the fn),
    /// and also `None` for objects with no uses (no use position exists; such
    /// an object is dead everywhere after its definition — [`Self::dead_after`]
    /// answers `true` for it).
    pub fn death(&self, o: ObjectId) -> Option<u32> {
        if self.escapes(o) {
            return None;
        }
        self.use_pos.get(o).copied()
    }

    /// `escapes` (rule 2): may this object's value outlive the fn (reachable
    /// into `Output`/`Return` incl. through `Pair` fields and `Phi` arms, a
    /// borrowed `Parameter`, or an outer fn's capture source — the last rides
    /// the body fn's own `Parameter` seed, per-fn)? Escaping objects are
    /// never freed and never written in place.
    pub fn escapes(&self, o: ObjectId) -> bool {
        self.escapes.get(o).copied().unwrap_or(false)
    }

    /// `carried_by` (rule 3): the `LoopMerge` this object's value crosses a
    /// `LoopBack` into — it lives "into the next iteration" (two-iteration
    /// liveness). `None` for everything else, the merge itself included.
    pub fn carried_by(&self, o: ObjectId) -> Option<ObjectId> {
        self.carried.get(o).copied()
    }

    /// `dead_after` (rule 4's consumer predicate): all uses of `o` sit at or
    /// before `idx` (rule 1's ranking, retention pins included) AND `o` does
    /// not escape AND `o` is not loop-carried. `idx` is typically
    /// [`Self::position`] of the writing morphism (e.g. the `Update` whose
    /// source is `o`). Rule 4's remaining half is the consumer's: a loop's
    /// borrowed init (`escapes(init)` — rule 2) is never written in place.
    pub fn dead_after(&self, o: ObjectId, idx: u32) -> bool {
        !self.escapes(o)
            && self.carried.get(o).is_none()
            && self.use_pos.get(o).copied().is_none_or(|p| p <= idx)
    }
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

    /// The per-function minimal-emission query. Classification is total over
    /// owned, non-constant, non-token objects and is a pure function of the
    /// sealed graph.
    ///
    /// Products are dissolved first. Consumer counts are then taken through
    /// those transparent products, so a field used by two consumers is named
    /// once rather than duplicated when its product wrapper disappears.
    pub fn emission_plan(&self, f: FuncId) -> EmissionPlan {
        let bounds = self.bounds_proof(f);
        let mut boundary: SecondaryMap<ObjectId, ()> = SecondaryMap::new();

        if let Some(fd) = self.func(f) {
            boundary.insert(fd.output, ());
        }
        for &o in &self.func_objects(f) {
            let obj = &self.objects[o];
            if obj.kind == ObjectKind::LoopMerge || matches!(obj.ty, Ty::Array { .. }) {
                boundary.insert(o, ());
            }
        }

        for (_, morph) in self.morphisms() {
            if self.try_owner(morph.source) != Some(f) {
                continue;
            }
            if self.emission_guarded(morph.id, &bounds) {
                boundary.insert(morph.target, ());
            }
            match morph.op {
                Operation::Output => {
                    boundary.insert(morph.source, ());
                    boundary.insert(morph.target, ());
                }
                Operation::Call(_) => {
                    boundary.insert(morph.source, ());
                }
                Operation::Map { .. }
                | Operation::Fold { .. }
                | Operation::Zip
                | Operation::Enumerate
                | Operation::Update
                | Operation::Iota
                | Operation::Fill
                    if self.objects[morph.source].ty.product_arity().is_some() =>
                {
                    boundary.insert(morph.source, ());
                }
                Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
                    boundary.insert(morph.source, ());
                    boundary.insert(morph.target, ());
                }
                _ => {}
            }
        }

        // Every object incident to a canonical loop's decide/advance cone is a
        // statement point. Non-canonical loop endpoints were covered above.
        for scc in self.loop_structure(f) {
            for merge in scc.merges {
                if let Some(plan) = self.loop_plan(f, merge) {
                    for &m in plan.decide_order.iter().chain(&plan.advance_order) {
                        let morph = &self.morphisms[m];
                        boundary.insert(morph.source, ());
                        boundary.insert(morph.target, ());
                    }
                }
            }
        }

        let mut dissolved: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for &o in &self.func_objects(f) {
            let obj = &self.objects[o];
            let Some(arity) = obj.ty.product_arity_u32() else {
                continue;
            };
            if obj.kind == ObjectKind::Constant
                || ty_contains_token(&obj.ty)
                || boundary.contains_key(o)
            {
                continue;
            }
            // Dissolution requires the product be FULLY Pair-built: the
            // redistribution below resolves fields via `pair_slot_source`,
            // which is `None` for a Proj-produced tuple — dissolving one
            // would silently drop its consumers from the counts, letting a
            // shared field chain classify Inline and duplicate textually
            // (an R-NODUP break). Proj-produced products stay countable.
            if (0..arity).all(|i| self.pair_slot_source(o, i).is_some())
                && self.out_edges(o).iter().all(|&m| {
                    matches!(self.morphisms[m].op, Operation::Proj { .. })
                        || is_pair_primitive(self.morphisms[m].op)
                })
            {
                dissolved.insert(o, ());
            }
        }

        // Ordinary edges count directly, except Pair edges into transparent
        // products. Those are replaced below by the product's actual field
        // uses: all fields for a primitive, one field for Proj.
        let mut consumers: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
        for &o in &self.func_objects(f) {
            let obj = &self.objects[o];
            if obj.kind == ObjectKind::Constant
                || ty_contains_token(&obj.ty)
                || dissolved.contains_key(o)
            {
                continue;
            }
            let count = self
                .out_edges(o)
                .iter()
                .filter(|&&m| {
                    !matches!(self.morphisms[m].op, Operation::Pair { .. })
                        || !dissolved.contains_key(self.morphisms[m].target)
                })
                .count() as u32;
            consumers.insert(o, count);
        }
        for (product, _) in dissolved.iter() {
            for &m in self.out_edges(product) {
                match self.morphisms[m].op {
                    Operation::Proj { index } => {
                        if let Some(source) = self.pair_slot_source(product, index) {
                            increment(&mut consumers, source);
                        }
                    }
                    op if is_pair_primitive(op) => {
                        for &pair in self.in_edges(product) {
                            if matches!(self.morphisms[pair].op, Operation::Pair { .. }) {
                                increment(&mut consumers, self.morphisms[pair].source);
                            }
                        }
                    }
                    _ => unreachable!("dissolved products have only transparent consumers"),
                }
            }
        }

        let mut class = SecondaryMap::new();
        for &o in &self.func_objects(f) {
            let obj = &self.objects[o];
            if obj.kind == ObjectKind::Constant || ty_contains_token(&obj.ty) {
                continue;
            }
            let c = if dissolved.contains_key(o) {
                EmissionClass::Dissolved
            } else if boundary.contains_key(o) || consumers.get(o).copied().unwrap_or(0) != 1 {
                EmissionClass::Named
            } else {
                EmissionClass::Inline
            };
            class.insert(o, c);
        }
        EmissionPlan { class }
    }

    /// Whether the producer needs statement-form guards.
    fn emission_guarded(&self, m: MorphismId, bounds: &BoundsProof) -> bool {
        let morph = &self.morphisms[m];
        match morph.op {
            Operation::Div | Operation::Mod => {
                if matches!(self.objects[morph.target].ty, Ty::Float { .. }) {
                    return false;
                }
                self.pair_slot_source(morph.source, 1)
                    .is_none_or(|o| !safe_integer_divisor(&self.objects[o]))
            }
            Operation::Index | Operation::Update => !bounds.proven(m),
            _ => false,
        }
    }

    /// Recognize affine map/fold sites whose cell chains may be interleaved
    /// without changing their operation or operand order.
    pub fn tile_plan(&self, f: FuncId) -> TilePlan {
        let mut sites = SecondaryMap::new();
        let Some(fd) = self.func(f) else {
            return TilePlan { sites };
        };
        for &m in &fd.morphisms {
            if let Some(site) = self.tile_site(m) {
                sites.insert(m, site);
            }
        }
        TilePlan { sites }
    }

    fn tile_site(&self, site: MorphismId) -> Option<TileSite> {
        let map = self.morphisms.get(site)?;
        let Operation::Map {
            body,
            captures: map_captures,
        } = map.op
        else {
            return None;
        };
        if map_captures == 0 {
            return None;
        }

        let mapped = self.pair_slot_source(map.source, map_captures)?;
        let mapped_size = self.tile_iota_size(mapped)?;
        let Ty::Array {
            size: target_size, ..
        } = &self.objects.get(map.target)?.ty
        else {
            return None;
        };
        if *target_size != mapped_size {
            return None;
        }

        let body_def = self.func(body)?;
        let mut fold_id = None;
        for &m in &body_def.morphisms {
            let morph = self.morphisms.get(m)?;
            if matches!(morph.op, Operation::Fold { .. }) {
                if fold_id.replace(m).is_some() {
                    return None;
                }
            }
        }
        let fold_id = fold_id?;
        let fold = self.morphisms.get(fold_id)?;
        if fold.target != body_def.output {
            return None;
        }
        let Operation::Fold {
            body: fold_body,
            captures: fold_captures,
        } = fold.op
        else {
            return None;
        };
        let depth = self
            .tile_iota_size(self.pair_slot_source(fold.source, fold_captures.checked_add(1)?)?)?;
        let seed_object = self
            .objects
            .get(self.pair_slot_source(fold.source, fold_captures)?)?;
        if seed_object.kind != ObjectKind::Constant {
            return None;
        }
        let seed = seed_object.value.clone()?;
        let body_bounds = self.bounds_proof(body);
        let fold_bounds = self.bounds_proof(fold_body);
        if !self.tile_trap_free(body, &body_bounds, Some(fold_id))
            || !self.tile_trap_free(fold_body, &fold_bounds, None)
        {
            return None;
        }

        let mut has_split = false;
        for &div_id in &body_def.morphisms {
            let Some((i, c)) =
                self.tile_split(div_id, body_def.input, map_captures, Operation::Div)
            else {
                continue;
            };
            for &mod_id in &body_def.morphisms {
                let Some((j, mod_c)) =
                    self.tile_split(mod_id, body_def.input, map_captures, Operation::Mod)
                else {
                    continue;
                };
                if mod_c != c {
                    continue;
                }
                has_split = true;
                if mapped_size % c != 0 {
                    continue;
                }
                let rows = mapped_size / c;
                if rows.checked_mul(c) != Some(mapped_size) {
                    continue;
                }
                let Some(site) = self.tile_fold_shape(
                    map.target,
                    map_captures,
                    body_def.input,
                    fold,
                    Some(i),
                    j,
                    rows,
                    c,
                    depth,
                    seed.clone(),
                ) else {
                    continue;
                };
                return Some(site);
            }
        }
        if has_split {
            return None;
        }
        if mapped_size == 0 {
            return None;
        }

        let lane = body_def.morphisms.iter().find_map(|m| {
            let morph = self.morphisms.get(*m)?;
            (self.tile_input_proj_index(morph.target, body_def.input) == Some(map_captures))
                .then_some(morph.target)
        })?;
        self.tile_fold_shape(
            map.target,
            map_captures,
            body_def.input,
            fold,
            None,
            lane,
            1,
            mapped_size,
            depth,
            seed,
        )
    }

    fn tile_split(
        &self,
        m: MorphismId,
        body_input: ObjectId,
        element_slot: u32,
        op: Operation,
    ) -> Option<(ObjectId, u64)> {
        let morph = self.morphisms.get(m)?;
        if morph.op != op {
            return None;
        }
        let element = self.pair_slot_source(morph.source, 0)?;
        if self.tile_input_proj_index(element, body_input) != Some(element_slot) {
            return None;
        }
        let c = self.tile_literal_u64(self.pair_slot_source(morph.source, 1)?)?;
        (c != 0).then_some((morph.target, c))
    }

    #[allow(clippy::too_many_arguments)]
    fn tile_fold_shape(
        &self,
        map_target: ObjectId,
        map_captures: u32,
        body_input: ObjectId,
        fold: &crate::graph::Morphism,
        i: Option<ObjectId>,
        lane: ObjectId,
        rows: u64,
        c: u64,
        k: u64,
        seed: Value,
    ) -> Option<TileSite> {
        let Operation::Fold {
            body,
            captures: fold_captures,
        } = fold.op
        else {
            return None;
        };
        let fold_def = self.func(body)?;
        // Fold-body analog of the map-body split (`tile_site`): bind a
        // `Div`/`Mod` pair on the fold's counted element to the derived axes
        // `(k÷div, k%div)` — conv2d's window taps. Rectangular windows only
        // (`k % div == 0`); a pair that fails this simply stays unbound and
        // the walker's `_ => None` arm refuses the site.
        let element_slot = fold_captures + 1;
        let ksplit_axes = fold_def.morphisms.iter().find_map(|&div_id| {
            let (kq, div) =
                self.tile_split(div_id, fold_def.input, element_slot, Operation::Div)?;
            fold_def.morphisms.iter().find_map(|&mod_id| {
                let (kr, mod_div) =
                    self.tile_split(mod_id, fold_def.input, element_slot, Operation::Mod)?;
                (mod_div == div && k % div == 0).then_some((kq, kr, div))
            })
        });
        let add = self.tile_definer(fold_def.output, Operation::Add)?;
        let add_lhs = self.pair_slot_source(add.source, 0)?;
        let add_rhs = self.pair_slot_source(add.source, 1)?;
        let lhs_acc = self.tile_input_proj_index(add_lhs, fold_def.input) == Some(fold_captures);
        let rhs_acc = self.tile_input_proj_index(add_rhs, fold_def.input) == Some(fold_captures);
        let (acc, product, add_acc_first) = match (lhs_acc, rhs_acc) {
            (true, false) => (add_lhs, add_rhs, true),
            (false, true) => (add_rhs, add_lhs, false),
            _ => return None,
        };

        let mul = self.tile_definer(product, Operation::Mul)?;
        let load0 = self.pair_slot_source(mul.source, 0)?;
        let load1 = self.pair_slot_source(mul.source, 1)?;
        let index_parts = |value: ObjectId| -> Option<(TileRead, Ty)> {
            let index = self.tile_definer(value, Operation::Index)?;
            let array = self.pair_slot_source(index.source, 0)?;
            let address = self.pair_slot_source(index.source, 1)?;
            let body_value =
                self.tile_fold_capture(array, fold.source, fold_def.input, fold_captures)?;
            let slot = self.tile_input_proj_index(body_value, body_input)?;
            if slot >= map_captures {
                return None;
            }
            let Ty::Array { elem, .. } = &self.objects.get(array)?.ty else {
                return None;
            };
            if self.objects.get(value)?.ty != **elem {
                return None;
            }
            let aff = self.tile_affine(
                address,
                fold.source,
                fold_def.input,
                fold_captures,
                i,
                lane,
                ksplit_axes.map(|(kq, kr, _)| (kq, kr)),
            )?;
            let ksplit = match ksplit_axes {
                Some((_, _, div)) if aff.cq != 0 || aff.cr != 0 => {
                    // A read is affine in raw `k` XOR in the derived axes —
                    // never mixed.
                    if aff.ck != 0 {
                        return None;
                    }
                    Some(TileKSplit {
                        div,
                        cq: aff.cq,
                        cr: aff.cr,
                    })
                }
                _ => None,
            };
            Some((
                TileRead {
                    slot,
                    base: aff.base,
                    ci: aff.ci,
                    ck: aff.ck,
                    clane: aff.clane,
                    ksplit,
                },
                (**elem).clone(),
            ))
        };
        let (read0, elem0) = index_parts(load0)?;
        let (read1, elem1) = index_parts(load1)?;

        if elem0 != elem1 {
            return None;
        }
        let (a, b, mul_a_first) = match (read0.clane, read1.clane) {
            (0, 1) => (read0, read1, true),
            (1, 0) => (read1, read0, false),
            _ => return None,
        };

        let acc_ty = self.objects.get(acc)?.ty.clone();
        let Ty::Array { elem: map_elem, .. } = &self.objects.get(map_target)?.ty else {
            return None;
        };
        if elem0 != acc_ty || elem0 != **map_elem || seed.ty() != elem0 || !elem0.is_numeric() {
            return None;
        }
        Some(TileSite {
            rows,
            c,
            k,
            a,
            b,
            seed,
            elem: elem0,
            mul_a_first,
            add_acc_first,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn tile_affine(
        &self,
        object: ObjectId,
        fold_source: ObjectId,
        fold_input: ObjectId,
        fold_captures: u32,
        i: Option<ObjectId>,
        lane: ObjectId,
        ksplit_axes: Option<(ObjectId, ObjectId)>,
    ) -> Option<TileAffine> {
        if let Some(captured) =
            self.tile_fold_capture(object, fold_source, fold_input, fold_captures)
        {
            if i == Some(captured) {
                return Some(TileAffine {
                    ci: 1,
                    ..TileAffine::default()
                });
            }
            if captured == lane {
                return Some(TileAffine {
                    clane: 1,
                    ..TileAffine::default()
                });
            }
        }
        if let Some((kq, kr)) = ksplit_axes {
            if object == kq {
                return Some(TileAffine {
                    cq: 1,
                    ..TileAffine::default()
                });
            }
            if object == kr {
                return Some(TileAffine {
                    cr: 1,
                    ..TileAffine::default()
                });
            }
        }
        if self.tile_input_proj_index(object, fold_input) == fold_captures.checked_add(1) {
            return Some(TileAffine {
                ck: 1,
                ..TileAffine::default()
            });
        }
        if let Some(base) = self.tile_literal_u64(object) {
            return Some(TileAffine {
                base,
                ..TileAffine::default()
            });
        }

        let [m] = self.in_edges(object) else {
            return None;
        };
        let morph = self.morphisms.get(*m)?;
        let lhs = self.pair_slot_source(morph.source, 0)?;
        let rhs = self.pair_slot_source(morph.source, 1)?;
        match morph.op {
            Operation::Add => {
                let x = self.tile_affine(
                    lhs,
                    fold_source,
                    fold_input,
                    fold_captures,
                    i,
                    lane,
                    ksplit_axes,
                )?;
                let y = self.tile_affine(
                    rhs,
                    fold_source,
                    fold_input,
                    fold_captures,
                    i,
                    lane,
                    ksplit_axes,
                )?;
                x.add(y)
            }
            Operation::Mul => {
                let (scale, value) = if let Some(scale) = self.tile_literal_u64(lhs) {
                    (scale, rhs)
                } else {
                    (self.tile_literal_u64(rhs)?, lhs)
                };
                let value = self.tile_affine(
                    value,
                    fold_source,
                    fold_input,
                    fold_captures,
                    i,
                    lane,
                    ksplit_axes,
                )?;
                value.scale(scale)
            }
            _ => None,
        }
    }

    fn tile_definer(&self, object: ObjectId, op: Operation) -> Option<&crate::graph::Morphism> {
        let [m] = self.in_edges(object) else {
            return None;
        };
        let morph = self.morphisms.get(*m)?;
        (morph.op == op).then_some(morph)
    }

    fn tile_input_proj_index(&self, object: ObjectId, input: ObjectId) -> Option<u32> {
        let [m] = self.in_edges(object) else {
            return None;
        };
        let proj = self.morphisms.get(*m)?;
        let Operation::Proj { index } = proj.op else {
            return None;
        };
        (proj.source == input).then_some(index)
    }

    fn tile_fold_capture(
        &self,
        object: ObjectId,
        fold_source: ObjectId,
        fold_input: ObjectId,
        captures: u32,
    ) -> Option<ObjectId> {
        let slot = self.tile_input_proj_index(object, fold_input)?;
        (slot < captures)
            .then(|| self.pair_slot_source(fold_source, slot))
            .flatten()
    }

    fn tile_iota_size(&self, mut array: ObjectId) -> Option<u64> {
        for _ in 0..16 {
            let next = self
                .proj_of_pair(array)
                .or_else(|| self.capture_proj_of_input(array));
            let Some(next) = next else {
                break;
            };
            if next == array {
                return None;
            }
            array = next;
        }
        let Ty::Array { size, .. } = &self.objects.get(array)?.ty else {
            return None;
        };
        // Consumes the STRUCTURE fact, not the op tag. This used to read
        // `op == Operation::Iota`, which asks what the node is TAGGED rather
        // than what the array's element IS — and `tile_site` calls this twice
        // per site (the outer mapped array and the inner fold's trip count),
        // with a silent fallback to the scalar emitter when it answers `None`.
        // A tag check goes stale the moment anything else can produce an index
        // law; a fact check follows. Behavior today is identical — `Iota` is
        // still the only producer of `ElemSrc::Index` — and the cost of getting
        // this wrong is pinned in `tests/tile_sites_pin.rs` (4.0x on matmul
        // 1024, docs/performance/matmul/s25.md:46-48).
        matches!(self.elem_law_local(array), Some(ElemSrc::Index)).then_some(*size)
    }

    /// The one-step element law of `array` from its producer alone: the local
    /// half of [`elem_plan`]'s recursion, with no loop-residence or depth guard
    /// (the caller owns those). Returns `None` for the cut.
    fn elem_law_local(&self, array: ObjectId) -> Option<ElemSrc> {
        let [definer] = self.in_edges(array) else {
            return None;
        };
        match self.morphisms.get(*definer)?.op {
            Operation::Iota => Some(ElemSrc::Index),
            Operation::Fill => Some(ElemSrc::Broadcast {
                source: self.morphisms.get(*definer)?.source,
                slot: 0,
            }),
            _ => None,
        }
    }

    /// The deduced query `elem : IR × FuncId → ElemPlan` — for each array in
    /// `f`, what `out[i]` is as a closed function of the index
    /// (plan-s37-stage-structure §3.1).
    ///
    /// The law is the unique homomorphism from the producer DAG into the
    /// [`ElemSrc`] term algebra: `Iota ↦ Index`, `Fill ↦ Broadcast`,
    /// `Zip ↦ Pair(·,·)`, `Enumerate ↦ Pair(Index, ·)`, **everything else ↦ the
    /// cut**. Stage composition is nothing more than the recursion continuing
    /// past a cut: `Consumer[mid[i]] ∘ Producer{L} = Consumer[L(i)]`.
    ///
    /// Producers are recognized by an exact op-tag set rather than by "carries
    /// no body", because trap-freedom is a documented guarantee of these four
    /// tags specifically (`graph.rs`) — a future bodyless-looking op, or a
    /// `Call` whose totality is underivable, must not be admitted by shape.
    pub fn elem_plan(&self, f: FuncId) -> ElemPlan {
        let mut plan = ElemPlan::default();
        let Some(def) = self.func(f) else {
            return plan;
        };
        let mut in_loop: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for scc in self.loop_structure(f) {
            for o in scc.objects {
                in_loop.insert(o, ());
            }
        }
        for &m in &def.morphisms {
            let Some(morph) = self.morphisms.get(m) else {
                continue;
            };
            if !matches!(
                morph.op,
                Operation::Iota
                    | Operation::Fill
                    | Operation::Zip
                    | Operation::Enumerate
                    | Operation::Map { .. }
            ) {
                continue;
            }
            if let Some(law) = self.elem_law(morph.target, &in_loop, 0) {
                plan.src.insert(morph.target, law);
            }
        }
        plan
    }

    /// `elem_law_local` plus the three guards, recursing through structural
    /// producers. `None` is the cut.
    ///
    /// Guards, each preventing a specific wrong answer: a **single in-edge**
    /// (a multi-producer object has no unique law to substitute); **outside
    /// every loop SCC** (a loop-carried array's value differs per iteration, so
    /// recomputing at read time would observe a different iteration than the
    /// load did); and a **depth cap of 16** (the `tile_iota_size` /
    /// `element_range` precedent — bounds query cost, and cutting early is
    /// always sound). The cap is what keeps this within J1's spirit: recursion
    /// depth is bounded by a constant, never by graph size.
    fn elem_law(
        &self,
        arr: ObjectId,
        in_loop: &SecondaryMap<ObjectId, ()>,
        depth: u32,
    ) -> Option<ElemSrc> {
        if depth >= 16 || in_loop.contains_key(arr) {
            return None;
        }
        let [definer] = self.in_edges(arr) else {
            return None;
        };
        let morph = self.morphisms.get(*definer)?;
        match morph.op {
            Operation::Iota | Operation::Fill => self.elem_law_local(arr),
            // `Zip`'s source is the 2-tuple product of the two arrays (ADR-0018).
            Operation::Zip => Some(ElemSrc::Pair(
                Box::new(self.elem_component(morph.source, 0, in_loop, depth + 1)),
                Box::new(self.elem_component(morph.source, 1, in_loop, depth + 1)),
            )),
            // `Enumerate`'s source is the bare array; the index half is free.
            Operation::Enumerate => Some(ElemSrc::Pair(
                Box::new(ElemSrc::Index),
                Box::new(self.elem_law(morph.source, in_loop, depth + 1).unwrap_or(
                    ElemSrc::Load {
                        source: morph.source,
                        slot: None,
                    },
                )),
            )),
            // `Map` joins only when its body is classifiable — see the
            // `ElemSrc::Apply` doc for why it is not a peer of the other four.
            Operation::Map { body, captures } if self.body_is_classifiable(body) => {
                let arr_out = arr;
                let arr = if captures == 0 {
                    morph.source
                } else {
                    self.pair_slot_source(morph.source, captures)?
                };
                let inner = self
                    .elem_law(arr, in_loop, depth + 1)
                    .unwrap_or(ElemSrc::Load {
                        source: morph.source,
                        slot: (captures > 0).then_some(captures),
                    });
                Some(ElemSrc::Apply {
                    body,
                    source: morph.source,
                    captures,
                    inner: Box::new(inner),
                    array: arr_out,
                })
            }
            _ => None,
        }
    }

    /// Whether a `Map` body may be **recomputed at a consumer's read site**.
    ///
    /// All three conjuncts are load-bearing, and each prevents a distinct way
    /// the oracle could be contradicted:
    ///
    /// - **trap-free** ([`Self::tile_trap_free`], the predicate the tiled path
    ///   already trusts): a body that can trap must fire at the producer's index
    ///   order, not at whatever order a consumer happens to read in.
    /// - **loop-free**: a body containing a loop could turn a `Trapped` outcome
    ///   into `Diverged` — a different observable class, not a slower path.
    /// - **effect-free**: `tile_trap_free`'s catch-all arm answers `true` for
    ///   `Print`/`TimeMs`. Its existing callers are covered by token threading;
    ///   this one is not, so the check is made here rather than inherited.
    fn body_is_classifiable(&self, body: FuncId) -> bool {
        let Some(def) = self.func(body) else {
            return false;
        };
        let effectful = def.morphisms.iter().any(|m| {
            matches!(
                self.morphisms.get(*m).map(|x| x.op),
                Some(Operation::Print { .. } | Operation::TimeMs)
            )
        });
        !effectful
            && self.loop_structure(body).is_empty()
            && self.tile_trap_free(body, &self.bounds_proof(body), None)
    }

    /// The law of component `slot` of `product`, cutting to a `Load` addressed
    /// as `(product, slot)` — the form the emitters' pointer helpers take.
    fn elem_component(
        &self,
        product: ObjectId,
        slot: u32,
        in_loop: &SecondaryMap<ObjectId, ()>,
        depth: u32,
    ) -> ElemSrc {
        let cut = ElemSrc::Load {
            source: product,
            slot: Some(slot),
        };
        match self.pair_slot_source(product, slot) {
            Some(component) => self.elem_law(component, in_loop, depth).unwrap_or(cut),
            None => cut,
        }
    }

    fn tile_literal_u64(&self, object: ObjectId) -> Option<u64> {
        let object = self.objects.get(object)?;
        if object.kind != ObjectKind::Constant {
            return None;
        }
        match object.value.as_ref()? {
            Value::I32(v) => u64::try_from(*v).ok(),
            Value::I64(v) => u64::try_from(*v).ok(),
            Value::U8(v) => Some((*v).into()),
            _ => None,
        }
    }

    /// Trap-freedom AND emission-completeness for a tiled body: the micro-kernel
    /// emits only the recognized chain, so anything it would skip must be
    /// provably unobservable — pure, trap-free, and with no nested body it
    /// cannot see into. `Call` and any Map/Fold other than the recognized fold
    /// (`allow_fold`) are rejected outright: their subgraphs could trap while
    /// the tiled form skips them (an R1 divergence, not just a missed proof).
    fn tile_trap_free(
        &self,
        f: FuncId,
        bounds: &BoundsProof,
        allow_fold: Option<MorphismId>,
    ) -> bool {
        self.func(f).is_some_and(|fd| {
            fd.morphisms.iter().all(|m| {
                let Some(morph) = self.morphisms.get(*m) else {
                    return false;
                };
                match morph.op {
                    Operation::Div | Operation::Mod => {
                        self.objects.get(morph.target).is_some_and(|target| {
                            matches!(target.ty, Ty::Float { .. })
                                || self
                                    .pair_slot_source(morph.source, 1)
                                    .and_then(|o| self.objects.get(o))
                                    .is_some_and(safe_integer_divisor)
                        })
                    }
                    Operation::Index => bounds.proven(*m),
                    Operation::Update => false,
                    Operation::Call { .. } => false,
                    Operation::Fold { .. } => allow_fold == Some(*m),
                    Operation::Map { .. } => false,
                    _ => true,
                }
            })
        })
    }

    /// The per-function parallel path query. Tasks are emitted in first topo
    /// occurrence order, so their vector indices are deterministic task ids.
    /// Token-bearing morphisms and every morphism in an effectful loop region
    /// remain on the host spine; Print/TimeMs morphisms still contribute
    /// checkpoints.
    /// Trap capability follows Call/Map/Fold function closures and is attributed
    /// to each reference site's topo position. A task containing a pure Call to
    /// a trap-capable named function is pinned; Map/Fold sites never pin.
    /// Checkpoint trap guards use the last earlier site as their watermark
    /// threshold; consumed-value producers require completion.
    pub fn path_plan(&self, f: FuncId) -> PathPlan {
        let topo = self.topo_order(f);
        let bounds = self.bounds_proof(f);
        let trap_capable = self.fn_trap_capabilities();
        let is_token = |m: MorphismId| {
            let morph = &self.morphisms[m];
            ty_contains_token(&self.objects[morph.source].ty)
                || ty_contains_token(&self.objects[morph.target].ty)
        };

        // Recover each loop's complete sequential region. SCC incidence covers
        // the cycle and loop control; loop_plan adds computed route cones that
        // execute inside a canonical loop despite sitting outside the SCC.
        let loops = self.loop_structure(f);
        let mut loop_members = Vec::with_capacity(loops.len());
        for scc in &loops {
            let mut objects: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            for &o in &scc.objects {
                objects.insert(o, ());
            }
            let mut members: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
            for &m in &topo {
                let morph = &self.morphisms[m];
                if objects.contains_key(morph.source) || objects.contains_key(morph.target) {
                    members.insert(m, ());
                }
            }
            for &merge in &scc.merges {
                if let Some(plan) = self.loop_plan(f, merge) {
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
            loop_members.push(
                topo.iter()
                    .copied()
                    .filter(|m| members.contains_key(*m))
                    .collect::<Vec<_>>(),
            );
        }

        let mut host_loop_member: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for members in &loop_members {
            if members.iter().copied().any(&is_token) {
                for &m in members {
                    host_loop_member.insert(m, ());
                }
            }
        }
        let mut loop_of: SecondaryMap<MorphismId, usize> = SecondaryMap::new();
        for (loop_id, members) in loop_members.iter().enumerate() {
            if !members.iter().copied().any(&is_token) {
                for &m in members {
                    if !host_loop_member.contains_key(m) && !loop_of.contains_key(m) {
                        loop_of.insert(m, loop_id);
                    }
                }
            }
        }

        // plan-time-builtin: `TimeMs` is the first host-spine op producing a
        // VALUE (milliseconds) rather than only a token. A task cannot read it
        // — tasks are dispatched at launch and the host writes that frame slot
        // later, on the spine — so a task consuming it races the write (§4.5
        // Law 1: no transformation reads data not present at its location; the
        // symptom is a NEGATIVE elapsed). Its whole consumer cone therefore
        // stays on the spine. The cone is scalar arithmetic in practice, so
        // this costs nothing; a bulk op fed by a clock read would be pinned
        // sequential, which is correct before it is fast.
        let mut host_value: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        let mut host_cone: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for &m in &topo {
            let morph = &self.morphisms[m];
            if matches!(morph.op, Operation::TimeMs) {
                host_value.insert(morph.target, ());
            } else if host_value.contains_key(morph.source) {
                host_cone.insert(m, ());
                host_value.insert(morph.target, ());
            }
        }

        // plan-s33b: a clock read is a DAG node, not a spine instruction. It is
        // token-bearing, so it would be host by `is_token` — but then the only
        // ordering it has is `before` (the source-order fence below), and the
        // tasks written AFTER it are free to start the moment their data is
        // ready, which is before the host reaches the read. Making it a task
        // gives it `after` too, as ordinary dependency edges (§3: one mechanism,
        // not two). A clock inside an effectful loop region stays host —
        // `host_loop_member` still claims it — and keeps its checkpoint.
        let is_clock = |m: MorphismId| matches!(self.morphisms[m].op, Operation::TimeMs);
        let is_host = |m: MorphismId| {
            (is_token(m) && !is_clock(m))
                || host_loop_member.contains_key(m)
                || host_cone.contains_key(m)
        };
        // plan-s39: guard-arm-owned morphisms fold into their Phi's Seq task —
        // a Split site cannot be gated (it is dispatched at launch, before the
        // condition's value exists), so a gated bulk op runs sequentially
        // inside its arm's branch instead.
        // ponytail: sequential guarded maps; per-task enable predicates in
        // mapal-rt if a real program ever guards a big map.
        let gated: SecondaryMap<MorphismId, ()> = {
            let mut g = SecondaryMap::new();
            for site in self.guard_plan(f).into_iter().filter(GuardSite::gated) {
                for &m in site.on_true.own.iter().chain(site.on_false.own.iter()) {
                    g.insert(m, ());
                }
            }
            g
        };
        // plan-s40: an arm-owned loop (its LoopEnter handle is gated) must not
        // mint its own launch-dispatched Seq task — like a gated bulk op, its
        // whole region folds into the Phi's sequential component, and the Phi
        // drives it only when the condition picks that arm.
        let gated_loop: Vec<bool> = loop_members
            .iter()
            .map(|members| {
                members.iter().any(|&m| {
                    matches!(self.morphisms[m].op, Operation::LoopEnter) && gated.contains_key(m)
                })
            })
            .collect();
        let in_gated_loop = |m: MorphismId| loop_of.get(m).is_some_and(|&id| gated_loop[id]);
        let is_scalar = |m: MorphismId| {
            !is_host(m)
                && !is_clock(m)
                // A gated loop's whole region is scalar — bulk ops inside it
                // run sequentially under the driver, under the gate.
                && (in_gated_loop(m)
                    || (!loop_of.contains_key(m)
                        && (gated.contains_key(m)
                            || !matches!(
                                self.morphisms[m].op,
                                Operation::Map { .. }
                                    | Operation::Fold { .. }
                                    | Operation::Zip
                                    | Operation::Enumerate
                                    | Operation::Iota
                                    | Operation::Fill
                            ))))
        };

        let mut tasks = Vec::new();
        let mut weights = Vec::new();
        let mut task_of: SecondaryMap<MorphismId, TaskId> = SecondaryMap::new();
        let mut scalar_seen: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        let mut loop_seen = vec![false; loops.len()];

        // First topo occurrence fixes task order. Scalar components are
        // undirected components of the scalar-only object/morphism subgraph.
        for &m in &topo {
            if is_host(m) || task_of.contains_key(m) {
                continue;
            }

            let (kind, weight) = if let Some(loop_id) =
                loop_of.get(m).copied().filter(|&id| !gated_loop[id])
            {
                if loop_seen[loop_id] {
                    continue;
                }
                loop_seen[loop_id] = true;
                let morphisms: Vec<_> = topo
                    .iter()
                    .copied()
                    .filter(|&candidate| loop_of.get(candidate).copied() == Some(loop_id))
                    .collect();
                (TaskKind::Seq { morphisms }, 1)
            } else {
                match self.morphisms[m].op {
                    // plan-s39: a gated bulk op falls through to the scalar
                    // component (`_` arm) — it joins its Phi's Seq task, since
                    // a Split site is dispatched at launch, before the guard
                    // condition's value exists.
                    Operation::Map { .. }
                    | Operation::Zip
                    | Operation::Enumerate
                    | Operation::Iota
                    | Operation::Fill
                        if !gated.contains_key(m) && loop_of.get(m).is_none() =>
                    {
                        let n = self.bulk_element_count(m);
                        (
                            TaskKind::Split { site: m, n },
                            n.min(u32::MAX as u64) as u32,
                        )
                    }
                    Operation::Fold { .. }
                        if !gated.contains_key(m) && loop_of.get(m).is_none() =>
                    {
                        let n = self.bulk_element_count(m);
                        (
                            TaskKind::Seq { morphisms: vec![m] },
                            n.min(u32::MAX as u64) as u32,
                        )
                    }
                    // plan-s33b: the read alone, never grouped with the scalar
                    // arithmetic around it — the whole point is that its edges
                    // fence work, and a shared task would drag that work inside.
                    Operation::TimeMs => (TaskKind::Seq { morphisms: vec![m] }, 1),
                    _ => {
                        if scalar_seen.contains_key(m) {
                            continue;
                        }
                        let mut component: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
                        let mut stack = vec![m];
                        scalar_seen.insert(m, ());
                        while let Some(current) = stack.pop() {
                            component.insert(current, ());
                            let morph = &self.morphisms[current];
                            for &o in &[morph.source, morph.target] {
                                for &neighbor in self.in_edges(o).iter().chain(self.out_edges(o)) {
                                    if is_scalar(neighbor) && !scalar_seen.contains_key(neighbor) {
                                        scalar_seen.insert(neighbor, ());
                                        stack.push(neighbor);
                                    }
                                }
                            }
                        }
                        let morphisms = topo
                            .iter()
                            .copied()
                            .filter(|candidate| component.contains_key(*candidate))
                            .collect();
                        (TaskKind::Seq { morphisms }, 1)
                    }
                }
            };

            let task_id = tasks.len();
            let members: &[MorphismId] = match &kind {
                TaskKind::Split { site, .. } => std::slice::from_ref(site),
                TaskKind::Seq { morphisms } => morphisms,
            };
            for &member in members {
                task_of.insert(member, task_id);
            }
            tasks.push(Task {
                kind,
                deps: Vec::new(),
                rank: 0,
                trap_min: None,
                pinned: false,
            });
            weights.push(weight);
        }

        let mut topo_pos: SecondaryMap<MorphismId, u32> = SecondaryMap::new();
        for (i, &m) in topo.iter().enumerate() {
            topo_pos.insert(m, i.min(u32::MAX as usize) as u32);
        }
        let mut trap_sites = vec![Vec::new(); tasks.len()];
        // The last SOURCE position each task occupies. `TimeMs` fences on it
        // (plan-time-builtin; see the checkpoint loop). Source position, not
        // topo position: pure work has no ordering relation to a clock read,
        // so topo order is free to schedule the whole program before or after
        // it — and does. What the programmer means by bracketing is the work
        // WRITTEN between the two reads, which is exactly this key.
        let task_max_loc: Vec<u32> = tasks
            .iter()
            .map(|task| {
                let members: &[MorphismId] = match &task.kind {
                    TaskKind::Split { site, .. } => std::slice::from_ref(site),
                    TaskKind::Seq { morphisms } => morphisms,
                };
                members
                    .iter()
                    .map(|&m| self.morphisms[m].loc.start)
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        // The mirror key, for the `after` half of a clock read's fence: the
        // first SOURCE position a task occupies, for the same reason.
        let task_min_loc: Vec<u32> = tasks
            .iter()
            .map(|task| {
                let members: &[MorphismId] = match &task.kind {
                    TaskKind::Split { site, .. } => std::slice::from_ref(site),
                    TaskKind::Seq { morphisms } => morphisms,
                };
                members
                    .iter()
                    .map(|&m| self.morphisms[m].loc.start)
                    .min()
                    .unwrap_or(0)
            })
            .collect();
        // The clock-read tasks, with the source position they fence on.
        let clock_tasks: Vec<(TaskId, u32)> = tasks
            .iter()
            .enumerate()
            .filter_map(|(task_id, task)| match &task.kind {
                TaskKind::Seq { morphisms } => morphisms
                    .first()
                    .filter(|&&m| morphisms.len() == 1 && is_clock(m))
                    .map(|&m| (task_id, self.morphisms[m].loc.start)),
                TaskKind::Split { .. } => None,
            })
            .collect();

        // A task produces every target written by one of its morphisms. Product
        // slot writers are in the same scalar component, hence the same task.
        let mut producer: SecondaryMap<ObjectId, TaskId> = SecondaryMap::new();
        for (task_id, task) in tasks.iter().enumerate() {
            let members: &[MorphismId] = match &task.kind {
                TaskKind::Split { site, .. } => std::slice::from_ref(site),
                TaskKind::Seq { morphisms } => morphisms,
            };
            for &member in members {
                let target = self.morphisms[member].target;
                let old = producer.insert(target, task_id);
                debug_assert!(old.is_none_or(|id| id == task_id));
            }
        }

        // Direct object dataflow is the complete dependency relation.
        for (task_id, task) in tasks.iter_mut().enumerate() {
            let members: &[MorphismId] = match &task.kind {
                TaskKind::Split { site, .. } => std::slice::from_ref(site),
                TaskKind::Seq { morphisms } => morphisms,
            };
            for &member in members {
                let morph = &self.morphisms[member];
                if let Some(dep) = producer.get(morph.source).copied()
                    && dep != task_id
                    && !task.deps.contains(&dep)
                {
                    task.deps.push(dep);
                }
                if self.path_trap_capable(member, &bounds, &trap_capable) {
                    let pos = topo_pos[member];
                    task.trap_min = Some(task.trap_min.map_or(pos, |old| old.min(pos)));
                    trap_sites[task_id].push(pos);
                }
                if let Operation::Call(g) = morph.op
                    && !is_token(member)
                    && trap_capable.get(g).copied().unwrap_or(true)
                {
                    task.pinned = true;
                }
                // The read runs on the host spine at its own topo position, the
                // same as a trap-capable call: a worker running it would put the
                // clock wherever the pool happened to be, which is the ordering
                // this task exists to remove.
                if matches!(morph.op, Operation::TimeMs) {
                    task.pinned = true;
                }
            }
            task.deps.sort_unstable();
        }

        // plan-s33b: the clock read's edges, both ways. `before` is the S29
        // fence restated as dependencies — every task written entirely earlier
        // must be Done. `after` is the half that was missing: every task written
        // entirely later cannot be dispatched until the read has happened, so
        // the interval `t1 - t0` is one the bracketed work cannot leave early.
        // Source order is the key for both halves, for the reason recorded at
        // `task_max_loc`: pure work has no dataflow relation to a clock read.
        // The two sets are disjoint (`max < start` vs `min > start`) and a
        // clock task is in neither of its own, so no edge can close a cycle.
        for &(clock, start) in &clock_tasks {
            for id in 0..tasks.len() {
                if id == clock {
                    continue;
                }
                if task_max_loc[id] < start {
                    if !tasks[clock].deps.contains(&id) {
                        tasks[clock].deps.push(id);
                    }
                } else if task_min_loc[id] > start && !tasks[id].deps.contains(&clock) {
                    tasks[id].deps.push(clock);
                }
            }
        }
        if !clock_tasks.is_empty() {
            for task in &mut tasks {
                task.deps.sort_unstable();
            }
        }

        // Critical-path weight to a sink. A scalar component may first appear
        // before all of its product slots are produced, so derive a task-level
        // topo order rather than assuming task ids are already topological.
        let mut dependents = vec![Vec::new(); tasks.len()];
        let mut remaining: Vec<usize> = tasks.iter().map(|task| task.deps.len()).collect();
        for (task_id, task) in tasks.iter().enumerate() {
            for &dep in &task.deps {
                dependents[dep].push(task_id);
            }
        }
        let mut task_topo: Vec<TaskId> = remaining
            .iter()
            .enumerate()
            .filter_map(|(id, &deps)| (deps == 0).then_some(id))
            .collect();
        let mut cursor = 0;
        while cursor < task_topo.len() {
            let task_id = task_topo[cursor];
            cursor += 1;
            for &dependent in &dependents[task_id] {
                remaining[dependent] -= 1;
                if remaining[dependent] == 0 {
                    task_topo.push(dependent);
                }
            }
        }
        debug_assert_eq!(task_topo.len(), tasks.len());
        for &task_id in task_topo.iter().rev() {
            let tail = dependents[task_id]
                .iter()
                .map(|&dependent| tasks[dependent].rank)
                .max()
                .unwrap_or(0);
            tasks[task_id].rank = weights[task_id].saturating_add(tail);
        }

        let mut checkpoints = Vec::new();
        for (i, &m) in topo.iter().enumerate() {
            let morph = &self.morphisms[m];
            // plan-s33b: a clock read that became a task carries its fence in
            // its own dependency edges, and the emitter's pinned sequence still
            // emits the `mapal_par_check` at this topo position — a second
            // wait list here would be the same ordering stated twice (§3).
            // A clock left on the spine (inside an effectful loop) keeps its
            // checkpoint, which is the only fence it has.
            let checkpoint = matches!(morph.op, Operation::Print { .. })
                || matches!(morph.op, Operation::TimeMs) && !task_of.contains_key(m)
                || matches!(morph.op, Operation::Call(_)) && is_token(m);
            if !checkpoint {
                continue;
            }
            let point = i.min(u32::MAX as usize) as u32;
            let mut wait: Vec<WaitEntry> = trap_sites
                .iter()
                .enumerate()
                .filter_map(|(task, sites)| {
                    sites
                        .iter()
                        .copied()
                        .filter(|&site| site < point)
                        .max()
                        .map(|threshold| WaitEntry {
                            task,
                            threshold: Some(threshold),
                        })
                })
                .collect();

            // Token products are host-built, so walk through their unassigned
            // structural edges until reaching each consumed value's producer
            // task. That producer's own deps have already completed.
            let mut seen: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            let mut stack = vec![morph.source];
            while let Some(o) = stack.pop() {
                if seen.contains_key(o) {
                    continue;
                }
                seen.insert(o, ());
                if let Some(task_id) = producer.get(o).copied() {
                    if let Some(entry) = wait.iter_mut().find(|entry| entry.task == task_id) {
                        entry.threshold = None;
                    } else {
                        wait.push(WaitEntry {
                            task: task_id,
                            threshold: None,
                        });
                    }
                    continue;
                }
                for &incoming in self.in_edges(o) {
                    stack.push(self.morphisms[incoming].source);
                }
            }
            // plan-time-builtin composition rule 1: a clock read must not be
            // reordered across the work it brackets. Under the parallel
            // orchestrator that reordering is the default — a `TimeMs` on the
            // host spine consumes only the token, so it has NO value producer
            // to wait for and would read the clock while the tasks it is meant
            // to measure are still in flight. So `TimeMs` FENCES: every task
            // written entirely BEFORE it in the source must have completed.
            // Source order is the right key precisely because the dataflow
            // graph gives none — pure work is unordered against a clock read,
            // so topo order legally puts all of it on either side. `t1 - t0`
            // therefore measures the work written between the two reads, which
            // is what a bracket means in any normal language, and what lets one
            // opened after the data generation exclude it (the S28 finding).
            if matches!(morph.op, Operation::TimeMs) {
                for (task, &last) in task_max_loc.iter().enumerate() {
                    if last >= morph.loc.start {
                        continue;
                    }
                    match wait.iter_mut().find(|entry| entry.task == task) {
                        Some(entry) => entry.threshold = None,
                        None => wait.push(WaitEntry {
                            task,
                            threshold: None,
                        }),
                    }
                }
            }
            wait.sort_unstable_by_key(|entry| entry.task);
            checkpoints.push(Checkpoint { topo: point, wait });
        }
        checkpoints.push(Checkpoint {
            topo: u32::MAX,
            wait: (0..tasks.len())
                .map(|task| WaitEntry {
                    task,
                    threshold: None,
                })
                .collect(),
        });

        PathPlan { tasks, checkpoints }
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
    /// `LoopBack` is appended after its source completes, never gating.
    ///
    /// **Ties break on source position** — `(loc.start, loc.end, insertion
    /// index)` — never on insertion order alone (plan-s38). The dataflow graph
    /// does not order two independent trapping operations, yet which of them
    /// traps first is observable, so the tie-break must be a function of the
    /// *program*. Insertion order is a function of the compiler: rewriting
    /// creates and destroys objects, so `Inline` could turn `Trapped(IndexOob)`
    /// into `Trapped(DivZero)` and break `eval ∘ rewrite = eval`. Source
    /// position is the only intrinsic key available, and it is the same
    /// reasoning as S29's clock-read fence at wider scope. The insertion index
    /// keeps the key total, so the walk stays deterministic when a desugaring
    /// emits several morphisms from one span.
    ///
    /// Note: exact `loc` ties (a Parameter and Return sharing the function
    /// span, loop objects sharing the loop span) fall back to insertion order.
    /// None of those objects trap on their own, so the ordering they receive is
    /// unobservable.
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

        // The tie-break key per object: source position, made total by the
        // object's insertion index. `nodes` is in insertion order, so the index
        // is also how we get back from a popped key to its object.
        let mut pos: SecondaryMap<ObjectId, (u32, u32, u32)> = SecondaryMap::new();
        for (i, &o) in nodes.iter().enumerate() {
            let loc = self.objects[o].loc;
            pos.insert(o, (loc.start, loc.end, i as u32));
        }

        // Objects that are complete and ready to release their out-edges,
        // popped in source-position order.
        let mut ready: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
        for &o in &nodes {
            if remaining[o] == 0 {
                ready.push(Reverse(pos[o]));
            }
        }

        let mut order: Vec<MorphismId> = Vec::new();
        let mut released: SecondaryMap<ObjectId, bool> = SecondaryMap::new();

        // LoopEnter edges whose source is complete, awaiting a drained worklist
        // (the deferral rule above). FIFO in source-release order (deterministic).
        let mut deferred_enters: Vec<MorphismId> = Vec::new();
        let mut dcursor = 0;

        // Process ready objects source-position-first. When the worklist drains,
        // release the next deferred LoopEnter and continue.
        loop {
            while let Some(Reverse(key)) = ready.pop() {
                let o = nodes[key.2 as usize];
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
                            ready.push(Reverse(pos[tgt]));
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
                            ready.push(Reverse(pos[tgt]));
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

        // The loop-variant cone of the route feeders: every object the route
        // Pair edges transitively depend on that is also forward-reachable
        // from the merge, over non-loop morphisms of `f`. A computed payload
        // (`t * 2 -> ret`) or an exit-arm fanout leaves the SCC — its chain
        // feeds only a route, never cycles back to the merge — so
        // SCC-incidence plus direct route Pairs never schedules it (ADR-0027
        // review: interp read-before-write; llvm emitted it after the loop).
        // Loop ops are never cone edges, so neither walk crosses into a
        // neighboring loop (gated by its LoopExit) nor into the merge's own
        // LoopEnter/LoopBack.
        //
        // The merge-reachability bound matters: a loop-INVARIANT producer
        // (unreachable from the merge — e.g. a param proj also read by the
        // init assembly) must stay walk-owned so it evaluates exactly once,
        // before the driver fires (the S12 rule). Only its *uses* inside the
        // cone re-fire per iteration; the invariant object itself is read
        // from the pre-loop env.
        let mut cone: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        cone.insert(back_route, ());
        cone.insert(exit_route, ());
        let mut variant: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        variant.insert(merge, ());
        loop {
            let mut changed = false;
            for (_, morph) in self.morphisms() {
                if self.try_owner(morph.source) != Some(f) {
                    continue;
                }
                if matches!(
                    morph.op,
                    Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit
                ) {
                    continue;
                }
                if cone.contains_key(morph.target) && !cone.contains_key(morph.source) {
                    cone.insert(morph.source, ());
                    changed = true;
                }
                if variant.contains_key(morph.source) && !variant.contains_key(morph.target) {
                    variant.insert(morph.target, ());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // body_order: topo_order filtered to morphisms incident to SCC(merge) OR
        // targeting the loop-variant route cone, excluding loop ops. Route
        // objects sit outside the SCC, but every morphism assembling them —
        // direct Pair edges and the whole computed-payload chain behind them —
        // must re-fire each iteration, so membership-by-SCC alone would drop
        // them.
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
                    || (cone.contains_key(morph.target) && variant.contains_key(morph.target))
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

    /// The per-fn last-use plan (docs/components/ir/plans/plan-last-use.md §2,
    /// rules 1-6): death positions + escape/carried classification, composed
    /// from [`topo_order`](CategoryIr::topo_order) and
    /// [`loop_plan`](CategoryIr::loop_plan) — never re-derived. `O(V + E)`,
    /// recursion-free (J1), total on any sealed fn: non-canonical loops
    /// contribute no carried set and no re-ranking, so consumers fall back.
    pub fn last_use_plan(&self, f: FuncId) -> LastUsePlan {
        let topo = self.topo_order(f);
        let mut position: SecondaryMap<MorphismId, u32> = SecondaryMap::new();
        for (i, &m) in topo.iter().enumerate() {
            position.insert(m, i as u32);
        }

        // Canonical loops with their plans (non-canonical shapes contribute
        // nothing — rule 6's graceful degradation).
        let mut plans: Vec<LoopPlan> = Vec::new();
        for scc in self.loop_structure(f) {
            for &merge in &scc.merges {
                if let Some(plan) = self.loop_plan(f, merge) {
                    plans.push(plan);
                }
            }
        }

        // Rule 1's re-ranking: within each canonical loop, the guard-first
        // quartet's execution order — decide cone, then the LoopExit, then
        // the advance cone, then the LoopBack — spliced into the topo slots
        // those morphisms already occupy (a global permutation: every other
        // morphism keeps its topo position; ties inside a phase keep topo
        // order). The LoopExit rank between the cones is what lets the
        // exit-route retention pin land correctly: an exit-route Pair use of
        // the merge precedes an advance-cone write (matmul4 — legal) but
        // follows a decide-cone write into the same buffer (illegal).
        let mut claimed: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        for plan in &plans {
            let mut members: Vec<(MorphismId, u8, u32)> = Vec::new();
            for &m in &plan.decide_order {
                members.push((m, 0, position[m]));
            }
            for &m in &plan.exits {
                members.push((m, 1, position[m]));
            }
            for &m in &plan.advance_order {
                members.push((m, 2, position[m]));
            }
            for &m in self.in_edges(plan.merge) {
                if self.morphisms[m].op == Operation::LoopBack {
                    members.push((m, 3, position[m]));
                }
            }
            // Nested canonical loops overlap (the inner's body is part of the
            // outer's): an already-claimed slot degrades this loop's ranking
            // (rule 6 — consumers fall back; carried/escape are unaffected).
            if members.iter().any(|&(m, _, _)| claimed.contains_key(m)) {
                continue;
            }
            let mut slots: Vec<u32> = members.iter().map(|&(_, _, p)| p).collect();
            slots.sort_unstable();
            members.sort_by_key(|&(_, phase, p)| (phase, p));
            for (slot, (m, _, _)) in slots.iter().zip(members.iter()) {
                position.insert(*m, *slot);
                claimed.insert(*m, ());
            }
        }

        // carried_by (rule 3) + the exempt set (the merge, its Proj views —
        // the current-state aliases — and the back-route Pair cone) per
        // canonical loop. Body objects only: a loop-INVARIANT route feeder
        // (not merge-reachable) is not carried — its single buffer would be
        // re-read after a back-edge free.
        let mut carried: SecondaryMap<ObjectId, ObjectId> = SecondaryMap::new();
        let mut exempt: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
        for plan in &plans {
            let mut body: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            for &o in &plan.scc_objects {
                body.insert(o, ());
            }
            for &m in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                body.insert(self.morphisms[m].target, ());
            }
            // The merge and its Proj-view closure.
            let mut stack = vec![plan.merge];
            while let Some(v) = stack.pop() {
                if exempt.contains_key(v) {
                    continue;
                }
                exempt.insert(v, ());
                for &m in self.out_edges(v) {
                    if matches!(self.morphisms[m].op, Operation::Proj { .. }) {
                        stack.push(self.morphisms[m].target);
                    }
                }
            }
            // The back-route Pair cone (the next-state chain crossing the
            // LoopBack), restricted to loop-body objects. Entry is through
            // the route's STATE field (slot 0 — the `(U × Bool) → U`
            // contract; the cond at slot 1 is consumed, not carried); from
            // there, every Pair feeder is state structure (a product state
            // packs its components at all slots).
            carried.insert(plan.back_route, plan.merge);
            exempt.insert(plan.back_route, ());
            let mut stack: Vec<ObjectId> = self
                .in_edges(plan.back_route)
                .iter()
                .filter(|&&m| matches!(self.morphisms[m].op, Operation::Pair { slot: 0, .. }))
                .map(|&m| self.morphisms[m].source)
                .collect();
            while let Some(v) = stack.pop() {
                if carried.contains_key(v) {
                    continue;
                }
                carried.insert(v, plan.merge);
                exempt.insert(v, ());
                for &m in self.in_edges(v) {
                    let morph = &self.morphisms[m];
                    if matches!(morph.op, Operation::Pair { .. }) && body.contains_key(morph.source)
                    {
                        stack.push(morph.source);
                    }
                }
            }
        }

        // Rule 2's escape sets, by reverse reachability from the fn's output:
        // full (any path) and no-LoopExit (the exempt set's variant — carried
        // state escapes only via a path that crosses NO loop exit, e.g. a
        // direct post-loop consumer; its own loop's exit is the per-iteration
        // release valve, and the escaping final instance rides the exit
        // object, which is NOT exempt). Parameters always escape (borrowed —
        // an outer fn's capture source rides the body fn's own Parameter).
        let fd = self.func(f).expect("func resolves");
        let esc_full = self.escape_reach(f, fd.output, true);
        let esc_noexit = self.escape_reach(f, fd.output, false);
        let mut escapes: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
        for (o, obj) in self.objects() {
            if self.try_owner(o) != Some(f) {
                continue;
            }
            let esc = obj.kind == ObjectKind::Parameter
                || if exempt.contains_key(o) {
                    esc_noexit.get(o).copied().unwrap_or(false)
                } else {
                    esc_full.get(o).copied().unwrap_or(false)
                };
            if esc {
                escapes.insert(o, true);
            }
        }

        // use_pos: the greatest adjusted use position, with Pair/Phi
        // retention pins (a packed handle lives as long as the product
        // holding it). Swept in descending definer-position order — a
        // product's definer always outranks its components' (def-before-use
        // survives the phase permutation), so its use_pos is settled first.
        let mut definer: Vec<(ObjectId, Option<u32>)> = Vec::new();
        for (o, obj) in self.objects() {
            if self.try_owner(o) != Some(f) {
                continue;
            }
            let d = match obj.kind {
                ObjectKind::Parameter | ObjectKind::Constant => None,
                ObjectKind::LoopMerge => self
                    .in_edges(o)
                    .iter()
                    .filter(|&&m| self.morphisms[m].op == Operation::LoopEnter)
                    .map(|&m| position[m])
                    .max(),
                _ => self.in_edges(o).iter().map(|&m| position[m]).max(),
            };
            definer.push((o, d));
        }
        // Descending definer position; no-definer objects (Parameters,
        // Constants) strictly last. The sort is stable, so ties keep
        // insertion order (deterministic, and no per-object position scans —
        // object_seq would make this O(V² log V) on the §16 deep graph).
        definer.sort_by_key(|&(_, d)| (d.is_none(), d.map_or(0, |p| u32::MAX - p)));
        let mut use_pos: SecondaryMap<ObjectId, u32> = SecondaryMap::new();
        for &(o, _) in &definer {
            let mut best: Option<u32> = None;
            for &m in self.out_edges(o) {
                let p = position[m];
                best = Some(best.map_or(p, |b: u32| b.max(p)));
                if matches!(
                    self.morphisms[m].op,
                    Operation::Pair { .. } | Operation::Phi
                ) && let Some(&up) = use_pos.get(self.morphisms[m].target)
                {
                    best = Some(best.map_or(up, |b: u32| b.max(up)));
                }
            }
            if let Some(b) = best {
                use_pos.insert(o, b);
            }
        }

        LastUsePlan {
            position,
            use_pos,
            escapes,
            carried,
        }
    }

    /// Reverse reachability from `root` over `f`'s in-edges — rule 2's escape
    /// walk — traversing only the value-RETAINING/-aliasing morphisms: `Pair`
    /// (the product holds the component), `Phi` (the result aliases either
    /// arm — plan §7's note), `Output` (identity), `LoopExit` (payload
    /// forwarding; dropped when `allow_loop_exit` is false — the
    /// carried-state exemption's walk), `LoopEnter` (the init aliases the
    /// merge's first instance), `Proj` (the projection aliases the product's
    /// field), `Call` (the call boundary may return an alias of its borrowed
    /// argument), and an array-typed `Index` (the sub-buffer alias).
    /// Everything else — arithmetic, comparisons, `Update`/`Map`/`Fold`/
    /// `Zip`/`Enumerate` (fresh buffers/results), `Print`, `TimeMs`, `Neg`,
    /// `Not` — consumes and produces anew: no escape flows through it.
    /// Worklist BFS, deterministic (in-edge insertion order), no HashMap (L2).
    fn escape_reach(
        &self,
        f: FuncId,
        root: ObjectId,
        allow_loop_exit: bool,
    ) -> SecondaryMap<ObjectId, bool> {
        let mut seen: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
        let mut stack = vec![root];
        while let Some(o) = stack.pop() {
            if seen.get(o).copied().unwrap_or(false) {
                continue;
            }
            seen.insert(o, true);
            for &m in self.in_edges(o) {
                let morph = &self.morphisms[m];
                let retention = match morph.op {
                    Operation::Pair { .. }
                    | Operation::Phi
                    | Operation::Output
                    | Operation::LoopEnter
                    | Operation::Proj { .. }
                    | Operation::Call(_) => true,
                    Operation::LoopExit => allow_loop_exit,
                    // The sub-buffer alias: an array element shares the
                    // parent's flat buffer region; a scalar element shares
                    // nothing.
                    Operation::Index => {
                        matches!(self.objects[morph.target].ty, crate::ty::Ty::Array { .. })
                    }
                    _ => false,
                };
                if !retention {
                    continue;
                }
                let s = morph.source;
                if self.try_owner(s) == Some(f) {
                    stack.push(s);
                }
            }
        }
        seen
    }

    /// The bounds-proof query: which of `f`'s `Index` morphisms are provably
    /// in-bounds (`[0, n)`)? Interval analysis over the object graph — ranges
    /// from `Constant`s, `Iota` elements, enumerate indices, literal-ramp
    /// arrays, and Map/Fold body quantification (the element param of a body
    /// fn rides the site source's element range: iota ⇒ `[0, n)`, enumerate ⇒
    /// `.0` of `(i32, X)`, a literal array of int constants ⇒ `[min, max]`).
    /// One `topo_order` pass; anything unknown/wrapping/loop-carried is NOT
    /// proven (consumers keep today's guards there — zero behavior change).
    pub fn bounds_proof(&self, f: FuncId) -> BoundsProof {
        let mut visited: Vec<FuncId> = Vec::new();
        self.bounds_proof_inner(f, &mut visited).0
    }

    /// The recursive worker: returns the proof AND the full range map so a
    /// nested body's site owner can harvest capture ranges from it.
    fn bounds_proof_inner(
        &self,
        f: FuncId,
        visited: &mut Vec<FuncId>,
    ) -> (BoundsProof, SecondaryMap<ObjectId, Rng>) {
        let mut rng: SecondaryMap<ObjectId, Rng> = SecondaryMap::new();
        let mut proven: SecondaryMap<MorphismId, ()> = SecondaryMap::new();

        // --- seed: constants (non-negative ints) ------------------------------
        for (o, obj) in self.objects() {
            if self.try_owner(o) != Some(f) || obj.kind != ObjectKind::Constant {
                continue;
            }
            match &obj.value {
                Some(Value::I32(x)) if *x >= 0 => {
                    rng.insert(o, Rng::Int(*x as u64, *x as u64));
                }
                Some(Value::I64(x)) if *x >= 0 => {
                    rng.insert(o, Rng::Int(*x as u64, *x as u64));
                }
                Some(Value::U8(x)) => {
                    rng.insert(o, Rng::Int(*x as u64, *x as u64));
                }
                _ => {}
            }
        }

        // --- seed: the Map/Fold body context (the quantified element param) ---
        let fd = self.func(f);
        if let Some(fd) = fd {
            for (_, morph) in self.morphisms() {
                let (k, is_fold) = match morph.op {
                    Operation::Map { body, captures } if body == f => (captures, false),
                    Operation::Fold { body, captures } if body == f => (captures, true),
                    _ => continue,
                };
                let arr = if is_fold {
                    self.pair_slot_source(morph.source, k + 1)
                } else if k == 0 {
                    Some(morph.source)
                } else {
                    self.pair_slot_source(morph.source, k)
                };
                if let Some(er) = arr.and_then(|a| self.element_range(a)) {
                    if !is_fold && k == 0 {
                        // captures=0 map: the body input IS the element.
                        rng.insert(fd.input, er);
                    } else {
                        let pos = if is_fold { k + 1 } else { k };
                        for &m in self.out_edges(fd.input) {
                            let mm = &self.morphisms[m];
                            if mm.op == (Operation::Proj { index: pos }) {
                                rng.insert(mm.target, er);
                            }
                        }
                    }
                }
                // Capture-range threading: a capture at position j rides the
                // site's slot-j feeder, whose range is computed by the site
                // owner's own analysis (recursing out through nested bodies —
                // the owner encloses the capture by construction). The matmul
                // fold's `i`/`j` thus arrive ranged from the enclosing map body.
                if let Some(of) = self.try_owner(morph.source)
                    && of != f
                    && !visited.contains(&of)
                {
                    visited.push(of);
                    let (_, outer_rng) = self.bounds_proof_inner(of, visited);
                    for j in 0..k {
                        let co = self.pair_slot_source(morph.source, j);
                        if let Some(co) = co
                            && let Some(r) = outer_rng.get(co).copied()
                        {
                            for &m in self.out_edges(fd.input) {
                                let mm = &self.morphisms[m];
                                if mm.op == (Operation::Proj { index: j }) {
                                    rng.insert(mm.target, r);
                                }
                            }
                        }
                    }
                }
                break;
            }
        }

        // --- one topo pass: propagate + collect proofs -------------------------
        for m in self.topo_order(f) {
            let morph = &self.morphisms[m];
            match morph.op {
                Operation::Add
                | Operation::Sub
                | Operation::Mul
                | Operation::Div
                | Operation::Mod => {
                    let a = self.pair_slot_source(morph.source, 0);
                    let b = self.pair_slot_source(morph.source, 1);
                    if let (Some(ra), Some(rb)) = (
                        a.and_then(|x| rng.get(x).copied()),
                        b.and_then(|x| rng.get(x).copied()),
                    ) {
                        let wmax = width_max(&self.objects[morph.target].ty);
                        if let Some(r) = arith_range(morph.op, ra, rb, wmax) {
                            rng.insert(morph.target, r);
                        }
                    }
                }
                Operation::Proj { index } => {
                    if let Some(r) = self.proj_range(&rng, morph.source, index) {
                        rng.insert(morph.target, r);
                    }
                }
                Operation::Index => {
                    // A value read from an iota / literal-ramp array is ranged.
                    if let Some(Rng::Int(lo, hi)) = self
                        .pair_slot_source(morph.source, 0)
                        .and_then(|a| self.element_range(a))
                    {
                        let _ = (lo, hi);
                        rng.insert(morph.target, Rng::Int(lo, hi));
                    }
                    // The proof: the index's range inside the array's size.
                    let idx = self.pair_slot_source(morph.source, 1);
                    let arr = self.pair_slot_source(morph.source, 0);
                    if let (Some(Rng::Int(_lo, hi)), Some(a)) =
                        (idx.and_then(|x| rng.get(x).copied()), arr)
                        && let Ty::Array { size, .. } = self.objects[a].ty
                        && hi < size
                    {
                        proven.insert(m, ());
                    }
                }
                _ => {}
            }
        }

        (BoundsProof { proven }, rng)
    }

    /// The element range of an array object: iota ⇒ `[0, n)`; enumerate ⇒
    /// `(i32, X)` elements over `n`; a literal array of int constants ⇒
    /// `[min, max]`. Anything else is unknown (None). `Proj`-of-pair
    /// indirection and body-input capture indirection (the array rides in as a
    /// captured value — e.g. the fold site's `krange` captured by the map
    /// body) are resolved first.
    fn element_range(&self, arr: ObjectId) -> Option<Rng> {
        let mut arr = arr;
        for _ in 0..16 {
            if let Some(next) = self.proj_of_pair(arr) {
                arr = next;
                continue;
            }
            if let Some(next) = self.capture_proj_of_input(arr) {
                arr = next;
                continue;
            }
            break;
        }
        let definer = self.in_edges(arr).first().copied()?;
        let dm = &self.morphisms[definer];
        let size = match &self.objects[arr].ty {
            Ty::Array { size, .. } => *size,
            _ => return None,
        };
        match dm.op {
            Operation::Iota => Some(Rng::Int(0, size.saturating_sub(1))),
            Operation::Enumerate => Some(Rng::EnumIdx(size)),
            Operation::Pair { .. } => {
                // A pack_array literal: every in-edge a Pair with a Constant
                // int source — the element range is [min, max] of the values.
                let mut lo = u64::MAX;
                let mut hi = 0u64;
                for &m in self.in_edges(arr) {
                    let pm = &self.morphisms[m];
                    if !matches!(pm.op, Operation::Pair { .. }) {
                        return None;
                    }
                    let src = &self.objects[pm.source];
                    if src.kind != ObjectKind::Constant {
                        return None;
                    }
                    let v = match &src.value {
                        Some(Value::I32(x)) if *x >= 0 => *x as u64,
                        Some(Value::I64(x)) if *x >= 0 => *x as u64,
                        Some(Value::U8(x)) => *x as u64,
                        _ => return None,
                    };
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                Some(Rng::Int(lo, hi))
            }
            _ => None,
        }
    }

    /// The range of a `Proj{index}` result: `.0` of an enumerate element, else
    /// the pair-slot feeder's own range (Parameters/products without one bail).
    fn proj_range(
        &self,
        rng: &SecondaryMap<ObjectId, Rng>,
        source: ObjectId,
        index: u32,
    ) -> Option<Rng> {
        if let Some(Rng::EnumIdx(n)) = rng.get(source).copied()
            && index == 0
        {
            return Some(Rng::Int(0, n.saturating_sub(1)));
        }
        self.pair_slot_source(source, index)
            .and_then(|s| rng.get(s).copied())
    }

    /// If `o` is a `Proj{k}` of a pair-assembled tuple, the component object
    /// (the `Pair{k}` feeder's source) — one indirection level.
    fn proj_of_pair(&self, o: ObjectId) -> Option<ObjectId> {
        let m = self.in_edges(o).first().copied()?;
        let pm = &self.morphisms[m];
        let Operation::Proj { index } = pm.op else {
            return None;
        };
        self.pair_slot_source(pm.source, index)
    }

    /// If `o` is a `Proj{k}` of a body fn's input `Parameter` at a capture
    /// position, the matching feeder of THAT fn's site source (the value the
    /// enclosing fn actually passed) — one capture indirection level.
    fn capture_proj_of_input(&self, o: ObjectId) -> Option<ObjectId> {
        let m = self.in_edges(o).first().copied()?;
        let pm = &self.morphisms[m];
        let Operation::Proj { index } = pm.op else {
            return None;
        };
        if self.objects[pm.source].kind != ObjectKind::Parameter {
            return None;
        }
        let of = self.try_owner(pm.source)?;
        for (_, morph) in self.morphisms() {
            let k = match morph.op {
                Operation::Map { body, captures } if body == of => captures,
                Operation::Fold { body, captures } if body == of => captures,
                _ => continue,
            };
            // Capture positions are 0..k (map: element at k; fold: acc at k,
            // element at k+1) — only capture Projs resolve here.
            if index < k {
                return self.pair_slot_source(morph.source, index);
            }
            return None;
        }
        None
    }

    /// The source object feeding slot `slot` of a product object (the
    /// `Pair{slot}` in-edge's source), if present.
    fn pair_slot_source(&self, product: ObjectId, slot: u32) -> Option<ObjectId> {
        self.in_edges(product).iter().find_map(|&m| {
            let pm = &self.morphisms[m];
            if matches!(pm.op, Operation::Pair { slot: s, .. } if s == slot) {
                Some(pm.source)
            } else {
                None
            }
        })
    }

    /// Static element count for a Map/Zip/Enumerate/Iota/Fill/Fold site.
    fn bulk_element_count(&self, m: MorphismId) -> u64 {
        let morph = &self.morphisms[m];
        let array = match morph.op {
            Operation::Fold { captures, .. } => self.pair_slot_source(morph.source, captures + 1),
            _ => Some(morph.target),
        }
        .expect("sealed bulk operation has its array operand");
        match self.objects[array].ty {
            Ty::Array { size, .. } => size,
            _ => unreachable!("sealed bulk operation has an array extent"),
        }
    }

    /// Direct trap capability within one function. Bounds proofs exempt Index
    /// only; Update remains trap-capable, as do integer Div/Mod.
    fn path_local_trap_capable(&self, m: MorphismId, bounds: &BoundsProof) -> bool {
        let morph = &self.morphisms[m];
        match morph.op {
            Operation::Div | Operation::Mod => {
                matches!(self.objects[morph.target].ty, Ty::Int { .. })
            }
            Operation::Index => !bounds.proven(m),
            Operation::Update => true,
            _ => false,
        }
    }

    /// Per-function trap-capability fixpoint. Trap-free leaves flow backward
    /// through Call/Map/Fold references; unresolved cycles remain capable.
    fn fn_trap_capabilities(&self) -> SecondaryMap<FuncId, bool> {
        let mut capable = SecondaryMap::new();
        let mut remaining = SecondaryMap::new();
        let mut dependents: SecondaryMap<FuncId, Vec<FuncId>> = SecondaryMap::new();
        for (f, _) in self.funcs() {
            capable.insert(f, true);
            dependents.insert(f, Vec::new());
        }

        for (f, def) in self.funcs() {
            let bounds = self.bounds_proof(f);
            let local = def
                .morphisms
                .iter()
                .copied()
                .any(|m| self.path_local_trap_capable(m, &bounds));
            let mut refs = Vec::new();
            for &m in &def.morphisms {
                match self.morphisms[m].op {
                    Operation::Call(g)
                    | Operation::Map { body: g, .. }
                    | Operation::Fold { body: g, .. } => refs.push(g),
                    _ => {}
                }
            }
            for &g in &refs {
                if let Some(callers) = dependents.get_mut(g) {
                    callers.push(f);
                }
            }
            if !local {
                remaining.insert(f, refs.len());
            }
        }

        let mut worklist = Vec::new();
        for (f, _) in self.funcs() {
            if remaining.get(f) == Some(&0) {
                capable.insert(f, false);
                worklist.push(f);
            }
        }
        let mut cursor = 0;
        while cursor < worklist.len() {
            let trap_free = worklist[cursor];
            cursor += 1;
            for &caller in dependents
                .get(trap_free)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let ready = if let Some(left) = remaining.get_mut(caller) {
                    *left -= 1;
                    *left == 0
                } else {
                    false
                };
                if ready {
                    capable.insert(caller, false);
                    worklist.push(caller);
                }
            }
        }
        capable
    }

    /// Trap capability at a path-plan site, including referenced closures.
    fn path_trap_capable(
        &self,
        m: MorphismId,
        bounds: &BoundsProof,
        capable: &SecondaryMap<FuncId, bool>,
    ) -> bool {
        if self.path_local_trap_capable(m, bounds) {
            return true;
        }
        match self.morphisms[m].op {
            Operation::Call(g)
            | Operation::Map { body: g, .. }
            | Operation::Fold { body: g, .. } => capable.get(g).copied().unwrap_or(true),
            _ => false,
        }
    }

    /// Per-Phi guard sites (plan-s39): the condition object and each arm's
    /// exclusive work. Sites are returned in topo order of their `Phi`.
    ///
    /// An arm's RAW exclusive work is what liveness loses when the arm's
    /// `Pair` edge is removed: objects backward-reachable from the roots
    /// (function output, token-bearing objects, loop machinery) through the
    /// full graph but not without that edge, plus the edge itself. Nested
    /// sites' work is then subtracted so own-lists partition (doc on
    /// [`GuardArm`]). A Phi whose triple is not the builder's three-`Pair`
    /// shape, or whose arm reaches loop machinery, is skipped — consumers keep
    /// strict semantics for it.
    pub fn guard_plan(&self, f: FuncId) -> Vec<GuardSite> {
        let topo = self.topo_order(f);

        // No Phi ⇒ no sites. Skip the unit construction, bounds proof and
        // trap-capability fixpoint entirely — most functions are Phi-free,
        // and this query runs per pass per fixpoint round (S40 compile-time
        // A/B: the sweep regressed +16% before this gate).
        if !topo.iter().any(|&m| self.morphisms[m].op == Operation::Phi) {
            return Vec::new();
        }

        // plan-s40: loop UNITS — per SCC, the whole sequential region the flat
        // walk hands the driver (SCC-incident morphisms, machinery, and the
        // loop_plan cones; the same construction as path_plan's loop_members).
        // An arm owns a loop as a unit or not at all: per-morphism consumer
        // closure provably cannot complete a cycle, so the only machinery it
        // can reach alone is the exit boundary, and gating that fragment
        // starves the driver ("route object built before read", S39 §4a).
        let mut units: Vec<LoopUnit> = Vec::new();
        let mut unit_of_machinery: SecondaryMap<MorphismId, usize> = SecondaryMap::new();
        let mut unit_of_member: SecondaryMap<MorphismId, usize> = SecondaryMap::new();
        for scc in self.loop_structure(f) {
            let mut objects: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
            for &o in &scc.objects {
                objects.insert(o, ());
            }
            let mut members: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
            for &m in &topo {
                let morph = &self.morphisms[m];
                if objects.contains_key(morph.source) || objects.contains_key(morph.target) {
                    members.insert(m, ());
                }
            }
            let mut canonical = scc.merges.len() == 1 && self.loop_plan(f, scc.merges[0]).is_some();
            for &merge in &scc.merges {
                if let Some(plan) = self.loop_plan(f, merge) {
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
            let idx = units.len();
            let mut enters = Vec::new();
            for (m, ()) in members.iter() {
                if matches!(
                    self.morphisms[m].op,
                    Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit
                ) {
                    if let Some(&prev) = unit_of_machinery.get(m) {
                        // Machinery shared between two SCC regions: neither
                        // unit is independently drivable — never join either.
                        units[prev].canonical = false;
                        canonical = false;
                    }
                    unit_of_machinery.insert(m, idx);
                    if matches!(self.morphisms[m].op, Operation::LoopEnter) {
                        enters.push(m);
                    }
                }
                unit_of_member.insert(m, idx);
            }
            units.push(LoopUnit {
                members,
                enters,
                canonical,
                can_trap: false,
            });
        }

        let bounds = self.bounds_proof(f);
        let trap_capable = self.fn_trap_capabilities();

        // Unit trap flags: any member trap-capable (transitively, via bodies
        // and calls). Needed because unit internals never enter an own-list —
        // the LoopEnter handle must carry the whole unit's flag.
        for i in 0..units.len() {
            let any = units[i]
                .members
                .iter()
                .any(|(m, ())| self.path_trap_capable(m, &bounds, &trap_capable));
            units[i].can_trap = any;
        }

        // Raw sites first; hierarchy subtraction below.
        let mut sites: Vec<GuardSite> = Vec::new();
        for &m in &topo {
            if self.morphisms[m].op != Operation::Phi {
                continue;
            }
            let triple = self.morphisms[m].source;
            // The builder's phi() mints a fresh triple with exactly three Pair
            // edges, slots 0/1/2 (builder.rs). Anything else is hand-built —
            // skip, keeping strict semantics there.
            let edges = self.in_edges(triple);
            if edges.len() != 3 {
                continue;
            }
            let mut feeder: [Option<MorphismId>; 3] = [None; 3];
            for &e in edges {
                if let Operation::Pair { slot, arity: 3 } = self.morphisms[e].op
                    && slot < 3
                {
                    feeder[slot as usize] = Some(e);
                }
            }
            let (Some(te), Some(fe), Some(ce)) = (feeder[0], feeder[1], feeder[2]) else {
                continue;
            };
            let [on_true, on_false] = [te, fe].map(|e| {
                self.guard_arm(&topo, &units, &unit_of_machinery, &bounds, &trap_capable, e)
            });
            sites.push(GuardSite {
                phi: m,
                cond: self.morphisms[ce].source,
                on_true,
                on_false,
            });
        }

        // Direct ownership: strip from each arm every morphism owned by a
        // nested site (one whose phi is in this arm's raw list). The nested
        // phi itself stays — it is the gate that fires the nested work.
        let raw: Vec<(SecondaryMap<MorphismId, ()>, MorphismId)> = sites
            .iter()
            .map(|s| {
                let mut set = SecondaryMap::new();
                for &m in s.on_true.own.iter().chain(s.on_false.own.iter()) {
                    set.insert(m, ());
                }
                (set, s.phi)
            })
            .collect();
        for i in 0..sites.len() {
            let site = &mut sites[i];
            for arm in [&mut site.on_true, &mut site.on_false] {
                let arm_has: SecondaryMap<MorphismId, ()> = {
                    let mut s = SecondaryMap::new();
                    for &m in &arm.own {
                        s.insert(m, ());
                    }
                    s
                };
                // Review find [2]: a nested site whose Phi sits INSIDE an
                // owned unit (an in-body guard) is gated by this arm through
                // the unit's handle, not through its Phi appearing in `own` —
                // but its exclusive work can be loop-INVARIANT (fed by
                // constants), hence not a unit member, and without this it
                // stayed in the enclosing arm's list too: owned twice, fired
                // twice.
                let in_owned_unit = |phi: MorphismId| {
                    unit_of_member
                        .get(phi)
                        .is_some_and(|&u| units[u].enters.iter().any(|h| arm_has.contains_key(*h)))
                };
                let mut nested: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
                for (j, (set, phi)) in raw.iter().enumerate() {
                    if j != i && (arm_has.contains_key(*phi) || in_owned_unit(*phi)) {
                        for (m, ()) in set.iter() {
                            nested.insert(m, ());
                        }
                    }
                }
                arm.own.retain(|m| !nested.contains_key(*m));

                // RE-CLOSE. Subtraction can break consumer closure: a morphism
                // may have joined this arm only because a sibling guard's edge
                // was also owned at walk time, and that edge has just been
                // removed. Left in, its target would be gated while a consumer
                // outside the arm still reads it. Drop to a fixpoint; anything
                // dropped simply runs unconditionally, which is always safe.
                // plan-s40: a consumer inside a loop unit counts as covered
                // when the unit's LoopEnter handle is in the arm — the driver
                // fires internals, so the handle stands for all of them.
                loop {
                    let have: SecondaryMap<MorphismId, ()> = {
                        let mut s = SecondaryMap::new();
                        for &m in &arm.own {
                            s.insert(m, ());
                        }
                        s
                    };
                    let covered = |c: MorphismId| {
                        have.contains_key(c)
                            || unit_of_member.get(c).is_some_and(|&u| {
                                units[u].enters.iter().any(|h| have.contains_key(*h))
                            })
                    };
                    let before = arm.own.len();
                    let edge = arm.edge;
                    arm.own.retain(|&m| {
                        if m == edge {
                            return true;
                        }
                        if let Some(&u) = unit_of_machinery.get(m) {
                            // Review find [0]: the handle stands for the whole
                            // unit, so it is re-tested with the JOIN predicate —
                            // the unit's outputs must still be consumed inside
                            // the arm. Testing only out_edges(merge) is vacuous
                            // (every consumer is a member), which let a gated
                            // loop survive while the subtraction cascade
                            // dropped its payload consumers: the un-gated
                            // survivors then read an object the gated loop
                            // never wrote. Sink members refuse here too.
                            return units[u].members.iter().all(|(mm, ())| {
                                let outs = self.out_edges(self.morphisms[mm].target);
                                !outs.is_empty()
                                    && outs
                                        .iter()
                                        .all(|&c| units[u].members.contains_key(c) || covered(c))
                            });
                        }
                        self.out_edges(self.morphisms[m].target)
                            .iter()
                            .all(|&c| covered(c))
                    });
                    if arm.own.len() == before {
                        break;
                    }
                }

                // Flags describe the FINAL list, not the raw walk. A LoopEnter
                // handle carries its whole unit's trap flag, and a loop is
                // heavy by definition (unbounded trip count).
                arm.can_trap = arm.own.iter().any(|&m| {
                    self.path_trap_capable(m, &bounds, &trap_capable)
                        || unit_of_machinery.get(m).is_some_and(|&u| units[u].can_trap)
                });
                arm.heavy = arm.own.iter().any(|&m| {
                    matches!(
                        self.morphisms[m].op,
                        Operation::Map { .. }
                            | Operation::Fold { .. }
                            | Operation::Zip
                            | Operation::Enumerate
                            | Operation::Iota
                            | Operation::Fill
                            | Operation::Call(_)
                            | Operation::LoopEnter
                    )
                });
            }
        }

        // TRANSITIVE flags. Subtraction moved each nested site's work out of
        // the enclosing arm's list, so direct flags under-report: `calc`'s
        // 5-arm match right-folds to nested Phis with `a % b` innermost, and
        // the outer arms looked trap-free because the `Div`/`Mod` had been
        // attributed to the innermost site. Ungated, the outer arms then ran
        // the whole nested chain and `calc(0, 20, 0)` still trapped. An arm
        // that gates a nested site inherits that site's flags. Sites are in
        // topo order of their `Phi`, so nested sites are already final when an
        // enclosing one is reached — one forward pass suffices.
        for i in 0..sites.len() {
            let inherited: Vec<(bool, bool, bool)> = (0..i)
                .map(|j| {
                    (
                        sites[i].on_true.own.contains(&sites[j].phi),
                        sites[j].on_true.can_trap || sites[j].on_false.can_trap,
                        sites[j].on_true.heavy || sites[j].on_false.heavy,
                    )
                })
                .collect();
            let inherited_f: Vec<(bool, bool, bool)> = (0..i)
                .map(|j| {
                    (
                        sites[i].on_false.own.contains(&sites[j].phi),
                        sites[j].on_true.can_trap || sites[j].on_false.can_trap,
                        sites[j].on_true.heavy || sites[j].on_false.heavy,
                    )
                })
                .collect();
            let site = &mut sites[i];
            for (arm, src) in [
                (&mut site.on_true, &inherited),
                (&mut site.on_false, &inherited_f),
            ] {
                for &(nested_here, trap, heavy) in src {
                    if nested_here {
                        arm.can_trap |= trap;
                        arm.heavy |= heavy;
                    }
                }
            }
        }
        sites
    }

    /// One arm's exclusive work.
    ///
    /// **Consumer closure, not liveness.** A morphism is arm-owned iff EVERY
    /// consumer of its target is arm-owned; the boundary `Pair` edge seeds the
    /// set. Liveness is the wrong test here and was a real defect: nothing in
    /// this pipeline deletes dead code before execution — the interpreter walks
    /// every morphism in topo order — so a *dead* consumer still reads its
    /// operand. Under the liveness rule a `Proj` feeding both the arm and a
    /// dead `Neg` looked exclusive (removing the arm edge left only dead uses),
    /// got gated, and the dead `Neg` then read an object the unchosen arm never
    /// wrote. Requiring every consumer to be owned is what makes gating an
    /// object safe, whether or not the other consumers matter.
    ///
    /// **Loops join as units (plan-s40).** Machinery is never a per-morphism
    /// candidate; reaching a `LoopEnter`/`LoopBack`/`LoopExit` offers its whole
    /// [`LoopUnit`], which joins iff every consumer outside the unit of every
    /// member's target is already owned. In the own-list the unit is its
    /// `LoopEnter` handle(s) alone — internals are the driver's, exactly the
    /// set the flat walk already skips. This replaces the v1 refusal, which
    /// keyed the site's SEMANTICS on graph shape (SCC presence) and therefore
    /// could not commute with `LiftLoops` (S39 §4a).
    #[allow(clippy::too_many_arguments)]
    fn guard_arm(
        &self,
        topo: &[MorphismId],
        units: &[LoopUnit],
        unit_of_machinery: &SecondaryMap<MorphismId, usize>,
        bounds: &BoundsProof,
        trap_capable: &SecondaryMap<FuncId, bool>,
        edge: MorphismId,
    ) -> GuardArm {
        let value = self.morphisms[edge].source;

        let mut owned_set: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        let mut owned_via_unit: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
        let mut joined = vec![false; units.len()];
        owned_set.insert(edge, ());
        // Backward worklist: after owning `m`, its source's producers become
        // candidates. A candidate joins iff all of its target's consumers are
        // already owned (an object with no consumer is a sink — never owned:
        // a Return/Output-class sink is observable, and a dead `Temporary`
        // sink must NOT be gated — a dead cone can read BOTH arms' values,
        // and gating it into one arm would read the other's un-fired value.
        // DCE preserves such cones instead; see graph_rewrites.rs).
        let mut queue: Vec<MorphismId> = self.in_edges(value).to_vec();
        while let Some(cand) = queue.pop() {
            if owned_set.contains_key(cand) {
                continue;
            }
            if let Some(&u) = unit_of_machinery.get(cand) {
                if joined[u] || !units[u].canonical {
                    continue;
                }
                let unit = &units[u];
                // A site INSIDE the loop (its boundary edge is a unit member)
                // can never own its own enclosing unit — from inside, every
                // "external" consumer is vacuously satisfied. The driver
                // already runs that site; the arm gates only body work.
                if unit.members.contains_key(edge) {
                    continue;
                }
                // Sink targets refuse the join exactly as they refuse
                // per-morphism ownership (review find [1]): `all()` over an
                // empty consumer list is vacuously true, and a unit member
                // writing a consumerless object — the function's Return, an
                // Output, or dead code DCE must preserve — is work the arm
                // must not gate.
                let joinable = unit.members.iter().all(|(m, ())| {
                    let outs = self.out_edges(self.morphisms[m].target);
                    !outs.is_empty()
                        && outs
                            .iter()
                            .all(|c| unit.members.contains_key(*c) || owned_set.contains_key(*c))
                });
                if !joinable {
                    continue;
                }
                joined[u] = true;
                for (m, ()) in unit.members.iter() {
                    owned_set.insert(m, ());
                    owned_via_unit.insert(m, ());
                }
                for (m, ()) in unit.members.iter() {
                    // The unit's external inputs join the walk; re-offering
                    // producers of member targets can complete other
                    // candidates' consumer sets.
                    queue.extend_from_slice(self.in_edges(self.morphisms[m].source));
                    queue.extend_from_slice(self.in_edges(self.morphisms[m].target));
                }
                continue;
            }
            let t = self.morphisms[cand].target;
            let outs = self.out_edges(t);
            if outs.is_empty() || !outs.iter().all(|o| owned_set.contains_key(*o)) {
                continue;
            }
            owned_set.insert(cand, ());
            queue.extend_from_slice(self.in_edges(self.morphisms[cand].source));
            // Owning `cand` can complete another candidate's consumer set, so
            // re-offer this object's other producers too.
            queue.extend_from_slice(self.in_edges(t));
        }

        let mut own: Vec<MorphismId> = Vec::new();
        let mut can_trap = false;
        let mut heavy = false;
        for &m in topo {
            let morph = &self.morphisms[m];
            if !owned_set.contains_key(m) {
                continue;
            }
            can_trap = can_trap || self.path_trap_capable(m, bounds, trap_capable);
            heavy = heavy
                || matches!(
                    morph.op,
                    Operation::Map { .. }
                        | Operation::Fold { .. }
                        | Operation::Zip
                        | Operation::Enumerate
                        | Operation::Iota
                        | Operation::Fill
                        | Operation::Call(_)
                        | Operation::LoopEnter
                );
            if owned_via_unit.contains_key(m) {
                // The LoopEnter handle stands for the unit in `own`; internals
                // stay out, so subtraction, re-close and the consumers' gated
                // skip-sets never see them — the driver fires them.
                if matches!(morph.op, Operation::LoopEnter) {
                    own.push(m);
                }
                continue;
            }
            own.push(m);
        }
        GuardArm {
            value,
            edge,
            own,
            can_trap,
            heavy,
        }
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

/// Pair-fed primitives whose product source can disappear into the operation.
fn is_pair_primitive(op: Operation) -> bool {
    matches!(
        op,
        Operation::Add
            | Operation::Sub
            | Operation::Mul
            | Operation::Div
            | Operation::Mod
            | Operation::Eq
            | Operation::Neq
            | Operation::Lt
            | Operation::Gt
            | Operation::Le
            | Operation::Ge
            | Operation::And
            | Operation::Or
            | Operation::Phi
            | Operation::Index
    )
}

fn safe_integer_divisor(object: &crate::graph::Object) -> bool {
    if object.kind != ObjectKind::Constant {
        return false;
    }
    match object.value.as_ref() {
        Some(Value::I32(v)) => *v != 0 && *v != -1,
        Some(Value::I64(v)) => *v != 0 && *v != -1,
        Some(Value::U8(v)) => *v != 0,
        _ => false,
    }
}

fn increment(counts: &mut SecondaryMap<ObjectId, u32>, o: ObjectId) {
    if let Some(&n) = counts.get(o) {
        counts.insert(o, n + 1);
    }
}

/// The max value of an int width (the wraparound bound for range arithmetic).
/// Floats/products are not range-typed (None).
fn width_max(ty: &crate::ty::Ty) -> Option<u128> {
    use crate::ty::Ty;
    match ty {
        Ty::Int { bits: 32, .. } => Some(i32::MAX as u128),
        Ty::Int { bits: 64, .. } => Some(i64::MAX as u128),
        Ty::Int { bits: 8, .. } => Some(u8::MAX as u128),
        _ => None,
    }
}

/// Interval arithmetic over non-negative ranges, wrapping-width-aware: any
/// operation whose `hi` can exceed the int width's max is unknown (None) —
/// wrapping would invalidate the interval.
fn arith_range(op: Operation, a: Rng, b: Rng, wmax: Option<u128>) -> Option<Rng> {
    let (Rng::Int(al, ah), Rng::Int(bl, bh)) = (a, b) else {
        return None;
    };
    let w = wmax?;
    match op {
        Operation::Add => {
            let hi = ah as u128 + bh as u128;
            if hi > w {
                None
            } else {
                Some(Rng::Int(al + bl, hi as u64))
            }
        }
        Operation::Sub => {
            if al >= bh {
                Some(Rng::Int(al - bh, ah - bl))
            } else {
                None // could wrap negative
            }
        }
        Operation::Mul => {
            let hi = ah as u128 * bh as u128;
            if hi > w {
                None
            } else {
                Some(Rng::Int(al * bl, hi as u64))
            }
        }
        Operation::Div => {
            if bl >= 1 {
                Some(Rng::Int(al / bh.max(1), ah / bl))
            } else {
                Some(Rng::Int(0, ah))
            }
        }
        Operation::Mod => {
            if bh >= 1 {
                Some(Rng::Int(0, ah.min(bh - 1)))
            } else {
                None
            }
        }
        _ => None,
    }
}
