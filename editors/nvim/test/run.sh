#!/usr/bin/env sh
# Assert the syntax file's highlighting decisions. Exits non-zero on failure.
set -e
cd "$(dirname "$0")/../../.."
exec nvim --headless -u NONE -c "luafile editors/nvim/test/syntax_test.lua"
