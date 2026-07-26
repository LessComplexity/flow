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
GRAMMAR = ROOT / "editors/vscode/syntaxes/mapal.tmLanguage.json"
FIXTURE = ROOT / "editors/nvim/test/fixture.mapal"


def load_rules(grammar):
    """Flatten the top-level include list into ordered concrete match rules.

    A node carrying its own `match`/`begin` IS the rule; only a pure container is
    expanded via `patterns`. Getting that backwards drops begin/end regions entirely —
    the first version expanded the string node into its nested escape pattern and lost
    the string rule, so `"` matched nothing and the escape passed by accident.
    """
    repo, rules = grammar["repository"], []

    def add(r):
        if "match" in r:
            rules.append({**r, "_rx": re.compile(r["match"])})
        elif "begin" in r:
            # Single-line model: the region is begin..end, and its nested patterns are
            # overlaid inside that span (see tokenize).
            rules.append({
                **r,
                "_rx": re.compile(r["begin"] + r'(?:\\.|[^"\\])*' + r["end"]),
                "_sub": [{**q, "_rx": re.compile(q["match"])} for q in r.get("patterns", [])
                         if "match" in q],
            })

    for inc in grammar["patterns"]:
        node = repo[inc["include"][1:]]
        if "match" in node or "begin" in node:
            add(node)
        else:
            for r in node.get("patterns", []):
                add(r)
    return rules


def tokenize(line, rules):
    """TextMate's rule: earliest match wins, ties by listed order. Returns [(start, end, scope)]."""
    out, pos = [], 0
    while pos < len(line):
        best = None
        for rule in rules:
            m = rule["_rx"].search(line, pos)
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
                s_, e_ = m.span(gi)
            except IndexError:
                continue
            if s_ != -1 and cap.get("name"):
                out.append((s_, e_, cap["name"]))
                emitted = True
        if not emitted and rule.get("name"):
            out.append((start, end, rule["name"]))
        # Inside a begin/end region, its own patterns apply and are narrower, so they
        # win via scope_at's innermost-wins rule.
        for sub in rule.get("_sub", []):
            for sm in sub["_rx"].finditer(line, start, end):
                if sub.get("name"):
                    out.append((sm.start(), sm.end(), sub["name"]))
        pos = max(end, pos + 1)
    return out


def scope_at(line, col, rules):
    """Innermost (narrowest) covering scope wins — an escape inside a string is the
    escape, not the string."""
    covering = [(e - s, name) for s, e, name in tokenize(line, rules) if s <= col < e]
    return min(covering)[1] if covering else "«none»"


# (line, token, expected scope, expected nvim group) — the fourth column is the
# consistency contract: the two editors must not disagree about what a token IS.
CASES = [
    (2, "double", "entity.name.function.mapal", "flowFnName"),
    (3, "ret", "keyword.control.return.mapal", "flowRet"),
    (7, "println", "support.function.builtin.mapal", "flowBuiltin"),
    (11, "a", "variable.other.definition.mapal", "flowBinding"),
    (13, "double", "entity.name.function.call.mapal", "flowDeclaredFn"),
    (13, "b", "variable.other.definition.mapal", "flowBinding"),
    (14, "iota", "support.function.builtin.mapal", "flowBuiltin"),
    (14, "ti", "variable.other.definition.mapal", "flowBinding"),
    (15, "widen_f32", "support.function.builtin.mapal", "flowBuiltin"),
    (16, "cmp", "variable.other.definition.mapal", "flowBinding"),
    (18, "true", "constant.language.boolean.mapal", "flowGuardBool"),
    (20, "map", "support.function.builtin.mapal", "flowBuiltin"),
    (21, "Pixel", "entity.name.type.mapal", "flowTypeName"),
    (25, ":myloop", "keyword.control.label.mapal", "flowLabel"),
    (27, ":myloop", "keyword.control.label.mapal", "flowLabelJump"),
    (32, '"', "string.quoted.double.mapal", "flowString"),
    (32, "\\t", "constant.character.escape.mapal", "flowEscape"),
    (32, "print", "support.function.builtin.mapal", "flowBuiltin"),
    (33, "zip", "support.function.builtin.mapal", "flowBuiltin"),
    (33, "pairs", "variable.other.definition.mapal", "flowBinding"),
    (34, "enumerate", "support.function.builtin.mapal", "flowBuiltin"),
    (35, "<-", "keyword.operator.arrow.mapal", "flowArrow"),
    (38, "0", "constant.numeric.integer.mapal", "flowGuardInt"),
    (40, "_", "variable.language.wildcard.mapal", "flowGuardWild"),
    (41, "Some", "entity.name.type.mapal", "flowGuardVariant"),
    (42, "[", "entity.name.type.mapal", "flowGuardVariant"),
    # Guard CHROME by column (0-based). The discriminant scopes are also produced by the
    # plain number/boolean/type rules, so they do not test the guard rules at all —
    # breaking one left this suite passing. The leading `-` does discriminate: it falls
    # to keyword.operator.mapal as soon as the guard rule stops matching.
    (18, 8, "keyword.operator.arrow.mapal", "flowGuardArrow"),
    (38, 8, "keyword.operator.arrow.mapal", "flowGuardArrow"),
    (41, 8, "keyword.operator.arrow.mapal", "flowGuardArrow"),
]

# Which nvim group each scope family is allowed to correspond to. Enforces that the two
# grammars stay semantically aligned even though their scope names differ.
FAMILY = {
    "support.function.builtin.mapal": {"flowBuiltin"},
    "variable.other.definition.mapal": {"flowBinding"},
    "entity.name.function.mapal": {"flowFnName"},
    "entity.name.function.call.mapal": {"flowDeclaredFn", "flowFlowFn"},
    "keyword.control.return.mapal": {"flowRet"},
    "constant.language.boolean.mapal": {"flowGuardBool", "flowBoolean"},
    "entity.name.type.mapal": {"flowTypeName", "flowGuardVariant"},
    "keyword.control.label.mapal": {"flowLabel", "flowLabelJump"},
    "string.quoted.double.mapal": {"flowString"},
    "constant.character.escape.mapal": {"flowEscape"},
    "keyword.operator.arrow.mapal": {"flowArrow", "flowGuardArrow"},
    "constant.numeric.integer.mapal": {"flowGuardInt", "flowNumber"},
    "variable.language.wildcard.mapal": {"flowGuardWild"},
    "keyword.operator.arrow.mapal": {"flowArrow", "flowGuardArrow"},
}


def main():
    rules = load_rules(json.loads(GRAMMAR.read_text()))
    lines = FIXTURE.read_text().splitlines()
    fails = 0

    for lnum, tok, want, nvim_group in CASES:
        line = lines[lnum - 1]
        if isinstance(tok, int):  # explicit 0-based column
            got, tok = scope_at(line, tok, rules), f"col{tok}"
        else:
            # Word-initial tokens get a boundary; ":myloop", "<-", "[", '"', "\\t" do not.
            pat = (r"\b" + re.escape(tok) + r"\b") if re.match(r"\w", tok) else re.escape(tok)
            m = re.search(pat, line)
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
    vim_src = (ROOT / "editors/nvim/syntax/mapal.vim").read_text()
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
