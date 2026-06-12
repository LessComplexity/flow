//! The lower-stage diagnostic catalogue (DESIGN §4): the L1000–L1999 band of
//! the `L` code space (LD16). Diagnostics reuse [`flow_syntax::Diagnostic`]
//! (pub fields, constructed directly); severity is always `Error` in v1, `fix`
//! is `None` (DESIGN §1). Message text is plain, no color (C3).
//!
//! Every L-code in DESIGN §4 has a `pub fn` constructor here, and `≥1` rejection
//! test exercises each (DESIGN §14.4) — except L1901, raised only by a builder
//! call escaping during emission (a lower bug by definition).

use flow_syntax::{DiagCode, Diagnostic, Severity, SourceLoc};

/// The lower-stage L-codes (DESIGN §4). One variant per catalogue row; the
/// discriminant string is the stable machine code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LCode {
    UncleanTree,           // L1000
    NoMain,                // L1001
    MainShape,             // L1002
    DuplicateFn,           // L1003
    DuplicateType,         // L1004
    DuplicateField,        // L1005
    DuplicateParam,        // L1006
    RecursiveType,         // L1007
    RecursiveCall,         // L1008
    ReservedName,          // L1009
    EmptyType,             // L1010
    UnknownName,           // L1101
    UnknownType,           // L1102
    UnknownField,          // L1103
    AssignImmutable,       // L1104
    FunctionAsValue,       // L1105
    NamedParamApplication, // L1106
    ReadAfterLoop,         // L1107
    CaptureInBody,         // L1108
    TypeMismatch,          // L1201
    LiteralOutOfRange,     // L1202
    LiteralTypeConflict,   // L1203
    NotAProduct,           // L1204
    SlotOutOfRange,        // L1205
    StrOutsidePrint,       // L1206
    Unprintable,           // L1207
    EmptyArray,            // L1208
    TypeTooDeep,           // L1209
    HeadlessChain,         // L1301
    ExprStage,             // L1302
    RetMidChain,           // L1303
    JumpMisplaced,         // L1304
    FanoutNoValue,         // L1305
    IncompleteReturn,      // L1306
    EffectfulReturnShape,  // L1307
    GuardArmMissing,       // L1401
    GuardArmDuplicate,     // L1402
    GuardArmMixed,         // L1403
    GuardArmEffectful,     // L1404
    GuardRetInPhiArm,      // L1405
    GuardScrutineeType,    // L1406
    RoutingGuardShape,     // L1407
    AssignInPhiArm,        // L1408
    RoutingGuardArms,      // L1409
    LoopNoExit,            // L1501
    LoopNoState,           // L1502
    LoopGuardShape,        // L1503
    NestedLoopShape,       // L1504
    BlockArity,            // L1601
    MapNonArray,           // L1602
    FoldShape,             // L1603
    BodyNoValue,           // L1604
    BodyEffectful,         // L1605
    Internal,              // L1901
}

impl LCode {
    /// The stable machine code string (e.g. `"L1306"`).
    pub fn code(self) -> &'static str {
        match self {
            LCode::UncleanTree => "L1000",
            LCode::NoMain => "L1001",
            LCode::MainShape => "L1002",
            LCode::DuplicateFn => "L1003",
            LCode::DuplicateType => "L1004",
            LCode::DuplicateField => "L1005",
            LCode::DuplicateParam => "L1006",
            LCode::RecursiveType => "L1007",
            LCode::RecursiveCall => "L1008",
            LCode::ReservedName => "L1009",
            LCode::EmptyType => "L1010",
            LCode::UnknownName => "L1101",
            LCode::UnknownType => "L1102",
            LCode::UnknownField => "L1103",
            LCode::AssignImmutable => "L1104",
            LCode::FunctionAsValue => "L1105",
            LCode::NamedParamApplication => "L1106",
            LCode::ReadAfterLoop => "L1107",
            LCode::CaptureInBody => "L1108",
            LCode::TypeMismatch => "L1201",
            LCode::LiteralOutOfRange => "L1202",
            LCode::LiteralTypeConflict => "L1203",
            LCode::NotAProduct => "L1204",
            LCode::SlotOutOfRange => "L1205",
            LCode::StrOutsidePrint => "L1206",
            LCode::Unprintable => "L1207",
            LCode::EmptyArray => "L1208",
            LCode::TypeTooDeep => "L1209",
            LCode::HeadlessChain => "L1301",
            LCode::ExprStage => "L1302",
            LCode::RetMidChain => "L1303",
            LCode::JumpMisplaced => "L1304",
            LCode::FanoutNoValue => "L1305",
            LCode::IncompleteReturn => "L1306",
            LCode::EffectfulReturnShape => "L1307",
            LCode::GuardArmMissing => "L1401",
            LCode::GuardArmDuplicate => "L1402",
            LCode::GuardArmMixed => "L1403",
            LCode::GuardArmEffectful => "L1404",
            LCode::GuardRetInPhiArm => "L1405",
            LCode::GuardScrutineeType => "L1406",
            LCode::RoutingGuardShape => "L1407",
            LCode::AssignInPhiArm => "L1408",
            LCode::RoutingGuardArms => "L1409",
            LCode::LoopNoExit => "L1501",
            LCode::LoopNoState => "L1502",
            LCode::LoopGuardShape => "L1503",
            LCode::NestedLoopShape => "L1504",
            LCode::BlockArity => "L1601",
            LCode::MapNonArray => "L1602",
            LCode::FoldShape => "L1603",
            LCode::BodyNoValue => "L1604",
            LCode::BodyEffectful => "L1605",
            LCode::Internal => "L1901",
        }
    }
}

/// Build a lower diagnostic at `span` with `message` and the given L-code. The
/// `&'static str` returned by [`LCode::code`] is exactly what [`DiagCode`]
/// wants.
pub fn diag(code: LCode, span: SourceLoc, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: DiagCode(code.code()),
        severity: Severity::Error,
        span,
        message: message.into(),
        fix: None,
    }
}
