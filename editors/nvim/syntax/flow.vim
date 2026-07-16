" Vim syntax file
" Language:    Flow (dataflow language — flow-lang Flow-Core subset, v0.3)
" Maintainer:  Flow project (editors/nvim)
" Reference:   docs/spec/user-guide.md §3 (as patched: E4 statement rule,
"              E5 `type` keyword, LC-3 sigiled labels), HANDOFF.md §4.1
"              (Flow-Core scope), ADR-0012 (labeled blocks `:label { … }`,
"              jumps `-> :label;`).
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

" Builtins: map/fold are inline-block collection ops; print/println are the
" effects (ADR-0015 — `print` raw, `println` appends a newline).
syn keyword flowBuiltin map fold print println

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
" Labeled blocks and label jumps (ADR-0012; user-guide §3.5/§8.5 as patched
" by LC-3)
" ---------------------------------------------------------------------------
" Labels carry a prefix `:` sigil on BOTH ends: declaration `:outer { … }`,
" jump `-> :outer;`. The sigil makes both forms self-identifying, so the old
" buffer-scanning "known-label narrowing" machinery that used to live here is
" gone — it existed precisely because un-sigiled jumps (`-> outer;`) were
" lexically identical to terminal bindings (`-> out;`). Under ADR-0012:
"   - `-> out;` (binding/sink)  — no sigil — stays unhighlighted.  ✓ by regex
"   - `-> :outer;` (jump)       — sigil — Label.
"   - `outer { … }` un-sigiled  — no longer a label form at all (the compiler
"     treats statement-initial `Ident {` as a struct literal and hints at the
"     sigiled spelling, P0110) — intentionally NOT highlighted as Label here.
"   - `-> loop;` stays flowKeyword; `-> ret;` stays flowRet (keywords outrank).
" The whole `:ident` (sigil included) is the Label — the sigil is the
" construct's identity.
"
" Both rules anchor on the `:` and exclude `::` (the out-of-Core path syntax,
" e.g. `List::map`) via a negative lookbehind, so the second colon of a `::`
" never starts a label match. Labels are written tight (`:outer`, no interior
" space — spec style per the LC-3 exhibits); ascriptions/struct fields put the
" colon AFTER the identifier (`x: i32`, `r: 200.0`), so the two never collide.
"
" Declaration: `:label` immediately followed by (optional space and) `{`.
syn match flowLabel ":\@<!:\h\w*\ze\s*{"
" Jump: `:label` after a flow arrow, bounded by `;`. The lookbehind
" (`\%(->\s*\)\@<=`) starts the match at the `:` so it does not consume the
" `->` (flowArrow owns it — a match cannot start inside another match's region).
syn match flowLabelJump "\%(->\s*\)\@<=:\@<!:\h\w*\ze\s*;"
" NOTE: labels are Core+1 — the compiler currently rejects them with P0110.
" They are highlighted (not error-flagged) because they are the decided
" surface (ADR-0012), exhibited by the patched spec; the plugin is
" best-effort/non-authoritative (ADR-0008).

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
hi default link flowDeclaredFn Function
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

" ---------------------------------------------------------------------------
" Terminal calls to no-value functions: `-> somefn;`
" ---------------------------------------------------------------------------
" A no-value function — `fn somefn() { … }` (no `-> Ret`; `void` is Core+1,
" P0113) — is invoked TERMINALLY: `data -> somefn;`. That is lexically identical
" to a terminal binding/sink (`-> result;`), so flowFlowFn above deliberately
" requires a trailing `->` and leaves both uncolored. To paint terminal CALLS
" without painting every BINDING, scan the buffer for `fn <name>` declarations
" and highlight only those names in call position (after `->`) — covering both
" `-> somefn;` and `-> somefn ->`. Re-run on load/edits so new fns are picked up.
" (Pure regex cannot tell a call from a binding; a true answer awaits LSP
" semantic tokens, ADR-0008. This is the same buffer-scan shape the label
" machinery used before ADR-0012's sigils made it unnecessary for labels.)
function! s:FlowHighlightDeclaredFns() abort
  silent! syntax clear flowDeclaredFn
  let l:names = {}
  for l:line in getline(1, '$')
    let l:name = matchstr(l:line, '\<fn\s\+\zs\h\w*')
    if l:name !=# ''
      let l:names[l:name] = 1
    endif
  endfor
  for l:name in keys(l:names)
    " `\h\w*` fn names are regex-safe to interpolate verbatim.
    execute 'syntax match flowDeclaredFn "\%(->\s*\)\@<=\<' . l:name . '\>"'
  endfor
endfunction

augroup flowDeclaredFns
  autocmd! * <buffer>
  autocmd BufEnter,BufWritePost,TextChanged,InsertLeave <buffer>
        \ call <SID>FlowHighlightDeclaredFns()
augroup END
call s:FlowHighlightDeclaredFns()

let b:current_syntax = "flow"
