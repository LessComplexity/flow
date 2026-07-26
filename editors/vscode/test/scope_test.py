#!/usr/bin/env python3
"""Assert the TextMate grammar's scopes, and that they agree with the Neovim file.

    python3 editors/vscode/test/scope_test.py

Why a hand-rolled driver: the real tokenizer is vscode-textmate, which needs npm. This
models the one rule that actually decides outcomes for a flat pattern list —

    at each scan position, the winning pattern is the one whose match starts EARLIEST;
    ties are broken by the order patterns are listed.

That "earliest" clause is the whole game, and getting it wrong is not academic: the
first version of this test simply asked "does any pattern, in listed order, cover this
column?", which silently passed a grammar where `-> println;` scoped `println` as a
variable. A binding rule written as `(?<=->)\\s*(ident)` begins its match at the
whitespace after the arrow — one column EARLIER than the keyword rule that matches the
identifier itself — so it won on position no matter how the patterns were ordered.
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
GRAMMAR = ROOT / "editors/vscode/syntaxes/flow.tmLanguage.json"
FIXTURE = ROOT / "editors/nvim/test/fixture.flow"


def load_rules(grammar):
    """Flatten the top-level include list into ordered concrete match rules."""
    repo, rules = grammar["repository"], []
    for inc in grammar["patterns"]:
        node = repo[inc["include"][1:]]
        for r in node.get("patterns", [node]):
            if "match" in r:
                rules.append(r)
            elif "begin" in r:  # strings: good enough for a single-line fixture
                rules.append({"match": r["begin"] + r'[^"]*' + r["end"], "name": r.get("name")})
    return [(re.compile(r["match"]), r) for r in rules]


def tokenize(line, rules):
    """TextMate's rule: earliest match wins, ties by listed order. Returns [(start, end, scope)]."""
    out, pos = [], 0
    while pos < len(line):
        best = None  # (start, end, rule)
        for rx, rule in rules:
            m = rx.search(line, pos)
            if m and m.end() > m.start():
                if best is None or m.start() < best[0]:
                    best = (m.start(), m.end(), rule, m)
        if best is None:
            break
        start, end, rule, m = best
        caps = rule.get("captures", {})
        emitted = False
        for gi_s, cap in sorted(caps.items(), key=lambda kv: int(kv[0])):
            gi = int(gi_s)
            if gi == 0:
                continue
            try:
                s, e = m.span(gi)
            except (IndexError, error := Exception):  # noqa: F841
                continue
            if s != -1 and cap.get("name"):
                out.append((s, e, cap["name"]))
                emitted = True
        if not emitted and rule.get("name"):
            out.append((start, end, rule["name"]))
        pos = max(end, pos + 1)
    return out


def scope_at(line, col, rules):
    for s, e, name in tokenize(line, rules):
        if s <= col < e:
            return name
    return "«none»"


# (line, token, expected scope, expected nvim group) — the fourth column is the
# consistency contract: the two editors must not disagree about what a token IS.
CASES = [
    (2, "double", "entity.name.function.flow", "flowFnName"),
    (3, "ret", "keyword.control.return.flow", "flowRet"),
    (7, "println", "support.function.builtin.flow", "flowBuiltin"),
    (11, "a", "variable.other.definition.flow", "flowBinding"),
    (13, "double", "entity.name.function.call.flow", "flowDeclaredFn"),
    (13, "b", "variable.other.definition.flow", "flowBinding"),
    (14, "iota", "support.function.builtin.flow", "flowBuiltin"),
    (14, "ti", "variable.other.definition.flow", "flowBinding"),
    (15, "widen_f32", "support.function.builtin.flow", "flowBuiltin"),
    (16, "cmp", "variable.other.definition.flow", "flowBinding"),
    (18, "true", "constant.language.boolean.flow", "flowGuardBool"),
    (20, "map", "support.function.builtin.flow", "flowBuiltin"),
    (21, "Pixel", "entity.name.type.flow", "flowTypeName"),
    (25, ":myloop", "keyword.control.label.flow", "flowLabel"),
    (27, ":myloop", "keyword.control.label.flow", "flowLabelJump"),
]

# Which nvim group each scope family is allowed to correspond to. Enforces that the two
# grammars stay semantically aligned even though their scope names differ.
FAMILY = {
    "support.function.builtin.flow": {"flowBuiltin"},
    "variable.other.definition.flow": {"flowBinding"},
    "entity.name.function.flow": {"flowFnName"},
    "entity.name.function.call.flow": {"flowDeclaredFn", "flowFlowFn"},
    "keyword.control.return.flow": {"flowRet"},
    "constant.language.boolean.flow": {"flowGuardBool", "flowBoolean"},
    "entity.name.type.flow": {"flowTypeName", "flowGuardVariant"},
    "keyword.control.label.flow": {"flowLabel", "flowLabelJump"},
}


def main():
    rules = load_rules(json.loads(GRAMMAR.read_text()))
    lines = FIXTURE.read_text().splitlines()
    fails = 0

    for lnum, tok, want, nvim_group in CASES:
        line = lines[lnum - 1]
        # A leading `:` has no word boundary before it, so anchor on the token itself.
        m = re.search(re.escape(tok) + r"(?![\w])", line)
        got = scope_at(line, m.start(), rules) if m else "«no token»"
        ok = got == want
        if not ok:
            fails += 1
        print(f"{'ok  ' if ok else 'FAIL'} {lnum}:{tok:<10} {got}")
        if ok and want in FAMILY and nvim_group not in FAMILY[want]:
            fails += 1
            print(f"     INCONSISTENT: {want} should map to one of "
                  f"{sorted(FAMILY[want])}, nvim says {nvim_group}")

    # The builtin list must match the Vim file's exactly, or a name will be highlighted
    # in one editor and mis-scoped in the other.
    grammar_src = GRAMMAR.read_text()
    builtins_tm = set(re.search(
        r'"match": "\\\\b\((map\|[^)]+)\)\\\\b"', grammar_src).group(1).split("|"))
    vim_src = (ROOT / "editors/nvim/syntax/flow.vim").read_text()
    builtins_vim = set()
    for line in vim_src.splitlines():
        if line.startswith("syn keyword flowBuiltin"):
            builtins_vim |= set(line.split()[3:])
    if builtins_tm != builtins_vim:
        fails += 1
        print(f"\nFAIL builtin lists differ:\n  only in TextMate: {builtins_tm - builtins_vim}"
              f"\n  only in Vim:      {builtins_vim - builtins_tm}")
    else:
        print(f"\nok   builtin lists identical across both editors ({len(builtins_tm)} names)")

    # The negative lookahead in #binding / #call-position must cover every builtin and
    # `ret`, or that name gets scoped as a variable/call. This is the assertion that
    # keeps the duplicated list from rotting: add a builtin to #keywords and forget the
    # exclusion, and this fails instead of the colour quietly going wrong.
    grammar = json.loads(grammar_src)
    must_exclude = builtins_tm | {"ret", "loop", "true", "false"}
    for rule_name in ("binding", "call-position"):
        pat = grammar["repository"][rule_name]["match"]
        excl = re.search(r"\(\?!\(\?:([^)]+)\)", pat)
        if not excl:
            fails += 1
            print(f"FAIL #{rule_name} has no keyword exclusion — it will swallow builtins")
            continue
        listed = set(excl.group(1).split("|"))
        missing = must_exclude - listed
        if missing:
            fails += 1
            print(f"FAIL #{rule_name} exclusion is missing: {sorted(missing)}")
        else:
            print(f"ok   #{rule_name} excludes all {len(must_exclude)} reserved names")

    print("all scope assertions passed" if not fails else f"{fails} failure(s)")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
