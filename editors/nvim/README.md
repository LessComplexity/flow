# Flow syntax highlighting for Neovim

A self-contained Neovim plugin that adds filetype detection and syntax
highlighting for Flow source files (`*.flow`).

Flow is the dataflow language whose surface syntax denotes its compiler graph IR
(`data -> f -> g -> ret;` *is* three nodes and two edges). This plugin highlights
the Flow-Core surface: flow arrows, guard arrows, the `type`/`fn`/`loop`/`seq`
keywords, loop labels and label jumps, primitive and named types, builtins
(`map`/`fold`/`print`/`println`), and the reserved-and-rejected `category`
keyword.

## What it is

- `ftdetect/flow.vim` — sets the `flow` filetype on `BufRead`/`BufNewFile` for
  any `*.flow` file.
- `syntax/flow.vim` — a classic Vimscript syntax file (regex-based).

Highlight groups link to standard groups (`Statement`, `Keyword`, `Type`,
`Function`, `String`, `Number`, `Float`, `Boolean`, `Operator`, `Comment`,
`Label`, `Special`, `Error`) via `hi default link`, so your colorscheme drives
the actual colors and can override any of them.

Design highlights:

- **`->` / `<-`** (`flowArrow`) link to `Statement` — composition *is* the
  language, so flow arrows are deliberately prominent.
- **Guard arrows** — split coloring. The **chrome** (the leading `-` and the
  trailing `->`, group `flowGuardArrow`) links to `Statement`, identical to a
  plain flow arrow, so a guard reads as flow plumbing. The **discriminant**
  inside gets the color of *what it is*, via contained overlay groups:
  `true`/`false` (`flowGuardBool`) → `Boolean`; integer guards `-0->`/`-42->`
  (`flowGuardInt`) → `Number`; the default `_` (`flowGuardWild`) → `Special`;
  variant and destructuring pattern heads `-Some(x)->`/`-None->`/`-[…]->`
  (`flowGuardVariant`) → `Type` (inner binder names stay plain). Guards are
  defined *after* the plain arrows and the `-` operator so Vim's "last match
  wins" rule lets the full guard win; the contained groups overlay only the
  discriminant span.
- **Functions in flows** — an identifier that sits *between two arrows*
  (`-> clamp ->`, `data -> f -> g ->`; group `flowFlowFn`) links to `Function`.
  This is a lexical heuristic: `syn keyword` builtins/keywords (`map`, `fold`,
  `print`, `println`, `ret`, `loop`, …) outrank it automatically, and it is
  defined before `flowTypeName` so a PascalCase head still wins as `Type`.
  Terminal bindings and sinks (`-> nr;`, `-> total_r;`) have no trailing `->`,
  so this rule does *not* fire on them.
- **Terminal calls to no-value functions** (`flowDeclaredFn`) — a no-value
  function `fn somefn() { … }` (no `-> Ret`) is invoked terminally as
  `data -> somefn;`, which is lexically identical to a terminal binding
  `-> result;`, so the rule above leaves both uncolored. To color the *call*
  without painting every binding, the plugin scans the buffer for `fn <name>`
  declarations and highlights only those names in call position (after `->`),
  covering both `-> somefn;` and `-> somefn ->`. It re-runs on load and edits
  (`BufEnter`/`TextChanged`/`InsertLeave`/`BufWritePost`) so new functions are
  picked up — the same buffer-scan shape labels used before sigils. A true
  call-vs-binding answer still awaits LSP semantic tokens (ADR-0008).
- **`ret`** links to `Special` (the graph sink); **labeled blocks and jumps**
  (ADR-0012) link to `Label`. Labels carry a prefix `:` sigil on both ends —
  declaration `:outer { … }`, jump `-> :outer;` — so both forms are
  self-identifying by regex alone: a terminal binding `-> out;` has no sigil
  and stays unhighlighted, and the whole `:ident` (sigil included) is the
  Label. The buffer-scanning "known-label narrowing" hack this plugin used to
  need (un-sigiled jumps were lexically identical to bindings) is gone — the
  sigil killed the ambiguity it worked around. Un-sigiled `outer {` is no
  longer a label form (the compiler reads statement-initial `Ident {` as a
  struct literal and hints at the sigiled spelling). The back-edge `-> loop;`
  stays a keyword and `-> ret;` stays the `ret` sink. Labels are Core+1
  surface (rejected with P0110 today) but are the decided spelling, exhibited
  by the LC-3-patched spec.
- **`category`** links to `Error` — it is reserved-and-rejected under ADR-0006
  (errata E5: the type keyword is `type`). The editor teaches E5 by flagging it.

> **Note.** Highlighting is regex-based today. A tree-sitter grammar is deferred
> until it can be derived from the real `flow-syntax` parser (ADR-0008).

## Install

### (a) lazy.nvim (local plugin)

```lua
{
  dir = "/Users/lesscomplex/Personal/Flow/editors/nvim",
  name = "flow.nvim",
  ft = "flow",
}
```

### (b) Plain `runtimepath` append in `init.lua`

```lua
vim.opt.runtimepath:append("/Users/lesscomplex/Personal/Flow/editors/nvim")
```

(Equivalent Vimscript for `init.vim`:
`set runtimepath+=/Users/lesscomplex/Personal/Flow/editors/nvim`.)

> **lazy.nvim users: use (a), not (b).** lazy.nvim resets `runtimepath` during
> startup, silently discarding manual appends (and `--cmd 'set rtp+=…'` from the
> command line, for the same reason). The `dir =` plugin spec in (a) is the
> supported path.

### (c) Quick try-out (no config changes)

```sh
nvim -u NONE --cmd 'set rtp+=/Users/lesscomplex/Personal/Flow/editors/nvim' \
  --cmd 'filetype on' --cmd 'syntax on' \
  /Users/lesscomplex/Personal/Flow/examples/sepia.flow
```

(`-u NONE` skips your config but also disables filetype detection, hence the
explicit `filetype on` / `syntax on`.) Run `:set filetype?` inside nvim — it
should report `filetype=flow`.

## Layout

```
editors/nvim/
├── README.md
├── ftdetect/
│   └── flow.vim     # *.flow -> filetype=flow
└── syntax/
    └── flow.vim     # regex syntax definitions
```
