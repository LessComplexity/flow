//! The Flow-Core lexer (DESIGN §4/§5): a single forward pass over the source,
//! maximal-munch, error-recovering, total (never panics).
//!
//! See `DESIGN.md` §4 (grammar), §5 (guard arrows / ADR-0010), §2 (diagnostic
//! codes L0001–L0008), and §8 (invariants I1–I5).

use crate::diag::{Diagnostic, SuggestedFix};
use crate::loc::SourceLoc;
use crate::token::{GuardKind, Token, TokenKind, keyword_kind};

/// The result of lexing: the token stream (always ending in `Eof`) and any
/// diagnostics produced during recovery.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Lex `source` into tokens + diagnostics.
///
/// Total function: never panics, never fails (C2/C4). The returned `tokens`
/// always end with a zero-width `Eof`.
pub fn lex(source: &str) -> LexOutput {
    Lexer::new(source).run()
}

/// Scanner state. `pos` is a byte offset; `bytes` is the source as bytes for
/// O(1) lookahead. UTF-8 only matters inside comments/strings, which are
/// scanned char-boundary safely by re-deriving char widths from `source`.
struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    len: usize,
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    /// Kind of the most recently emitted *token* (for the guard context gate).
    /// `None` at start of input. Trivia does not update this.
    last_kind: Option<TokenKind>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Lexer {
            source,
            bytes: source.as_bytes(),
            len: source.len(),
            pos: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            last_kind: None,
        }
    }

    fn run(mut self) -> LexOutput {
        while self.pos < self.len {
            let before = self.pos;
            self.skip_trivia();
            if self.pos >= self.len {
                break;
            }
            // If trivia advanced us to EOF or moved forward, re-check loop top.
            if self.pos != before && self.pos >= self.len {
                break;
            }
            self.scan_token();
            // I1: the scanner must always advance. Every `scan_*` path consumes
            // at least one byte; this is a defensive backstop.
            debug_assert!(
                self.pos > before || self.pos >= self.len,
                "lexer failed to advance at byte {before}"
            );
        }
        let eof_at = self.len as u32;
        self.push_token(TokenKind::Eof, SourceLoc::empty_at(eof_at));
        LexOutput {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    // --- byte helpers -----------------------------------------------------

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn peek_at(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.pos + n).copied()
    }

    #[inline]
    fn bump(&mut self) {
        self.pos += 1;
    }

    // --- emission ---------------------------------------------------------

    fn push_token(&mut self, kind: TokenKind, span: SourceLoc) {
        // I2: spans ascending, non-overlapping, in-bounds, char-aligned.
        debug_assert!(span.start <= span.end, "span start after end: {span:?}");
        debug_assert!(
            span.end as usize <= self.len,
            "span out of bounds: {span:?} (len {})",
            self.len
        );
        debug_assert!(
            self.source.is_char_boundary(span.start as usize),
            "span start not on char boundary: {span:?}"
        );
        debug_assert!(
            self.source.is_char_boundary(span.end as usize),
            "span end not on char boundary: {span:?}"
        );
        if let Some(prev) = self.tokens.last() {
            debug_assert!(
                prev.span.start <= span.start && prev.span.end <= span.start,
                "non-ascending / overlapping spans: {:?} then {span:?}",
                prev.span
            );
        }
        self.tokens.push(Token::new(kind, span));
        self.last_kind = Some(kind);
    }

    fn span(&self, start: usize, end: usize) -> SourceLoc {
        SourceLoc::new(start as u32, end as u32)
    }

    // --- trivia -----------------------------------------------------------

    /// Skip whitespace, line comments, and `/* */` block comments (the last
    /// with an L0007 diagnostic). Char-boundary safe: comment bodies may hold
    /// arbitrary UTF-8.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                    self.bump();
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    self.skip_line_comment();
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    self.skip_block_comment();
                }
                _ => break,
            }
        }
    }

    /// `//` to end of line (the `\n` itself is left to the whitespace branch).
    fn skip_line_comment(&mut self) {
        // Consume the two slashes.
        self.bump();
        self.bump();
        while let Some(b) = self.peek() {
            if b == b'\n' {
                break;
            }
            self.bump();
        }
    }

    /// `/* ... */` consumed as trivia, with L0007. Nesting is *not* honoured
    /// (Flow has line comments only; this is purely friendly recovery).
    fn skip_block_comment(&mut self) {
        let start = self.pos;
        // Consume `/*`.
        self.bump();
        self.bump();
        let mut closed = false;
        while self.pos < self.len {
            if self.peek() == Some(b'*') && self.peek_at(1) == Some(b'/') {
                self.bump();
                self.bump();
                closed = true;
                break;
            }
            self.bump();
        }
        let end = self.pos;
        let span = self.span(start, end);
        let message = if closed {
            "block comments (`/* */`) are not Flow syntax; use line comments (`//`)".to_string()
        } else {
            "block comments (`/* */`) are not Flow syntax; use line comments (`//`) \
             — also unterminated, skipped to end of input"
                .to_string()
        };
        self.diagnostics
            .push(Diagnostic::error("L0007", span, message));
    }

    // --- token scanning ---------------------------------------------------

    fn scan_token(&mut self) {
        let b = match self.peek() {
            Some(b) => b,
            None => return,
        };

        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.scan_ident_or_keyword(),
            b'0'..=b'9' => self.scan_number(),
            b'"' => self.scan_string(),
            b'-' => self.scan_minus(),
            b'<' => self.scan_lt(),
            b'>' => self.scan_gt(),
            b'=' => self.scan_eq(),
            b'!' => self.scan_bang(),
            b'&' => self.scan_amp(),
            b'|' => self.scan_pipe(),
            b'.' => self.scan_dot(),
            b'+' => self.single(TokenKind::Plus),
            b'*' => self.single(TokenKind::Star),
            b'/' => self.single(TokenKind::Slash),
            b'%' => self.single(TokenKind::Percent),
            b'(' => self.single(TokenKind::LParen),
            b')' => self.single(TokenKind::RParen),
            b'{' => self.single(TokenKind::LBrace),
            b'}' => self.single(TokenKind::RBrace),
            b'[' => self.single(TokenKind::LBracket),
            b']' => self.single(TokenKind::RBracket),
            b',' => self.single(TokenKind::Comma),
            b':' => self.single(TokenKind::Colon),
            b';' => self.single(TokenKind::Semi),
            b'?' => self.single(TokenKind::Question),
            b'@' => self.single(TokenKind::At),
            _ => self.scan_unknown(),
        }
    }

    /// Emit a one-byte token of `kind`.
    fn single(&mut self, kind: TokenKind) {
        let start = self.pos;
        self.bump();
        let span = self.span(start, self.pos);
        self.push_token(kind, span);
    }

    fn scan_ident_or_keyword(&mut self) {
        let start = self.pos;
        // First char already known to be [A-Za-z_].
        self.bump();
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.bump();
            } else {
                break;
            }
        }
        let end = self.pos;
        let text = &self.source[start..end];
        let kind = keyword_kind(text).unwrap_or(TokenKind::Ident);
        let span = self.span(start, end);
        if kind == TokenKind::KwCategory {
            // L0004: reserved keyword; point at `type`, with a fix.
            let diag = Diagnostic::error(
                "L0004",
                span,
                "`category` is not a keyword in Flow; the type-definition keyword is `type`",
            )
            .with_fix(SuggestedFix {
                span,
                replacement: "type".to_string(),
                label: "replace `category` with `type`",
            });
            self.diagnostics.push(diag);
        }
        self.push_token(kind, span);
    }

    /// Integer or float. Float requires digits on both sides of `.`
    /// (`ret.0` → `KwRet Dot Int`, `5.` → `Int Dot`). Integers exceeding u64
    /// clamp with L0008.
    fn scan_number(&mut self) {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.bump();
            } else {
                break;
            }
        }
        // Float? Need `.` followed by a digit.
        let is_float = self.peek() == Some(b'.')
            && self.peek_at(1).map(|b| b.is_ascii_digit()).unwrap_or(false);
        if is_float {
            // Consume the dot.
            self.bump();
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.bump();
                } else {
                    break;
                }
            }
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::Float, span);
        } else {
            let end = self.pos;
            let span = self.span(start, end);
            // Range check for L0008 on integer literals.
            let text = &self.source[start..end];
            if text.parse::<u64>().is_err() {
                self.diagnostics.push(Diagnostic::error(
                    "L0008",
                    span,
                    "integer literal exceeds the maximum value (u64::MAX); value clamped",
                ));
            }
            self.push_token(TokenKind::Int, span);
        }
    }

    /// String literal `"..."` on a single line. Handles escapes (L0003 for
    /// unknown), and L0002 for raw newline / EOF before the closing quote.
    fn scan_string(&mut self) {
        let start = self.pos;
        // Consume opening quote.
        self.bump();
        let mut unterminated = false;
        loop {
            match self.peek() {
                None => {
                    unterminated = true;
                    break;
                }
                Some(b'\n') => {
                    // Raw newline before close: token ends here, '\n' is trivia.
                    unterminated = true;
                    break;
                }
                Some(b'"') => {
                    self.bump(); // closing quote
                    break;
                }
                Some(b'\\') => {
                    let esc_start = self.pos;
                    self.bump(); // backslash
                    match self.peek() {
                        Some(b'\\') | Some(b'"') | Some(b'n') | Some(b't') => {
                            self.bump();
                        }
                        Some(b'\n') | None => {
                            // Backslash at EOL/EOF: unknown escape, then the
                            // string is unterminated at the break point.
                            let span = self.span(esc_start, self.pos);
                            self.diagnostics.push(Diagnostic::error(
                                "L0003",
                                span,
                                "unknown escape sequence; backslash taken literally",
                            ));
                            unterminated = true;
                            break;
                        }
                        Some(_) => {
                            // Unknown escape: take the escaped char literally,
                            // continue scanning the string.
                            let esc_end = self.next_char_boundary(self.pos);
                            let span = self.span(esc_start, esc_end);
                            self.diagnostics.push(Diagnostic::error(
                                "L0003",
                                span,
                                "unknown escape sequence; character taken literally",
                            ));
                            self.pos = esc_end;
                        }
                    }
                }
                Some(_) => {
                    // Ordinary char (possibly multi-byte UTF-8): advance one char.
                    self.pos = self.next_char_boundary(self.pos);
                }
            }
        }
        let end = self.pos;
        let span = self.span(start, end);
        if unterminated {
            self.diagnostics.push(Diagnostic::error(
                "L0002",
                span,
                "unterminated string literal (reached end of line or end of input \
                 before the closing `\"`)",
            ));
        }
        self.push_token(TokenKind::Str, span);
    }

    /// Advance from a char-boundary byte offset to the start of the next char.
    fn next_char_boundary(&self, from: usize) -> usize {
        let mut i = from + 1;
        while i < self.len && !self.source.is_char_boundary(i) {
            i += 1;
        }
        i.min(self.len)
    }

    /// `-`: may begin a guard arrow (§5/ADR-0010), `->`, or `Minus`.
    fn scan_minus(&mut self) {
        if self.try_guard_arrow() {
            // The guard token was emitted and `pos` advanced; nothing else to do.
            return;
        }
        let start = self.pos;
        // Ordinary: `->` if `>` follows, else `-`.
        if self.peek_at(1) == Some(b'>') {
            self.bump();
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::Arrow, span);
        } else {
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::Minus, span);
        }
    }

    /// Attempt the guard-arrow production `-D->` with zero interior whitespace,
    /// gated on the previous-token context (`{`, `;`, `}`, or start of input).
    ///
    /// Returns `true` and emits a `Guard` token on success, `false` if the
    /// production does not apply (the caller falls back to ordinary `-`/`->`).
    fn try_guard_arrow(&mut self) -> bool {
        // Context gate: prev token must be one of `{`, `;`, `}`, or none.
        let ctx_ok = matches!(
            self.last_kind,
            None | Some(TokenKind::LBrace) | Some(TokenKind::Semi) | Some(TokenKind::RBrace)
        );
        if !ctx_ok {
            return false;
        }

        let start = self.pos;
        debug_assert_eq!(self.peek(), Some(b'-'));
        // Position just after the leading '-'.
        let mut i = start + 1;

        // Parse the discriminant D at byte offset i.
        let d_start = i;
        let (kind, after_d) = match self.match_discriminant(i) {
            Some(d) => d,
            None => return false,
        };
        i = after_d;

        // Require an immediately-following `->` (zero interior whitespace).
        if self.bytes.get(i).copied() != Some(b'-') {
            return false;
        }
        if self.bytes.get(i + 1).copied() != Some(b'>') {
            return false;
        }
        let end = i + 2;

        // Commit: emit the guard token spanning `start..end`.
        self.pos = end;
        let span = self.span(start, end);
        // L0008 (DESIGN §2/§5): an integer discriminant that overflows u64 is
        // clamped to u64::MAX *and* flagged. Detect overflow by re-parsing the
        // discriminant digits; an exact u64::MAX value is not flagged.
        if let GuardKind::Int(_) = kind {
            let digits = &self.source[d_start..after_d];
            if digits.parse::<u64>().is_err() {
                self.diagnostics.push(Diagnostic::error(
                    "L0008",
                    span,
                    "guard discriminant exceeds the maximum value (u64::MAX); value clamped",
                ));
            }
        }
        self.push_token(TokenKind::Guard(kind), span);
        true
    }

    /// Match a guard discriminant `D ∈ { true, false, _, [0-9]+ }` at byte
    /// offset `at`. Returns `(GuardKind, byte offset just past D)`.
    ///
    /// The match must be *exactly* the keyword/underscore/digit-run; for the
    /// keyword/underscore forms the next byte must terminate the discriminant
    /// (a guard discriminant is followed immediately by `-`), which is checked
    /// by the caller requiring `-` next. We additionally ensure `true`/`false`/
    /// `_` are not a prefix of a longer identifier by checking the following
    /// byte is `-` (the only valid continuation), preventing e.g. `-trueX->`
    /// from matching `true`.
    fn match_discriminant(&self, at: usize) -> Option<(GuardKind, usize)> {
        // Digit run.
        if self
            .bytes
            .get(at)
            .map(|b| b.is_ascii_digit())
            .unwrap_or(false)
        {
            let mut j = at;
            while self
                .bytes
                .get(j)
                .map(|b| b.is_ascii_digit())
                .unwrap_or(false)
            {
                j += 1;
            }
            let text = &self.source[at..j];
            let value = text.parse::<u64>().unwrap_or(u64::MAX);
            return Some((GuardKind::Int(value), j));
        }
        // `_`
        if self.bytes.get(at).copied() == Some(b'_') {
            // Ensure it's a lone `_`, not `_foo`: next byte must be `-`.
            if self.bytes.get(at + 1).copied() == Some(b'-') {
                return Some((GuardKind::Default, at + 1));
            }
            return None;
        }
        // `true`
        if self.source[at..].starts_with("true") {
            let next = at + 4;
            if self.bytes.get(next).copied() == Some(b'-') {
                return Some((GuardKind::True, next));
            }
            return None;
        }
        // `false`
        if self.source[at..].starts_with("false") {
            let next = at + 5;
            if self.bytes.get(next).copied() == Some(b'-') {
                return Some((GuardKind::False, next));
            }
            return None;
        }
        None
    }

    /// `<`: `<-` (BackArrow), `<=` (Le), or `<` (Lt).
    fn scan_lt(&mut self) {
        let start = self.pos;
        self.bump();
        let kind = match self.peek() {
            Some(b'-') => {
                self.bump();
                TokenKind::BackArrow
            }
            Some(b'=') => {
                self.bump();
                TokenKind::Le
            }
            _ => TokenKind::Lt,
        };
        let span = self.span(start, self.pos);
        self.push_token(kind, span);
    }

    /// `>`: `>=` (Ge) or `>` (Gt).
    fn scan_gt(&mut self) {
        let start = self.pos;
        self.bump();
        let kind = if self.peek() == Some(b'=') {
            self.bump();
            TokenKind::Ge
        } else {
            TokenKind::Gt
        };
        let span = self.span(start, self.pos);
        self.push_token(kind, span);
    }

    /// `=`: `==` (EqEq) or lone `=` (Error + L0005).
    fn scan_eq(&mut self) {
        let start = self.pos;
        self.bump();
        if self.peek() == Some(b'=') {
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::EqEq, span);
        } else {
            let span = self.span(start, self.pos);
            self.diagnostics.push(Diagnostic::error(
                "L0005",
                span,
                "unexpected `=`; use `<-` for binding or `==` for comparison",
            ));
            self.push_token(TokenKind::Error, span);
        }
    }

    /// `!`: `!=` (BangEq) or `!` (Bang).
    fn scan_bang(&mut self) {
        let start = self.pos;
        self.bump();
        let kind = if self.peek() == Some(b'=') {
            self.bump();
            TokenKind::BangEq
        } else {
            TokenKind::Bang
        };
        let span = self.span(start, self.pos);
        self.push_token(kind, span);
    }

    /// `&`: `&&` (AmpAmp) or lone `&` (Error + L0006).
    fn scan_amp(&mut self) {
        let start = self.pos;
        self.bump();
        if self.peek() == Some(b'&') {
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::AmpAmp, span);
        } else {
            let span = self.span(start, self.pos);
            self.diagnostics.push(Diagnostic::error(
                "L0006",
                span,
                "unexpected `&`; Flow uses `&&` for logical and",
            ));
            self.push_token(TokenKind::Error, span);
        }
    }

    /// `|`: `||` (PipePipe) or lone `|` (Error + L0006).
    fn scan_pipe(&mut self) {
        let start = self.pos;
        self.bump();
        if self.peek() == Some(b'|') {
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::PipePipe, span);
        } else {
            let span = self.span(start, self.pos);
            self.diagnostics.push(Diagnostic::error(
                "L0006",
                span,
                "unexpected `|`; Flow uses `||` for logical or",
            ));
            self.push_token(TokenKind::Error, span);
        }
    }

    /// `.`: `...` (DotDotDot) or `.` (Dot). A bare `..` yields two `Dot`s
    /// (handled by emitting one `Dot` at a time).
    fn scan_dot(&mut self) {
        let start = self.pos;
        if self.peek_at(1) == Some(b'.') && self.peek_at(2) == Some(b'.') {
            self.bump();
            self.bump();
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::DotDotDot, span);
        } else {
            self.bump();
            let span = self.span(start, self.pos);
            self.push_token(TokenKind::Dot, span);
        }
    }

    /// Unknown input: coalesce the maximal run of consecutive unknown chars
    /// into one `Error` token + one L0001 (DESIGN §2, W8). Char-boundary safe.
    fn scan_unknown(&mut self) {
        let start = self.pos;
        // Advance by whole chars while the current char does not start any
        // recognized token and is not trivia.
        loop {
            match self.peek() {
                None => break,
                Some(b) if self.starts_known(b) => break,
                Some(_) => {
                    self.pos = self.next_char_boundary(self.pos);
                }
            }
        }
        let end = self.pos;
        let span = self.span(start, end);
        let lexeme = &self.source[start..end];
        self.diagnostics.push(Diagnostic::error(
            "L0001",
            span,
            format!("unexpected character(s): `{lexeme}`"),
        ));
        self.push_token(TokenKind::Error, span);
    }

    /// Whether byte `b` can start a recognized token or trivia (so the
    /// unknown-run should stop). ASCII bytes that map to a token/trivia branch
    /// in `scan_token`/`skip_trivia` return true; everything else (non-ASCII,
    /// stray control chars, `'`, `$`, `#`, `~`, `^`, backtick, `\`) is unknown.
    fn starts_known(&self, b: u8) -> bool {
        matches!(
            b,
            b'A'..=b'Z' | b'a'..=b'z' | b'_'
                | b'0'..=b'9'
                | b'"'
                | b'-' | b'<' | b'>' | b'=' | b'!' | b'&' | b'|' | b'.'
                | b'+' | b'*' | b'/' | b'%'
                | b'(' | b')' | b'{' | b'}' | b'[' | b']'
                | b',' | b':' | b';' | b'?' | b'@'
                // trivia
                | b' ' | b'\t' | b'\r' | b'\n'
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loc::LineIndex;
    use crate::token::{GuardKind, TokenKind};

    /// Lex and return just the kinds (dropping the trailing Eof).
    fn kinds(src: &str) -> Vec<TokenKind> {
        let out = lex(src);
        let mut ks: Vec<TokenKind> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks.last(), Some(&TokenKind::Eof), "missing Eof");
        ks.pop();
        ks
    }

    fn lexeme<'a>(src: &'a str, t: &Token) -> &'a str {
        &src[t.span.start as usize..t.span.end as usize]
    }

    fn codes(src: &str) -> Vec<&'static str> {
        lex(src).diagnostics.iter().map(|d| d.code.0).collect()
    }

    use TokenKind::*;

    // ---- §5 worked-consequences table (all 10 rows) ----------------------

    #[test]
    fn s5_row1_true_guard_after_brace() {
        // `-true->  x;` with prev token `{`.
        assert_eq!(
            kinds("{ -true->  x; }"),
            vec![LBrace, Guard(GuardKind::True), Ident, Semi, RBrace]
        );
    }

    #[test]
    fn s5_row2_false_guard_after_rbrace() {
        // `-false-> -> ret;` with prev token `}`.
        // Construct: `{ } -false-> -> ret;`
        assert_eq!(
            kinds("{ } -false-> -> ret;"),
            vec![LBrace, RBrace, Guard(GuardKind::False), Arrow, KwRet, Semi]
        );
    }

    #[test]
    fn s5_row3_default_guard_after_semi() {
        // `-_-> "unknown";` with prev token `;`.
        assert_eq!(
            kinds("a; -_-> \"unknown\";"),
            vec![Ident, Semi, Guard(GuardKind::Default), Str, Semi]
        );
    }

    #[test]
    fn s5_row4_int_guard_after_brace() {
        assert_eq!(
            kinds("{ -42-> f;"),
            vec![LBrace, Guard(GuardKind::Int(42)), Ident, Semi]
        );
    }

    #[test]
    fn s5_row5_spaced_minus_adjacency_fails() {
        // `-7 -> abs;` after `{`: adjacency fails → Minus Int Arrow Ident Semi.
        assert_eq!(
            kinds("{ -7 -> abs;"),
            vec![LBrace, Minus, Int, Arrow, Ident, Semi]
        );
    }

    #[test]
    fn s5_row6_star_context_fails() {
        // `x * -1;` → Ident Star Minus Int Semi (prev Star; also adjacency
        // would fail since `;` follows `1`).
        assert_eq!(kinds("x * -1;"), vec![Ident, Star, Minus, Int, Semi]);
    }

    #[test]
    fn s5_row7_ident_context_fails() {
        // `a-1->c;` → Ident Minus Int Arrow Ident Semi (prev Ident).
        assert_eq!(
            kinds("a-1->c;"),
            vec![Ident, Minus, Int, Arrow, Ident, Semi]
        );
    }

    #[test]
    fn s5_row8_interior_space_adjacency_fails() {
        // `-true ->` after `{` → Minus KwTrue Arrow.
        assert_eq!(kinds("{ -true ->"), vec![LBrace, Minus, KwTrue, Arrow]);
    }

    #[test]
    fn s5_row9_pattern_not_core_discriminant() {
        // `-Some(x)->` after `{` → Minus Ident LParen Ident RParen Arrow.
        assert_eq!(
            kinds("{ -Some(x)->"),
            vec![LBrace, Minus, Ident, LParen, Ident, RParen, Arrow]
        );
    }

    #[test]
    fn s5_row10_float_discriminant_not_core() {
        // `-1.5->` after `{` → Minus Float Arrow.
        assert_eq!(kinds("{ -1.5->"), vec![LBrace, Minus, Float, Arrow]);
    }

    #[test]
    fn s5_guard_at_start_of_input() {
        // Start-of-input context gate passes.
        assert_eq!(
            kinds("-true-> x;"),
            vec![Guard(GuardKind::True), Ident, Semi]
        );
    }

    #[test]
    fn s5_guard_int_value_carried() {
        let out = lex("{ -123-> f;");
        let g = out.tokens.iter().find_map(|t| match t.kind {
            Guard(k) => Some(k),
            _ => None,
        });
        assert_eq!(g, Some(GuardKind::Int(123)));
    }

    // ---- §6 wart ledger rows W1–W9 ---------------------------------------

    #[test]
    fn w1_tight_negation_after_brace() {
        assert_eq!(
            kinds("{ -7->x;"),
            vec![LBrace, Guard(GuardKind::Int(7)), Ident, Semi]
        );
    }

    #[test]
    fn w1_tight_negation_after_semi() {
        assert_eq!(
            kinds("a; -7->x;"),
            vec![Ident, Semi, Guard(GuardKind::Int(7)), Ident, Semi]
        );
    }

    #[test]
    fn w1_tight_negation_after_rbrace() {
        // The `}` context specifically called out in W1.
        assert_eq!(
            kinds("{ } -7->x;"),
            vec![LBrace, RBrace, Guard(GuardKind::Int(7)), Ident, Semi]
        );
    }

    #[test]
    fn w2_backarrow_maximal_munch() {
        // `i<-1` → Ident BackArrow Int.
        assert_eq!(kinds("i<-1"), vec![Ident, BackArrow, Int]);
    }

    #[test]
    fn w3_ret_dot_zero_not_float() {
        assert_eq!(kinds("ret.0"), vec![KwRet, Dot, Int]);
    }

    #[test]
    fn w3_five_dot_is_int_dot() {
        // `5.` → Int Dot (parser error later).
        assert_eq!(kinds("5."), vec![Int, Dot]);
    }

    #[test]
    fn w4_double_dot_two_dots() {
        assert_eq!(kinds(".."), vec![Dot, Dot]);
    }

    #[test]
    fn w4_range_in_expr() {
        // `0..255` (only appears in comments normally) → Int Dot Dot Int.
        assert_eq!(kinds("0..255"), vec![Int, Dot, Dot, Int]);
    }

    #[test]
    fn w5_unicode_outside_strings_is_error() {
        // A lone non-ASCII char → one L0001 Error token.
        assert_eq!(kinds("∘"), vec![Error]);
        assert_eq!(codes("∘"), vec!["L0001"]);
    }

    #[test]
    fn w5_unicode_in_comment_ok() {
        // `∘`, `×`, `—` in a comment are trivia.
        let out = lex("// ∘ × —\nx");
        assert!(out.diagnostics.is_empty());
        let ks: Vec<_> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks, vec![Ident, Eof]);
    }

    #[test]
    fn w5_unicode_in_string_ok() {
        let out = lex("\"∘ × —\"");
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.tokens[0].kind, Str);
    }

    #[test]
    fn w6_crlf_is_trivia() {
        let out = lex("a\r\nb");
        assert!(out.diagnostics.is_empty());
        let ks: Vec<_> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks, vec![Ident, Ident, Eof]);
    }

    #[test]
    fn w7_two_hundred_mhz() {
        // `200MHz` → Int Ident (maximal munch stops at the letter).
        let out = lex("200MHz");
        let ks: Vec<_> = out.tokens.iter().take(2).map(|t| t.kind).collect();
        assert_eq!(ks, vec![Int, Ident]);
        assert_eq!(lexeme("200MHz", &out.tokens[0]), "200");
        assert_eq!(lexeme("200MHz", &out.tokens[1]), "MHz");
    }

    #[test]
    fn w7_underscored_and_hex_and_exp() {
        // `1_000` → Int(1) Ident(_000); `0xFF` → Int(0) Ident(xFF);
        // `1e10` → Int(1) Ident(e10).
        assert_eq!(kinds("1_000"), vec![Int, Ident]);
        assert_eq!(kinds("0xFF"), vec![Int, Ident]);
        assert_eq!(kinds("1e10"), vec![Int, Ident]);
    }

    #[test]
    fn w8_coalesced_unknown_run() {
        // Adjacent unknown chars → one Error token + one L0001.
        let out = lex("$$$");
        let errs: Vec<_> = out.tokens.iter().filter(|t| t.kind == Error).collect();
        assert_eq!(errs.len(), 1);
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code.0, "L0001");
        assert_eq!(lexeme("$$$", errs[0]), "$$$");
    }

    #[test]
    fn w9_empty_input() {
        let out = lex("");
        assert_eq!(out.tokens.len(), 1);
        assert_eq!(out.tokens[0].kind, Eof);
        assert!(out.tokens[0].span.is_empty());
        assert!(out.diagnostics.is_empty());
    }

    // ---- operator munch boundaries ---------------------------------------

    #[test]
    fn munch_lt_variants() {
        assert_eq!(kinds("<"), vec![Lt]);
        assert_eq!(kinds("<="), vec![Le]);
        assert_eq!(kinds("<-"), vec![BackArrow]);
    }

    #[test]
    fn munch_gt_variants() {
        assert_eq!(kinds(">"), vec![Gt]);
        assert_eq!(kinds(">="), vec![Ge]);
    }

    #[test]
    fn munch_eq_bang_amp_pipe() {
        assert_eq!(kinds("=="), vec![EqEq]);
        assert_eq!(kinds("!="), vec![BangEq]);
        assert_eq!(kinds("!"), vec![Bang]);
        assert_eq!(kinds("&&"), vec![AmpAmp]);
        assert_eq!(kinds("||"), vec![PipePipe]);
    }

    #[test]
    fn munch_arrow_vs_minus() {
        // Not in guard context (after Ident).
        assert_eq!(kinds("x->y"), vec![Ident, Arrow, Ident]);
        assert_eq!(kinds("x-y"), vec![Ident, Minus, Ident]);
    }

    #[test]
    fn munch_dotdotdot() {
        assert_eq!(kinds("..."), vec![DotDotDot]);
        assert_eq!(
            kinds("[a, ...b]"),
            vec![LBracket, Ident, Comma, DotDotDot, Ident, RBracket]
        );
    }

    #[test]
    fn munch_colon_colon() {
        // `::` → two Colon (out-of-Core path syntax). `map` is a hard keyword,
        // so `List::map` lexes the tail as KwMap (resolved by the parser).
        assert_eq!(kinds("List::map"), vec![Ident, Colon, Colon, KwMap]);
        assert_eq!(kinds("Foo::bar"), vec![Ident, Colon, Colon, Ident]);
    }

    #[test]
    fn munch_slash_vs_comments() {
        // `/` alone is Slash; `//` is a line comment; `/*` is a block comment.
        assert_eq!(kinds("a / b"), vec![Ident, Slash, Ident]);
        let out = lex("a // comment\nb");
        let ks: Vec<_> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks, vec![Ident, Ident, Eof]);
        assert!(out.diagnostics.is_empty());
    }

    // ---- strings & escapes ----------------------------------------------

    #[test]
    fn string_basic() {
        let out = lex("\"hello\"");
        assert_eq!(out.tokens[0].kind, Str);
        assert!(out.diagnostics.is_empty());
        assert_eq!(lexeme("\"hello\"", &out.tokens[0]), "\"hello\"");
    }

    #[test]
    fn string_known_escapes() {
        let out = lex("\"a\\n\\t\\\\\\\"b\"");
        assert!(out.diagnostics.is_empty());
        assert_eq!(out.tokens[0].kind, Str);
    }

    #[test]
    fn string_unknown_escape_l0003() {
        let out = lex("\"a\\qb\"");
        assert_eq!(out.tokens[0].kind, Str);
        assert_eq!(codes("\"a\\qb\""), vec!["L0003"]);
    }

    #[test]
    fn string_unterminated_eof_l0002() {
        let out = lex("\"abc");
        assert_eq!(out.tokens[0].kind, Str);
        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].code.0, "L0002");
        // Token spans to EOF.
        assert_eq!(out.tokens[0].span.end as usize, "\"abc".len());
    }

    #[test]
    fn string_unterminated_newline_l0002() {
        let out = lex("\"abc\nrest");
        assert_eq!(out.tokens[0].kind, Str);
        assert_eq!(out.diagnostics[0].code.0, "L0002");
        // Token ends at the newline; `rest` lexes as Ident.
        assert_eq!(lexeme("\"abc\nrest", &out.tokens[0]), "\"abc");
        assert_eq!(out.tokens[1].kind, Ident);
    }

    #[test]
    fn unescape_helper() {
        use crate::token::unescape_string;
        assert_eq!(unescape_string("\"a\\nb\""), "a\nb");
        assert_eq!(unescape_string("\"\\t\\\\\\\"\""), "\t\\\"");
        // Unknown escape taken literally.
        assert_eq!(unescape_string("\"\\q\""), "q");
    }

    // ---- floats / ints ---------------------------------------------------

    #[test]
    fn float_basic() {
        assert_eq!(kinds("3.14"), vec![Float]);
        assert_eq!(kinds("0.0"), vec![Float]);
    }

    #[test]
    fn int_basic() {
        assert_eq!(kinds("0"), vec![Int]);
        assert_eq!(kinds("255"), vec![Int]);
        assert_eq!(kinds("007"), vec![Int]); // leading zeros lex as written.
    }

    #[test]
    fn float_needs_digit_after_dot() {
        // `5.` → Int Dot; `5.x` → Int Dot Ident.
        assert_eq!(kinds("5.x"), vec![Int, Dot, Ident]);
    }

    // ---- L-codes ---------------------------------------------------------

    #[test]
    fn l0004_category_with_fix() {
        let out = lex("category Foo");
        assert_eq!(out.tokens[0].kind, KwCategory);
        let d = &out.diagnostics[0];
        assert_eq!(d.code.0, "L0004");
        let fix = d.fix.as_ref().expect("fix present");
        assert_eq!(fix.replacement, "type");
    }

    #[test]
    fn l0005_lone_eq() {
        let out = lex("=");
        assert_eq!(out.tokens[0].kind, Error);
        assert_eq!(out.diagnostics[0].code.0, "L0005");
    }

    #[test]
    fn l0006_lone_amp_pipe() {
        assert_eq!(codes("&"), vec!["L0006"]);
        assert_eq!(codes("|"), vec!["L0006"]);
    }

    #[test]
    fn l0007_block_comment_closed() {
        let out = lex("a /* skip */ b");
        let ks: Vec<_> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks, vec![Ident, Ident, Eof]);
        assert_eq!(out.diagnostics[0].code.0, "L0007");
    }

    #[test]
    fn l0007_block_comment_unterminated() {
        let out = lex("a /* skip");
        let ks: Vec<_> = out.tokens.iter().map(|t| t.kind).collect();
        assert_eq!(ks, vec![Ident, Eof]);
        assert_eq!(out.diagnostics[0].code.0, "L0007");
    }

    #[test]
    fn l0008_int_overflow_clamps() {
        // Way over u64::MAX.
        let src = "99999999999999999999999999";
        let out = lex(src);
        assert_eq!(out.tokens[0].kind, Int);
        assert_eq!(out.diagnostics[0].code.0, "L0008");
    }

    #[test]
    fn l0008_guard_discriminant_overflow_no_panic() {
        // Over-u64 guard discriminant clamps to u64::MAX, no panic (I1), and is
        // flagged with L0008 (DESIGN §2/§5).
        let src = "{ -99999999999999999999-> f;";
        let out = lex(src);
        let g = out.tokens.iter().find_map(|t| match t.kind {
            Guard(k) => Some(k),
            _ => None,
        });
        assert_eq!(g, Some(GuardKind::Int(u64::MAX)));
        assert_eq!(codes(src), vec!["L0008"]);
    }

    #[test]
    fn guard_max_u64_exact_no_diagnostic() {
        // The exact u64::MAX value is valid — clamped representation, but NOT
        // flagged (it did not overflow).
        let src = "{ -18446744073709551615-> f;";
        let out = lex(src);
        let g = out.tokens.iter().find_map(|t| match t.kind {
            Guard(k) => Some(k),
            _ => None,
        });
        assert_eq!(g, Some(GuardKind::Int(u64::MAX)));
        assert!(out.diagnostics.is_empty(), "exact u64::MAX must not flag");
    }

    // ---- invariants ------------------------------------------------------

    #[test]
    fn error_tokens_overlap_a_diagnostic() {
        // I4: every Error token overlaps >= 1 diagnostic span.
        let out = lex("= & | $$$ \x07");
        for t in out.tokens.iter().filter(|t| t.kind == Error) {
            let overlaps = out.diagnostics.iter().any(|d| {
                d.span.start < t.span.end && t.span.start < d.span.end || (d.span == t.span)
            });
            assert!(overlaps, "Error token {t:?} has no overlapping diagnostic");
        }
    }

    /// Re-scan `source[start..end)` and assert it is trivia only — whitespace,
    /// `//` line comments, and `/* */` block comments — mirroring `skip_trivia`.
    /// This is the in-crate twin of the test-support coverage helper; it makes
    /// the unit test below genuinely enforce I3 (not defer it elsewhere).
    fn gap_is_trivia(src: &str, start: usize, end: usize) -> bool {
        let b = src.as_bytes();
        let mut i = start;
        while i < end {
            match b[i] {
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    i += 2;
                    while i < end && b[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    while i < end {
                        if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => return false,
            }
        }
        true
    }

    #[test]
    fn spans_ascending_and_covering() {
        // I2 + I3 on a representative (guard-heavy, commented) source. Every
        // inter-token gap is re-scanned and must be whitespace/comments only, so
        // a dropped non-trivia byte would fail here too — not just in proptest.
        let src =
            "fn f(x: i32) -> i32 { // hdr\n (x > 0) -> { -true-> x; -false-> x * -1; } -> ret; }";
        let out = lex(src);
        let mut prev_end = 0u32;
        let mut covered = 0usize;
        for t in &out.tokens {
            assert!(t.span.start >= prev_end, "non-ascending: {t:?}");
            assert!(t.span.end <= src.len() as u32);
            // I3: the gap before this token is trivia only.
            assert!(
                gap_is_trivia(src, prev_end as usize, t.span.start as usize),
                "non-trivia gap in [{prev_end}..{}): a byte was dropped",
                t.span.start
            );
            prev_end = t.span.end;
            covered += (t.span.end - t.span.start) as usize;
        }
        // Eof is zero-width at end of input, so prev_end now == src.len(): every
        // byte is accounted for as a token span or a (verified-trivia) gap.
        assert_eq!(prev_end as usize, src.len(), "Eof must reach end of input");
        assert!(covered <= src.len());
    }

    // ---- LineIndex -------------------------------------------------------

    #[test]
    fn line_index_basic() {
        let src = "ab\ncde\nf";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_col(0), crate::loc::LineCol { line: 1, col: 1 });
        assert_eq!(idx.line_col(1), crate::loc::LineCol { line: 1, col: 2 });
        // Offset 3 is start of line 2 ('c').
        assert_eq!(idx.line_col(3), crate::loc::LineCol { line: 2, col: 1 });
        assert_eq!(idx.line_col(7), crate::loc::LineCol { line: 3, col: 1 });
    }

    #[test]
    fn line_index_multibyte() {
        // '∘' is 3 bytes; columns must count chars, not bytes.
        let src = "x∘y\nz";
        let idx = LineIndex::new(src);
        // byte 0 = 'x' col 1; byte 1 = start of '∘' col 2; byte 4 = 'y' col 3.
        assert_eq!(idx.line_col(0), crate::loc::LineCol { line: 1, col: 1 });
        assert_eq!(idx.line_col(1), crate::loc::LineCol { line: 1, col: 2 });
        assert_eq!(idx.line_col(4), crate::loc::LineCol { line: 1, col: 3 });
        // After the '\n' (byte 5 is '\n', line 2 starts at byte 6).
        assert_eq!(idx.line_col(6), crate::loc::LineCol { line: 2, col: 1 });
    }

    #[test]
    fn line_index_clamps_past_end() {
        let src = "ab";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_col(999), crate::loc::LineCol { line: 1, col: 3 });
    }

    // ---- examples sanity (zero diagnostics) ------------------------------

    #[test]
    fn examples_zero_diagnostics() {
        for name in ["abs", "fanout", "fir", "pipeline", "sepia", "sum_to_n"] {
            let path = format!("{}/../../examples/{name}.flow", env!("CARGO_MANIFEST_DIR"));
            let src = std::fs::read_to_string(&path).expect("read example");
            let out = lex(&src);
            assert!(
                out.diagnostics.is_empty(),
                "{name}.flow produced diagnostics: {:?}",
                out.diagnostics
            );
            assert!(
                !out.tokens.iter().any(|t| t.kind == Error),
                "{name}.flow produced Error tokens"
            );
            assert_eq!(out.tokens.last().map(|t| t.kind), Some(Eof));
        }
    }
}
