-- File icon for *.mapal.
--
-- The Rust and C++ logos in a file tree are font glyphs: Nerd Fonts ships those brand
-- marks as characters. Neovim's icon providers take a glyph, not an image, so the only
-- way to get *Mapal's* logo the same way is to put it in a font — which
-- assets/font/MapalIcons.ttf does, one glyph at U+F8F0.
--
-- Registration is automatic (see plugin/mapal_icon.lua); calling setup() by hand is
-- optional and only needed to override the glyph or colour.
--
--     :MapalIcon                       -- report what is actually registered
--     require("mapal.icon").setup({ glyph = require("mapal.icon").logo })

local M = {}

-- The real Mapal mark, from assets/font/MapalIcons.ttf. Renders as a blank box unless
-- that font is installed AND reachable through terminal font fallback.
M.logo = "\u{F8F0}"

-- Default: U+F0E8 nf-fa-sitemap, a node-and-edge graph — the nearest glyph any Nerd
-- Font already has, so this works with no extra install.
-- Escaped, not literal: a raw private-use character does not survive every tool that
-- touches this file, and an empty glyph silently renders as no icon at all.
M.glyph = "\u{F0E8}"
M.color = "#14B8A6" -- the logo teal
M.name = "Mapal"
M.hl = "MapalIcon" -- our own group, so the icon is teal rather than a borrowed colour

local state = { providers = {}, glyph = nil }

local function ensure_hl(color)
  -- Re-applied on ColorScheme: a colorscheme load clears user groups.
  vim.api.nvim_set_hl(0, M.hl, { fg = color, default = true })
end

--- Register the icon with whichever provider is present. Idempotent.
--- @return table  which providers were reached
function M.setup(opts)
  opts = opts or {}
  local glyph = opts.glyph or M.glyph
  local color = opts.color or M.color

  ensure_hl(color)
  vim.api.nvim_create_autocmd("ColorScheme", {
    group = vim.api.nvim_create_augroup("MapalIconHl", { clear = true }),
    callback = function() ensure_hl(color) end,
  })

  state.glyph, state.providers = glyph, {}

  -- nvim-web-devicons: direct API, keyed by extension.
  local ok_dev, devicons = pcall(require, "nvim-web-devicons")
  if ok_dev then
    devicons.set_icon({
      flow = { icon = glyph, color = color, cterm_color = "43", name = M.name },
    })
    table.insert(state.providers, "nvim-web-devicons")
  end

  -- mini.icons (the LazyVim default) is configured declaratively and has no
  -- add-one-icon call, so re-run setup with the CURRENT config merged — passing only
  -- our table would reset everything else the user configured.
  local ok_mini, mini = pcall(require, "mini.icons")
  if ok_mini then
    local cfg = vim.deepcopy(mini.config or {})
    cfg.extension = cfg.extension or {}
    cfg.extension.mapal = { glyph = glyph, hl = M.hl }
    cfg.filetype = cfg.filetype or {}
    cfg.filetype.mapal = { glyph = glyph, hl = M.hl }
    local ok = pcall(mini.setup, cfg)
    if ok then table.insert(state.providers, "mini.icons") end
  end

  return state.providers
end

--- Is MapalIcons.ttf actually installed? Best-effort, per platform.
local function font_installed()
  local candidates = {
    vim.fn.expand("~/Library/Fonts/MapalIcons.ttf"),
    vim.fn.expand("~/.local/share/fonts/MapalIcons.ttf"),
    vim.fn.expand("~/.fonts/MapalIcons.ttf"),
  }
  for _, p in ipairs(candidates) do
    if vim.fn.filereadable(p) == 1 then return true, p end
  end
  if vim.fn.executable("fc-list") == 1 then
    if vim.fn.system("fc-list | grep -c MapalIcons"):gsub("%s", "") ~= "0" then
      return true, "via fc-list"
    end
  end
  return false, nil
end

--- Human-readable diagnostic — why you are or are not seeing the icon.
function M.report()
  local lines = { "flow.icon:" }
  local providers = state.glyph and state.providers or {}
  table.insert(lines, ("  registered with : %s"):format(
    #providers > 0 and table.concat(providers, ", ") or "NOTHING (no provider found)"))

  local g = state.glyph or M.glyph
  table.insert(lines, ("  glyph           : %s  (U+%04X)%s"):format(
    g, vim.fn.char2nr(g), g == M.logo and "  <- the real mark" or "  <- portable fallback"))

  local have, where = font_installed()
  table.insert(lines, ("  MapalIcons.ttf   : %s"):format(
    have and ("installed (" .. where .. ")") or "NOT installed"))

  if g == M.logo and not have then
    table.insert(lines, "  ! using the real mark without the font -> renders blank.")
    table.insert(lines, "    install assets/font/MapalIcons.ttf, or drop the glyph override.")
  elseif not have then
    table.insert(lines, "  note: install assets/font/MapalIcons.ttf and set")
    table.insert(lines, "        glyph = require('flow.icon').logo  for the real mark.")
  end

  if #providers == 0 then
    table.insert(lines, "  ! neither mini.icons nor nvim-web-devicons was loadable when")
    table.insert(lines, "    registration ran. If the plugin is lazy-loaded on ft=flow, the")
    table.insert(lines, "    explorer never triggers it — load it eagerly (lazy = false).")
  end

  return table.concat(lines, "\n")
end

return M
