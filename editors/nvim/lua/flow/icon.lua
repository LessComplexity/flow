-- File icon for *.flow.
--
-- Neovim's icon providers take a FONT GLYPH, not an image — nvim-web-devicons and
-- mini.icons both map a filetype to one character from a patched Nerd Font plus a
-- colour. So this cannot be the actual SVG logo; the closest honest thing is a
-- glyph shaped like the mark (edges converging on a node) in the logo's teal.
-- For a real SVG logo in a file tree, see editors/vscode/.
--
-- Usage (either provider, or both — each is a no-op if not installed):
--     require("flow.icon").setup()

local M = {}

-- U+F0E8 nf-fa-sitemap: a node-and-edge graph, the nearest glyph to the mark.
-- Present in Nerd Fonts v2 and v3. Alternatives if your font lacks it:
--     "" U+E725 nf-dev-git_branch   (branching lines)
--     "" U+F419 nf-oct-git_merge    (lines converging — closest semantically)
M.glyph = ""
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
