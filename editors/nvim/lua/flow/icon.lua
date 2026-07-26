-- File icon for *.flow.
--
-- The Rust and C++ logos you see in a file tree are font glyphs: Nerd Fonts ships
-- those brand marks as characters. Neovim's icon providers take a glyph, not an
-- image, so the only way to get *Flow's* logo the same way is to put it in a font.
--
-- assets/font/FlowIcons.ttf does exactly that — one glyph, the real mark, at
-- U+F8F0. Add it as a fallback font in your terminal and set `glyph` below to
-- `M.logo`. Without it, the default is the closest existing Nerd Font glyph, so
-- this works out of the box either way.
--
-- Usage (either provider, or both — each is a no-op if not installed):
--     require("flow.icon").setup()                        -- portable glyph
--     require("flow.icon").setup({ glyph = require("flow.icon").logo })

local M = {}

-- The real Flow mark, from assets/font/FlowIcons.ttf. Renders as a blank box
-- unless that font is installed and reachable via fallback.
M.logo = "\u{F8F0}"

-- Default: U+F0E8 nf-fa-sitemap, a node-and-edge graph — the nearest glyph any
-- Nerd Font already has. Written as an escape, not a literal: a raw private-use
-- character does not survive every editor and tool that touches this file, and an
-- empty string here silently renders as no icon at all.
-- Alternatives if your font lacks it:
--     U+E725 nf-dev-git_branch   (branching lines)
--     U+F419 nf-oct-git_merge    (lines converging — closest semantically)
M.glyph = "\u{F0E8}"
M.color = "#14B8A6" -- the logo teal
M.name = "Flow"

function M.setup(opts)
  opts = opts or {}
  local glyph, color = opts.glyph or M.glyph, opts.color or M.color

  local ok_dev, devicons = pcall(require, "nvim-web-devicons")
  if ok_dev then
    devicons.set_icon({
      flow = { icon = glyph, color = color, cterm_color = "43", name = M.name },
    })
  end

  local ok_mini, mini = pcall(require, "mini.icons")
  if ok_mini and mini.config then
    -- mini.icons is configured declaratively, so merge rather than replace.
    local cfg = vim.tbl_deep_extend("force", mini.config or {}, {
      extension = { flow = { glyph = glyph, hl = "MiniIconsAzure" } },
    })
    mini.setup(cfg)
  end

  return ok_dev or ok_mini
end

return M
