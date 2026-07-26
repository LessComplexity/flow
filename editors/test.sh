#!/usr/bin/env sh
# Both editors' highlighting, asserted. Exits non-zero on any failure.
#
# The two grammars are hand-maintained copies of the same rules with OPPOSITE
# precedence (Vim last-match-wins, TextMate first-match-wins/earliest-match), so
# "consistent" is something to check, not to hope for. The TextMate suite compares
# its scopes against the expected Vim group for the same token and asserts the
# builtin lists are identical.
set -e
cd "$(dirname "$0")/.."
echo "== neovim"
sh editors/nvim/test/run.sh
echo
echo "== vscode"
python3 editors/vscode/test/scope_test.py
