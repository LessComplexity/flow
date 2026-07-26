//! Diagnostics: structured, renderer-free values (DESIGN §2).
//!
//! `Debug` is derived so tests can snapshot the structured values. There is
//! deliberately **no** `Display` impl on `Diagnostic` in this crate (invariant
//! I5 / constraint C3): terminal rendering lives only in `mapal-cli`.

use crate::loc::SourceLoc;

/// A stable, machine-readable diagnostic code.
///
/// Convention: `"L####"` for lexer, `"P####"` for parser, `"T####"` for check.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DiagCode(pub &'static str);

/// Diagnostic severity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Severity {
    Error,
    Warning,
}

/// A machine-applicable suggested fix: replace `span` with `replacement`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SuggestedFix {
    pub span: SourceLoc,
    pub replacement: String,
    pub label: &'static str,
}

/// A structured diagnostic. Plain-text `message`, no formatting or color.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    /// Stable machine code, e.g. `L0004`.
    pub code: DiagCode,
    pub severity: Severity,
    pub span: SourceLoc,
    /// Plain text, no formatting/color (C3).
    pub message: String,
    /// Machine-applicable suggestion, if any.
    pub fix: Option<SuggestedFix>,
}

impl Diagnostic {
    /// Construct an error-severity diagnostic with no fix.
    pub(crate) fn error(code: &'static str, span: SourceLoc, message: impl Into<String>) -> Self {
        Diagnostic {
            code: DiagCode(code),
            severity: Severity::Error,
            span,
            message: message.into(),
            fix: None,
        }
    }

    /// Attach a suggested fix (builder style).
    pub(crate) fn with_fix(mut self, fix: SuggestedFix) -> Self {
        self.fix = Some(fix);
        self
    }
}
