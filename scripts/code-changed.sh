#!/bin/sh
# Whether a commit, HEAD unless named, changes anything but documentation since its merge base
# with another: prints `code=true` or `code=false`, for CI's `$GITHUB_OUTPUT`, and the files that
# count on stderr.
# Documentation is every Markdown file and everything in docs/, but README.md, whose patches and
# Rune rev examples/consumer's test reads. With no base, or one git does not have, everything
# counts as changed.
# usage: scripts/code-changed.sh [base [head]]
set -eu
cd "$(dirname "$0")/.."
base=${1-}
head=${2-HEAD}
if [ -z "$base" ] || ! git cat-file -e "$base^{commit}" 2> /dev/null; then
  echo "no base to compare with: everything counts" >&2
  echo code=true
  exit 0
fi
# --no-renames lists a renamed file under both names, so a source moved into docs/ still counts.
code=$(git diff --name-only --no-renames "$base...$head" | awk '$0 == "README.md" || !(/^docs\// || /\.md$/)')
if [ -n "$code" ]; then
  printf 'code changed:\n%s\n' "$code" >&2
  echo code=true
else
  echo "documentation only" >&2
  echo code=false
fi
