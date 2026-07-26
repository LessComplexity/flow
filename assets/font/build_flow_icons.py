#!/usr/bin/env python3
"""Build FlowIcons.ttf — one glyph, the Flow mark, at U+F8F0.

Why a font at all: the Rust and C++ logos you see in a file tree are *font glyphs*.
Nerd Fonts ships those brand marks as characters, which is why they render as real
logos in a terminal. Flow's mark is in no font, so to get the same effect it has to
be put in one.

Why our own font instead of patching a Nerd Font: patching redistributes someone
else's font under their licence. A separate single-glyph font that we author can be
shipped freely, and every modern terminal supports font fallback, so it composes
with whatever Nerd Font the user already has.

The mark is drawn as FILLED outlines, not strokes — fonts have no stroke concept, so
each edge is a rectangle and each node ring is an outer contour plus a reversed inner
one. Circles are 32-gons: indistinguishable from round at icon sizes and far more
robust than curve approximation.

    python3 assets/font/build_flow_icons.py            # -> assets/font/FlowIcons.ttf
"""

import math
import pathlib
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

UPEM = 1000
CODEPOINT = 0xF8F0  # BMP private use; outside every Nerd Fonts v3 assigned range
GLYPH_NAME = "flowmark"

# Geometry, in font units, y-up. Mirrors assets/logo-icon.svg.
EDGE = 60  # edge thickness  (SVG stroke-width)
RING_OUT, RING_IN = 100, 45  # node ring radii
Y_TOP, Y_BOT, Y_MID = 660, 260, 460
X_IN, X_JOIN, X_OUT = 130, 470, 860


def rect(pen, x0, y0, x1, y1):
    """Axis-aligned filled rectangle, clockwise."""
    pen.moveTo((x0, y0))
    pen.lineTo((x0, y1))
    pen.lineTo((x1, y1))
    pen.lineTo((x1, y0))
    pen.closePath()


def circle(pen, cx, cy, r, clockwise=True, sides=32):
    """Filled disc as a regular polygon. Winding direction selects fill vs hole."""
    step = 2 * math.pi / sides
    pts = [
        (cx + r * math.cos(i * step), cy + r * math.sin(i * step))
        for i in range(sides)
    ]
    if not clockwise:
        pts.reverse()
    pen.moveTo(pts[0])
    for p in pts[1:]:
        pen.lineTo(p)
    pen.closePath()


def ring(pen, cx, cy, outer, inner):
    """Node: outer contour one way, inner the other, so nonzero winding leaves a hole."""
    circle(pen, cx, cy, outer, clockwise=True)
    circle(pen, cx, cy, inner, clockwise=False)


def draw_mark():
    pen = TTGlyphPen(None)
    half = EDGE // 2

    # Edges. They overlap at the junction on purpose: same winding direction, so
    # nonzero winding merges them into one shape with no seam.
    rect(pen, X_IN + RING_OUT - 10, Y_TOP - half, X_JOIN + half, Y_TOP + half)
    rect(pen, X_IN + RING_OUT - 10, Y_BOT - half, X_JOIN + half, Y_BOT + half)
    rect(pen, X_JOIN - half, Y_BOT - half, X_JOIN + half, Y_TOP + half)
    rect(pen, X_JOIN - half, Y_MID - half, X_OUT - RING_OUT + 10, Y_MID + half)

    # Nodes: two hollow inputs, one solid output — the value that leaves.
    ring(pen, X_IN, Y_TOP, RING_OUT, RING_IN)
    ring(pen, X_IN, Y_BOT, RING_OUT, RING_IN)
    circle(pen, X_OUT, Y_MID, RING_OUT, clockwise=True)

    return pen.glyph()


def main():
    out = pathlib.Path(__file__).parent / "FlowIcons.ttf"

    fb = FontBuilder(UPEM, isTTF=True)
    order = [".notdef", GLYPH_NAME]
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({CODEPOINT: GLYPH_NAME})

    empty = TTGlyphPen(None).glyph()
    fb.setupGlyf({".notdef": empty, GLYPH_NAME: draw_mark()})

    # Full-width advance so the glyph occupies one cell like other icon glyphs.
    fb.setupHorizontalMetrics({".notdef": (UPEM, 0), GLYPH_NAME: (UPEM, 0)})
    fb.setupHorizontalHeader(ascent=800, descent=-200)
    fb.setupNameTable(
        {
            "familyName": "FlowIcons",
            "styleName": "Regular",
            "uniqueFontIdentifier": "FlowIcons-Regular-0.1",
            "fullName": "FlowIcons Regular",
            "psName": "FlowIcons-Regular",
            "version": "Version 0.1",
            "manufacturer": "The Flow project",
            "licenseDescription": "Apache-2.0 WITH LLVM-exception",
        }
    )
    fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    fb.setupPost()

    fb.save(out)

    # Assert what we claim: the codepoint maps to a glyph with actual contours.
    from fontTools.ttLib import TTFont

    f = TTFont(out)
    name = f.getBestCmap().get(CODEPOINT)
    assert name == GLYPH_NAME, f"cmap missing U+{CODEPOINT:04X}: {name!r}"
    # 4 edge rects + 2 rings (each an outer AND a reversed inner contour) + 1 disc.
    ncontours = f["glyf"][GLYPH_NAME].numberOfContours
    assert ncontours == 9, f"expected 9 contours (4 edges + 2x2 ring + 1 disc), got {ncontours}"
    print(f"wrote {out} ({out.stat().st_size} bytes)")
    print(f"  U+{CODEPOINT:04X} -> {name}, {ncontours} contours")
    return 0


if __name__ == "__main__":
    sys.exit(main())
