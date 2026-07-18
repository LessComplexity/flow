//! Shared test support: render a token stream into the line-oriented form the
//! golden snapshots assert against (DESIGN §9.1).
//!
//! Render format, one token per line:
//!   `{line}:{col} {start}..{end} {Kind} ‹{lexeme}›`
//! The lexeme is omitted for `Eof` (a zero-width token).

#![allow(dead_code)]

use flow_syntax::{
    ArmPayload, BinOp, Block, BlockItem, Chain, CollOp, Diagnostic, Expr, ExprKind, FanoutKind,
    Field, FieldInit, FnDecl, GuardArm, GuardDiscr, GuardKind, Item, LexOutput, LineIndex,
    LoopLabel, MemberField, Param, Program, SourceLoc, Stage, StageKind, Stmt, StmtKind, Token,
    TokenKind, Ty, TyKind, UnOp, lex,
};

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
fn render_token(source: &str, index: &LineIndex<'_>, t: &Token) -> String {
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
///   after the last non-`Eof` token (up to `Eof`) are trivia only.
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

// ==========================================================================
// Parse-tree renderer (DESIGN §20): one node per line, two-space indent,
// pre-order; every line starts `{line}:{col}` (LineIndex on span.start);
// lexemes/names/literals/operators show their lexeme in U+2039…U+203A quotes;
// stages render as `-> Kind`; arms as `arm ‹-true->›`. Deterministic and
// lexeme-faithful — same review discipline as token snapshots.
// ==========================================================================

struct TreeWriter<'a> {
    source: &'a str,
    index: LineIndex<'a>,
    out: String,
}

impl<'a> TreeWriter<'a> {
    fn new(source: &'a str) -> Self {
        TreeWriter {
            source,
            index: LineIndex::new(source),
            out: String::new(),
        }
    }

    fn lexeme(&self, span: SourceLoc) -> &'a str {
        &self.source[span.start as usize..span.end as usize]
    }

    /// Emit one line: `{line}:{col}` + indent + `label`.
    fn line(&mut self, depth: usize, span: SourceLoc, label: &str) {
        let lc = self.index.line_col(span.start);
        self.out.push_str(&format!("{}:{} ", lc.line, lc.col));
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(label);
        self.out.push('\n');
    }

    /// A lexeme wrapped in U+2039…U+203A quotes.
    fn quoted(&self, span: SourceLoc) -> String {
        format!("\u{2039}{}\u{203a}", self.lexeme(span))
    }

    // --- items ---------------------------------------------------------

    fn program(&mut self, p: &Program) {
        self.line(0, p.span, "Program");
        for item in &p.items {
            self.item(1, item);
        }
    }

    fn item(&mut self, d: usize, item: &Item) {
        match item {
            Item::Fn(f) => self.fn_decl(d, f),
            Item::Type(t) => {
                self.line(d, t.span, &format!("Type {}", self.quoted(t.name.span)));
                for field in &t.fields {
                    self.field(d + 1, field);
                }
            }
            Item::Error(span) => self.line(d, *span, "Item::Error"),
        }
    }

    fn fn_decl(&mut self, d: usize, f: &FnDecl) {
        self.line(d, f.span, &format!("Fn {}", self.quoted(f.name.span)));
        for p in &f.params {
            self.param(d + 1, p);
        }
        if let Some(rt) = &f.ret_ty {
            self.line(d + 1, rt.span, "ret_ty");
            self.ty(d + 2, rt);
        }
        self.block(d + 1, &f.body);
    }

    fn param(&mut self, d: usize, p: &Param) {
        let m = if p.mut_span.is_some() { "mut " } else { "" };
        self.line(
            d,
            p.span,
            &format!("Param {}{}", m, self.quoted(p.name.span)),
        );
        self.ty(d + 1, &p.ty);
    }

    fn field(&mut self, d: usize, f: &Field) {
        self.line(d, f.span, &format!("Field {}", self.quoted(f.name.span)));
        self.ty(d + 1, &f.ty);
    }

    fn ty(&mut self, d: usize, t: &Ty) {
        match &t.kind {
            TyKind::Named(n) => self.line(d, t.span, &format!("Ty::Named {}", self.quoted(n.span))),
            TyKind::Tuple(es) => {
                self.line(d, t.span, "Ty::Tuple");
                for e in es {
                    self.ty(d + 1, e);
                }
            }
            TyKind::Array { elem, len, .. } => {
                self.line(d, t.span, &format!("Ty::Array len={len}"));
                self.ty(d + 1, elem);
            }
            TyKind::Dynamic(elem) => {
                self.line(d, t.span, "Ty::Dynamic");
                self.ty(d + 1, elem);
            }
            TyKind::Error => self.line(d, t.span, "Ty::Error"),
        }
    }

    // --- blocks / statements -------------------------------------------

    fn block(&mut self, d: usize, b: &Block) {
        self.line(d, b.span, "Block");
        for it in &b.items {
            match it {
                BlockItem::Stmt(s) => self.stmt(d + 1, s),
                BlockItem::Arm(a) => self.arm(d + 1, a),
            }
        }
        if let Some(t) = &b.tail {
            self.line(d + 1, t.span, "tail");
            self.chain(d + 2, t);
        }
    }

    fn stmt(&mut self, d: usize, s: &Stmt) {
        match &s.kind {
            StmtKind::Chain(c) => {
                self.line(d, s.span, "Stmt::Chain");
                self.chain(d + 1, c);
            }
            StmtKind::Bind(b) => {
                let m = if b.mut_span.is_some() { "mut " } else { "" };
                let idx = if b.index.is_some() { "[]" } else { "" };
                self.line(
                    d,
                    s.span,
                    &format!("Stmt::Bind {}{}{}", m, self.quoted(b.name.span), idx),
                );
                if let Some(index) = &b.index {
                    self.line(d + 1, index.span, "index");
                    self.expr(d + 2, index);
                }
                if let Some(ty) = &b.ty {
                    self.ty(d + 1, ty);
                }
                self.expr(d + 1, &b.value);
            }
            StmtKind::Loop(l) => {
                let label = match &l.label {
                    LoopLabel::Loop(_) => "loop".to_string(),
                    LoopLabel::Custom(n) => format!("custom {}", self.quoted(n.span)),
                };
                self.line(d, s.span, &format!("Stmt::Loop {label}"));
                self.block(d + 1, &l.body);
            }
            StmtKind::Error => self.line(d, s.span, "Stmt::Error"),
        }
    }

    fn chain(&mut self, d: usize, c: &Chain) {
        self.line(d, c.span, "Chain");
        if let Some(h) = &c.head {
            self.line(d + 1, h.span, "head");
            self.expr(d + 2, h);
        }
        for st in &c.stages {
            self.stage(d + 1, st);
        }
    }

    fn stage(&mut self, d: usize, st: &Stage) {
        match &st.kind {
            StageKind::Expr(e) => {
                self.line(d, st.span, "-> Expr");
                self.expr(d + 1, e);
            }
            StageKind::Bind { mut_span, name, ty } => {
                let m = if mut_span.is_some() { "mut " } else { "" };
                self.line(
                    d,
                    st.span,
                    &format!("-> Bind {}{}", m, self.quoted(name.span)),
                );
                if let Some(ty) = ty {
                    self.ty(d + 1, ty);
                }
            }
            StageKind::Ret { proj } => {
                let p = match proj {
                    Some((v, _)) => format!(".{v}"),
                    None => String::new(),
                };
                self.line(d, st.span, &format!("-> Ret{p}"));
            }
            StageKind::LoopJump => self.line(d, st.span, "-> LoopJump"),
            StageKind::OpShorthand { expr } => {
                self.line(d, st.span, "-> OpShorthand");
                self.expr(d + 1, expr);
            }
            StageKind::Guard(arms) => {
                self.line(d, st.span, "-> Guard");
                for a in arms {
                    self.arm(d + 1, a);
                }
            }
            StageKind::Fanout { kind, branches } => {
                let k = match kind {
                    FanoutKind::Plain => "plain",
                    FanoutKind::Void(_) => "void",
                };
                self.line(d, st.span, &format!("-> Fanout {k} ({})", branches.len()));
                for b in branches {
                    self.chain(d + 1, b);
                }
            }
            StageKind::SeqBlock(b) => {
                self.line(d, st.span, "-> SeqBlock");
                self.block(d + 1, b);
            }
            StageKind::MapFold { op, params, body } => {
                let o = match op {
                    CollOp::Map => "map",
                    CollOp::Fold => "fold",
                };
                let ps: Vec<String> = params.iter().map(|n| self.quoted(n.span)).collect();
                self.line(d, st.span, &format!("-> {o} [{}]", ps.join(", ")));
                self.block(d + 1, body);
            }
            StageKind::StmtBlock(b) => {
                self.line(d, st.span, "-> StmtBlock");
                self.block(d + 1, b);
            }
            StageKind::Error(span) => self.line(d, *span, "-> Error"),
        }
    }

    fn arm(&mut self, d: usize, a: &GuardArm) {
        let disc = match a.discr {
            GuardDiscr::True => "true".to_string(),
            GuardDiscr::False => "false".to_string(),
            GuardDiscr::Default => "_".to_string(),
            GuardDiscr::Int(n) => format!("{n}"),
            GuardDiscr::OutOfCore => "pattern".to_string(),
        };
        // `arm ‹-true->›` form: show the discriminant source lexeme.
        self.line(
            d,
            a.span,
            &format!("arm {} ({})", self.quoted(a.discr_span), disc),
        );
        match &a.payload {
            ArmPayload::Chain(c) => self.chain(d + 1, c),
            ArmPayload::Block(b) => self.block(d + 1, b),
        }
    }

    // --- expressions ---------------------------------------------------

    fn expr(&mut self, d: usize, e: &Expr) {
        match &e.kind {
            ExprKind::Int(v) => self.line(d, e.span, &format!("Int {v} {}", self.quoted(e.span))),
            ExprKind::Float => self.line(d, e.span, &format!("Float {}", self.quoted(e.span))),
            ExprKind::Str => self.line(d, e.span, &format!("Str {}", self.quoted(e.span))),
            ExprKind::Bool(b) => self.line(d, e.span, &format!("Bool {b}")),
            ExprKind::Var(n) => self.line(d, e.span, &format!("Var {}", self.quoted(n.span))),
            ExprKind::Hole => self.line(d, e.span, "Hole"),
            ExprKind::Unary { op, operand, .. } => {
                let o = match op {
                    UnOp::Neg => "Neg",
                    UnOp::Not => "Not",
                };
                self.line(d, e.span, &format!("Unary {o}"));
                self.expr(d + 1, operand);
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                self.line(d, e.span, &format!("Binary {}", binop_str(*op)));
                self.expr(d + 1, lhs);
                self.expr(d + 1, rhs);
            }
            ExprKind::Member { base, field } => {
                let f = match field {
                    MemberField::Named(n) => format!(".{}", self.lexeme(n.span)),
                    MemberField::Index { value, .. } => format!(".{value}"),
                };
                self.line(d, e.span, &format!("Member {f}"));
                self.expr(d + 1, base);
            }
            ExprKind::Index { base, index } => {
                self.line(d, e.span, "Index");
                self.expr(d + 1, base);
                self.expr(d + 1, index);
            }
            ExprKind::Tuple(es) => {
                self.line(d, e.span, &format!("Tuple ({})", es.len()));
                for x in es {
                    self.expr(d + 1, x);
                }
            }
            ExprKind::Array(es) => {
                self.line(d, e.span, &format!("Array ({})", es.len()));
                for x in es {
                    self.expr(d + 1, x);
                }
            }
            ExprKind::Struct { name, fields } => {
                self.line(d, e.span, &format!("Struct {}", self.quoted(name.span)));
                for f in fields {
                    self.field_init(d + 1, f);
                }
            }
            ExprKind::Call { callee, args } => {
                self.line(d, e.span, &format!("Call ({})", args.len()));
                self.expr(d + 1, callee);
                for a in args {
                    self.expr(d + 1, a);
                }
            }
            ExprKind::Question(b) => {
                self.line(d, e.span, "Question");
                self.expr(d + 1, b);
            }
            ExprKind::Error => self.line(d, e.span, "Expr::Error"),
        }
    }

    fn field_init(&mut self, d: usize, f: &FieldInit) {
        match &f.value {
            Some(v) => {
                self.line(
                    d,
                    f.span,
                    &format!("FieldInit {}", self.quoted(f.name.span)),
                );
                self.expr(d + 1, v);
            }
            None => self.line(
                d,
                f.span,
                &format!("FieldInit {} (pun)", self.quoted(f.name.span)),
            ),
        }
    }
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Render a parse tree into the line-oriented snapshot form (DESIGN §20).
pub fn render_tree(source: &str, program: &Program) -> String {
    let mut w = TreeWriter::new(source);
    w.program(program);
    // Trim the trailing newline so snapshots have no blank last line.
    w.out.trim_end().to_string()
}

/// Render a diagnostic list for snapshots: one diagnostic per block, with its
/// code, span, `{line}:{col}`, message, and any fix. Deterministic and
/// rendering-free in the C3 sense (no color/format) — this is test-side
/// presentation of the structured values, not a `Display` impl.
pub fn render_diagnostics(source: &str, diags: &[Diagnostic]) -> String {
    let index = LineIndex::new(source);
    let mut out = String::new();
    for d in diags {
        let lc = index.line_col(d.span.start);
        out.push_str(&format!(
            "{} {}..{} {}:{} {}\n",
            d.code.0, d.span.start, d.span.end, lc.line, lc.col, d.message
        ));
        if let Some(fix) = &d.fix {
            out.push_str(&format!(
                "    fix {}..{} {:?} — {}\n",
                fix.span.start, fix.span.end, fix.replacement, fix.label
            ));
        }
    }
    out.trim_end().to_string()
}
