//! The evaluator: per-function env, the flat topo walk, `eval_morphism` with
//! product assembly, and the per-operation semantics (interp DESIGN §2, §3).
//!
//! The loop driver (§4) lives in [`crate::loops`]; this module owns the flat
//! walk and the per-op happy path, threading `Result<RValue, Abort>` so `?`
//! propagates the first abort (Diverged/Trapped).

use slotmap::SecondaryMap;

use flow_ir::{CategoryIr, FuncId, MorphismId, ObjectId, ObjectKind, Operation, Ty, Value};

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
}

impl<'a> EvalCtx<'a> {
    fn new(ir: &'a CategoryIr, f: FuncId) -> Self {
        EvalCtx {
            ir,
            f,
            env: SecondaryMap::new(),
            staging: SecondaryMap::new(),
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
    pub(crate) fn morph(&self, m: MorphismId) -> &'a flow_ir::Morphism {
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

    // The flat topo walk (interp DESIGN §2).
    for m in ir.topo_order(f) {
        let morph = ir.morphism(m).expect("sealed graph: morphism resolves");
        let op = morph.op;
        match op {
            Operation::LoopEnter => {
                // Invoke the driver once per merge (the target LoopMerge).
                crate::loops::run_loop(&mut ctx, morph.target, budget)?;
            }
            Operation::LoopBack | Operation::LoopExit => {
                // Driver owns these.
            }
            _ => {
                let incident =
                    in_scc.contains_key(morph.source) || in_scc.contains_key(morph.target);
                if incident {
                    // Driver owns every non-LoopEnter morphism incident to an SCC.
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
        Operation::Phi => {
            // (T, T, Bool): src@2 ? src@0 : src@1.
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
        Operation::Map { body } => {
            let arr = read(ctx, source).clone();
            let elems = match arr {
                RValue::Array(es) => es,
                _ => unreachable!("Map on non-array"),
            };
            let mut out = Vec::with_capacity(elems.len());
            for e in elems {
                out.push(eval_fn(ctx.ir, body, e, budget)?);
            }
            write(ctx, target, RValue::Array(out));
            Ok(())
        }
        Operation::Fold { body } => {
            // (Acc, Array): left fold.
            let src = read(ctx, source).clone();
            let mut acc = component(&src, 0).clone();
            let elems = match component(&src, 1).clone() {
                RValue::Array(es) => es,
                _ => unreachable!("Fold's second component is not an array"),
            };
            for e in elems {
                let pair = RValue::Tuple(vec![acc, e]);
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
        Operation::Print { newline } => {
            let v = print_op(ctx, source, newline);
            write(ctx, target, v);
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
        Ty::Array { .. } => RValue::Array(comps),
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
