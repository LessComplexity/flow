//! Layer 3 (DESIGN §2) — constant folding + algebraic identities. Pure analysis:
//! `&CategoryIr → RewritePlan`. Fills `constify` (folded scalars, `x*0→0`) and
//! `alias` (algebraic forwards, `proj∘pack`, index-of-const, index∘update L-a,
//! phi-select).
//!
//! Justifying law: per-op axioms **at the oracle's exact semantics** (interp
//! `eval.rs`): integers wrap, `Div`/`Mod` fold only on a non-zero constant
//! divisor (a runtime trap is the program's meaning, R4), floats IEEE at width.
//! Every entry is bounded by §1.2 P1/P2: keys are non-SCC `Temporary` objects,
//! and an `alias` value shares its key's (absent) SCC membership — enforced by
//! requiring the forwarded object to be non-SCC too.

use slotmap::SecondaryMap;

use mapal_ir::{
    CategoryIr, GuardSite, MorphismId, ObjectId, ObjectKind, Operation, Value, ty_contains_token,
};

use crate::plan::RewritePlan;

/// Analyze `ir` for layer-3 folds + identities (DESIGN §2). One pass; the driver
/// fixpoint chains multi-level folds across rounds (an op whose operand was
/// folded this round is a `Constant` next round).
pub fn analyze_const_fold(ir: &CategoryIr) -> RewritePlan {
    let mut plan = RewritePlan::new();
    let scc = scc_membership(ir);

    // plan-s39: resolving a guard's condition decides the arm, so the LOSING
    // arm's exclusive work is dropped in the same plan. It must happen here,
    // not be left to DCE: once the `Phi` is aliased away the guard site no
    // longer exists, and DCE's R4 rule would re-pin the orphaned trapping work
    // as a root — the resolved guard would still trap.
    let mut guard_arms: SecondaryMap<MorphismId, (Vec<ObjectId>, Vec<ObjectId>)> =
        SecondaryMap::new();
    // plan-s40: a Phi whose arm owns a loop (a LoopEnter handle in its
    // own-list) is not folded at all — dropping a whole loop unit is replay
    // surgery this pass has no business doing, and the loop's internals are
    // not in the own-list to drop anyway.
    // ponytail: refuse-to-fold; the fixpoint driver refolds after LiftLoops
    // turns the arm's loop into a droppable Map/Fold.
    let mut loop_armed: SecondaryMap<MorphismId, ()> = SecondaryMap::new();
    for (f, _) in ir.funcs() {
        let sites: Vec<_> = ir
            .guard_plan(f)
            .into_iter()
            .filter(GuardSite::gated)
            .collect();
        for site in &sites {
            // Transitive like `all_work` below: an arm gating a loop-armed
            // nested site inherits the mark — its transitive drop would reach
            // the nested handle. Sites arrive in topo order, nested first.
            let has_loop = site
                .on_true
                .own
                .iter()
                .chain(site.on_false.own.iter())
                .any(|&m| {
                    matches!(ir.morphism(m).expect("own edge").op, Operation::LoopEnter)
                        || loop_armed.contains_key(m)
                });
            if has_loop {
                loop_armed.insert(site.phi, ());
            }
        }
        // TRANSITIVE work per arm. `guard_plan` gives each site only its DIRECT
        // work — a nested site's morphisms belong to that site, not to the arm
        // gating it. Dropping the direct list alone leaves the nested chain
        // alive, and `calc`'s right-folded 5-arm match then still trapped on
        // `a % b` after ConstFold resolved the outer arm. Sites arrive in topo
        // order of their `Phi`, so nested ones are complete first.
        let mut all_work: SecondaryMap<MorphismId, Vec<MorphismId>> = SecondaryMap::new();
        for site in &sites {
            let mut work: Vec<MorphismId> = Vec::new();
            for &m in site.on_true.own.iter().chain(site.on_false.own.iter()) {
                work.push(m);
                if let Some(nested) = all_work.get(m) {
                    work.extend_from_slice(nested);
                }
            }
            all_work.insert(site.phi, work);
        }
        for site in &sites {
            // Per arm: its own work plus every nested site's, transitively.
            let arm_work = |own: &[MorphismId]| -> Vec<ObjectId> {
                let mut out = Vec::new();
                for &m in own {
                    let mut chain = vec![m];
                    if let Some(nested) = all_work.get(m) {
                        chain.extend_from_slice(nested);
                    }
                    for c in chain {
                        out.push(ir.morphism(c).expect("own edge").target);
                    }
                }
                out
            };
            guard_arms.insert(
                site.phi,
                (arm_work(&site.on_true.own), arm_work(&site.on_false.own)),
            );
        }
    }

    for (o, obj) in ir.objects() {
        // P1: fold only `Temporary` targets. A primitive targeting Return
        // directly (`fn f() -> i32 { 2 + 3 }`) is left alone — folding it would
        // drop the sole Return writer (I-RET).
        if obj.kind != ObjectKind::Temporary {
            continue;
        }
        // P2: keys sit outside every loop SCC (loop-carried arithmetic is left
        // alone; loop-invariants sit outside the SCC and still fold).
        if scc.contains_key(o) {
            continue;
        }
        let m = match op_edge(ir, o) {
            Some(m) => m,
            None => continue, // product / param / const / loop object.
        };
        let morph = ir.morphism(m).expect("op edge");
        if morph.op == Operation::Phi && loop_armed.contains_key(m) {
            continue;
        }
        if morph.op == Operation::Phi
            && let Some((t_own, e_own)) = guard_arms.get(m)
        {
            let f = slot_feeders(ir, morph.source);
            if let Some(&Value::Bool(b)) = const_val(ir, f[2]) {
                // plan-s40 review find [4]: the drop is sound ONLY when the
                // phi-select alias in `try_fold_object` actually fires —
                // `forward` refuses an SCC or token winner, and dropping the
                // arm + triple while the Phi survives makes replay rebuild
                // the losing boundary `Pair` edge against a dropped feeder
                // ("feeder is not mapped"). Mirror forward's refusal exactly.
                let winner = if b { f[0] } else { f[1] };
                let alias_fires = !scc.contains_key(winner)
                    && winner != o
                    && !ty_contains_token(&ir.object(winner).expect("winner").ty);
                if alias_fires {
                    for &dead in if b { e_own } else { t_own } {
                        plan.drop.insert(dead, ());
                    }
                    // The triple too. `try_fold_object` aliases this Phi to
                    // the winning arm, so nothing reads the triple any more —
                    // and leaving it alive would replay the LOSING arm's
                    // boundary `Pair` edge, whose feeder is now dropped
                    // (replay.rs:989 "feeder is not mapped").
                    plan.drop.insert(morph.source, ());
                }
            }
        }
        try_fold_object(ir, &scc, &mut plan, o, morph.op, morph.source);
    }

    plan
}

/// Dispatch one op-defined `Temporary` `o` (op `op`, source `src`) to its §2
/// rule, recording a `constify`/`alias` entry when one fires.
fn try_fold_object(
    ir: &CategoryIr,
    scc: &SecondaryMap<ObjectId, ()>,
    plan: &mut RewritePlan,
    o: ObjectId,
    op: Operation,
    src: ObjectId,
) {
    use Operation::*;
    match op {
        Add | Sub | Mul | Div | Mod | Eq | Neq | Lt | Gt | Le | Ge | And | Or => {
            let f = slot_feeders(ir, src);
            let (l, r) = (f[0], f[1]);
            // Full const fold at oracle semantics.
            if let (Some(a), Some(b)) = (const_val(ir, l), const_val(ir, r))
                && let Some(v) = fold_binop(op, a, b)
            {
                plan.constify.insert(o, v);
                return;
            }
            // Algebraic identity (integer / bool only; float is IEEE-unsound).
            try_algebraic(ir, scc, plan, o, op, l, r);
        }
        Neg | Not => {
            if let Some(a) = const_val(ir, src)
                && let Some(v) = fold_unop(op, a)
            {
                plan.constify.insert(o, v);
                return;
            }
            // Not(Not(x)) → x.
            if op == Not
                && let Some(inner) = op_edge(ir, src)
                && ir.morphism(inner).expect("inner").op == Not
            {
                let x = ir.morphism(inner).expect("inner").source;
                forward(ir, scc, plan, o, x);
            }
        }
        Widen => {
            if let Some(a) = const_val(ir, src)
                && let Some(v) = fold_widen(a, &ir.object(o).expect("Widen target").ty)
            {
                plan.constify.insert(o, v);
            }
        }
        Proj { index } => {
            // proj∘pack: source is a pack-built product ⇒ forward its slot feeder.
            if is_pack(ir, src)
                && let Some(&x) = slot_feeders(ir, src).get(index as usize)
            {
                forward(ir, scc, plan, o, x);
            }
        }
        Index => {
            let f = slot_feeders(ir, src);
            let (arr, idx) = (f[0], f[1]);
            if let Some(Value::I32(_) | Value::I64(_) | Value::U8(_)) = const_val(ir, idx) {
                let i = int_val(const_val(ir, idx).unwrap());
                if is_pack(ir, arr) {
                    let elems = slot_feeders(ir, arr);
                    if i >= 0 && (i as u128) < elems.len() as u128 {
                        forward(ir, scc, plan, o, elems[i as usize]);
                    }
                } else if let Some(m) = op_edge(ir, arr)
                    && ir.morphism(m).expect("update edge").op == Update
                {
                    // L-a (ADR-0021 §3): `index_i ∘ update_i` with both indices
                    // CONSTANT, equal, and in-bounds ⇒ the Index result IS the
                    // written value. Aliases to the Update's value operand (an
                    // already-existing object — the alias channel). OOB or unequal
                    // index does not fold (a real OOB read is a trap = the program's
                    // meaning, R4; the base-read law L-b and update∘update L-c need
                    // the operand-rewrite channel that does not exist — headroom).
                    let triple = ir.morphism(m).expect("update edge").source;
                    let uf = slot_feeders(ir, triple);
                    let (uidx, uval) = (uf[1], uf[2]);
                    let n = ir.object(arr).and_then(|x| x.ty.product_arity());
                    if let (Some(u @ (Value::I32(_) | Value::I64(_) | Value::U8(_))), Some(n)) =
                        (const_val(ir, uidx), n)
                        && int_val(u) == i
                        && i >= 0
                        && (i as u128) < n as u128
                    {
                        forward(ir, scc, plan, o, uval);
                    }
                }
            }
        }
        Phi => {
            let f = slot_feeders(ir, src);
            let (t, e, cond) = (f[0], f[1], f[2]);
            if let Some(&Value::Bool(b)) = const_val(ir, cond) {
                forward(ir, scc, plan, o, if b { t } else { e });
            }
        }
        _ => {}
    }
}

/// Integer / bool algebraic identities (DESIGN §2 table; RW4 — float excluded).
fn try_algebraic(
    ir: &CategoryIr,
    scc: &SecondaryMap<ObjectId, ()>,
    plan: &mut RewritePlan,
    o: ObjectId,
    op: Operation,
    l: ObjectId,
    r: ObjectId,
) {
    use Operation::*;
    let cl = const_val(ir, l);
    let cr = const_val(ir, r);
    let l_int = is_int_obj(ir, l);
    let zero = |v: &Value| int_val(v) == 0;
    let one = |v: &Value| int_val(v) == 1;
    let tru = |v: &Value| matches!(v, Value::Bool(true));
    let fal = |v: &Value| matches!(v, Value::Bool(false));

    match op {
        Add if l_int => {
            if cr.map(zero).unwrap_or(false) {
                forward(ir, scc, plan, o, l);
            } else if cl.map(zero).unwrap_or(false) {
                forward(ir, scc, plan, o, r);
            }
        }
        Sub if l_int && cr.map(zero).unwrap_or(false) => forward(ir, scc, plan, o, l),
        Mul if l_int => {
            if cr.map(one).unwrap_or(false) {
                forward(ir, scc, plan, o, l);
            } else if cl.map(one).unwrap_or(false) {
                forward(ir, scc, plan, o, r);
            } else if cr.map(zero).unwrap_or(false) || cl.map(zero).unwrap_or(false) {
                constify_zero(ir, plan, o);
            }
        }
        Div if l_int && cr.map(one).unwrap_or(false) => forward(ir, scc, plan, o, l),
        Mod if l_int && cr.map(one).unwrap_or(false) => constify_zero(ir, plan, o),
        And => {
            if cr.map(tru).unwrap_or(false) {
                forward(ir, scc, plan, o, l);
            } else if cl.map(tru).unwrap_or(false) {
                forward(ir, scc, plan, o, r);
            } else if cr.map(fal).unwrap_or(false) || cl.map(fal).unwrap_or(false) {
                plan.constify.insert(o, Value::Bool(false));
            }
        }
        Or => {
            if cr.map(fal).unwrap_or(false) {
                forward(ir, scc, plan, o, l);
            } else if cl.map(fal).unwrap_or(false) {
                forward(ir, scc, plan, o, r);
            } else if cr.map(tru).unwrap_or(false) || cl.map(tru).unwrap_or(false) {
                plan.constify.insert(o, Value::Bool(true));
            }
        }
        _ => {}
    }
}

/// Record `alias[o] = x` when P2 holds (`x` outside every loop SCC, matching the
/// non-SCC key). Skips (leaving the op) when `x` is loop-carried, or when `x` is
/// token-bearing: aliasing a token-slot `proj∘pack` / index-of-const result would
/// give `x` a second token consumer at re-seal (I4 `TokenNotLinear`). Mirrors
/// CSE's exclusion (`graph_rewrites.rs`); a no-op for the numeric/bool forwards.
fn forward(
    ir: &CategoryIr,
    scc: &SecondaryMap<ObjectId, ()>,
    plan: &mut RewritePlan,
    o: ObjectId,
    x: ObjectId,
) {
    if scc.contains_key(x) || x == o || ty_contains_token(&ir.object(x).expect("forward target").ty)
    {
        return;
    }
    plan.alias.insert(o, x);
}

/// `constify[o] = 0` at `o`'s integer type (`x*0`, `x%1`).
fn constify_zero(ir: &CategoryIr, plan: &mut RewritePlan, o: ObjectId) {
    let v = match &ir.object(o).expect("obj").ty {
        t if *t == mapal_ir::Ty::i32() => Value::I32(0),
        t if *t == mapal_ir::Ty::i64() => Value::I64(0),
        t if *t == mapal_ir::Ty::u8() => Value::U8(0),
        _ => return,
    };
    plan.constify.insert(o, v);
}

// --- oracle-exact fold semantics (mirror interp eval.rs) ------------------

/// Fold a binary op on two constants (DESIGN §2 / RW5). `None` = do not fold: an
/// integer zero divisor (the trap is the program's meaning, R4).
fn fold_binop(op: Operation, a: &Value, b: &Value) -> Option<Value> {
    use Operation::*;
    match op {
        Add | Sub | Mul | Div | Mod => fold_arith(op, a, b),
        Eq => Some(Value::Bool(a == b)),
        Neq => Some(Value::Bool(a != b)),
        Lt => Some(Value::Bool(num_lt(a, b))),
        Gt => Some(Value::Bool(num_lt(b, a))),
        Le => Some(Value::Bool(num_le(a, b))),
        Ge => Some(Value::Bool(num_le(b, a))),
        And => Some(Value::Bool(as_bool(a) && as_bool(b))),
        Or => Some(Value::Bool(as_bool(a) || as_bool(b))),
        _ => None,
    }
}

/// `Add/Sub/Mul/Div/Mod` at the operand width (interp `eval::arith`). Integer
/// `Div`/`Mod` by zero returns `None` (unfolded — R4).
fn fold_arith(op: Operation, a: &Value, b: &Value) -> Option<Value> {
    use Operation::*;
    macro_rules! int_op {
        ($ctor:path, $x:expr, $y:expr) => {{
            let (x, y) = ($x, $y);
            let r = match op {
                Add => x.wrapping_add(y),
                Sub => x.wrapping_sub(y),
                Mul => x.wrapping_mul(y),
                Div => {
                    if y == 0 {
                        return None;
                    }
                    x.wrapping_div(y)
                }
                Mod => {
                    if y == 0 {
                        return None;
                    }
                    x.wrapping_rem(y)
                }
                _ => return None,
            };
            $ctor(r)
        }};
    }
    macro_rules! float_op {
        ($ctor:path, $x:expr, $y:expr) => {{
            let (x, y) = ($x, $y);
            let r = match op {
                Add => x + y,
                Sub => x - y,
                Mul => x * y,
                Div => x / y,
                Mod => x % y,
                _ => return None,
            };
            $ctor(r)
        }};
    }
    let v = match (a, b) {
        (Value::I32(x), Value::I32(y)) => int_op!(Value::I32, *x, *y),
        (Value::I64(x), Value::I64(y)) => int_op!(Value::I64, *x, *y),
        (Value::U8(x), Value::U8(y)) => int_op!(Value::U8, *x, *y),
        (Value::F32(x), Value::F32(y)) => float_op!(Value::F32, *x, *y),
        (Value::F64(x), Value::F64(y)) => float_op!(Value::F64, *x, *y),
        _ => return None,
    };
    Some(v)
}

/// `Neg`/`Not` on a constant (interp `eval::neg` + the `Not` arm).
fn fold_unop(op: Operation, a: &Value) -> Option<Value> {
    match (op, a) {
        (Operation::Neg, Value::I32(n)) => Some(Value::I32(n.wrapping_neg())),
        (Operation::Neg, Value::I64(n)) => Some(Value::I64(n.wrapping_neg())),
        (Operation::Neg, Value::U8(n)) => Some(Value::U8(n.wrapping_neg())),
        (Operation::Neg, Value::F32(x)) => Some(Value::F32(-x)),
        (Operation::Neg, Value::F64(x)) => Some(Value::F64(-x)),
        (Operation::Not, Value::Bool(b)) => Some(Value::Bool(!b)),
        _ => None,
    }
}

/// `Widen(Constant)` at the same Rust conversion semantics as the interpreter.
fn fold_widen(a: &Value, target: &mapal_ir::Ty) -> Option<Value> {
    match (a, target) {
        (Value::I32(x), t) if *t == mapal_ir::Ty::i64() => Some(Value::I64(*x as i64)),
        (Value::I32(x), t) if *t == mapal_ir::Ty::f32() => Some(Value::F32(*x as f32)),
        (Value::I32(x), t) if *t == mapal_ir::Ty::f64() => Some(Value::F64(*x as f64)),
        (Value::F32(x), t) if *t == mapal_ir::Ty::f64() => Some(Value::F64(*x as f64)),
        _ => None,
    }
}

/// Strict numeric `<` at the operand width (IEEE for floats — NaN ⇒ false).
fn num_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => x < y,
        (Value::I64(x), Value::I64(y)) => x < y,
        (Value::U8(x), Value::U8(y)) => x < y,
        (Value::F32(x), Value::F32(y)) => x < y,
        (Value::F64(x), Value::F64(y)) => x < y,
        _ => false,
    }
}

/// Numeric `<=` at the operand width (IEEE for floats — NaN ⇒ false).
fn num_le(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::I32(x), Value::I32(y)) => x <= y,
        (Value::I64(x), Value::I64(y)) => x <= y,
        (Value::U8(x), Value::U8(y)) => x <= y,
        (Value::F32(x), Value::F32(y)) => x <= y,
        (Value::F64(x), Value::F64(y)) => x <= y,
        _ => false,
    }
}

fn as_bool(v: &Value) -> bool {
    matches!(v, Value::Bool(true))
}

/// An integer constant as `i128` (sign-extended); panics on a non-integer.
fn int_val(v: &Value) -> i128 {
    match v {
        Value::I32(n) => *n as i128,
        Value::I64(n) => *n as i128,
        Value::U8(n) => *n as i128,
        _ => i128::MIN, // sentinel: never equals 0/1, so no identity fires.
    }
}

// --- graph readers --------------------------------------------------------

/// The single op-defining in-edge of `o` (its one non-`Pair` in-edge), or `None`
/// for a product / source / loop object.
fn op_edge(ir: &CategoryIr, o: ObjectId) -> Option<MorphismId> {
    let ins = ir.in_edges(o);
    if ins.len() != 1 {
        return None;
    }
    let m = ins[0];
    match ir.morphism(m).expect("in-edge").op {
        Operation::Pair { .. }
        | Operation::LoopEnter
        | Operation::LoopBack
        | Operation::LoopExit
        | Operation::Output => None,
        _ => Some(m),
    }
}

/// The `Value` of a `Constant` object, else `None`.
fn const_val(ir: &CategoryIr, o: ObjectId) -> Option<&Value> {
    let obj = ir.object(o)?;
    if obj.kind == ObjectKind::Constant {
        obj.value.as_ref()
    } else {
        None
    }
}

/// Whether `o` is an integer-typed object.
fn is_int_obj(ir: &CategoryIr, o: ObjectId) -> bool {
    ir.object(o).map(|x| x.ty.is_integer()).unwrap_or(false)
}

/// Whether `o` is a pack-built product (all in-edges are `Pair`, ≥1).
fn is_pack(ir: &CategoryIr, o: ObjectId) -> bool {
    let ins = ir.in_edges(o);
    !ins.is_empty()
        && ins
            .iter()
            .all(|&m| matches!(ir.morphism(m).expect("in").op, Operation::Pair { .. }))
}

/// Slot feeders of a product, in slot order (declaration = slot order).
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

/// Per-object membership in **any** loop SCC (each function's SCCs disjoint by
/// ownership). Present ⇒ loop-carried, excluded from all §2 keys/values (P2).
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
