#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
scripts/notices.sh
cargo fmt --check
cargo clippy -q --all-targets -- -D warnings
cargo test -q
# rune-pele without rune-mermaid, as an app that turns its default features off builds it.
cargo clippy -q -p rune-pele --no-default-features --all-targets -- -D warnings
cargo test -q -p rune-pele --no-default-features
# Every public item of both crates has a doc comment: the doc gate denies a missing one.
RUSTDOCFLAGS="-D warnings -D missing_docs" cargo doc -q --no-deps -p pele-core -p rune-pele
# Both crates build on the Rust their rust-version names: under a second warm, about 30 s cold.
mise run msrv
# pele-core, rune-pele and the gallery for the web, where std's clocks panic: 5 s warm.
mise run web-check
scripts/consumer.sh
