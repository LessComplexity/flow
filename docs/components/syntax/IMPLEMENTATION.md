# syntax — implementation map

> The functor DESIGN.md ("Categorical model") → code. Each categorical object/morphism →
> the file:symbol that realises it. Keep in sync WITH the code (FRAMEWORK §6.3):
> a new morphism gets a row here in the same change that adds its code.

Firewall (ADR-0014, Level B only): every row below names one of the **compiler's own**
Rust types/passes. Nothing here describes a Flow program as a category — `Chain`/`Stage`/
`StageKind` are Level-B AST data that *represent* Level-A constructs, never arrows of this
category.

## Objects (Dat) → code
| Object | Form / shape | Realised at | State |
| --- | --- | --- | --- |
| `𝕊` (source) | primitive string set | the `source: &str` parameter of `crates/flow-syntax/src/lexer.rs:lex` / `crates/flow-syntax/src/parser.rs:parse` | built |
| `Token` | product `TokenKind × SourceLoc`, `Copy` | `crates/flow-syntax/src/token.rs:Token` | built |
| `TokenKind` | sum (payload-free except `Guard`) | `crates/flow-syntax/src/token.rs:TokenKind` | built |
| `GuardKind` | sum `True ⊕ False ⊕ Default ⊕ Int(u64)`; the `Guard` token payload (§3) | `crates/flow-syntax/src/token.rs:GuardKind` | built |
| `SourceLoc` | product `{start:u32, end:u32}`, half-open span, `Copy Ord` | `crates/flow-syntax/src/loc.rs:SourceLoc` | built |
| `LineCol` | product `{line,col}`, 1-based; display-only deduction target | `crates/flow-syntax/src/loc.rs:LineCol` | built |
| `Diagnostic` | product (code × severity × span × message × fix?) | `crates/flow-syntax/src/diag.rs:Diagnostic` | built |
| `DiagCode` | newtype `&'static str` (`L####`/`P####`/`T####`) | `crates/flow-syntax/src/diag.rs:DiagCode` | built |
| `Severity` | sum `Error ⊕ Warning` | `crates/flow-syntax/src/diag.rs:Severity` | built |
| `SuggestedFix` | product (span × replacement × label) | `crates/flow-syntax/src/diag.rs:SuggestedFix` | built |
| `Program` | product `Item* × SourceLoc` | `crates/flow-syntax/src/ast.rs:Program` | built |
| `Item` | sum `Fn ⊕ Type ⊕ Error` | `crates/flow-syntax/src/ast.rs:Item` | built |
| `FnDecl` | product (name × params × ret_ty? × body × span) | `crates/flow-syntax/src/ast.rs:FnDecl` | built |
| `Block` | product `BlockItem* × Chain? × SourceLoc` | `crates/flow-syntax/src/ast.rs:Block` | built |
| `Chain` | product `Expr? × Stage* × SourceLoc` (flat spine, C11) | `crates/flow-syntax/src/ast.rs:Chain` | built |
| `Stage` | product (arrow_span × StageKind × span) | `crates/flow-syntax/src/ast.rs:Stage` | built |
| `StageKind` | sum (Expr/Bind/Ret/LoopJump/OpShorthand/Guard/Fanout/MapFold/**SeqBlock**/StmtBlock/Error); `SeqBlock(Block)` is `seq { … }` (ADR-0019) | `crates/flow-syntax/src/ast.rs:StageKind` | built |
| `Expr` | recursive product/sum tree, each node `× SourceLoc` | `crates/flow-syntax/src/ast.rs:Expr` | built |
| `Ty` | product `TyKind × SourceLoc` (surface type, distinct from IR `Ty`) | `crates/flow-syntax/src/ast.rs:Ty` | built |
| `LexOutput` | product `Token* × Diagnostic*` | `crates/flow-syntax/src/lexer.rs:LexOutput` | built |
| `ParseOutput` | product `Program × Diagnostic*` | `crates/flow-syntax/src/ast.rs:ParseOutput` | built |

## Morphisms (Trn / relations) → code
| Morphism | Signature | Realising code | State |
| --- | --- | --- | --- |
| `kind` | `Token → TokenKind` | `crates/flow-syntax/src/token.rs:Token` (field `kind`) | built |
| `span` | `Token → SourceLoc` | `crates/flow-syntax/src/token.rs:Token` (field `span`) | built |
| `lexeme` | `Token × 𝕊 → 𝕊` (Deduced) | `crates/flow-syntax/src/parser.rs:Parser::text` (`&self.source[span]`; never stored) | built |
| `span` (uniform) | `Node → SourceLoc` (nat. transf. `Id ⇒ ConstSourceLoc`, C12/J2) | per-node `span` field across `crates/flow-syntax/src/ast.rs` (`Program`/`FnDecl`/`Stmt`/`Expr`/… `.span`) | built |
| `code` | `Diagnostic → DiagCode` | `crates/flow-syntax/src/diag.rs:Diagnostic` (field `code`) | built |
| `severity` | `Diagnostic → Severity` | `crates/flow-syntax/src/diag.rs:Diagnostic` (field `severity`) | built |
| `fix?` | `Diagnostic → SuggestedFix` (Partial) | `crates/flow-syntax/src/diag.rs:Diagnostic` (field `fix: Option<…>`) | built |
| `items` | `Program → Item*` | `crates/flow-syntax/src/ast.rs:Program` (field `items`) | built |
| `fn?` | `Item → FnDecl` (Partial) | `crates/flow-syntax/src/ast.rs:Item::Fn` | built |
| `body` | `FnDecl → Block` | `crates/flow-syntax/src/ast.rs:FnDecl` (field `body`) | built |
| `tail?` | `Block → Chain` (Partial) | `crates/flow-syntax/src/ast.rs:Block` (field `tail: Option<Chain>`) | built |
| `head?` | `Chain → Expr` (Partial) | `crates/flow-syntax/src/ast.rs:Chain` (field `head: Option<Expr>`) | built |
| `stages` | `Chain → Stage*` | `crates/flow-syntax/src/ast.rs:Chain` (field `stages`) | built |
| `kind` | `Stage → StageKind` | `crates/flow-syntax/src/ast.rs:Stage` (field `kind`) | built |
| `tokens` | `LexOutput → Token*` (Eof-terminated, I1) | `crates/flow-syntax/src/lexer.rs:LexOutput` (field `tokens`) | built |
| `lex_diags` | `LexOutput → Diagnostic*` | `crates/flow-syntax/src/lexer.rs:LexOutput` (field `diagnostics`) | built |
| `program` | `ParseOutput → Program` | `crates/flow-syntax/src/ast.rs:ParseOutput` (field `program`) | built |
| `parse_diags` | `ParseOutput → Diagnostic*` (⊇ lex diags, J6) | `crates/flow-syntax/src/ast.rs:ParseOutput` (field `diagnostics`) | built |
| `line_col` | `SourceLoc.start → LineCol` (Deduced, display-only) | `crates/flow-syntax/src/loc.rs:LineIndex::line_col` | built |
| `lex` (pass) | `𝕊 → LexOutput` (Total) | `crates/flow-syntax/src/lexer.rs:lex` | built |
| `parse` (pass) | `𝕊 → ParseOutput` (Total; composes `lex ; parse_program ; merge+sort`) | `crates/flow-syntax/src/parser.rs:parse` | built |

## Composition rules / invariants → where enforced
| Rule (from DESIGN) | Enforced at | Tested at |
| --- | --- | --- |
| I1 — `lex` total, `Eof`-terminated, scanner always advances | `crates/flow-syntax/src/lexer.rs:lex` (scan loop) | `tests/proptest_lexer.rs::arbitrary_strings_total`, `::arbitrary_unicode_total` |
| I2 — token spans strictly ascending, non-overlapping, char-aligned | `crates/flow-syntax/src/lexer.rs` (debug-asserts at emit) | `tests/proptest_lexer.rs::flow_soup_deterministic_and_total` |
| I3 — coverage: every byte in exactly one token span or trivia region | `crates/flow-syntax/src/lexer.rs` (scan structure) | `tests/coverage.rs::trivia_gaps_pass_coverage`, `::dropped_non_trivia_byte_fails_coverage` |
| I4 — every `Error` token overlaps ≥1 diagnostic span | `crates/flow-syntax/src/lexer.rs` (Error emit paired with diag) | `tests/lex_errors.rs::lex_errors_all_codes_present` |
| I5 — no rendering: `Diagnostic` has no `Display` in-crate (C3) | `crates/flow-syntax/src/diag.rs` (only `Debug` derived) | grep/review (`tests/lex_errors.rs::lex_errors_snapshot` snapshots `Debug`) |
| J1 — `parse` total: never panics, never hangs | `crates/flow-syntax/src/parser.rs:parse` (`DEPTH_LIMIT`=128 + progress lemma) | `tests/proptest_parser.rs::arbitrary_strings_total`, `::flow_soup_total` |
| J2 — span sanity: child ⊆ parent, siblings ordered non-overlapping | `crates/flow-syntax/src/parser.rs` (debug-asserts in node constructors) | `tests/proptest_parser.rs::flow_soup_total` (`check_program`) |
| J3 — zero diagnostics ⇒ no `Error`/rejected-but-kept nodes | `crates/flow-syntax/src/parser.rs` (clean-tree discipline) | `tests/proptest_parser.rs` (`has_error_or_rejected`), `tests/golden_trees.rs` |
| J4 — eight `examples/*.flow` parse with zero diagnostics (+zip_demo, +vector_add per ADR-0018) | `crates/flow-syntax/src/parser.rs:parse` | `tests/golden_trees.rs::tree_abs` … `::tree_seq_demo` (the nine example goldens; `::tree_unit_time_head` is the S29 inline-source golden, not an example) |
| J5 — presentation-free: no `Display` anywhere in crate | `crates/flow-syntax/src/ast.rs`, `diag.rs` (only `Debug`) | grep/review |
| J6 — `parse(s).diagnostics ⊇ lex(s).diagnostics` (same values) | `crates/flow-syntax/src/parser.rs:parse` (seeds `Parser` with lex diags, then stable-sorts) | `tests/proptest_parser.rs::flow_soup_total` |
| Pipe-weld — `t_to(lex) = t_from(parse_program) = Token*` (Coherence Law 1, specialised) | `crates/flow-syntax/src/parser.rs:parse` (`lex` → `Parser::new(&lexed.tokens)`) | `tests/golden_trees.rs` (end-to-end), `tests/out_of_core.rs::full_surface_exact_pcode_multiset` |

## Notes / divergences
Per FRAMEWORK §6.6 — where code carries structure the model's diagram abstracts over, or
vice versa.

- **`LineIndex<'a>` borrows the source** (`crates/flow-syntax/src/loc.rs:LineIndex`).
  DESIGN §1 sketches `LineIndex { /* sorted line-start offsets */ }` — offsets only; the
  code additionally holds `source: &'a str` (a borrow of the caller's buffer, not a copy)
  so `line_col` can count columns char-aware. It is the `line_col` deduced morphism's
  realising machinery, not a `Dat` object. The earlier owned-`String` copy (a §5
  duplication smell) was dropped in Session 09 — source is now single-sourced in the
  caller's buffer.
- **The §15 AST sub-node zoo is intentionally below the diagram's abstraction level, not a
  divergence.** The core `Dat` diagram shows a *selected* spine (`Program → Item → FnDecl →
  Block → Chain → Stage → StageKind`, with `Expr`/`Ty` as leaves). The concrete tree adds
  many more Level-B node types — `Stmt`/`StmtKind`, `BindStmt`, `LoopStmt`/`LoopLabel`,
  `Param`, `TypeDecl`, `Field`, `Name`, `GuardArm`/`GuardDiscr`/`ArmPayload`, `FanoutKind`,
  `CollOp`, `MemberField`, `FieldInit`, `UnOp`/`BinOp`, and the `ExprKind`/`TyKind` variant
  sets (all in `crates/flow-syntax/src/ast.rs`). DESIGN §15 catalogues them in full; the
  diagram deliberately elides them. No row above per variant — the parent object's row
  covers them. Recorded so their absence from the tables reads as intent.
- **`unescape_string` and `Diagnostic::error`/`with_fix` are bridge/constructor helpers, not
  core `Dat` morphisms.** `unescape_string` (`crates/flow-syntax/src/token.rs`) realises the
  `𝕊 → 𝕊` "materialise `Str` value" bridge to `flow-lower::emit` (DESIGN Bridges table), off
  the core diagram. `Diagnostic::error`/`with_fix` (`crates/flow-syntax/src/diag.rs`) are the
  single-source diagnostic constructors reused by the parser (C15) — one-source-of-truth
  done right, not a morphism.
- **`keyword_kind` and `SourceLoc::{new,empty_at,len,is_empty}`** are lexer-internal helpers
  with no model element; expected for a Level-B crate. Not divergences.
- **`seq` statement block (ADR-0019).** The `KwSeq` arm of `crates/flow-syntax/src/parser.rs:Parser::parse_stage_body`
  builds `StageKind::SeqBlock` via `Parser::parse_block` (the ordinary block production,
  `guard_ok=false`), not `parse_fanout_block`. `FanoutKind` lost its `Seq` summand.
  **P0117** is emitted in `crates/flow-syntax/src/parser.rs:Parser::parse_fanout_block`
  (now `void`-only) for each dropped non-chain statement — a diagnostic, not a model
  morphism (like the other P-codes, off the `Dat`/`Trn` tables). `SeqBlock` is a block form
  in `stage_is_block_form` and widened in `stage_kind_extent` (both `parser.rs`).
- **Unit literal `()` (S29, `time` builtin).** `crates/flow-syntax/src/ast.rs:ExprKind::Unit`
  is a new `Expr` variant, built by `crates/flow-syntax/src/parser.rs:Parser::parse_paren_or_tuple`
  (the `at(RParen)` arm, which used to emit **P0001** "empty parentheses `()` are not an
  expression" and return `error_expr()`). Like every other `ExprKind` variant it gets no row
  of its own above — the `Expr` object row covers it (see the §15 sub-node-zoo note) — but the
  *diagnostic* change is recorded here: `()` is now a zero-diagnostic parse. It denotes no
  value; the wire-less-head reading and the rejection of every other position live in lower
  (`crates/flow-lower/src/emit.rs` — `ExprKind::Unit` in the `emit_chain` head seed → `None`,
  and in `emit_expr_dest` → **L1301**), per
  `docs/components/lower/plans/plan-time-builtin.md`. Rendered as `Unit` by
  `crates/flow-syntax/tests/support/mod.rs:TreeWriter::expr`.
- **Element-update bind `c[i] <- x` (ADR-0021).** `BindStmt` gained an optional
  `index: Option<Expr>` field (`crates/flow-syntax/src/ast.rs:BindStmt`): `Some` = the indexed
  form, parsed in `crates/flow-syntax/src/parser.rs:parse_bind_stmt` (the optional `[' expr ']`
  after the name). Three warnings, all diagnostics (off the `Dat`/`Trn` tables like every P-code):
  **P0013** `mut` on an indexed target, **P0014** type annotation on one, **P0015** a nested
  `c[i][j]` target (recovered to a clean one-dimensional bind). The desugar to `Update` is
  lower's job (lower §8 / IMPLEMENTATION `element update` row), not syntax's.
