#!/bin/sh
# Writes the workspace's [patch.crates-io] table into Cargo's home config. A tool that builds
# rune-pele in a project of its own, as cargo-semver-checks does, then resolves Rune too: a
# workspace's [patch] does not reach other projects, and the home config reaches every one.
# For CI and throwaway Cargo homes only; it overwrites $CARGO_HOME/config.toml.
set -eu
cd "$(dirname "$0")/.."
home=${CARGO_HOME:-$HOME/.cargo}
mkdir -p "$home"
awk '/^\[patch\.crates-io\]/{on=1; print; next} /^\[/{on=0} on && NF' Cargo.toml > "$home/config.toml"
test "$(grep -c '^[a-z_-]* = { git' "$home/config.toml")" = 8
echo "Rune's eight patches written to $home/config.toml"
