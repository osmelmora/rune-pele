#!/bin/sh
# Builds and tests examples/consumer, an app in a workspace of its own that depends on rune-pele
# as the README says. It resolves on its own, with its own lock and patches, and builds into the
# workspace's target directory, so that a warm run builds nothing new.
set -eu
cd "$(dirname "$0")/.."
manifest=examples/consumer/Cargo.toml
cargo fmt --check --manifest-path "$manifest"
cargo clippy -q --all-targets --manifest-path "$manifest" --target-dir target -- -D warnings
cargo test -q --manifest-path "$manifest" --target-dir target
