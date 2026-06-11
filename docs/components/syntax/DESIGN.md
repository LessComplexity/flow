# Component: syntax — DESIGN

Last updated: 2026-06-11 · Session 02
Living document per HANDOFF §7.1.5 — written before code, updated every session that touches this component.

Increment map:

- **§1–§11 (this session): the lexer.** Token model, lexical grammar, diagnostics, API, tests.
- **Parser (next increment):** recursive-descent, statement-level flow chains (ADR-0005), error-recovering (ADR-0008). Parser-design notes that fell out of lexer design are collected in §12 so they are not lost; they are *not yet binding*.

## 0. Spec basis and binding constraints

Sources, in authority order (HANDOFF §2.2): ADR-0005/0006/0008/0009/0010 · `user-guide.md` §2–3 (as patched: E4, E5) for the Flow-Core surface, **plus §5–§10 and the flow snippets in `category-ir.md` enumerated for full-surface tokenization per C8** (`@`-annotations, `?`, `channel<i32>`, `::`, `200MHz`, …) · `architecture.md` §2.2.1 · HANDOFF §4.1 (Flow-Core scope), §5 items 2/6 · `docs/spec/ERRATA.md` (LC-2).

Constraints that bind this design:

| # | Constraint | Source |
|---|---|---|
| C1 | Handwritten lexer; spans (`SourceLoc`) from day one | HANDOFF §5 item 2 |
| C2 | Error-recovering: always returns tokens *and* diagnostics, never bails at first error | ADR-0008(a) — names the parser; applied to the lexer by analogy (strictly stronger) |
| C3 | Diagnostics are structured values (code, severity, span, message, optional fix); **no rendering** in flow-syntax — rendering only in flow-cli | ADR-0008(b) |
| C4 | Pure `fn(source) -> artifacts`, no global state, no incremental machinery | ADR-0008(d) |
| C5 | `type` is the type keyword; `category` reserved-and-rejected with a diagnostic pointing at `type` | ADR-0006 (E5) |
| C6 | `map`/`fold` take a postfix inline block — operator syntax, never a call argument | ADR-0009 (LC-2) |
| C7 | Guard arrows (`-true->`, `-false->`, `-_->`, integer guards) are surface lexemes | architecture.md §2.2.1; user-guide §3.4 |
| C8 | Out-of-Core constructs are *rejected with a clear diagnostic*, not silently accepted → the lexer must tokenize the full v0.2 surface so the **parser** can reject with precision | HANDOFF §4 |
| C9 | Guard arrows are single lexemes, recognized under an adjacency + statement-initial context rule | ADR-0010 (this session; details in §5) |

## 1. Source model: `SourceLoc` and `LineIndex`

```rust
/// Half-open byte range [start, end) into the source text. Always on char boundaries.
pub struct SourceLoc { pub start: u32, pub end: u32 }   // Copy, Eq, Ord

pub struct LineCol { pub line: u32, pub col: u32 }      // 1-based, for display

/// Built once per file; converts byte offsets to line/col. O(log n) lookup.
pub struct LineIndex { /* sorted line-start offsets */ }
impl LineIndex {
    pub fn new(source: &str) -> Self;
    pub fn line_col(&self, offset: u32) -> LineCol;
}
```

Byte offsets are the canonical representation (LSP-friendly; 0-based offsets convert to either 1-based terminal display or 0-based LSP positions at the consumer). `u32` suffices — Flow-Core programs are single files (HANDOFF §4.2: no modules).

## 2. Diagnostics

Defined in `flow-syntax` and shared by the parser later; `flow-check` will reuse these types (the fixed crate list of HANDOFF §6 has no diagnostics crate, and check depends on syntax for spans anyway).

```rust
pub struct Diagnostic {
    pub code: DiagCode,            // stable machine code, e.g. L0004
    pub severity: Severity,        // Error | Warning
    pub span: SourceLoc,
    pub message: String,           // plain text, no formatting/color
    pub fix: Option<SuggestedFix>, // machine-applicable suggestion
}
pub struct DiagCode(pub &'static str);     // "L####" lexer, "P####" parser, "T####" check
pub enum Severity { Error, Warning }
pub struct SuggestedFix { pub span: SourceLoc, pub replacement: String, pub label: &'static str }
```

`Debug` is derived (tests snapshot the structured values). `Display`/terminal rendering live in `flow-cli` only (C3).

### Lexer diagnostic codes

| Code | Severity | Trigger | Recovery |
|---|---|---|---|
| L0001 | Error | Unknown character(s) | Coalesce the maximal run of consecutive unknown chars into **one** `Error` token + one diagnostic; continue |
| L0002 | Error | Unterminated string (EOF or raw newline before closing `"`) | Emit `Str` token up to the break point; continue |
| L0003 | Error | Unknown escape sequence in string | Take the escaped char literally; continue lexing the string |
| L0004 | Error | Reserved keyword `category` | Emit `KwCategory`; message points at `type`; `fix` = replace with `type` (ADR-0006). Parser may still parse the declaration for downstream diagnostics |
| L0005 | Error | Lone `=` | Message hints `<-` (binding) or `==` (comparison); emit `Error` token |
| L0006 | Error | Lone `&` or `\|` | Message hints `&&` / `\|\|`; emit `Error` token |
| L0007 | Error | `/*` block comment | Skip to matching `*/` (or EOF, noted in message) as trivia; Flow has line comments only |
| L0008 | Error | Integer literal **or guard discriminant** exceeds `u64` | Value clamps to `u64::MAX` (`GuardKind::Int` included — `-99999999999999999999->` must not panic, I1); typed range checks are flow-check's job |

Reserved-but-lexable keywords (`executor`, `pub`, `use`, `void`) get **no lexer diagnostic** — they are legal v0.2 surface that is merely out of Flow-Core; scope rejection is the parser's job (C8), with targeted P-codes next increment. (Scoping authority: `executor`/`pub`/`use` are out of Core per HANDOFF §4.2 and keyword-listed in architecture.md §2.2.1; `void` is the discard-fanout form of user-guide §3.3, absent from HANDOFF §4.1's exhaustive in-scope list → out of Core by §4's default-reject rule.) `category` is different: after E5 it is not legal surface at all, so the lexer flags it (C5, "from day one").

## 3. Token model

```rust
pub struct Token { pub kind: TokenKind, pub span: SourceLoc }   // Copy

pub enum TokenKind {
    // payload-free; lexeme text is recovered via span when needed
    Ident, Int, Float, Str,
    // Flow-Core keywords
    KwFn, KwType, KwMut, KwLoop, KwRet, KwSeq, KwMap, KwFold, KwTrue, KwFalse,
    // reserved keywords (lexed, rejected later or at lex time per §2)
    KwCategory, KwExecutor, KwPub, KwUse, KwVoid,
    // delimiters & punctuation
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Comma, Colon, Semi, Dot, DotDotDot,
    // operators
    Plus, Minus, Star, Slash, Percent,
    EqEq, BangEq, Lt, Gt, Le, Ge, AmpAmp, PipePipe, Bang,
    Arrow,                    // ->
    BackArrow,                // <-
    Guard(GuardKind),         // -true->  -false->  -_->  -42->   (single lexeme, §5)
    Question,                 // ?   (Core+1 surface, lexed for parser-side rejection)
    At,                       // @   (§9 annotations, lexed for parser-side rejection)
    Error,                    // unknown input, carries diagnostics alongside
    Eof,                      // always last, zero-width span at end of input
}

pub enum GuardKind { True, False, Default /* -_-> */, Int(u64) }
```

Design choices and rationale:

- **Payload-free `Ident`/`Int`/`Float`/`Str`** — text is `&source[span]`; `Token` stays `Copy`, and there is exactly one source of truth for the lexeme. Numeric *typed* values (i32 vs i64 vs f32…) are type-directed and parsed in lower/check from the span text. Exception: `Guard(GuardKind::Int(u64))` carries its value because the digits are *inside* the lexeme `-42->` and sub-span re-parsing at every use site would be error-prone.
- **String values:** the unescaped content differs from the lexeme; `flow-syntax` provides `pub fn unescape_string(lexeme: &str) -> String` (assumes a lexically valid token) used by later phases. Core strings are `print`-only arguments (HANDOFF §4.1).
- **Keywords vs identifiers.** `map`/`fold` are *hard keywords*: they have special syntax (postfix block, C6), so they are operators of the language, not names. `print` is a **plain identifier**: it has ordinary flow-target syntax (`x -> print;`) and is resolved as a builtin by flow-check — no lexical specialness. Primitive type names (`i32`, `f32`, `bool`, …) are plain identifiers resolved by the type grammar/checker, uniform with user type names like `Pixel`.
- **`true`/`false`** are keywords (bool literals). A standalone `_` lexes as `Ident` (it only validly occurs inside `-_->`, which is a single Guard token; the parser rejects stray `_`).
- **`;` is overloaded** by the surface: statement terminator *and* array-type size separator (`[f32; 8]`). One `Semi` token; the parser disambiguates by context.

## 4. Lexical grammar (except guard arrows — §5)

**Trivia** (skipped, no tokens; every byte of input is attributable to exactly one token or trivia region — see §8 invariants):

- Whitespace: space, `\t`, `\r`, `\n`. Newlines are not significant (statements end with `;`).
- Line comments: `//` to end of line. Comments may contain arbitrary UTF-8 (the shipped examples use `∘`, `×`, `—` in comments); skipping must be char-boundary safe.
- `/* … */` is *not* Flow syntax: consumed as trivia with diagnostic L0007 (friendlier than lexing `/` `*`).

**Identifiers / keywords:** `[A-Za-z_][A-Za-z0-9_]*` (ASCII). Match against the keyword table after scanning; otherwise `Ident`. Non-ASCII outside comments/strings → L0001.

**Integer literals:** `[0-9]+` → `Int`. No sign (unary `-` is the parser's), no underscores, no hex/octal/binary, no suffixes — none appear in the v0.2 surface. Leading zeros lex as written.

**Float literals:** `[0-9]+ '.' [0-9]+` → `Float`. Digits required on **both** sides of the dot: consequently `ret.0` lexes as `KwRet Dot Int` (tuple projection, user-guide §3.2) and `5.` lexes as `Int Dot` (parser error). No exponent form (`1e10` is not v0.2 surface; it lexes as `Int Ident` and the parser rejects).
Maximal munch: at a digit, scan digits; if the next char is `.` **and** the char after it is a digit, continue as float; otherwise stop (so `coeffs[k]`, `4 + k`, `0..` all work — `0..255` appears only in comments).

**String literals:** `"` … `"` on a single line. Escapes: `\\`, `\"`, `\n`, `\t`; anything else → L0003 (char taken literally). Raw newline or EOF before the closing quote → L0002, token ends at the break. Content may be arbitrary UTF-8.

**Operators and punctuation — maximal munch, longest first:**

| Lookahead | Result |
|---|---|
| `...` | `DotDotDot` (slice rest-pattern surface, §3.5; out of Core, parser rejects). A bare `..` is two `Dot` tokens; `::` (path syntax, category-ir.md flow snippets) is two `Colon` tokens — both out-of-Core, parser rejects |
| `->` | `Arrow` (but see guard-arrow rule §5 first) |
| `<-` / `<=` | `BackArrow` / `Le` (distinct second chars; `<` alone → `Lt`) |
| `==` `!=` `>=` `&&` `\|\|` | `EqEq` `BangEq` `Ge` `AmpAmp` `PipePipe` |
| singles | `+ - * / % < > ! ( ) { } [ ] , : ; . ? @` → their kinds |
| `=` alone | `Error` + L0005 |
| `&` / `\|` alone | `Error` + L0006 |
| anything else | `Error` + L0001 (coalescing run) |

## 5. Guard arrows — the one hard lexical problem

Guard arms (`-true->`, `-false->`, `-_->`, `-0->`) collide lexically with `Minus`/`Arrow` sequences, and **whitespace is semantically load-bearing**: inside a guard block, `-7-> x;` (arm with discriminant 7, payload `x`) and `-7 -> x;` (the value −7 flowing into `x`) contain the *same* token sequence if guards are assembled from `Minus Int Arrow` — only adjacency distinguishes them. Token streams must not erase that distinction, so a guard arrow is a **single token** produced by the lexer (this also matches architecture.md §2.2.1, which lists guards at the lexer level).

**The rule.** A guard arrow is the lexeme

```
'-' D '->'        with no whitespace anywhere inside,
D ∈ { 'true', 'false', '_', [0-9]+ }
```

recognized **only when both** hold:

1. *(adjacency)* the full pattern matches exactly, with zero interior whitespace;
2. *(context)* the most recently emitted token is `{`, `;`, or `}` — or there is no previous token (start of input). Guard arms are statement-initial: after `{` (first arm), `;` (after a statement payload), or `}` (after a block payload, e.g. `-false-> -> ret;` following the true-arm's closing brace in user-guide §3.5). 

If either fails, the lexer does **not** consume a guard lexeme: it re-scans from the leading `-` with the ordinary rules — `Arrow` if `>` immediately follows the `-`, otherwise `Minus` — and continues normal scanning (so `-7 -> abs` becomes `Minus Int Arrow Ident`).

Because `>` is never a discriminant-start character, the guard rule can never capture a plain `->`: only a leading `-D` with full adjacency can produce a `Guard` token. (This is why fanout arms like `-> square -> sq;` after `{` are safe even though the context gate passes.)

This rule is a binding surface-syntax decision recorded as **ADR-0010** (the spec exhibits the lexemes but is silent on adjacency and context; the ADR fixes both, and the W1 wart below is part of the accepted tradeoff).

**Why the context gate:** without it, `a-1->c` (tightly written `(a-1) -> c`, prev token `Ident`) would mis-lex as `a` `Guard(Int 1)` `c`. With it, expression-position `-` can never start a guard. The remaining wart is statement-initial *tight* negation, `-7->x;`, which lexes as a guard arm — see the ledger (§6, W1); spec style always spaces binary/unary minus, and the parser will report stray `Guard` tokens outside guard blocks with a "did you mean `-7 -> x`? add a space" hint (§12).

**Worked consequences** (all asserted by unit tests, §9):

| Input (context) | Tokens |
|---|---|
| `-true->  x;` (after `{`) | `Guard(True) Ident(x) Semi` |
| `-false-> -> ret;` (after `}`) | `Guard(False) Arrow KwRet Semi` |
| `-_-> "unknown";` (after `;`) | `Guard(Default) Str Semi` |
| `-42-> f;` (after `{`) | `Guard(Int 42) Ident Semi` |
| `-7 -> abs;` (after `{`; spaced) | `Minus Int(7) Arrow Ident Semi` — adjacency fails |
| `x * -1;` | `Ident Star Minus Int(1) Semi` — context fails (prev `Star`); adjacency would fail too (after `1` comes `;`, not `->`) |
| `a-1->c;` | `Ident Minus Int(1) Arrow Ident Semi` — context fails (prev `Ident`) |
| `-true ->` (interior space) | `Minus KwTrue Arrow` — adjacency fails; parser hints |
| `-Some(x)->` (after `{`) | `Minus Ident(Some) LParen Ident RParen Arrow` — `Some` ∉ D; Core+1 pattern guards are a parser-recovery concern (§12), not a lexeme |
| `-1.5->` (after `{`) | `Minus Float(1.5) Arrow` — float discriminants are not Core guards |

The lexer keeps one piece of state for the gate (kind of the last emitted token) — the standard context-sensitivity hack (cf. regex-vs-division in JS lexers), confined to this one production and documented here.

## 6. Edge-case / wart ledger

Decisions, made once, so neither the implementation nor review re-litigates them:

| # | Case | Decision |
|---|---|---|
| W1 | `-7->x;` statement-initial tight negation | Lexes as `Guard(Int 7) Ident(x) Semi`. Applies in **all three** gate contexts (`{`, `;`, and `}` — e.g. `} -7->x;`). Accepted tradeoff of ADR-0010; parser gives the add-a-space hint on stray guards (§12) |
| W2 | `i<-1` meaning `i < -1` | Maximal munch gives `BackArrow`: `i <- 1`. Same family as W1; spec style spaces comparisons. No lexer special case |
| W3 | `ret.0` vs floats | Floats need digits both sides of `.` → `KwRet Dot Int(0)`. `5.` → `Int Dot` (parser error) |
| W4 | `..` (ranges) | Not v0.2 surface outside comments. Two `Dot` tokens; parser rejects |
| W5 | Unicode | Only in comments and strings (examples rely on this). Elsewhere → L0001. All scanning is char-boundary safe; spans never split a UTF-8 sequence |
| W6 | CRLF | `\r` is trivia whitespace; no normalization, spans index the bytes as-is |
| W7 | `1_000`, `0xFF`, `1e10`, `200MHz` | Not lexed as single literals (no suffixed/based/exponent literals in the v0.2 surface); they lex as literal-then-ident etc. and the parser reports. `200MHz` (user-guide §9 annotations) → `Int(200) Ident(MHz)` — maximal munch stops at the letter. Listed here so it's known-deliberate |
| W8 | Adjacent unknown chars | One coalesced `Error` token + one L0001 |
| W9 | Empty input | `[Eof]`, no diagnostics |

## 7. Public API (crate `flow-syntax`)

```rust
// lib.rs re-exports — the whole lexer surface:
pub use loc::{SourceLoc, LineCol, LineIndex};
pub use diag::{Diagnostic, DiagCode, Severity, SuggestedFix};
pub use token::{Token, TokenKind, GuardKind};
pub use lexer::{lex, LexOutput};
pub use token::unescape_string;

pub struct LexOutput { pub tokens: Vec<Token>, pub diagnostics: Vec<Diagnostic> }

/// Total function: never panics, never fails; C2/C4.
pub fn lex(source: &str) -> LexOutput;
```

Module layout:

```
crates/flow-syntax/src/
├── lib.rs      // re-exports + crate docs
├── loc.rs      // SourceLoc, LineCol, LineIndex
├── diag.rs     // Diagnostic, DiagCode, Severity, SuggestedFix
├── token.rs    // Token, TokenKind, GuardKind, unescape_string
└── lexer.rs    // lex(), the scanner, keyword table
```

Implementation shape: single forward pass over `source.char_indices()` with bounded lookahead (guard munch needs ≤ discriminant+2 chars; everything else ≤ 2); no regex, no allocation except `String`s inside diagnostics. O(n).

## 8. Invariants (and where enforced)

| # | Invariant | Enforcement |
|---|---|---|
| I1 | `lex` is total: any `&str` → tokens + diagnostics; last token is always `Eof`; **the scanner always advances** (no infinite loop on any byte) | Structure of the scanner loop; proptest (§9) |
| I2 | Token spans are strictly ascending, non-overlapping, within source, on char boundaries | Debug assertions in the token emitter + proptest |
| I3 | Coverage: every input byte belongs to exactly one token span or trivia region | Test helper re-walks tokens vs source on all fixtures + proptest |
| I4 | Every `Error` token overlaps ≥ 1 diagnostic span | Unit tests per L-code |
| I5 | No rendering: `Diagnostic` has no `Display` impl in this crate (C3) | Code review; grep-able |

## 9. Test plan (this increment)

Dev-dependencies (first external deps in the workspace, per plan): `insta` (golden snapshots), `proptest` (lexer totality). Both dev-only — the compiler itself stays dependency-free.

1. **Golden token streams (insta), the acceptance surface:** `tests/golden_tokens.rs` lexes each of `examples/{abs,fanout,fir,pipeline,sepia,sum_to_n}.flow` and snapshots the rendered stream — one token per line:
   `{line}:{col} {start}..{end} {Kind} ‹{lexeme}›`
   (lexeme omitted for `Eof`). Six `.flow` files must produce **zero diagnostics**. Snapshot review is the verification that tokenization is *correct*, not merely stable — reviewers read the `.snap` against the source.
2. **Golden diagnostics:** `tests/fixtures/lex_errors.flow` exercises every L-code (L0001–L0008, incl. `category` and an over-`u64` guard discriminant); snapshot of `Debug`-formatted `LexOutput` (structured values — not rendering, C3).
3. **Golden full-surface (C8 evidence):** `tests/fixtures/full_surface.flow` exercises the out-of-Core v0.2 surface — `@executor(ThreadPool)`, `@target_frequency(200MHz)`, `x -> f? -> g?`, `channel<i32>`, `Result<T, E>`, `List::map`, `[head, ...tail]`, `pub`/`use`/`executor`/`void` keywords — and snapshots that everything lexes to the intended **non-Error** tokens with zero diagnostics. This is the positive evidence that C8 holds (the parser can later reject these with precision because the lexer never mangles them).
4. **Unit tests** (`lexer.rs` `#[cfg(test)]`): the full §5 worked-consequences table; the §6 ledger rows; operator munch boundaries (`<-` vs `<=` vs `<`, `...`, `::`, `//` vs `/*` vs `/`); string escapes incl. L0002/L0003; float/int boundaries; guard-discriminant overflow (L0008, no panic); CRLF; empty input; `LineIndex` line/col correctness incl. multi-byte chars.
5. **Property tests** (proptest, small): for arbitrary `String`s — no panic; I1–I3 hold. Plus a generator over "Flow-ish" token soup (keywords/operators/literals joined with random whitespace) asserting I1–I3 and that lexing is deterministic.

Benchmarks: deferred until the parser exists (HANDOFF §7.2 step 6 says "once the component is functional" — the *component* is syntax = lexer **+** parser; a lexer-only microbench has no consumer and low signal, so the component-level criterion bench lands with the parser. Recorded as a deliberate choice in STATUS).

## 10. Deliberately not supported (this increment)

Char literals (`char` is v0.2 but not Core; no literal *form* appears anywhere in the corpus — `'` → L0001; known C8 soft-spot, revisit with a dedicated token/L-code if an example ever introduces `'x'`), trivia-preserving token stream (no consumer yet; see §11), interning/symbol table (premature at Core scale), incremental lexing (explicitly excluded by ADR-0008(d)).

## 11. Open questions (→ STATUS / future ADR candidates)

- **String escape set**: `\\ \" \n \t` chosen; spec is silent. Revisit if an example needs more. (Design decision, not a spec deviation — no ADR needed unless the spec gains a position.)
- **Single-line strings**: raw newline terminates with L0002. Same status as above.
- **Trivia for LSP semantic tokens** (ADR-0008 ladder v2): if needed post-M1, add a `lex_with_trivia` variant rather than changing `lex`.
- **Core+1 pattern guards** (`-Some(x)->`, `-[head, ...tail]->`): cannot be single lexemes (nested structure). Expected resolution: keep Core discriminants as single Guard tokens; pattern arms become parser-level compositions with span-adjacency checks, decided in the Core+1 coproducts ADR. Flagged now so the Guard token design isn't treated as accidentally closed.

## 12. Parser-increment notes (recorded, not yet binding)

- Two-tier grammar per ADR-0005: expression parser (precedence 1–7) + statement-level flow-chain parser (`->`/`<-` over expression operands; `?` reserved at level 9).
- Stray `Guard` token outside a guard block → targeted diagnostic with the W1 add-a-space hint.
- `- true ->` / `-true ->` *inside* a guard block (adjacency failure) → targeted "guard arrows are written without spaces" diagnostic.
- `Ident {` ambiguity: struct literal (`Pixel { r: … }`) vs labeled loop (`outer { … }`) — disambiguation strategy is a parser-design question; Core may restrict loop labels to the `loop` keyword (HANDOFF §4.1 shows only `loop`), making custom labels an out-of-Core rejection. Decide next increment.
- `KwCategory` recovery: parse the declaration as if `type` so field diagnostics still fire (L0004 already reported).
- Out-of-Core keyword rejections (`KwExecutor`, `KwPub`, `KwUse`, `KwVoid`, `Question`, `At`, `DotDotDot`) get dedicated P-codes with "out of Flow-Core (HANDOFF §4) — Core+1/post-M5" messages.
