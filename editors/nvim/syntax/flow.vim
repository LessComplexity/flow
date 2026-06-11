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
"       flow-fn    (defined after)   <-  `-> clamp ->` head (between two arrows)
"                                        defined BEFORE flowTypeName so PascalCase
"                                        heads stay Type, not Function
"       arrows     (defined after)   <-  `->`, `<-`  win over the `-`/`<`/`>` ops
"       guards     (defined LAST)    <-  `-true->`, `-42->`, `-Some(x)->`, ...
"                                        win over the plain arrows
"   The guard OUTER match (flowGuardArrow) links to Statement — the chrome (the
"   leading `-` and trailing `->`) reads as flow plumbing, identical to flowArrow.
"   The discriminant inside is overlaid by CONTAINED groups (flowGuardBool /
"   flowGuardInt / flowGuardWild / flowGuardVariant) pulled in via `contains=`, so
"   it gets the color of WHAT IT IS (Boolean / Number / Special / Type).
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

" ---------------------------------------------------------------------------
" Functions appearing in flows (CHANGE 2)
" ---------------------------------------------------------------------------
" An identifier that is BOTH preceded by `->` and followed by `->` sits in a
" call position inside a pipeline, e.g. `clamp` in `(v, lo, hi) -> clamp ->
" bounded;`, or `f`/`g` in `data -> f -> g -> ret;`. We link it to Function.
"   - Lookbehind `\%(->\s*\)\@<=` starts the match at the identifier (the `->`
"     is owned by flowArrow — a Vim match cannot start inside another's region).
"   - Lookahead `\%(\s*->\)\@=` requires a trailing `->`, so terminal bindings
"     and sinks (`-> nr;`, `-> total_r;`, `-> ret;`) stay UNhighlighted.
"   - `syn keyword` groups (ret/loop/map/fold/print/seq/…) outrank `syn match`
"     in Vim automatically, so `data -> map -> …` keeps `map` as flowBuiltin.
" DEFINED BEFORE flowTypeName so a PascalCase head in flow position keeps
" winning as Type (last-match-wins): flowTypeName, defined after, overlays it.
" NOTE: this is a purely LEXICAL heuristic (position between two arrows). True
" call-vs-binding resolution arrives with LSP semantic tokens (ADR-0008).
syn match flowFlowFn "\%(->\s*\)\@<=\h\w*\%(\s*->\)\@="

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
"
" KNOWN-LABEL NARROWING (the load-bearing fix). A jump edge `-> ident;` and a
" terminal binding/sink `-> ident;` are LEXICALLY IDENTICAL: `-> outer;` (jump,
" must be Label) and `-> out;` (binding, must be UNhighlighted) differ only in
" whether `ident` was DECLARED as a loop label (`ident {`) elsewhere in the
" buffer. A position-only regex cannot tell them apart, so the old rule
" (`-> \h\w* \ze ;`) wrongly painted every terminal binding as a label.
"
" We resolve this by SCANNING the buffer for label declarations and restricting
" the jump match to that exact set. A label declaration is a lowercase-initial
" identifier immediately followed by `{` (the same shape flowLabel highlights),
" EXCLUDING tokens that open a block for another reason: the `loop` keyword and
" primitive return types (`-> f64 {`, etc.). The harvested names are spliced
" into a single anchored alternation; only those identifiers become jump targets.
"   - `-> outer;` matches iff `outer {` was declared  -> flowLabelJump (Label).
"   - `-> out;`/`-> diff;`/`-> nr;`/`-> done;` are NOT declared labels -> no
"     match here, so they fall through to NOTHING and stay unhighlighted.
"   - `-> loop;` stays flowKeyword (the `loop` keyword outranks any match) and
"     `-> ret;` stays flowRet — both are intentionally not in the label set.
" A lookbehind (`\%(->\s*\)\@<=`) starts the match at the label word itself so it
" does not consume the `->` (flowArrow owns it — a match cannot start inside
" another match's region); `\ze\s*;` bounds it to a jump edge.
" NOTE: still a lexical heuristic over a single buffer. Cross-file / true
" target resolution arrives with LSP semantic tokens (ADR-0008).
function! s:FlowLabelNames() abort
  " Tokens that look like `ident {` but are NOT loop labels.
  let l:exclude = {'loop': 1, 'seq': 1, 'fn': 1, 'type': 1, 'mut': 1, 'void': 1,
        \ 'map': 1, 'fold': 1, 'print': 1, 'ret': 1, 'true': 1, 'false': 1,
        \ 'i32': 1, 'i64': 1, 'u8': 1, 'f32': 1, 'f64': 1, 'bool': 1}
  let l:seen = {}
  let l:names = []
  for l:line in getline(1, '$')
    " Strip line comments so a `foo {` inside `// ...` is not harvested.
    let l:code = substitute(l:line, '//.*$', '', '')
    let l:start = 0
    while 1
      let l:m = matchstrpos(l:code, '\<[a-z_]\w*\ze\s*{', l:start)
      if l:m[1] < 0
        break
      endif
      let l:start = l:m[2]
      let l:name = l:m[0]
      if !has_key(l:exclude, l:name) && !has_key(l:seen, l:name)
        let l:seen[l:name] = 1
        call add(l:names, l:name)
      endif
    endwhile
  endfor
  return l:names
endfunction

let s:flow_labels = s:FlowLabelNames()
if !empty(s:flow_labels)
  " Anchored word alternation of the declared label names, e.g.
  " \%(outer\|inner\)\>, so only real labels match as jump targets.
  let s:flow_label_alt = '\%(' . join(s:flow_labels, '\|') . '\)\>'
  execute 'syn match flowLabelJump "\%(->\s*\)\@<=' . s:flow_label_alt . '\ze\s*;"'
endif
" If no labels are declared in the buffer, flowLabelJump is intentionally never
" defined — every `-> ident;` is then a terminal binding and stays unhighlighted.

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
" SPLIT COLORING (CHANGE 1): the outer flowGuardArrow match links to Statement,
" so the CHROME (the leading `-` and the trailing `->`) looks identical to a
" plain flow arrow. The discriminant inside gets the color of WHAT IT IS, via
" CONTAINED overlay groups pulled in by `contains=` on each outer match:
"   flowGuardBool    (true|false)  -> Boolean
"   flowGuardInt     (\d\+)        -> Number
"   flowGuardWild    (_)           -> Special
"   flowGuardVariant (Some/None/…) -> Type   (inner binder names stay plain)
" These contained groups are defined just below and only match inside a guard.

" Boolean / default / integer-literal guards:
"   -true->  -false->  -_->  -0->  -42->
syn match flowGuardArrow "-\%(true\|false\|_\|\d\+\)->" contains=flowGuardBool,flowGuardInt,flowGuardWild

" Variant-style guards (user-guide §2.1, §3.4) — e.g. -Some(x)->, -None->,
" -Ok(...)-> ; the variant tag is a PascalCase identifier, with an optional
" parenthesized binder.
syn match flowGuardArrow "-[A-Z]\w*\%((\%([^()]*\))\)\?->" contains=flowGuardVariant

" Destructuring guards (user-guide §3.5): empty list and head/tail.
"   -[]->   -[head, ...tail]->  (the pattern head reads as a Type-ish shape)
syn match flowGuardArrow "-\[\%([^][]*\)\]->" contains=flowGuardVariant

" Contained discriminant overlays (only match inside a flowGuardArrow region).
"   Bool literal in a guard: the `true`/`false` token.
syn match flowGuardBool    contained "\<\%(true\|false\)\>"
"   Integer-literal guard: the digits (e.g. 0, 42).
syn match flowGuardInt     contained "\<\d\+\>"
"   Default/wildcard guard: the bare `_`.
syn match flowGuardWild    contained "_"
"   Variant constructor / destructuring pattern head: the PascalCase tag
"   (Some, None, Ok, …) or the `[` of a list pattern. Inner binder names (the
"   `x` in `Some(x)`, `head`/`tail` in `[head, ...tail]`) are NOT matched, so
"   they stay plain.
syn match flowGuardVariant contained "[A-Z]\w*\|\["

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
hi default link flowFlowFn     Function
hi default link flowLabel      Label
hi default link flowLabelJump  Label
hi default link flowOperator   Operator
hi default link flowArrow      Statement
" Guard CHROME reads as flow plumbing — same as flowArrow (CHANGE 1).
hi default link flowGuardArrow    Statement
" Guard DISCRIMINANT gets the color of what it IS (CHANGE 1).
hi default link flowGuardBool     Boolean
hi default link flowGuardInt      Number
hi default link flowGuardWild     Special
hi default link flowGuardVariant  Type

let b:current_syntax = "flow"
