#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy -q --all-targets -- -D warnings
cargo test -q
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps -p rune-pele
scripts/consumer.sh
