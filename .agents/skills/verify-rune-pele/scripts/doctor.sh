#!/bin/sh
# Read-only: is this checkout worth driving? Prints one line per check and exits 1 on the first failure.
set -u
root=$(cd "$(dirname "$0")/../../../.." && pwd)
cd "$root"
fail() { echo "doctor: FAIL $1"; exit 1; }

want_node=$(sed -n 's/^node = "\(.*\)"/\1/p' mise.toml)
have_node=$(mise exec -- node --version 2>/dev/null) || fail "mise cannot run node; run \`mise install\`"
[ "$have_node" = "v$want_node" ] || fail "node is $have_node, mise.toml pins v$want_node"
echo "doctor: node $have_node"

want_rust=$(sed -n 's/^rust = { version = "\([^"]*\)".*/\1/p' mise.toml)
have_rust=$(mise exec -- rustc --version 2>/dev/null | cut -d' ' -f2) || fail "mise cannot run rustc; run \`mise install\`"
[ "$have_rust" = "$want_rust" ] || fail "rustc is $have_rust, mise.toml pins $want_rust"
echo "doctor: rustc $have_rust"

pin=$(cat scripts/pele-pin.txt)
head=$(git -C vendor/pele rev-parse HEAD 2>/dev/null) || fail "vendor/pele missing; run \`mise run setup\`"
[ "$head" = "$pin" ] || fail "vendor/pele is at $head, pin is $pin; run \`mise run setup\`"
dirty=$(git -C vendor/pele status --porcelain)
[ -z "$dirty" ] || fail "vendor/pele has local edits; the oracle would not be Pele 1.13.0"
echo "doctor: pele $(git -C vendor/pele describe --tags) at $pin"

[ -f harness/dist/oracle.mjs ] || fail "harness/dist/oracle.mjs missing; run \`mise run setup\`"
stamp="// harness ts sha256 $(cat $(command ls harness/*.ts | LC_ALL=C sort) | shasum -a 256 | cut -d' ' -f1) pele $pin"
[ "$(head -1 harness/dist/oracle.mjs)" = "$stamp" ] || fail "oracle bundle was not built from these harness/*.ts files and pin; run \`mise run setup\`"
answer=$(printf 'num\t3fc0000000000000\nnum\tbfc0000000000000\n' | mise exec -- node harness/dist/oracle.mjs | tr '\n' ' ')
[ "$answer" = "0.13 -0.12 " ] || fail "oracle answered num(±0.125) as '$answer', want '0.13 -0.12 '"
echo "doctor: oracle answers num(0.125)=0.13 num(-0.125)=-0.12"

mise exec -- cargo build -q -p pele-cli 2>/dev/null || fail "pele-cli does not build; run \`mise exec -- cargo build -p pele-cli\`"
echo "doctor: pele binary at target/debug/pele"

mise exec -- cargo build -q -p rune-pele --example pie 2>/dev/null || fail "the rune-pele example does not build; run \`mise exec -- cargo build -p rune-pele --example pie\`"
echo "doctor: rune-pele example at target/debug/examples/pie"
echo "doctor: ok"
