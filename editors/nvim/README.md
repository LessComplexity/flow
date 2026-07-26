# Mapal — Neovim support

Regex syntax highlighting and filetype detection for `*.mapal`. Best-effort and
non-authoritative: a tree-sitter grammar is deferred until it can be derived from the real
`mapal-syntax` parser (ADR-0008).

```
ftdetect/mapal.vim        *.mapal -> filetype=flow
syntax/mapal.vim          highlighting
plugin/mapal_icon.lua     registers the file icon automatically, adds :MapalIcon
lua/mapal/icon.lua        icon module (glyph, color, diagnostics)
test/run.sh              assert the highlighting decisions
```

Groups link to standard groups via `hi default link`, so your colorscheme drives the actual
colors and can override any of them.

## Install

**lazy.nvim**

```lua
{
  dir = "/path/to/flow/editors/nvim",
  lazy = false, -- NOT ft = "mapal": the icon must register at startup, see below
  -- optional, for the real logo instead of a stock glyph:
  -- config = function() local i = require("mapal.icon"); i.setup({ glyph = i.logo }) end,
}
```

**packer**

```lua
use { "/path/to/flow/editors/nvim" }
```

**No plugin manager** — add the directory to your runtimepath:

```lua
vim.opt.runtimepath:append("/path/to/flow/editors/nvim")
```

## File icon

**Registers itself** — no `setup()` call needed. Works with `mini.icons` (the LazyVim default)
and `nvim-web-devicons`; whichever is present.

**`lazy = false` is required**, not optional. A file tree showing a `.mapal` file has no `.mapal`
buffer open, so an `ft = "mapal"` spec never loads the plugin and the icon never appears.

```vim
:MapalIcon
```

reports what actually registered, which glyph, whether the font is installed, and — if nothing
registered — the likely reason.

### Getting the real logo, not an approximation

The Rust and C++ marks in a file tree are **font glyphs**: Nerd Fonts ships those brand logos as
characters. Providers take a glyph, not an image, so the only way to get Mapal's own mark is to
put it in a font — which [`../../assets/font/`](../../assets/font/) does, one glyph at U+F8F0.

```lua
local icon = require("mapal.icon")
icon.setup({ glyph = icon.logo })   -- needs MapalIcons.ttf + a terminal fallback mapping
```

Install steps and per-terminal fallback config are in
[`assets/font/README.md`](../../assets/font/README.md). Without it the default is the closest
glyph your Nerd Font already has, so nothing breaks — you just do not get the real mark.

Needs a [Nerd Font](https://www.nerdfonts.com/) either way.

## What gets highlighted

| Construct                                                                       | Group                        | Links to                                  |
| ------------------------------------------------------------------------------- | ---------------------------- | ----------------------------------------- |
| `fn` `type` `loop` `seq` `mut` `void`                                           | `flowKeyword`                | `Keyword`                                 |
| `map` `fold` `zip` `enumerate` `iota` `fill` `widen_*` `time` `print` `println` | `flowBuiltin`                | `Function`                                |
| `ret`                                                                           | `flowRet`                    | `Special`                                 |
| `i32 i64 u8 f32 f64 bool`, `PascalCase`                                         | `flowPrimType`/`flowTypeName` | `Type`                                    |
| name after `fn`                                                                 | `flowFnName`                 | `Function`                                |
| **`x -> name;`** — binds a variable                                             | **`flowBinding`**            | `Identifier`                              |
| **`x -> myfn;`** / **`-> myfn ->`** — calls a declared `fn`                     | **`flowDeclaredFn`**         | `Function`                                |
| `-> f ->` — call position                                                       | `flowFlowFn`                 | `Function`                                |
| `->` `<-`                                                                       | `flowArrow`                  | `Statement` — composition *is* the language |
| `-true->` `-42->` `-Some(x)->`                                                  | `flowGuardArrow` + contained | chrome `Statement`, discriminant by kind  |
| `:label { }` and `-> :label;`                                                   | `flowLabel`                  | `Label`                                   |
| `category`                                                                      | `flowReserved`               | `Error` — reserved-and-rejected (ADR-0006) |

Guard arrows are split-colored on purpose: the **chrome** (leading `-`, trailing `->`) reads
as flow plumbing, identical to a plain arrow, while the **discriminant** inside gets the color
of what it is — `true`/`false` as `Boolean`, `42` as `Number`, `_` as `Special`, `Some` as
`Type`.

### The one genuinely ambiguous case

`x -> name;` is lexically identical whether `name` is a **new variable** or a **call to a
no-value function**. Nothing in the token stream separates them. The syntax file resolves it
by scanning the buffer for `fn <name>` declarations and treating only those as calls;
everything else is a binding. Re-scanned on `BufEnter`, `BufWritePost`, `TextChanged` and
`InsertLeave`, so a newly written `fn` starts coloring its call sites immediately.

A correct answer needs the compiler's own name resolution — LSP semantic tokens (ADR-0008).
This is a heuristic that is right for every program in `examples/`.

## Tests

```sh
editors/test.sh              # both editors
editors/nvim/test/run.sh     # this one only
```

Asserts the group each token actually resolves to, and that no file in `examples/` lights up
as an error. Exits non-zero on failure.

Worth running after **any** edit to `syntax/mapal.vim`. That file depends on Vim's
last-match-wins rule and its header warns against reordering; the test pins outcomes rather
than ordering, so a reorder that changes behavior fails loudly instead of silently
mis-painting code. It has already earned its place twice — it caught `iota` being mis-painted
as a user function, and a false failure of its own from a substring match.
