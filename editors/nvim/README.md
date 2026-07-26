# Flow — Neovim support

Regex syntax highlighting and filetype detection for `*.flow`. Best-effort and
non-authoritative: a tree-sitter grammar is deferred until it can be derived from the real
`flow-syntax` parser (ADR-0008).

```
ftdetect/flow.vim        *.flow -> filetype=flow
syntax/flow.vim          highlighting
lua/flow/icon.lua        file icon registration (optional)
test/run.sh              assert the highlighting decisions
```

Groups link to standard groups via `hi default link`, so your colorscheme drives the actual
colours and can override any of them.

## Install

**lazy.nvim**

```lua
{ dir = "/path/to/flow/editors/nvim", ft = "flow" }
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

Neovim's icon providers take a **font glyph, not an image**, so this is a Nerd Font character
in the logo's teal rather than the actual SVG. For the real logo on `.flow` files, see
[`../vscode/`](../vscode/).

```lua
require("flow.icon").setup()                 -- nvim-web-devicons and/or mini.icons
require("flow.icon").setup({ glyph = "" })  -- override if your font lacks U+F0E8
```

Needs a [Nerd Font](https://www.nerdfonts.com/). Each provider is skipped silently when not
installed, so this is safe to call unconditionally.

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

Guard arrows are split-coloured on purpose: the **chrome** (leading `-`, trailing `->`) reads
as flow plumbing, identical to a plain arrow, while the **discriminant** inside gets the colour
of what it is — `true`/`false` as `Boolean`, `42` as `Number`, `_` as `Special`, `Some` as
`Type`.

### The one genuinely ambiguous case

`x -> name;` is lexically identical whether `name` is a **new variable** or a **call to a
no-value function**. Nothing in the token stream separates them. The syntax file resolves it
by scanning the buffer for `fn <name>` declarations and treating only those as calls;
everything else is a binding. Re-scanned on `BufEnter`, `BufWritePost`, `TextChanged` and
`InsertLeave`, so a newly written `fn` starts colouring its call sites immediately.

A correct answer needs the compiler's own name resolution — LSP semantic tokens (ADR-0008).
This is a heuristic that is right for every program in `examples/`.

## Tests

```sh
editors/test.sh              # both editors
editors/nvim/test/run.sh     # this one only
```

Asserts the group each token actually resolves to, and that no file in `examples/` lights up
as an error. Exits non-zero on failure.

Worth running after **any** edit to `syntax/flow.vim`. That file depends on Vim's
last-match-wins rule and its header warns against reordering; the test pins outcomes rather
than ordering, so a reorder that changes behaviour fails loudly instead of silently
mis-painting code. It has already earned its place twice — it caught `iota` being mis-painted
as a user function, and a false failure of its own from a substring match.
