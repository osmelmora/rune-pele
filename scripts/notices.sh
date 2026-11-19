#!/bin/sh
# Each crate carries the root LICENSE and NOTICE, so the notices travel with a packaged or copied
# crate. `scripts/notices.sh` checks the copies; `scripts/notices.sh --fix` copies the root over them.
set -eu
cd "$(dirname "$0")/.."
for crate in pele-core rune-pele; do
  if [ "${1-}" = --fix ]; then
    cp LICENSE NOTICE "crates/$crate/"
  elif ! cmp -s LICENSE "crates/$crate/LICENSE" || ! cmp -s NOTICE "crates/$crate/NOTICE"; then
    echo "crates/$crate: LICENSE or NOTICE differs from the root copy (scripts/notices.sh --fix)"
    exit 1
  fi
done
