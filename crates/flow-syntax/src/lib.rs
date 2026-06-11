//! `flow-syntax`: the Flow-Core lexer.
//!
//! This crate provides a handwritten, error-recovering, total lexer for the
//! Flow language (DESIGN `docs/components/syntax/DESIGN.md`). The public
//! surface is [`lex`], which turns a `&str` into a [`LexOutput`] of tokens and
//! structured [`Diagnostic`]s.
//!
//! Design highlights:
//!
//! - **Spans from day one.** Every token carries a [`SourceLoc`] byte range;
//!   [`LineIndex`] converts offsets to 1-based line/column for display.
//! - **Error-recovering.** Lexing never bails: unknown input becomes [`Error`]
//!   tokens paired with diagnostics, and scanning continues.
//! - **Renderer-free diagnostics.** [`Diagnostic`] derives `Debug` only — no
//!   `Display`/terminal rendering lives here (that is `flow-cli`'s job).
//! - **Guard arrows are single lexemes** (`-true->`, `-42->`, …), recognized
//!   under an adjacency + statement-initial context rule (ADR-0010).
//!
//! [`Error`]: TokenKind::Error

mod diag;
mod lexer;
mod loc;
mod token;

pub use diag::{DiagCode, Diagnostic, Severity, SuggestedFix};
pub use lexer::{LexOutput, lex};
pub use loc::{LineCol, LineIndex, SourceLoc};
pub use token::{GuardKind, Token, TokenKind, unescape_string};
