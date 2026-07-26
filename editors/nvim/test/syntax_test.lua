-- Pins the highlighting decisions that regex precedence makes fragile.
--
-- syntax/flow.vim depends on Vim's last-match-wins rule, and its own header says
-- "read before reordering anything". This asserts the outcomes rather than the
-- ordering, so a reorder that changes behaviour fails here instead of silently
-- mis-painting code.
--
-- Run:  editors/nvim/test/run.sh

-- run.sh cd's to the repo root, so these are repo-relative. `<sfile>` is not
-- used: it does not resolve inside a `:luafile`.
vim.cmd("set noswapfile")
vim.cmd("set rtp+=editors/nvim")
vim.cmd("edit! editors/nvim/test/fixture.flow")
vim.cmd("syntax on")

-- {line, token, expected group, why}
local cases = {
  { 2, "double", "flowFnName", "name after `fn`" },
  { 3, "ret", "flowRet", "the graph sink" },
  { 6, "shout", "flowFnName", "no-value fn declaration" },
  { 7, "println", "flowBuiltin", "effect builtin, terminal" },

  -- The two the user reported: a binding and a call to one of our own fns,
  -- which are lexically identical and were both unhighlighted.
  { 11, "a", "flowBinding", "terminal arrow BINDS a variable" },
  { 12, "shout", "flowDeclaredFn", "terminal arrow CALLS a declared fn" },
  { 13, "double", "flowDeclaredFn", "declared fn mid-pipeline" },
  { 13, "b", "flowBinding", "binding after a call in the same statement" },

  -- Builtins must not fall through to the generic between-arrows rule; `iota`
  -- did exactly that before being added to the keyword list.
  { 14, "iota", "flowBuiltin", "procedural source, not a user fn" },
  { 14, "ti", "flowBinding", "binding at the end of a pipeline" },
  { 15, "widen_f32", "flowBuiltin", "widening family" },

  -- Keywords must keep outranking the binding match.
  { 18, "true", "flowGuardBool", "guard discriminant keeps its own colour" },
  { 21, "Pixel", "flowTypeName", "PascalCase stays a type, not a binding" },

  -- ADR-0012 labelled blocks. Absent from this fixture until a report of them
  -- being unhighlighted; neither suite covered them.
  { 25, ":myloop", "flowLabel", "labelled block declaration" },
  { 27, ":myloop", "flowLabelJump", "jump to a label" },
}

local failures = 0
for _, c in ipairs(cases) do
  local lnum, tok, want, why = c[1], c[2], c[3], c[4]
  local line = vim.fn.getline(lnum)
  -- Whole-word match. A plain substring search finds the `b` inside `double`,
  -- which is how the first version of this test reported a false failure.
  -- A leading `:` has no word frontier before it, so match plainly in that case.
  local col = tok:sub(1, 1) == ":" and line:find(tok, 1, true)
    or line:find("%f[%w_]" .. tok .. "%f[^%w_]")
  local got = col and vim.fn.synIDattr(vim.fn.synID(lnum, col, 1), "name") or "«no such token»"
  if got == "" then got = "«none»" end
  if got ~= want then
    failures = failures + 1
    print(string.format("FAIL  %d:%-10s want %-16s got %-16s (%s)", lnum, tok, want, got, why))
  else
    print(string.format("ok    %d:%-10s %-16s (%s)", lnum, tok, got, why))
  end
end

-- No valid example may light up as an error.
for _, f in ipairs(vim.fn.glob(vim.fn.getcwd() .. "/examples/*.flow", false, true)) do
  vim.cmd("edit! " .. vim.fn.fnameescape(f))
  vim.cmd("syntax on")
  for l = 1, vim.fn.line("$") do
    for c = 1, #vim.fn.getline(l) do
      if vim.fn.synIDattr(vim.fn.synID(l, c, 1), "name") == "flowReserved" then
        failures = failures + 1
        print(string.format("FAIL  %s:%d:%d flagged as reserved/error", vim.fn.fnamemodify(f, ":t"), l, c))
      end
    end
  end
end

print(failures == 0 and "\nall highlighting assertions passed"
  or ("\n" .. failures .. " highlighting assertion(s) failed"))
vim.cmd(failures == 0 and "qa!" or "cq!")
