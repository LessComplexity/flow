# Flow syntax highlighting for Neovim

A self-contained Neovim plugin that adds filetype detection and syntax
highlighting for Flow source files (`*.flow`).

Flow is the dataflow language whose surface syntax denotes its compiler graph IR
(`data -> f -> g -> ret;` *is* three nodes and two edges). This plugin highlights
the Flow-Core surface: flow arrows, guard arrows, the `type`/`fn`/`loop`/`seq`
keywords, loop labels and label jumps, primitive and named types, builtins
(`map`/`fold`/`print`), and the reserved-and-rejected `category` keyword.

## What it is

- `ftdetect/flow.vim` — sets the `flow` filetype on `BufRead`/`BufNewFile` for
  any `*.flow` file.
- `syntax/flow.vim` — a classic Vimscript syntax file (regex-based).

Highlight groups link to standard groups (`Statement`, `Conditional`, `Keyword`,
`Type`, `Function`, `String`, `Number`, `Float`, `Comment`, `Label`, `Special`,
`Error`) via `hi default link`, so your colorscheme drives the actual colors and
can override any of them.

Design highlights:

- **`->` / `<-`** (`flowArrow`) link to `Statement` — composition *is* the
  language, so flow arrows are deliberately prominent.
- **Guard arrows** (`flowGuardArrow`) — `-true->`, `-false->`, `-_->`,
  integer guards (`-0->`, `-42->`), variant guards (`-Some(x)->`, `-None->`),
  and destructuring guards (`-[]->`, `-[head, ...tail]->`) link to
  `Conditional`. They are defined *after* the plain arrows and the `-` operator
  so Vim's "last match wins" rule lets the full guard win.
- **`ret`** links to `Special` (the graph sink); loop-label declarations and
  `-> label;` jump targets link to `Label`.
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
