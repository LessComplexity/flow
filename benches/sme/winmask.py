#!/usr/bin/env python3
"""Operand-window instrument for plan-s43 (backend-llvm §3). NOTHING HERE SHIPS.

Rewrites the k loop of the emitted SME panel kernel so the two k-derived operand
offsets are masked: the four loads then wrap inside a window of the chosen size.
Same instruction sequence, same `fmopa` count, same pack, same ZA read-out, same
output stores -- only the addresses' upper bits change.

    %aoff  = mul nuw nsw i64 %k, 32                 %aoff  = mul nuw nsw i64 %k, 32
    %apk   = gep float, ptr %ap, i64 %aoff    ==>   %aoffm = and i64 %aoff, <A_MASK>
    %boff  = mul nuw nsw i64 %k, %bn                %apk   = gep float, ptr %ap, i64 %aoffm
    %bk    = gep float, ptr %b, i64 %boff           %boff  = mul nuw nsw i64 %k, %bn
                                                    %boffm = and i64 %boff, <B_MASK>
                                                    %bk    = gep float, ptr %b, i64 %boffm

Masks are ELEMENT counts (the offsets index floats), and must be 2^n-1.
Masking only shrinks an offset, so `inbounds` is preserved.

A silent no-op patch is the worst possible failure here, so this exits non-zero
unless it finds EXACTLY ONE of each pattern inside the named function.

Usage: winmask.py <A_MASK> <B_MASK> [in.ll] [-o out.ll] [--sym mapal_sme_panel]
       (reads stdin / writes stdout when the paths are omitted)
"""
import argparse
import re
import sys

# Deliberately not anchored on the `32`: the A stride is ti*t, emitted from the
# realization's geometry, and hardcoding it would silently no-op on a re-tune.
A_PAT = re.compile(
    r"(?m)^(?P<i>[ \t]*)%aoff = (?P<mul>mul[^\n]* i64 %k, \d+)\n"
    r"[ \t]*%apk = getelementptr inbounds float, ptr %ap, i64 %aoff$"
)
B_PAT = re.compile(
    r"(?m)^(?P<i>[ \t]*)%boff = (?P<mul>mul[^\n]* i64 %k, %bn)\n"
    r"[ \t]*%bk = getelementptr inbounds float, ptr %b, i64 %boff$"
)


def body(ll: str, sym: str) -> tuple[int, int]:
    """(start, end) of the named define's body, or die."""
    m = re.search(r"(?m)^define\b[^\n]*@" + re.escape(sym) + r"\(", ll)
    if not m:
        sys.exit(f"winmask: no `define ... @{sym}(` in the module -- nothing to patch")
    end = ll.find("\n}\n", m.start())
    if end < 0:
        sys.exit(f"winmask: unterminated body for @{sym}")
    return m.start(), end


def patch_one(pat, ll, lo, hi, name, repl):
    hits = list(pat.finditer(ll, lo, hi))
    if len(hits) != 1:
        sys.exit(f"winmask: expected exactly 1 {name} match, found {len(hits)} -- refusing to emit an unpatched module")
    m = hits[0]
    return ll[:m.start()] + repl(m) + ll[m.end():], hi + len(repl(m)) - len(m.group(0))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("a_mask")
    ap.add_argument("b_mask")
    ap.add_argument("src", nargs="?", default="-")
    ap.add_argument("-o", "--out", default="-")
    ap.add_argument("--sym", default="mapal_sme_panel")
    args = ap.parse_args()

    for label, raw in (("A_MASK", args.a_mask), ("B_MASK", args.b_mask)):
        v = int(raw, 0)
        if v <= 0 or (v + 1) & v:
            sys.exit(f"winmask: {label}={raw} is not 2^n-1")

    ll = sys.stdin.read() if args.src == "-" else open(args.src).read()
    lo, hi = body(ll, args.sym)

    ll, hi = patch_one(
        A_PAT, ll, lo, hi, "A-stream",
        lambda m: f"{m['i']}%aoff = {m['mul']}\n"
                  f"{m['i']}%aoffm = and i64 %aoff, {int(args.a_mask, 0)}\n"
                  f"{m['i']}%apk = getelementptr inbounds float, ptr %ap, i64 %aoffm",
    )
    ll, _ = patch_one(
        B_PAT, ll, lo, hi, "B-stream",
        lambda m: f"{m['i']}%boff = {m['mul']}\n"
                  f"{m['i']}%boffm = and i64 %boff, {int(args.b_mask, 0)}\n"
                  f"{m['i']}%bk = getelementptr inbounds float, ptr %b, i64 %boffm",
    )

    if args.out == "-":
        sys.stdout.write(ll)
    else:
        open(args.out, "w").write(ll)
    print(f"winmask: @{args.sym} windowed  A&{int(args.a_mask, 0)}  B&{int(args.b_mask, 0)}", file=sys.stderr)


if __name__ == "__main__":
    main()
