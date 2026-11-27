#!/bin/sh
# The checks that need no build: the licence copies, rustfmt, the workflows and every tracked
# shell script. `mise run lint` runs it, CI's lint job runs it, and `scripts/check.sh` starts with it.
set -eu
cd "$(dirname "$0")/.."
scripts/notices.sh
cargo fmt --check
actionlint
git ls-files -z '*.sh' | xargs -0 shellcheck
