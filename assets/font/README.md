# MapalIcons.ttf — the logo as a font glyph

The Rust and C++ logos you see in a Neovim file tree or bufferline are **font glyphs**.
Nerd Fonts ships those brand marks as characters, which is why they render as real logos in a
terminal. Terminals draw text, so an SVG is not an option — to get Mapal's mark the same way,
the mark has to be in a font.

This is a **single-glyph font** containing the Mapal mark at **U+F8F0**.

Built by [`build_mapal_icons.py`](build_mapal_icons.py) with `fontTools`.

## Why not patch a Nerd Font

Patching redistributes someone else's font under their license. A separate font we author can
be shipped freely, and every modern terminal does font fallback — so this composes with
whatever Nerd Font you already use instead of replacing it.

## Install

```sh
# macOS
cp assets/font/MapalIcons.ttf ~/Library/Fonts/

# Linux
mkdir -p ~/.local/share/fonts && cp assets/font/MapalIcons.ttf ~/.local/share/fonts/ && fc-cache -f
```

Then add it as a **fallback** after your main font:

| terminal    | setting                                                              |
| ----------- | -------------------------------------------------------------------- |
| Kitty       | `symbol_map U+F8F0 MapalIcons`                                         |
| WezTerm     | `font = wezterm.font_with_fallback { "Your Font", "MapalIcons" }`       |
| Ghostty     | `font-family = MapalIcons` (listed after your main family)              |
| Alacritty   | no per-range fallback; relies on the system font fallback chain        |
| iTerm2      | Preferences → Profiles → Text → *Use a different font for non-ASCII*   |

Then point Neovim at it:

```lua
local icon = require("mapal.icon")
icon.setup({ glyph = icon.logo })
```

Without the font installed, `icon.setup()` uses the closest glyph your Nerd Font already has,
so nothing breaks — you just do not get the real mark.

## Rebuild

```sh
python3 assets/font/build_mapal_icons.py
```

The script asserts what it claims on the way out: that U+F8F0 maps to the glyph, and that the
glyph has 9 contours (4 edge rectangles, 2 node rings at 2 contours each for the hole, 1 solid
output disc). That assertion has already caught one miscount.

## Design notes

Fonts have no stroke concept, so the mark is drawn as filled outlines: each edge is a
rectangle, each hollow node is an outer contour plus a reversed inner one so nonzero winding
leaves the hole. Circles are 32-gons — indistinguishable from round at icon size and far more
robust than approximating curves.

U+F8F0 sits in the BMP private-use area, outside every range Nerd Fonts v3 assigns. Private
use means no codepoint is ever formally "safe"; if it collides with something in your setup,
change `CODEPOINT` in the script and `M.logo` in
[`../../editors/nvim/lua/mapal/icon.lua`](../../editors/nvim/lua/mapal/icon.lua).
