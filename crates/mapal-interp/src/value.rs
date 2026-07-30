//! The value domain (`Dat`) and the public outcome types (interp DESIGN §1).
//!
//! `RValue` mirrors `mapal_ir::Ty` structurally (scalar/tuple/struct/array) plus
//! the world `Token`. Internally evaluation threads `Result<RValue, Abort>`;
//! `Outcome` is the public lift at the entry boundary (§7). No `Display` impls
//! (C3) — a plain [`render`] owns value→string.

use std::rc::Rc;

use mapal_ir::Value;

/// The interpreter's runtime value domain (interp DESIGN §1).
#[derive(Clone, Debug, PartialEq)]
pub enum RValue {
    /// `i32`/`i64`/`u8`/`f32`/`f64`/`bool`/`str` (str only as a `Print` arg, §5).
    Scalar(Value),
    /// An anonymous product; arity ≥ 2.
    Tuple(Vec<RValue>),
    /// A named product, in declared field order.
    Struct {
        name: String,
        fields: Vec<(String, RValue)>,
    },
    /// A fixed-size array; len pinned by the source `Ty`.
    ///
    /// `Rc` so that cloning a value *containing* an array is O(1). Evaluation
    /// clones constantly — capture broadcast per `Map`/`Fold` step (eval.rs
    /// `caps.clone()`) and `Pair` staging — so a deep copy per clone made every
    /// array program quadratic in the array length. Writes go through
    /// `Rc::make_mut` (`Update`), which keeps the copy-on-write semantics
    /// ADR-0021 already specifies: a fresh array, the original untouched.
    Array(Rc<Vec<RValue>>),
    /// The world token: the output log accumulated so far (§5).
    Token(String),
    /// The `Unit` witness (mirrors `Ty::Unit`; `Tuple` stays arity ≥ 2).
    Unit,
}

impl RValue {
    /// Build an [`RValue::Array`] from its elements.
    pub fn array(elems: Vec<RValue>) -> Self {
        RValue::Array(Rc::new(elems))
    }
}

/// A defined runtime trap (ADR-0013).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// Integer divide/modulo by zero.
    DivZero,
    /// Out-of-range (incl. negative) `Index`.
    IndexOob,
}

/// The public outcome of a run (interp DESIGN §1). The lift of the internal
/// `Result<RValue, Abort>` at the entry boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The run halted with a value.
    Done(RValue),
    /// The global step budget was exhausted (E1).
    Diverged,
    /// A defined runtime trap fired (ADR-0013).
    Trapped(TrapKind),
}

/// The internal abort carrier: every per-op happy path threads
/// `Result<RValue, Abort>`, and `?` propagates the first abort (interp DESIGN §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// Budget exhausted.
    Diverged,
    /// A trap fired.
    Trapped(TrapKind),
}

impl Abort {
    /// Lift an internal abort into the public outcome.
    pub fn into_outcome(self) -> Outcome {
        match self {
            Abort::Diverged => Outcome::Diverged,
            Abort::Trapped(k) => Outcome::Trapped(k),
        }
    }
}

/// Render a scalar value for `Print` (interp DESIGN §3/§5): integers → decimal;
/// `Bool` → `true`/`false`; floats via Rust shortest round-trip `Display`
/// (`4080.0 → "4080"`, `5.375 → "5.375"`); `Str(s) → s`.
pub fn render(v: &Value) -> String {
    match v {
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::F32(x) => format!("{x}"),
        Value::F64(x) => format!("{x}"),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
    }
}
