-- Register the *.mapal file icon automatically.
--
-- This exists because requiring the user to call require("mapal.icon").setup()
-- meant the icon silently never appeared: the plugin was installed, and nothing
-- happened. An icon is not a feature you opt into, it is what the plugin is for.
--
-- Timing is the whole difficulty. The icon providers (mini.icons under LazyVim,
-- or nvim-web-devicons) are themselves lazy, so they may not be loadable at the
-- moment this file is sourced. So: try now, and again once the editor is up.
-- setup() is idempotent, so repeating it costs nothing.
--
-- NOTE for the plugin spec: registering the icon has to happen at startup, not on
-- `ft = "mapal"`. A file tree showing a .mapal file has no .mapal buffer open, so an
-- ft-gated plugin never loads and the icon never registers. Use `lazy = false`.

if vim.g.loaded_mapal_icon then
  return
end
vim.g.loaded_mapal_icon = 1

local function register()
  local ok, icon = pcall(require, "flow.icon")
  if ok then
    pcall(icon.setup)
  end
end

register()

local group = vim.api.nvim_create_augroup("MapalIconRegister", { clear = true })
-- LazyVim fires User VeryLazy after startup; VimEnter covers every other setup.
vim.api.nvim_create_autocmd("User", {
  group = group,
  pattern = "VeryLazy",
  once = true,
  callback = register,
})
vim.api.nvim_create_autocmd("VimEnter", {
  group = group,
  once = true,
  callback = register,
})

vim.api.nvim_create_user_command("MapalIcon", function()
  local ok, icon = pcall(require, "flow.icon")
  print(ok and icon.report() or "flow.icon module not on the runtimepath")
end, { desc = "Report the Mapal file-icon registration state" })
