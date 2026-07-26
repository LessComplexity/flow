# Flow — VS Code support

Puts the **actual Flow logo** on `.flow` files, plus comment and bracket behaviour.

```
package.json                 language registration + the file icon
language-configuration.json  // comments, bracket matching, auto-close
icons/flow.svg               the mark, square variant
```

## Why this and not an icon theme

VS Code has no "add one icon" API for file icon themes — a file icon theme is
**all-or-nothing**, so shipping a Flow-only one would blank the icons for every other file
type in your project. That is why this uses `contributes.languages[].icon` instead (VS Code
≥ 1.66): the icon shows for `.flow` files, and your existing icon theme keeps handling
everything else. If your active icon theme *does* define an icon for `.flow`, the theme wins —
that is by design.

## Install

Not published to the Marketplace. Load it from disk:

```sh
ln -s /path/to/flow/editors/vscode ~/.vscode/extensions/flow-lang
```

Then restart VS Code. (`~/.vscode-server/extensions/` for remote, `~/.cursor/extensions/` for
Cursor.)

## What it does not do

**No syntax highlighting.** That needs a TextMate grammar, which would be a second
implementation of the highlighting rules that already exist for Neovim in
[`../nvim/syntax/flow.vim`](../nvim/syntax/flow.vim) — two hand-maintained copies of the same
regexes, drifting apart. Not worth it before the real answer, which is one LSP server driving
both editors with the compiler's own parse (ADR-0008). Until then this extension is the icon
and the editing niceties only.
