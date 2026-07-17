//! Pass C + D2/D3 + E — declaration, the §8 emission recipes, and seal.
//!
//! Emission walks statements in source order maintaining the scope stack, the
//! wire (`cur: Option<ObjectId>` within a chain), and the current token
//! (effectful fns). Every builder call passes the `SourceLoc` of the surface
//! node driving it (§10). The builder is the second line of type checking
//! (LD12): a `TypeMismatch`/`NotAProduct`/etc. that corresponds to a user
//! condition is mapped to its L-code; anything truly unexpected is L1901.

use std::collections::BTreeMap;

use flow_ir::{
    CategoryIr, Dest, FnBuilder, FuncId, FuncKind, IrBuilder, IrError, ObjectId, Operation, Ty,
    Value,
};
use flow_syntax::{
    self as syn, ArmPayload, BinOp, Block, BlockItem, Chain, CollOp, Expr, ExprKind, GuardArm,
    GuardDiscr, MemberField, Program, Stage, StageKind, Stmt, StmtKind, UnOp,
};

use crate::diag::{LCode, diag};
use crate::effects::Effects;
use crate::scope::ScopeStack;
use crate::typing::{FnSig, TypeInfo};
use crate::tys::{TypeTable, ir_loc, name_text, src_slice};

/// A surface-bound symbol (DESIGN §3).
#[derive(Clone)]
pub struct Binding {
    pub obj: ObjectId,
    pub ty: Ty,
    pub mutable: bool,
    /// Declaration sequence number (params first, then bindings in source
    /// order). Carried-state order is declaration order (LD4); a rebind keeps
    /// the original `decl_seq`.
    pub decl_seq: u32,
}

/// A short alias for the per-function diagnostic-or-id result. Emission is
/// fail-fast (LD12): the first error aborts via `?`.
type ER<T> = Result<T, syn::Diagnostic>;

/// The per-function emission context.
struct Emitter<'a> {
    source: &'a str,
    types: &'a TypeTable,
    effects: &'a Effects,
    fn_ids: &'a BTreeMap<String, FuncId>,
    fn_sigs: &'a BTreeMap<String, FnSig>,
    info: &'a TypeInfo,
    /// stage span → declared+built body FuncId (D2).
    body_ids: &'a BTreeMap<(u32, u32), FuncId>,
    owner_name: String,
    /// current token register (effectful fns); None for pure fns.
    token: Option<ObjectId>,
    /// the scope stack of surface-bound symbols (DESIGN §3).
    scope: ScopeStack<Binding>,
    /// object → its ty (the builder does not expose object tys, so we track them
    /// for branch decisions; the builder remains the authority on correctness).
    obj_ty: BTreeMap<ObjectId, Ty>,
    /// loop nesting depth (for the inner-exit-via-ret restriction, §8.5/LD20).
    loop_depth: u32,
    /// monotonic declaration sequence counter (carried-state order = declaration
    /// order, LD4).
    decl_seq: u32,
    /// objects derived from the current loop's merge view (for L1503): seeded at
    /// the carried-name merge projs + token, propagated through primitives, and
    /// killed by a merge-independent rebind (§7.3).
    derived: std::collections::BTreeSet<ObjectId>,
    /// whether any enclosing loop carries a token (a nested loop inside one is
    /// L1504 — the token cannot get a third route consumer).
    enclosing_token_loop: bool,
    /// names carried by an enclosing loop (an inner loop assigning one is L1504).
    outer_carried: std::collections::BTreeSet<String>,
}

/// Map a builder `IrError` to an L-code diagnostic at `span`. User-diagnosable
/// conditions get their specific code; anything else is L1901 (a lower bug).
fn ir_err(e: IrError, span: syn::SourceLoc) -> syn::Diagnostic {
    let (code, msg): (LCode, String) = match &e {
        IrError::TypeMismatch {
            expected, found, ..
        } => (
            LCode::TypeMismatch,
            format!(
                "type mismatch: expected `{}`, found `{}`",
                ty_name(expected),
                ty_name(found)
            ),
        ),
        IrError::NotAProduct => (
            LCode::NotAProduct,
            "operation on a non-product value".to_string(),
        ),
        IrError::SlotOutOfRange => (LCode::SlotOutOfRange, "slot index out of range".to_string()),
        IrError::StrOutsidePrint => (
            LCode::StrOutsidePrint,
            "string value used outside `print`".to_string(),
        ),
        IrError::EmptyProduct => (LCode::EmptyArray, "empty product/array".to_string()),
        IrError::TyTooDeep => (LCode::TypeTooDeep, "type nests too deeply".to_string()),
        IrError::RetTypeMismatch => (
            LCode::TypeMismatch,
            "return value type does not match the declared output".to_string(),
        ),
        other => (
            LCode::Internal,
            format!("internal lowering error: {other:?}"),
        ),
    };
    diag(code, span, msg)
}

/// Render a ty for diagnostics (delegates to flow-ir's label fn).
pub fn ty_name(ty: &Ty) -> String {
    flow_ir::ty_label(ty)
}

/// Lower a parse-clean program to a sealed IR (the crate entry; DESIGN §1).
pub fn lower_program(
    source: &str,
    program: &Program,
    types: &TypeTable,
    effects: &Effects,
    fn_sigs: &BTreeMap<String, FnSig>,
    typing: &BTreeMap<String, TypeInfo>,
) -> Result<CategoryIr, Vec<syn::Diagnostic>> {
    let mut b = IrBuilder::new();

    // Pass C: declare every surface fn in source order with its IR tys (§6.2).
    let mut fn_ids: BTreeMap<String, FuncId> = BTreeMap::new();
    let mut main_id: Option<FuncId> = None;
    for item in &program.items {
        if let syn::Item::Fn(fd) = item {
            let name = name_text(source, fd.name).to_string();
            let sig = fn_sigs.get(&name).expect("fn sig");
            let (in_ty, out_ty) = ir_signature(sig);
            let fid = b
                .declare(FuncKind::Named, &name, in_ty, out_ty, ir_loc(fd.span))
                .map_err(|e| vec![ir_err(e, fd.span)])?;
            if name == "main" {
                main_id = Some(fid);
            }
            fn_ids.insert(name, fid);
        }
    }
    let main_id = main_id.ok_or_else(|| {
        vec![diag(
            LCode::NoMain,
            program.span,
            "no `fn main`".to_string(),
        )]
    })?;

    // Pass D: per fn in source order — D2 (bodies) then D3 (outer).
    for item in &program.items {
        if let syn::Item::Fn(fd) = item {
            let name = name_text(source, fd.name).to_string();
            let info = typing.get(&name).expect("typing info");

            // D2: declare + build every map/fold body, innermost-first.
            let mut body_ids: BTreeMap<(u32, u32), FuncId> = BTreeMap::new();
            build_bodies(
                source,
                types,
                effects,
                &fn_ids,
                fn_sigs,
                info,
                &name,
                &fd.body,
                &mut b,
                &mut body_ids,
            )
            .map_err(|d| vec![d])?;

            // D3: build the outer fn body.
            let fid = fn_ids[&name];
            let mut em = Emitter {
                source,
                types,
                effects,
                fn_ids: &fn_ids,
                fn_sigs,
                info,
                body_ids: &body_ids,
                owner_name: name.clone(),
                token: None,
                scope: ScopeStack::new(),
                obj_ty: BTreeMap::new(),
                loop_depth: 0,
                decl_seq: 0,
                derived: std::collections::BTreeSet::new(),
                enclosing_token_loop: false,
                outer_carried: std::collections::BTreeSet::new(),
            };
            let fb = b.build_fn(fid).map_err(|e| vec![ir_err(e, fd.span)])?;
            em.emit_fn(fd, fb).map_err(|d| vec![d])?;
        }
    }

    // Pass E: seal.
    b.seal(main_id).map_err(|e| {
        vec![diag(
            LCode::Internal,
            program.span,
            format!("seal failed: {e:?}"),
        )]
    })
}

/// The IR signature (input, output tys) for a surface fn per §6.2.
fn ir_signature(sig: &FnSig) -> (Ty, Ty) {
    let a: Option<Ty> = if sig.params.is_empty() {
        None
    } else if sig.params.len() == 1 {
        Some(sig.params[0].clone())
    } else {
        Some(Ty::Tuple(sig.params.clone()))
    };
    let b = sig.ret.clone();
    if !sig.effectful {
        let in_ty = a.unwrap_or(Ty::Unit);
        let out_ty = b.unwrap_or(Ty::Unit);
        (in_ty, out_ty)
    } else {
        let in_ty = match a {
            Some(a) => Ty::Tuple(vec![Ty::IoToken, a]),
            None => Ty::IoToken,
        };
        let out_ty = match b {
            Some(b) => Ty::Tuple(vec![Ty::IoToken, b]),
            None => Ty::IoToken,
        };
        (in_ty, out_ty)
    }
}

/// D2: declare and build every map/fold body in `block`, innermost-first.
#[allow(clippy::too_many_arguments)]
fn build_bodies(
    source: &str,
    types: &TypeTable,
    effects: &Effects,
    fn_ids: &BTreeMap<String, FuncId>,
    fn_sigs: &BTreeMap<String, FnSig>,
    info: &TypeInfo,
    owner: &str,
    block: &Block,
    b: &mut IrBuilder,
    out: &mut BTreeMap<(u32, u32), FuncId>,
) -> ER<()> {
    // Find every MapFold stage by walking the tree; for each, recurse into its
    // body first (innermost-first), then declare+build it.
    let stages = collect_mapfold_stages(block);
    for (stage, body, params, op) in stages {
        // Recurse into nested map/fold inside this body first.
        build_bodies(
            source, types, effects, fn_ids, fn_sigs, info, owner, body, b, out,
        )?;

        let sig = info
            .block(stage.span)
            .cloned()
            .ok_or_else(|| internal(stage.span, "missing block signature"))?;
        let body_out_ty = sig.body_out.clone();
        let body_in_ty = sig.body_in.clone();
        let kind = match op {
            CollOp::Map => FuncKind::MapBody,
            CollOp::Fold => FuncKind::FoldBody,
        };
        let bid = b
            .declare(
                kind,
                &sig.name,
                body_in_ty.clone(),
                body_out_ty.clone(),
                ir_loc(stage.span),
            )
            .map_err(|e| ir_err(e, stage.span))?;
        out.insert((stage.span.start, stage.span.end), bid);

        // Build the body fn.
        let mut em = Emitter {
            source,
            types,
            effects,
            fn_ids,
            fn_sigs,
            info,
            body_ids: out,
            owner_name: owner.to_string(),
            token: None,
            scope: ScopeStack::new(),
            obj_ty: BTreeMap::new(),
            loop_depth: 0,
            decl_seq: 0,
            derived: std::collections::BTreeSet::new(),
            enclosing_token_loop: false,
            outer_carried: std::collections::BTreeSet::new(),
        };
        let fb = b.build_fn(bid).map_err(|e| ir_err(e, stage.span))?;
        em.emit_body(op, params, body, &body_in_ty, &body_out_ty, fb)?;
    }
    Ok(())
}

/// Collect every map/fold stage with its body/params/op, in source order,
/// recursing through statements/guards/fanouts but NOT into nested map/fold
/// bodies (the recursion in `build_bodies` handles nesting).
fn collect_mapfold_stages(block: &Block) -> Vec<(&Stage, &Block, &[syn::Name], CollOp)> {
    let mut out = Vec::new();
    collect_block(block, &mut out);
    out
}

fn collect_block<'a>(
    block: &'a Block,
    out: &mut Vec<(&'a Stage, &'a Block, &'a [syn::Name], CollOp)>,
) {
    for item in &block.items {
        match item {
            BlockItem::Stmt(s) => collect_stmt(s, out),
            BlockItem::Arm(a) => collect_arm(&a.payload, out),
        }
    }
    if let Some(t) = &block.tail {
        collect_chain(t, out);
    }
}

fn collect_stmt<'a>(
    stmt: &'a Stmt,
    out: &mut Vec<(&'a Stage, &'a Block, &'a [syn::Name], CollOp)>,
) {
    match &stmt.kind {
        StmtKind::Chain(c) => collect_chain(c, out),
        StmtKind::Bind(_) => {}
        StmtKind::Loop(l) => collect_block(&l.body, out),
        StmtKind::Error => {}
    }
}

fn collect_arm<'a>(
    p: &'a ArmPayload,
    out: &mut Vec<(&'a Stage, &'a Block, &'a [syn::Name], CollOp)>,
) {
    match p {
        ArmPayload::Chain(c) => collect_chain(c, out),
        ArmPayload::Block(b) => collect_block(b, out),
    }
}

fn collect_chain<'a>(
    chain: &'a Chain,
    out: &mut Vec<(&'a Stage, &'a Block, &'a [syn::Name], CollOp)>,
) {
    for stage in &chain.stages {
        match &stage.kind {
            StageKind::MapFold { op, params, body } => {
                out.push((stage, body, params.as_slice(), *op));
            }
            StageKind::Guard(arms) => {
                for arm in arms {
                    collect_arm(&arm.payload, out);
                }
            }
            StageKind::Fanout { branches, .. } => {
                for b in branches {
                    collect_chain(b, out);
                }
            }
            StageKind::SeqBlock(body) => collect_block(body, out),
            _ => {}
        }
    }
}

fn internal(span: syn::SourceLoc, msg: &str) -> syn::Diagnostic {
    diag(LCode::Internal, span, msg.to_string())
}

// ===========================================================================
//  The emission walk
// ===========================================================================

impl Emitter<'_> {
    /// Build a surface function body (D3).
    fn emit_fn(&mut self, fd: &syn::FnDecl, mut fb: FnBuilder) -> ER<()> {
        let name = self.owner_name.clone();
        let sig = self.fn_sigs.get(&name).expect("sig").clone();
        let effectful = sig.effectful;
        let input = fb.input();

        // Bind params. Effectful: input is (IoToken, A) or IoToken alone.
        let (in_ty, _) = ir_signature(&sig);
        self.record(input, in_ty);
        if effectful {
            if sig.params.is_empty() {
                // input() IS the token.
                self.record(input, Ty::IoToken);
                self.token = Some(input);
            } else {
                // tok = proj(input, 0); params view = proj(input, 1).
                let tok = fb
                    .proj(input, 0, Dest::Fresh(None), ir_loc(fd.span))
                    .map_err(|e| ir_err(e, fd.span))?;
                self.record(tok, Ty::IoToken);
                self.token = Some(tok);
                self.bind_params(&mut fb, fd, &sig, input, Some(1))?;
            }
        } else {
            self.bind_params(&mut fb, fd, &sig, input, None)?;
        }

        // Emit the body block.
        self.emit_block(
            &mut fb,
            &fd.body,
            BodyCtx::FnBody {
                ret: sig.ret.clone(),
                effectful,
            },
        )?;

        // For an effectful B-absent fn whose token was not yet written to Return,
        // write the final token. (Pure fns: nothing to do; Unit returns may have
        // zero writers.)
        if effectful
            && sig.ret.is_none()
            && let Some(tok) = self.token
        {
            // Only write if Return has no writer yet (a bare-ret exit already
            // wrote it). We always write here for the straight-line case; a
            // ret-terminal loop exit consumes the token (self.token = None).
            fb.output(tok, None, ir_loc(fd.span))
                .map_err(|e| ir_err(e, fd.span))?;
        }

        fb.finish().map_err(|e| ir_err(e, fd.span))?;
        Ok(())
    }

    fn bind_params(
        &mut self,
        fb: &mut FnBuilder,
        fd: &syn::FnDecl,
        sig: &FnSig,
        input: ObjectId,
        view_slot: Option<u32>,
    ) -> ER<()> {
        let multi = sig.params.len() >= 2;
        // Effectful: the params view = proj(input, view_slot), emitted once.
        let view = if let Some(vslot) = view_slot {
            let view_ty = if multi {
                Ty::Tuple(sig.params.clone())
            } else {
                sig.params[0].clone()
            };
            let v = fb
                .proj(input, vslot, Dest::Fresh(None), ir_loc(fd.span))
                .map_err(|e| ir_err(e, fd.span))?;
            self.record(v, view_ty);
            Some(v)
        } else {
            None
        };
        for (k, p) in fd.params.iter().enumerate() {
            let pname = name_text(self.source, p.name).to_string();
            let pty = sig.params[k].clone();
            let obj = if let Some(view) = view {
                if multi {
                    fb.proj(
                        view,
                        k as u32,
                        Dest::Fresh(Some(pname.clone())),
                        ir_loc(p.span),
                    )
                    .map_err(|e| ir_err(e, p.span))?
                } else {
                    view
                }
            } else if multi {
                fb.proj(
                    input,
                    k as u32,
                    Dest::Fresh(Some(pname.clone())),
                    ir_loc(p.span),
                )
                .map_err(|e| ir_err(e, p.span))?
            } else {
                input
            };
            self.record(obj, pty.clone());
            self.bind_new(&pname, obj, pty, p.mut_span.is_some());
        }
        Ok(())
    }

    /// Build a map/fold body fn (D2).
    fn emit_body(
        &mut self,
        op: CollOp,
        params: &[syn::Name],
        body: &Block,
        body_in: &Ty,
        body_out: &Ty,
        mut fb: FnBuilder,
    ) -> ER<()> {
        let input = fb.input();
        let span = body.span;
        match op {
            CollOp::Map => {
                // single param T = input.
                if let Some(p0) = params.first() {
                    self.record(input, body_in.clone());
                    let pn = name_text(self.source, *p0).to_string();
                    self.bind_new(&pn, input, body_in.clone(), false);
                }
            }
            CollOp::Fold => {
                // (Acc, T): proj 0, proj 1.
                self.record(input, body_in.clone());
                let acc = fb
                    .proj(input, 0, Dest::Fresh(None), ir_loc(span))
                    .map_err(|e| ir_err(e, span))?;
                let t = fb
                    .proj(input, 1, Dest::Fresh(None), ir_loc(span))
                    .map_err(|e| ir_err(e, span))?;
                if let Ty::Tuple(ts) = body_in {
                    self.record(acc, ts[0].clone());
                    self.record(t, ts[1].clone());
                    let p0 = name_text(self.source, params[0]).to_string();
                    let p1 = name_text(self.source, params[1]).to_string();
                    self.bind_new(&p0, acc, ts[0].clone(), false);
                    self.bind_new(&p1, t, ts[1].clone(), false);
                }
            }
        }
        // Body: statements then tail = the body value, written canonically.
        self.emit_block(
            &mut fb,
            body,
            BodyCtx::MapFoldBody {
                out_ty: body_out.clone(),
            },
        )?;
        fb.finish().map_err(|e| ir_err(e, span))?;
        Ok(())
    }

    // --- block / statement emission ----------------------------------------

    fn emit_block(&mut self, fb: &mut FnBuilder, block: &Block, ctx: BodyCtx) -> ER<()> {
        for item in &block.items {
            match item {
                BlockItem::Stmt(s) => self.emit_stmt(fb, s)?,
                BlockItem::Arm(_) => {}
            }
        }
        // The W11 tail chain is the final item (LD21).
        if let Some(tail) = &block.tail {
            match &ctx {
                BodyCtx::FnBody { ret, effectful } => {
                    // fn-body tail: virtual ret continuation when output non-Unit.
                    self.emit_tail_as_return(fb, tail, ret.clone(), *effectful)?;
                }
                BodyCtx::MapFoldBody { out_ty } => {
                    self.emit_body_tail(fb, tail, out_ty.clone())?;
                }
                BodyCtx::Plain => {
                    self.emit_chain(fb, tail, ChainCtx::Statement)?;
                }
            }
        }
        Ok(())
    }

    fn emit_stmt(&mut self, fb: &mut FnBuilder, stmt: &Stmt) -> ER<()> {
        match &stmt.kind {
            StmtKind::Chain(c) => {
                self.emit_chain(fb, c, ChainCtx::Statement)?;
                Ok(())
            }
            StmtKind::Bind(bnd) => {
                let v = self.emit_expr(fb, &bnd.value)?;
                let ty = self.ty_of(fb, v);
                let bn = name_text(self.source, bnd.name).to_string();
                self.bind_new(&bn, v, ty, bnd.mut_span.is_some());
                Ok(())
            }
            StmtKind::Loop(l) => self.emit_loop(fb, l),
            StmtKind::Error => Err(diag(LCode::UncleanTree, stmt.span, "error node")),
        }
    }

    /// Emit the fn-body tail as a return (a virtual `-> ret`), if the declared
    /// output is non-Unit; otherwise it is dead code (still walked for effects).
    fn emit_tail_as_return(
        &mut self,
        fb: &mut FnBuilder,
        tail: &Chain,
        ret: Option<Ty>,
        effectful: bool,
    ) -> ER<()> {
        if ret.is_none() {
            // Unit output: the tail is dead code as a *value*; the final token (if
            // effectful) is written by `emit_fn`, not the tail (LD21 / ATK-17).
            // Walk it as a statement regardless of effectfulness — effects still
            // thread via `self.token`, and a fanout tail does not demand branch
            // values (the §6.2 yes|absent rows have no surface return).
            self.emit_chain(fb, tail, ChainCtx::Statement)?;
            return Ok(());
        }
        if effectful {
            // Effectful with B present: the tail value is a full-tuple writer —
            // produce it Fresh, then `pack(tok, value) -> Dest::Ret { slot: None }`
            // (§8.1 / LD18, LOWER-2). If the tail itself ends in `-> ret`, the Ret
            // marker already packed the token via `emit_ret_existing`.
            if chain_ends_in_ret(tail) {
                self.emit_chain(fb, tail, ChainCtx::FnBodyReturn { effectful: true })?;
                return Ok(());
            }
            // `RetValue` (not `Statement`): the tail is in return position, so a
            // tail-less `seq` here draws L1611 (ADR-0019 pin c), not the outer
            // L1306 fall-through. A valued tail is still handed back for the
            // token-pack below (RetValue writes nothing to Ret itself).
            let v = self.emit_chain(fb, tail, ChainCtx::RetValue)?;
            let value = v.ok_or_else(|| {
                diag(
                    LCode::IncompleteReturn,
                    tail.span,
                    "fn-body tail produces no value to return",
                )
            })?;
            let tok = self
                .token
                .ok_or_else(|| internal(tail.span, "effectful tail without token"))?;
            self.pack(fb, &[tok, value], Dest::Ret { slot: None }, tail.span)
                .map_err(|e| ir_err(e, tail.span))?;
            self.token = None;
            return Ok(());
        }
        // Non-Unit, pure: the tail value returns via the lookahead `Dest::Ret`.
        self.emit_chain(fb, tail, ChainCtx::FnBodyReturn { effectful: false })?;
        Ok(())
    }

    fn emit_body_tail(&mut self, fb: &mut FnBuilder, tail: &Chain, out_ty: Ty) -> ER<()> {
        let _ = out_ty;
        self.emit_chain(fb, tail, ChainCtx::BodyReturn)?;
        Ok(())
    }

    // --- chains -------------------------------------------------------------

    /// Lower a chain head-to-tail. Returns the final wire object (if any).
    fn emit_chain(
        &mut self,
        fb: &mut FnBuilder,
        chain: &Chain,
        ctx: ChainCtx,
    ) -> ER<Option<ObjectId>> {
        let stages = &chain.stages;
        // The head's Dest (canonical ret-write, pin 2): a value-producing head
        // whose only continuation is `-> ret` (directly, or no stages in a return
        // context) targets Return. A bare pre-existing wire (Var/param/const) uses
        // `output()` at the `-> ret` stage instead.
        let head_is_value = chain
            .head
            .as_ref()
            .map(|h| expr_is_value_producing(&h.kind))
            .unwrap_or(false);
        // A bare pre-existing-object tail (`fn f(x: i32) -> i32 { x }` / a body
        // tail `{ … x }`) must route through `output()` / `emit_ret_existing`
        // (LD21 + §8.1: bare pre-existing → Output), not pass `Dest::Ret` to the
        // `Var` arm (which would ignore it → seal `RetSlotMissing` / L1901; ATK-07a).
        let tail_is_return = matches!(ctx, ChainCtx::FnBodyReturn { .. } | ChainCtx::BodyReturn);
        if stages.is_empty()
            && !head_is_value
            && tail_is_return
            && let Some(h) = &chain.head
        {
            let wire = self.emit_expr(fb, h)?;
            self.emit_ret_existing(fb, wire, None, h.span, &ctx)?;
            return Ok(None);
        }
        let head_dest = if stages.is_empty() {
            self.lookahead_dest(None, &ctx, true)?
        } else if head_is_value {
            // §8.1's lookahead applies to heads too: `-> ret` targets Return
            // (pin 2), and a following binding names the head's object (LD17 —
            // `[…] -> image: …` labels the pack; `acc + i -> acc` labels the new
            // SSA object). The symbol itself still binds at the Bind/Var stage.
            self.lookahead_dest(stages.first(), &ctx, false)?
        } else {
            Dest::Fresh(None)
        };
        let head_to_dest = matches!(head_dest, Dest::Ret { .. });

        // Seed the wire.
        let mut cur: Option<ObjectId> = match &chain.head {
            Some(h) => {
                let r = self.emit_expr_dest(fb, h, head_dest.clone())?;
                if head_to_dest {
                    return Ok(None); // head wrote to Return; chain done.
                }
                Some(r)
            }
            None => match &ctx {
                ChainCtx::HeadlessSeed(seed) => Some(*seed),
                _ => None,
            },
        };

        // Whether the wire was produced by a value-producing primitive that took
        // the lookahead Dest (so a following `-> ret`/bind is already wired).
        let mut prev_to_dest = false;
        let mut i = 0;
        while i < stages.len() {
            let stage = &stages[i];
            let next = stages.get(i + 1);
            let is_last = i + 1 == stages.len();
            // A `Ret` stage following a value-producing stage is already wired by
            // the lookahead Dest (pin 2); it is a no-op marker. Only a `-> ret`
            // of a *pre-existing* wire (no preceding value-producing stage) needs
            // `output()` (§8.1).
            if let StageKind::Ret { proj } = &stage.kind {
                if !is_last {
                    return Err(diag(
                        LCode::RetMidChain,
                        stages[i + 1].span,
                        "stages after `-> ret` in one chain",
                    ));
                }
                if prev_to_dest {
                    return Ok(None); // already written to Return.
                }
                // A value-less `-> ret` (no wire): the statement-level / fn-body
                // bare return marker (ATK-09/ATK-10). The catalogue permits it.
                // §8.3: in a pure Unit fn it is a no-op; in an effectful B-absent
                // fn it writes the current token to Return (and consumes the
                // register — later token use → L1307); a non-Unit output was
                // already L1306'd in D1.
                let Some(wire) = cur else {
                    self.emit_valueless_ret(fb, *proj, stage.span)?;
                    return Ok(None);
                };
                self.emit_ret_existing(fb, wire, *proj, stage.span, &ctx)?;
                return Ok(None);
            }
            let value_producing = self.stage_writes_value(&stage.kind);
            cur = self.emit_stage(fb, stage, next, is_last, cur, &ctx)?;
            // The wire went to Return iff this stage was value-producing and the
            // next stage is `-> ret` (the lookahead chose Dest::Ret). In an
            // effectful B-present fn the producer took Fresh (a full-tuple writer
            // packs the token at the `Ret` marker — §8.1 / LD18), so the marker
            // must still reach `emit_ret_existing`.
            prev_to_dest = value_producing
                && !self.effectful_with_ret()
                && matches!(next.map(|n| &n.kind), Some(StageKind::Ret { .. }));
            i += 1;
        }
        Ok(cur)
    }

    /// Whether a stage emits a fresh-value primitive that takes the lookahead
    /// `Dest` (so a following `-> ret` is pre-wired; pin 2). A bare-name `Expr`
    /// stage counts only when it is a *pure-fn call*; a rebind, alias-bind, or
    /// `print` does not.
    fn stage_writes_value(&self, kind: &StageKind) -> bool {
        match kind {
            StageKind::OpShorthand { .. }
            | StageKind::Guard(_)
            | StageKind::MapFold { .. }
            | StageKind::Fanout { .. } => true,
            StageKind::Expr(e) => {
                if let ExprKind::Var(n) = &e.kind {
                    let text = name_text(self.source, *n);
                    if let Some(sig) = self.fn_sigs.get(text) {
                        return !sig.effectful;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Lower one stage. `next` is the following stage (one-stage lookahead, LD2).
    #[allow(clippy::too_many_arguments)]
    fn emit_stage(
        &mut self,
        fb: &mut FnBuilder,
        stage: &Stage,
        next: Option<&Stage>,
        is_last: bool,
        cur: Option<ObjectId>,
        ctx: &ChainCtx,
    ) -> ER<Option<ObjectId>> {
        let sp = stage.span;
        match &stage.kind {
            StageKind::Expr(e) => self.emit_expr_stage(fb, e, next, cur, ctx),
            StageKind::OpShorthand { expr } => {
                let wire =
                    cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "shorthand with no wire"))?;
                let dest = self.lookahead_dest(next, ctx, is_last)?;
                let r = self.emit_op_shorthand(fb, expr, wire, dest)?;
                self.maybe_bind_next(fb, next, r);
                Ok(Some(r))
            }
            StageKind::Bind { mut_span, name, ty } => {
                let wire =
                    cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "bind with no wire"))?;
                let bty = if let Some(annot) = ty {
                    self.types
                        .resolve(self.source, annot, &mut Vec::new())
                        .unwrap_or_else(|| self.ty_of(fb, wire))
                } else {
                    self.ty_of(fb, wire)
                };
                let bn = name_text(self.source, *name).to_string();
                self.bind_new(&bn, wire, bty, mut_span.is_some());
                Ok(Some(wire))
            }
            StageKind::Ret { .. } => {
                // `Ret` is handled directly in `emit_chain` (the lookahead-Dest
                // pin-2 logic needs the preceding stage's context).
                Err(internal(sp, "ret stage reached emit_stage"))
            }
            StageKind::LoopJump => Err(diag(
                LCode::JumpMisplaced,
                sp,
                "`-> loop` outside a routing-guard jump arm",
            )),
            StageKind::Guard(arms) => {
                let wire =
                    cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "guard with no wire"))?;
                // Routing guards are handled by the loop recipe; a routing guard
                // reaching here is misplaced: inside a loop body but not the
                // final item → L1407; outside any loop → L1304.
                if arms
                    .iter()
                    .any(|a| crate::typing::arm_is_routing(&a.payload))
                {
                    if self.loop_depth > 0 {
                        return Err(diag(
                            LCode::RoutingGuardShape,
                            sp,
                            "routing guard is not the loop body's final item",
                        ));
                    }
                    return Err(diag(
                        LCode::JumpMisplaced,
                        sp,
                        "routing guard outside a loop body",
                    ));
                }
                let dest = self.lookahead_dest(next, ctx, is_last)?;
                let r = self.emit_phi_guard(fb, arms, wire, dest, sp)?;
                self.maybe_bind_next(fb, next, r);
                Ok(Some(r))
            }
            StageKind::Fanout { branches, .. } => {
                let wire =
                    cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "fanout with no wire"))?;
                self.emit_fanout(fb, branches, wire, next, ctx, sp)
            }
            StageKind::MapFold { op, params, body } => {
                let wire =
                    cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "map/fold with no wire"))?;
                let dest = self.lookahead_dest(next, ctx, is_last)?;
                let r = self.emit_map_fold(fb, *op, params, body, stage, wire, dest)?;
                self.maybe_bind_next(fb, next, r);
                Ok(Some(r))
            }
            StageKind::SeqBlock(body) => {
                let wire = cur.ok_or_else(|| diag(LCode::HeadlessChain, sp, "seq with no wire"))?;
                self.emit_seq_block(fb, body, wire, next, ctx, sp)
            }
            StageKind::StmtBlock(_) | StageKind::Error(_) => {
                Err(diag(LCode::UncleanTree, sp, "unclean stage"))
            }
        }
    }

    /// An `Expr` stage: a Var resolving to a local (rebind/bind), a fn (call),
    /// or `print`; anything else is L1302/L1106.
    fn emit_expr_stage(
        &mut self,
        fb: &mut FnBuilder,
        e: &Expr,
        next: Option<&Stage>,
        cur: Option<ObjectId>,
        ctx: &ChainCtx,
    ) -> ER<Option<ObjectId>> {
        match &e.kind {
            ExprKind::Var(n) => {
                let text = name_text(self.source, *n).to_string();
                let wire = cur;
                if let Some((bind, poisoned)) =
                    self.scope.resolve(&text).map(|(b, p)| (b.clone(), p))
                {
                    if poisoned {
                        return Err(diag(
                            LCode::ReadAfterLoop,
                            e.span,
                            format!(
                                "`{text}` is a loop-carried name read after its loop; bind the exit value instead"
                            ),
                        ));
                    }
                    // Existing local in stage position = a rebind (mut) target.
                    let wire = wire
                        .ok_or_else(|| diag(LCode::HeadlessChain, e.span, "rebind with no wire"))?;
                    if !bind.mutable {
                        return Err(diag(
                            LCode::AssignImmutable,
                            e.span,
                            format!("cannot reassign `{text}`: not `mut`"),
                        ));
                    }
                    // Rebind to a fresh object naming `text` (the wire's value).
                    // The wire value already lives in `wire`; alias the symbol.
                    let ty = self.ty_of(fb, wire);
                    self.rebind(&text, wire, ty, true);
                    Ok(Some(wire))
                } else if let Some(&fid) = self.fn_ids.get(&text) {
                    // call.
                    let wire =
                        wire.ok_or_else(|| diag(LCode::HeadlessChain, e.span, "call with no arg"))?;
                    let dest = self.lookahead_dest(next, ctx, next.is_none())?;
                    let r = self.emit_call(fb, fid, &text, wire, dest, e.span)?;
                    if let Some(r) = r {
                        self.maybe_bind_next(fb, next, r);
                    }
                    Ok(r)
                } else if crate::is_print_builtin(&text) {
                    let wire = wire
                        .ok_or_else(|| diag(LCode::HeadlessChain, e.span, "print with no value"))?;
                    self.emit_print(fb, wire, text == "println", e.span)?;
                    Ok(None) // print is a sink; chain ends.
                } else if crate::is_collection_builtin(&text) {
                    let wire = wire.ok_or_else(|| {
                        diag(LCode::HeadlessChain, e.span, "collection op with no source")
                    })?;
                    let dest = self.lookahead_dest(next, ctx, next.is_none())?;
                    let r = if text == "zip" {
                        self.emit_zip(fb, wire, dest, e.span)?
                    } else {
                        self.emit_enumerate(fb, wire, dest, e.span)?
                    };
                    self.maybe_bind_next(fb, next, r);
                    Ok(Some(r))
                } else {
                    // Unbound bare name = a fresh binding (LD1): the wire's value
                    // takes this name. The previous stage's lookahead Dest already
                    // named the object; bind the symbol to it.
                    let wire = wire.ok_or_else(|| {
                        diag(LCode::HeadlessChain, e.span, "binding with no wire")
                    })?;
                    let ty = self.ty_of(fb, wire);
                    self.bind_new(&text, wire, ty, false);
                    Ok(Some(wire))
                }
            }
            ExprKind::Member { base, field } => {
                // fn.param → L1106; else L1302.
                if let (ExprKind::Var(bn), MemberField::Named(_)) = (&base.kind, field) {
                    let bt = name_text(self.source, *bn);
                    if self.fn_ids.contains_key(bt) {
                        return Err(diag(
                            LCode::NamedParamApplication,
                            e.span,
                            "named-parameter partial application is out of Core",
                        ));
                    }
                }
                Err(diag(
                    LCode::ExprStage,
                    e.span,
                    "member expression does not consume the wire",
                ))
            }
            _ => Err(diag(
                LCode::ExprStage,
                e.span,
                "expression stage does not consume the wire",
            )),
        }
    }

    /// The one-stage-lookahead Dest (LD2). Returns `ER` because a `ret.k` slot
    /// `> u32::MAX` is rejected here with L1205 (the Session-04 truncation
    /// lesson). In an effectful fn with a declared return B present, a value
    /// producer whose next stage is `-> ret` does NOT target `Dest::Ret`: it
    /// produces Fresh and the `Ret` marker packs `(tok, value)` via
    /// `emit_ret_existing` (DESIGN §8.1 / LD18). For the fn-body / map-fold tail
    /// in such a fn, likewise produce Fresh and let `emit_ret_existing` /
    /// `emit_tail_as_return` pack the token.
    fn lookahead_dest(&self, next: Option<&Stage>, ctx: &ChainCtx, is_last: bool) -> ER<Dest> {
        if let Some(n) = next {
            match &n.kind {
                StageKind::Ret { proj } => {
                    if self.effectful_with_ret() {
                        // Full-tuple writer: produce Fresh, packed at the Ret marker.
                        return Ok(Dest::Fresh(None));
                    }
                    let slot = Self::ret_slot(*proj)?;
                    Ok(Dest::Ret { slot })
                }
                StageKind::Bind { name, .. } => {
                    Ok(Dest::Fresh(Some(name_text(self.source, *name).to_string())))
                }
                StageKind::Expr(e) => {
                    if let ExprKind::Var(n) = &e.kind {
                        let text = name_text(self.source, *n);
                        // bare name binding (unbound or mut) → name it.
                        if !self.fn_ids.contains_key(text)
                            && !crate::is_print_builtin(text)
                            && !crate::is_collection_builtin(text)
                        {
                            return Ok(Dest::Fresh(Some(text.to_string())));
                        }
                    }
                    Ok(Dest::Fresh(None))
                }
                _ => Ok(Dest::Fresh(None)),
            }
        } else if is_last {
            // No next stage: this is the chain tail. If the chain context is a
            // return position, target Ret — but an effectful B-present fn packs
            // the token at the marker / tail, so produce Fresh there too.
            match ctx {
                ChainCtx::FnBodyReturn { effectful: false } => Ok(Dest::Ret { slot: None }),
                ChainCtx::BodyReturn => Ok(Dest::Ret { slot: None }),
                _ => Ok(Dest::Fresh(None)),
            }
        } else {
            Ok(Dest::Fresh(None))
        }
    }

    /// If the next stage is a binding, bind the produced object to that name
    /// (the Dest already named it, but the symbol must rebind too).
    fn maybe_bind_next(&mut self, fb: &mut FnBuilder, next: Option<&Stage>, obj: ObjectId) {
        // Bare-name `Expr` stages self-bind when processed (LD1); only an explicit
        // `Bind` stage needs pre-binding here (the Dest already named it).
        if let Some(Stage {
            kind: StageKind::Bind { mut_span, name, ty },
            ..
        }) = next
        {
            let bty = ty
                .as_ref()
                .and_then(|t| self.types.resolve(self.source, t, &mut Vec::new()))
                .unwrap_or_else(|| self.ty_of(fb, obj));
            let bn = name_text(self.source, *name).to_string();
            self.bind_new(&bn, obj, bty, mut_span.is_some());
        }
    }

    // --- return writes ------------------------------------------------------

    /// `x -> ret` / `x -> ret.k` for a (possibly pre-existing) wire object.
    fn emit_ret_existing(
        &mut self,
        fb: &mut FnBuilder,
        wire: ObjectId,
        proj: Option<(u64, syn::SourceLoc)>,
        sp: syn::SourceLoc,
        ctx: &ChainCtx,
    ) -> ER<()> {
        // A `Str` value can only feed `print`; returning it is L1206.
        if matches!(self.tyq(wire), Ty::Str) {
            return Err(diag(
                LCode::StrOutsidePrint,
                sp,
                "string value used outside `print`",
            ));
        }
        // In an effectful fn with a declared return, each surface ret-write
        // consumes *the* final token, so at most one surface return site exists.
        // A second ret-write finds the token already consumed (None) — that is
        // L1307 (>1 surface ret-write), not the pure-output L1201 path (ATK-16).
        if self.token.is_none() && self.effectful_with_ret() {
            return Err(diag(
                LCode::EffectfulReturnShape,
                sp,
                "more than one surface return in an effectful function (each consumes the final token)",
            ));
        }
        let effectful = matches!(ctx, ChainCtx::FnBodyReturn { effectful: true })
            || (self.token.is_some() && self.in_effectful_fn());
        if effectful {
            // full-tuple writer: pack(tok, wire) → Dest::Ret{None}. ret.k banned.
            if proj.is_some() {
                return Err(diag(
                    LCode::EffectfulReturnShape,
                    sp,
                    "slot return in an effectful function",
                ));
            }
            let tok = self
                .token
                .ok_or_else(|| internal(sp, "effectful ret without token"))?;
            let r = self
                .pack(fb, &[tok, wire], Dest::Ret { slot: None }, sp)
                .map_err(|e| ir_err(e, sp))?;
            let _ = r;
            self.token = None;
        } else {
            // Reject a `ret.k` slot > u32::MAX (the Session-04 truncation lesson,
            // L1205) before casting — mirrors the `x.k` member guard.
            let slot = Self::ret_slot(proj)?;
            fb.output(wire, slot, ir_loc(sp))
                .map_err(|e| ir_err(e, sp))?;
        }
        Ok(())
    }

    fn in_effectful_fn(&self) -> bool {
        self.effects.is_effectful(&self.owner_name)
    }

    /// A value-less `-> ret` marker (no wire), §8.3 (ATK-09/ATK-10). The
    /// catalogue permits this statement-level / loop-exit / fn-body form. A
    /// `ret.k` slot here makes no sense (no value to write a slot of) — L1306. In
    /// a pure **Unit**-output fn the marker is a no-op (zero Return writers is
    /// legal for Unit — I-RET). In an **effectful B-absent** fn it writes the
    /// current token to Return (`output(tok, None)`) and consumes the register;
    /// any later token use then finds `None` → L1307. A non-Unit declared output
    /// was already L1306'd in D1 (return completeness), so it never reaches here
    /// with a live wire; defensively re-report L1306.
    fn emit_valueless_ret(
        &mut self,
        fb: &mut FnBuilder,
        proj: Option<(u64, syn::SourceLoc)>,
        sp: syn::SourceLoc,
    ) -> ER<()> {
        if proj.is_some() {
            return Err(diag(
                LCode::IncompleteReturn,
                sp,
                "value-less `-> ret.k` with no value to write",
            ));
        }
        let b_present = self
            .fn_sigs
            .get(&self.owner_name)
            .map(|s| s.ret.is_some())
            .unwrap_or(false);
        if self.in_effectful_fn() {
            if b_present {
                // Effectful B-present: a value-less ret cannot complete the
                // `(IoToken × B)` writer (no B value) — D1's L1306.
                return Err(diag(
                    LCode::IncompleteReturn,
                    sp,
                    "value-less `-> ret` in a function with a declared return",
                ));
            }
            // Effectful B-absent: write the current token to Return (the sole
            // writer is the final token — §8.1, golden g). Consume the register.
            let tok = self.token.ok_or_else(|| {
                // The token was already consumed by an earlier ret-terminal site
                // (ATK-08): a second surface return in an effectful fn.
                diag(
                    LCode::EffectfulReturnShape,
                    sp,
                    "token already consumed by an earlier return",
                )
            })?;
            fb.output(tok, None, ir_loc(sp))
                .map_err(|e| ir_err(e, sp))?;
            self.token = None;
            return Ok(());
        }
        // Pure fn. A non-Unit pure output with a value-less ret is L1306 (D1);
        // defensively re-report. A Unit pure output: no-op.
        if b_present {
            return Err(diag(
                LCode::IncompleteReturn,
                sp,
                "value-less `-> ret` in a function with a declared return",
            ));
        }
        Ok(())
    }

    /// Validate a `ret.k` slot index, rejecting any `k > u32::MAX` with L1205
    /// (the Session-04 truncation lesson; DESIGN §4 L1205). Returns the slot as
    /// `Option<u32>` (None for a bare `-> ret`). Mirrors the `x.k` member guard.
    fn ret_slot(proj: Option<(u64, syn::SourceLoc)>) -> ER<Option<u32>> {
        match proj {
            Some((k, sp)) => {
                if k > u32::MAX as u64 {
                    return Err(diag(
                        LCode::SlotOutOfRange,
                        sp,
                        "slot index exceeds u32::MAX",
                    ));
                }
                Ok(Some(k as u32))
            }
            None => Ok(None),
        }
    }

    /// Whether the enclosing fn is effectful with a declared return B present.
    /// In this case every surface ret-write lowers as a full-tuple writer
    /// `pack(tok, value) -> Dest::Ret { slot: None }` (DESIGN §8.1 / LD18), so a
    /// value-producing primitive whose next stage is `-> ret` must NOT take
    /// `Dest::Ret` — it produces Fresh, and `emit_ret_existing` packs the token.
    fn effectful_with_ret(&self) -> bool {
        self.in_effectful_fn()
            && self
                .fn_sigs
                .get(&self.owner_name)
                .map(|s| s.ret.is_some())
                .unwrap_or(false)
    }

    // --- expressions --------------------------------------------------------

    /// Lower an expression to an object (DESIGN §8.2).
    fn emit_expr(&mut self, fb: &mut FnBuilder, expr: &Expr) -> ER<ObjectId> {
        self.emit_expr_dest(fb, expr, Dest::Fresh(None))
    }

    /// Lower an expression, with the outermost primitive taking `dest`.
    fn emit_expr_dest(&mut self, fb: &mut FnBuilder, expr: &Expr, dest: Dest) -> ER<ObjectId> {
        let sp = expr.span;
        match &expr.kind {
            ExprKind::Int(v) => {
                let ty = self.lit_ty(sp).unwrap_or_else(Ty::i32);
                let val = self.int_value(*v, &ty, sp, false)?;
                self.constant(fb, val, sp)
            }
            ExprKind::Float => {
                let ty = self.lit_ty(sp).unwrap_or_else(Ty::f64);
                let val = self.float_value(sp, &ty)?;
                self.constant(fb, val, sp)
            }
            ExprKind::Bool(b) => self.constant(fb, Value::Bool(*b), sp),
            ExprKind::Str => {
                // A Str constant is a legal object; only a non-`print` *use*
                // (pack/binop/ret of a non-Str) is L1206 — the builder's
                // `StrOutsidePrint` maps to L1206. `print` consumes it fine.
                let raw = src_slice(self.source, sp);
                let unesc = flow_syntax::unescape_string(raw);
                self.constant(fb, Value::Str(unesc), sp)
            }
            ExprKind::Var(n) => {
                let text = name_text(self.source, *n);
                match self.scope.resolve(text) {
                    Some((b, false)) => Ok(b.obj),
                    Some((_, true)) => Err(diag(
                        LCode::ReadAfterLoop,
                        sp,
                        format!("`{text}` read after its loop; bind the exit value instead"),
                    )),
                    None => {
                        if self.fn_ids.contains_key(text) {
                            Err(diag(
                                LCode::FunctionAsValue,
                                sp,
                                format!("function `{text}` used as a value"),
                            ))
                        } else {
                            Err(diag(
                                LCode::UnknownName,
                                sp,
                                format!("unresolved name `{text}`"),
                            ))
                        }
                    }
                }
            }
            ExprKind::Hole => Err(internal(sp, "bare Hole in expression position")),
            ExprKind::Unary { op, operand, .. } => match op {
                UnOp::Neg => {
                    // Fold negative literals (pin 3).
                    if let ExprKind::Int(v) = &operand.kind {
                        let ty = self.lit_ty(operand.span).unwrap_or_else(Ty::i32);
                        let val = self.int_value(*v, &ty, operand.span, true)?;
                        return self.constant(fb, val, sp);
                    }
                    if let ExprKind::Float = &operand.kind {
                        let ty = self.lit_ty(operand.span).unwrap_or_else(Ty::f64);
                        let val = self.float_value_neg(operand.span, &ty)?;
                        return self.constant(fb, val, sp);
                    }
                    let x = self.emit_expr(fb, operand)?;
                    self.unop(fb, Operation::Neg, x, dest, sp)
                }
                UnOp::Not => {
                    let x = self.emit_expr(fb, operand)?;
                    self.unop(fb, Operation::Not, x, dest, sp)
                }
            },
            ExprKind::Binary {
                op,
                lhs,
                rhs,
                op_span,
            } => {
                let l = self.emit_expr(fb, lhs)?;
                let r = self.emit_expr(fb, rhs)?;
                self.binop(fb, map_binop(*op), l, r, dest, *op_span)
            }
            ExprKind::Member { base, field } => {
                let b = self.emit_expr(fb, base)?;
                let bty = self.ty_of(fb, b);
                match field {
                    MemberField::Named(fname) => {
                        let fn_ = name_text(self.source, *fname);
                        let idx = match &bty {
                            Ty::Struct { fields, .. } => fields.iter().position(|(n, _)| n == fn_),
                            _ => None,
                        };
                        match idx {
                            Some(i) => self.proj(fb, b, i as u32, dest, sp),
                            None => Err(diag(
                                LCode::UnknownField,
                                sp,
                                format!("`{fn_}` is not a field"),
                            )),
                        }
                    }
                    MemberField::Index { value, span } => {
                        if matches!(bty, Ty::Array { .. }) {
                            return Err(diag(
                                LCode::NotAProduct,
                                *span,
                                "index on an array; use `arr[k]`",
                            ));
                        }
                        if *value > u32::MAX as u64 {
                            return Err(diag(
                                LCode::SlotOutOfRange,
                                *span,
                                "index exceeds u32::MAX",
                            ));
                        }
                        self.proj(fb, b, *value as u32, dest, sp)
                    }
                }
            }
            ExprKind::Index { base, index } => {
                let a = self.emit_expr(fb, base)?;
                let i = self.emit_expr(fb, index)?;
                self.index(fb, a, i, dest, sp)
            }
            ExprKind::Tuple(es) => {
                let mut comps = Vec::with_capacity(es.len());
                for e in es {
                    comps.push(self.emit_expr(fb, e)?);
                }
                self.pack(fb, &comps, dest, sp).map_err(|e| ir_err(e, sp))
            }
            ExprKind::Array(es) => {
                if es.is_empty() {
                    return Err(diag(LCode::EmptyArray, sp, "empty array literal"));
                }
                let mut comps = Vec::with_capacity(es.len());
                for e in es {
                    comps.push(self.emit_expr(fb, e)?);
                }
                self.pack_array(fb, &comps, dest, sp)
            }
            ExprKind::Struct { name, fields } => {
                let tname = name_text(self.source, *name);
                let sty = self.types.get(tname).cloned().ok_or_else(|| {
                    diag(LCode::UnknownType, sp, format!("unknown type `{tname}`"))
                })?;
                let decl = match &sty {
                    Ty::Struct { fields, .. } => fields.clone(),
                    _ => return Err(internal(sp, "non-struct in struct ctor")),
                };
                // Evaluate inits, reorder to decl order.
                let mut by_name: BTreeMap<String, ObjectId> = BTreeMap::new();
                for fi in fields {
                    let fname = name_text(self.source, fi.name).to_string();
                    let v = match &fi.value {
                        Some(e) => self.emit_expr(fb, e)?,
                        None => {
                            // pun: read the symbol.
                            match self.scope.resolve(&fname) {
                                Some((b, false)) => b.obj,
                                _ => {
                                    return Err(diag(
                                        LCode::UnknownName,
                                        fi.span,
                                        format!("`{fname}` not in scope"),
                                    ));
                                }
                            }
                        }
                    };
                    by_name.insert(fname, v);
                }
                let mut comps = Vec::with_capacity(decl.len());
                for (fname, _) in &decl {
                    match by_name.get(fname) {
                        Some(&o) => comps.push(o),
                        None => {
                            return Err(diag(
                                LCode::TypeMismatch,
                                sp,
                                format!("missing field `{fname}`"),
                            ));
                        }
                    }
                }
                self.pack_struct(fb, sty, &comps, dest, sp)
                    .map_err(|e| ir_err(e, sp))
            }
            ExprKind::Call { .. } | ExprKind::Question(_) | ExprKind::Error => {
                Err(diag(LCode::UncleanTree, sp, "unclean expression node"))
            }
        }
    }

    /// Lower an OpShorthand expr where the leftmost Hole leaf is `wire`.
    fn emit_op_shorthand(
        &mut self,
        fb: &mut FnBuilder,
        expr: &Expr,
        wire: ObjectId,
        dest: Dest,
    ) -> ER<ObjectId> {
        self.emit_hole_expr(fb, expr, wire, dest)
    }

    fn emit_hole_expr(
        &mut self,
        fb: &mut FnBuilder,
        expr: &Expr,
        wire: ObjectId,
        dest: Dest,
    ) -> ER<ObjectId> {
        let sp = expr.span;
        match &expr.kind {
            ExprKind::Hole => Ok(wire),
            ExprKind::Unary { op, operand, .. } => {
                let x = self.emit_hole_expr(fb, operand, wire, Dest::Fresh(None))?;
                let o = match op {
                    UnOp::Neg => Operation::Neg,
                    UnOp::Not => Operation::Not,
                };
                self.unop(fb, o, x, dest, sp)
            }
            ExprKind::Binary {
                op,
                lhs,
                rhs,
                op_span,
            } => {
                let l = self.emit_hole_expr(fb, lhs, wire, Dest::Fresh(None))?;
                let r = self.emit_hole_expr(fb, rhs, wire, Dest::Fresh(None))?;
                self.binop(fb, map_binop(*op), l, r, dest, *op_span)
            }
            _ => self.emit_expr_dest(fb, expr, dest),
        }
    }

    // --- calls / print ------------------------------------------------------

    fn emit_call(
        &mut self,
        fb: &mut FnBuilder,
        callee: FuncId,
        callee_name: &str,
        arg: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<Option<ObjectId>> {
        let csig = self.fn_sigs.get(callee_name).expect("callee sig").clone();
        if csig.effectful {
            let tok = self.consume_token(sp)?;
            // arg' = pack(tok, arg) (or tok alone when A absent — but a call
            // stage always has a wire arg here).
            let arg2 = self
                .pack(fb, &[tok, arg], Dest::Fresh(None), sp)
                .map_err(|e| ir_err(e, sp))?;
            let r = fb
                .call(callee, arg2, Dest::Fresh(None), ir_loc(sp))
                .map_err(|e| ir_err(e, sp))?;
            // Derives-from-merge: any derived operand ⇒ derived result (§7.3).
            // The call result derives from its argument (the packed `(tok, arg)`);
            // propagate to the result and both projs (ATK-12).
            self.prop(r, &[arg2, tok, arg]);
            if let Some(rty) = &csig.ret {
                // result is (IoToken, B). tok := proj(r,0); wire := proj(r,1).
                self.record(r, Ty::Tuple(vec![Ty::IoToken, rty.clone()]));
                let nt = self.proj(fb, r, 0, Dest::Fresh(None), sp)?;
                let wire = self.proj(fb, r, 1, Dest::Fresh(None), sp)?;
                // `proj` already propagates from its source `r`; both projs are
                // derived iff `r` is.
                self.token = Some(nt);
                let _ = dest;
                Ok(Some(wire))
            } else {
                // B absent: r : IoToken; tok := r directly (no proj). Chain ends.
                self.record(r, Ty::IoToken);
                self.token = Some(r);
                Ok(None)
            }
        } else {
            let out = csig.ret.clone().unwrap_or(Ty::Unit);
            let r = fb
                .call(callee, arg, dest, ir_loc(sp))
                .map_err(|e| ir_err(e, sp))?;
            self.record(r, out);
            // Derives-from-merge: a pure-call result derives from its argument
            // (§7.3 / ATK-12), so a guard cond flowing through a pure call passes
            // the L1503 derivation test.
            self.prop(r, &[arg]);
            Ok(Some(r))
        }
    }

    /// The current token for an effect (print / effectful call). If the token
    /// register is `None` inside an *effectful* fn, the final token was already
    /// consumed by a ret-write or a ret-terminal loop exit (§8.5) — a later
    /// effect is L1307, not the internal "print in pure fn" path (ATK-08). In a
    /// genuinely pure fn the None is a lower bug (L1901).
    fn consume_token(&self, sp: syn::SourceLoc) -> ER<ObjectId> {
        self.token.ok_or_else(|| {
            if self.in_effectful_fn() {
                diag(
                    LCode::EffectfulReturnShape,
                    sp,
                    "effect after the function's return consumed the token",
                )
            } else {
                internal(sp, "effect in a pure fn")
            }
        })
    }

    fn emit_print(
        &mut self,
        fb: &mut FnBuilder,
        value: ObjectId,
        newline: bool,
        sp: syn::SourceLoc,
    ) -> ER<()> {
        let tok = self.consume_token(sp)?;
        // Printability: value ty must be numeric/bool/str.
        let vty = self.ty_of(fb, value);
        if !vty.is_printable() {
            return Err(diag(
                LCode::Unprintable,
                sp,
                format!("cannot print `{}`", ty_name(&vty)),
            ));
        }
        let res = if newline {
            fb.println(tok, value, ir_loc(sp))
        } else {
            fb.print(tok, value, ir_loc(sp))
        };
        let nt = res.map_err(|e| ir_err(e, sp))?;
        self.record(nt, Ty::IoToken);
        self.token = Some(nt);
        Ok(())
    }

    // --- literals -----------------------------------------------------------

    /// The resolved ty for the literal at `sp` (from D1), if recorded.
    fn lit_ty(&self, sp: syn::SourceLoc) -> Option<Ty> {
        self.info.lit(sp).cloned()
    }

    /// Build the `Value` for an integer literal `v` of resolved ty `ty`,
    /// range-checking (L1202). `neg` negates (a folded `-lit`, pin 3).
    fn int_value(&self, v: u64, ty: &Ty, sp: syn::SourceLoc, neg: bool) -> ER<Value> {
        macro_rules! oor {
            () => {
                diag(
                    LCode::LiteralOutOfRange,
                    sp,
                    format!(
                        "literal {}{v} does not fit `{}`",
                        if neg { "-" } else { "" },
                        ty_name(ty)
                    ),
                )
            };
        }
        match ty {
            Ty::Int {
                bits: 32,
                signed: true,
            } => {
                if neg {
                    let nv = -(v as i128);
                    if nv < i32::MIN as i128 {
                        return Err(oor!());
                    }
                    Ok(Value::I32(nv as i32))
                } else if v <= i32::MAX as u64 {
                    Ok(Value::I32(v as i32))
                } else {
                    Err(oor!())
                }
            }
            Ty::Int {
                bits: 64,
                signed: true,
            } => {
                if neg {
                    let nv = -(v as i128);
                    if nv < i64::MIN as i128 {
                        return Err(oor!());
                    }
                    Ok(Value::I64(nv as i64))
                } else if v <= i64::MAX as u64 {
                    Ok(Value::I64(v as i64))
                } else {
                    Err(oor!())
                }
            }
            Ty::Int {
                bits: 8,
                signed: false,
            } => {
                if neg {
                    return Err(oor!());
                }
                if v <= u8::MAX as u64 {
                    Ok(Value::U8(v as u8))
                } else {
                    Err(oor!())
                }
            }
            _ => Err(diag(
                LCode::LiteralTypeConflict,
                sp,
                format!("integer literal cannot have type `{}`", ty_name(ty)),
            )),
        }
    }

    /// Build the `Value` for a float literal at `sp` of resolved ty `ty`.
    fn float_value(&self, sp: syn::SourceLoc, ty: &Ty) -> ER<Value> {
        self.float_value_signed(sp, ty, false)
    }
    fn float_value_neg(&self, sp: syn::SourceLoc, ty: &Ty) -> ER<Value> {
        self.float_value_signed(sp, ty, true)
    }
    fn float_value_signed(&self, sp: syn::SourceLoc, ty: &Ty, neg: bool) -> ER<Value> {
        let raw = src_slice(self.source, sp);
        match ty {
            Ty::Float { bits: 32 } => {
                let f: f32 = raw.parse().map_err(|_| {
                    diag(
                        LCode::LiteralOutOfRange,
                        sp,
                        format!("invalid f32 literal `{raw}`"),
                    )
                })?;
                Ok(Value::F32(if neg { -f } else { f }))
            }
            Ty::Float { bits: 64 } => {
                let f: f64 = raw.parse().map_err(|_| {
                    diag(
                        LCode::LiteralOutOfRange,
                        sp,
                        format!("invalid f64 literal `{raw}`"),
                    )
                })?;
                Ok(Value::F64(if neg { -f } else { f }))
            }
            _ => Err(diag(
                LCode::LiteralTypeConflict,
                sp,
                format!("float literal cannot have type `{}`", ty_name(ty)),
            )),
        }
    }

    // --- guards (Phi) -------------------------------------------------------

    fn emit_phi_guard(
        &mut self,
        fb: &mut FnBuilder,
        arms: &[GuardArm],
        scrutinee: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        // Classify: bool guard vs value-match.
        let scrut_ty = self.ty_of(fb, scrutinee);
        let has_int = arms.iter().any(|a| matches!(a.discr, GuardDiscr::Int(_)));
        let has_bool = arms
            .iter()
            .any(|a| matches!(a.discr, GuardDiscr::True | GuardDiscr::False));
        if has_int && has_bool {
            return Err(diag(
                LCode::GuardArmMixed,
                sp,
                "bool and integer discriminants in one guard",
            ));
        }
        if has_int {
            self.emit_value_match(fb, arms, scrutinee, &scrut_ty, dest, sp)
        } else {
            self.emit_bool_guard(fb, arms, scrutinee, &scrut_ty, dest, sp)
        }
    }

    fn emit_bool_guard(
        &mut self,
        fb: &mut FnBuilder,
        arms: &[GuardArm],
        scrutinee: ObjectId,
        scrut_ty: &Ty,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        if *scrut_ty != Ty::Bool {
            return Err(diag(
                LCode::GuardScrutineeType,
                sp,
                "bool guard on non-bool scrutinee",
            ));
        }
        // Find true / false arms (a Default may stand for a missing pole).
        let mut t_arm: Option<&GuardArm> = None;
        let mut f_arm: Option<&GuardArm> = None;
        let mut default: Option<&GuardArm> = None;
        for a in arms {
            match a.discr {
                GuardDiscr::True => {
                    if t_arm.is_some() {
                        return Err(diag(
                            LCode::GuardArmDuplicate,
                            a.discr_span,
                            "duplicate `-true->`",
                        ));
                    }
                    t_arm = Some(a);
                }
                GuardDiscr::False => {
                    if f_arm.is_some() {
                        return Err(diag(
                            LCode::GuardArmDuplicate,
                            a.discr_span,
                            "duplicate `-false->`",
                        ));
                    }
                    f_arm = Some(a);
                }
                GuardDiscr::Default => {
                    // A duplicate `-_->` is a duplicate discriminant (L1402; ATK-15b).
                    if default.is_some() {
                        return Err(diag(
                            LCode::GuardArmDuplicate,
                            a.discr_span,
                            "duplicate `-_->` default arm",
                        ));
                    }
                    default = Some(a);
                }
                GuardDiscr::Int(_) => {
                    return Err(diag(
                        LCode::GuardArmMixed,
                        a.discr_span,
                        "integer arm in bool guard",
                    ));
                }
                GuardDiscr::OutOfCore => {
                    return Err(diag(LCode::UncleanTree, a.discr_span, "out-of-core arm"));
                }
            }
        }
        // A `-_->` may stand for THE single missing pole (§8.4, singular). With
        // both concrete poles present, the default is an unreachable arm (L1402;
        // ATK-15c). With BOTH poles missing (default-only), it cannot stand for
        // two poles → L1401 (ATK-15a).
        if let Some(d) = default
            && t_arm.is_some()
            && f_arm.is_some()
        {
            return Err(diag(
                LCode::GuardArmDuplicate,
                d.discr_span,
                "unreachable `-_->` default arm: both `-true->` and `-false->` are present",
            ));
        }
        let t = t_arm.or(default);
        let f = f_arm.or(default);
        let (t, f) = match (t, f) {
            // Default-only (both concrete poles missing): the default cannot
            // stand for both poles → L1401 (ATK-15a). `t` and `f` would alias the
            // same default arm here, which is exactly the missing-poles case.
            (Some(t), Some(f)) if t_arm.is_some() || f_arm.is_some() => (t, f),
            _ => {
                return Err(diag(
                    LCode::GuardArmMissing,
                    sp,
                    "bool guard without both poles",
                ));
            }
        };
        self.check_phi_arm(&t.payload)?;
        self.check_phi_arm(&f.payload)?;
        let t_val = self.emit_arm_value(fb, &t.payload, scrutinee)?;
        let f_val = self.emit_arm_value(fb, &f.payload, scrutinee)?;
        // Use the `self.phi` wrapper (NOT a bare `fb.phi`) so the result ty is
        // recorded and the derives-from-merge tag propagates — otherwise the
        // result defaults to Ty::Unit and is absent from `self.derived`,
        // falsely rejecting downstream print/route/loop uses (ATK-04).
        self.phi(fb, t_val, f_val, scrutinee, dest, sp)
    }

    fn emit_value_match(
        &mut self,
        fb: &mut FnBuilder,
        arms: &[GuardArm],
        scrutinee: ObjectId,
        scrut_ty: &Ty,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        if !scrut_ty.is_integer() {
            return Err(diag(
                LCode::GuardScrutineeType,
                sp,
                "integer discriminants on non-integer scrutinee",
            ));
        }
        // Collect ordered int arms + the default (innermost).
        let mut int_arms: Vec<(&GuardArm, u64)> = Vec::new();
        let mut default: Option<&GuardArm> = None;
        let mut seen: BTreeMap<u64, ()> = BTreeMap::new();
        for a in arms {
            match a.discr {
                GuardDiscr::Int(k) => {
                    if seen.contains_key(&k) {
                        return Err(diag(
                            LCode::GuardArmDuplicate,
                            a.discr_span,
                            "duplicate discriminant",
                        ));
                    }
                    seen.insert(k, ());
                    int_arms.push((a, k));
                }
                GuardDiscr::Default => default = Some(a),
                GuardDiscr::True | GuardDiscr::False => {
                    return Err(diag(
                        LCode::GuardArmMixed,
                        a.discr_span,
                        "bool arm in value match",
                    ));
                }
                GuardDiscr::OutOfCore => {
                    return Err(diag(LCode::UncleanTree, a.discr_span, "out-of-core arm"));
                }
            }
        }
        let default = default
            .ok_or_else(|| diag(LCode::GuardArmMissing, sp, "value match without `-_->`"))?;
        // Check arms, compute conds in arm order, then right-fold phi.
        for (a, _) in &int_arms {
            self.check_phi_arm(&a.payload)?;
        }
        self.check_phi_arm(&default.payload)?;
        // cond_i = Eq(scrutinee, const k_i).
        let mut conds: Vec<ObjectId> = Vec::new();
        for (a, k) in &int_arms {
            let kval = self.int_value(*k, scrut_ty, a.discr_span, false)?;
            let kc = self.constant(fb, kval, a.discr_span)?;
            let c = self.binop(
                fb,
                Operation::Eq,
                scrutinee,
                kc,
                Dest::Fresh(None),
                a.discr_span,
            )?;
            conds.push(c);
        }
        // arm values.
        let mut arm_vals: Vec<ObjectId> = Vec::new();
        for (a, _) in &int_arms {
            arm_vals.push(self.emit_arm_value(fb, &a.payload, scrutinee)?);
        }
        let dval = self.emit_arm_value(fb, &default.payload, scrutinee)?;
        // Right-fold: innermost phi(e_n, e_d, c_n), outward; outermost takes dest.
        let n = int_arms.len();
        if n == 0 {
            return Err(diag(LCode::GuardArmMissing, sp, "value match with no arms"));
        }
        // Build innermost first.
        let mut acc = {
            let i = n - 1;
            self.phi(
                fb,
                arm_vals[i],
                dval,
                conds[i],
                if n == 1 {
                    dest.clone()
                } else {
                    Dest::Fresh(None)
                },
                int_arms[i].0.discr_span,
            )?
        };
        for i in (0..n - 1).rev() {
            let d = if i == 0 {
                dest.clone()
            } else {
                Dest::Fresh(None)
            };
            acc = self.phi(fb, arm_vals[i], acc, conds[i], d, int_arms[i].0.discr_span)?;
        }
        Ok(acc)
    }

    /// Phi-arm restrictions: no ret (L1405), no effects (L1404), no
    /// enclosing-mut assignment (L1408).
    fn check_phi_arm(&self, payload: &ArmPayload) -> ER<()> {
        let block_or_chain: PhiArmScan = scan_phi_arm(self.source, self.fn_sigs, payload);
        if let Some(sp) = block_or_chain.ret_span {
            return Err(diag(
                LCode::GuardRetInPhiArm,
                sp,
                "`-> ret` inside a Phi-position arm",
            ));
        }
        if let Some(sp) = block_or_chain.effect_span {
            return Err(diag(
                LCode::GuardArmEffectful,
                sp,
                "effectful operation in a Phi-position arm",
            ));
        }
        if let Some(sp) = block_or_chain.loopjump_span {
            return Err(diag(
                LCode::JumpMisplaced,
                sp,
                "`-> loop` in a Phi-position arm",
            ));
        }
        // Enclosing-mut assignment detection.
        for (name, sp) in &block_or_chain.assigns {
            if let Some((b, _)) = self.scope.resolve(name)
                && b.mutable
            {
                return Err(diag(
                    LCode::AssignInPhiArm,
                    *sp,
                    format!("assignment to enclosing `mut` `{name}` in a Phi arm"),
                ));
            }
        }
        Ok(())
    }

    /// Emit an arm payload's value (in a child scope against the snapshot).
    fn emit_arm_value(
        &mut self,
        fb: &mut FnBuilder,
        payload: &ArmPayload,
        scrutinee: ObjectId,
    ) -> ER<ObjectId> {
        self.scope.push();
        let r = self.emit_arm_value_inner(fb, payload, scrutinee);
        self.scope.pop();
        r
    }

    fn emit_arm_value_inner(
        &mut self,
        fb: &mut FnBuilder,
        payload: &ArmPayload,
        scrutinee: ObjectId,
    ) -> ER<ObjectId> {
        match payload {
            ArmPayload::Chain(c) => {
                let r = self.emit_chain(fb, c, ChainCtx::HeadlessSeed(scrutinee))?;
                r.ok_or_else(|| diag(LCode::TypeMismatch, c.span, "guard arm produces no value"))
            }
            ArmPayload::Block(b) => {
                for item in &b.items {
                    if let BlockItem::Stmt(s) = item {
                        self.emit_stmt(fb, s)?;
                    }
                }
                match &b.tail {
                    Some(t) => {
                        let r = self.emit_chain(fb, t, ChainCtx::HeadlessSeed(scrutinee))?;
                        r.ok_or_else(|| {
                            diag(LCode::TypeMismatch, t.span, "guard arm produces no value")
                        })
                    }
                    None => Err(diag(
                        LCode::TypeMismatch,
                        b.span,
                        "guard arm produces no value",
                    )),
                }
            }
        }
    }

    // --- fanout -------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn emit_fanout(
        &mut self,
        fb: &mut FnBuilder,
        branches: &[Chain],
        source: ObjectId,
        next: Option<&Stage>,
        ctx: &ChainCtx,
        sp: syn::SourceLoc,
    ) -> ER<Option<ObjectId>> {
        // Branches are headless chains seeded with the fanout source, lowered in
        // the enclosing scope (no child scope). No join unless the chain
        // continues past the fanout.
        let continues =
            next.is_some() || matches!(ctx, ChainCtx::FnBodyReturn { .. } | ChainCtx::BodyReturn);
        let mut tails: Vec<ObjectId> = Vec::new();
        for br in branches {
            let r = self.emit_chain(fb, br, ChainCtx::HeadlessSeed(source))?;
            if continues {
                match r {
                    Some(o) => tails.push(o),
                    None => {
                        return Err(diag(
                            LCode::FanoutNoValue,
                            br.span,
                            "fanout branch produces no value",
                        ));
                    }
                }
            }
        }
        if !continues {
            return Ok(None);
        }
        // Materialize the join.
        if tails.len() == 1 {
            Ok(Some(tails[0])) // single-branch fanout: no pack (FAN-1).
        } else {
            let dest = self.lookahead_dest(next, ctx, next.is_none())?;
            let r = self.pack(fb, &tails, dest, sp).map_err(|e| ir_err(e, sp))?;
            self.maybe_bind_next(fb, next, r);
            Ok(Some(r))
        }
    }

    // --- seq ----------------------------------------------------------------

    /// `seq { … }` (ADR-0019): an ordered statement block in stage position.
    /// `seq` has **no IR footprint** (pin d) — its ordering guarantee *is* the
    /// token thread that source-order statement lowering already produces. The
    /// statements lower in order **in the enclosing scope** (no child scope —
    /// bindings escape, pin b); a headless chain statement seeds from the seq
    /// input `wire` (pin a — the old bare-chain branch form still means the
    /// same). The seq's value is its tail chain's value (pin c); a seq that
    /// continues (a following stage, or return position) with no tail value is
    /// L1611.
    fn emit_seq_block(
        &mut self,
        fb: &mut FnBuilder,
        block: &Block,
        wire: ObjectId,
        next: Option<&Stage>,
        ctx: &ChainCtx,
        sp: syn::SourceLoc,
    ) -> ER<Option<ObjectId>> {
        // ponytail: a chain statement routes through `emit_chain(HeadlessSeed)`
        // (so a headless one seeds from the wire, pin a); Bind/Loop reuse
        // `emit_stmt`. This is the one place seq diverges from `emit_block`'s
        // non-seeding statement loop.
        for item in &block.items {
            if let BlockItem::Stmt(s) = item {
                match &s.kind {
                    StmtKind::Chain(c) => {
                        self.emit_chain(fb, c, ChainCtx::HeadlessSeed(wire))?;
                    }
                    _ => self.emit_stmt(fb, s)?,
                }
            }
        }
        let continues = next.is_some()
            || matches!(
                ctx,
                ChainCtx::FnBodyReturn { .. } | ChainCtx::BodyReturn | ChainCtx::RetValue
            );
        let Some(tail) = &block.tail else {
            if continues {
                return Err(diag(
                    LCode::SeqNoValue,
                    sp,
                    "chain continues past `seq` but the block has no tail value",
                ));
            }
            return Ok(None);
        };
        let value = self.emit_chain(fb, tail, ChainCtx::HeadlessSeed(wire))?;
        if !continues {
            return Ok(value);
        }
        let value = value.ok_or_else(|| {
            diag(
                LCode::SeqNoValue,
                tail.span,
                "`seq` tail produces no value but the chain continues",
            )
        })?;
        // The seq value comes bare off its tail (the tail lowered under
        // HeadlessSeed, so no lookahead `Dest::Ret` was taken). If the seq is the
        // final stage of a return-position chain, write the value to Return here —
        // the fn-body / map-fold-body tail handler ignores the returned wire and
        // relies on the last stage having targeted Ret. Otherwise hand the value
        // back as the wire: a following `-> ret` / bind / call stage consumes it
        // exactly as a pre-existing wire (so `SeqBlock` is not a
        // `stage_writes_value` — the `-> ret` marker routes through
        // `emit_ret_existing`).
        if next.is_none() && matches!(ctx, ChainCtx::FnBodyReturn { .. } | ChainCtx::BodyReturn) {
            self.emit_ret_existing(fb, value, None, tail.span, ctx)?;
            return Ok(None);
        }
        Ok(Some(value))
    }

    // --- map / fold ---------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn emit_map_fold(
        &mut self,
        fb: &mut FnBuilder,
        op: CollOp,
        _params: &[syn::Name],
        _body: &Block,
        stage: &Stage,
        wire: ObjectId,
        dest: Dest,
    ) -> ER<ObjectId> {
        let sp = stage.span;
        let bid = *self
            .body_ids
            .get(&(sp.start, sp.end))
            .ok_or_else(|| internal(sp, "missing body id"))?;
        let sig = self.info.block(sp).cloned();
        match op {
            CollOp::Map => {
                let wty = self.ty_of(fb, wire);
                let size = match &wty {
                    Ty::Array { size, .. } => *size,
                    _ => return Err(diag(LCode::MapNonArray, sp, "map applied to a non-array")),
                };
                let out = Ty::Array {
                    elem: Box::new(sig.map(|s| s.body_out).unwrap_or(Ty::Unit)),
                    size,
                };
                let o = fb
                    .map(bid, wire, dest, ir_loc(sp))
                    .map_err(|e| ir_err(e, sp))?;
                Ok(self.record(o, out))
            }
            CollOp::Fold => {
                let wty = self.ty_of(fb, wire);
                match &wty {
                    Ty::Tuple(ts) if ts.len() == 2 && matches!(ts[1], Ty::Array { .. }) => {}
                    _ => return Err(diag(LCode::FoldShape, sp, "fold wire is not (init, array)")),
                }
                let out = sig.map(|s| s.body_out).unwrap_or(Ty::Unit);
                let o = fb
                    .fold(bid, wire, dest, ir_loc(sp))
                    .map_err(|e| ir_err(e, sp))?;
                Ok(self.record(o, out))
            }
        }
    }

    // --- collection builtins (ADR-0018) -------------------------------------

    /// `zip` (ADR-0018): source is a 2-tuple `([A;n], [B;n])`, result `[(A,B);n]`.
    /// Independently re-derives the shape here (owning L1606/L1607/L1608) before
    /// calling the builder, which re-checks defensively (the builder is not the
    /// diagnostic surface — LD12). Projects the two arrays out of the tuple wire
    /// (Pair-then-primitive; the builder re-pairs, exactly as `binop`).
    fn emit_zip(
        &mut self,
        fb: &mut FnBuilder,
        wire: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let wty = self.ty_of(fb, wire);
        let comps = match &wty {
            Ty::Tuple(ts) if ts.len() == 2 => ts.clone(),
            _ => {
                return Err(diag(
                    LCode::ZipNonTuple,
                    sp,
                    "zip source is not a 2-tuple of arrays",
                ));
            }
        };
        let (a_elem, a_size) = match &comps[0] {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => {
                return Err(diag(
                    LCode::ZipNonArray,
                    sp,
                    "zip component 0 is not an array",
                ));
            }
        };
        let (b_elem, b_size) = match &comps[1] {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => {
                return Err(diag(
                    LCode::ZipNonArray,
                    sp,
                    "zip component 1 is not an array",
                ));
            }
        };
        if a_size != b_size {
            return Err(diag(
                LCode::ZipSizeMismatch,
                sp,
                format!("zip arrays differ in length: {a_size} vs {b_size}"),
            ));
        }
        // Project the two arrays out of the tuple wire, then hand to the builder.
        let lhs = self.proj(fb, wire, 0, Dest::Fresh(None), sp)?;
        let rhs = self.proj(fb, wire, 1, Dest::Fresh(None), sp)?;
        let out = Ty::Array {
            elem: Box::new(Ty::Tuple(vec![a_elem, b_elem])),
            size: a_size,
        };
        let o = fb
            .zip(lhs, rhs, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        Ok(self.record(o, out))
    }

    /// `enumerate` (ADR-0018): source `[A;n]`, result `[(i32,A);n]`. Owns L1609
    /// (non-array) and L1610 (n > i32::MAX — the index `i32` could not name every
    /// element, F4/SND-3 precedent); the builder re-derives the bound defensively.
    fn emit_enumerate(
        &mut self,
        fb: &mut FnBuilder,
        wire: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let wty = self.ty_of(fb, wire);
        let (elem, size) = match &wty {
            Ty::Array { elem, size } => ((**elem).clone(), *size),
            _ => {
                return Err(diag(
                    LCode::EnumerateNonArray,
                    sp,
                    "enumerate applied to a non-array",
                ));
            }
        };
        if size > i32::MAX as u64 {
            return Err(diag(
                LCode::EnumerateOverflow,
                sp,
                format!("enumerate array length {size} exceeds i32::MAX"),
            ));
        }
        let out = Ty::Array {
            elem: Box::new(Ty::Tuple(vec![Ty::i32(), elem])),
            size,
        };
        let o = fb
            .enumerate(wire, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        Ok(self.record(o, out))
    }

    // --- loops --------------------------------------------------------------

    fn emit_loop(&mut self, fb: &mut FnBuilder, l: &syn::LoopStmt) -> ER<()> {
        let sp = l.span;
        // 1. Discover the carried set: enclosing-scope `mut` names assigned in
        //    the body (params-then-bindings declaration order), token last iff
        //    the body has effects (§8.5/LD4).
        let mut assigned: Vec<String> = Vec::new();
        collect_loop_assigns(self.source, &l.body, &mut assigned);
        // Keep only names that resolve to an enclosing-scope mut binding, in
        // declaration order (= the order they were bound; we approximate with
        // resolution order, deduplicated).
        let mut carried: Vec<(String, Ty, ObjectId, u32)> = Vec::new();
        let mut seen: BTreeMap<String, ()> = BTreeMap::new();
        for name in &assigned {
            if seen.contains_key(name) {
                continue;
            }
            if let Some((b, _)) = self.scope.resolve(name)
                && b.mutable
            {
                seen.insert(name.clone(), ());
                carried.push((name.clone(), b.ty.clone(), b.obj, b.decl_seq));
            }
        }
        // Carried order = declaration order (LD4): params first, then bindings in
        // source order — exactly the `decl_seq` ordering.
        carried.sort_by_key(|(_, _, _, seq)| *seq);
        let carried: Vec<(String, Ty, ObjectId)> =
            carried.into_iter().map(|(n, t, o, _)| (n, t, o)).collect();
        let body_effectful = loop_body_has_effect(self.source, &l.body, self.fn_sigs);
        if carried.is_empty() {
            return Err(diag(
                LCode::LoopNoState,
                sp,
                "loop assigns no enclosing `mut` variable",
            ));
        }
        // L1504 (nested loops): a nested loop inside a token-carrying loop gives
        // the token a third route consumer; an inner loop assigning an
        // outer-carried `mut` closes a cross-merge cycle. Both are out of Core.
        if self.loop_depth > 0 {
            if self.enclosing_token_loop {
                return Err(diag(
                    LCode::NestedLoopShape,
                    sp,
                    "a nested loop inside a token-carrying loop is out of Core",
                ));
            }
            for (name, _, _) in &carried {
                if self.outer_carried.contains(name) {
                    return Err(diag(
                        LCode::NestedLoopShape,
                        sp,
                        format!("inner loop assigns the enclosing-loop-carried `{name}`"),
                    ));
                }
            }
        }

        // 2. init: the carried values (+ token), packed in carried order when
        //    |U| >= 2; the single object otherwise.
        let token_carried = body_effectful;
        let mut init_objs: Vec<ObjectId> = carried.iter().map(|(_, _, o)| *o).collect();
        let mut u_tys: Vec<Ty> = carried.iter().map(|(_, t, _)| t.clone()).collect();
        if token_carried {
            let tok = self
                .token
                .ok_or_else(|| internal(sp, "effectful loop without token"))?;
            init_objs.push(tok);
            u_tys.push(Ty::IoToken);
        }
        let u_single = init_objs.len() == 1;
        let init = if u_single {
            init_objs[0]
        } else {
            self.pack(fb, &init_objs, Dest::Fresh(None), sp)
                .map_err(|e| ir_err(e, sp))?
        };

        // 3. begin_loop + merge; rebind carried names to projs of the merge.
        let lh = fb.begin_loop(init, ir_loc(sp)).map_err(|e| ir_err(e, sp))?;
        let merge = fb.merge_of(&lh);
        let u_ty = if u_single {
            u_tys[0].clone()
        } else {
            Ty::Tuple(u_tys.clone())
        };
        self.record(merge, u_ty.clone());
        self.loop_depth += 1;
        // Seed the derives-from-merge set with the merge view (§7.3).
        let saved_derived = std::mem::take(&mut self.derived);
        self.derived.insert(merge);
        // Nesting state for L1504 (restored after the loop).
        let saved_token_loop = self.enclosing_token_loop;
        let saved_outer = std::mem::take(&mut self.outer_carried);
        self.enclosing_token_loop = saved_token_loop || token_carried;
        self.outer_carried = saved_outer.clone();
        for (name, _, _) in &carried {
            self.outer_carried.insert(name.clone());
        }

        self.scope.push();
        let n_state = carried.len();
        for (k, (name, ty, _)) in carried.iter().enumerate() {
            let obj = if u_single {
                merge
            } else {
                self.proj(fb, merge, k as u32, Dest::Fresh(Some(name.clone())), sp)?
            };
            self.derived.insert(obj);
            self.record(obj, ty.clone());
            self.rebind(name, obj, ty.clone(), true);
        }
        if token_carried {
            let tok = if u_single {
                merge
            } else {
                self.proj(fb, merge, n_state as u32, Dest::Fresh(None), sp)?
            };
            self.derived.insert(tok);
            self.record(tok, Ty::IoToken);
            self.token = Some(tok);
        }

        // 4. Lower the body items, then find the routing guard (final item/tail).
        let routing =
            self.lower_loop_body(fb, &l.body, &lh, &carried, token_carried, u_single, &u_ty)?;
        if !routing {
            self.scope.pop();
            self.loop_depth -= 1;
            self.derived = saved_derived;
            self.enclosing_token_loop = saved_token_loop;
            self.outer_carried = saved_outer;
            return Err(diag(
                LCode::LoopNoExit,
                sp,
                "loop has no routing guard / exit",
            ));
        }

        fb.end_loop(lh).map_err(|e| ir_err(e, sp))?;
        self.scope.pop();
        self.loop_depth -= 1;
        self.derived = saved_derived;
        self.enclosing_token_loop = saved_token_loop;
        self.outer_carried = saved_outer;

        // Poison the carried names in the enclosing scope (LD9/L1107).
        for (name, _, _) in &carried {
            self.scope.poison(name);
        }
        Ok(())
    }

    /// Lower a loop body: statements then the routing guard (final item or
    /// tail). Returns whether a routing guard was found and emitted.
    #[allow(clippy::too_many_arguments)]
    fn lower_loop_body(
        &mut self,
        fb: &mut FnBuilder,
        body: &Block,
        lh: &flow_ir::LoopHandle,
        carried: &[(String, Ty, ObjectId)],
        token_carried: bool,
        u_single: bool,
        u_ty: &Ty,
    ) -> ER<bool> {
        // The routing guard is the final body item (statement or tail). Identify
        // it; lower all preceding items as statements.
        // Build the list of "items" including the tail as the last item.
        enum Item<'a> {
            Stmt(&'a Stmt),
            Tail(&'a Chain),
        }
        let mut items: Vec<Item> = Vec::new();
        for it in &body.items {
            if let BlockItem::Stmt(s) = it {
                items.push(Item::Stmt(s));
            }
        }
        if let Some(t) = &body.tail {
            items.push(Item::Tail(t));
        }
        let n = items.len();
        for (i, it) in items.into_iter().enumerate() {
            let is_last = i + 1 == n;
            match it {
                Item::Stmt(s) => {
                    if is_last {
                        // Last item: must be the routing guard (a chain whose
                        // last stage is a Guard with a routing arm).
                        if let StmtKind::Chain(c) = &s.kind
                            && let Some(g) = chain_routing_guard(c)
                        {
                            self.emit_routing_guard(
                                fb,
                                c,
                                g,
                                lh,
                                carried,
                                token_carried,
                                u_single,
                                u_ty,
                            )?;
                            return Ok(true);
                        }
                        // Not a routing guard in final position.
                        self.emit_stmt(fb, s)?;
                        return Ok(false);
                    }
                    self.emit_stmt(fb, s)?;
                }
                Item::Tail(c) => {
                    if let Some(g) = chain_routing_guard(c) {
                        self.emit_routing_guard(
                            fb,
                            c,
                            g,
                            lh,
                            carried,
                            token_carried,
                            u_single,
                            u_ty,
                        )?;
                        return Ok(true);
                    }
                    self.emit_chain(fb, c, ChainCtx::Statement)?;
                    return Ok(false);
                }
            }
        }
        Ok(false)
    }

    /// Emit a routing guard: the loop's route point (§8.5 step 4).
    #[allow(clippy::too_many_arguments)]
    fn emit_routing_guard(
        &mut self,
        fb: &mut FnBuilder,
        chain: &Chain,
        arms: &[GuardArm],
        lh: &flow_ir::LoopHandle,
        carried: &[(String, Ty, ObjectId)],
        token_carried: bool,
        u_single: bool,
        u_ty: &Ty,
    ) -> ER<()> {
        let guard_sp = chain.stages.last().map(|s| s.span).unwrap_or(chain.span);
        // Classify the two arms first (a structural check, independent of the
        // scrutinee): exactly one jump pole + one exit pole over {true,false},
        // else L1409. This must precede the scrutinee-type check so an
        // integer-discriminant routing guard reports L1409, not L1406.
        let (jump_arm, jump_pole, exit_arm, exit_pole) = classify_routing(arms, guard_sp)?;
        let _ = exit_pole;

        // The chain up to the guard computes the scrutinee (= cond).
        let cond = self.lower_scrutinee(fb, chain)?;
        let cty = self.ty_of(fb, cond);
        if cty != Ty::Bool {
            return Err(diag(
                LCode::GuardScrutineeType,
                guard_sp,
                "routing guard scrutinee is not bool",
            ));
        }
        // L1503: the cond's value must derive from the merge view (§7.3) — a
        // non-derived cond is a vacuous or unconditionally-divergent loop.
        if !self.derived.contains(&cond) {
            return Err(diag(
                LCode::LoopGuardShape,
                guard_sp,
                "loop guard condition does not derive from the loop state",
            ));
        }

        // Snapshot bindings + token register (§3 / SF-2c).
        let snap_scope = self.scope.snapshot();
        let snap_token = self.token;

        // --- jump arm: arm-local rebinds, then recomputed next-state -----------
        self.scope = snap_scope.clone();
        self.token = snap_token;
        self.scope.push();
        self.emit_jump_arm_updates(fb, &jump_arm.payload)?;
        // next-state = arm-local carried values (+ arm-local token), in order.
        let mut next_objs: Vec<ObjectId> = Vec::new();
        for (name, _, _) in carried {
            let (b, _) = self
                .scope
                .resolve(name)
                .ok_or_else(|| internal(guard_sp, "carried name vanished"))?;
            next_objs.push(b.obj);
        }
        if token_carried {
            next_objs.push(
                self.token
                    .ok_or_else(|| internal(guard_sp, "token vanished in jump arm"))?,
            );
        }
        // L1503: ≥1 component of the jump arm's next-state must derive from the
        // merge (a fully non-derived next-state is seal `LoopBackOutsideScc`).
        if !next_objs.iter().any(|o| self.derived.contains(o)) {
            self.scope.pop();
            return Err(diag(
                LCode::LoopGuardShape,
                guard_sp,
                "loop back-edge state does not derive from the loop state",
            ));
        }
        self.scope.pop();
        let next = if next_objs.len() == 1 {
            next_objs[0]
        } else {
            self.pack(fb, &next_objs, Dest::Fresh(None), guard_sp)
                .map_err(|e| ir_err(e, guard_sp))?
        };
        // Polarity: LoopBack fires on true. A `-false->` jump wraps cond in Not.
        let cond_back = if jump_pole {
            cond
        } else {
            self.unop(fb, Operation::Not, cond, Dest::Fresh(None), guard_sp)?
        };
        let _ = u_ty;
        let _ = u_single;
        fb.loop_back(lh, next, cond_back, ir_loc(guard_sp))
            .map_err(|e| ir_err(e, guard_sp))?;

        // --- exit arm: against the snapshot ------------------------------------
        self.scope = snap_scope;
        self.token = snap_token;
        // cond for exit is the complement of the jump pole.
        let cond_exit = if jump_pole {
            // jump on true → exit on false → builder LoopExit fires on false, so
            // pass the shared cond directly (LoopExit fires on false).
            cond
        } else {
            // jump on false → exit on true → LoopExit must fire on true, so pass Not(cond).
            self.unop(fb, Operation::Not, cond, Dest::Fresh(None), guard_sp)?
        };
        self.emit_exit_arm(
            fb,
            &exit_arm.payload,
            cond_exit,
            cond,
            lh,
            token_carried,
            guard_sp,
        )?;
        Ok(())
    }

    /// Lower the routing-guard chain's scrutinee (head + stages before the
    /// final Guard stage). Returns the cond object.
    fn lower_scrutinee(&mut self, fb: &mut FnBuilder, chain: &Chain) -> ER<ObjectId> {
        let mut cur: Option<ObjectId> = match &chain.head {
            Some(h) => Some(self.emit_expr(fb, h)?),
            None => None,
        };
        let n = chain.stages.len();
        for (i, stage) in chain.stages.iter().enumerate() {
            let is_guard = i + 1 == n && matches!(stage.kind, StageKind::Guard(_));
            if is_guard {
                break;
            }
            let next = chain.stages.get(i + 1);
            cur = self.emit_stage(fb, stage, next, false, cur, &ChainCtx::Statement)?;
        }
        // A headless routing guard (`-> { -true-> … -false-> … }`) has no value
        // to route on — a user error, not a lower bug (ATK-13): report L1301
        // HeadlessChain, keeping L1901 unreachable from parse-clean input.
        cur.ok_or_else(|| {
            diag(
                LCode::HeadlessChain,
                chain.span,
                "routing guard has no scrutinee",
            )
        })
    }

    /// Emit a jump-arm payload's state updates (arm-local rebinds). The trailing
    /// `-> loop;` marker carries no value and is skipped (the back edge is wired
    /// by the routing-guard recipe).
    fn emit_jump_arm_updates(&mut self, fb: &mut FnBuilder, payload: &ArmPayload) -> ER<()> {
        match payload {
            ArmPayload::Chain(c) => self.emit_jump_chain(fb, c),
            ArmPayload::Block(b) => {
                for item in &b.items {
                    if let BlockItem::Stmt(s) = item {
                        if let StmtKind::Chain(c) = &s.kind {
                            self.emit_jump_chain(fb, c)?;
                        } else {
                            self.emit_stmt(fb, s)?;
                        }
                    }
                }
                if let Some(t) = &b.tail {
                    self.emit_jump_chain(fb, t)?;
                }
                Ok(())
            }
        }
    }

    /// A jump-arm chain is the bare `-> loop` marker (after the state updates,
    /// which are separate statements). The chain itself is just `-> loop;`.
    fn emit_jump_chain(&mut self, fb: &mut FnBuilder, chain: &Chain) -> ER<()> {
        let _ = fb;
        // The chain is the `-> loop` marker (headless, single LoopJump stage) —
        // carries no value; nothing to emit (the back edge is wired by the
        // routing-guard recipe).
        if chain.head.is_none()
            && chain.stages.len() == 1
            && matches!(chain.stages[0].kind, StageKind::LoopJump)
        {
            return Ok(());
        }
        // Otherwise it is a normal statement chain (a state update written as the
        // block tail). Emit it.
        self.emit_chain(fb, chain, ChainCtx::Statement)?;
        Ok(())
    }

    /// Emit an exit-arm payload against the snapshot (§8.5). `scrutinee` is the
    /// guard's incoming value (the cond) — a headless exit-arm chain that is not
    /// a bare `-> ret` marker seeds `cur := scrutinee` (§8.3 / ATK-19).
    #[allow(clippy::too_many_arguments)]
    fn emit_exit_arm(
        &mut self,
        fb: &mut FnBuilder,
        payload: &ArmPayload,
        cond_exit: ObjectId,
        scrutinee: ObjectId,
        lh: &flow_ir::LoopHandle,
        token_carried: bool,
        guard_sp: syn::SourceLoc,
    ) -> ER<()> {
        // L1504: an inner-loop exit must terminate in `-> ret` (the only nested
        // shape Core admits). `loop_depth >= 2` means we are emitting a nested
        // loop's routing guard.
        if self.loop_depth >= 2 && !exit_payload_is_ret(payload) {
            return Err(diag(
                LCode::NestedLoopShape,
                guard_sp,
                "an inner-loop exit must terminate in `-> ret`",
            ));
        }
        // Determine the terminal: `-> ret`, a binding, or a bare value.
        let (value, terminal) = self.exit_arm_value(fb, payload, scrutinee, guard_sp)?;
        let tok = if token_carried { self.token } else { None };
        match terminal {
            ExitTerminal::Ret => {
                // A value-less `-> ret` exit with no token can only be a pure
                // **Unit**-output fn here (D1's L1306 already rejected value-less
                // `-> ret` for non-Unit outputs). §8.5/LP-1: the payload is the
                // merge-state view itself (the merge object — any |U|), exiting to
                // `Dest::Fresh(None)` with zero Return writers (legal for Unit).
                if value.is_none() && tok.is_none() {
                    let merge = fb.merge_of(lh);
                    fb.loop_exit(lh, merge, cond_exit, Dest::Fresh(None), ir_loc(guard_sp))
                        .map_err(|e| ir_err(e, guard_sp))?;
                    return Ok(());
                }
                let payload_obj = match (value, tok) {
                    (Some(v), Some(t)) => self
                        .pack(fb, &[t, v], Dest::Fresh(None), guard_sp)
                        .map_err(|e| ir_err(e, guard_sp))?,
                    (None, Some(t)) => t, // value-less ret carrying the token.
                    (Some(v), None) => v,
                    (None, None) => unreachable!("handled above"),
                };
                fb.loop_exit(
                    lh,
                    payload_obj,
                    cond_exit,
                    Dest::Ret { slot: None },
                    ir_loc(guard_sp),
                )
                .map_err(|e| ir_err(e, guard_sp))?;
                // After a ret-terminal exit, the token register is consumed.
                if token_carried {
                    self.token = None;
                }
            }
            ExitTerminal::Bind(name, mutable) => {
                let v = value.ok_or_else(|| {
                    diag(LCode::FanoutNoValue, guard_sp, "exit binding with no value")
                })?;
                let payload_obj = match tok {
                    Some(t) => self
                        .pack(fb, &[t, v], Dest::Fresh(None), guard_sp)
                        .map_err(|e| ir_err(e, guard_sp))?,
                    None => v,
                };
                let exit = fb
                    .loop_exit(
                        lh,
                        payload_obj,
                        cond_exit,
                        Dest::Fresh(Some(name.clone())),
                        ir_loc(guard_sp),
                    )
                    .map_err(|e| ir_err(e, guard_sp))?;
                if token_carried {
                    self.record(exit, Ty::Tuple(vec![Ty::IoToken, self.tyq(v)]));
                    let nt = self.proj(fb, exit, 0, Dest::Fresh(None), guard_sp)?;
                    let nv = self.proj(fb, exit, 1, Dest::Fresh(Some(name.clone())), guard_sp)?;
                    self.token = Some(nt);
                    let vty = self.tyq(v);
                    // The exit value binds in the loop's enclosing scope (§8.5),
                    // so it is visible after the loop.
                    self.bind_new_enclosing(&name, nv, vty, mutable);
                } else {
                    let vty = self.tyq(v);
                    self.record(exit, vty.clone());
                    self.bind_new_enclosing(&name, exit, vty, mutable);
                }
            }
            ExitTerminal::BareValue => {
                let v = value
                    .ok_or_else(|| diag(LCode::FanoutNoValue, guard_sp, "exit with no value"))?;
                let payload_obj = match tok {
                    Some(t) => self
                        .pack(fb, &[t, v], Dest::Fresh(None), guard_sp)
                        .map_err(|e| ir_err(e, guard_sp))?,
                    None => v,
                };
                let exit = fb
                    .loop_exit(
                        lh,
                        payload_obj,
                        cond_exit,
                        Dest::Fresh(None),
                        ir_loc(guard_sp),
                    )
                    .map_err(|e| ir_err(e, guard_sp))?;
                if token_carried {
                    self.record(exit, Ty::Tuple(vec![Ty::IoToken, self.tyq(v)]));
                    let nt = self.proj(fb, exit, 0, Dest::Fresh(None), guard_sp)?;
                    self.token = Some(nt);
                } else {
                    let vty = self.tyq(v);
                    self.record(exit, vty);
                }
            }
        }
        Ok(())
    }

    /// Compute the exit arm's value and classify its terminal. The exit arm
    /// payload's final stage is a `-> ret`, a binding, or a bare value.
    fn exit_arm_value(
        &mut self,
        fb: &mut FnBuilder,
        payload: &ArmPayload,
        scrutinee: ObjectId,
        guard_sp: syn::SourceLoc,
    ) -> ER<(Option<ObjectId>, ExitTerminal)> {
        // Get the final chain of the payload (its terminal). For a Block payload
        // with a tail, the tail is the terminal and all items emit as statements.
        // For a `;`-terminated Block (`tail: None`, e.g. `{ -> ret; }` or
        // `{ 99 -> print; -> ret; }`), peel the LAST chain item off as the
        // terminal and emit the preceding items as statements — statement-vs-tail
        // is identical (LD21 / ATK-11), mirroring `arm_is_routing`.
        let chain: &Chain = match payload {
            ArmPayload::Chain(c) => c,
            ArmPayload::Block(b) => match &b.tail {
                Some(t) => {
                    for item in &b.items {
                        if let BlockItem::Stmt(s) = item {
                            self.emit_stmt(fb, s)?;
                        }
                    }
                    t
                }
                None => {
                    // Find the last chain item (the terminal); emit everything
                    // before it as statements.
                    let last_chain_idx = b.items.iter().rposition(|it| {
                        matches!(it, BlockItem::Stmt(s) if matches!(s.kind, StmtKind::Chain(_)))
                    });
                    let Some(last_idx) = last_chain_idx else {
                        return Err(diag(
                            LCode::RoutingGuardShape,
                            b.span,
                            "exit arm with no terminal",
                        ));
                    };
                    for (i, item) in b.items.iter().enumerate() {
                        if i == last_idx {
                            continue;
                        }
                        if let BlockItem::Stmt(s) = item {
                            self.emit_stmt(fb, s)?;
                        }
                    }
                    match &b.items[last_idx] {
                        BlockItem::Stmt(s) => match &s.kind {
                            StmtKind::Chain(c) => c,
                            _ => unreachable!("rposition matched a Chain"),
                        },
                        _ => unreachable!("rposition matched a Stmt"),
                    }
                }
            },
        };
        // Inspect the last stage.
        let last = chain.stages.last();
        match last.map(|s| &s.kind) {
            Some(StageKind::Ret { proj }) => {
                if proj.is_some() {
                    // ret.k exit — fold into a value-then-ret (Core allows full
                    // exits; a slot exit is non-canonical → treat as L1306).
                    return Err(diag(
                        LCode::IncompleteReturn,
                        guard_sp,
                        "slot `ret.k` in a loop exit",
                    ));
                }
                // value = chain up to ret (or none for a bare `-> ret` marker).
                let val = self.exit_chain_value(fb, chain, scrutinee, true)?;
                Ok((val, ExitTerminal::Ret))
            }
            Some(StageKind::Bind { mut_span, name, .. }) => {
                let val = self.exit_chain_value(fb, chain, scrutinee, false)?;
                Ok((
                    val,
                    ExitTerminal::Bind(
                        name_text(self.source, *name).to_string(),
                        mut_span.is_some(),
                    ),
                ))
            }
            // A final bare-name `Expr(Var)` that resolves to no existing binding
            // (and is not a fn / `print`) is a fresh binding (LD1): the exit value
            // takes this name, bound in the enclosing scope (§8.5 / ATK-19). A
            // headless `-> c` exit arm seeds `cur := scrutinee` for the value.
            Some(StageKind::Expr(e)) if self.exit_bare_name_binding(e).is_some() => {
                let name = self.exit_bare_name_binding(e).unwrap();
                let val = self.exit_chain_value(fb, chain, scrutinee, false)?;
                Ok((val, ExitTerminal::Bind(name, false)))
            }
            _ => {
                // bare value: the whole chain produces the value.
                let val = self.exit_chain_value_all(fb, chain, scrutinee)?;
                Ok((val, ExitTerminal::BareValue))
            }
        }
    }

    /// If `e` is a bare `Var` that resolves to no existing binding and is not a
    /// fn / `print`, return the fresh binding name (LD1). Such a final stage in
    /// an exit arm is a binding terminal, not a wire-discarding stage.
    fn exit_bare_name_binding(&self, e: &Expr) -> Option<String> {
        if let ExprKind::Var(n) = &e.kind {
            let text = name_text(self.source, *n);
            if self.scope.resolve(text).is_none()
                && !self.fn_ids.contains_key(text)
                && !crate::is_print_builtin(text)
                && !crate::is_collection_builtin(text)
            {
                return Some(text.to_string());
            }
        }
        None
    }

    /// Value of an exit chain up to (but excluding) the final Ret/Bind stage.
    /// A headless chain seeds `cur := scrutinee` (§8.3 / ATK-19) unless it is the
    /// bare `-> ret` marker (`is_ret` with no head and a single stage), which
    /// carries no value.
    fn exit_chain_value(
        &mut self,
        fb: &mut FnBuilder,
        chain: &Chain,
        scrutinee: ObjectId,
        is_ret: bool,
    ) -> ER<Option<ObjectId>> {
        let n = chain.stages.len();
        // A headless bare `-> ret` marker carries no value (it never adopts the
        // scrutinee — §8.3). A headless `-> name` / `-> name -> …` exit chain is
        // NOT a bare marker: it seeds `cur := scrutinee`.
        if chain.head.is_none() && n == 1 && is_ret {
            return Ok(None);
        }
        // Lower head (or the seeded scrutinee) + all stages except the final one.
        let mut cur: Option<ObjectId> = match &chain.head {
            Some(h) => Some(self.emit_expr(fb, h)?),
            None => Some(scrutinee),
        };
        for (i, stage) in chain.stages.iter().enumerate() {
            if i + 1 == n {
                break; // skip the final Ret/Bind marker.
            }
            let next = chain.stages.get(i + 1);
            cur = self.emit_stage(fb, stage, next, false, cur, &ChainCtx::Statement)?;
        }
        Ok(cur)
    }

    /// Value of an exit chain in bare-value position (all stages). A headless
    /// chain seeds `cur := scrutinee` (§8.3); a headed chain ignores the seed.
    fn exit_chain_value_all(
        &mut self,
        fb: &mut FnBuilder,
        chain: &Chain,
        scrutinee: ObjectId,
    ) -> ER<Option<ObjectId>> {
        if chain.head.is_none() {
            self.emit_chain(fb, chain, ChainCtx::HeadlessSeed(scrutinee))
        } else {
            self.emit_chain(fb, chain, ChainCtx::Statement)
        }
    }

    // --- builder primitive wrappers (record output ty + error mapping) -----

    fn record(&mut self, obj: ObjectId, ty: Ty) -> ObjectId {
        self.obj_ty.insert(obj, ty);
        obj
    }

    /// Propagate the derives-from-merge tag (§7.3): a result is derived iff any
    /// input is derived (within an active loop).
    fn prop(&mut self, result: ObjectId, inputs: &[ObjectId]) -> ObjectId {
        if self.loop_depth > 0 && inputs.iter().any(|i| self.derived.contains(i)) {
            self.derived.insert(result);
        }
        result
    }

    fn constant(&mut self, fb: &mut FnBuilder, v: Value, sp: syn::SourceLoc) -> ER<ObjectId> {
        let ty = v.ty();
        let o = fb.constant(v, ir_loc(sp)).map_err(|e| ir_err(e, sp))?;
        Ok(self.record(o, ty))
    }

    fn unop(
        &mut self,
        fb: &mut FnBuilder,
        op: Operation,
        x: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let xty = self.tyq(x);
        let out = match op {
            Operation::Not => Ty::Bool,
            _ => xty,
        };
        let o = fb
            .unop(op, x, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, &[x]);
        Ok(self.record(o, out))
    }

    fn binop(
        &mut self,
        fb: &mut FnBuilder,
        op: Operation,
        l: ObjectId,
        r: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let lty = self.tyq(l);
        let out = match op {
            Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
                lty
            }
            _ => Ty::Bool,
        };
        let o = fb
            .binop(op, l, r, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, &[l, r]);
        Ok(self.record(o, out))
    }

    fn phi(
        &mut self,
        fb: &mut FnBuilder,
        t: ObjectId,
        f: ObjectId,
        c: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let ty = self.tyq(t);
        let o = fb
            .phi(t, f, c, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, &[t, f, c]);
        Ok(self.record(o, ty))
    }

    fn proj(
        &mut self,
        fb: &mut FnBuilder,
        src: ObjectId,
        idx: u32,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let sty = self.tyq(src);
        let out = sty.component_ty(idx).cloned().unwrap_or(Ty::Unit);
        let o = fb
            .proj(src, idx, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, &[src]);
        Ok(self.record(o, out))
    }

    fn index(
        &mut self,
        fb: &mut FnBuilder,
        a: ObjectId,
        i: ObjectId,
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let aty = self.tyq(a);
        let out = match &aty {
            Ty::Array { elem, .. } => (**elem).clone(),
            _ => Ty::Unit,
        };
        let o = fb
            .index(a, i, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, &[a, i]);
        Ok(self.record(o, out))
    }

    pub(crate) fn pack(
        &mut self,
        fb: &mut FnBuilder,
        comps: &[ObjectId],
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let tys: Vec<Ty> = comps.iter().map(|&c| self.tyq(c)).collect();
        let out = Ty::Tuple(tys);
        let o = fb.pack(comps, dest, ir_loc(sp))?;
        self.prop(o, comps);
        Ok(self.record(o, out))
    }

    fn pack_array(
        &mut self,
        fb: &mut FnBuilder,
        comps: &[ObjectId],
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> ER<ObjectId> {
        let elem = comps.first().map(|&c| self.tyq(c)).unwrap_or(Ty::Unit);
        let out = Ty::Array {
            elem: Box::new(elem),
            size: comps.len() as u64,
        };
        let o = fb
            .pack_array(comps, dest, ir_loc(sp))
            .map_err(|e| ir_err(e, sp))?;
        self.prop(o, comps);
        Ok(self.record(o, out))
    }

    fn pack_struct(
        &mut self,
        fb: &mut FnBuilder,
        ty: Ty,
        comps: &[ObjectId],
        dest: Dest,
        sp: syn::SourceLoc,
    ) -> Result<ObjectId, IrError> {
        let o = fb.pack_struct(ty.clone(), comps, dest, ir_loc(sp))?;
        Ok(self.record(o, ty))
    }

    // --- helpers ------------------------------------------------------------

    /// Query the recorded ty of an object (default `Unit` if not tracked).
    fn tyq(&self, obj: ObjectId) -> Ty {
        self.obj_ty.get(&obj).cloned().unwrap_or(Ty::Unit)
    }

    /// Bind a new surface symbol with a fresh declaration sequence number.
    fn bind_new(&mut self, name: &str, obj: ObjectId, ty: Ty, mutable: bool) {
        let seq = self.decl_seq;
        self.decl_seq += 1;
        self.scope.bind(
            name,
            Binding {
                obj,
                ty,
                mutable,
                decl_seq: seq,
            },
        );
    }

    /// Bind a new surface symbol into the loop's *enclosing* scope (one frame
    /// below the loop-body frame). A routing-guard exit arm binds its exit value
    /// here so it is visible after the loop (DESIGN §8.5).
    fn bind_new_enclosing(&mut self, name: &str, obj: ObjectId, ty: Ty, mutable: bool) {
        let seq = self.decl_seq;
        self.decl_seq += 1;
        self.scope.bind_enclosing(
            name,
            Binding {
                obj,
                ty,
                mutable,
                decl_seq: seq,
            },
        );
    }

    /// Rebind an existing symbol to a fresh object, preserving its declaration
    /// sequence (the one-definition rule allocates a fresh object; the symbol's
    /// declaration order does not change, LD4).
    fn rebind(&mut self, name: &str, obj: ObjectId, ty: Ty, mutable: bool) {
        let seq = self
            .scope
            .resolve(name)
            .map(|(b, _)| b.decl_seq)
            .unwrap_or(self.decl_seq);
        self.scope.rebind_existing(
            name,
            Binding {
                obj,
                ty,
                mutable,
                decl_seq: seq,
            },
        );
    }

    pub(crate) fn ty_of(&self, _fb: &FnBuilder, obj: ObjectId) -> Ty {
        self.tyq(obj)
    }
}

/// Context for how a block's tail is treated.
enum BodyCtx {
    FnBody {
        ret: Option<Ty>,
        effectful: bool,
    },
    MapFoldBody {
        out_ty: Ty,
    },
    #[allow(dead_code)]
    Plain,
}

/// Context for how a chain's final wire is treated.
#[derive(Clone)]
pub enum ChainCtx {
    Statement,
    FnBodyReturn {
        effectful: bool,
    },
    BodyReturn,
    HeadlessSeed(ObjectId),
    /// A return-position tail whose value the caller consumes directly (the
    /// effectful-B-present path packs it with the token — §8.1). Behaves like
    /// `Statement` (the value is handed back, not written to Ret), but the chain
    /// is in **return position**: a tail-less `seq` here demands a value it has
    /// none of, so it draws L1611 rather than being a legal no-op (ADR-0019 pin c).
    RetValue,
}

/// What a Phi-arm scan found (for the L1404/L1405/L1408 checks).
struct PhiArmScan {
    ret_span: Option<syn::SourceLoc>,
    effect_span: Option<syn::SourceLoc>,
    loopjump_span: Option<syn::SourceLoc>,
    assigns: Vec<(String, syn::SourceLoc)>,
}

fn scan_phi_arm(
    source: &str,
    fn_sigs: &BTreeMap<String, FnSig>,
    payload: &ArmPayload,
) -> PhiArmScan {
    let mut scan = PhiArmScan {
        ret_span: None,
        effect_span: None,
        loopjump_span: None,
        assigns: Vec::new(),
    };
    match payload {
        ArmPayload::Chain(c) => scan_chain(source, fn_sigs, c, &mut scan),
        ArmPayload::Block(b) => scan_block(source, fn_sigs, b, &mut scan),
    }
    scan
}

fn scan_block(
    source: &str,
    fn_sigs: &BTreeMap<String, FnSig>,
    block: &Block,
    scan: &mut PhiArmScan,
) {
    for item in &block.items {
        if let BlockItem::Stmt(s) = item {
            scan_stmt(source, fn_sigs, s, scan);
        }
    }
    if let Some(t) = &block.tail {
        scan_chain(source, fn_sigs, t, scan);
    }
}

fn scan_stmt(source: &str, fn_sigs: &BTreeMap<String, FnSig>, stmt: &Stmt, scan: &mut PhiArmScan) {
    match &stmt.kind {
        StmtKind::Chain(c) => scan_chain(source, fn_sigs, c, scan),
        StmtKind::Bind(_) => {}
        StmtKind::Loop(_) => {}
        StmtKind::Error => {}
    }
}

fn scan_chain(
    source: &str,
    fn_sigs: &BTreeMap<String, FnSig>,
    chain: &Chain,
    scan: &mut PhiArmScan,
) {
    for stage in &chain.stages {
        match &stage.kind {
            StageKind::Ret { .. } if scan.ret_span.is_none() => {
                scan.ret_span = Some(stage.span);
            }
            StageKind::LoopJump if scan.loopjump_span.is_none() => {
                scan.loopjump_span = Some(stage.span);
            }
            StageKind::Expr(e) => {
                if let ExprKind::Var(n) = &e.kind {
                    let text = name_text(source, *n);
                    // A bare-name stage that resolves to `print` or to a
                    // user-defined effectful fn is an effect in a Phi arm (L1404;
                    // tokens cannot pass a Phi). Mirror typing.rs's L1605 check.
                    let effectful = crate::is_print_builtin(text)
                        || fn_sigs.get(text).map(|s| s.effectful).unwrap_or(false);
                    if effectful {
                        if scan.effect_span.is_none() {
                            scan.effect_span = Some(stage.span);
                        }
                    } else {
                        // a bare-name stage may be an enclosing-mut rebind.
                        scan.assigns.push((text.to_string(), stage.span));
                    }
                }
            }
            StageKind::Guard(arms) => {
                for arm in arms {
                    match &arm.payload {
                        ArmPayload::Chain(c) => scan_chain(source, fn_sigs, c, scan),
                        ArmPayload::Block(b) => scan_block(source, fn_sigs, b, scan),
                    }
                }
            }
            // Fanout branches and seq bodies lower in the enclosing scope (pin b /
            // LD8), so an effect / `-> ret` / enclosing-mut rebind inside one is
            // still an effect / ret / rebind in the Phi arm — it would be emitted
            // unconditionally, exactly the hazard L1404/L1405/L1408 exist to catch.
            // Descend into both, matching `effect_chain` (ADR-0019 §8.10).
            StageKind::Fanout { branches, .. } => {
                for b in branches {
                    scan_chain(source, fn_sigs, b, scan);
                }
            }
            StageKind::SeqBlock(body) => scan_block(source, fn_sigs, body, scan),
            _ => {}
        }
    }
}

/// How an exit arm terminates.
enum ExitTerminal {
    Ret,
    Bind(String, bool),
    BareValue,
}

/// Collect the names assigned (rebound in stage position) anywhere in a loop
/// body, in source order. An assignment is a stage that is a bare `Var` name
/// resolving to an existing mut (we collect all bare-name stages; the caller
/// filters to enclosing-scope muts).
fn collect_loop_assigns(source: &str, block: &Block, out: &mut Vec<String>) {
    for item in &block.items {
        match item {
            BlockItem::Stmt(s) => collect_assigns_stmt(source, s, out),
            BlockItem::Arm(a) => collect_assigns_payload(source, &a.payload, out),
        }
    }
    if let Some(t) = &block.tail {
        collect_assigns_chain(source, t, out);
    }
}

fn collect_assigns_stmt(source: &str, stmt: &Stmt, out: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Chain(c) => collect_assigns_chain(source, c, out),
        StmtKind::Bind(_) => {}
        StmtKind::Loop(_) => {
            // Nested-loop interiors cannot assign outer-carried muts (L1504); we
            // do NOT descend (their assignments belong to the inner loop).
        }
        StmtKind::Error => {}
    }
}

fn collect_assigns_payload(source: &str, p: &ArmPayload, out: &mut Vec<String>) {
    match p {
        ArmPayload::Chain(c) => collect_assigns_chain(source, c, out),
        ArmPayload::Block(b) => collect_loop_assigns(source, b, out),
    }
}

fn collect_assigns_chain(source: &str, chain: &Chain, out: &mut Vec<String>) {
    for stage in &chain.stages {
        match &stage.kind {
            StageKind::Expr(e) => {
                if let ExprKind::Var(n) = &e.kind {
                    out.push(name_text(source, *n).to_string());
                }
            }
            StageKind::Guard(arms) => {
                for arm in arms {
                    collect_assigns_payload(source, &arm.payload, out);
                }
            }
            // Fanout branches and seq bodies lower in the enclosing scope (pin b /
            // LD8), so a rebind of an enclosing loop `mut` inside one is a
            // loop-carried assignment — it MUST reach the carried-set discovery,
            // else the state is dropped and the loop miscompiles. Descend into
            // both, matching `effect_chain` (ADR-0019 §8.10).
            StageKind::Fanout { branches, .. } => {
                for b in branches {
                    collect_assigns_chain(source, b, out);
                }
            }
            StageKind::SeqBlock(body) => collect_loop_assigns(source, body, out),
            _ => {}
        }
    }
}

/// Whether a loop body contains a direct `print` (the loop then carries a token).
/// Whether a loop body carries an effect (ATK-02): a direct `print`, **or** a
/// bare-name stage resolving (past locals) to an effectful fn. Uses the same
/// predicate as typing's `body_effect_span` (§6.1) so the loop-body effect
/// detector and the L1605 detector agree — otherwise an effectful call hoists
/// out of the loop cycle (token absent from `U`).
fn loop_body_has_effect(source: &str, block: &Block, fn_sigs: &BTreeMap<String, FnSig>) -> bool {
    let mut found = false;
    effect_block(source, block, fn_sigs, &mut found);
    found
}

fn effect_block(source: &str, block: &Block, fn_sigs: &BTreeMap<String, FnSig>, found: &mut bool) {
    for item in &block.items {
        match item {
            BlockItem::Stmt(s) => effect_stmt(source, s, fn_sigs, found),
            BlockItem::Arm(a) => effect_payload(source, &a.payload, fn_sigs, found),
        }
    }
    if let Some(t) = &block.tail {
        effect_chain(source, t, fn_sigs, found);
    }
}

fn effect_stmt(source: &str, stmt: &Stmt, fn_sigs: &BTreeMap<String, FnSig>, found: &mut bool) {
    match &stmt.kind {
        StmtKind::Chain(c) => effect_chain(source, c, fn_sigs, found),
        StmtKind::Loop(l) => effect_block(source, &l.body, fn_sigs, found),
        _ => {}
    }
}

fn effect_payload(
    source: &str,
    p: &ArmPayload,
    fn_sigs: &BTreeMap<String, FnSig>,
    found: &mut bool,
) {
    match p {
        ArmPayload::Chain(c) => effect_chain(source, c, fn_sigs, found),
        ArmPayload::Block(b) => effect_block(source, b, fn_sigs, found),
    }
}

fn effect_chain(source: &str, chain: &Chain, fn_sigs: &BTreeMap<String, FnSig>, found: &mut bool) {
    for stage in &chain.stages {
        match &stage.kind {
            StageKind::Expr(e) => {
                if let ExprKind::Var(n) = &e.kind {
                    let text = name_text(source, *n);
                    let effectful = fn_sigs.get(text).map(|s| s.effectful).unwrap_or(false);
                    if crate::is_print_builtin(text) || effectful {
                        *found = true;
                    }
                }
            }
            StageKind::Guard(arms) => {
                for arm in arms {
                    effect_payload(source, &arm.payload, fn_sigs, found);
                }
            }
            StageKind::Fanout { branches, .. } => {
                for b in branches {
                    effect_chain(source, b, fn_sigs, found);
                }
            }
            StageKind::SeqBlock(body) => effect_block(source, body, fn_sigs, found),
            _ => {}
        }
    }
}

/// If a chain's final stage is a Guard with a routing arm, return its arms.
fn chain_routing_guard(chain: &Chain) -> Option<&[GuardArm]> {
    if let Some(stage) = chain.stages.last()
        && let StageKind::Guard(arms) = &stage.kind
        && arms
            .iter()
            .any(|a| crate::typing::arm_is_routing(&a.payload))
    {
        return Some(arms);
    }
    None
}

/// Classify a routing guard's arms into (jump_arm, jump_pole, exit_arm,
/// exit_pole). Exactly one jump pole + one exit pole over {true,false} (L1409);
/// `-_->` may stand for one pole.
fn classify_routing(
    arms: &[GuardArm],
    sp: syn::SourceLoc,
) -> ER<(&GuardArm, bool, &GuardArm, bool)> {
    // Routing arm = the one whose final chain ends in LoopJump.
    let mut jump: Option<(&GuardArm, Option<bool>)> = None;
    let mut exit: Option<(&GuardArm, Option<bool>)> = None;
    for a in arms {
        let pole = match a.discr {
            GuardDiscr::True => Some(true),
            GuardDiscr::False => Some(false),
            GuardDiscr::Default => None,
            GuardDiscr::Int(_) => {
                return Err(diag(
                    LCode::RoutingGuardArms,
                    a.discr_span,
                    "integer-discriminant routing arm is out of Core",
                ));
            }
            GuardDiscr::OutOfCore => {
                return Err(diag(LCode::UncleanTree, a.discr_span, "out-of-core arm"));
            }
        };
        if crate::typing::arm_is_routing(&a.payload) {
            if jump.is_some() {
                return Err(diag(
                    LCode::RoutingGuardArms,
                    a.discr_span,
                    "two jump arms in one routing guard",
                ));
            }
            jump = Some((a, pole));
        } else {
            if exit.is_some() {
                return Err(diag(
                    LCode::RoutingGuardArms,
                    a.discr_span,
                    "two exit arms in one routing guard",
                ));
            }
            exit = Some((a, pole));
        }
    }
    let (jump_arm, jpole) =
        jump.ok_or_else(|| diag(LCode::LoopNoExit, sp, "routing guard has no jump arm"))?;
    let (exit_arm, epole) =
        exit.ok_or_else(|| diag(LCode::LoopNoExit, sp, "routing guard has no exit arm"))?;
    if arms.len() != 2 {
        return Err(diag(
            LCode::RoutingGuardArms,
            sp,
            "routing guard must have exactly two arms",
        ));
    }
    // Resolve poles: each must be a distinct member of {true,false}; a Default
    // stands for whichever pole the other arm does not occupy.
    let (jp, ep) = match (jpole, epole) {
        (Some(j), Some(e)) => {
            if j == e {
                return Err(diag(
                    LCode::RoutingGuardArms,
                    sp,
                    "both routing arms share a pole",
                ));
            }
            (j, e)
        }
        (Some(j), None) => (j, !j),
        (None, Some(e)) => (!e, e),
        (None, None) => {
            return Err(diag(
                LCode::RoutingGuardArms,
                sp,
                "routing guard with no concrete poles",
            ));
        }
    };
    Ok((jump_arm, jp, exit_arm, ep))
}

/// Whether an expression produces a fresh value via a primitive that can take a
/// `Dest` (so `<expr> -> ret` can write Return directly; pin 2). A bare
/// `Var`/literal is a pre-existing wire (uses `output()` for `-> ret`); a
/// negative-literal fold is a fresh Constant and still uses `output()` (it has
/// no producing primitive to redirect), so it is NOT value-producing here.
fn expr_is_value_producing(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Binary { .. }
        | ExprKind::Member { .. }
        | ExprKind::Index { .. }
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::Struct { .. } => true,
        ExprKind::Unary { op, operand, .. } => {
            // A folded negative literal is a Constant (pre-existing); a real
            // unop is value-producing.
            !matches!(
                (op, &operand.kind),
                (UnOp::Neg, ExprKind::Int(_)) | (UnOp::Neg, ExprKind::Float)
            )
        }
        _ => false,
    }
}

/// Whether a chain's final stage is a `-> ret` marker.
fn chain_ends_in_ret(chain: &Chain) -> bool {
    matches!(
        chain.stages.last().map(|s| &s.kind),
        Some(StageKind::Ret { .. })
    )
}

/// Whether an exit-arm payload terminates in `-> ret` (for the L1504 nested
/// inner-exit-via-ret check). For a `;`-terminated Block (`tail: None`) the
/// terminal is the LAST chain item (LD21: statement-vs-tail identical; ATK-11).
fn exit_payload_is_ret(payload: &ArmPayload) -> bool {
    let chain: Option<&Chain> = match payload {
        ArmPayload::Chain(c) => Some(c),
        ArmPayload::Block(b) => match b.tail.as_ref() {
            Some(t) => Some(t),
            None => b.items.iter().rev().find_map(|it| match it {
                BlockItem::Stmt(s) => match &s.kind {
                    StmtKind::Chain(c) => Some(c),
                    _ => None,
                },
                _ => None,
            }),
        },
    };
    match chain {
        Some(c) => matches!(
            c.stages.last().map(|s| &s.kind),
            Some(StageKind::Ret { .. })
        ),
        None => false,
    }
}

/// Map a surface `BinOp` to the IR `Operation`.
fn map_binop(op: BinOp) -> Operation {
    match op {
        BinOp::Add => Operation::Add,
        BinOp::Sub => Operation::Sub,
        BinOp::Mul => Operation::Mul,
        BinOp::Div => Operation::Div,
        BinOp::Mod => Operation::Mod,
        BinOp::Eq => Operation::Eq,
        BinOp::Ne => Operation::Neq,
        BinOp::Lt => Operation::Lt,
        BinOp::Gt => Operation::Gt,
        BinOp::Le => Operation::Le,
        BinOp::Ge => Operation::Ge,
        BinOp::And => Operation::And,
        BinOp::Or => Operation::Or,
    }
}
