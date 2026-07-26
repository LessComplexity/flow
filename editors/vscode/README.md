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
# The target MUST be absolute. `ln -s` resolves a relative target against the LINK's
# directory, not your shell's cwd, so `ln -s editors/vscode ...` silently creates a
# link to ~/.vscode/extensions/editors/vscode, which does not exist. A broken link is
# skipped in silence — the extension simply never appears.
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/flow-lang     # from the repo root

# Cursor:  ~/.cursor/extensions/
# Remote:  ~/.vscode-server/extensions/
```

Then **fully quit and reopen** the editor (Cmd-Q, not just closing the window). Verify:

```sh
[ -f ~/.vscode/extensions/flow-lang/package.json ] && echo OK || echo BROKEN LINK
```

Two checks inside the editor, in order:

1. Open a `.flow` file — the status bar should read **Flow**, not *Plain Text*. If it says
   Plain Text, the extension is not loaded at all.
2. `Developer: Inspect Editor Tokens and Scopes` from the command palette, cursor on a
   token — the *textmate scopes* line should show `source.flow`. If the language is Flow but
   scopes show only `source.flow` with no token scope, the grammar loaded but that token has
   no rule.

If the language mode is still Plain Text after a full restart, copy instead of symlinking —
some setups will not scan a symlinked extension directory:

```sh
rm -f ~/.vscode/extensions/flow-lang
cp -R editors/vscode ~/.vscode/extensions/flow-lang
```

## Syntax highlighting

`syntaxes/flow.tmLanguage.json` covers comments, strings, guard arrows (split-scoped — chrome
as an arrow, discriminant by what it is), labels, `fn` declarations, builtins, bindings, call
position, arrows, numbers, types, operators, and the reserved-and-rejected `category`.

**Two things to know if you edit it.**

It has the **opposite precedence** to the Neovim file: TextMate takes the *first* matching
pattern, Vim the *last*. So this file is ordered most-specific first and
[`../nvim/syntax/flow.vim`](../nvim/syntax/flow.vim) least-specific first. A rule added to one
goes at the other end of the other.

And it is **less capable in one specific way**: `x -> name;` is lexically identical whether
`name` is a new variable or a call to a no-value function. The Vim file resolves it by scanning
the buffer for `fn` declarations; TextMate has no way to know what has been declared, so here
every `-> name;` reads as a binding. A terminal call to your own function will be coloured as a
variable.

That divergence is the argument for not maintaining two grammars forever. The real fix is one
LSP server driving both editors from the compiler's own parse (ADR-0008), at which point both
hand-written grammars become fallbacks.
