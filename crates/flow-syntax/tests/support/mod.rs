//! Shared test support: render a token stream into the line-oriented form the
//! golden snapshots assert against (DESIGN §9.1).
//!
//! Render format, one token per line:
//!   `{line}:{col} {start}..{end} {Kind} ‹{lexeme}›`
//! The lexeme is omitted for `Eof` (a zero-width token).

#![allow(dead_code)]

use flow_syntax::{GuardKind, LexOutput, LineIndex, Token, TokenKind, lex};

/// Render a token's kind in a compact, stable textual form.
fn kind_str(kind: TokenKind) -> String {
    use TokenKind::*;
    match kind {
        Ident => "Ident".into(),
        Int => "Int".into(),
        Float => "Float".into(),
        Str => "Str".into(),
        KwFn => "KwFn".into(),
        KwType => "KwType".into(),
        KwMut => "KwMut".into(),
        KwLoop => "KwLoop".into(),
        KwRet => "KwRet".into(),
        KwSeq => "KwSeq".into(),
        KwMap => "KwMap".into(),
        KwFold => "KwFold".into(),
        KwTrue => "KwTrue".into(),
        KwFalse => "KwFalse".into(),
        KwCategory => "KwCategory".into(),
        KwExecutor => "KwExecutor".into(),
        KwPub => "KwPub".into(),
        KwUse => "KwUse".into(),
        KwVoid => "KwVoid".into(),
        LParen => "LParen".into(),
        RParen => "RParen".into(),
        LBrace => "LBrace".into(),
        RBrace => "RBrace".into(),
        LBracket => "LBracket".into(),
        RBracket => "RBracket".into(),
        Comma => "Comma".into(),
        Colon => "Colon".into(),
        Semi => "Semi".into(),
        Dot => "Dot".into(),
        DotDotDot => "DotDotDot".into(),
        Plus => "Plus".into(),
        Minus => "Minus".into(),
        Star => "Star".into(),
        Slash => "Slash".into(),
        Percent => "Percent".into(),
        EqEq => "EqEq".into(),
        BangEq => "BangEq".into(),
        Lt => "Lt".into(),
        Gt => "Gt".into(),
        Le => "Le".into(),
        Ge => "Ge".into(),
        AmpAmp => "AmpAmp".into(),
        PipePipe => "PipePipe".into(),
        Bang => "Bang".into(),
        Arrow => "Arrow".into(),
        BackArrow => "BackArrow".into(),
        Guard(GuardKind::True) => "Guard(True)".into(),
        Guard(GuardKind::False) => "Guard(False)".into(),
        Guard(GuardKind::Default) => "Guard(Default)".into(),
        Guard(GuardKind::Int(n)) => format!("Guard(Int {n})"),
        Question => "Question".into(),
        At => "At".into(),
        Error => "Error".into(),
        Eof => "Eof".into(),
    }
}

/// Render one token line.
fn render_token(source: &str, index: &LineIndex, t: &Token) -> String {
    let lc = index.line_col(t.span.start);
    let head = format!(
        "{}:{} {}..{} {}",
        lc.line,
        lc.col,
        t.span.start,
        t.span.end,
        kind_str(t.kind)
    );
    if t.kind == TokenKind::Eof {
        head
    } else {
        let lexeme = &source[t.span.start as usize..t.span.end as usize];
        format!("{head} \u{2039}{lexeme}\u{203a}")
    }
}

/// Render a whole lex output's token stream (one token per line).
pub fn render_tokens(source: &str, out: &LexOutput) -> String {
    let index = LineIndex::new(source);
    out.tokens
        .iter()
        .map(|t| render_token(source, &index, t))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lex `source` and render its token stream.
pub fn render(source: &str) -> String {
    let out = lex(source);
    render_tokens(source, &out)
}

/// Re-scan the byte range `source[start..end)` and assert it is **trivia only**:
/// whitespace (` \t\r\n`), `//` line comments (to but not including `\n`), and
/// `/* ... */` block comments (terminated or running to EOF). This mirrors the
/// lexer's `skip_trivia` grammar exactly (DESIGN §4), so a gap that is genuine
/// trivia passes and a gap containing a *dropped* token byte fails. Returns an
/// `Err(message)` describing the first non-trivia byte; `Ok(())` if all trivia.
///
/// This is the real I3 coverage check: it is the only thing that distinguishes
/// "the gap is whitespace/comments" from "the lexer silently dropped bytes".
fn assert_gap_is_trivia(source: &str, start: usize, end: usize) -> Result<(), String> {
    let bytes = source.as_bytes();
    let mut i = start;
    while i < end {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                // Line comment: skip to (but not past) the next `\n`. The `\n`
                // itself is whitespace trivia, handled by the branch above.
                i += 2;
                while i < end && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                // Block comment: skip to the matching `*/` or to `end` (the
                // unterminated case the lexer also tolerates).
                i += 2;
                while i < end {
                    if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            other => {
                return Err(format!(
                    "I3 violated: gap byte {i} (0x{other:02x}) in [{start}..{end}) is not trivia \
                     (whitespace/`//`/`/* */`) — the lexer dropped a source byte"
                ));
            }
        }
    }
    Ok(())
}

/// Verify invariants I1–I3 on a source and **enforce I3 coverage**: every input
/// byte belongs to exactly one token span *or* a trivia gap, and every trivia
/// gap is re-validated to contain only whitespace/comments (DESIGN §8 I3).
///
/// - I1: last token is `Eof`, zero-width at end of input.
/// - I2: spans strictly ascending, non-overlapping, in-bounds, char-aligned.
/// - I3: the bytes before the first token, between consecutive token spans, and
///       after the last non-`Eof` token (up to `Eof`) are trivia only.
///
/// Panics with a message on violation. Returns the `LexOutput` for further use.
pub fn check_invariants(source: &str) -> LexOutput {
    let out = lex(source);
    assert_invariants(source, &out).unwrap_or_else(|e| panic!("{e}"));
    out
}

/// The invariant body, shared by the fixture helper above and the property
/// tests, so I3 is enforced identically in both places (DESIGN §8/§9).
/// Returns `Err(message)` on the first violation rather than panicking, so the
/// proptest caller can surface it as a shrunk failing case.
pub fn assert_invariants(source: &str, out: &LexOutput) -> Result<(), String> {
    let mut prev_end = 0u32;
    for t in &out.tokens {
        // I2: ascending / non-overlap / in-bounds / char boundary.
        if t.span.start < prev_end {
            return Err(format!(
                "I2 violated: span {:?} starts before previous end {prev_end}",
                t.span
            ));
        }
        if t.span.start > t.span.end {
            return Err(format!("I2: start>end {:?}", t.span));
        }
        if t.span.end as usize > source.len() {
            return Err(format!(
                "I2: span {:?} out of bounds (len {})",
                t.span,
                source.len()
            ));
        }
        if !source.is_char_boundary(t.span.start as usize) {
            return Err(format!("I2: start not on char boundary {:?}", t.span));
        }
        if !source.is_char_boundary(t.span.end as usize) {
            return Err(format!("I2: end not on char boundary {:?}", t.span));
        }
        // I3: the gap `[prev_end, t.span.start)` must be trivia only. For `Eof`
        // (zero-width at source end) this validates the tail after the last
        // real token; for the first token it validates any leading trivia.
        assert_gap_is_trivia(source, prev_end as usize, t.span.start as usize)?;
        prev_end = t.span.end;
    }
    // I1: last token is Eof at end of input.
    let last = out.tokens.last().ok_or("I1: no tokens (expected Eof)")?;
    if last.kind != TokenKind::Eof {
        return Err(format!("I1: last token not Eof: {:?}", last.kind));
    }
    if last.span.start as usize != source.len() {
        return Err(format!(
            "I1: Eof not at end of input ({} != {})",
            last.span.start,
            source.len()
        ));
    }
    Ok(())
}

/// Load an example `.flow` file's source by name (without extension).
pub fn read_example(name: &str) -> String {
    let path = format!("{}/../../examples/{name}.flow", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Load a fixture `.flow` file from `tests/fixtures/`.
pub fn read_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}
