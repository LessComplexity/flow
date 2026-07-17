//! The Flow-Core parse tree (DESIGN §15): a **thin** tree — syntax + names +
//! spans + literal values + flags only. No name resolution, no type inference,
//! no effect classification (C10; architecture.md §2.2.2).
//!
//! `Debug` is derived on every node so tests can snapshot the structured
//! values; there is deliberately **no** `Display` impl anywhere in this crate
//! (C3 / I5 / J5) — rendering belongs to `flow-cli`. Every node carries a
//! [`SourceLoc`] span (C12); `Name` is a bare span (text via `&source[span]`,
//! the same single-source-of-truth rule as tokens).
//!
//! See DESIGN §15 for the type catalogue, §14 for the grammar each node maps
//! to, and §16 for the rejected-but-kept forms (`Call`, `Question`, `Dynamic`,
//! `StmtBlock`, `LoopLabel::Custom`, `GuardDiscr::OutOfCore`).

use crate::diag::Diagnostic;
use crate::loc::SourceLoc;

/// The result of parsing: the program tree plus the merged lex+parse
/// diagnostics, stably sorted by `(span.start, span.end)` (DESIGN §15/§18).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParseOutput {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

/// `program := item* EOF` (DESIGN §14.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: SourceLoc,
}

/// A top-level item: a function or type declaration (DESIGN §14.1). `Error`
/// holds a recovery region (e.g. a top-level statement, ⟦P0012⟧).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Item {
    Fn(FnDecl),
    Type(TypeDecl),
    Error(SourceLoc),
}

/// `fn-decl := 'fn' IDENT '(' params? ')' ( '->' type )? block` (DESIGN §14.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FnDecl {
    pub name: Name,
    pub params: Vec<Param>,
    pub ret_ty: Option<Ty>,
    pub body: Block,
    pub span: SourceLoc,
}

/// `param := 'mut'? IDENT ':' type` (DESIGN §14.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Param {
    pub mut_span: Option<SourceLoc>,
    pub name: Name,
    pub ty: Ty,
    pub span: SourceLoc,
}

/// `type-decl := ('type' | category) IDENT '{' field-list? '}'` (DESIGN §14.1).
/// A `category` keyword in this position is recovered as `type` with no new
/// diagnostic (L0004 already emitted; C14).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeDecl {
    pub name: Name,
    pub fields: Vec<Field>,
    pub span: SourceLoc,
}

/// `field := IDENT ':' type` (DESIGN §14.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    pub name: Name,
    pub ty: Ty,
    pub span: SourceLoc,
}

/// An identifier occurrence: a bare span (text via `&source[span]`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Name {
    pub span: SourceLoc,
}

/// A type with its span (DESIGN §14.2 / §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ty {
    pub kind: TyKind,
    pub span: SourceLoc,
}

/// The shape of a type (DESIGN §14.2). Generic args (⟦P0103⟧) keep their base
/// `Named`; `Dynamic` (`[T]`, ⟦P0104⟧) is kept for span-precise rejection.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TyKind {
    /// `i32`, `Pixel`, …; also the kept base of ⟦P0103⟧ generics.
    Named(Name),
    Tuple(Vec<Ty>),
    Array {
        elem: Box<Ty>,
        len: u64,
        len_span: SourceLoc,
    },
    /// `[T]` — kept, P0104 reported.
    Dynamic(Box<Ty>),
    Error,
}

/// `block := '{' block-item* tail? '}'` (DESIGN §14.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Block {
    pub items: Vec<BlockItem>,
    pub tail: Option<Chain>,
    pub span: SourceLoc,
}

/// A block item is a statement or a guard arm; mixing the two draws P0006
/// (DESIGN §14.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BlockItem {
    Stmt(Stmt),
    Arm(GuardArm),
}

/// A statement with its span (DESIGN §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: SourceLoc,
}

/// The shape of a statement (DESIGN §14.3 / §15). `Error` marks a recovery
/// region.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StmtKind {
    Chain(Chain),
    /// `place <- expr`.
    Bind(BindStmt),
    Loop(LoopStmt),
    /// Recovery region.
    Error,
}

/// `loop-stmt := 'loop' block | IDENT block` (DESIGN §14.3). A `Custom` label
/// means P0110 was reported (ADR-0011).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LoopStmt {
    pub label: LoopLabel,
    pub body: Block,
    pub span: SourceLoc,
}

/// The loop label: the keyword `loop`, or a custom identifier (⟦P0110⟧).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LoopLabel {
    Loop(SourceLoc),
    /// Custom ⇒ P0110 was reported.
    Custom(Name),
}

/// `bind-stmt := 'mut'? IDENT (':' type)? '<-' expr` (DESIGN §14.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BindStmt {
    pub mut_span: Option<SourceLoc>,
    pub name: Name,
    pub ty: Option<Ty>,
    pub value: Expr,
    pub span: SourceLoc,
}

/// `chain := expr stage* | stage+` — a headed or headless flat ordered stage
/// list (DESIGN §14.3; chains never nest arrows, C11).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Chain {
    pub head: Option<Expr>,
    pub stages: Vec<Stage>,
    pub span: SourceLoc,
}

/// A single `-> …` stage: the arrow's span, the stage kind, and the whole-stage
/// span (DESIGN §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Stage {
    pub arrow_span: SourceLoc,
    pub kind: StageKind,
    pub span: SourceLoc,
}

/// The classified body of a stage (DESIGN §14.3/§14.4 / §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum StageKind {
    /// Targets, tuple stages, ADR-0005 general expression stages.
    Expr(Expr),
    /// `-> x: i32` / `-> mut y` — a binding stage.
    Bind {
        mut_span: Option<SourceLoc>,
        name: Name,
        ty: Option<Ty>,
    },
    /// `-> ret` / `-> ret.0`.
    Ret {
        proj: Option<(u64, SourceLoc)>,
    },
    /// `-> loop` — back-edge jump, innermost loop (ADR-0011).
    LoopJump,
    /// Hole-expression: contains exactly one `Expr::Hole` leaf, as the leftmost
    /// leaf. `-> + 5` ⇒ `Binary(Add, Hole, Int 5)`; `-> * 2 + 1` ⇒
    /// `Binary(Add, Binary(Mul, Hole, Int 2), Int 1)`.
    OpShorthand {
        expr: Expr,
    },
    /// Ordered guard arms; lowering selects Phi vs Trace routing (§4.4/§4.5).
    Guard(Vec<GuardArm>),
    /// Fanout branches are headless chains.
    Fanout {
        kind: FanoutKind,
        branches: Vec<Chain>,
    },
    /// `map`/`fold` with an inline op-block (ADR-0009).
    MapFold {
        op: CollOp,
        params: Vec<Name>,
        body: Block,
    },
    /// `seq { … }` — an ordered statement block in stage position (ADR-0019).
    /// The body is the ordinary block production (statements + optional tail),
    /// **not** a fanout: `seq` is Flow-Core's keyword-marked block stage. Guard
    /// arms are illegal in it (a clean guard token → stray-guard P0004).
    SeqBlock(Block),
    /// ⟦P0115⟧ anonymous block stage — kept.
    StmtBlock(Block),
    Error(SourceLoc),
}

/// The kind of a fanout stage (DESIGN §14.4). `Void` ⇒ P0113 reported.
/// `seq` is no longer a fanout kind (ADR-0019): it is `StageKind::SeqBlock`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FanoutKind {
    Plain,
    Void(SourceLoc),
}

/// The collection operator of a `map`/`fold` stage (DESIGN §15).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CollOp {
    Map,
    Fold,
}

/// A guard arm: the discriminant, its span, the payload, and the whole-arm
/// span (DESIGN §14.3 / §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GuardArm {
    pub discr: GuardDiscr,
    pub discr_span: SourceLoc,
    pub payload: ArmPayload,
    pub span: SourceLoc,
}

/// The discriminant of a guard arm (DESIGN §15). `OutOfCore` ⇒ P0106 (pattern
/// arm).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardDiscr {
    True,
    False,
    Default,
    Int(u64),
    /// ⇒ P0106 (pattern guard arm).
    OutOfCore,
}

/// A guard-arm payload: a chain or a plain payload block (DESIGN §14.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArmPayload {
    Chain(Chain),
    Block(Block),
}

/// An expression with its span (DESIGN §14.6 / §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: SourceLoc,
}

/// The shape of an expression (DESIGN §14.6 / §15). `Call`, `Question`, and
/// `Error` are rejected-but-kept or recovery forms (C13).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExprKind {
    /// Value clamped like L0008 (re-parse of span digits; no new diagnostic, C14).
    Int(u64),
    /// Value is type-directed (f32 vs f64) — parsed later from the span.
    Float,
    /// Unescaping is the consumer's job (`unescape_string`).
    Str,
    Bool(bool),
    Var(Name),
    /// The piped value inside an `OpShorthand` rhs — never constructible from
    /// ordinary expression syntax.
    Hole,
    Unary {
        op: UnOp,
        op_span: SourceLoc,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        op_span: SourceLoc,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Member {
        base: Box<Expr>,
        field: MemberField,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Struct {
        name: Name,
        fields: Vec<FieldInit>,
    },
    /// ⟦P0108⟧ — kept for precision.
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// ⟦P0101⟧ — kept.
    Question(Box<Expr>),
    Error,
}

/// A member-access field: a named field or a tuple projection (`x.0`) (DESIGN §15).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MemberField {
    Named(Name),
    Index { value: u64, span: SourceLoc },
}

/// A struct field initializer; `value: None` is a pun shorthand (DESIGN §14.6).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FieldInit {
    pub name: Name,
    pub value: Option<Expr>,
    pub span: SourceLoc,
}

/// A unary operator (DESIGN §15). Not in the §3.6 table; binds tighter than
/// `*`, looser than postfix (W15).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnOp {
    Neg,
    Not,
}

/// A binary operator (DESIGN §15; §14.6 precedence levels 3–7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}
