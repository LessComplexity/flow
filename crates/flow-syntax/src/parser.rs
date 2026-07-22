//! The Flow-Core recursive-descent parser (DESIGN §13–§19).
//!
//! Two tiers per ADR-0005: an expression parser (precedence levels 1–7, §14.6)
//! and a statement-level flow-chain parser (`->`/`<-` over expression operands).
//! A flow is a statement, never an expression value.
//!
//! The parser is a module of `flow-syntax`: [`parse`] runs [`crate::lex`]
//! internally and merges lex+parse diagnostics, stably sorted by
//! `(span.start, span.end)` (§15/§18). It is **total** (J1): a shared depth
//! counter (P0011 at 128) and the §16.1 progress lemma (every `*`-loop iteration
//! consumes ≥1 token or exits) guarantee it never panics or hangs on adversarial
//! input.
//!
//! Out-of-Core v0.2 surface is parsed precisely and rejected with a dedicated
//! P-code (C13); the structure is kept for span-precise downstream messages.
//! Lexer diagnostics are never duplicated (C14): `Error` tokens are skipped, not
//! re-reported.

use crate::ast::*;
use crate::diag::{Diagnostic, SuggestedFix};
use crate::loc::SourceLoc;
use crate::token::{GuardKind, Token, TokenKind};

// Re-export so the §18 public API `pub use parser::{parse, ParseOutput};` is
// satisfiable; `ParseOutput` itself is defined with the other §15 node types in
// `ast.rs`. A glob (`ast::*`) plus this explicit path resolve to one item.
pub use crate::ast::ParseOutput;

/// Parse `source` into a [`ParseOutput`] (DESIGN §18).
///
/// Total function: never panics, never hangs (J1). Lexes internally; the
/// returned diagnostics are the lexer's diagnostics followed by the parser's,
/// stably sorted by `(span.start, span.end)` so lexer and parser reports
/// interleave by position (J6 preserves the lexer diagnostics verbatim).
pub fn parse(source: &str) -> ParseOutput {
    let lexed = crate::lex(source);
    let mut p = Parser::new(source, &lexed.tokens, lexed.diagnostics);
    let program = p.parse_program();
    let mut diagnostics = p.into_diagnostics();
    // Stable sort by (start, end): lexer + parser diagnostics interleave by
    // position; ties keep insertion order (lex before parse at the same span).
    diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
    ParseOutput {
        program,
        diagnostics,
    }
}

/// The shared recursion-depth limit (DESIGN §16.1). Examples nest ≤ 8; exceeding
/// this triggers P0011 and unwinds — the totality guard (J1) against a
/// recursive-descent stack overflow.
const DEPTH_LIMIT: u32 = 128;

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    /// Cursor into `tokens`. `Eof` is guaranteed last, so `cur()` never indexes
    /// out of bounds.
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    /// Shared expression/block recursion depth (P0011 guard).
    depth: u32,
    /// Cooldown (DESIGN §16.1): after a diagnostic, suppress further ones until
    /// the cursor advances. Holds the cursor position at which the last
    /// diagnostic fired; `None` means no cooldown is active.
    cooldown_at: Option<usize>,
    /// Set once a P0011 fires, to avoid a cascade of depth diagnostics while
    /// unwinding.
    depth_exceeded: bool,
    /// Bracket nesting depth for the *current expression* (parens, brackets,
    /// call args, tuples, struct fields, indices). A `<-` is a W2/P0009 expression
    /// defect only when `> 0`; at statement level (`== 0`) a stray `<-` is the
    /// statement's concern (P0008 double-bind / P0001 mixed-direction).
    paren_depth: u32,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: &'a [Token], lex_diagnostics: Vec<Diagnostic>) -> Self {
        debug_assert!(
            tokens.last().map(|t| t.kind) == Some(TokenKind::Eof),
            "token stream must end with Eof"
        );
        Parser {
            source,
            tokens,
            pos: 0,
            diagnostics: lex_diagnostics,
            depth: 0,
            cooldown_at: None,
            depth_exceeded: false,
            paren_depth: 0,
        }
    }

    fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    // --- cursor helpers ---------------------------------------------------

    #[inline]
    fn cur(&self) -> Token {
        // Eof is the last token; clamp defensively.
        self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    #[inline]
    fn kind(&self) -> TokenKind {
        self.cur().kind
    }

    #[inline]
    fn peek_kind(&self, n: usize) -> TokenKind {
        let i = (self.pos + n).min(self.tokens.len() - 1);
        self.tokens[i].kind
    }

    /// The token `n` ahead (clamped to `Eof`).
    #[inline]
    fn peek(&self, n: usize) -> Token {
        let i = (self.pos + n).min(self.tokens.len() - 1);
        self.tokens[i]
    }

    #[inline]
    fn at(&self, k: TokenKind) -> bool {
        self.kind() == k
    }

    #[inline]
    fn at_eof(&self) -> bool {
        self.kind() == TokenKind::Eof
    }

    /// Advance the cursor one token (never past `Eof`). Clears any active
    /// cooldown once real progress is made.
    #[inline]
    fn bump(&mut self) -> Token {
        let t = self.cur();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    /// Consume `k` if present, returning its span; otherwise `None`.
    fn eat(&mut self, k: TokenKind) -> Option<SourceLoc> {
        if self.at(k) {
            Some(self.bump().span)
        } else {
            None
        }
    }

    /// The span of the current token (for placing "expected" diagnostics).
    #[inline]
    fn cur_span(&self) -> SourceLoc {
        self.cur().span
    }

    /// Lexeme text for a span.
    #[inline]
    fn text(&self, span: SourceLoc) -> &'a str {
        &self.source[span.start as usize..span.end as usize]
    }

    /// A zero-width span at the **consumed-input boundary** (`prev_end`) — the
    /// position where a recovery placeholder (missing name/type/expr) is located.
    /// Using `prev_end` rather than the current (unconsumed) token guarantees a
    /// placeholder never overshoots into a token belonging to the next
    /// production, so it always sits at or before any enclosing node's end. The
    /// node-span builders (`node_span`, `node_span_covering`, and the explicit
    /// `min`/`max` widenings) take the union with children, so a placeholder that
    /// lands *before* a node's captured start is still nested (J2).
    #[inline]
    fn here(&self) -> SourceLoc {
        SourceLoc::empty_at(self.prev_end())
    }

    /// Close a node that began at `start`, ending at the consumed boundary
    /// (`prev_end`). When the node consumed nothing (a pure-recovery node where
    /// the cursor never advanced past `start`), `prev_end < start`; collapse both
    /// ends to `start` so the span is a well-formed zero-width region exactly at
    /// the node's entry point, where its `here()` recovery children also sit (J2).
    fn node_span(&self, start: u32) -> SourceLoc {
        let end = self.prev_end();
        if end < start {
            SourceLoc::empty_at(start)
        } else {
            self.span(start, end)
        }
    }

    /// Close a node spanning `[start, prev_end]` **widened** to also cover an
    /// already-built child span (used when a synthesized/recovery child may lie
    /// outside the consumed range, e.g. a missing `{` body block placed at the
    /// current token, or an op-shorthand `Hole` lhs before the operator).
    /// Guarantees `child ⊆ result` (J2).
    fn node_span_covering(&self, start: u32, child: SourceLoc) -> SourceLoc {
        let end = self.prev_end();
        let lo = start.min(child.start);
        let hi = end.max(child.end).max(lo);
        self.span(lo, hi)
    }

    /// The covering span of a block's body (items + tail), for widening a block
    /// span so every body node nests within it (J2). Returns `None` for an empty
    /// body.
    fn body_extent(items: &[BlockItem], tail: &Option<Chain>) -> Option<SourceLoc> {
        let mut acc: Option<SourceLoc> = None;
        let mut widen = |s: SourceLoc| {
            acc = Some(match acc {
                None => s,
                Some(a) => SourceLoc::new(a.start.min(s.start), a.end.max(s.end)),
            });
        };
        for it in items {
            match it {
                BlockItem::Stmt(s) => widen(s.span),
                BlockItem::Arm(a) => widen(a.span),
            }
        }
        if let Some(t) = tail {
            widen(t.span);
        }
        acc
    }

    // --- diagnostics (cooldown) -------------------------------------------

    /// Whether a new diagnostic may fire (cooldown released, DESIGN §16.1):
    /// after a diagnostic, none more until the cursor advances ≥1 token.
    fn cooldown_ok(&self) -> bool {
        match self.cooldown_at {
            None => true,
            Some(at) => self.pos > at,
        }
    }

    /// Emit a diagnostic under the cooldown rule; no-op if on cooldown.
    fn diag(&mut self, code: &'static str, span: SourceLoc, message: impl Into<String>) {
        if !self.cooldown_ok() {
            return;
        }
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
        self.cooldown_at = Some(self.pos);
    }

    /// Emit a diagnostic with a suggested fix under the cooldown rule.
    fn diag_fix(
        &mut self,
        code: &'static str,
        span: SourceLoc,
        message: impl Into<String>,
        fix: SuggestedFix,
    ) {
        if !self.cooldown_ok() {
            return;
        }
        self.diagnostics
            .push(Diagnostic::error(code, span, message).with_fix(fix));
        self.cooldown_at = Some(self.pos);
    }

    // --- span helper with J2 debug-assert ---------------------------------

    /// Build a node span `[start, end)` and assert it is well-formed (J2). The
    /// child-within-parent checks live at the use sites (each constructor asserts
    /// its children's spans are within the span it builds).
    fn span(&self, start: u32, end: u32) -> SourceLoc {
        debug_assert!(start <= end, "node span start>end: {start}..{end}");
        debug_assert!(
            end as usize <= self.source.len(),
            "node span out of bounds: {start}..{end} (len {})",
            self.source.len()
        );
        SourceLoc::new(start, end)
    }

    /// The end offset of the most recently consumed token (for closing a node
    /// span at the right boundary). Falls back to the current token start.
    fn prev_end(&self) -> u32 {
        if self.pos == 0 {
            self.cur_span().start
        } else {
            self.tokens[self.pos - 1].span.end
        }
    }

    // --- depth guard ------------------------------------------------------

    /// Enter a recursion level; returns `false` (and emits P0011 once) if the
    /// shared depth limit is exceeded (DESIGN §16.1, J1).
    fn enter(&mut self) -> bool {
        self.depth += 1;
        if self.depth > DEPTH_LIMIT {
            if !self.depth_exceeded {
                let span = self.cur_span();
                self.diag(
                    "P0011",
                    span,
                    "nesting too deep (over 128 levels); parsing stopped here to stay total",
                );
                self.depth_exceeded = true;
            }
            false
        } else {
            true
        }
    }

    #[inline]
    fn leave(&mut self) {
        self.depth -= 1;
        if self.depth <= DEPTH_LIMIT {
            self.depth_exceeded = false;
        }
    }

    // ======================================================================
    // §14.1 — program / items
    // ======================================================================

    fn parse_program(&mut self) -> Program {
        let start = self.cur_span().start;
        let mut items = Vec::new();
        while !self.at_eof() {
            let before = self.pos;
            if let Some(item) = self.parse_item() {
                items.push(item);
            }
            // Progress lemma (DESIGN §16.1): every item* iteration consumes ≥1
            // token or exits. If nothing was consumed, skip exactly one token
            // under cooldown to guarantee termination (J1).
            if self.pos == before && !self.at_eof() {
                let span = self.cur_span();
                self.diag("P0001", span, "unexpected token at top level");
                self.bump();
            }
            debug_assert!(
                self.pos > before || self.at_eof(),
                "item* failed to make progress at {before}"
            );
        }
        // Cover the items: a trailing recovery node (e.g. a degenerate fn whose
        // body reached EOF) may extend to `source.len()`, past the last consumed
        // token's end (J2).
        let mut span = self.node_span(start);
        for item in &items {
            let s = match item {
                Item::Fn(f) => f.span,
                Item::Type(t) => t.span,
                Item::Error(e) => *e,
            };
            span = SourceLoc::new(span.start.min(s.start), span.end.max(s.end));
        }
        Program { items, span }
    }

    /// `item := fn-decl | type-decl | @-annotation item | executor … | pub/use …
    ///        | statement` (DESIGN §14.1).
    fn parse_item(&mut self) -> Option<Item> {
        match self.kind() {
            TokenKind::KwFn => Some(Item::Fn(self.parse_fn_decl())),
            TokenKind::KwType | TokenKind::KwCategory => {
                // KwCategory: parse as `type`, NO new diagnostic (L0004 emitted).
                Some(Item::Type(self.parse_type_decl()))
            }
            TokenKind::At => self.parse_annotated_item(),
            TokenKind::KwExecutor => Some(self.parse_executor_decl()),
            TokenKind::KwPub | TokenKind::KwUse => Some(self.parse_pub_use()),
            TokenKind::Error => {
                // Lexer-diagnosed token: skip silently (C14).
                self.bump();
                None
            }
            _ => {
                // ⟦P0012⟧ statement at top level — parse for spans, store Error.
                Some(self.parse_top_level_statement())
            }
        }
    }

    /// ⟦P0102⟧ `'@' IDENT ( '(' …balanced… ')' )? item` — the annotation tokens
    /// are skipped, the annotated item is kept (DESIGN §14.1).
    fn parse_annotated_item(&mut self) -> Option<Item> {
        let at_span = self.bump().span; // `@`
        self.diag(
            "P0102",
            at_span,
            "annotations (`@…`) are out of Flow-Core (HANDOFF §4); planned for Core+1",
        );
        // Skip the annotation name and an optional balanced (...) group. The
        // name may be an identifier OR a keyword (`@executor(…)`), so accept both.
        if self.at(TokenKind::Ident) || is_keyword(self.kind()) {
            self.bump();
        }
        if self.at(TokenKind::LParen) {
            self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
        }
        // The annotated item follows; if it is another annotation, recurse.
        if self.at_eof() {
            return None;
        }
        self.parse_item()
    }

    /// ⟦P0111⟧ `'executor' IDENT '{' …balanced… '}'` — skipped (DESIGN §14.1).
    fn parse_executor_decl(&mut self) -> Item {
        let start = self.cur_span().start;
        let kw_span = self.bump().span; // `executor`
        self.diag(
            "P0111",
            kw_span,
            "`executor` declarations are out of Flow-Core (HANDOFF §4); planned for post-M5",
        );
        if self.at(TokenKind::Ident) {
            self.bump();
        }
        if self.at(TokenKind::LBrace) {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
        }
        let end = self.prev_end();
        Item::Error(self.span(start, end))
    }

    /// ⟦P0112⟧ `('pub' | 'use') …to ';' or item start…` — skipped (DESIGN §14.1).
    fn parse_pub_use(&mut self) -> Item {
        let start = self.cur_span().start;
        let kw_span = self.bump().span; // `pub` / `use`
        let what = self.text(kw_span);
        self.diag(
            "P0112",
            kw_span,
            format!("`{what}` is out of Flow-Core (HANDOFF §4); planned for post-M5"),
        );
        // Skip to the next `;` (consume) or item-start keyword / EOF (leave).
        loop {
            match self.kind() {
                TokenKind::Semi => {
                    self.bump();
                    break;
                }
                TokenKind::Eof
                | TokenKind::KwFn
                | TokenKind::KwType
                | TokenKind::KwCategory
                | TokenKind::KwExecutor
                | TokenKind::At => break,
                _ => {
                    self.bump();
                }
            }
        }
        Item::Error(self.node_span(start))
    }

    /// `fn-decl := 'fn' IDENT '(' params? ')' ( '->' type )? block`.
    fn parse_fn_decl(&mut self) -> FnDecl {
        let start = self.cur_span().start;
        self.bump(); // `fn`
        let name = self.expect_name("function name");
        let params = if self.at(TokenKind::LParen) {
            self.parse_params()
        } else {
            self.diag("P0001", self.cur_span(), "expected `(` after function name");
            Vec::new()
        };
        let ret_ty = if self.at(TokenKind::Arrow) {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let body = if self.at(TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.diag("P0001", self.cur_span(), "expected `{` for function body");
            self.empty_block()
        };
        let span = self.node_span_covering(start, body.span);
        FnDecl {
            name,
            params,
            ret_ty,
            body,
            span,
        }
    }

    /// `params := param (',' param)* ','?` — caller has checked the `(`.
    fn parse_params(&mut self) -> Vec<Param> {
        self.bump(); // `(`
        let mut params = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            params.push(self.parse_param());
            if self.at(TokenKind::Comma) {
                self.bump();
            } else if !self.at(TokenKind::RParen) {
                // Not a comma and not the close: stop the list.
                if self.pos == before {
                    // Avoid an infinite loop on a token a param can't start.
                    break;
                }
                break;
            }
            if self.pos == before {
                break;
            }
        }
        if self.eat(TokenKind::RParen).is_none() {
            self.diag("P0001", self.cur_span(), "expected `)` to close parameters");
        }
        params
    }

    fn parse_param(&mut self) -> Param {
        let start = self.cur_span().start;
        let mut_span = self.eat(TokenKind::KwMut);
        let name = self.expect_name("parameter name");
        let ty = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_type()
        } else {
            self.diag(
                "P0001",
                self.cur_span(),
                "expected `:` and a parameter type",
            );
            self.error_ty()
        };
        let span = self.node_span_covering(start, ty.span);
        Param {
            mut_span,
            name,
            ty,
            span,
        }
    }

    /// `type-decl := ('type'|category) IDENT '{' field-list? '}'`.
    fn parse_type_decl(&mut self) -> TypeDecl {
        let start = self.cur_span().start;
        self.bump(); // `type` or `category` (L0004 already emitted; no new diag)
        let name = self.expect_name("type name");
        if self.at(TokenKind::LBrace) {
            self.bump();
        } else {
            self.diag("P0001", self.cur_span(), "expected `{` for type body");
        }
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            let before = self.pos;
            if self.at(TokenKind::Minus) {
                // ⟦P0105⟧ enum/coproduct variant: skip, decl kept.
                self.parse_enum_variant();
            } else {
                fields.push(self.parse_field());
            }
            if self.at(TokenKind::Comma) {
                self.bump();
            }
            if self.pos == before {
                // Progress lemma: skip one token we can't classify.
                self.diag("P0001", self.cur_span(), "expected a field or `}`");
                self.bump();
            }
        }
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag("P0002", self.cur_span(), "unclosed `{` in type body");
        }
        TypeDecl {
            name,
            fields,
            span: self.node_span(start),
        }
    }

    fn parse_field(&mut self) -> Field {
        let start = self.cur_span().start;
        let name = self.expect_name("field name");
        let ty = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_type()
        } else {
            self.diag("P0001", self.cur_span(), "expected `:` and a field type");
            self.error_ty()
        };
        let span = self.node_span_covering(start, ty.span);
        Field { name, ty, span }
    }

    /// ⟦P0105⟧ `'-' IDENT ( '{' …balanced… '}' | '(' …balanced… ')' )` — an enum
    /// variant in a type body; the variant is skipped, the decl kept.
    fn parse_enum_variant(&mut self) {
        let minus = self.bump().span; // `-`
        let mut end = minus.end;
        if self.at(TokenKind::Ident) {
            end = self.bump().span.end;
        }
        if self.at(TokenKind::LBrace) {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            end = self.prev_end();
        } else if self.at(TokenKind::LParen) {
            self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
            end = self.prev_end();
        }
        self.diag(
            "P0105",
            self.span(minus.start, end),
            "enum/coproduct variants in a `type` body are out of Flow-Core (HANDOFF §4); \
             planned for Core+1 coproducts",
        );
    }

    // ======================================================================
    // §14.2 — types
    // ======================================================================

    /// Parse a type, guarded by the shared recursion-depth limit (DESIGN §16.1,
    /// J1). Type positions nest via `[`/`(`; deeply nested `[[[…` or `(…,…)`
    /// would otherwise recurse unboundedly and overflow the stack. On exceed,
    /// emit P0011 once and return a `TyKind::Error` placeholder, unwinding like
    /// the expression/block guard.
    fn parse_type(&mut self) -> Ty {
        if !self.enter() {
            self.leave();
            return self.error_ty();
        }
        let ty = self.parse_type_inner();
        self.leave();
        ty
    }

    fn parse_type_inner(&mut self) -> Ty {
        let start = self.cur_span().start;
        match self.kind() {
            TokenKind::Ident => {
                let name = Name {
                    span: self.bump().span,
                };
                if self.at(TokenKind::Lt) {
                    // ⟦P0103⟧ generic args — keep the base name.
                    self.skip_balanced(TokenKind::Lt, TokenKind::Gt);
                    let end = self.prev_end();
                    self.diag(
                        "P0103",
                        self.span(start, end),
                        "generic type arguments are out of Flow-Core (HANDOFF §4); \
                         planned for Core+1",
                    );
                    Ty {
                        kind: TyKind::Named(name),
                        span: self.span(start, end),
                    }
                } else {
                    Ty {
                        kind: TyKind::Named(name),
                        span: name.span,
                    }
                }
            }
            TokenKind::LBracket => self.parse_array_type(),
            TokenKind::LParen => self.parse_tuple_type(),
            _ => {
                self.diag("P0001", self.cur_span(), "expected a type");
                self.error_ty()
            }
        }
    }

    /// `'[' type ';' INT ']'` (fixed array) or ⟦P0104⟧ `'[' type ']'` (dynamic).
    fn parse_array_type(&mut self) -> Ty {
        let start = self.cur_span().start;
        self.bump(); // `[`
        let elem = self.parse_type();
        if self.at(TokenKind::Semi) {
            self.bump();
            // Length must be an INT literal.
            let (len, len_span) = if self.at(TokenKind::Int) {
                let span = self.bump().span;
                (self.parse_int(span), span)
            } else {
                self.diag(
                    "P0001",
                    self.cur_span(),
                    "array length must be an integer literal",
                );
                (0, self.here())
            };
            if self.eat(TokenKind::RBracket).is_none() {
                self.diag("P0002", self.cur_span(), "unclosed `[` in array type");
            }
            let span = self.node_span_covering(start, elem.span);
            Ty {
                kind: TyKind::Array {
                    elem: Box::new(elem),
                    len,
                    len_span,
                },
                span,
            }
        } else {
            // ⟦P0104⟧ dynamic array / slice — kept.
            if self.eat(TokenKind::RBracket).is_none() {
                self.diag("P0002", self.cur_span(), "unclosed `[` in array type");
            }
            let span = self.node_span_covering(start, elem.span);
            self.diag(
                "P0104",
                span,
                "dynamic array / slice types (`[T]`) are out of Flow-Core (HANDOFF §4); \
                 planned for Core+1",
            );
            Ty {
                kind: TyKind::Dynamic(Box::new(elem)),
                span,
            }
        }
    }

    /// `'(' type (',' type)+ ','? ')'` — a tuple type, ≥2 elements.
    fn parse_tuple_type(&mut self) -> Ty {
        let start = self.cur_span().start;
        self.bump(); // `(`
        let mut elems = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            elems.push(self.parse_type());
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        if self.eat(TokenKind::RParen).is_none() {
            self.diag("P0002", self.cur_span(), "unclosed `(` in tuple type");
        }
        // Cover the elements (a recovery element may sit outside the consumed
        // range, J2).
        let mut span = self.node_span(start);
        for e in &elems {
            span = SourceLoc::new(span.start.min(e.span.start), span.end.max(e.span.end));
        }
        if elems.len() < 2 {
            self.diag("P0001", span, "a tuple type needs at least two elements");
        }
        Ty {
            kind: TyKind::Tuple(elems),
            span,
        }
    }

    fn error_ty(&self) -> Ty {
        Ty {
            kind: TyKind::Error,
            span: self.here(),
        }
    }

    // ======================================================================
    // §14.3 — blocks, statements, chains
    // ======================================================================

    fn empty_block(&self) -> Block {
        Block {
            items: Vec::new(),
            tail: None,
            span: self.here(),
        }
    }

    /// `block := '{' block-item* tail? '}'` — caller has checked the `{`.
    fn parse_block(&mut self) -> Block {
        let start = self.cur_span().start;
        if !self.enter() {
            // Depth guard: synthesize an empty block and skip the brace group.
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return Block {
                items: Vec::new(),
                tail: None,
                span: self.node_span(start),
            };
        }
        self.bump(); // `{`
        // A non-stage block (fn body, loop body, payload block): guard arms are
        // not legitimate here, so a leading guard is stray → P0004.
        let (items, tail) = self.parse_block_body(false);
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag("P0002", self.span(start, start + 1), "unclosed `{`");
        }
        self.leave();
        let span = self.block_span(start, &items, &tail);
        Block { items, tail, span }
    }

    /// A block span: `[start, prev_end]` widened to cover its body (J2).
    fn block_span(&self, start: u32, items: &[BlockItem], tail: &Option<Chain>) -> SourceLoc {
        let base = self.node_span(start);
        match Self::body_extent(items, tail) {
            None => base,
            Some(ext) => self.span(base.start.min(ext.start), base.end.max(ext.end)),
        }
    }

    /// Parse a block's items and optional tail (the content between the braces).
    /// Mixing guard arms with non-arm statements draws P0006 (§14.3).
    ///
    /// `guard_ok` is true only for a stage-position block (`-> { … }`), where
    /// guard arms are legitimate (§14.4). Elsewhere a leading guard token is a
    /// **stray** guard → P0004 (W1), recovered as a statement.
    fn parse_block_body(&mut self, guard_ok: bool) -> (Vec<BlockItem>, Option<Chain>) {
        let mut items: Vec<BlockItem> = Vec::new();
        let mut tail: Option<Chain> = None;
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            let before = self.pos;
            // ⟦P0001⟧ empty statement.
            if self.at(TokenKind::Semi) {
                let span = self.bump().span;
                self.diag("P0001", span, "empty statement (`;`) — removed");
                continue;
            }
            let class = self.classify_block_item();
            // A clean `Guard` token in a non-guard block is a *stray* guard (W1)
            // → P0004, recovered as a statement so the chain still parses.
            if !guard_ok && matches!(self.kind(), TokenKind::Guard(_)) {
                match self.parse_stray_guard_statement() {
                    StmtOrTail::Stmt(stmt) => items.push(BlockItem::Stmt(stmt)),
                    StmtOrTail::Tail(chain) => {
                        tail = Some(chain);
                        break;
                    }
                }
                if self.pos == before {
                    self.diag("P0001", self.cur_span(), "unexpected token in block");
                    self.bump();
                }
                continue;
            }
            match class {
                BlockItemClass::Arm => {
                    let arm = self.parse_guard_arm();
                    items.push(BlockItem::Arm(arm));
                }
                BlockItemClass::Statement => {
                    // A bare-expression chain ending at `}` is the block tail.
                    match self.parse_statement_or_tail() {
                        StmtOrTail::Stmt(stmt) => items.push(BlockItem::Stmt(stmt)),
                        StmtOrTail::Tail(chain) => {
                            tail = Some(chain);
                            // Tail must be the last thing before `}`.
                            break;
                        }
                    }
                }
            }
            if self.pos == before {
                // Progress lemma: skip one token under cooldown.
                let span = self.cur_span();
                self.diag("P0001", span, "unexpected token in block");
                self.bump();
            }
            debug_assert!(
                self.pos > before || self.at(TokenKind::RBrace) || self.at_eof(),
                "block-item* failed to make progress"
            );
        }
        // §14.3 mixing: if both arms and non-arm statements are present, P0006.
        self.check_arm_mixing(&items);
        (items, tail)
    }

    /// ⟦P0004⟧ stray guard arrow outside a guard block (W1). The lexer produced a
    /// clean `Guard` token because the context gate passed, but no guard block
    /// surrounds it. Emit P0004 with a fix built by **slicing the source lexeme**
    /// — `&source[span]` minus the trailing `->`, plus `" ->"` — never re-rendered
    /// from the clamped `GuardKind::Int` (which would rewrite an over-`u64`
    /// literal). Recover: an Int discriminant becomes a chain head
    /// `Unary(Neg, Int)` followed by an implicit arrow stage; other discriminants
    /// become an `Expr::Error` head.
    fn parse_stray_guard_statement(&mut self) -> StmtOrTail {
        let start = self.cur_span().start;
        let TokenKind::Guard(gk) = self.kind() else {
            unreachable!("caller checked for a Guard token");
        };
        let guard_span = self.bump().span; // the `-D->` lexeme
        // Build the fix by slicing the source: drop the trailing `->`, add ` ->`.
        let lexeme = self.text(guard_span);
        let trimmed = lexeme.strip_suffix("->").unwrap_or(lexeme);
        let replacement = format!("{trimmed} ->");
        self.diag_fix(
            "P0004",
            guard_span,
            "stray guard arrow outside a guard block; `-7-> x` reads as a guard arm. \
             If you meant a negated value flowing to a target, add a space: `-7 -> x`",
            SuggestedFix {
                span: guard_span,
                replacement,
                label: "add a space inside the would-be guard arrow",
            },
        );
        // Recover the head expression.
        let head = match gk {
            GuardKind::Int(n) => {
                let operand = Expr {
                    kind: ExprKind::Int(n),
                    span: guard_span,
                };
                Expr {
                    kind: ExprKind::Unary {
                        op: UnOp::Neg,
                        op_span: guard_span,
                        operand: Box::new(operand),
                    },
                    span: guard_span,
                }
            }
            _ => Expr {
                kind: ExprKind::Error,
                span: guard_span,
            },
        };
        // The implicit arrow stage uses the guard lexeme's span as the arrow span;
        // the stage span starts at the guard lexeme (the implicit `->`) and is
        // widened to cover an inner expression placeholder (J2).
        let mut stages = Vec::new();
        let stage_kind = self.parse_stage_body();
        let mut stage_span = self.node_span(guard_span.start);
        if let Some(ext) = stage_kind_extent(&stage_kind) {
            stage_span =
                SourceLoc::new(stage_span.start.min(ext.start), stage_span.end.max(ext.end));
        }
        stages.push(Stage {
            arrow_span: guard_span,
            kind: stage_kind,
            span: stage_span,
        });
        // Continue any further `-> …` stages.
        while self.at(TokenKind::Arrow) {
            let before = self.pos;
            stages.push(self.parse_stage());
            if self.pos == before {
                break;
            }
        }
        // Cover the head and last stage (J2).
        let mut chain_span = self.node_span(start);
        chain_span = SourceLoc::new(
            chain_span.start.min(head.span.start),
            chain_span.end.max(head.span.end),
        );
        if let Some(s) = stages.last() {
            chain_span = SourceLoc::new(
                chain_span.start.min(s.span.start),
                chain_span.end.max(s.span.end),
            );
        }
        let chain = Chain {
            head: Some(head),
            stages,
            span: chain_span,
        };
        self.finish_chain(start, chain)
    }

    /// Detect arm/non-arm mixing and emit P0006 with the bidirectional hint for
    /// the `Minus Int Arrow` statement case (DESIGN §14.3 / §16).
    fn check_arm_mixing(&mut self, items: &[BlockItem]) {
        let has_arm = items.iter().any(|i| matches!(i, BlockItem::Arm(_)));
        if !has_arm {
            return;
        }
        for item in items {
            if let BlockItem::Stmt(stmt) = item {
                // A statement among arms — P0006 (bidirectional hint when it is a
                // negated-int flow, which the user may have meant as a guard arm).
                // Cooldown is irrelevant here (we report at a settled position);
                // push directly so the mixing is always surfaced.
                let msg = "statement mixed with guard arms in the same block; \
                           a guard block contains only arms. \
                           If you meant a guard arm, remove the space: `-7->`";
                self.diagnostics
                    .push(Diagnostic::error("P0006", stmt.span, msg));
            }
        }
    }

    /// Classify the block-item at the cursor: guard arm vs statement
    /// (DESIGN §14.3 leading-`Minus` disambiguation rules 1–3).
    fn classify_block_item(&self) -> BlockItemClass {
        // A clean Guard token is always an arm.
        if matches!(self.kind(), TokenKind::Guard(_)) {
            return BlockItemClass::Arm;
        }
        if self.at(TokenKind::Minus) {
            // Rule 1: `Minus (true|false|Ident‹_›) Arrow` (any spacing) → P0005 arm.
            if self.is_spaced_bool_or_default_arm() {
                return BlockItemClass::Arm;
            }
            // Rule 2: `Minus` span-adjacent to Ident≠_ or `[`, balanced group,
            // span-adjacent `Arrow` → P0106 pattern arm.
            if self.is_pattern_arm() {
                return BlockItemClass::Arm;
            }
            // Rule 3: otherwise an expression statement.
        }
        BlockItemClass::Statement
    }

    /// Rule 1 lookahead: `Minus (KwTrue|KwFalse|Ident‹_›) Arrow`, any spacing.
    fn is_spaced_bool_or_default_arm(&self) -> bool {
        debug_assert!(self.at(TokenKind::Minus));
        let k1 = self.peek_kind(1);
        let is_disc = match k1 {
            TokenKind::KwTrue | TokenKind::KwFalse => true,
            TokenKind::Ident => self.text(self.peek(1).span) == "_",
            _ => false,
        };
        is_disc && self.peek_kind(2) == TokenKind::Arrow
    }

    /// Rule 2 lookahead: `Minus` span-adjacent to `Ident‹≠_›` or `[`, followed by
    /// a balanced `(…)`/`[…]` group (possibly empty), followed by a span-adjacent
    /// `Arrow` → pattern arm (`-Some(x)->`, `-None->`, `-[]->`, `-[h, ...t]->`).
    fn is_pattern_arm(&self) -> bool {
        debug_assert!(self.at(TokenKind::Minus));
        let minus = self.cur().span;
        let head = self.peek(1);
        // Span-adjacency of `-` and the head.
        if minus.end != head.span.start {
            return false;
        }
        let is_head = match head.kind {
            TokenKind::Ident => self.text(head.span) != "_",
            TokenKind::LBracket => true,
            _ => false,
        };
        if !is_head {
            return false;
        }
        // Walk a balanced group of (...) and [...] from after the head, then
        // require a span-adjacent `->`. Bounded scan over the token slice.
        let mut i = self.pos + 1; // index of `head`
        let mut after = i + 1; // first token after the head
        // The head may be `[` (start of a group) or an Ident (then a group may
        // follow). Group brackets: `(` `)` and `[` `]`.
        if head.kind == TokenKind::LBracket {
            // The head itself opens a `[...]` group.
            after = self.scan_balanced_group(i);
            if after == usize::MAX {
                return false;
            }
        } else {
            // Ident head: an optional adjacent `(...)` or `[...]` group.
            i = after;
            if i < self.tokens.len()
                && matches!(self.tokens[i].kind, TokenKind::LParen | TokenKind::LBracket)
            {
                after = self.scan_balanced_group(i);
                if after == usize::MAX {
                    return false;
                }
            } else {
                after = i;
            }
        }
        // Now require an `Arrow` at `after`, span-adjacent to the prior token.
        if after >= self.tokens.len() {
            return false;
        }
        let arrow = self.tokens[after];
        if arrow.kind != TokenKind::Arrow {
            return false;
        }
        let prev_end = self.tokens[after - 1].span.end;
        arrow.span.start == prev_end
    }

    /// Scan a balanced bracket group starting at token index `open` (which must
    /// be `(` or `[`). Returns the index of the first token *after* the matching
    /// close, or `usize::MAX` if unbalanced before EOF.
    fn scan_balanced_group(&self, open: usize) -> usize {
        let open_kind = self.tokens[open].kind;
        let close_kind = match open_kind {
            TokenKind::LParen => TokenKind::RParen,
            TokenKind::LBracket => TokenKind::RBracket,
            TokenKind::LBrace => TokenKind::RBrace,
            _ => return usize::MAX,
        };
        let mut depth = 0i32;
        let mut i = open;
        while i < self.tokens.len() {
            let k = self.tokens[i].kind;
            if k == TokenKind::Eof {
                return usize::MAX;
            }
            if k == open_kind {
                depth += 1;
            } else if k == close_kind {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            i += 1;
        }
        usize::MAX
    }

    /// Parse a statement, or recognize that a bare-expression chain ending at `}`
    /// is the block's tail (DESIGN §14.3 termination rule).
    fn parse_statement_or_tail(&mut self) -> StmtOrTail {
        let start = self.cur_span().start;
        match self.kind() {
            TokenKind::KwLoop => StmtOrTail::Stmt(self.parse_loop_stmt()),
            // ⟦P0110⟧ statement-initial labeled block `:label { … }` (ADR-0012).
            // Recognized on the `Colon` with one token of lookahead; a second
            // `Colon` (a `::` path) is NOT a label, so require `Colon Ident LBrace`.
            TokenKind::Colon
                if self.peek_kind(1) == TokenKind::Ident
                    && self.peek_kind(2) == TokenKind::LBrace =>
            {
                StmtOrTail::Stmt(self.parse_sigiled_labeled_block())
            }
            TokenKind::KwVoid => {
                // ⟦P0113⟧ statement-initial `void` (with or without block).
                StmtOrTail::Stmt(self.parse_void_stmt())
            }
            // Statement-initial keyword stages that must follow `->` (W22).
            TokenKind::KwSeq | TokenKind::KwMap | TokenKind::KwFold => {
                let span = self.cur_span();
                let kw = self.text(span);
                self.diag(
                    "P0001",
                    span,
                    format!("`{kw}` must follow `->` (it is a stage, not a statement head)"),
                );
                // Recover: skip the keyword and its block if present.
                self.bump();
                if self.at(TokenKind::LBrace) {
                    self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
                }
                StmtOrTail::Stmt(Stmt {
                    kind: StmtKind::Error,
                    span: self.node_span(start),
                })
            }
            _ => self.parse_chain_or_bind_stmt(),
        }
    }

    /// Parse a `loop`-introduced or `Ident {`-labeled loop / a chain / a bind.
    fn parse_chain_or_bind_stmt(&mut self) -> StmtOrTail {
        // §14.5: statement-initial `Ident {` four-token scan (ADR-0011).
        if self.at(TokenKind::Ident)
            && self.peek_kind(1) == TokenKind::LBrace
            && self.ident_brace_is_loop()
        {
            return StmtOrTail::Stmt(self.parse_labeled_loop());
        }
        // bind-stmt: `mut`? IDENT (':' type)? '<-' expr.
        if self.looks_like_bind() {
            return StmtOrTail::Stmt(self.parse_bind_stmt());
        }
        // Otherwise a chain (statement or tail).
        self.parse_chain_statement()
    }

    /// §14.5 scan: from the `{` to its matching `}` (depth count), any `Semi`,
    /// `Arrow`, `BackArrow`, or `Guard` (at any depth) ⇒ labeled loop (ADR-0011).
    fn ident_brace_is_loop(&self) -> bool {
        // Find the `{` (it is at pos+1).
        let open = self.pos + 1;
        debug_assert_eq!(self.tokens[open].kind, TokenKind::LBrace);
        let mut depth = 0i32;
        let mut i = open;
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokenKind::Semi | TokenKind::Arrow | TokenKind::BackArrow | TokenKind::Guard(_) => {
                    return true;
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// ⟦P0110⟧ statement-initial labeled block `:label { … }` (ADR-0012). The
    /// `Custom` label `Name` span covers **just the identifier** (so its text is a
    /// valid name); the `:` sigil is folded into the diagnostic span and the
    /// `LoopStmt` span (which begins at the `Colon`). Parsed as a loop for
    /// recovery; the body goes through `parse_block` (depth guard, J1/J2).
    fn parse_sigiled_labeled_block(&mut self) -> Stmt {
        let start = self.cur_span().start; // the `:`
        let colon = self.bump().span; // `:`
        let name = Name {
            span: self.bump().span, // IDENT
        };
        // Diagnostic span covers `:label` (sigil + ident).
        let sigil_span = self.span(colon.start, name.span.end);
        self.diag(
            "P0110",
            sigil_span,
            "labeled blocks (`:label { … }`, ADR-0012) are out of Flow-Core (HANDOFF §4); \
             Flow-Core's only loop introducer is `loop`. Planned for Core+1",
        );
        let body = self.parse_block();
        let span = self.node_span_covering(start, body.span);
        Stmt {
            kind: StmtKind::Loop(LoopStmt {
                label: LoopLabel::Custom(name),
                body,
                span,
            }),
            span,
        }
    }

    /// ⟦P0110⟧ recovery heuristic (demoted ADR-0011 scan, §14.5): a
    /// statement-initial `IDENT block` whose braces are loop-shaped (contain
    /// `Semi`/`Arrow`/`BackArrow`/`Guard`). Under ADR-0012 un-sigiled `Ident {`
    /// is *always* a struct literal grammatically; this path survives only to
    /// improve the error — it parses the body as a labeled block and points at
    /// the sigiled spelling.
    fn parse_labeled_loop(&mut self) -> Stmt {
        let start = self.cur_span().start;
        let name = Name {
            span: self.bump().span,
        };
        let ident = self.text(name.span).to_string();
        self.diag(
            "P0110",
            name.span,
            format!(
                "labeled blocks are written `:{ident} {{ … }}` (ADR-0012) and are out of \
                 Flow-Core (HANDOFF §4); Flow-Core's only loop introducer is `loop`. \
                 Planned for Core+1"
            ),
        );
        let body = self.parse_block();
        let span = self.node_span_covering(start, body.span);
        Stmt {
            kind: StmtKind::Loop(LoopStmt {
                label: LoopLabel::Custom(name),
                body,
                span,
            }),
            span,
        }
    }

    /// `loop-stmt := 'loop' block`.
    fn parse_loop_stmt(&mut self) -> Stmt {
        let start = self.cur_span().start;
        let kw = self.bump().span; // `loop`
        let body = if self.at(TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.diag("P0001", self.cur_span(), "expected `{` for loop body");
            self.empty_block()
        };
        let span = self.node_span_covering(start, body.span);
        Stmt {
            kind: StmtKind::Loop(LoopStmt {
                label: LoopLabel::Loop(kw),
                body,
                span,
            }),
            span,
        }
    }

    /// ⟦P0113⟧ statement-initial `void` — both `void { … }` (fanout) and a bare
    /// `void;` keyword. Bare ⇒ skip + `Stmt::Error`.
    fn parse_void_stmt(&mut self) -> Stmt {
        let start = self.cur_span().start;
        let kw = self.bump().span; // `void`
        if self.at(TokenKind::LBrace) {
            // void { … } — parse as a fanout-shaped block stage for recovery.
            self.diag(
                "P0113",
                kw,
                "`void` is out of Flow-Core (HANDOFF §4); planned for Core+1",
            );
            let _block = self.parse_block();
            // Consume the trailing `;` if present (statement form).
            self.eat(TokenKind::Semi);
            Stmt {
                kind: StmtKind::Error,
                span: self.node_span(start),
            }
        } else {
            // bare `void;` — skip the keyword (and a trailing `;`), Stmt::Error.
            self.diag(
                "P0113",
                kw,
                "`void` is out of Flow-Core (HANDOFF §4); planned for Core+1",
            );
            self.eat(TokenKind::Semi);
            Stmt {
                kind: StmtKind::Error,
                span: self.node_span(start),
            }
        }
    }

    /// Heuristic: does the cursor start a `bind-stmt`? `mut`? IDENT (':' type)?
    /// '<-' — we look for a `<-` before any `->`/`;`/`{`.
    fn looks_like_bind(&self) -> bool {
        let mut i = self.pos;
        if self.tokens[i].kind == TokenKind::KwMut {
            i += 1;
        }
        if self.tokens.get(i).map(|t| t.kind) != Some(TokenKind::Ident) {
            return false;
        }
        i += 1;
        // Optional `: type` — scan until `<-`, `->`, `;`, `{`, or EOF, but a
        // BackArrow before any of those means a bind.
        let mut depth = 0i32;
        while i < self.tokens.len() {
            match self.tokens[i].kind {
                TokenKind::BackArrow if depth == 0 => return true,
                TokenKind::Arrow | TokenKind::Semi if depth == 0 => return false,
                TokenKind::LBrace if depth == 0 => return false,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::Lt => depth += 1,
                TokenKind::RParen | TokenKind::RBracket | TokenKind::Gt => {
                    depth = (depth - 1).max(0);
                }
                TokenKind::Eof | TokenKind::RBrace => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// `bind-stmt := 'mut'? IDENT ('[' expr ']')? (':' type)? '<-' expr` — a
    /// second `<-` in one statement draws P0008 (DESIGN §14.3 / §16). The
    /// optional `[' expr ']` is the element-update sugar `c[i] <- x` (ADR-0021):
    /// it excludes `mut` (P0013) and a type annotation (P0014); a nested target
    /// `c[i][j]` draws P0015.
    fn parse_bind_stmt(&mut self) -> Stmt {
        let start = self.cur_span().start;
        let mut_span = self.eat(TokenKind::KwMut);
        let name = self.expect_name("binding name");
        // Optional element-update index `[' expr ']` (ADR-0021 §2).
        let index = if self.at(TokenKind::LBracket) {
            self.bump(); // `[`
            self.paren_depth += 1;
            let idx = self.parse_expr();
            self.paren_depth -= 1;
            if self.eat(TokenKind::RBracket).is_none() {
                self.diag("P0002", self.cur_span(), "unclosed `[` in update target");
            }
            // A nested index target `c[i][j] <- x` is out of this increment
            // (ADR-0021 §2, one-dimensional update only): P0015, extra
            // `[' … ']` groups consumed and dropped so the recovered node is a
            // clean one-dimensional indexed bind.
            while self.at(TokenKind::LBracket) {
                let open = self.cur_span();
                self.bump(); // `[`
                self.paren_depth += 1;
                let _ = self.parse_expr();
                self.paren_depth -= 1;
                let close = self
                    .eat(TokenKind::RBracket)
                    .unwrap_or_else(|| self.cur_span());
                self.diag(
                    "P0015",
                    self.span(open.start, close.end),
                    "nested element-update target `c[i][j] <- x` is out of Flow-Core \
                     (ADR-0021: one-dimensional update only); planned for a later increment",
                );
            }
            // `mut c[i] <- x` — the index form is a rebind, not a fresh
            // declaration, so `mut` is meaningless (ADR-0021 §2): P0013.
            if let Some(m) = mut_span {
                self.diag(
                    "P0013",
                    m,
                    "`mut` is not allowed on an element-update target (`c[i] <- x` is a \
                     rebind, not a new binding)",
                );
            }
            Some(idx)
        } else {
            None
        };
        let ty = if self.at(TokenKind::Colon) {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        // `c[i]: T <- x` — an element-update target takes no type annotation
        // (ADR-0021 §2; the slot type is already fixed by the array): P0014.
        if let (Some(_), Some(t)) = (&index, &ty) {
            self.diag(
                "P0014",
                t.span,
                "an element-update target (`c[i] <- x`) takes no type annotation",
            );
        }
        // Consume `<-`.
        if self.eat(TokenKind::BackArrow).is_none() {
            self.diag("P0001", self.cur_span(), "expected `<-` in binding");
        }
        let value = self.parse_expr();
        // A second `<-` in the same statement → P0008 (W17), consume to `;`.
        if self.at(TokenKind::BackArrow) {
            let span = self.cur_span();
            self.diag(
                "P0008",
                span,
                "only one `<-` binding per statement (W17); chained `<-` is not Flow-Core",
            );
            while !self.at(TokenKind::Semi) && !self.at(TokenKind::RBrace) && !self.at_eof() {
                self.bump();
            }
        }
        // Statement terminator `;`.
        self.expect_semi("binding statement");
        // Cover the bound value (and its type/index) so recovery placeholders nest (J2).
        let mut span = self.node_span_covering(start, value.span);
        if let Some(t) = &ty {
            span = SourceLoc::new(span.start.min(t.span.start), span.end.max(t.span.end));
        }
        if let Some(i) = &index {
            span = SourceLoc::new(span.start.min(i.span.start), span.end.max(i.span.end));
        }
        Stmt {
            kind: StmtKind::Bind(BindStmt {
                mut_span,
                name,
                index,
                ty,
                value,
                span,
            }),
            span,
        }
    }

    // ======================================================================
    // chains
    // ======================================================================

    /// Parse a chain at statement position, then apply the §14.3 termination
    /// rule: `;` ⇒ statement; `}` ⇒ tail; block-final stage ⇒ statement w/o `;`;
    /// else P0001.
    fn parse_chain_statement(&mut self) -> StmtOrTail {
        let start = self.cur_span().start;
        let chain = self.parse_chain();
        self.finish_chain(start, chain)
    }

    /// Apply the termination rule to a freshly-parsed chain.
    fn finish_chain(&mut self, start: u32, chain: Chain) -> StmtOrTail {
        let bare_expr = chain.head.is_some() && chain.stages.is_empty();
        let last_is_block = chain
            .stages
            .last()
            .map(|s| stage_is_block_form(&s.kind))
            .unwrap_or(false);

        if self.at(TokenKind::Semi) {
            self.bump();
            if bare_expr {
                // A bare-expression chain as a `;`-terminated statement → P0003.
                self.diag(
                    "P0003",
                    chain.span,
                    "a bare expression is not a statement; flow it (`-> target`) or make it \
                     the block tail",
                );
            }
            let span = self.node_span_covering(start, chain.span);
            StmtOrTail::Stmt(Stmt {
                kind: StmtKind::Chain(chain),
                span,
            })
        } else if self.at(TokenKind::RBrace) || self.at_eof() {
            // Chain is the block's tail (a bare expr or an unterminated chain).
            StmtOrTail::Tail(chain)
        } else if last_is_block {
            // Block-final stage: `;` optional (W10). Also consume a stray `;`
            // handled above; here we have no `;`.
            self.eat(TokenKind::Semi);
            let span = self.node_span_covering(start, chain.span);
            StmtOrTail::Stmt(Stmt {
                kind: StmtKind::Chain(chain),
                span,
            })
        } else {
            // Missing `;`.
            self.diag("P0001", self.cur_span(), "expected `;` after statement");
            self.sync_to_stmt_end();
            let span = self.node_span_covering(start, chain.span);
            StmtOrTail::Stmt(Stmt {
                kind: StmtKind::Chain(chain),
                span,
            })
        }
    }

    /// `chain := expr stage* | stage+` (headed | headless).
    fn parse_chain(&mut self) -> Chain {
        let start = self.cur_span().start;
        let head = if self.at(TokenKind::Arrow) {
            // Headless chain (e.g. `-> loop;`, fanout branches).
            None
        } else {
            Some(self.parse_expr())
        };
        let mut stages = Vec::new();
        while self.at(TokenKind::Arrow) {
            let before = self.pos;
            stages.push(self.parse_stage());
            if self.pos == before {
                break;
            }
        }
        // A mixed-direction chain (`a -> b <- c`) → P0001 (W18).
        if self.at(TokenKind::BackArrow) && (!stages.is_empty() || head.is_some()) {
            let span = self.cur_span();
            self.diag(
                "P0001",
                span,
                "`<-` cannot follow a `->` stage (mixed-direction chain, W18)",
            );
        }
        // Cover the head and the last stage so a recovery placeholder that lands
        // just outside the consumed range still nests within the chain (J2).
        let mut span = self.node_span(start);
        if let Some(h) = &head {
            span = SourceLoc::new(span.start.min(h.span.start), span.end.max(h.span.end));
        }
        if let Some(s) = stages.last() {
            span = SourceLoc::new(span.start.min(s.span.start), span.end.max(s.span.end));
        }
        Chain { head, stages, span }
    }

    /// `stage := '->' stage-body` — caller has checked the `->`.
    fn parse_stage(&mut self) -> Stage {
        let arrow_span = self.bump().span; // `->`
        let start = arrow_span.start;
        let kind = self.parse_stage_body();
        // Cover the stage's children so a recovery placeholder that lands just
        // outside the consumed range still nests within the stage (J2).
        let mut span = self.node_span(start);
        if let Some(ext) = stage_kind_extent(&kind) {
            span = SourceLoc::new(span.start.min(ext.start), span.end.max(ext.end));
        }
        Stage {
            arrow_span,
            kind,
            span,
        }
    }

    fn parse_stage_body(&mut self) -> StageKind {
        match self.kind() {
            TokenKind::KwRet => {
                let _kw = self.bump().span;
                // Optional `.INT` projection.
                let proj = if self.at(TokenKind::Dot) && self.peek_kind(1) == TokenKind::Int {
                    self.bump(); // `.`
                    let span = self.bump().span; // INT
                    Some((self.parse_int(span), span))
                } else {
                    None
                };
                StageKind::Ret { proj }
            }
            TokenKind::KwLoop => {
                self.bump();
                StageKind::LoopJump
            }
            TokenKind::KwSeq => {
                // ADR-0019: `seq { … }` is a statement block in stage position —
                // the ordinary block production, not a fanout. Guard arms stay
                // illegal in it (a clean guard token → stray-guard P0004).
                self.bump(); // `seq`
                let body = if self.at(TokenKind::LBrace) {
                    self.parse_block()
                } else {
                    self.diag("P0001", self.cur_span(), "expected `{` for seq block");
                    self.empty_block()
                };
                StageKind::SeqBlock(body)
            }
            TokenKind::KwVoid => {
                // ⟦P0113⟧ `-> void { … }`.
                let kw = self.bump().span;
                self.diag(
                    "P0113",
                    kw,
                    "`void` is out of Flow-Core (HANDOFF §4); planned for Core+1",
                );
                let branches = self.parse_fanout_block();
                StageKind::Fanout {
                    kind: FanoutKind::Void(kw),
                    branches,
                }
            }
            TokenKind::KwMap => {
                let _kw = self.bump().span;
                self.parse_map_fold(CollOp::Map)
            }
            TokenKind::KwFold => {
                let _kw = self.bump().span;
                self.parse_map_fold(CollOp::Fold)
            }
            TokenKind::KwMut => {
                // `-> mut y` / `-> mut y: T` — mut binding stage.
                let mut_span = self.bump().span;
                let name = self.expect_name("binding name");
                let ty = if self.at(TokenKind::Colon) {
                    self.bump();
                    Some(self.parse_type())
                } else {
                    None
                };
                StageKind::Bind {
                    mut_span: Some(mut_span),
                    name,
                    ty,
                }
            }
            TokenKind::LBrace => {
                // §14.4 block-stage classification.
                self.parse_block_stage()
            }
            // op-shorthand: a leading binary operator starts a hole-expression.
            k if is_op_shorthand_lead(k) => self.parse_op_shorthand(),
            // `-> IDENT : T` typed binding stage, OR a general expression stage,
            // OR an out-of-Core `Ident {` collection operator.
            TokenKind::Ident => self.parse_ident_stage(),
            // ⟦P0110⟧ labeled jump `-> :label` (ADR-0012). Recognized on the
            // `Colon` followed by an `Ident`; a `::` path (`Colon Colon`) is NOT a
            // label and falls through to the general expression stage below.
            TokenKind::Colon if self.peek_kind(1) == TokenKind::Ident => {
                let colon = self.bump().span; // `:`
                let name_span = self.bump().span; // IDENT
                let span = self.span(colon.start, name_span.end);
                self.diag(
                    "P0110",
                    span,
                    "labeled jumps (`-> :label`, ADR-0012) are out of Flow-Core (HANDOFF §4); \
                     Flow-Core's only back-edge jump is `-> loop`. Planned for Core+1",
                );
                StageKind::Error(span)
            }
            _ => {
                // General expression stage (ADR-0005: a -> b + c -> d).
                let expr = self.parse_expr();
                StageKind::Expr(expr)
            }
        }
    }

    /// `-> IDENT …`: a typed binding stage (`-> x: i32`), an out-of-Core
    /// collection operator (`-> filter { … }`, P0114), or a general expression
    /// stage (member access etc.).
    fn parse_ident_stage(&mut self) -> StageKind {
        // Out-of-Core collection operator: `Ident {` with op-block prefix.
        if self.peek_kind(1) == TokenKind::LBrace && self.ident_brace_is_op_block() {
            let name_span = self.cur_span();
            let op = self.text(name_span).to_string();
            self.bump(); // the Ident
            self.diag(
                "P0114",
                name_span,
                format!(
                    "`{op}` is out of Flow-Core (HANDOFF §4): only `map`/`fold` are Core \
                     collection operators. Planned for Core+1"
                ),
            );
            let (params, body) = self.parse_op_block();
            return StageKind::MapFold {
                op: CollOp::Map,
                params,
                body,
            };
        }
        // Typed binding stage: `IDENT : type` (a lone Ident followed by a SINGLE
        // `:`). A span-adjacent `::` is an out-of-Core path (P0109), handled as a
        // general expression stage below, so exclude it here.
        if self.peek_kind(1) == TokenKind::Colon && !self.next_is_path_colons() {
            let name = Name {
                span: self.bump().span,
            };
            self.bump(); // `:`
            let ty = self.parse_type();
            return StageKind::Bind {
                mut_span: None,
                name,
                ty: Some(ty),
            };
        }
        // General expression stage (targets like `abs`, `print`, member access).
        let expr = self.parse_expr();
        StageKind::Expr(expr)
    }

    /// Whether the next two tokens are a span-adjacent `::` (a path, P0109) — so
    /// the `IDENT :` typed-binding rule must not claim the first colon.
    fn next_is_path_colons(&self) -> bool {
        self.peek_kind(1) == TokenKind::Colon
            && self.peek_kind(2) == TokenKind::Colon
            && self.peek(1).span.end == self.peek(2).span.start
    }

    /// op-block sniff: `Ident { IDENT (',' IDENT)* '->' …` (DESIGN §14.4).
    fn ident_brace_is_op_block(&self) -> bool {
        let open = self.pos + 1; // the `{`
        debug_assert_eq!(self.tokens[open].kind, TokenKind::LBrace);
        let mut i = open + 1;
        // At least one Ident.
        if self.tokens.get(i).map(|t| t.kind) != Some(TokenKind::Ident) {
            return false;
        }
        i += 1;
        while self.tokens.get(i).map(|t| t.kind) == Some(TokenKind::Comma) {
            i += 1;
            if self.tokens.get(i).map(|t| t.kind) != Some(TokenKind::Ident) {
                return false;
            }
            i += 1;
        }
        self.tokens.get(i).map(|t| t.kind) == Some(TokenKind::Arrow)
    }

    /// `('map' | 'fold') op-block` — caller consumed the keyword.
    fn parse_map_fold(&mut self, op: CollOp) -> StageKind {
        if !self.at(TokenKind::LBrace) {
            self.diag(
                "P0001",
                self.cur_span(),
                "expected `{` op-block after `map`/`fold`",
            );
            return StageKind::MapFold {
                op,
                params: Vec::new(),
                body: self.empty_block(),
            };
        }
        let (params, body) = self.parse_op_block();
        StageKind::MapFold { op, params, body }
    }

    /// `op-block := '{' IDENT (',' IDENT)* '->' block-item* tail? '}'`
    /// (DESIGN §14.3). A `(` pattern parameter → ⟦P0116⟧.
    fn parse_op_block(&mut self) -> (Vec<Name>, Block) {
        let start = self.cur_span().start;
        if !self.enter() {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return (Vec::new(), self.empty_block());
        }
        self.bump(); // `{`
        let mut params = Vec::new();
        // Parse params: plain idents only; `(` pattern → P0116. §14.3 grammar
        // `IDENT (',' IDENT)*` permits NO trailing comma and requires ≥1 param.
        let mut saw_param = false; // an Ident or a `(` destructuring pattern
        let mut trailing_comma: Option<SourceLoc> = None;
        loop {
            match self.kind() {
                TokenKind::Ident => {
                    saw_param = true;
                    trailing_comma = None;
                    params.push(Name {
                        span: self.bump().span,
                    });
                }
                TokenKind::LParen => {
                    // ⟦P0116⟧ destructuring op-block parameter.
                    saw_param = true;
                    trailing_comma = None;
                    let span = self.cur_span();
                    self.diag(
                        "P0116",
                        span,
                        "destructuring op-block parameters (`(x, y)`) are out of Flow-Core \
                         (HANDOFF §4); planned for Core+1",
                    );
                    self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
                }
                _ => break,
            }
            if self.at(TokenKind::Comma) {
                trailing_comma = Some(self.cur_span());
                self.bump();
            } else {
                break;
            }
        }
        // §14.3: a trailing comma before `->` is rejected; the params parsed so
        // far are kept so recovery continues (the `->` parse below still runs).
        if let Some(comma_span) = trailing_comma {
            self.diag(
                "P0001",
                comma_span,
                "op-block parameters take no trailing comma before `->`",
            );
        }
        // §14.3: an op-block needs at least one parameter (`{ -> … }` is invalid).
        if !saw_param {
            self.diag(
                "P0001",
                self.cur_span(),
                "op-block needs at least one parameter before `->`",
            );
        }
        if self.eat(TokenKind::Arrow).is_none() {
            self.diag(
                "P0001",
                self.cur_span(),
                "expected `->` after op-block parameters",
            );
        }
        // An op-block body is not a guard block: stray guards → P0004.
        let (items, tail) = self.parse_block_body(false);
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag(
                "P0002",
                self.span(start, start + 1),
                "unclosed op-block `{`",
            );
        }
        self.leave();
        let span = self.block_span(start, &items, &tail);
        (params, Block { items, tail, span })
    }

    /// `op-shorthand := BINOP hole-expr` — a leading binary operator after `->`.
    /// The piped value is the hole (leftmost leaf); parse by the precedence
    /// climber with a synthetic `Hole` lhs at the operator's level (DESIGN §14.3).
    fn parse_op_shorthand(&mut self) -> StageKind {
        let op_tok = self.cur();
        let op = binop_of(op_tok.kind).expect("op-shorthand lead is a binop");
        let level = binop_level(op);
        let start = op_tok.span.start;
        let hole = Expr {
            kind: ExprKind::Hole,
            span: SourceLoc::empty_at(start),
        };
        // Climb from `hole` at the operator's level.
        let expr = self.parse_binop_rhs(hole, level);
        StageKind::OpShorthand { expr }
    }

    // ======================================================================
    // §14.4 — block-stage classification
    // ======================================================================

    /// A `{` in stage position parses as a generic block, then classifies
    /// (DESIGN §14.4): GuardBlock / Fanout / StmtBlock.
    fn parse_block_stage(&mut self) -> StageKind {
        // Empty `{}` → Fanout(0) + P0010.
        if self.peek_kind(1) == TokenKind::RBrace {
            let start = self.cur_span().start;
            self.bump(); // `{`
            self.bump(); // `}`
            let end = self.prev_end();
            self.diag(
                "P0010",
                self.span(start, end),
                "empty block `{}` in stage position — classified as a zero-branch fanout",
            );
            return StageKind::Fanout {
                kind: FanoutKind::Plain,
                branches: Vec::new(),
            };
        }
        // Look at the first block-item: is it an arm? If so it's a GuardBlock.
        // We parse the block generically and classify by content.
        self.parse_classified_block()
    }

    /// Parse a stage-position block and classify it (DESIGN §14.4 table).
    fn parse_classified_block(&mut self) -> StageKind {
        let start = self.cur_span().start;
        if !self.enter() {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return StageKind::Error(self.node_span(start));
        }
        self.bump(); // `{`
        // Stage-position block (`-> { … }`): guard arms ARE legitimate here.
        let (items, tail) = self.parse_block_body(true);
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag("P0002", self.span(start, start + 1), "unclosed `{`");
        }
        self.leave();
        let span = self.block_span(start, &items, &tail);

        let all_arms = !items.is_empty()
            && items.iter().all(|i| matches!(i, BlockItem::Arm(_)))
            && tail.is_none();
        let has_arm = items.iter().any(|i| matches!(i, BlockItem::Arm(_)));

        if all_arms {
            // GuardBlock — arm order preserved.
            let arms = items
                .into_iter()
                .map(|i| match i {
                    BlockItem::Arm(a) => a,
                    BlockItem::Stmt(_) => unreachable!("all_arms guarantees arms"),
                })
                .collect();
            return StageKind::Guard(arms);
        }
        if has_arm {
            // Arms mixed with non-arms: GuardBlock + P0006 (already reported by
            // parse_block_body's check_arm_mixing). Keep the arms, drop the
            // statements into the guard for recovery.
            let arms: Vec<GuardArm> = items
                .into_iter()
                .filter_map(|i| match i {
                    BlockItem::Arm(a) => Some(a),
                    BlockItem::Stmt(_) => None,
                })
                .collect();
            return StageKind::Guard(arms);
        }

        // Fanout if all items are headless chains with no tail; else StmtBlock.
        let all_headless = !items.is_empty()
            && items.iter().all(|i| match i {
                BlockItem::Stmt(Stmt {
                    kind: StmtKind::Chain(c),
                    ..
                }) => c.head.is_none(),
                _ => false,
            })
            && tail.is_none();

        if all_headless {
            let branches = items
                .into_iter()
                .filter_map(|i| match i {
                    BlockItem::Stmt(Stmt {
                        kind: StmtKind::Chain(c),
                        ..
                    }) => Some(c),
                    _ => None,
                })
                .collect();
            StageKind::Fanout {
                kind: FanoutKind::Plain,
                branches,
            }
        } else {
            // ⟦P0115⟧ anonymous block stage.
            self.diag(
                "P0115",
                span,
                "anonymous block stage (`-> { … }` with statements/tail) is out of Flow-Core \
                 (HANDOFF §4); user-guide §8.3 full-language form. Planned for Core+1",
            );
            StageKind::StmtBlock(Block { items, tail, span })
        }
    }

    /// A fanout block payload for the `void` stage: `'{' headless-chains '}'`.
    /// (After ADR-0019 `seq` is a statement block, so `void` is the only caller.)
    /// Non-chain statements (`x <- e` rebinds, `loop`s) do not belong in a fanout
    /// block: each is flagged with **P0117** at its span, replacing the old silent
    /// `filter_map` drop (ADR-0019 defect #3).
    fn parse_fanout_block(&mut self) -> Vec<Chain> {
        if !self.at(TokenKind::LBrace) {
            self.diag("P0001", self.cur_span(), "expected `{` for fanout block");
            return Vec::new();
        }
        let start = self.cur_span().start;
        if !self.enter() {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return Vec::new();
        }
        self.bump(); // `{`
        // A seq/void fanout block holds headless chains, not guard arms.
        let (items, tail) = self.parse_block_body(false);
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag("P0002", self.span(start, start + 1), "unclosed `{`");
        }
        self.leave();
        let mut branches = Vec::new();
        for item in items {
            match item {
                BlockItem::Stmt(Stmt {
                    kind: StmtKind::Chain(c),
                    ..
                }) => branches.push(c),
                // ⟦P0117⟧ a rebind / loop is not a fanout branch — flag, don't drop
                // silently. Cooldown is irrelevant here: the cursor is settled past
                // `}`, so `self.diag`'s cooldown would collapse every drop after the
                // first into silence. Push directly, exactly as `check_arm_mixing`
                // does, so *each* dropped statement is surfaced (ADR-0019 defect #3).
                BlockItem::Stmt(Stmt {
                    kind: StmtKind::Bind(_) | StmtKind::Loop(_),
                    span,
                }) => self.diagnostics.push(Diagnostic::error(
                    "P0117",
                    span,
                    "only chains are fanout branches; `x <- e` / `loop` statements \
                     do not belong in a fanout block",
                )),
                // Error stmts and stray/spaced arms already carry their own
                // diagnostics (P0004/P0005/P0006), so they stay silent here.
                _ => {}
            }
        }
        // The final chain written without a trailing `;` is the block tail — it is a
        // branch too (the `;` is optional), not a silent drop (ADR-0019 defect #3).
        if let Some(c) = tail {
            branches.push(c);
        }
        branches
    }

    // ======================================================================
    // §14.3 — guard arms
    // ======================================================================

    /// `guard-arm := GUARD arm-payload | spaced-arm | pattern-arm`.
    fn parse_guard_arm(&mut self) -> GuardArm {
        let start = self.cur_span().start;
        let (discr, discr_span) = self.parse_guard_discr();
        let payload = self.parse_arm_payload();
        let payload_span = match &payload {
            ArmPayload::Chain(c) => c.span,
            ArmPayload::Block(b) => b.span,
        };
        let span = self.node_span_covering(start, payload_span);
        GuardArm {
            discr,
            discr_span,
            payload,
            span,
        }
    }

    /// Parse the discriminant portion of a guard arm, returning its kind and the
    /// span covering the discriminant lexeme(s) up to and including the `->`.
    fn parse_guard_discr(&mut self) -> (GuardDiscr, SourceLoc) {
        if let TokenKind::Guard(gk) = self.kind() {
            let span = self.bump().span;
            return (guard_discr_of(gk), span);
        }
        // Spaced bool/default arm (rule 1) → P0005, recovered as the meant arm.
        if self.at(TokenKind::Minus) && self.is_spaced_bool_or_default_arm() {
            let start = self.cur_span().start;
            self.bump(); // `-`
            let disc = match self.kind() {
                TokenKind::KwTrue => {
                    self.bump();
                    GuardDiscr::True
                }
                TokenKind::KwFalse => {
                    self.bump();
                    GuardDiscr::False
                }
                _ => {
                    // Ident `_`.
                    self.bump();
                    GuardDiscr::Default
                }
            };
            let arrow = self.eat(TokenKind::Arrow).unwrap_or_else(|| self.here());
            let span = self.span(start, arrow.end);
            // Fix: remove interior whitespace (write `-true->`).
            let lexeme = self.canonical_arm_lexeme(disc);
            self.diag_fix(
                "P0005",
                span,
                "guard arrows are written without interior spaces",
                SuggestedFix {
                    span,
                    replacement: lexeme,
                    label: "remove the spaces inside the guard arrow",
                },
            );
            return (disc, span);
        }
        // Pattern arm (rule 2) → P0106; OutOfCore discr, payload parsed.
        if self.at(TokenKind::Minus) && self.is_pattern_arm() {
            let start = self.cur_span().start;
            self.bump(); // `-`
            // Skip head + optional balanced group up to the `->`.
            if self.at(TokenKind::Ident) {
                self.bump();
            }
            if self.at(TokenKind::LParen) {
                self.skip_balanced(TokenKind::LParen, TokenKind::RParen);
            } else if self.at(TokenKind::LBracket) {
                self.skip_balanced(TokenKind::LBracket, TokenKind::RBracket);
            }
            let arrow = self.eat(TokenKind::Arrow).unwrap_or_else(|| self.here());
            let span = self.span(start, arrow.end);
            self.diag(
                "P0106",
                span,
                "pattern guard arms (`-Some(x)->`, `-[h, ...t]->`) are out of Flow-Core \
                 (HANDOFF §4); planned for Core+1 coproducts",
            );
            return (GuardDiscr::OutOfCore, span);
        }
        // Should not be reached (classify_block_item gates this), but stay total.
        let span = self.cur_span();
        (GuardDiscr::OutOfCore, span)
    }

    /// Build the canonical no-space guard lexeme for a spaced-arm fix.
    fn canonical_arm_lexeme(&self, disc: GuardDiscr) -> String {
        match disc {
            GuardDiscr::True => "-true->".to_string(),
            GuardDiscr::False => "-false->".to_string(),
            GuardDiscr::Default => "-_->".to_string(),
            // Invariant: P0005 (the only caller) fires solely for the spaced
            // bool/default arms, so `disc` is always True/False/Default here;
            // Int/OutOfCore discriminants never reach this fix builder.
            GuardDiscr::Int(_) | GuardDiscr::OutOfCore => {
                unreachable!("canonical_arm_lexeme only builds the P0005 bool/default fix")
            }
        }
    }

    /// `arm-payload := chain (';' per statement rule) | block`.
    fn parse_arm_payload(&mut self) -> ArmPayload {
        // A plain payload block (`-true-> { … }`) is NOT classified (§14.4).
        if self.at(TokenKind::LBrace) {
            let block = self.parse_payload_block();
            ArmPayload::Block(block)
        } else {
            // A chain payload (incl. bare expr `-true-> x;` and headless
            // `-false-> -> ret;`).
            let start = self.cur_span().start;
            let chain = self.parse_chain();
            // Termination: consume `;` if present; a `}` ends the enclosing block.
            self.eat(TokenKind::Semi);
            let _ = start;
            ArmPayload::Chain(chain)
        }
    }

    /// A plain payload block: statements + tail, NOT classified (§14.4).
    fn parse_payload_block(&mut self) -> Block {
        let start = self.cur_span().start;
        if !self.enter() {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return self.empty_block();
        }
        self.bump(); // `{`
        // An arm-payload block is NOT classified (§14.4): it holds statements and
        // a tail, never guard arms — a leading guard here is stray (P0004).
        let (items, tail) = self.parse_block_body(false);
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag("P0002", self.span(start, start + 1), "unclosed `{`");
        }
        self.leave();
        let span = self.block_span(start, &items, &tail);
        // A trailing `;` after the payload block is legal (statement form).
        self.eat(TokenKind::Semi);
        Block { items, tail, span }
    }

    // ======================================================================
    // §14.6 — expressions
    // ======================================================================

    fn parse_expr(&mut self) -> Expr {
        if !self.enter() {
            self.leave();
            return self.error_expr();
        }
        let e = self.parse_or();
        self.leave();
        e
    }

    /// `or := and ( '||' and )*` — level 7.
    fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_and();
        while self.at(TokenKind::PipePipe) {
            let op_span = self.bump().span;
            let rhs = self.parse_and();
            lhs = self.binary(BinOp::Or, op_span, lhs, rhs);
        }
        lhs
    }

    /// `and := cmp ( '&&' cmp )*` — level 6.
    fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_cmp();
        while self.at(TokenKind::AmpAmp) {
            let op_span = self.bump().span;
            let rhs = self.parse_cmp();
            lhs = self.binary(BinOp::And, op_span, lhs, rhs);
        }
        lhs
    }

    /// `cmp := add ( CMPOP add )?` — level 5, NON-ASSOCIATIVE: a second CMPOP →
    /// P0007 (W14).
    fn parse_cmp(&mut self) -> Expr {
        let lhs = self.parse_add();
        // ⟦P0009⟧ a `<-` where a comparison could go *inside* an expression:
        // `(i<-1)` lexes as `(i <- 1)` (W2); the meant comparison is `(i < -1)`.
        // Only fire when nested in a bracket group (`paren_depth > 0`); at
        // statement level a stray `<-` is the statement's concern (P0008/P0001).
        if self.at(TokenKind::BackArrow) && self.paren_depth > 0 {
            let op_span = self.bump().span;
            self.diag_fix(
                "P0009",
                op_span,
                "`<-` cannot appear inside an expression; `i<-1` lexes as `i <- 1`. \
                 For a comparison write `i < -1`",
                SuggestedFix {
                    span: op_span,
                    replacement: "< -".to_string(),
                    label: "did you mean `< -` (comparison with a negative)?",
                },
            );
            let rhs = self.parse_add();
            let neg = Expr {
                kind: ExprKind::Unary {
                    op: UnOp::Neg,
                    op_span,
                    operand: Box::new(rhs.clone()),
                },
                span: self.span(op_span.start, rhs.span.end.max(op_span.end)),
            };
            return self.binary(BinOp::Lt, op_span, lhs, neg);
        }
        if let Some(op) = cmp_op(self.kind()) {
            let op_span = self.bump().span;
            let rhs = self.parse_add();
            let result = self.binary(op, op_span, lhs, rhs);
            // A second comparison at this level is non-associative. Recovery
            // absorbs *all* further `CMPOP add` pairs under the single P0007 so a
            // chain like `a < b < c < d` reports exactly one P0007 with no P0001
            // cascade (DESIGN §17 P0007: one P0007 per chain).
            if cmp_op(self.kind()).is_some() {
                let span = self.cur_span();
                self.diag(
                    "P0007",
                    span,
                    "comparison operators are non-associative; parenthesize (e.g. `(a < b) < c`)",
                );
                // Parse and discard the rest at this level (lhs kept).
                while cmp_op(self.kind()).is_some() {
                    let _op_span = self.bump().span;
                    let _rhs = self.parse_add();
                }
            }
            result
        } else {
            lhs
        }
    }

    /// `add := mul ( ('+'|'-') mul )*` — level 4.
    fn parse_add(&mut self) -> Expr {
        let mut lhs = self.parse_mul();
        loop {
            let op = match self.kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let op_span = self.bump().span;
            let rhs = self.parse_mul();
            lhs = self.binary(op, op_span, lhs, rhs);
        }
        lhs
    }

    /// `mul := unary ( ('*'|'/'|'%') unary )*` — level 3.
    fn parse_mul(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            let op_span = self.bump().span;
            let rhs = self.parse_unary();
            lhs = self.binary(op, op_span, lhs, rhs);
        }
        lhs
    }

    /// `unary := ('-'|'!') unary | postfix` — binds tighter than `*`, looser than
    /// postfix (W15).
    fn parse_unary(&mut self) -> Expr {
        let (op, op_span) = match self.kind() {
            TokenKind::Minus => (UnOp::Neg, self.cur_span()),
            TokenKind::Bang => (UnOp::Not, self.cur_span()),
            _ => return self.parse_postfix(),
        };
        self.bump();
        if !self.enter() {
            self.leave();
            return self.error_expr();
        }
        let operand = self.parse_unary();
        self.leave();
        let span = self.span(op_span.start, operand.span.end.max(op_span.end));
        debug_assert!(
            operand.span.start >= span.start && operand.span.end <= span.end,
            "unary operand span not within parent"
        );
        Expr {
            kind: ExprKind::Unary {
                op,
                op_span,
                operand: Box::new(operand),
            },
            span,
        }
    }

    /// Continue a binary expression climb from an existing `lhs` at `level`. Used
    /// by op-shorthand (lhs = Hole). Implements the §14.6 precedence levels.
    fn parse_binop_rhs(&mut self, lhs: Expr, level: u8) -> Expr {
        match level {
            7 => {
                // `|| and`*
                let mut lhs = lhs;
                while self.at(TokenKind::PipePipe) {
                    let op_span = self.bump().span;
                    let rhs = self.parse_and();
                    lhs = self.binary(BinOp::Or, op_span, lhs, rhs);
                }
                lhs
            }
            6 => {
                let mut lhs = lhs;
                while self.at(TokenKind::AmpAmp) {
                    let op_span = self.bump().span;
                    let rhs = self.parse_cmp();
                    lhs = self.binary(BinOp::And, op_span, lhs, rhs);
                }
                // After consuming `&&` chains, a `||` may still follow.
                self.parse_binop_rhs(lhs, 7)
            }
            5 => {
                // Single comparison from lhs; level 5 is NON-ASSOCIATIVE, so a
                // second `CMPOP` is P0007 (W14), mirroring `parse_cmp`. Recovery
                // absorbs *all* further `CMPOP add` pairs under the single P0007
                // (one P0007 per chain, no P0001 cascade — DESIGN §17 P0007,
                // enforced on both expression paths).
                let lhs = if let Some(op) = cmp_op(self.kind()) {
                    let op_span = self.bump().span;
                    let rhs = self.parse_add();
                    let result = self.binary(op, op_span, lhs, rhs);
                    if cmp_op(self.kind()).is_some() {
                        let span = self.cur_span();
                        self.diag(
                            "P0007",
                            span,
                            "comparison operators are non-associative; parenthesize \
                             (e.g. `(a < b) < c`)",
                        );
                        while cmp_op(self.kind()).is_some() {
                            let _op_span = self.bump().span;
                            let _rhs = self.parse_add();
                        }
                    }
                    result
                } else {
                    lhs
                };
                self.parse_binop_rhs(lhs, 6)
            }
            4 => {
                let mut lhs = lhs;
                loop {
                    let op = match self.kind() {
                        TokenKind::Plus => BinOp::Add,
                        TokenKind::Minus => BinOp::Sub,
                        _ => break,
                    };
                    let op_span = self.bump().span;
                    let rhs = self.parse_mul();
                    lhs = self.binary(op, op_span, lhs, rhs);
                }
                self.parse_binop_rhs(lhs, 5)
            }
            3 => {
                let mut lhs = lhs;
                loop {
                    let op = match self.kind() {
                        TokenKind::Star => BinOp::Mul,
                        TokenKind::Slash => BinOp::Div,
                        TokenKind::Percent => BinOp::Mod,
                        _ => break,
                    };
                    let op_span = self.bump().span;
                    let rhs = self.parse_unary();
                    lhs = self.binary(op, op_span, lhs, rhs);
                }
                self.parse_binop_rhs(lhs, 4)
            }
            _ => lhs,
        }
    }

    /// Build a `Binary` node with J2 span checks.
    fn binary(&self, op: BinOp, op_span: SourceLoc, lhs: Expr, rhs: Expr) -> Expr {
        let start = lhs.span.start;
        let end = rhs.span.end.max(op_span.end);
        debug_assert!(start <= end, "binary span start>end");
        debug_assert!(
            lhs.span.start >= start && lhs.span.end <= end,
            "binary lhs span not within parent"
        );
        debug_assert!(
            rhs.span.start >= start && rhs.span.end <= end,
            "binary rhs span not within parent"
        );
        Expr {
            kind: ExprKind::Binary {
                op,
                op_span,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: self.span(start, end),
        }
    }

    /// `postfix := primary ( '.' IDENT | '.' INT | '[' expr ']' | '(' args ')'
    ///           | '?' )*`.
    fn parse_postfix(&mut self) -> Expr {
        let mut base = self.parse_primary();
        loop {
            match self.kind() {
                TokenKind::Dot => {
                    self.bump();
                    let field = match self.kind() {
                        TokenKind::Ident => MemberField::Named(Name {
                            span: self.bump().span,
                        }),
                        TokenKind::Int => {
                            let span = self.bump().span;
                            MemberField::Index {
                                value: self.parse_int(span),
                                span,
                            }
                        }
                        _ => {
                            self.diag(
                                "P0001",
                                self.cur_span(),
                                "expected a field name or tuple index after `.`",
                            );
                            // Place the recovery name at the consumed boundary so
                            // it stays within the member span (J2).
                            MemberField::Named(Name {
                                span: SourceLoc::empty_at(self.prev_end()),
                            })
                        }
                    };
                    let span = self.span(base.span.start, self.prev_end());
                    base = Expr {
                        kind: ExprKind::Member {
                            base: Box::new(base),
                            field,
                        },
                        span,
                    };
                }
                TokenKind::LBracket => {
                    self.bump();
                    self.paren_depth += 1;
                    let index = self.parse_expr();
                    self.paren_depth -= 1;
                    if self.eat(TokenKind::RBracket).is_none() {
                        self.diag("P0002", self.cur_span(), "unclosed `[` in index");
                    }
                    // Cover the index child (a recovery placeholder may sit just
                    // outside the consumed range, J2).
                    let span = SourceLoc::new(base.span.start, self.prev_end().max(index.span.end));
                    base = Expr {
                        kind: ExprKind::Index {
                            base: Box::new(base),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                TokenKind::LParen => {
                    // ⟦P0108⟧ call expression.
                    let args = self.parse_call_args();
                    // Cover the args (a recovery arg may sit just outside the
                    // consumed range, J2).
                    let mut end = self.prev_end();
                    for a in &args {
                        end = end.max(a.span.end);
                    }
                    let span = self.span(base.span.start, end);
                    // ADR-0029: `iota(n)` / `fill(x, n)` are Core builtins — the
                    // only legal call expressions (lower owns the L1612/L1613
                    // misuse diagnostics).
                    let is_array_builtin = matches!(
                        &base.kind,
                        ExprKind::Var(n) if matches!(self.text(n.span), "iota" | "fill")
                    );
                    if !is_array_builtin {
                        self.diag(
                            "P0108",
                            span,
                            "call expressions are out of Flow-Core (HANDOFF §4); \
                             use a tuple-input flow: `(args) -> f`",
                        );
                    }
                    base = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(base),
                            args,
                        },
                        span,
                    };
                }
                TokenKind::Question => {
                    // ⟦P0101⟧ `?` postfix (W23: bound to the stage target).
                    let q = self.bump().span;
                    self.diag(
                        "P0101",
                        q,
                        "the `?` operator is out of Flow-Core (HANDOFF §4); planned for Core+1 \
                         error handling",
                    );
                    let span = self.span(base.span.start, q.end);
                    base = Expr {
                        kind: ExprKind::Question(Box::new(base)),
                        span,
                    };
                }
                TokenKind::Colon if self.peek_kind(1) == TokenKind::Colon => {
                    // ⟦P0109⟧ `::` path. Only when the two colons are
                    // span-adjacent (DESIGN §14.6).
                    let c0 = self.cur().span;
                    let c1 = self.peek(1).span;
                    if c0.end != c1.start {
                        break;
                    }
                    self.bump(); // first `:`
                    self.bump(); // second `:`
                    self.diag(
                        "P0109",
                        self.span(c0.start, c1.end),
                        "`::` paths are out of Flow-Core (HANDOFF §4); planned for Core+1 modules",
                    );
                    // Consume the next token as the path segment if it is an
                    // Ident or any keyword (e.g. `List::map` ends at KwMap).
                    if self.at(TokenKind::Ident) || is_keyword(self.kind()) {
                        self.bump();
                    }
                    // Continue the postfix loop on the base expression; the span
                    // grows to the consumed segment.
                    let span = self.span(base.span.start, self.prev_end());
                    base = Expr {
                        kind: base.kind,
                        span,
                    };
                }
                _ => break,
            }
        }
        base
    }

    /// `'(' args? ')'` — caller has checked the `(`. Returns the argument exprs.
    fn parse_call_args(&mut self) -> Vec<Expr> {
        self.bump(); // `(`
        let mut args = Vec::new();
        self.paren_depth += 1;
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            let before = self.pos;
            args.push(self.parse_expr());
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        self.paren_depth -= 1;
        if self.eat(TokenKind::RParen).is_none() {
            self.diag("P0002", self.cur_span(), "unclosed `(` in call");
        }
        args
    }

    fn parse_primary(&mut self) -> Expr {
        let start_span = self.cur_span();
        match self.kind() {
            TokenKind::Int => {
                let span = self.bump().span;
                Expr {
                    kind: ExprKind::Int(self.parse_int(span)),
                    span,
                }
            }
            TokenKind::Float => {
                let span = self.bump().span;
                Expr {
                    kind: ExprKind::Float,
                    span,
                }
            }
            TokenKind::Str => {
                let span = self.bump().span;
                Expr {
                    kind: ExprKind::Str,
                    span,
                }
            }
            TokenKind::KwTrue => {
                let span = self.bump().span;
                Expr {
                    kind: ExprKind::Bool(true),
                    span,
                }
            }
            TokenKind::KwFalse => {
                let span = self.bump().span;
                Expr {
                    kind: ExprKind::Bool(false),
                    span,
                }
            }
            TokenKind::Ident => {
                // `IDENT '{' field-inits '}'` struct literal, else a Var.
                if self.peek_kind(1) == TokenKind::LBrace {
                    self.parse_struct_literal()
                } else {
                    let span = self.bump().span;
                    Expr {
                        kind: ExprKind::Var(Name { span }),
                        span,
                    }
                }
            }
            TokenKind::LParen => self.parse_paren_or_tuple(),
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::BackArrow => {
                // ⟦P0009⟧ `<-` inside an expression (W2 hint).
                let span = self.bump().span;
                self.diag_fix(
                    "P0009",
                    span,
                    "`<-` cannot appear inside an expression; `i<-1` lexes as `i <- 1`. \
                     For a comparison write `i < -1`",
                    SuggestedFix {
                        span,
                        replacement: "< -".to_string(),
                        label: "did you mean `< -` (comparison with a negative)?",
                    },
                );
                self.error_expr()
            }
            TokenKind::Error => {
                // Lexer-diagnosed token: skip silently, ends the atom (C14).
                self.bump();
                self.error_expr()
            }
            _ => {
                // `ret`/`loop` as a chain head, or any other non-expression token.
                self.diag("P0001", start_span, "expected an expression");
                self.error_expr()
            }
        }
    }

    /// `IDENT '{' field-inits? '}'` — caller verified `Ident {`.
    fn parse_struct_literal(&mut self) -> Expr {
        let start = self.cur_span().start;
        let name = Name {
            span: self.bump().span,
        };
        if !self.enter() {
            self.skip_balanced(TokenKind::LBrace, TokenKind::RBrace);
            self.leave();
            return Expr {
                kind: ExprKind::Struct {
                    name,
                    fields: Vec::new(),
                },
                span: self.node_span(start),
            };
        }
        self.bump(); // `{`
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            let before = self.pos;
            let f = self.parse_field_init();
            fields.push(f);
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        if self.eat(TokenKind::RBrace).is_none() {
            self.diag(
                "P0002",
                self.span(start, start + 1),
                "unclosed `{` in struct literal",
            );
        }
        self.leave();
        // Cover the field inits (a recovery value may sit outside the consumed
        // range, J2).
        let mut span = self.node_span(start);
        for f in &fields {
            span = SourceLoc::new(span.start.min(f.span.start), span.end.max(f.span.end));
        }
        Expr {
            kind: ExprKind::Struct { name, fields },
            span,
        }
    }

    /// `field-init := IDENT (':' expr)?` — pun shorthand allowed.
    fn parse_field_init(&mut self) -> FieldInit {
        let start = self.cur_span().start;
        let name = self.expect_name("field name");
        let value = if self.at(TokenKind::Colon) {
            self.bump();
            self.paren_depth += 1;
            let v = self.parse_expr();
            self.paren_depth -= 1;
            Some(v)
        } else {
            None
        };
        let span = match &value {
            Some(v) => self.node_span_covering(start, v.span),
            None => self.node_span(start),
        };
        FieldInit { name, value, span }
    }

    /// `'(' expr ')'` (grouping) or `'(' expr (',' expr)+ ','? ')'` (tuple ≥2).
    fn parse_paren_or_tuple(&mut self) -> Expr {
        let start = self.cur_span().start;
        self.bump(); // `(`
        // Empty `()` is not valid; report and recover.
        if self.at(TokenKind::RParen) {
            self.bump();
            self.diag(
                "P0001",
                self.span(start, self.prev_end()),
                "empty parentheses `()` are not an expression",
            );
            return self.error_expr();
        }
        // Inside parens: a `<-` is the W2/P0009 expression defect (gated below).
        self.paren_depth += 1;
        let result = self.parse_paren_or_tuple_inner(start);
        self.paren_depth -= 1;
        result
    }

    fn parse_paren_or_tuple_inner(&mut self, start: u32) -> Expr {
        let first = self.parse_expr();
        if self.at(TokenKind::Comma) {
            // Tuple.
            let mut elems = vec![first];
            let mut trailing_only = false;
            while self.at(TokenKind::Comma) {
                self.bump();
                if self.at(TokenKind::RParen) {
                    trailing_only = true;
                    break;
                }
                let before = self.pos;
                elems.push(self.parse_expr());
                if self.pos == before {
                    break;
                }
            }
            if self.eat(TokenKind::RParen).is_none() {
                self.diag("P0002", self.cur_span(), "unclosed `(` in tuple");
            }
            let span = self.expr_list_span(start, &elems);
            // `(e,)` is not a 1-tuple → P0001.
            if elems.len() < 2 && trailing_only {
                self.diag(
                    "P0001",
                    span,
                    "`(e,)` is not a one-tuple; tuples need at least two elements",
                );
            }
            Expr {
                kind: ExprKind::Tuple(elems),
                span,
            }
        } else {
            // Grouping — no Paren node; nesting carries it (DESIGN §14.6).
            if self.eat(TokenKind::RParen).is_none() {
                self.diag("P0002", self.cur_span(), "unclosed `(`");
            }
            // The grouped expression's span widens to include the parentheses so
            // J2 nesting (child ⊆ parent) holds for downstream usage.
            let span = self.node_span_covering(start, first.span);
            Expr {
                kind: first.kind,
                span,
            }
        }
    }

    /// The span of an expression composite (tuple/array): `[start, prev_end]`
    /// widened to cover all element spans (a recovery element may land outside
    /// the consumed range, J2).
    fn expr_list_span(&self, start: u32, elems: &[Expr]) -> SourceLoc {
        let mut span = self.node_span(start);
        for e in elems {
            span = SourceLoc::new(span.start.min(e.span.start), span.end.max(e.span.end));
        }
        span
    }

    /// `'[' array-elem (',' array-elem)* ','? ']'`. A `...expr` element → P0107.
    fn parse_array_literal(&mut self) -> Expr {
        let start = self.cur_span().start;
        self.bump(); // `[`
        // `[]` → P0001 (no element type, not exhibited).
        if self.at(TokenKind::RBracket) {
            self.bump();
            self.diag(
                "P0001",
                self.span(start, self.prev_end()),
                "empty array literal `[]` has no element type and is not Flow-Core surface",
            );
            return Expr {
                kind: ExprKind::Array(Vec::new()),
                span: self.span(start, self.prev_end()),
            };
        }
        let mut elems = Vec::new();
        self.paren_depth += 1;
        while !self.at(TokenKind::RBracket) && !self.at_eof() {
            let before = self.pos;
            if self.at(TokenKind::DotDotDot) {
                // ⟦P0107⟧ rest element.
                let dots = self.bump().span;
                let e = self.parse_expr();
                self.diag(
                    "P0107",
                    self.span(dots.start, e.span.end),
                    "rest elements (`...tail`) in array literals are out of Flow-Core \
                     (HANDOFF §4); planned for Core+1 slices",
                );
                elems.push(e);
            } else {
                elems.push(self.parse_expr());
            }
            if self.at(TokenKind::Comma) {
                self.bump();
            } else {
                break;
            }
            if self.pos == before {
                break;
            }
        }
        self.paren_depth -= 1;
        if self.eat(TokenKind::RBracket).is_none() {
            self.diag(
                "P0002",
                self.span(start, start + 1),
                "unclosed `[` in array literal",
            );
        }
        let span = self.expr_list_span(start, &elems);
        Expr {
            kind: ExprKind::Array(elems),
            span,
        }
    }

    fn error_expr(&self) -> Expr {
        Expr {
            kind: ExprKind::Error,
            span: self.here(),
        }
    }

    // ======================================================================
    // shared helpers
    // ======================================================================

    /// Re-parse the digits of an `Int`/projection span, clamping to `u64::MAX`
    /// silently (the lexer already emitted L0008 on overflow; never double-report,
    /// C14).
    fn parse_int(&self, span: SourceLoc) -> u64 {
        self.text(span).parse::<u64>().unwrap_or(u64::MAX)
    }

    /// Expect an identifier; on absence, emit P0001 and return a zero-width name
    /// at the consumed boundary (so it stays within any enclosing node, J2).
    fn expect_name(&mut self, what: &str) -> Name {
        if self.at(TokenKind::Ident) {
            Name {
                span: self.bump().span,
            }
        } else {
            self.diag("P0001", self.cur_span(), format!("expected {what}"));
            Name { span: self.here() }
        }
    }

    /// Expect a statement-terminating `;`; on absence emit P0001.
    fn expect_semi(&mut self, what: &str) {
        if self.eat(TokenKind::Semi).is_none() {
            self.diag(
                "P0001",
                self.cur_span(),
                format!("expected `;` after {what}"),
            );
            self.sync_to_stmt_end();
        }
    }

    /// Statement-level sync (DESIGN §16.1): skip to `;` (consume) or `}` (leave)
    /// or a statement-start keyword (leave).
    fn sync_to_stmt_end(&mut self) {
        loop {
            match self.kind() {
                TokenKind::Semi => {
                    self.bump();
                    break;
                }
                TokenKind::RBrace
                | TokenKind::Eof
                | TokenKind::KwLoop
                | TokenKind::KwFn
                | TokenKind::KwType => break,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip a balanced `open … close` group (depth-counted), consuming through
    /// the matching close. Used to skip annotation/executor/generic bodies. If
    /// the cursor is not at `open`, it is a no-op.
    fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
        if !self.at(open) {
            return;
        }
        let mut depth = 0i32;
        loop {
            let k = self.kind();
            if k == TokenKind::Eof {
                self.diag(
                    "P0002",
                    self.cur_span(),
                    "unclosed delimiter at end of input",
                );
                break;
            }
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    break;
                }
            }
            self.bump();
        }
    }

    /// ⟦P0012⟧ top-level statement: parse for spans, store `Item::Error`.
    fn parse_top_level_statement(&mut self) -> Item {
        let start = self.cur_span().start;
        let span0 = self.cur_span();
        self.diag(
            "P0012",
            span0,
            "statements are not allowed at the top level; wrap them in a function",
        );
        // Parse a statement-or-tail for spans, then discard the structure.
        let before = self.pos;
        match self.parse_statement_or_tail() {
            StmtOrTail::Stmt(_) | StmtOrTail::Tail(_) => {}
        }
        if self.pos == before {
            // Could not consume — skip one token to make progress (J1).
            self.bump();
        }
        Item::Error(self.node_span(start))
    }
}

/// The classification of a block-item at the cursor (DESIGN §14.3).
enum BlockItemClass {
    Arm,
    Statement,
}

/// Result of parsing a block item that may be a statement or the block tail.
enum StmtOrTail {
    Stmt(Stmt),
    Tail(Chain),
}

// --- free functions -------------------------------------------------------

/// The covering span of a stage kind's direct children (for widening the stage
/// span so every child nests within it, J2). `None` for childless stages.
fn stage_kind_extent(kind: &StageKind) -> Option<SourceLoc> {
    fn widen(acc: &mut Option<SourceLoc>, s: SourceLoc) {
        *acc = Some(match *acc {
            None => s,
            Some(a) => SourceLoc::new(a.start.min(s.start), a.end.max(s.end)),
        });
    }
    let mut acc = None;
    match kind {
        StageKind::Expr(e) | StageKind::OpShorthand { expr: e } => widen(&mut acc, e.span),
        StageKind::Bind { ty: Some(ty), .. } => widen(&mut acc, ty.span),
        StageKind::Guard(arms) => {
            for a in arms {
                widen(&mut acc, a.span);
            }
        }
        StageKind::Fanout { branches, .. } => {
            for b in branches {
                widen(&mut acc, b.span);
            }
        }
        StageKind::MapFold { body, .. }
        | StageKind::StmtBlock(body)
        | StageKind::SeqBlock(body) => widen(&mut acc, body.span),
        _ => {}
    }
    acc
}

/// Whether a stage kind is a "block form" (guard/fanout/seq/map/fold/stmt-block):
/// for these the trailing `;` is optional (DESIGN §14.3 termination rule, W10).
fn stage_is_block_form(kind: &StageKind) -> bool {
    matches!(
        kind,
        StageKind::Guard(_)
            | StageKind::Fanout { .. }
            | StageKind::MapFold { .. }
            | StageKind::SeqBlock(_)
            | StageKind::StmtBlock(_)
    )
}

/// Whether `k` is a leading binary operator that starts an op-shorthand after
/// `->` (DESIGN §14.3). `!` is NOT a shorthand (unary, starts a general expr).
/// A leading `-` is always subtraction shorthand here (W12).
fn is_op_shorthand_lead(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::EqEq
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::Le
            | TokenKind::Ge
            | TokenKind::AmpAmp
            | TokenKind::PipePipe
    )
}

/// Map a token kind to a binary operator (for op-shorthand and the climber).
fn binop_of(k: TokenKind) -> Option<BinOp> {
    Some(match k {
        TokenKind::Plus => BinOp::Add,
        TokenKind::Minus => BinOp::Sub,
        TokenKind::Star => BinOp::Mul,
        TokenKind::Slash => BinOp::Div,
        TokenKind::Percent => BinOp::Mod,
        TokenKind::EqEq => BinOp::Eq,
        TokenKind::BangEq => BinOp::Ne,
        TokenKind::Lt => BinOp::Lt,
        TokenKind::Gt => BinOp::Gt,
        TokenKind::Le => BinOp::Le,
        TokenKind::Ge => BinOp::Ge,
        TokenKind::AmpAmp => BinOp::And,
        TokenKind::PipePipe => BinOp::Or,
        _ => return None,
    })
}

/// The precedence level of a binary operator (DESIGN §14.6): mul=3, add=4,
/// cmp=5, and=6, or=7.
fn binop_level(op: BinOp) -> u8 {
    match op {
        BinOp::Mul | BinOp::Div | BinOp::Mod => 3,
        BinOp::Add | BinOp::Sub => 4,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 5,
        BinOp::And => 6,
        BinOp::Or => 7,
    }
}

/// A comparison operator token → its `BinOp` (level 5).
fn cmp_op(k: TokenKind) -> Option<BinOp> {
    Some(match k {
        TokenKind::EqEq => BinOp::Eq,
        TokenKind::BangEq => BinOp::Ne,
        TokenKind::Lt => BinOp::Lt,
        TokenKind::Gt => BinOp::Gt,
        TokenKind::Le => BinOp::Le,
        TokenKind::Ge => BinOp::Ge,
        _ => return None,
    })
}

/// Map a clean `GuardKind` to a tree `GuardDiscr`.
fn guard_discr_of(gk: GuardKind) -> GuardDiscr {
    match gk {
        GuardKind::True => GuardDiscr::True,
        GuardKind::False => GuardDiscr::False,
        GuardKind::Default => GuardDiscr::Default,
        GuardKind::Int(n) => GuardDiscr::Int(n),
    }
}

/// Whether a token kind is a keyword (used by the `::` path segment rule:
/// `List::map` ends at `KwMap`).
fn is_keyword(k: TokenKind) -> bool {
    matches!(
        k,
        TokenKind::KwFn
            | TokenKind::KwType
            | TokenKind::KwMut
            | TokenKind::KwLoop
            | TokenKind::KwRet
            | TokenKind::KwSeq
            | TokenKind::KwMap
            | TokenKind::KwFold
            | TokenKind::KwTrue
            | TokenKind::KwFalse
            | TokenKind::KwCategory
            | TokenKind::KwExecutor
            | TokenKind::KwPub
            | TokenKind::KwUse
            | TokenKind::KwVoid
    )
}

#[cfg(test)]
mod tests;
