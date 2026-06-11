//! Token model (DESIGN §3): payload-free literals, keywords, punctuation,
//! operators, and the single-lexeme guard arrows.

use crate::loc::SourceLoc;

/// A lexed token: a kind plus its source span. `Copy`.
///
/// Lexeme text is recovered via `&source[span.start..span.end]` when needed,
/// so there is exactly one source of truth for the text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceLoc,
}

/// The discriminant of a guard arm, carried inside the `Guard` token because
/// the digits live *inside* the lexeme (`-42->`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardKind {
    /// `-true->`
    True,
    /// `-false->`
    False,
    /// `-_->`
    Default,
    /// `-<digits>->`; value clamped to `u64::MAX` on overflow (L0008).
    Int(u64),
}

/// The kind of a token. Payload-free except `Guard`, which carries its
/// discriminant (DESIGN §3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    // Literals / names — text recovered via span.
    Ident,
    Int,
    Float,
    Str,

    // Flow-Core keywords.
    KwFn,
    KwType,
    KwMut,
    KwLoop,
    KwRet,
    KwSeq,
    KwMap,
    KwFold,
    KwTrue,
    KwFalse,

    // Reserved keywords (lexed, rejected later or at lex time per §2).
    KwCategory,
    KwExecutor,
    KwPub,
    KwUse,
    KwVoid,

    // Delimiters & punctuation.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDotDot,

    // Operators.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    BangEq,
    Lt,
    Gt,
    Le,
    Ge,
    AmpAmp,
    PipePipe,
    Bang,
    /// `->`
    Arrow,
    /// `<-`
    BackArrow,
    /// `-true->` `-false->` `-_->` `-42->` (single lexeme, §5).
    Guard(GuardKind),
    /// `?` — Core+1 surface, lexed for parser-side rejection.
    Question,
    /// `@` — annotations (§9), lexed for parser-side rejection.
    At,
    /// Unknown input; carries diagnostics alongside.
    Error,
    /// Always last, zero-width span at end of input.
    Eof,
}

impl Token {
    #[inline]
    pub(crate) fn new(kind: TokenKind, span: SourceLoc) -> Self {
        Token { kind, span }
    }
}

/// Map an identifier-shaped lexeme to its keyword kind, or `None` for a plain
/// identifier.
///
/// Covers Flow-Core keywords, the reserved-but-lexable keywords, and the
/// reserved-and-rejected `category` (§3 / §2).
pub(crate) fn keyword_kind(text: &str) -> Option<TokenKind> {
    use TokenKind::*;
    Some(match text {
        // Flow-Core keywords.
        "fn" => KwFn,
        "type" => KwType,
        "mut" => KwMut,
        "loop" => KwLoop,
        "ret" => KwRet,
        "seq" => KwSeq,
        "map" => KwMap,
        "fold" => KwFold,
        "true" => KwTrue,
        "false" => KwFalse,
        // Reserved keywords.
        "category" => KwCategory,
        "executor" => KwExecutor,
        "pub" => KwPub,
        "use" => KwUse,
        "void" => KwVoid,
        _ => return None,
    })
}

/// Unescape a *lexically valid* string token's content.
///
/// `lexeme` is the full token text **including** the surrounding double
/// quotes (e.g. `"\"a\\n\""`). Recognized escapes: `\\`, `\"`, `\n`, `\t`.
/// Any other escape is left as its literal escaped character (matching the
/// lexer's L0003 recovery, which takes the char literally). Assumes the
/// caller passes a token the lexer produced; on a malformed/unterminated
/// input it does the most faithful thing and never panics.
pub fn unescape_string(lexeme: &str) -> String {
    // Strip surrounding quotes if present.
    let inner = {
        let bytes = lexeme.as_bytes();
        let start = if bytes.first() == Some(&b'"') { 1 } else { 0 };
        let end = if bytes.len() > start && bytes.last() == Some(&b'"') {
            lexeme.len() - 1
        } else {
            lexeme.len()
        };
        if start <= end {
            &lexeme[start..end]
        } else {
            ""
        }
    };

    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            // Unknown escape: take the char literally (mirrors L0003).
            Some(other) => out.push(other),
            // Dangling backslash at end (shouldn't occur in a valid token).
            None => out.push('\\'),
        }
    }
    out
}
