#!/bin/sh
# Breaks the workflows one way at a time, and shows that `mise run actionlint` then fails.
# usage: scripts/break-ci.sh [name...]   (under `mise exec --`)
# Each break is one exact edit of a workflow, undone before the next. With names, only those
# breaks run. Exits 1 if a break does not apply any more, or if actionlint still passes with it.
set -u
cd "$(dirname "$0")/.." || exit
logs=out/verify/break-ci
mkdir -p "$logs"
if ! actionlint > "$logs/unbroken.log" 2>&1; then
  echo "actionlint fails on the unbroken workflows: see $logs/unbroken.log"
  exit 1
fi
failed=0

# break_one <name> <file> <from> <to>
break_one() {
  name=$1 file=$2 from=$3 to=$4
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  if ! FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"; then
    mv "$file.unbroken" "$file"
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
    return
  fi
  if actionlint > "$logs/$name.log" 2>&1; then
    echo "break $name: actionlint still PASSES with it"
    failed=1
  else
    echo "break $name: actionlint fails, as it should:"
    head -1 "$logs/$name.log" | sed 's/^/    /'
  fi
  mv "$file.unbroken" "$file"
}

only="$*"
release=.github/workflows/release-plz.yml

break_one app-token-if-reads-secrets $release \
  "if: env.RELEASE_APP == 'true'" \
  "if: secrets.RELEASE_APP_ID != ''"
# shellcheck disable=SC2016 # $RUNNER_TEMP is the workflow step's, written into it unexpanded
break_one shell-step-unquoted .github/workflows/commits.yml \
  'hk util check-conventional-commit "$RUNNER_TEMP/message"' \
  'hk util check-conventional-commit $RUNNER_TEMP/message'

exit "$failed"
