" Vim syntax file
" Language:    Flow (dataflow language — flow-lang Flow-Core subset, v0.3)
" Maintainer:  Flow project (editors/nvim)
" Reference:   docs/spec/user-guide.md §3 (as patched: E4 statement rule,
"              E5 `type` keyword), HANDOFF.md §4.1 (Flow-Core scope).
"
" Regex-based highlighting. A tree-sitter grammar is deferred until it can be
" derived from the real flow-syntax parser (ADR-0008).
"
" ----------------------------------------------------------------------------
" VIM MATCH-PRECEDENCE NOTE (load-bearing — read before reordering anything):
"   For overlapping `syn match` items at the same position, the LAST one
"   defined wins. We therefore define groups from least-specific to
"   most-specific so the intended group highlights:
"       operators  (defined first)   <-  `-`, `+`, `<`, `>`, ...
"       arrows     (defined after)   <-  `->`, `<-`  win over the `-`/`<`/`>` ops
"       guards     (defined LAST)    <-  `-true->`, `-42->`, `-Some(x)->`, ...
"                                        win over the plain arrows
"   Do not move flowGuardArrow above flowArrow, or guards will be swallowed by
"   the arrow match. Keep this ordering.
" ----------------------------------------------------------------------------

if exists("b:current_syntax")
  finish
endif

" Case matters in Flow (snake_case values, PascalCase types).
syn case match

" ---------------------------------------------------------------------------
" Keywords, builtins, booleans
" ---------------------------------------------------------------------------
" Core keyword set confirmed against user-guide §3 and HANDOFF §4.1:
"   fn type loop seq mut void  — all real Flow-Core keywords.
" (`category` is NOT here — it is reserved-and-rejected; see flowReserved.)
syn keyword flowKeyword fn type loop seq mut void

" Builtins: map/fold are inline-block collection ops; print is the only effect.
syn keyword flowBuiltin map fold print

" Boolean literals.
syn keyword flowBoolean true false

" ---------------------------------------------------------------------------
" ret  — the graph sink (function return object). Highlight as Special.
" ---------------------------------------------------------------------------
syn keyword flowRet ret

" ---------------------------------------------------------------------------
" Primitive and named types
" ---------------------------------------------------------------------------
" Flow-Core primitives (HANDOFF §4.1).
syn keyword flowPrimType i32 i64 u8 f32 f64 bool

" Capitalized identifiers are named product types (PascalCase convention,
" user-guide §10.2). Heuristic: any `\<[A-Z]\w*\>` token.
syn match flowTypeName "\<[A-Z]\w*\>"

" ---------------------------------------------------------------------------
" Reserved-and-rejected keyword (ADR-0006 / E5)
" ---------------------------------------------------------------------------
" `category` was the v0.1 spelling of the type keyword. Under ADR-0006 it is
" reserved-and-rejected: the compiler emits a helpful error pointing at `type`.
" The editor teaches E5 by flagging `category` as an error here.
syn keyword flowReserved category

" ---------------------------------------------------------------------------
" Function name after `fn`
" ---------------------------------------------------------------------------
" Lookbehind (`\%(\<fn\>\s\+\)\@<=`) so the match STARTS at the name and does
" not try to consume the `fn` keyword (flowKeyword already owns it — a Vim match
" cannot start inside another match's region).
syn match flowFnName "\%(\<fn\>\s\+\)\@<=\h\w*"

" ---------------------------------------------------------------------------
" Loop-label declarations and jump targets (user-guide §3.5)
" ---------------------------------------------------------------------------
" Label declaration: an identifier immediately followed by `{` that opens a
" loop body, e.g. `outer {`, `inner {`, `search {`. (`loop` itself is a
" keyword and is matched above; named labels are matched here.)
" Restricted to a lowercase-initial identifier so a PascalCase `type Pixel {`
" stays a type name (snake_case labels by convention; user-guide §10.2).
syn match flowLabel "\<[a-z_]\w*\ze\s*{" contains=NONE

" Label jump targets: the identifier in a jump edge `-> outer;`, `-> inner;`,
" `-> search;`. We highlight the destination identifier as a Label.
" A lookbehind (`\%(->\s*\)\@<=`) is used so this match STARTS at the label word
" itself and does not try to consume the `->` (which flowArrow already owns — a
" Vim match cannot start inside another match's region). `\h\w*` matches the
" destination; `\ze\s*;` bounds it to a jump edge.
" The negative lookahead `\%(loop\|ret\)\@!` excludes the `loop` keyword and the
" `ret` sink so `-> loop;` stays flowKeyword and `-> ret;` stays flowRet
" (Special). Named labels (`outer`, `inner`, `search`, ...) become Label.
syn match flowLabelJump "\%(->\s*\)\@<=\%(\%(loop\|ret\)\>\)\@!\h\w*\ze\s*;"

" ---------------------------------------------------------------------------
" Numbers
" ---------------------------------------------------------------------------
" Integer first, float LAST: Vim's last-match-wins means the float rule (which
" overlaps the integer part of `0.393`) wins over the bare integer at that spot.
syn match flowNumber "\<\d\+\>"
syn match flowFloat  "\<\d\+\.\d\+\>"

" ---------------------------------------------------------------------------
" Strings (only valid as `print` arguments, but highlighted everywhere)
" ---------------------------------------------------------------------------
syn match  flowEscape  contained "\\[nrt\"\\0]"
syn region flowString  start=+"+ skip=+\\"+ end=+"+ contains=flowEscape

" ---------------------------------------------------------------------------
" Operators  (DEFINED BEFORE ARROWS so arrows win over `-`, `<`, `>`)
" ---------------------------------------------------------------------------
" Arithmetic, comparison, logical, assignment. user-guide §3.6 inventory:
"   + - * / %   == != <= >= < >   && || !   =
syn match flowOperator "[+\-*/%=!<>]\|==\|!=\|<=\|>=\|&&\|||"

" ---------------------------------------------------------------------------
" Arrows  (DEFINED AFTER operators, BEFORE guards)
" ---------------------------------------------------------------------------
" Composition is the language — make `->` / `<-` prominent (link Statement).
syn match flowArrow "->\|<-"

" ---------------------------------------------------------------------------
" Guard arrows  (DEFINED LAST so they win over flowArrow and the `-` operator)
" ---------------------------------------------------------------------------
" Boolean / default / integer-literal guards:
"   -true->  -false->  -_->  -0->  -42->
syn match flowGuardArrow "-\%(true\|false\|_\|\d\+\)->"

" Variant-style guards (user-guide §2.1, §3.4) — e.g. -Some(x)->, -None->,
" -Ok(...)-> ; the variant tag is a PascalCase identifier, with an optional
" parenthesized binder.
syn match flowGuardArrow "-[A-Z]\w*\%((\%([^()]*\))\)\?->"

" Destructuring guards (user-guide §3.5): empty list and head/tail.
"   -[]->   -[head, ...tail]->
syn match flowGuardArrow "-\[\%([^][]*\)\]->"

" ---------------------------------------------------------------------------
" Comments  (user-guide §3 shows ONLY `//` line comments — no block comments)
" ---------------------------------------------------------------------------
syn keyword flowTodo contained TODO FIXME XXX NOTE
syn match   flowComment "//.*$" contains=flowTodo,@Spell

" ---------------------------------------------------------------------------
" Highlight links (use `hi default link` so user colorschemes can override)
" ---------------------------------------------------------------------------
hi default link flowComment    Comment
hi default link flowTodo       Todo
hi default link flowString     String
hi default link flowEscape     SpecialChar
hi default link flowNumber     Number
hi default link flowFloat      Float
hi default link flowKeyword    Keyword
hi default link flowBuiltin    Function
hi default link flowBoolean    Boolean
hi default link flowRet        Special
hi default link flowPrimType   Type
hi default link flowTypeName   Type
hi default link flowReserved   Error
hi default link flowFnName     Function
hi default link flowLabel      Label
hi default link flowLabelJump  Label
hi default link flowOperator   Operator
hi default link flowArrow      Statement
hi default link flowGuardArrow Conditional

let b:current_syntax = "flow"
