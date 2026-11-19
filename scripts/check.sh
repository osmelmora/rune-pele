#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
for crate in pele-core rune-pele; do
  cmp -s LICENSE crates/$crate/LICENSE && cmp -s NOTICE crates/$crate/NOTICE ||
    { echo "crates/$crate: LICENSE or NOTICE differs from the root copy"; exit 1; }
done
cargo fmt --check
cargo clippy -q --all-targets -- -D warnings
cargo test -q
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps -p pele-core -p rune-pele
scripts/consumer.sh
