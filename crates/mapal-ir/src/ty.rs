//! `Ty` and `Value` — the Core type subset (DESIGN §3; the ADR-0013 delta from
//! category-ir §3.4) plus the shared, iterative, depth-guarded type predicates.
//!
//! Three rules concentrate here, and every one of them must be iterative or
//! depth-guarded (I10 / J1 — mapal-syntax's one blocker-grade defect was
//! unguarded type recursion; this crate does not repeat it):
//!
//! - [`ty_depth_ok`] — the `MAX_TY_DEPTH` intake guard (I10).
//! - [`ty_contains_token`] — the ONE shared token predicate used by every token
//!   rule (I4, I4b, Phi, Map/Fold bodies, loop `U`; DESIGN §8/§9). A
//!   top-level-only check would miss `((IoToken, x), y)`.
//! - [`ty_label`] — a plain function (NOT `Display`; C3) for Mermaid text.

/// `Ty` recursion is bounded at intake (I10). `((((…))))` past this is rejected
/// with `TyTooDeep` rather than risking a stack overflow in any later walk.
pub const MAX_TY_DEPTH: usize = 64;

/// The Core type universe (DESIGN §3; ADR-0013 delta from category-ir §3.4).
///
/// Omitted vs spec §3.4 (added when their feature lands): `Char`, `Never`,
/// `Sum`, `Option`, `Result`, `Fn`, `Io`/`State` (subsumed by `IoToken`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// Core admits exactly `(32,true)=i32`, `(64,true)=i64`, `(8,false)=u8`.
    Int {
        bits: u8,
        signed: bool,
    },
    /// Core admits exactly `32` and `64`.
    Float {
        bits: u8,
    },
    Bool,
    /// Terminal object `1`; the ty of main's input and of unwritten returns.
    Unit,
    /// String-literal type; Core: valid ONLY as a `Print` input (HANDOFF §4.1).
    Str,
    /// The linear world token (DESIGN §8; ADR-0013).
    IoToken,
    /// Anonymous product; arity ≥ 2 (no 0/1-tuples — `Unit` is its own ty).
    Tuple(Vec<Ty>),
    /// Named product; structural equality includes the name.
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
    },
    /// Fixed-size array; `size` ≥ 1 (matches `TyKind::Array { len: u64 }`).
    Array {
        elem: Box<Ty>,
        size: u64,
    },
}

impl Ty {
    /// The three Core signed integers / one unsigned byte (`i32`, `i64`, `u8`).
    pub fn i32() -> Ty {
        Ty::Int {
            bits: 32,
            signed: true,
        }
    }
    pub fn i64() -> Ty {
        Ty::Int {
            bits: 64,
            signed: true,
        }
    }
    pub fn u8() -> Ty {
        Ty::Int {
            bits: 8,
            signed: false,
        }
    }
    pub fn f32() -> Ty {
        Ty::Float { bits: 32 }
    }
    pub fn f64() -> Ty {
        Ty::Float { bits: 64 }
    }

    /// Core numeric scalar `N` (DESIGN §5.1): `i32`/`i64`/`u8`/`f32`/`f64`.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int { .. } | Ty::Float { .. })
    }

    /// Core integer scalar `I` (DESIGN §5.1): the `Int` cases only (`Index`).
    pub fn is_integer(&self) -> bool {
        matches!(self, Ty::Int { .. })
    }

    /// Printable `P` (DESIGN §5.1): `N`, `Bool`, or `Str`.
    pub fn is_printable(&self) -> bool {
        self.is_numeric() || matches!(self, Ty::Bool | Ty::Str)
    }

    /// The product arity of a Tuple/Struct/Array, else `None` (DESIGN §5.1 /
    /// I-RET slot mechanics).
    ///
    /// Returns the TRUE arity as `u64` — `Array.size` is a `u64` (DESIGN §3), so
    /// a `u32` return would silently truncate a `size > u32::MAX` array and let
    /// an out-of-bounds ret-slot index pass the `k < arity` check (reviewer
    /// F4/SND-3). Slot indices are `u32`; callers compare them against this `u64`
    /// without truncation, and reject products whose arity exceeds `u32::MAX` as
    /// not slot-addressable (the `Pair { arity: u32 }` field cannot name them).
    pub fn product_arity(&self) -> Option<u64> {
        match self {
            Ty::Tuple(ts) => Some(ts.len() as u64),
            Ty::Struct { fields, .. } => Some(fields.len() as u64),
            Ty::Array { size, .. } => Some(*size),
            _ => None,
        }
    }

    /// The product arity as a slot-addressable `u32`, else `None`.
    ///
    /// `None` when the ty is not a product OR its arity exceeds `u32::MAX`: a
    /// `Pair { slot: u32, arity: u32 }` edge cannot name a slot in such a
    /// product, so it is not slot-addressable (reviewer F4/SND-3). Used wherever
    /// the builder must materialize a `Pair { arity }` field.
    pub fn product_arity_u32(&self) -> Option<u32> {
        match self.product_arity() {
            Some(a) if a <= u32::MAX as u64 => Some(a as u32),
            _ => None,
        }
    }

    /// The component ty at product slot `k` (Tuple/Struct/Array), else `None`.
    pub fn component_ty(&self, k: u32) -> Option<&Ty> {
        match self {
            Ty::Tuple(ts) => ts.get(k as usize),
            Ty::Struct { fields, .. } => fields.get(k as usize).map(|(_, t)| t),
            Ty::Array { elem, size } => {
                if (k as u64) < *size {
                    Some(elem)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Runtime values for `Constant` objects (DESIGN §3). `PartialEq` only — floats.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    I32(i32),
    I64(i64),
    U8(u8),
    F32(f32),
    F64(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    /// The ty of a value — total (DESIGN §3). `value.ty() == object.ty` is I7.
    pub fn ty(&self) -> Ty {
        match self {
            Value::I32(_) => Ty::i32(),
            Value::I64(_) => Ty::i64(),
            Value::U8(_) => Ty::u8(),
            Value::F32(_) => Ty::f32(),
            Value::F64(_) => Ty::f64(),
            Value::Bool(_) => Ty::Bool,
            Value::Str(_) => Ty::Str,
        }
    }
}

/// Whether `ty` and every nested ty is within `MAX_TY_DEPTH` (I10).
///
/// Iterative explicit-stack walk; never recurses (J1). Depth is the nesting
/// level: a scalar is depth 1, `Tuple([scalar])` is depth 2, and so on.
pub fn ty_depth_ok(ty: &Ty) -> bool {
    // Stack of (ty, depth-at-which-this-ty-sits). Depth starts at 1.
    let mut stack: Vec<(&Ty, usize)> = vec![(ty, 1)];
    while let Some((t, depth)) = stack.pop() {
        if depth > MAX_TY_DEPTH {
            return false;
        }
        match t {
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Struct { fields, .. } => {
                for (_, inner) in fields {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Array { elem, .. } => {
                stack.push((elem, depth + 1));
            }
            _ => {}
        }
    }
    true
}

/// Whether `ty` contains an [`Ty::IoToken`] anywhere (DESIGN §8/§9).
///
/// The ONE shared token predicate (I4, I4b, Phi, Map/Fold bodies, loop `U`).
/// Iterative + depth-guarded: a malformed-deep ty short-circuits to `false`
/// (such a ty cannot pass intake anyway, so this can never be reached for a
/// real graph object — the guard is defense in depth for J1).
pub fn ty_contains_token(ty: &Ty) -> bool {
    let mut stack: Vec<(&Ty, usize)> = vec![(ty, 1)];
    while let Some((t, depth)) = stack.pop() {
        if depth > MAX_TY_DEPTH {
            return false;
        }
        match t {
            Ty::IoToken => return true,
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Struct { fields, .. } => {
                for (_, inner) in fields {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Array { elem, .. } => {
                stack.push((elem, depth + 1));
            }
            _ => {}
        }
    }
    false
}

/// Whether `ty` has **no runtime representation** (the backends' erased rule:
/// `Unit`/`Str`/`IoToken`, an array of an erased element, or a product whose
/// components are all erased — the LLVM `lower_ty → None` rule, lifted to the
/// IR so the builder and validate can share it). A ty has a representation iff
/// some `Int`/`Float`/`Bool` leaf survives in its tree (an array materializes
/// iff its element does; a product iff any component does). Iterative +
/// depth-guarded, same shape as [`ty_contains_token`].
pub fn ty_residual_empty(ty: &Ty) -> bool {
    let mut stack: Vec<(&Ty, usize)> = vec![(ty, 1)];
    while let Some((t, depth)) = stack.pop() {
        if depth > MAX_TY_DEPTH {
            return false;
        }
        match t {
            Ty::Int { .. } | Ty::Float { .. } | Ty::Bool => return false,
            Ty::Unit | Ty::Str | Ty::IoToken => {}
            Ty::Array { elem, .. } => stack.push((elem, depth + 1)),
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Struct { fields, .. } => {
                for (_, inner) in fields {
                    stack.push((inner, depth + 1));
                }
            }
        }
    }
    true
}

/// Whether `ty` contains a [`Ty::Str`] anywhere (I9s helper).
///
/// Iterative + depth-guarded, same shape as [`ty_contains_token`]. Used to keep
/// `Str` out of every product/array except the `print()`-internal pair.
pub fn ty_contains_str(ty: &Ty) -> bool {
    let mut stack: Vec<(&Ty, usize)> = vec![(ty, 1)];
    while let Some((t, depth)) = stack.pop() {
        if depth > MAX_TY_DEPTH {
            return false;
        }
        match t {
            Ty::Str => return true,
            Ty::Tuple(ts) => {
                for inner in ts {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Struct { fields, .. } => {
                for (_, inner) in fields {
                    stack.push((inner, depth + 1));
                }
            }
            Ty::Array { elem, .. } => {
                stack.push((elem, depth + 1));
            }
            _ => {}
        }
    }
    false
}

/// Render a `Ty` as Mermaid/debug text (DESIGN §14): `i32`, `(i32, bool)`,
/// `[f32; 8]`, `Pixel`, `str`, `io`.
///
/// A plain named function, NOT a `Display` impl (constraint C3). Iterative —
/// builds the string with an explicit work stack so deep tys cannot overflow
/// (J1); a too-deep ty (already rejected at intake) renders as `…`.
pub fn ty_label(ty: &Ty) -> String {
    let mut out = String::new();
    ty_label_into(ty, 1, &mut out);
    out
}

/// Depth-guarded recursive helper bounded by `MAX_TY_DEPTH`. Recursion here is
/// safe: the bound is `MAX_TY_DEPTH` (64), every Ty in a sealed graph already
/// passed `ty_depth_ok`, and the guard makes the worst case a no-op (J1).
fn ty_label_into(ty: &Ty, depth: usize, out: &mut String) {
    if depth > MAX_TY_DEPTH {
        out.push('…');
        return;
    }
    match ty {
        Ty::Int { bits, signed } => {
            out.push(if *signed { 'i' } else { 'u' });
            out.push_str(&bits.to_string());
        }
        Ty::Float { bits } => {
            out.push('f');
            out.push_str(&bits.to_string());
        }
        Ty::Bool => out.push_str("bool"),
        Ty::Unit => out.push_str("()"),
        Ty::Str => out.push_str("str"),
        Ty::IoToken => out.push_str("io"),
        Ty::Tuple(ts) => {
            out.push('(');
            for (i, t) in ts.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                ty_label_into(t, depth + 1, out);
            }
            out.push(')');
        }
        Ty::Struct { name, .. } => out.push_str(name),
        Ty::Array { elem, size } => {
            out.push('[');
            ty_label_into(elem, depth + 1, out);
            out.push_str("; ");
            out.push_str(&size.to_string());
            out.push(']');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nest(depth: usize) -> Ty {
        // depth=1 → scalar; each extra wraps a 1-tuple-ish via Array (size 1).
        let mut t = Ty::i32();
        for _ in 1..depth {
            t = Ty::Array {
                elem: Box::new(t),
                size: 1,
            };
        }
        t
    }

    #[test]
    fn depth_guard_accepts_at_limit_rejects_past() {
        // depth 64 is the deepest allowed.
        assert!(ty_depth_ok(&nest(MAX_TY_DEPTH)));
        assert!(!ty_depth_ok(&nest(MAX_TY_DEPTH + 1)));
    }

    #[test]
    fn token_predicate_finds_deeply_nested_token() {
        // ((IoToken, i32), i32) — nested inside a tuple-of-tuple.
        let inner = Ty::Tuple(vec![Ty::IoToken, Ty::i32()]);
        let outer = Ty::Tuple(vec![inner, Ty::i32()]);
        assert!(ty_contains_token(&outer));
        // No token anywhere.
        let clean = Ty::Tuple(vec![Ty::Tuple(vec![Ty::i32(), Ty::Bool]), Ty::f32()]);
        assert!(!ty_contains_token(&clean));
        // Token in a struct field.
        let s = Ty::Struct {
            name: "W".into(),
            fields: vec![("t".into(), Ty::IoToken)],
        };
        assert!(ty_contains_token(&s));
        // Token in an array element.
        let a = Ty::Array {
            elem: Box::new(Ty::IoToken),
            size: 3,
        };
        assert!(ty_contains_token(&a));
    }

    #[test]
    fn str_predicate_finds_nested_str() {
        let t = Ty::Tuple(vec![Ty::i32(), Ty::Tuple(vec![Ty::Str, Ty::Bool])]);
        assert!(ty_contains_str(&t));
        assert!(!ty_contains_str(&Ty::Tuple(vec![Ty::i32(), Ty::Bool])));
    }

    #[test]
    fn value_ty_is_total() {
        assert_eq!(Value::I32(1).ty(), Ty::i32());
        assert_eq!(Value::U8(1).ty(), Ty::u8());
        assert_eq!(Value::F64(1.0).ty(), Ty::f64());
        assert_eq!(Value::Bool(true).ty(), Ty::Bool);
        assert_eq!(Value::Str("x".into()).ty(), Ty::Str);
    }

    #[test]
    fn labels_match_design_examples() {
        assert_eq!(ty_label(&Ty::i32()), "i32");
        assert_eq!(ty_label(&Ty::u8()), "u8");
        assert_eq!(ty_label(&Ty::Bool), "bool");
        assert_eq!(ty_label(&Ty::Unit), "()");
        assert_eq!(ty_label(&Ty::Str), "str");
        assert_eq!(ty_label(&Ty::IoToken), "io");
        assert_eq!(
            ty_label(&Ty::Tuple(vec![Ty::i32(), Ty::Bool])),
            "(i32, bool)"
        );
        assert_eq!(
            ty_label(&Ty::Array {
                elem: Box::new(Ty::f32()),
                size: 8
            }),
            "[f32; 8]"
        );
        assert_eq!(
            ty_label(&Ty::Struct {
                name: "Pixel".into(),
                fields: vec![]
            }),
            "Pixel"
        );
    }
}
