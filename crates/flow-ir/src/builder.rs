//! The invariant-enforcing builder (DESIGN §10) — the only way to construct a
//! [`CategoryIr`]. "Ill-formed is unconstructible": every public call either
//! produces a well-typed edge or returns the exact [`IrError`] of the §16
//! rejection matrix.
//!
//! Structure:
//! - [`IrBuilder`] holds the graph under construction; [`IrBuilder::seal`] runs
//!   the global checks (I4b, I5, I6, I-RET, unclosed loops) and freezes it.
//! - [`FnBuilder`] is scoped to one function; its primitives mint a fresh target
//!   object per call (except the two sanctioned existing-object targets: the
//!   Return object via `Dest::Ret`/`output`, and the LoopMerge via `loop_back`).
//! - Composite primitives (`binop`, `phi`, `index`, `call`, `map`, `fold`,
//!   `print`, `loop_back`, `loop_exit`) build their internal `Pair` packs
//!   **atomically** — callers cannot half-build them.

use slotmap::{SecondaryMap, SlotMap};

use crate::graph::{
    CategoryIr, FuncDef, FuncId, FuncKind, Morphism, MorphismId, Object, ObjectId, ObjectKind,
    Operation,
};
use crate::loc::SourceLoc;
use crate::ty::{self, Ty, Value};

#[cfg(test)]
mod tests;

/// Where a value-producing primitive writes its result (DESIGN §10).
#[derive(Clone, Debug, PartialEq)]
pub enum Dest {
    /// A fresh object, with an optional debug name.
    Fresh(Option<String>),
    /// The function's Return object; `slot: None` = full value, `Some(k)` = a
    /// `Pair{k, arity}` slot write (output ty must then be a product).
    Ret { slot: Option<u32> },
}

/// Every rejection-matrix row (DESIGN §10.1 / §16). `#[derive(... PartialEq)]`
/// and no `Display` (C3) — renderer-free, like flow-syntax diag values.
#[derive(Clone, Debug, PartialEq)]
pub enum IrError {
    /// Two `Named` functions declared with the same name.
    DuplicateName,
    /// A `FuncId` not in this builder, or wrong kind for the call site.
    UnknownFunction,
    /// An `ObjectId` not in this builder.
    UnknownObject,
    /// An object from a different function used in this one.
    WrongBuilder,
    /// `build_fn` called twice for the same function.
    AlreadyBuilt,
    /// A function declared but never `build_fn`'d, or its `FnBuilder` dropped
    /// without `finish`.
    UnfinishedFunction,
    /// `unop` with an op outside `{Neg, Not}`.
    NotUnary,
    /// `binop` with an op outside the §10 membership set.
    NotBinary,
    /// A typing-table mismatch (DESIGN §5.1).
    TypeMismatch {
        expected: Ty,
        found: Ty,
        loc: SourceLoc,
    },
    /// A ty outside the Core whitelist / arity floors (I9).
    NonCoreType,
    /// A ty deeper than `MAX_TY_DEPTH` (I10).
    TyTooDeep,
    /// `Proj`/slot write on a non-product ty.
    NotAProduct,
    /// A slot index `≥ arity`.
    SlotOutOfRange,
    /// A pack/call/etc. with the wrong number of components.
    ArityMismatch,
    /// ADR-0018: `enumerate` on an array whose size exceeds `i32::MAX`, so the
    /// index component (`i32`) cannot represent every index (F4/SND-3 precedent).
    EnumerateIndexOverflow,
    /// ADR-0029: an `iota`/`fill` count that is not a `Constant` integer > 0
    /// (a runtime size is ADR-0023 dynamic-array territory, out of Core).
    NonStaticCount,
    /// ADR-0029: source and target are not an allowed numeric widening pair.
    InvalidWiden,
    /// `pack`/`pack_array` of zero components.
    EmptyProduct,
    /// `pack` of exactly one component (no 1-tuples).
    SingletonTuple,
    /// A `Constant` whose `value.ty()` disagrees with a requested ty.
    ValueTyMismatch,
    /// I4: a token-bearing object gains a second token-bearing consumer outside
    /// the sanctioned loop fork.
    TokenNotLinear,
    /// I4b: a token-bearing object with no token-bearing out-edge that is not
    /// the Return object.
    TokenDropped,
    /// I4: a `Phi` whose selected ty contains `IoToken` (recursively).
    TokenInPhi,
    /// I4: a token inside a Map/Fold body (bodies must be token-free).
    TokenInBody,
    /// ADR-0027: a capture whose ty has no runtime representation (`Unit`, an
    /// erased-element array, or an all-erased product) — the body-input
    /// component would have no slot on the backends (LLVM panicked; the
    /// erased rule is L4). `IoToken` captures stay `TokenInBody`; `Str`
    /// captures stay `StrOutsidePrint` (via `pack`).
    ErasedCapture,
    /// §8: a loop whose carried `U` contains a token but a `LoopExit` payload
    /// does not (the token would die in the loop).
    TokenNotEscaping,
    /// I9s: `Str` outside a Constant ty or the `print()`-internal pair.
    StrOutsidePrint,
    /// Two `Struct` tys sharing a name but with different fields.
    StructNameConflict,
    /// `Call` targeting a non-`Named` function.
    CallToBody,
    /// `Map`/`Fold` body of the wrong `FuncKind`.
    BodyKindMismatch,
    /// I6: a cycle in the function reference graph.
    RecursiveCall { cycle: Vec<FuncId> },
    /// A `LoopHandle` dropped (or `finish`ed) without `end_loop`.
    OpenLoop,
    /// `end_loop` with zero `LoopBack` edges recorded.
    LoopWithoutBack,
    /// `end_loop` with zero `LoopExit` edges recorded.
    LoopWithoutExit,
    /// I5: a `LoopBack` whose source is not in the merge's SCC.
    LoopBackOutsideScc,
    /// I-RET: a function mixing full-value and slot Return writers.
    RetMixedWriters,
    /// I-RET: two slot writers to the same Return slot.
    RetSlotConflict,
    /// I-RET: a product Return with a missing slot.
    RetSlotMissing,
    /// `Dest::Ret { slot: Some(_) }` on a non-product output ty.
    RetNotProduct,
    /// A Return writer whose ty disagrees with the declared output ty.
    RetTypeMismatch,
    /// `seal` with no entry / a non-existent entry.
    NoEntry,
    /// `seal` whose entry is not `FuncKind::Named`.
    EntryNotNamed,
}

// --- internal under-construction representation ---------------------------

/// A function being assembled inside the builder (mutable until `finish`).
struct FuncUnderConstruction {
    def: FuncDef,
    built: bool,
    finished: bool,
    /// Open loop count: a `FnBuilder::finish` with this > 0 is `OpenLoop`.
    open_loops: u32,
    /// Per-merge `(back, exit)` edge tallies, so `end_loop` can verify ≥1 each
    /// while `loop_back`/`loop_exit` take `&LoopHandle` (counts live here).
    loop_counts: Vec<(ObjectId, u32, u32)>,
}

/// The graph under construction (DESIGN §10). Append-only; `seal` freezes it.
pub struct IrBuilder {
    objects: SlotMap<ObjectId, Object>,
    morphisms: SlotMap<MorphismId, Morphism>,
    out_edges: SecondaryMap<ObjectId, Vec<MorphismId>>,
    in_edges: SecondaryMap<ObjectId, Vec<MorphismId>>,
    owner: SecondaryMap<ObjectId, FuncId>,
    funcs: SlotMap<FuncId, FuncUnderConstruction>,
    /// Named-function name → FuncId, for duplicate detection. Insertion-ordered
    /// Vec (no HashMap; I12) — function counts are tiny.
    named: Vec<(String, FuncId)>,
}

impl Default for IrBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl IrBuilder {
    /// A fresh, empty builder.
    pub fn new() -> Self {
        IrBuilder {
            objects: SlotMap::with_key(),
            morphisms: SlotMap::with_key(),
            out_edges: SecondaryMap::new(),
            in_edges: SecondaryMap::new(),
            owner: SecondaryMap::new(),
            funcs: SlotMap::with_key(),
            named: Vec::new(),
        }
    }

    /// Declare a function before defining it (DESIGN §10). Calls may reference
    /// fns defined later in source order — acyclicity is a seal check.
    ///
    /// Creates the input Parameter and output Return objects (the latter's
    /// in-edges are filled during `build_fn`). Duplicate `Named` names are
    /// rejected. Both `input` and `output` tys go through Core intake (I9).
    pub fn declare(
        &mut self,
        kind: FuncKind,
        name: &str,
        input: Ty,
        output: Ty,
        loc: SourceLoc,
    ) -> Result<FuncId, IrError> {
        self.intake_ty(&input)?;
        self.intake_ty(&output)?;
        if kind == FuncKind::Named && self.named.iter().any(|(n, _)| n == name) {
            return Err(IrError::DuplicateName);
        }

        // Reserve the FuncId, then mint its input/output objects owned by it.
        let f = self.funcs.insert(FuncUnderConstruction {
            def: FuncDef {
                name: name.to_owned(),
                kind,
                input: ObjectId::default(),
                output: ObjectId::default(),
                morphisms: Vec::new(),
                loc,
            },
            built: false,
            finished: false,
            open_loops: 0,
            loop_counts: Vec::new(),
        });

        let input_obj = self.mint_object(f, input, None, ObjectKind::Parameter, None, loc);
        let output_obj = self.mint_object(f, output, None, ObjectKind::Return, None, loc);
        let fc = &mut self.funcs[f];
        fc.def.input = input_obj;
        fc.def.output = output_obj;

        if kind == FuncKind::Named {
            self.named.push((name.to_owned(), f));
        }
        Ok(f)
    }

    /// Begin building function `f`'s body (DESIGN §10). Once per function.
    pub fn build_fn(&mut self, f: FuncId) -> Result<FnBuilder<'_>, IrError> {
        let fc = self.funcs.get_mut(f).ok_or(IrError::UnknownFunction)?;
        if fc.built {
            return Err(IrError::AlreadyBuilt);
        }
        fc.built = true;
        Ok(FnBuilder { b: self, f })
    }

    /// Seal the graph (DESIGN §9/§10): run every global check and freeze.
    /// `entry` must be a `Named` function.
    pub fn seal(self, entry: FuncId) -> Result<CategoryIr, IrError> {
        // Entry checks.
        let entry_fc = self.funcs.get(entry).ok_or(IrError::NoEntry)?;
        if entry_fc.def.kind != FuncKind::Named {
            return Err(IrError::EntryNotNamed);
        }

        // Every declared function must have been built and finished.
        for (_, fc) in self.funcs.iter() {
            if !fc.built || !fc.finished {
                return Err(IrError::UnfinishedFunction);
            }
            if fc.open_loops > 0 {
                return Err(IrError::OpenLoop);
            }
        }

        // Materialize the CategoryIr so the SCC/topo algorithms can run; the
        // global checks read it, then we hand it back if clean. The func map is
        // converted FuncUnderConstruction → FuncDef preserving every FuncId key.
        let ir = CategoryIr {
            objects: self.objects,
            morphisms: self.morphisms,
            out_edges: self.out_edges,
            in_edges: self.in_edges,
            owner: self.owner,
            funcs: Self::into_func_defs(self.funcs),
            entry,
        };

        // Run the global graph-shape checks on the materialized graph.
        check_sealed(&ir)?;
        Ok(ir)
    }

    /// Convert the under-construction func map into the sealed `FuncDef` map,
    /// preserving every `FuncId` key.
    ///
    /// `SlotMap` re-issues identical keys when a fresh map is filled in the same
    /// insertion order with no intervening removals — which is exactly our case
    /// (v1 graphs are append-only). The `debug_assert_eq!` is the guard.
    fn into_func_defs(funcs: SlotMap<FuncId, FuncUnderConstruction>) -> SlotMap<FuncId, FuncDef> {
        let pairs: Vec<(FuncId, FuncDef)> = funcs.into_iter().map(|(k, fc)| (k, fc.def)).collect();
        let mut out: SlotMap<FuncId, FuncDef> = SlotMap::with_key();
        for (expected, def) in pairs {
            let got = out.insert(def);
            debug_assert_eq!(got, expected, "FuncId key not preserved across seal");
        }
        out
    }

    // --- shared object/edge minting (used by FnBuilder via &mut self) -----

    fn mint_object(
        &mut self,
        owner: FuncId,
        ty: Ty,
        value: Option<Value>,
        kind: ObjectKind,
        name: Option<String>,
        loc: SourceLoc,
    ) -> ObjectId {
        let id = self.objects.insert_with_key(|id| Object {
            id,
            ty,
            value,
            kind,
            name,
            loc,
        });
        self.owner.insert(id, owner);
        self.out_edges.insert(id, Vec::new());
        self.in_edges.insert(id, Vec::new());
        id
    }

    fn add_edge(
        &mut self,
        owner: FuncId,
        source: ObjectId,
        target: ObjectId,
        op: Operation,
        loc: SourceLoc,
    ) -> MorphismId {
        let m = self.morphisms.insert_with_key(|id| Morphism {
            id,
            source,
            target,
            op,
            loc,
        });
        self.out_edges[source].push(m);
        self.in_edges[target].push(m);
        self.funcs[owner].def.morphisms.push(m);
        m
    }

    /// Core type intake (I9 + I10): scalar whitelist, arity floors, depth guard.
    /// Runs on EVERY ty entering the builder, declared or synthesized.
    fn intake_ty(&self, ty: &Ty) -> Result<(), IrError> {
        if !ty::ty_depth_ok(ty) {
            return Err(IrError::TyTooDeep);
        }
        // Iterative structural check (no recursion; J1).
        let mut stack: Vec<&Ty> = vec![ty];
        while let Some(t) = stack.pop() {
            match t {
                Ty::Int { bits, signed } => {
                    let ok = matches!((bits, signed), (32, true) | (64, true) | (8, false));
                    if !ok {
                        return Err(IrError::NonCoreType);
                    }
                }
                Ty::Float { bits } => {
                    if *bits != 32 && *bits != 64 {
                        return Err(IrError::NonCoreType);
                    }
                }
                Ty::Bool | Ty::Unit | Ty::Str | Ty::IoToken => {}
                Ty::Tuple(ts) => {
                    if ts.len() < 2 {
                        return Err(IrError::NonCoreType);
                    }
                    for inner in ts {
                        stack.push(inner);
                    }
                }
                Ty::Struct { fields, .. } => {
                    // Field count ≥ 1 (mirrors Tuple arity ≥ 2 / Array size ≥ 1).
                    // A zero-field Struct is a product `1` whose literal mints a
                    // Temporary with zero in-edges — it seals but fails validate
                    // (BadInEdges). Reject it at intake so it is unconstructible
                    // anywhere (declare, pack_struct, constants; review TY-1).
                    if fields.is_empty() {
                        return Err(IrError::NonCoreType);
                    }
                    for (_, inner) in fields {
                        stack.push(inner);
                    }
                }
                Ty::Array { elem, size } => {
                    if *size < 1 {
                        return Err(IrError::NonCoreType);
                    }
                    stack.push(elem);
                }
            }
        }
        Ok(())
    }
}

// --- the seal-time global checks ------------------------------------------

/// Run the global graph-shape checks `seal` owns (DESIGN §9 ledger): I4b token
/// sinks, I5 per-edge SCC placement, I6 acyclicity + StructNameConflict, I-RET
/// completeness, and loop completeness.
fn check_sealed(ir: &CategoryIr) -> Result<(), IrError> {
    check_struct_name_conflict(ir)?;
    check_str(ir)?;
    check_reference_acyclic(ir)?;
    check_i_ret(ir)?;
    check_token_linearity(ir)?;
    check_token_sinks(ir)?;
    check_loops(ir)?;
    Ok(())
}

/// I9s graph-shape clause (DESIGN §9): `Str` appears only as a `Constant`
/// object's ty, or the second component of the `(IoToken, Str)` pair that
/// `print()` builds internally. The per-call packers already reject `Str`
/// components, but a `Str`-bearing *declared* input/output ty (a bare `Str`
/// param/return, or `Str` inside a tuple/struct/array) reaches the graph without
/// going through a packer — so `seal` must re-derive this graph-shape clause to
/// keep the builder's rejection set identical to `validate`'s (the headline
/// "seal Ok ⇒ validate empty" property; DESIGN §16 item 3). Mirrors
/// `validate::check_str` exactly.
fn check_str(ir: &CategoryIr) -> Result<(), IrError> {
    for (_, obj) in ir.objects() {
        match &obj.ty {
            // A bare Str object is legitimate only as a Constant (the literal).
            Ty::Str => {
                if obj.kind != ObjectKind::Constant {
                    return Err(IrError::StrOutsidePrint);
                }
            }
            // The print pair (IoToken, Str) is allowed; no other product may
            // contain Str.
            Ty::Tuple(ts) => {
                if ts.iter().any(ty::ty_contains_str) {
                    let is_print_pair = ts.len() == 2 && ts[0] == Ty::IoToken && ts[1] == Ty::Str;
                    if !is_print_pair {
                        return Err(IrError::StrOutsidePrint);
                    }
                }
            }
            other => {
                if ty::ty_contains_str(other) {
                    return Err(IrError::StrOutsidePrint);
                }
            }
        }
    }
    Ok(())
}

/// I4 (token linearity, DESIGN §8): a token-bearing object has at most one
/// token-bearing consumer, except the structural loop fork (exactly two `Pair`
/// consumers, one route into a `LoopBack` to a merge in the object's SCC, the
/// other into a `LoopExit` whose source is in that SCC). SCC-derived, no
/// `LoopHandle` knowledge.
fn check_token_linearity(ir: &CategoryIr) -> Result<(), IrError> {
    // Per-object SCC membership across all functions (each function's SCCs are
    // disjoint by ownership).
    let mut scc_of: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
    let mut base = 0usize;
    for (f, _) in ir.funcs() {
        let comps = ir.sccs(f);
        for (i, comp) in comps.iter().enumerate() {
            for &o in comp {
                scc_of.insert(o, base + i);
            }
        }
        base += comps.len();
    }

    for (id, obj) in ir.objects() {
        if !ty::ty_contains_token(&obj.ty) {
            continue;
        }
        let consumers: Vec<MorphismId> = ir
            .out_edges(id)
            .iter()
            .copied()
            .filter(|&m| {
                let tgt = ir.morphism(m).unwrap().target;
                ir.object(tgt)
                    .map(|o| ty::ty_contains_token(&o.ty))
                    .unwrap_or(false)
            })
            .collect();
        if consumers.len() > 1 && !is_loop_fork(ir, &scc_of, id, &consumers) {
            return Err(IrError::TokenNotLinear);
        }
    }
    Ok(())
}

/// The structural loop fork (DESIGN §8/I4): a token-bearing object inside a loop
/// SCC with exactly two token consumers (both `Pair` edges), one whose forward
/// cone reaches a `LoopBack` returning into the object's SCC, the other whose
/// cone reaches a `LoopExit` leaving the SCC. The two fire mutually exclusively
/// on the shared `Bool`.
///
/// Generalized beyond a single hop: when `U` is a product, the forking token is
/// packed into the loop-state object first, so the `LoopBack`/`LoopExit` is
/// several edges downstream. We classify each consumer by what its forward cone
/// reaches.
fn is_loop_fork(
    ir: &CategoryIr,
    scc_of: &SecondaryMap<ObjectId, usize>,
    obj: ObjectId,
    consumers: &[MorphismId],
) -> bool {
    if consumers.len() != 2 {
        return false;
    }
    let obj_scc = match scc_of.get(obj) {
        Some(&s) => s,
        None => return false,
    };
    let mut has_back = false;
    let mut has_exit = false;
    for &mid in consumers {
        let m = ir.morphism(mid).unwrap();
        if !matches!(m.op, Operation::Pair { .. }) {
            return false;
        }
        match cone_reaches(ir, scc_of, m.target, obj_scc) {
            ConeKind::Back => has_back = true,
            ConeKind::Exit => has_exit = true,
            ConeKind::Neither => {}
        }
    }
    has_back && has_exit
}

/// What a forward cone from `start` reaches w.r.t. the loop SCC `scc`.
enum ConeKind {
    /// Reaches a `LoopBack` returning into `scc`.
    Back,
    /// Reaches a `LoopExit` leaving `scc`.
    Exit,
    Neither,
}

/// Classify the forward cone from `start` (iterative BFS; J1). `Back` if it hits
/// a `LoopBack` into `scc`; `Exit` if it hits a `LoopExit` leaving `scc`; the
/// back classification wins if both are somehow reachable (the back route is the
/// in-SCC path).
fn cone_reaches(
    ir: &CategoryIr,
    scc_of: &SecondaryMap<ObjectId, usize>,
    start: ObjectId,
    scc: usize,
) -> ConeKind {
    let mut seen: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
    let mut queue: Vec<ObjectId> = vec![start];
    seen.insert(start, true);
    let mut found_exit = false;
    while let Some(o) = queue.pop() {
        for &m in ir.out_edges(o) {
            let mm = ir.morphism(m).unwrap();
            match mm.op {
                Operation::LoopBack if scc_of.get(mm.target) == Some(&scc) => {
                    return ConeKind::Back;
                }
                Operation::LoopExit if scc_of.get(mm.target) != Some(&scc) => {
                    found_exit = true;
                }
                _ => {}
            }
            let t = mm.target;
            // Don't traverse past a LoopBack into the SCC (handled above) or out
            // through a LoopExit; just continue the forward walk for packs.
            if !seen.get(t).copied().unwrap_or(false) {
                seen.insert(t, true);
                queue.push(t);
            }
        }
    }
    if found_exit {
        ConeKind::Exit
    } else {
        ConeKind::Neither
    }
}

/// I6 / StructNameConflict: all `Struct` tys sharing a name must have identical
/// fields across the whole graph.
fn check_struct_name_conflict(ir: &CategoryIr) -> Result<(), IrError> {
    // Insertion-ordered (name → first fields) registry; no HashMap (I12).
    let mut seen: Vec<(String, Vec<(String, Ty)>)> = Vec::new();
    for (_, obj) in ir.objects() {
        collect_struct_defs(&obj.ty, &mut seen)?;
    }
    Ok(())
}

/// Walk a ty (iterative; J1), recording each `Struct`'s fields and flagging a
/// name reused with different fields.
fn collect_struct_defs(
    ty: &Ty,
    seen: &mut Vec<(String, Vec<(String, Ty)>)>,
) -> Result<(), IrError> {
    let mut stack: Vec<&Ty> = vec![ty];
    while let Some(t) = stack.pop() {
        match t {
            Ty::Struct { name, fields } => {
                if let Some((_, prev)) = seen.iter().find(|(n, _)| n == name) {
                    if prev != fields {
                        return Err(IrError::StructNameConflict);
                    }
                } else {
                    seen.push((name.clone(), fields.clone()));
                }
                for (_, inner) in fields {
                    stack.push(inner);
                }
            }
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push(inner);
                }
            }
            Ty::Array { elem, .. } => stack.push(elem),
            _ => {}
        }
    }
    Ok(())
}

/// I6: the reference graph (Call edges + Map/Fold body refs) is acyclic.
/// Iterative DFS with an explicit stack (J1); reports the cycle on detection.
fn check_reference_acyclic(ir: &CategoryIr) -> Result<(), IrError> {
    // Build a reference adjacency: f → [referenced funcs], in deterministic
    // order (over the function's morphisms).
    let func_ids: Vec<FuncId> = ir.funcs().map(|(id, _)| id).collect();

    // 0 = unvisited, 1 = on stack (gray), 2 = done (black).
    let mut color: SecondaryMap<FuncId, u8> = SecondaryMap::new();
    for &f in &func_ids {
        color.insert(f, 0);
    }

    fn refs_of(ir: &CategoryIr, f: FuncId) -> Vec<FuncId> {
        let mut out = Vec::new();
        if let Some(def) = ir.func(f) {
            for &m in &def.morphisms {
                if let Some(morph) = ir.morphism(m) {
                    match morph.op {
                        Operation::Call(g)
                        | Operation::Map { body: g, .. }
                        | Operation::Fold { body: g, .. } => out.push(g),
                        _ => {}
                    }
                }
            }
        }
        out
    }

    for &root in &func_ids {
        if color[root] != 0 {
            continue;
        }
        // path holds the gray stack for cycle reconstruction.
        let mut path: Vec<FuncId> = Vec::new();
        struct Frame {
            node: FuncId,
            refs: Vec<FuncId>,
            cursor: usize,
        }
        let mut stack: Vec<Frame> = vec![Frame {
            node: root,
            refs: refs_of(ir, root),
            cursor: 0,
        }];
        color.insert(root, 1);
        path.push(root);

        while let Some(frame) = stack.last_mut() {
            if frame.cursor < frame.refs.len() {
                let w = frame.refs[frame.cursor];
                frame.cursor += 1;
                match color.get(w).copied().unwrap_or(2) {
                    0 => {
                        color.insert(w, 1);
                        path.push(w);
                        let refs = refs_of(ir, w);
                        stack.push(Frame {
                            node: w,
                            refs,
                            cursor: 0,
                        });
                    }
                    1 => {
                        // Back edge → cycle. Slice path from w to the end.
                        let start = path.iter().position(|&x| x == w).unwrap_or(0);
                        let cycle = path[start..].to_vec();
                        return Err(IrError::RecursiveCall { cycle });
                    }
                    _ => {}
                }
            } else {
                color.insert(frame.node, 2);
                path.pop();
                stack.pop();
            }
        }
    }
    Ok(())
}

/// I-RET completeness (DESIGN §9): every function's Return object has a complete
/// writer set — either ≥1 full-value writer, or all `arity` distinct slots, and
/// never mixed. Unit returns may have zero writers.
fn check_i_ret(ir: &CategoryIr) -> Result<(), IrError> {
    for (_, def) in ir.funcs() {
        let ret = def.output;
        let out_ty = ir.object(ret).map(|o| &o.ty).cloned().unwrap_or(Ty::Unit);
        let mut full = 0usize;
        let mut slots: Vec<u32> = Vec::new();
        for &m in ir.in_edges(ret) {
            let morph = ir.morphism(m).expect("ret in-edge");
            match morph.op {
                Operation::Pair { slot, .. } => slots.push(slot),
                _ => full += 1,
            }
        }
        if full > 0 && !slots.is_empty() {
            return Err(IrError::RetMixedWriters);
        }
        if !slots.is_empty() {
            let arity = match out_ty.product_arity() {
                Some(a) => a,
                None => return Err(IrError::RetNotProduct),
            };
            slots.sort_unstable();
            for w in slots.windows(2) {
                if w[0] == w[1] {
                    return Err(IrError::RetSlotConflict);
                }
            }
            // Completeness against the true `u64` arity without truncation: the
            // deduplicated `u32` slots must be exactly `0..arity` (F4/SND-3).
            if (slots.len() as u64) != arity {
                return Err(IrError::RetSlotMissing);
            }
            for (k, &slot) in slots.iter().enumerate() {
                if slot as u64 != k as u64 {
                    return Err(IrError::RetSlotMissing);
                }
            }
        } else if full == 0 && out_ty != Ty::Unit {
            // No writers and a non-Unit output: a missing return.
            return Err(IrError::RetSlotMissing);
        }
    }
    Ok(())
}

/// I4b (token sink): every token-bearing object with zero token-bearing
/// out-edges must be its function's Return object.
fn check_token_sinks(ir: &CategoryIr) -> Result<(), IrError> {
    for (id, obj) in ir.objects() {
        if !ty::ty_contains_token(&obj.ty) {
            continue;
        }
        let has_token_consumer = ir.out_edges(id).iter().any(|&m| {
            let tgt = ir.morphism(m).unwrap().target;
            ir.object(tgt)
                .map(|o| ty::ty_contains_token(&o.ty))
                .unwrap_or(false)
        });
        if has_token_consumer {
            continue;
        }
        // No token-bearing out-edge: must be the owning function's Return.
        let owner = ir.owner(id);
        let is_return = ir.func(owner).map(|d| d.output) == Some(id);
        if !is_return {
            return Err(IrError::TokenDropped);
        }
    }
    Ok(())
}

/// I5 (per-edge SCC placement) + loop completeness + the loop-carried-token
/// escape rule (DESIGN §7/§8). Runs per LoopMerge using the function's SCCs.
fn check_loops(ir: &CategoryIr) -> Result<(), IrError> {
    for (f, _) in ir.funcs() {
        // Membership lookup: object → its SCC index (for this function).
        let comps = ir.sccs(f);
        let mut scc_of: SecondaryMap<ObjectId, usize> = SecondaryMap::new();
        for (i, comp) in comps.iter().enumerate() {
            for &o in comp {
                scc_of.insert(o, i);
            }
        }
        let nontrivial = |i: usize| -> bool {
            comps[i].len() > 1
                || comps[i]
                    .first()
                    .map(|&o| {
                        ir.out_edges(o)
                            .iter()
                            .any(|&m| ir.morphism(m).unwrap().target == o)
                    })
                    .unwrap_or(false)
        };

        for (id, obj) in ir.objects() {
            if ir.owner(id) != f || obj.kind != ObjectKind::LoopMerge {
                continue;
            }
            let merge_scc = scc_of[id];
            if !nontrivial(merge_scc) {
                // A LoopMerge whose SCC is trivial means no back edge closes the
                // cycle: treat as a missing back edge.
                return Err(IrError::LoopWithoutBack);
            }

            let mut backs = 0usize;
            let mut exits = 0usize;
            // Token-escape bookkeeping: does U carry a token?
            let u_has_token = ty::ty_contains_token(&obj.ty);

            // LoopEnter source must be OUTSIDE the SCC; LoopBack sources INSIDE.
            for &m in ir.in_edges(id) {
                let morph = ir.morphism(m).unwrap();
                match morph.op {
                    // Enter from inside the cycle — malformed.
                    Operation::LoopEnter if scc_of.get(morph.source) == Some(&merge_scc) => {
                        return Err(IrError::LoopBackOutsideScc);
                    }
                    Operation::LoopBack => {
                        backs += 1;
                        // I5 is a per-edge check on the carried STATE, not on the
                        // route object. The route `(next_state, cond)` is always
                        // pulled into the SCC by its `cond` slot edge (the guard
                        // is genuinely in the cycle), so testing the route's own
                        // SCC membership would pass even when `next_state` came
                        // from outside the loop (e.g. a constant). Test the
                        // route's slot-0 `Pair` source — the next_state object —
                        // instead (DESIGN §5.1 LoopBack row; §9 I5).
                        let state = loopback_state_source(ir, morph.source);
                        match state {
                            Some(s) if scc_of.get(s) == Some(&merge_scc) => {}
                            _ => return Err(IrError::LoopBackOutsideScc),
                        }
                    }
                    _ => {}
                }
            }
            if backs == 0 {
                return Err(IrError::LoopWithoutBack);
            }

            // LoopExit edges belonging to this merge: the exit's source is a
            // route object `(B, Bool)` packed from a value derived *inside* the
            // SCC (the route itself is a downward leaf, not in the cycle). A
            // LoopExit belongs to this loop iff its route has a `Pair` in-edge
            // whose source is in this SCC, and the exit target is outside.
            for (_, morph) in ir.morphisms() {
                if morph.op != Operation::LoopExit {
                    continue;
                }
                if !exit_reachable_from_merge(ir, id, morph.source) {
                    continue;
                }
                if scc_of.get(morph.target) == Some(&merge_scc) {
                    // Exit target inside the cycle — not a real exit.
                    continue;
                }
                exits += 1;
                if u_has_token {
                    // token-in ⇒ token-out: the exit payload B must carry it.
                    let b_ty = &ir.object(morph.target).unwrap().ty;
                    if !ty::ty_contains_token(b_ty) {
                        return Err(IrError::TokenNotEscaping);
                    }
                }
            }
            if exits == 0 {
                return Err(IrError::LoopWithoutExit);
            }
        }
    }
    Ok(())
}

// --- LoopHandle -----------------------------------------------------------

/// A handle to an in-progress loop (DESIGN §10). Half-built loops cannot escape:
/// the merge object is reachable only through this handle until `end_loop`.
///
/// Back/exit edge counts are tracked in the builder (keyed by `merge`), not on
/// the handle, so `loop_back`/`loop_exit` take `&LoopHandle` exactly per §10.
#[derive(Debug)]
pub struct LoopHandle {
    merge: ObjectId,
    u_ty: Ty,
}

// --- FnBuilder ------------------------------------------------------------

/// A builder scoped to one function (DESIGN §10). Borrows the [`IrBuilder`].
pub struct FnBuilder<'a> {
    b: &'a mut IrBuilder,
    f: FuncId,
}

impl FnBuilder<'_> {
    /// The function's input Parameter object.
    pub fn input(&self) -> ObjectId {
        self.b.funcs[self.f].def.input
    }

    fn output_id(&self) -> ObjectId {
        self.b.funcs[self.f].def.output
    }

    fn output_ty(&self) -> Ty {
        self.b.objects[self.output_id()].ty.clone()
    }

    /// Resolve an object id, checking it exists and is owned by this function.
    fn check_obj(&self, id: ObjectId) -> Result<(), IrError> {
        if !self.b.objects.contains_key(id) {
            return Err(IrError::UnknownObject);
        }
        if self.b.owner.get(id) != Some(&self.f) {
            return Err(IrError::WrongBuilder);
        }
        Ok(())
    }

    fn ty_of(&self, id: ObjectId) -> Ty {
        self.b.objects[id].ty.clone()
    }

    /// A Constant source object (DESIGN §10); `ty := v.ty()` (I7).
    pub fn constant(&mut self, v: Value, loc: SourceLoc) -> Result<ObjectId, IrError> {
        let ty = v.ty();
        self.b.intake_ty(&ty)?;
        Ok(self
            .b
            .mint_object(self.f, ty, Some(v), ObjectKind::Constant, None, loc))
    }

    /// Resolve a `Dest` to a target object for a primitive producing `value_ty`,
    /// returning the object the caller should treat as "the result" plus the
    /// morphism wiring already done. The primitive's defining edge targets the
    /// returned object (for `Fresh`/`Ret{None}`) or a fresh component which then
    /// gets a `Pair` slot edge into Return (for `Ret{Some}`).
    ///
    /// Returns the object the produced value *lives in* (so callers can keep
    /// using it). For `Ret{Some(k)}` that is the fresh component object.
    fn dest_target(
        &mut self,
        dest: &Dest,
        value_ty: &Ty,
        loc: SourceLoc,
    ) -> Result<DestPlan, IrError> {
        match dest {
            Dest::Fresh(name) => {
                let obj = self.b.mint_object(
                    self.f,
                    value_ty.clone(),
                    None,
                    ObjectKind::Temporary,
                    name.clone(),
                    loc,
                );
                Ok(DestPlan::Direct(obj))
            }
            Dest::Ret { slot: None } => {
                let ret = self.output_id();
                let out_ty = self.output_ty();
                if &out_ty != value_ty {
                    return Err(IrError::RetTypeMismatch);
                }
                Ok(DestPlan::Direct(ret))
            }
            Dest::Ret { slot: Some(k) } => {
                let out_ty = self.output_ty();
                if out_ty.product_arity().is_none() {
                    return Err(IrError::RetNotProduct);
                }
                // u64 arity capped to u32: an array of size > u32::MAX is not
                // slot-addressable (the `Pair { arity: u32 }` field can't name
                // it) — reject the slot write rather than truncate (F4/SND-3).
                let arity = out_ty.product_arity_u32().ok_or(IrError::SlotOutOfRange)?;
                if *k >= arity {
                    return Err(IrError::SlotOutOfRange);
                }
                let comp_ty = out_ty.component_ty(*k).unwrap().clone();
                if &comp_ty != value_ty {
                    return Err(IrError::RetTypeMismatch);
                }
                // Fresh component object holds the value; a Pair slot edge wires
                // it into Return.
                let comp = self.b.mint_object(
                    self.f,
                    value_ty.clone(),
                    None,
                    ObjectKind::Temporary,
                    None,
                    loc,
                );
                let ret = self.output_id();
                Ok(DestPlan::Slot {
                    comp,
                    ret,
                    slot: *k,
                    arity,
                })
            }
        }
    }

    /// Wire a primitive's defining edge to the dest plan's target, returning the
    /// object the produced value lives in.
    fn emit_to_dest(
        &mut self,
        plan: DestPlan,
        source: ObjectId,
        op: Operation,
        loc: SourceLoc,
    ) -> ObjectId {
        match plan {
            DestPlan::Direct(tgt) => {
                self.b.add_edge(self.f, source, tgt, op, loc);
                tgt
            }
            DestPlan::Slot {
                comp,
                ret,
                slot,
                arity,
            } => {
                self.b.add_edge(self.f, source, comp, op, loc);
                self.b
                    .add_edge(self.f, comp, ret, Operation::Pair { slot, arity }, loc);
                comp
            }
        }
    }

    /// `π_index` out of a Tuple/Struct (DESIGN §5.1).
    pub fn proj(
        &mut self,
        src: ObjectId,
        index: u32,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(src)?;
        let src_ty = self.ty_of(src);
        let comp = match &src_ty {
            Ty::Tuple(_) | Ty::Struct { .. } => src_ty
                .component_ty(index)
                .ok_or(IrError::SlotOutOfRange)?
                .clone(),
            _ => return Err(IrError::NotAProduct),
        };
        self.b.intake_ty(&comp)?;
        let plan = self.dest_target(&dest, &comp, loc)?;
        let out = self.emit_to_dest(plan, src, Operation::Proj { index }, loc);
        Ok(out)
    }

    /// Build a product object by packing `components` with `Pair` slot edges.
    /// `target_ty` is the (already-intaken) product ty; checks per-slot tys and
    /// I9s `Str` restriction. Returns the product object.
    fn build_pack(
        &mut self,
        components: &[ObjectId],
        target_ty: Ty,
        dest: Dest,
        loc: SourceLoc,
        allow_str_components: bool,
    ) -> Result<ObjectId, IrError> {
        let arity = components.len() as u32;
        // Per-component ty checks + Str restriction.
        for (k, &c) in components.iter().enumerate() {
            self.check_obj(c)?;
            let cty = self.ty_of(c);
            let expected = target_ty.component_ty(k as u32).unwrap().clone();
            if cty != expected {
                return Err(IrError::TypeMismatch {
                    expected,
                    found: cty,
                    loc,
                });
            }
            if !allow_str_components && ty::ty_contains_str(&cty) {
                return Err(IrError::StrOutsidePrint);
            }
        }
        self.b.intake_ty(&target_ty)?;
        // Choose the product object per dest.
        let plan = self.dest_target(&dest, &target_ty, loc)?;
        let product = match plan {
            DestPlan::Direct(tgt) => tgt,
            DestPlan::Slot { .. } => {
                // Packing into a Ret slot: the pack target is a fresh component,
                // then a Pair edge wires it into Return. We need the product to
                // exist as one object; emit pack into the fresh comp.
                // emit_to_dest expects a single defining edge, but a pack has
                // `arity` defining edges. Handle slot-dest packs explicitly.
                return self.build_pack_into_slot(components, target_ty, dest, loc);
            }
        };
        for (k, &c) in components.iter().enumerate() {
            self.b.add_edge(
                self.f,
                c,
                product,
                Operation::Pair {
                    slot: k as u32,
                    arity,
                },
                loc,
            );
        }
        Ok(product)
    }

    /// Pack components into a fresh product object, then `Pair`-write that
    /// object into a Return slot (the `Dest::Ret{Some(k)}` pack case).
    fn build_pack_into_slot(
        &mut self,
        components: &[ObjectId],
        target_ty: Ty,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let arity = components.len() as u32;
        let slot = match dest {
            Dest::Ret { slot: Some(k) } => k,
            _ => unreachable!("build_pack_into_slot only for Ret slot dest"),
        };
        let out_ty = self.output_ty();
        if out_ty.product_arity().is_none() {
            return Err(IrError::RetNotProduct);
        }
        // u64 arity capped to u32: a > u32::MAX-arity product is not
        // slot-addressable (F4/SND-3).
        let ret_arity = out_ty.product_arity_u32().ok_or(IrError::SlotOutOfRange)?;
        if slot >= ret_arity {
            return Err(IrError::SlotOutOfRange);
        }
        let comp_ty = out_ty.component_ty(slot).unwrap().clone();
        if comp_ty != target_ty {
            return Err(IrError::RetTypeMismatch);
        }
        let product = self.b.mint_object(
            self.f,
            target_ty.clone(),
            None,
            ObjectKind::Temporary,
            None,
            loc,
        );
        for (k, &c) in components.iter().enumerate() {
            self.b.add_edge(
                self.f,
                c,
                product,
                Operation::Pair {
                    slot: k as u32,
                    arity,
                },
                loc,
            );
        }
        let ret = self.output_id();
        self.b.add_edge(
            self.f,
            product,
            ret,
            Operation::Pair {
                slot,
                arity: ret_arity,
            },
            loc,
        );
        Ok(product)
    }

    /// Tuple literal (DESIGN §10): `len ≥ 2` (`0`→`EmptyProduct`, `1`→`SingletonTuple`).
    pub fn pack(
        &mut self,
        components: &[ObjectId],
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        if components.is_empty() {
            return Err(IrError::EmptyProduct);
        }
        if components.len() == 1 {
            return Err(IrError::SingletonTuple);
        }
        // Synthesize the Tuple ty from components.
        let mut tys = Vec::with_capacity(components.len());
        for &c in components {
            self.check_obj(c)?;
            tys.push(self.ty_of(c));
        }
        let target = Ty::Tuple(tys);
        self.build_pack(components, target, dest, loc, false)
    }

    /// Struct literal (DESIGN §10): `ty` must be `Ty::Struct`; `components` in
    /// field order; `len == field count`.
    pub fn pack_struct(
        &mut self,
        ty: Ty,
        components: &[ObjectId],
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let fields = match &ty {
            Ty::Struct { fields, .. } => fields.clone(),
            _ => return Err(IrError::NotAProduct),
        };
        // Defense in depth (mirrors `pack`): a zero-component struct literal would
        // mint a Temporary with zero in-edges. `intake_ty` already rejects a
        // zero-field `Ty::Struct` as `NonCoreType` (so this is unreachable via a
        // well-typed `(ty, components)` pair where the arities agree), but the
        // `EmptyProduct` guard makes the empty-product rejection local and explicit
        // here too, independent of the arity-vs-ty agreement (review TY-1).
        if components.is_empty() {
            return Err(IrError::EmptyProduct);
        }
        if components.len() != fields.len() {
            return Err(IrError::ArityMismatch);
        }
        self.b.intake_ty(&ty)?;
        self.build_pack(components, ty, dest, loc, false)
    }

    /// Array literal (DESIGN §10): `size = len ≥ 1`, all elem tys equal.
    pub fn pack_array(
        &mut self,
        components: &[ObjectId],
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        if components.is_empty() {
            return Err(IrError::EmptyProduct);
        }
        for &c in components {
            self.check_obj(c)?;
        }
        let elem = self.ty_of(components[0]);
        for &c in &components[1..] {
            let cty = self.ty_of(c);
            if cty != elem {
                return Err(IrError::TypeMismatch {
                    expected: elem.clone(),
                    found: cty,
                    loc,
                });
            }
        }
        let target = Ty::Array {
            elem: Box::new(elem),
            size: components.len() as u64,
        };
        self.build_pack(components, target, dest, loc, false)
    }

    /// Unary op (DESIGN §5.1): `{Neg, Not}` only.
    pub fn unop(
        &mut self,
        op: Operation,
        x: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(x)?;
        let xty = self.ty_of(x);
        let out_ty = match op {
            Operation::Neg => {
                if !xty.is_numeric() {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::i32(),
                        found: xty,
                        loc,
                    });
                }
                xty.clone()
            }
            Operation::Not => {
                if xty != Ty::Bool {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::Bool,
                        found: xty,
                        loc,
                    });
                }
                Ty::Bool
            }
            _ => return Err(IrError::NotUnary),
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, x, op, loc))
    }

    /// Binary op (DESIGN §5.1 / §10): the §10 membership set; packs `(lhs, rhs)`
    /// then applies the primitive (Pair-then-primitive, atomic).
    pub fn binop(
        &mut self,
        op: Operation,
        lhs: ObjectId,
        rhs: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(lhs)?;
        self.check_obj(rhs)?;
        let lty = self.ty_of(lhs);
        let rty = self.ty_of(rhs);

        // Per-op typing dispatch (explicit membership; not enum-order range).
        let out_ty = match op {
            Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
                if !lty.is_numeric() {
                    return Err(IrError::TypeMismatch {
                        expected: lty.clone(),
                        found: lty.clone(),
                        loc,
                    });
                }
                if lty != rty {
                    return Err(IrError::TypeMismatch {
                        expected: lty,
                        found: rty,
                        loc,
                    });
                }
                lty.clone()
            }
            Operation::Eq | Operation::Neq => {
                // A ∈ N ∪ {Bool}.
                let ok = lty.is_numeric() || lty == Ty::Bool;
                if !ok {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::Bool,
                        found: lty.clone(),
                        loc,
                    });
                }
                if lty != rty {
                    return Err(IrError::TypeMismatch {
                        expected: lty,
                        found: rty,
                        loc,
                    });
                }
                Ty::Bool
            }
            Operation::Lt | Operation::Gt | Operation::Le | Operation::Ge => {
                if !lty.is_numeric() {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::i32(),
                        found: lty.clone(),
                        loc,
                    });
                }
                if lty != rty {
                    return Err(IrError::TypeMismatch {
                        expected: lty,
                        found: rty,
                        loc,
                    });
                }
                Ty::Bool
            }
            Operation::And | Operation::Or => {
                if lty != Ty::Bool {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::Bool,
                        found: lty,
                        loc,
                    });
                }
                if rty != Ty::Bool {
                    return Err(IrError::TypeMismatch {
                        expected: Ty::Bool,
                        found: rty,
                        loc,
                    });
                }
                Ty::Bool
            }
            _ => return Err(IrError::NotBinary),
        };

        // Str may never be a binop operand (I9s) — none of these accept Str
        // anyway, but guard explicitly for nested cases.
        if ty::ty_contains_str(&lty) || ty::ty_contains_str(&rty) {
            return Err(IrError::StrOutsidePrint);
        }

        // Build the (lhs, rhs) 2-tuple internally, then apply.
        let pair_ty = Ty::Tuple(vec![lty.clone(), rty.clone()]);
        self.b.intake_ty(&pair_ty)?;
        let pair = self
            .b
            .mint_object(self.f, pair_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            lhs,
            pair,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            rhs,
            pair,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );

        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, pair, op, loc))
    }

    /// `Phi` (DESIGN §5.1): `(T, T, Bool) → T`; `T` must not contain `IoToken`.
    pub fn phi(
        &mut self,
        t: ObjectId,
        f: ObjectId,
        cond: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(t)?;
        self.check_obj(f)?;
        self.check_obj(cond)?;
        let tty = self.ty_of(t);
        let fty = self.ty_of(f);
        let cty = self.ty_of(cond);
        if tty != fty {
            return Err(IrError::TypeMismatch {
                expected: tty,
                found: fty,
                loc,
            });
        }
        if cty != Ty::Bool {
            return Err(IrError::TypeMismatch {
                expected: Ty::Bool,
                found: cty,
                loc,
            });
        }
        if ty::ty_contains_token(&tty) {
            return Err(IrError::TokenInPhi);
        }
        if ty::ty_contains_str(&tty) {
            return Err(IrError::StrOutsidePrint);
        }
        let triple_ty = Ty::Tuple(vec![tty.clone(), fty.clone(), Ty::Bool]);
        self.b.intake_ty(&triple_ty)?;
        let triple = self
            .b
            .mint_object(self.f, triple_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            t,
            triple,
            Operation::Pair { slot: 0, arity: 3 },
            loc,
        );
        self.b.add_edge(
            self.f,
            f,
            triple,
            Operation::Pair { slot: 1, arity: 3 },
            loc,
        );
        self.b.add_edge(
            self.f,
            cond,
            triple,
            Operation::Pair { slot: 2, arity: 3 },
            loc,
        );
        let plan = self.dest_target(&dest, &tty, loc)?;
        Ok(self.emit_to_dest(plan, triple, Operation::Phi, loc))
    }

    /// `Index` (DESIGN §5.1): `(Array{T,n}, I) → T`; `I` a Core integer scalar.
    pub fn index(
        &mut self,
        arr: ObjectId,
        idx: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(arr)?;
        self.check_obj(idx)?;
        let aty = self.ty_of(arr);
        let ity = self.ty_of(idx);
        let elem = match &aty {
            Ty::Array { elem, .. } => (**elem).clone(),
            _ => return Err(IrError::NotAProduct),
        };
        if !ity.is_integer() {
            return Err(IrError::TypeMismatch {
                expected: Ty::i32(),
                found: ity,
                loc,
            });
        }
        let pair_ty = Ty::Tuple(vec![aty.clone(), ity.clone()]);
        self.b.intake_ty(&pair_ty)?;
        let pair = self
            .b
            .mint_object(self.f, pair_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            arr,
            pair,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            idx,
            pair,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        self.b.intake_ty(&elem)?;
        let plan = self.dest_target(&dest, &elem, loc)?;
        Ok(self.emit_to_dest(plan, pair, Operation::Index, loc))
    }

    /// `Zip` (DESIGN §5.1; ADR-0018): `([A; n], [B; n]) → [(A, B); n]`. Builds the
    /// internal 2-tuple of the two arrays (Pair-then-primitive, exactly as
    /// `binop`), then applies `Zip`. Both arrays must share size `n`.
    pub fn zip(
        &mut self,
        lhs: ObjectId,
        rhs: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(lhs)?;
        self.check_obj(rhs)?;
        let lty = self.ty_of(lhs);
        let rty = self.ty_of(rhs);
        let (a_elem, a_size) = match &lty {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => return Err(IrError::NotAProduct),
        };
        let (b_elem, b_size) = match &rty {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => return Err(IrError::NotAProduct),
        };
        if a_size != b_size {
            return Err(IrError::TypeMismatch {
                expected: lty.clone(),
                found: rty.clone(),
                loc,
            });
        }
        // Str never lives in a product/array (I9s) — such an array is already
        // unconstructible, but guard explicitly like `binop` for nested cases.
        if ty::ty_contains_str(&lty) || ty::ty_contains_str(&rty) {
            return Err(IrError::StrOutsidePrint);
        }
        // Internal (lhs, rhs) 2-tuple of the two arrays, then Zip.
        let pair_ty = Ty::Tuple(vec![lty.clone(), rty.clone()]);
        self.b.intake_ty(&pair_ty)?;
        let pair = self
            .b
            .mint_object(self.f, pair_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            lhs,
            pair,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            rhs,
            pair,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        let out_ty = Ty::Array {
            elem: Box::new(Ty::Tuple(vec![a_elem, b_elem])),
            size: a_size,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, pair, Operation::Zip, loc))
    }

    /// `Enumerate` (DESIGN §5.1; ADR-0018): `[A; n] → [(i32, A); n]`. Single
    /// source (direct edge, no internal pair). Rejects `n > i32::MAX` up front —
    /// the index `i32` could not name every element (F4/SND-3 precedent).
    pub fn enumerate(
        &mut self,
        arr: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(arr)?;
        let aty = self.ty_of(arr);
        let (elem, size) = match &aty {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => return Err(IrError::NotAProduct),
        };
        if size > i32::MAX as u64 {
            return Err(IrError::EnumerateIndexOverflow);
        }
        if ty::ty_contains_str(&aty) {
            return Err(IrError::StrOutsidePrint);
        }
        let out_ty = Ty::Array {
            elem: Box::new(Ty::Tuple(vec![Ty::i32(), elem])),
            size,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, arr, Operation::Enumerate, loc))
    }

    /// `Widen` (ADR-0029): one of `i32→i64`, `i32→f32`, `i32→f64`, or
    /// `f32→f64`. Direct unary edge; pure and trap-free.
    pub fn widen(
        &mut self,
        src: ObjectId,
        target: Ty,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(src)?;
        let source = self.ty_of(src);
        if !matches!(
            (&source, &target),
            (
                Ty::Int {
                    bits: 32,
                    signed: true
                },
                Ty::Int {
                    bits: 64,
                    signed: true
                } | Ty::Float { bits: 32 | 64 }
            ) | (Ty::Float { bits: 32 }, Ty::Float { bits: 64 })
        ) {
            return Err(IrError::InvalidWiden);
        }
        self.b.intake_ty(&target)?;
        let plan = self.dest_target(&dest, &target, loc)?;
        Ok(self.emit_to_dest(plan, src, Operation::Widen, loc))
    }

    /// `Iota` (ADR-0029): `[i32; n]` of `0..n-1`. The count rides as a
    /// `Constant` integer source object so the static-n rule is a graph
    /// property (validate's `IotaCountMismatch`). Rejects a non-`Constant` or
    /// non-integer count (`NonStaticCount` — a runtime size is ADR-0023
    /// territory, out of Core) and `n == 0` / `n > i32::MAX` (the element type
    /// is pinned `i32` in v1; `Ty::Array` requires size ≥ 1).
    pub fn iota(
        &mut self,
        count: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let n = self.static_count(count)?;
        if n > i32::MAX as u64 {
            return Err(IrError::EnumerateIndexOverflow);
        }
        let out_ty = Ty::Array {
            elem: Box::new(Ty::i32()),
            size: n,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, count, Operation::Iota, loc))
    }

    /// `Fill` (ADR-0029): `[T; n]` with every element the source value. Builds
    /// the internal 2-tuple `(x, count)` (Pair-then-primitive, as `update`'s
    /// 3-tuple); the count must be a `Constant` integer (`NonStaticCount`).
    /// Rejects `n == 0` and a `Str`-carrying value (I9s).
    pub fn fill(
        &mut self,
        x: ObjectId,
        count: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let n = self.static_count(count)?;
        self.check_obj(x)?;
        let xty = self.ty_of(x);
        if ty::ty_contains_str(&xty) {
            return Err(IrError::StrOutsidePrint);
        }
        // Internal (x, count) 2-tuple, then Fill (the `zip`/`update` pattern).
        let pair_ty = Ty::Tuple(vec![xty.clone(), self.ty_of(count)]);
        self.b.intake_ty(&pair_ty)?;
        let pair = self
            .b
            .mint_object(self.f, pair_ty, None, ObjectKind::Temporary, None, loc);
        self.b
            .add_edge(self.f, x, pair, Operation::Pair { slot: 0, arity: 2 }, loc);
        self.b.add_edge(
            self.f,
            count,
            pair,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        let out_ty = Ty::Array {
            elem: Box::new(xty),
            size: n,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, pair, Operation::Fill, loc))
    }

    /// `Fill` from an EXISTING internal `(x, count)` 2-tuple — the replay
    /// entry (S21). Emits only the `Fill` edge, minting nothing, so rewrite
    /// replay is structure-faithful: a CSE'd duplicate tuple stays dead
    /// instead of being reborn by `fill`'s sugar mint every round (the S21
    /// fixpoint fix). Same static-n rule as `fill` (slot-1 feeder must be a
    /// `Constant` integer ≥ 1); validate's `IotaCountMismatch` twin re-checks
    /// independently.
    pub fn fill_from(
        &mut self,
        pair: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(pair)?;
        let (xty, count) = match self.ty_of(pair) {
            Ty::Tuple(ts) if ts.len() == 2 => {
                let count = self
                    .b
                    .in_edges
                    .get(pair)
                    .and_then(|ms| {
                        ms.iter().find_map(|&m| {
                            let mo = &self.b.morphisms[m];
                            matches!(mo.op, Operation::Pair { slot: 1, .. }).then_some(mo.source)
                        })
                    })
                    .ok_or(IrError::NonStaticCount)?;
                (ts[0].clone(), count)
            }
            _ => return Err(IrError::NonStaticCount),
        };
        let n = self.static_count(count)?;
        if ty::ty_contains_str(&xty) {
            return Err(IrError::StrOutsidePrint);
        }
        let out_ty = Ty::Array {
            elem: Box::new(xty),
            size: n,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, pair, Operation::Fill, loc))
    }

    /// The `iota`/`fill` static-count rule (ADR-0029): a `Constant` integer
    /// object with value `1..` (0 is not a legal `Ty::Array` size).
    fn static_count(&self, count: ObjectId) -> Result<u64, IrError> {
        let obj = self.b.objects.get(count).ok_or(IrError::UnknownObject)?;
        match (obj.kind, &obj.value) {
            (ObjectKind::Constant, Some(Value::I32(v))) if *v > 0 => Ok(*v as u64),
            (ObjectKind::Constant, Some(Value::I64(v))) if *v > 0 => Ok(*v as u64),
            (ObjectKind::Constant, Some(Value::U8(v))) if *v > 0 => Ok(*v as u64),
            _ => Err(IrError::NonStaticCount),
        }
    }

    /// `Update` (DESIGN §5.1; ADR-0021): `(Array{T,n}, I, T) → Array{T,n}`. Builds
    /// the internal 3-tuple `(arr, idx, val)` (Pair-then-primitive, as `phi`), then
    /// applies `Update`. `I` is a Core integer scalar (exactly `index`); `val` must
    /// match the element ty. Result is the same array ty (fresh array, slot `i`
    /// replaced); OOB is a run-time trap, not a shape error.
    pub fn update(
        &mut self,
        arr: ObjectId,
        idx: ObjectId,
        val: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(arr)?;
        self.check_obj(idx)?;
        self.check_obj(val)?;
        let aty = self.ty_of(arr);
        let ity = self.ty_of(idx);
        let vty = self.ty_of(val);
        let elem = match &aty {
            Ty::Array { elem, .. } => (**elem).clone(),
            _ => return Err(IrError::NotAProduct),
        };
        if !ity.is_integer() {
            return Err(IrError::TypeMismatch {
                expected: Ty::i32(),
                found: ity,
                loc,
            });
        }
        if vty != elem {
            return Err(IrError::TypeMismatch {
                expected: elem,
                found: vty,
                loc,
            });
        }
        let triple_ty = Ty::Tuple(vec![aty.clone(), ity.clone(), vty.clone()]);
        self.b.intake_ty(&triple_ty)?;
        let triple = self
            .b
            .mint_object(self.f, triple_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            arr,
            triple,
            Operation::Pair { slot: 0, arity: 3 },
            loc,
        );
        self.b.add_edge(
            self.f,
            idx,
            triple,
            Operation::Pair { slot: 1, arity: 3 },
            loc,
        );
        self.b.add_edge(
            self.f,
            val,
            triple,
            Operation::Pair { slot: 2, arity: 3 },
            loc,
        );
        let plan = self.dest_target(&dest, &aty, loc)?;
        Ok(self.emit_to_dest(plan, triple, Operation::Update, loc))
    }

    /// `Call(f)` (DESIGN §5.1): `f` is `FuncKind::Named`; arg ty = input ty.
    pub fn call(
        &mut self,
        callee: FuncId,
        arg: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let cf = self.b.funcs.get(callee).ok_or(IrError::UnknownFunction)?;
        if cf.def.kind != FuncKind::Named {
            return Err(IrError::CallToBody);
        }
        let in_ty = self.b.objects[cf.def.input].ty.clone();
        let out_ty = self.b.objects[cf.def.output].ty.clone();
        self.check_obj(arg)?;
        let aty = self.ty_of(arg);
        if aty != in_ty {
            return Err(IrError::TypeMismatch {
                expected: in_ty,
                found: aty,
                loc,
            });
        }
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, arg, Operation::Call(callee), loc))
    }

    /// `Map{body}` (DESIGN §5.1): `Array{T,n} → Array{U,n}`; body `T → U`,
    /// `MapBody`, token-free.
    pub fn map(
        &mut self,
        body: FuncId,
        arr: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let bf = self.b.funcs.get(body).ok_or(IrError::UnknownFunction)?;
        if bf.def.kind != FuncKind::MapBody {
            return Err(IrError::BodyKindMismatch);
        }
        let body_in = self.b.objects[bf.def.input].ty.clone();
        let body_out = self.b.objects[bf.def.output].ty.clone();
        if ty::ty_contains_token(&body_in) || ty::ty_contains_token(&body_out) {
            return Err(IrError::TokenInBody);
        }
        self.check_obj(arr)?;
        let aty = self.ty_of(arr);
        let (elem, size) = match &aty {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => return Err(IrError::NotAProduct),
        };
        if elem != body_in {
            return Err(IrError::TypeMismatch {
                expected: body_in,
                found: elem,
                loc,
            });
        }
        let out_ty = Ty::Array {
            elem: Box::new(body_out),
            size,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(plan, arr, Operation::Map { body, captures: 0 }, loc))
    }

    /// `Fold{body}` (DESIGN §5.1): `(Acc, Array{T,n}) → Acc`; body
    /// `(Acc, T) → Acc`, `FoldBody`, token-free.
    pub fn fold(
        &mut self,
        body: FuncId,
        seed_and_arr: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let bf = self.b.funcs.get(body).ok_or(IrError::UnknownFunction)?;
        if bf.def.kind != FuncKind::FoldBody {
            return Err(IrError::BodyKindMismatch);
        }
        let body_in = self.b.objects[bf.def.input].ty.clone();
        let body_out = self.b.objects[bf.def.output].ty.clone();
        if ty::ty_contains_token(&body_in) || ty::ty_contains_token(&body_out) {
            return Err(IrError::TokenInBody);
        }
        self.check_obj(seed_and_arr)?;
        let pty = self.ty_of(seed_and_arr);
        // Source must be (Acc, Array{T, n}); body_in must be (Acc, T).
        let (acc_ty, t_ty) = match &body_in {
            Ty::Tuple(ts) if ts.len() == 2 => (ts[0].clone(), ts[1].clone()),
            _ => {
                return Err(IrError::TypeMismatch {
                    expected: body_in.clone(),
                    found: pty.clone(),
                    loc,
                });
            }
        };
        let expected_src = Ty::Tuple(vec![
            acc_ty.clone(),
            Ty::Array {
                elem: Box::new(t_ty.clone()),
                size: array_size_of(&pty).unwrap_or(1),
            },
        ]);
        // Validate the source is (Acc, Array{T,n}) for some n, and body_out==Acc.
        match &pty {
            Ty::Tuple(ts) if ts.len() == 2 => {
                if ts[0] != acc_ty {
                    return Err(IrError::TypeMismatch {
                        expected: acc_ty.clone(),
                        found: ts[0].clone(),
                        loc,
                    });
                }
                match &ts[1] {
                    Ty::Array { elem, .. } if **elem == t_ty => {}
                    _ => {
                        return Err(IrError::TypeMismatch {
                            expected: expected_src,
                            found: pty.clone(),
                            loc,
                        });
                    }
                }
            }
            _ => {
                return Err(IrError::TypeMismatch {
                    expected: expected_src,
                    found: pty.clone(),
                    loc,
                });
            }
        }
        if body_out != acc_ty {
            return Err(IrError::TypeMismatch {
                expected: acc_ty,
                found: body_out,
                loc,
            });
        }
        self.b.intake_ty(&body_out)?;
        let plan = self.dest_target(&dest, &body_out, loc)?;
        Ok(self.emit_to_dest(
            plan,
            seed_and_arr,
            Operation::Fold { body, captures: 0 },
            loc,
        ))
    }

    /// `Map{body, captures}` (ADR-0027): `(c₁…cₖ, Array{T,n}) → Array{U,n}`;
    /// body `(c₁…cₖ, T) → U`. The `caps` objects are packed with the array
    /// into the source product (the broadcast edges); the capture count is
    /// recorded on the op. `caps` empty ⇒ [`Self::map`].
    pub fn map_captured(
        &mut self,
        body: FuncId,
        caps: &[ObjectId],
        arr: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        if caps.is_empty() {
            return self.map(body, arr, dest, loc);
        }
        let bf = self.b.funcs.get(body).ok_or(IrError::UnknownFunction)?;
        if bf.def.kind != FuncKind::MapBody {
            return Err(IrError::BodyKindMismatch);
        }
        let body_in = self.b.objects[bf.def.input].ty.clone();
        let body_out = self.b.objects[bf.def.output].ty.clone();
        if ty::ty_contains_token(&body_in) || ty::ty_contains_token(&body_out) {
            return Err(IrError::TokenInBody);
        }
        // Body input must be the (caps…, elem) product with matching types.
        let comps = match &body_in {
            Ty::Tuple(ts) if ts.len() == caps.len() + 1 => ts.clone(),
            _ => {
                return Err(IrError::TypeMismatch {
                    expected: body_in.clone(),
                    found: self.ty_of(arr),
                    loc,
                });
            }
        };
        for (i, &c) in caps.iter().enumerate() {
            self.check_obj(c)?;
            let cty = self.ty_of(c);
            if cty != comps[i] {
                return Err(IrError::TypeMismatch {
                    expected: comps[i].clone(),
                    found: cty,
                    loc,
                });
            }
            // ADR-0027: a capture with no runtime representation would have no
            // body-input slot on the backends (LLVM panicked). `IoToken` is
            // already `TokenInBody` above; `Str` stays `StrOutsidePrint` (the
            // pack below), so neither is re-diagnosed here.
            if ty::ty_residual_empty(&cty) && !ty::ty_contains_str(&cty) {
                return Err(IrError::ErasedCapture);
            }
        }
        self.check_obj(arr)?;
        let (elem, size) = match &self.ty_of(arr) {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => return Err(IrError::NotAProduct),
        };
        if elem != comps[caps.len()] {
            return Err(IrError::TypeMismatch {
                expected: comps[caps.len()].clone(),
                found: elem,
                loc,
            });
        }
        let mut all: Vec<ObjectId> = caps.to_vec();
        all.push(arr);
        let src = self.pack(&all, Dest::Fresh(None), loc)?;
        let out_ty = Ty::Array {
            elem: Box::new(body_out),
            size,
        };
        self.b.intake_ty(&out_ty)?;
        let plan = self.dest_target(&dest, &out_ty, loc)?;
        Ok(self.emit_to_dest(
            plan,
            src,
            Operation::Map {
                body,
                captures: caps.len() as u32,
            },
            loc,
        ))
    }

    /// `Fold{body, captures}` (ADR-0027): `(c₁…cₖ, Acc, Array{T,n}) → Acc`;
    /// body `(c₁…cₖ, Acc, T) → Acc`. `caps` empty ⇒ [`Self::fold`].
    pub fn fold_captured(
        &mut self,
        body: FuncId,
        caps: &[ObjectId],
        seed: ObjectId,
        arr: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        if caps.is_empty() {
            let seed_and_arr = self.pack(&[seed, arr], Dest::Fresh(None), loc)?;
            return self.fold(body, seed_and_arr, dest, loc);
        }
        let bf = self.b.funcs.get(body).ok_or(IrError::UnknownFunction)?;
        if bf.def.kind != FuncKind::FoldBody {
            return Err(IrError::BodyKindMismatch);
        }
        let body_in = self.b.objects[bf.def.input].ty.clone();
        let body_out = self.b.objects[bf.def.output].ty.clone();
        if ty::ty_contains_token(&body_in) || ty::ty_contains_token(&body_out) {
            return Err(IrError::TokenInBody);
        }
        // Body input must be (caps…, Acc, T); Acc = body_out.
        let comps = match &body_in {
            Ty::Tuple(ts) if ts.len() == caps.len() + 2 => ts.clone(),
            _ => {
                return Err(IrError::TypeMismatch {
                    expected: body_in.clone(),
                    found: self.ty_of(seed),
                    loc,
                });
            }
        };
        for (i, &c) in caps.iter().enumerate() {
            self.check_obj(c)?;
            let cty = self.ty_of(c);
            if cty != comps[i] {
                return Err(IrError::TypeMismatch {
                    expected: comps[i].clone(),
                    found: cty,
                    loc,
                });
            }
            // ADR-0027: a capture with no runtime representation would have no
            // body-input slot on the backends (LLVM panicked). `IoToken` is
            // already `TokenInBody` above; `Str` stays `StrOutsidePrint` (the
            // pack below), so neither is re-diagnosed here.
            if ty::ty_residual_empty(&cty) && !ty::ty_contains_str(&cty) {
                return Err(IrError::ErasedCapture);
            }
        }
        let acc_ty = comps[caps.len()].clone();
        let elem = comps[caps.len() + 1].clone();
        if body_out != acc_ty {
            return Err(IrError::TypeMismatch {
                expected: acc_ty,
                found: body_out,
                loc,
            });
        }
        self.check_obj(seed)?;
        if self.ty_of(seed) != acc_ty {
            return Err(IrError::TypeMismatch {
                expected: acc_ty,
                found: self.ty_of(seed),
                loc,
            });
        }
        self.check_obj(arr)?;
        match &self.ty_of(arr) {
            Ty::Array { elem: ae, .. } if **ae == elem => {}
            _ => {
                return Err(IrError::TypeMismatch {
                    expected: comps[caps.len() + 1].clone(),
                    found: self.ty_of(arr),
                    loc,
                });
            }
        }
        let mut all: Vec<ObjectId> = caps.to_vec();
        all.push(seed);
        all.push(arr);
        let src = self.pack(&all, Dest::Fresh(None), loc)?;
        self.b.intake_ty(&acc_ty)?;
        let plan = self.dest_target(&dest, &acc_ty, loc)?;
        Ok(self.emit_to_dest(
            plan,
            src,
            Operation::Fold {
                body,
                captures: caps.len() as u32,
            },
            loc,
        ))
    }

    /// `print` — raw, no trailing newline (ADR-0015): emits `Print { newline: false }`.
    pub fn print(
        &mut self,
        token: ObjectId,
        value: ObjectId,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.print_with(token, value, false, loc)
    }

    /// `println` — appends a trailing `"\n"` (ADR-0015): emits `Print { newline: true }`.
    pub fn println(
        &mut self,
        token: ObjectId,
        value: ObjectId,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.print_with(token, value, true, loc)
    }

    /// `Print { newline }` (DESIGN §5.1): `(IoToken, P) → IoToken`. Builds the internal
    /// `(IoToken, P)` pair (the only place `Str` may be a pair component, I9s).
    fn print_with(
        &mut self,
        token: ObjectId,
        value: ObjectId,
        newline: bool,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(token)?;
        self.check_obj(value)?;
        let tty = self.ty_of(token);
        let vty = self.ty_of(value);
        if tty != Ty::IoToken {
            return Err(IrError::TypeMismatch {
                expected: Ty::IoToken,
                found: tty,
                loc,
            });
        }
        if !vty.is_printable() {
            return Err(IrError::TypeMismatch {
                expected: Ty::Str,
                found: vty,
                loc,
            });
        }
        // The (IoToken, Str|N|Bool) pair — Str allowed here only (I9s).
        let pair_ty = Ty::Tuple(vec![Ty::IoToken, vty.clone()]);
        self.b.intake_ty(&pair_ty)?;
        let pair = self
            .b
            .mint_object(self.f, pair_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            token,
            pair,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            value,
            pair,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        // Result is a fresh IoToken.
        let out = self
            .b
            .mint_object(self.f, Ty::IoToken, None, ObjectKind::Temporary, None, loc);
        self.b
            .add_edge(self.f, pair, out, Operation::Print { newline }, loc);
        Ok(out)
    }

    /// Bare `x -> ret` / `x -> ret.k` for an EXISTING object (DESIGN §10, D6):
    /// `Output` (full) or a `Pair` slot edge.
    pub fn output(
        &mut self,
        value: ObjectId,
        slot: Option<u32>,
        loc: SourceLoc,
    ) -> Result<(), IrError> {
        self.check_obj(value)?;
        let vty = self.ty_of(value);
        let ret = self.output_id();
        let out_ty = self.output_ty();
        match slot {
            None => {
                if vty != out_ty {
                    return Err(IrError::RetTypeMismatch);
                }
                self.b.add_edge(self.f, value, ret, Operation::Output, loc);
            }
            Some(k) => {
                if out_ty.product_arity().is_none() {
                    return Err(IrError::RetNotProduct);
                }
                // u64 arity capped to u32: an array of size > u32::MAX is not
                // slot-addressable; the truncated arity would let an OOB slot
                // pass `k < arity` (reviewer F4/SND-3). Reject instead.
                let arity = out_ty.product_arity_u32().ok_or(IrError::SlotOutOfRange)?;
                if k >= arity {
                    return Err(IrError::SlotOutOfRange);
                }
                let comp_ty = out_ty.component_ty(k).unwrap().clone();
                if vty != comp_ty {
                    return Err(IrError::RetTypeMismatch);
                }
                self.b
                    .add_edge(self.f, value, ret, Operation::Pair { slot: k, arity }, loc);
            }
        }
        Ok(())
    }

    /// Begin a loop (DESIGN §10): mints the `LoopMerge` (ty = `init.ty`) and the
    /// `LoopEnter` edge from `init`.
    pub fn begin_loop(&mut self, init: ObjectId, loc: SourceLoc) -> Result<LoopHandle, IrError> {
        self.check_obj(init)?;
        let u_ty = self.ty_of(init);
        self.b.intake_ty(&u_ty)?;
        let merge =
            self.b
                .mint_object(self.f, u_ty.clone(), None, ObjectKind::LoopMerge, None, loc);
        self.b
            .add_edge(self.f, init, merge, Operation::LoopEnter, loc);
        let fc = &mut self.b.funcs[self.f];
        fc.open_loops += 1;
        fc.loop_counts.push((merge, 0, 0));
        Ok(LoopHandle { merge, u_ty })
    }

    /// Bump the `(back, exit)` tally for an open loop's merge.
    fn bump_loop(&mut self, merge: ObjectId, back: bool) {
        let fc = &mut self.b.funcs[self.f];
        if let Some(entry) = fc.loop_counts.iter_mut().find(|(m, _, _)| *m == merge) {
            if back {
                entry.1 += 1;
            } else {
                entry.2 += 1;
            }
        }
    }

    /// Read the `(back, exit)` tally for a merge.
    fn loop_tally(&self, merge: ObjectId) -> (u32, u32) {
        self.b.funcs[self.f]
            .loop_counts
            .iter()
            .find(|(m, _, _)| *m == merge)
            .map(|(_, b, e)| (*b, *e))
            .unwrap_or((0, 0))
    }

    /// The `LoopMerge` object of an open loop.
    pub fn merge_of(&self, lh: &LoopHandle) -> ObjectId {
        lh.merge
    }

    /// Record a back edge (DESIGN §10): packs `(next_state, cond)` into a route
    /// object, then `LoopBack` → merge. `next_state.ty == U`, `cond: Bool`.
    pub fn loop_back(
        &mut self,
        lh: &LoopHandle,
        next_state: ObjectId,
        cond: ObjectId,
        loc: SourceLoc,
    ) -> Result<(), IrError> {
        self.check_obj(next_state)?;
        self.check_obj(cond)?;
        let sty = self.ty_of(next_state);
        let cty = self.ty_of(cond);
        if sty != lh.u_ty {
            return Err(IrError::TypeMismatch {
                expected: lh.u_ty.clone(),
                found: sty,
                loc,
            });
        }
        if cty != Ty::Bool {
            return Err(IrError::TypeMismatch {
                expected: Ty::Bool,
                found: cty,
                loc,
            });
        }
        let route_ty = Ty::Tuple(vec![lh.u_ty.clone(), Ty::Bool]);
        self.b.intake_ty(&route_ty)?;
        let route = self
            .b
            .mint_object(self.f, route_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            next_state,
            route,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            cond,
            route,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        self.b
            .add_edge(self.f, route, lh.merge, Operation::LoopBack, loc);
        self.bump_loop(lh.merge, true);
        Ok(())
    }

    /// Record an exit edge (DESIGN §10): packs `(value, cond)` into a route
    /// object, then `LoopExit` → a fresh exit object (or Ret per `dest`).
    pub fn loop_exit(
        &mut self,
        lh: &LoopHandle,
        value: ObjectId,
        cond: ObjectId,
        dest: Dest,
        loc: SourceLoc,
    ) -> Result<ObjectId, IrError> {
        self.check_obj(value)?;
        self.check_obj(cond)?;
        let vty = self.ty_of(value);
        let cty = self.ty_of(cond);
        if cty != Ty::Bool {
            return Err(IrError::TypeMismatch {
                expected: Ty::Bool,
                found: cty,
                loc,
            });
        }
        let route_ty = Ty::Tuple(vec![vty.clone(), Ty::Bool]);
        self.b.intake_ty(&route_ty)?;
        let route = self
            .b
            .mint_object(self.f, route_ty, None, ObjectKind::Temporary, None, loc);
        self.b.add_edge(
            self.f,
            value,
            route,
            Operation::Pair { slot: 0, arity: 2 },
            loc,
        );
        self.b.add_edge(
            self.f,
            cond,
            route,
            Operation::Pair { slot: 1, arity: 2 },
            loc,
        );
        self.b.intake_ty(&vty)?;
        let plan = self.dest_target(&dest, &vty, loc)?;
        let out = self.emit_to_dest(plan, route, Operation::LoopExit, loc);
        self.bump_loop(lh.merge, false);
        Ok(out)
    }

    /// Close a loop (DESIGN §10): requires ≥1 back + ≥1 exit; consumes handle.
    pub fn end_loop(&mut self, lh: LoopHandle) -> Result<(), IrError> {
        let (backs, exits) = self.loop_tally(lh.merge);
        if backs == 0 {
            return Err(IrError::LoopWithoutBack);
        }
        if exits == 0 {
            return Err(IrError::LoopWithoutExit);
        }
        self.b.funcs[self.f].open_loops -= 1;
        Ok(())
    }

    /// Finish the function (DESIGN §10): local I-RET completeness + no open
    /// loops. The seal pass re-checks everything globally.
    pub fn finish(self) -> Result<(), IrError> {
        let fc = &self.b.funcs[self.f];
        if fc.open_loops > 0 {
            return Err(IrError::OpenLoop);
        }
        // Local I-RET pre-check (seal re-derives, but report early).
        let ret = fc.def.output;
        let out_ty = self.b.objects[ret].ty.clone();
        let mut full = 0usize;
        let mut slots: Vec<u32> = Vec::new();
        for &m in &self.b.in_edges[ret] {
            match self.b.morphisms[m].op {
                Operation::Pair { slot, .. } => slots.push(slot),
                _ => full += 1,
            }
        }
        if full > 0 && !slots.is_empty() {
            return Err(IrError::RetMixedWriters);
        }
        if !slots.is_empty() {
            let arity = out_ty.product_arity().ok_or(IrError::RetNotProduct)?;
            slots.sort_unstable();
            for w in slots.windows(2) {
                if w[0] == w[1] {
                    return Err(IrError::RetSlotConflict);
                }
            }
            // Completeness: every slot `0..arity` must have a distinct writer.
            // `slots` is `u32`-valued and now deduplicated; a product whose arity
            // exceeds the writer count (incl. any > u32::MAX-arity array, which
            // is not slot-addressable anyway) is missing slots. Comparing counts
            // avoids iterating a u64 range (F4/SND-3).
            if (slots.len() as u64) != arity {
                return Err(IrError::RetSlotMissing);
            }
            for (k, &slot) in slots.iter().enumerate() {
                if slot as u64 != k as u64 {
                    return Err(IrError::RetSlotMissing);
                }
            }
        } else if full == 0 && out_ty != Ty::Unit {
            return Err(IrError::RetSlotMissing);
        }
        self.b.funcs[self.f].finished = true;
        Ok(())
    }
}

/// Internal plan for where a primitive's defining edge lands.
enum DestPlan {
    /// Edge targets this object directly (Fresh, or Ret with no slot).
    Direct(ObjectId),
    /// Edge targets `comp`, then a `Pair{slot, arity}` wires `comp` into `ret`.
    Slot {
        comp: ObjectId,
        ret: ObjectId,
        slot: u32,
        arity: u32,
    },
}

/// Whether a `LoopExit` whose source is `route` belongs to the loop whose merge
/// is `merge` — i.e. `route` is forward-reachable from `merge` (the exit reads
/// merge-derived state). The exit route is a downward leaf outside the SCC, so
/// SCC membership alone cannot attribute it; reachability from the merge can.
///
/// Iterative BFS (J1); stops as soon as `route` is found.
fn exit_reachable_from_merge(ir: &CategoryIr, merge: ObjectId, route: ObjectId) -> bool {
    let mut seen: SecondaryMap<ObjectId, bool> = SecondaryMap::new();
    let mut queue: Vec<ObjectId> = vec![merge];
    seen.insert(merge, true);
    while let Some(o) = queue.pop() {
        if o == route {
            return true;
        }
        for &m in ir.out_edges(o) {
            let t = ir.morphism(m).unwrap().target;
            if !seen.get(t).copied().unwrap_or(false) {
                seen.insert(t, true);
                queue.push(t);
            }
        }
    }
    false
}

/// The carried-state object of a `LoopBack` route (DESIGN §7): the source of
/// the route's slot-0 `Pair` in-edge (the `next_state`; slot 1 is the `cond`).
/// `route` is the `LoopBack` morphism's source object. Returns `None` if the
/// route is not the canonical `(next_state, cond)` shape (a corrupted graph —
/// validate's own pass flags the in-edge shape separately).
fn loopback_state_source(ir: &CategoryIr, route: ObjectId) -> Option<ObjectId> {
    for &m in ir.in_edges(route) {
        let morph = ir.morphism(m)?;
        if let Operation::Pair { slot: 0, .. } = morph.op {
            return Some(morph.source);
        }
    }
    None
}

/// The element count of an `Array` ty, else `None`.
fn array_size_of(ty: &Ty) -> Option<u64> {
    if let Ty::Tuple(ts) = ty
        && let Some(Ty::Array { size, .. }) = ts.get(1)
    {
        return Some(*size);
    }
    None
}
