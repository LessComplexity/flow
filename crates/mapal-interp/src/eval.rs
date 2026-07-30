//! The evaluator: per-function env, the flat topo walk, `eval_morphism` with
//! product assembly, and the per-operation semantics (interp DESIGN §2, §3).
//!
//! The loop driver (§4) lives in [`crate::loops`]; this module owns the flat
//! walk and the per-op happy path, threading `Result<RValue, Abort>` so `?`
//! propagates the first abort (Diverged/Trapped).

use std::rc::Rc;

use slotmap::SecondaryMap;

use mapal_ir::{
    CategoryIr, FuncId, GuardSite, MorphismId, ObjectId, ObjectKind, Operation, Ty, Value,
};

use crate::value::{Abort, RValue, TrapKind};

/// Evaluation state, scoped to one function activation. Objects never cross
/// functions (ir I6), so a fresh `EvalCtx` is created per `eval_fn`.
pub(crate) struct EvalCtx<'a> {
    pub ir: &'a CategoryIr,
    /// The current function being evaluated.
    pub f: FuncId,
    /// Object → its resolved value.
    pub env: SecondaryMap<ObjectId, RValue>,
    /// Per-product staging buffers: `target → [Option<component>; arity]`.
    pub staging: SecondaryMap<ObjectId, Vec<Option<RValue>>>,
    /// Guard sites keyed by their `Phi` (plan-s39): the Phi handler evaluates
    /// the condition, fires the chosen arm's own-list, and selects.
    pub guards: SecondaryMap<MorphismId, GuardSite>,
    /// Every guard-arm-owned morphism (the sites' own-lists partition these).
    /// The flat walk and the loop driver both skip them — an arm's work fires
    /// only from its Phi, and only when the condition picks that arm.
    pub gated: SecondaryMap<MorphismId, ()>,
}

impl<'a> EvalCtx<'a> {
    fn new(ir: &'a CategoryIr, f: FuncId) -> Self {
        let mut guards = SecondaryMap::new();
        let mut gated = SecondaryMap::new();
        for site in ir.guard_plan(f).into_iter().filter(GuardSite::gated) {
            for &m in site.on_true.own.iter().chain(site.on_false.own.iter()) {
                gated.insert(m, ());
            }
            guards.insert(site.phi, site);
        }
        EvalCtx {
            ir,
            f,
            env: SecondaryMap::new(),
            staging: SecondaryMap::new(),
            guards,
            gated,
        }
    }

    /// Decrement the budget once; `0 ⇒ Err(Diverged)` (interp DESIGN §6).
    pub(crate) fn spend(&self, budget: &mut u64) -> Result<(), Abort> {
        if *budget == 0 {
            return Err(Abort::Diverged);
        }
        *budget -= 1;
        Ok(())
    }

    /// The single morphism by id (the IR is sealed; ids resolve).
    pub(crate) fn morph(&self, m: MorphismId) -> &'a mapal_ir::Morphism {
        self.ir
            .morphism(m)
            .expect("sealed graph: morphism resolves")
    }

    /// The ty of an object.
    pub(crate) fn ty_of(&self, o: ObjectId) -> &'a Ty {
        &self.ir.object(o).expect("sealed graph: object resolves").ty
    }
}

/// Evaluate one `FuncDef` against one argument (interp DESIGN §2).
pub(crate) fn eval_fn(
    ir: &CategoryIr,
    f: FuncId,
    arg: RValue,
    budget: &mut u64,
) -> Result<RValue, Abort> {
    let fd = ir.func(f).expect("sealed graph: func resolves");
    let mut ctx = EvalCtx::new(ir, f);

    // Seed: the one Parameter object, and every Constant.
    ctx.env.insert(fd.input, arg);
    for (id, obj) in ir.objects() {
        if ir.try_owner(id) != Some(f) {
            continue;
        }
        if obj.kind == ObjectKind::Constant {
            let v = obj.value.clone().expect("ir I7: Constant carries a value");
            ctx.env.insert(id, RValue::Scalar(v));
        }
    }

    // The in-SCC object set (incidence test for driver ownership; interp §2).
    let in_scc = build_in_scc(ir, f);

    // Driver-owned morphisms: everything in a loop plan's decide/advance
    // cones (the llvm `func.rs` walk rule, BL7-shared). Plan membership is the
    // precise rule — a computed exit payload or an exit-arm fanout leaves the
    // SCC but still belongs to the decide cone; SCC incidence alone would
    // re-evaluate it after the loop (a dead recompute for values, a DOUBLE
    // side effect for an exit-arm Print).
    let mut owned: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    for scc in ir.loop_structure(f) {
        for &mg in &scc.merges {
            if let Some(plan) = ir.loop_plan(f, mg) {
                for &mo in plan.decide_order.iter().chain(plan.advance_order.iter()) {
                    owned.insert(mo, ());
                }
            }
        }
    }

    // The flat topo walk (interp DESIGN §2).
    for m in ir.topo_order(f) {
        let morph = ir.morphism(m).expect("sealed graph: morphism resolves");
        let op = morph.op;
        match op {
            Operation::LoopEnter => {
                if ctx.gated.contains_key(m) {
                    // plan-s40: an arm-owned loop — the handle is gated, and
                    // its Phi invokes the driver iff the condition picks the
                    // arm.
                    continue;
                }
                // Invoke the driver once per merge (the target LoopMerge).
                crate::loops::run_loop(&mut ctx, morph.target, budget)?;
            }
            Operation::LoopBack | Operation::LoopExit => {
                // Driver owns these.
            }
            _ => {
                let incident =
                    in_scc.contains_key(morph.source) || in_scc.contains_key(morph.target);
                if incident || owned.contains_key(m) {
                    // Driver owns every morphism in a loop plan's cones or
                    // incident to an SCC.
                    continue;
                }
                if ctx.gated.contains_key(m) {
                    // plan-s39: guard-arm work fires only from its Phi, and
                    // only when the condition picks that arm.
                    continue;
                }
                eval_morphism(&mut ctx, m, budget)?;
            }
        }
    }

    // Return env[output], or Unit if the output ty is Unit and unwritten (§7).
    match ctx.env.get(fd.output) {
        Some(v) => Ok(v.clone()),
        None => {
            if *ctx.ty_of(fd.output) == Ty::Unit {
                Ok(RValue::Unit)
            } else {
                // I-RET guarantees a writer fired for non-Unit returns.
                unreachable!("sealed graph: non-Unit return is always written");
            }
        }
    }
}

/// The set of objects in any loop SCC of `f` (for the incidence test).
pub(crate) fn build_in_scc(ir: &CategoryIr, f: FuncId) -> SecondaryMap<ObjectId, ()> {
    let mut set: SecondaryMap<ObjectId, ()> = SecondaryMap::new();
    for scc in ir.loop_structure(f) {
        for o in scc.objects {
            set.insert(o, ());
        }
    }
    set
}

/// Evaluate one morphism: read `env[source]`, apply `op`, write `env[target]`.
/// Decrements `budget` (interp DESIGN §2, §3).
pub(crate) fn eval_morphism(
    ctx: &mut EvalCtx,
    m: MorphismId,
    budget: &mut u64,
) -> Result<(), Abort> {
    ctx.spend(budget)?;
    let morph = ctx.morph(m);
    let source = morph.source;
    let target = morph.target;
    let op = morph.op;

    match op {
        Operation::Pair { slot, arity } => {
            stage_pair(ctx, source, target, slot, arity);
            Ok(())
        }
        Operation::Proj { index } => {
            let v = component(read(ctx, source), index).clone();
            write(ctx, target, v);
            Ok(())
        }
        Operation::Add | Operation::Sub | Operation::Mul | Operation::Div | Operation::Mod => {
            let v = arith(ctx, source, op)?;
            write(ctx, target, v);
            Ok(())
        }
        Operation::Neg => {
            let v = neg(read(ctx, source));
            write(ctx, target, v);
            Ok(())
        }
        Operation::Eq
        | Operation::Neq
        | Operation::Lt
        | Operation::Gt
        | Operation::Le
        | Operation::Ge => {
            let v = compare(ctx, source, op);
            write(ctx, target, v);
            Ok(())
        }
        Operation::And | Operation::Or => {
            let v = logic(ctx, source, op);
            write(ctx, target, v);
            Ok(())
        }
        Operation::Not => {
            let v = match read(ctx, source) {
                RValue::Scalar(Value::Bool(b)) => RValue::Scalar(Value::Bool(!b)),
                _ => unreachable!("Not on non-bool"),
            };
            write(ctx, target, v);
            Ok(())
        }
        Operation::Widen => {
            let v = match (read(ctx, source), ctx.ty_of(target)) {
                (RValue::Scalar(Value::I32(x)), Ty::Int { bits: 64, .. }) => Value::I64(*x as i64),
                (RValue::Scalar(Value::I32(x)), Ty::Float { bits: 32 }) => Value::F32(*x as f32),
                (RValue::Scalar(Value::I32(x)), Ty::Float { bits: 64 }) => Value::F64(*x as f64),
                (RValue::Scalar(Value::F32(x)), Ty::Float { bits: 64 }) => Value::F64(*x as f64),
                _ => unreachable!("invalid Widen pair passed validation"),
            };
            write(ctx, target, RValue::Scalar(v));
            Ok(())
        }
        Operation::Phi => {
            if let Some(site) = ctx.guards.get(m).cloned() {
                // plan-s39: the condition picks the arm; only that arm's work
                // fires. The condition is staged at slot 2 (its Pair edge is
                // unconditional and precedes the Phi in topo); the unchosen
                // slot never fills, so the triple never finalizes into env —
                // read components from the staging buffer.
                let cond = match ctx.staging.get(source).and_then(|b| b[2].clone()) {
                    Some(RValue::Scalar(Value::Bool(b))) => b,
                    _ => unreachable!("Phi condition staged before the Phi"),
                };
                let arm = if cond { &site.on_true } else { &site.on_false };
                for &g in &arm.own {
                    let gm = ctx.morph(g);
                    if gm.op == Operation::LoopEnter {
                        // plan-s40: the handle stands for its whole loop unit —
                        // the driver fires the machinery and cones.
                        crate::loops::run_loop(ctx, gm.target, budget)?;
                    } else {
                        eval_morphism(ctx, g, budget)?;
                    }
                }
                let slot = if cond { 0 } else { 1 };
                let chosen = ctx
                    .staging
                    .get(source)
                    .and_then(|b| b[slot].clone())
                    .unwrap_or_else(|| unreachable!("chosen arm staged its slot"));
                write(ctx, target, chosen);
                return Ok(());
            }
            // Hand-built (non-builder) triple shape: strict select over the
            // finalized product — both arms were computed in the flat walk.
            let src = read(ctx, source).clone();
            let cond = match component(&src, 2) {
                RValue::Scalar(Value::Bool(b)) => *b,
                _ => unreachable!("Phi selector is not bool"),
            };
            let chosen = component(&src, if cond { 0 } else { 1 }).clone();
            write(ctx, target, chosen);
            Ok(())
        }
        Operation::Call(g) => {
            let arg = read(ctx, source).clone();
            let v = eval_fn(ctx.ir, g, arg, budget)?;
            write(ctx, target, v);
            Ok(())
        }
        Operation::Map { body, captures } => {
            let k = captures as usize;
            let src = read(ctx, source).clone();
            // ADR-0027: the source is (c₁…cₖ, array); captures broadcast to
            // every body call (read-at-position — the value as of the map site).
            let (caps, arr) = if k == 0 {
                (Vec::new(), src)
            } else {
                let caps: Vec<RValue> = (0..k as u32).map(|i| component(&src, i).clone()).collect();
                (caps, component(&src, k as u32).clone())
            };
            let elems = match arr {
                RValue::Array(es) => es,
                _ => unreachable!("Map on non-array"),
            };
            let mut out = Vec::with_capacity(elems.len());
            for e in elems.iter() {
                let arg = if k == 0 {
                    e.clone()
                } else {
                    let mut v = caps.clone();
                    v.push(e.clone());
                    RValue::Tuple(v)
                };
                out.push(eval_fn(ctx.ir, body, arg, budget)?);
            }
            write(ctx, target, RValue::array(out));
            Ok(())
        }
        Operation::Fold { body, captures } => {
            // (Acc, Array): left fold. ADR-0027: (c₁…cₖ, Acc, Array); the body
            // gets (c₁…cₖ, acc, e) per step.
            let k = captures as usize;
            let src = read(ctx, source).clone();
            let (caps, mut acc, elems) = if k == 0 {
                let acc = component(&src, 0).clone();
                let elems = match component(&src, 1).clone() {
                    RValue::Array(es) => es,
                    _ => unreachable!("Fold's second component is not an array"),
                };
                (Vec::new(), acc, elems)
            } else {
                let caps: Vec<RValue> = (0..k as u32).map(|i| component(&src, i).clone()).collect();
                let acc = component(&src, k as u32).clone();
                let elems = match component(&src, k as u32 + 1).clone() {
                    RValue::Array(es) => es,
                    _ => unreachable!("Fold's array component is not an array"),
                };
                (caps, acc, elems)
            };
            for e in elems.iter() {
                let pair = if k == 0 {
                    RValue::Tuple(vec![acc, e.clone()])
                } else {
                    let mut v = caps.clone();
                    v.push(acc);
                    v.push(e.clone());
                    RValue::Tuple(v)
                };
                acc = eval_fn(ctx.ir, body, pair, budget)?;
            }
            write(ctx, target, acc);
            Ok(())
        }
        Operation::Index => {
            let v = index(ctx, source)?;
            write(ctx, target, v);
            Ok(())
        }
        Operation::Update => {
            let v = update(ctx, source)?;
            write(ctx, target, v);
            Ok(())
        }
        Operation::Zip => {
            // src is the internal 2-tuple (Array A, Array B); pair elementwise
            // (ADR-0018 denotation: [(a[0],b[0]), …, (a[n-1],b[n-1])]). Sizes are
            // equal by ir typing, so `zip` consumes both fully.
            let src = read(ctx, source).clone();
            let a = as_array(component(&src, 0));
            let b = as_array(component(&src, 1));
            let out = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| RValue::Tuple(vec![x.clone(), y.clone()]))
                .collect();
            write(ctx, target, RValue::array(out));
            Ok(())
        }
        Operation::Enumerate => {
            // (ADR-0018 denotation: [(0 as i32, a[0]), …, (n-1 as i32, a[n-1])]).
            // The index is pinned i32; ir guarantees n ≤ i32::MAX so the cast is exact.
            let arr = read(ctx, source).clone();
            let elems = match arr {
                RValue::Array(es) => es,
                _ => unreachable!("Enumerate on non-array"),
            };
            let out = elems
                .iter()
                .enumerate()
                .map(|(i, x)| RValue::Tuple(vec![RValue::Scalar(Value::I32(i as i32)), x.clone()]))
                .collect();
            write(ctx, target, RValue::array(out));
            Ok(())
        }
        Operation::Iota => {
            // (ADR-0029 denotation: [0, 1, …, n-1] as i32.) The count is the
            // constant source (validate ties it to the target's static size).
            let n = match read(ctx, source) {
                RValue::Scalar(Value::I32(v)) => *v as usize,
                _ => unreachable!("Iota on non-i32 count"),
            };
            let out = (0..n)
                .map(|i| RValue::Scalar(Value::I32(i as i32)))
                .collect();
            write(ctx, target, RValue::array(out));
            Ok(())
        }
        Operation::Fill => {
            // (ADR-0029 denotation: [x; n] — the internal (x, count) pair, as Zip.)
            let src = read(ctx, source).clone();
            let v = component(&src, 0).clone();
            let n = match component(&src, 1) {
                RValue::Scalar(Value::I32(c)) => *c as usize,
                RValue::Scalar(Value::I64(c)) => *c as usize,
                RValue::Scalar(Value::U8(c)) => *c as usize,
                _ => unreachable!("Fill on non-integer count"),
            };
            write(ctx, target, RValue::array(vec![v; n]));
            Ok(())
        }
        Operation::Print { newline } => {
            let v = print_op(ctx, source, newline);
            write(ctx, target, v);
            Ok(())
        }
        Operation::TimeMs => {
            // `IoToken → (IoToken, f64)` (plan-time-builtin): milliseconds from
            // a monotonic clock against one process-lifetime epoch. The token
            // threads through unchanged as the pair's first component.
            let tok = read(ctx, source).clone();
            debug_assert!(matches!(tok, RValue::Token(_)), "TimeMs source not a token");
            let ms = time_epoch().elapsed().as_secs_f64() * 1000.0;
            write(
                ctx,
                target,
                RValue::Tuple(vec![tok, RValue::Scalar(Value::F64(ms))]),
            );
            Ok(())
        }
        Operation::Output => {
            let v = read(ctx, source).clone();
            write(ctx, target, v);
            Ok(())
        }
        Operation::LoopEnter | Operation::LoopBack | Operation::LoopExit => {
            unreachable!("loop ops are driver-owned, never eval_morphism'd")
        }
    }
}

// --- helpers --------------------------------------------------------------

/// The process-lifetime monotonic epoch for `TimeMs` (plan-time-builtin): one
/// `Instant` shared by every read in the process, so two reads are
/// non-decreasing and a difference is real elapsed milliseconds. Same clock
/// (`std::time::Instant`) as mapal-rt's `mapal_time_ms`/`mapal_perf_begin`.
fn time_epoch() -> &'static std::time::Instant {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    EPOCH.get_or_init(std::time::Instant::now)
}

/// Read `env[o]`; panics on an undefined object (a sealed, topo-ordered graph
/// never reads before write).
fn read<'c>(ctx: &'c EvalCtx, o: ObjectId) -> &'c RValue {
    ctx.env
        .get(o)
        .unwrap_or_else(|| unreachable!("read before write of object"))
}

/// Write `env[o]`.
fn write(ctx: &mut EvalCtx, o: ObjectId, v: RValue) {
    ctx.env.insert(o, v);
}

/// Component `k` of an aggregate (Tuple/Struct/Array).
fn component(x: &RValue, k: u32) -> &RValue {
    match x {
        RValue::Tuple(es) => &es[k as usize],
        RValue::Struct { fields, .. } => &fields[k as usize].1,
        RValue::Array(es) => &es[k as usize],
        _ => unreachable!("component of a non-aggregate"),
    }
}

/// Stage a `Pair` component into `target`'s buffer; finalize at the last slot.
fn stage_pair(ctx: &mut EvalCtx, source: ObjectId, target: ObjectId, slot: u32, arity: u32) {
    let v = read(ctx, source).clone();
    let buf = ctx
        .staging
        .entry(target)
        .unwrap_or_else(|| unreachable!("target object resolves"))
        .or_insert_with(|| vec![None; arity as usize]);
    if buf.len() != arity as usize {
        buf.resize(arity as usize, None);
    }
    buf[slot as usize] = Some(v);
    if buf.iter().all(|s| s.is_some()) {
        let comps: Vec<RValue> = buf.iter().cloned().map(|s| s.unwrap()).collect();
        let ty = ctx.ty_of(target).clone();
        let finalized = finalize_product(&ty, comps);
        ctx.env.insert(target, finalized);
    }
}

/// Assemble a finalized product value from staged components per the target ty.
fn finalize_product(ty: &Ty, comps: Vec<RValue>) -> RValue {
    match ty {
        Ty::Tuple(_) => RValue::Tuple(comps),
        Ty::Array { .. } => RValue::array(comps),
        Ty::Struct { name, fields } => {
            let named = fields.iter().map(|(n, _)| n.clone()).zip(comps).collect();
            RValue::Struct {
                name: name.clone(),
                fields: named,
            }
        }
        _ => unreachable!("Pair target is not a product ty"),
    }
}

/// `Neg` (IEEE fneg for floats; arithmetic negation for ints).
fn neg(x: &RValue) -> RValue {
    match x {
        RValue::Scalar(v) => RValue::Scalar(match v {
            Value::I32(n) => Value::I32(n.wrapping_neg()),
            Value::I64(n) => Value::I64(n.wrapping_neg()),
            Value::U8(n) => Value::U8(n.wrapping_neg()),
            Value::F32(x) => Value::F32(-x),
            Value::F64(x) => Value::F64(-x),
            Value::Bool(_) | Value::Str(_) => unreachable!("Neg on non-numeric"),
        }),
        _ => unreachable!("Neg on non-scalar"),
    }
}

/// `Add/Sub/Mul/Div/Mod` at the operand width; integer ÷/% by 0 ⇒ trap.
fn arith(ctx: &EvalCtx, source: ObjectId, op: Operation) -> Result<RValue, Abort> {
    let pair = read(ctx, source);
    let a = scalar(component(pair, 0));
    let b = scalar(component(pair, 1));
    macro_rules! int_arith {
        ($ctor:path, $x:expr, $y:expr) => {{
            let (x, y) = ($x, $y);
            let r = match op {
                Operation::Add => x.wrapping_add(y),
                Operation::Sub => x.wrapping_sub(y),
                Operation::Mul => x.wrapping_mul(y),
                Operation::Div => {
                    if y == 0 {
                        return Err(Abort::Trapped(TrapKind::DivZero));
                    }
                    x.wrapping_div(y)
                }
                Operation::Mod => {
                    if y == 0 {
                        return Err(Abort::Trapped(TrapKind::DivZero));
                    }
                    x.wrapping_rem(y)
                }
                _ => unreachable!(),
            };
            $ctor(r)
        }};
    }
    macro_rules! float_arith {
        ($ctor:path, $x:expr, $y:expr) => {{
            let (x, y) = ($x, $y);
            let r = match op {
                Operation::Add => x + y,
                Operation::Sub => x - y,
                Operation::Mul => x * y,
                Operation::Div => x / y,
                Operation::Mod => x % y,
                _ => unreachable!(),
            };
            $ctor(r)
        }};
    }
    let v = match (a, b) {
        (Value::I32(x), Value::I32(y)) => int_arith!(Value::I32, *x, *y),
        (Value::I64(x), Value::I64(y)) => int_arith!(Value::I64, *x, *y),
        (Value::U8(x), Value::U8(y)) => int_arith!(Value::U8, *x, *y),
        (Value::F32(x), Value::F32(y)) => float_arith!(Value::F32, *x, *y),
        (Value::F64(x), Value::F64(y)) => float_arith!(Value::F64, *x, *y),
        _ => unreachable!("arith on mismatched/non-numeric operands"),
    };
    Ok(RValue::Scalar(v))
}

/// `Eq/Neq/Lt/Gt/Le/Ge`.
fn compare(ctx: &EvalCtx, source: ObjectId, op: Operation) -> RValue {
    let pair = read(ctx, source);
    let a = scalar(component(pair, 0));
    let b = scalar(component(pair, 1));
    let r = match op {
        Operation::Eq => a == b,
        Operation::Neq => a != b,
        Operation::Lt => num_lt(a, b),
        Operation::Gt => num_lt(b, a),
        // Le/Ge use a native `<=` (NOT `!num_lt`): for floats, IEEE ordering
        // makes every comparison with NaN false, so `Le(NaN, x)` must be false —
        // whereas `!(x < NaN)` would wrongly be true (interp DESIGN §3; per-op review).
        Operation::Le => num_le(a, b),
        Operation::Ge => num_le(b, a),
        _ => unreachable!(),
    };
    RValue::Scalar(Value::Bool(r))
}

/// Strict numeric `<` at the operand width (IEEE ordering for floats — NaN ⇒ false).
fn num_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => x < y,
        (Value::I64(x), Value::I64(y)) => x < y,
        (Value::U8(x), Value::U8(y)) => x < y,
        (Value::F32(x), Value::F32(y)) => x < y,
        (Value::F64(x), Value::F64(y)) => x < y,
        _ => unreachable!("ordered compare on non-numeric/mismatched operands"),
    }
}

/// Numeric `<=` at the operand width (IEEE ordering for floats — NaN ⇒ false).
fn num_le(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => x <= y,
        (Value::I64(x), Value::I64(y)) => x <= y,
        (Value::U8(x), Value::U8(y)) => x <= y,
        (Value::F32(x), Value::F32(y)) => x <= y,
        (Value::F64(x), Value::F64(y)) => x <= y,
        _ => unreachable!("ordered compare on non-numeric/mismatched operands"),
    }
}

/// `And/Or` (strict; both operands already evaluated).
fn logic(ctx: &EvalCtx, source: ObjectId, op: Operation) -> RValue {
    let pair = read(ctx, source);
    let a = matches!(component(pair, 0), RValue::Scalar(Value::Bool(true)));
    let b = matches!(component(pair, 1), RValue::Scalar(Value::Bool(true)));
    let r = match op {
        Operation::And => a && b,
        Operation::Or => a || b,
        _ => unreachable!(),
    };
    RValue::Scalar(Value::Bool(r))
}

/// `Index`: `(Array, I)`; `i < 0 ∨ i ≥ n ⇒ Trapped(IndexOob)`.
fn index(ctx: &EvalCtx, source: ObjectId) -> Result<RValue, Abort> {
    let pair = read(ctx, source);
    let arr = match component(pair, 0) {
        RValue::Array(es) => es,
        _ => unreachable!("Index on a non-array"),
    };
    let i = as_int(scalar(component(pair, 1)));
    if i < 0 || i as u128 >= arr.len() as u128 {
        return Err(Abort::Trapped(TrapKind::IndexOob));
    }
    Ok(arr[i as usize].clone())
}

/// `Update`: `(Array, I, T)`; `i < 0 ∨ i ≥ n ⇒ Trapped(IndexOob)`; else a fresh
/// array with slot `i` replaced by the value operand (ADR-0021). The index
/// zero/sign-extends exactly like `Index` (shared `as_int` path).
fn update(ctx: &EvalCtx, source: ObjectId) -> Result<RValue, Abort> {
    let triple = read(ctx, source);
    let mut arr = match component(triple, 0) {
        RValue::Array(es) => Rc::clone(es),
        _ => unreachable!("Update on a non-array"),
    };
    let i = as_int(scalar(component(triple, 1)));
    if i < 0 || i as u128 >= arr.len() as u128 {
        return Err(Abort::Trapped(TrapKind::IndexOob));
    }
    // `make_mut` is the copy in "a fresh array with slot `i` replaced"
    // (ADR-0021): the source is still live in `env`, so this copies once here
    // rather than on every clone of a value that merely *contains* the array.
    Rc::make_mut(&mut arr)[i as usize] = component(triple, 2).clone();
    Ok(RValue::Array(arr))
}

/// `Print { newline }`: `(IoToken, P) → IoToken`.
fn print_op(ctx: &EvalCtx, source: ObjectId, newline: bool) -> RValue {
    let pair = read(ctx, source);
    let log = match component(pair, 0) {
        RValue::Token(s) => s.clone(),
        _ => unreachable!("Print's first component is not a token"),
    };
    let p = scalar(component(pair, 1));
    let mut out = log;
    out.push_str(&crate::value::render(p));
    if newline {
        out.push('\n');
    }
    RValue::Token(out)
}

/// Extract the array elements, panicking on a non-array.
fn as_array(x: &RValue) -> &[RValue] {
    match x {
        RValue::Array(es) => es,
        _ => unreachable!("expected an array value"),
    }
}

/// Extract the scalar payload, panicking on a non-scalar.
fn scalar(x: &RValue) -> &Value {
    match x {
        RValue::Scalar(v) => v,
        _ => unreachable!("expected a scalar value"),
    }
}

/// An integer value as an `i128` (sign-extended).
fn as_int(v: &Value) -> i128 {
    match v {
        Value::I32(n) => *n as i128,
        Value::I64(n) => *n as i128,
        Value::U8(n) => *n as i128,
        _ => unreachable!("Index's index operand is not an integer"),
    }
}
