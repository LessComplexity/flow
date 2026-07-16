//! The check-stage diagnostic catalogue (DESIGN §2): the `T####` code space
//! (reserved in flow-syntax diag.rs). Diagnostics reuse
//! [`flow_syntax::Diagnostic`] (pub fields, constructed directly); severity is
//! always `Error` in v1, `fix` is `None` (DESIGN §1). Mirror of lower's
//! `diag.rs` in style.
//!
//! Every T-code in DESIGN §2 has a `pub fn` constructor here (via [`diag`]) and
//! `≥1` rejection test exercises each (DESIGN §2 / §7).

use flow_syntax::{DiagCode, Diagnostic, Severity, SourceLoc};

/// The check-stage T-codes (DESIGN §2). One variant per catalogue row; the
/// discriminant string is the stable machine code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TCode {
    MultipleReturnWriters, // T0101
    EffectInFanout,        // T0201
}

impl TCode {
    /// The stable machine code string (e.g. `"T0101"`).
    pub fn code(self) -> &'static str {
        match self {
            TCode::MultipleReturnWriters => "T0101",
            TCode::EffectInFanout => "T0201",
        }
    }
}

/// Build a check diagnostic at `span` with `message` and the given T-code. The
/// `&'static str` returned by [`TCode::code`] is exactly what [`DiagCode`]
/// wants.
pub fn diag(code: TCode, span: SourceLoc, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: DiagCode(code.code()),
        severity: Severity::Error,
        span,
        message: message.into(),
        fix: None,
    }
}
