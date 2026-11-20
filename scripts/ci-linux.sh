#!/bin/sh
# CI's Linux job (.github/workflows/ci.yml), run locally as `mise run ci-linux`: the workflows
# linted, everything `mise run check` runs, and `pele parity` on a fresh folder, which must read
# all 522 corpus sources identical. On Linux the rune-pele tests draw on a Vulkan GPU, which
# Mesa's lavapipe is in software, and Pele's SVG in Liberation Sans, which has Arial's metrics:
# Debian and Ubuntu name them mesa-vulkan-drivers and fonts-liberation.
set -eu
cd "$(dirname "$0")/.."
actionlint
scripts/setup.sh
scripts/check.sh
out=out/ci
rm -f "$out/parity.txt"
mkdir -p "$out"
if ! cargo run -q -p pele-cli -- parity --out "$out/parity" > "$out/parity.txt"; then
  cat "$out/parity.txt"
  exit 1
fi
cat "$out/parity.txt"
total=$(awk '$1 == "total" { print $2, $3, $4, $5, $6 }' "$out/parity.txt")
if [ "$total" != "522 522 0 0 0" ]; then
  echo "pele parity: total $total, where 522 522 0 0 0 is the gate"
  exit 1
fi
