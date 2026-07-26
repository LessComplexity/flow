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

Not published to the Marketplace, so build a `.vsix` and install it:

```sh
python3 editors/vscode/package-vsix.py
code   --install-extension editors/vscode/flow-lang-0.1.0.vsix   # or:
cursor --install-extension editors/vscode/flow-lang-0.1.0.vsix
```

Then restart the editor. Verify it actually registered — the message alone is not proof:

```sh
code --list-extensions | grep flow-lang
```

If the `code` command is not on your PATH, use the binary inside the app:
`"/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"`.

### Do not just copy the folder in

Dropping this directory (or a symlink to it) into `~/.vscode/extensions` **does not work** on
current VS Code or Cursor. They keep `extensions.json` in that directory as the authoritative
registry and do not scan for unregistered folders, so a hand-placed extension is ignored
**in silence** — no error, no icon, no highlighting, indistinguishable from a broken
extension. Only the CLI install writes the registry entry.

That is also why `package-vsix.py` exists: `vsce` is the normal way to build a `.vsix` and it
needs npm, whereas a `.vsix` is just a ZIP holding `extension.vsixmanifest`,
`[Content_Types].xml` and an `extension/` directory. The script builds one directly and
asserts the archive contains all three before claiming success.

### Iterating on the extension

A CLI install copies the files, so editing the repo afterwards changes nothing in the editor.
Either rebuild and reinstall:

```sh
python3 editors/vscode/package-vsix.py && code --install-extension editors/vscode/flow-lang-0.1.0.vsix --force
```

or launch a throwaway window that loads the source directly, which is better for grammar work:

```sh
code --extensionDevelopmentPath="$(pwd)/editors/vscode" .
```

### Checking it works

1. Open a `.flow` file — the status bar should read **Flow**, not *Plain Text*. If it says Plain
   Text the extension is not loaded and nothing else matters.
2. Command palette → `Developer: Inspect Editor Tokens and Scopes`, cursor on a token — should
   show `source.flow` and a scope such as `support.function.builtin.flow`.

## Syntax highlighting

`syntaxes/flow.tmLanguage.json` covers comments, strings, guard arrows (split-scoped — chrome
as an arrow, discriminant by what it is), labels, `fn` declarations, builtins, bindings, call
position, arrows, numbers, types, operators, and the reserved-and-rejected `category`.

Run `editors/test.sh` after any edit — it asserts both editors together.

**Three things to know if you edit it.**

The rule that decides outcomes is **earliest match wins**, ties broken by listed order — not
listed order alone. This bites specifically: a rule like `(?<=->)\s*(ident)` begins its match
at the *whitespace* after the arrow, one column before the keyword rule that matches the
identifier itself, so it wins on position however the patterns are ordered. That is why
`#binding` and `#call-position` carry an explicit negative lookahead listing every builtin and
`ret`; without it, `-> println;` scoped `println` as a variable. `scope_test.py` asserts that
exclusion covers every builtin, so adding one to `#keywords` and forgetting the exclusion fails
the test rather than quietly mis-colouring.

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
