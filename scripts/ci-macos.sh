#!/bin/sh
# CI's macOS job (.github/workflows/ci.yml), run locally as `mise run ci-macos`: the rune-pele
# tests, which draw on the system's GPU and hold the drawings to Pele's SVG through resvg, then
# `mise run rune-corpus`, every corpus source of the graph types drawn so in release. Its reports
# are in out/verify/rune-graph.
set -eu
cd "$(dirname "$0")/.."
scripts/setup.sh
cargo test -q -p rune-pele
mise run rune-corpus
