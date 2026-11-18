#!/bin/sh
# Measures whether Pele on an Intel Mac computes what Pele on Apple Silicon and this port compute.
# Under the pinned Node and the same version built for x86-64 (`mise run intel-node`), it compares
# the maths functions Pele calls bit for bit, Pele's answers over the corpus and the fixtures, and
# runs `pele parity` with the x86-64 Node as the oracle. Tables land in out/verify/intel-trig/.
# usage: mise run intel-trig [-- <seed> [<random inputs a function>]]
set -eu
cd "$(dirname "$0")/.."
fail() { echo "intel-trig: FAIL $1" >&2; exit 1; }
out="$PWD/out/verify/intel-trig"
mkdir -p "$out"
x64=$(scripts/intel-node.sh)

# MISE_NODE_VERSION=path:<folder> makes `mise which node`, and so the oracle and the doctor, name
# that Node. The oracle still refuses one whose version is not mise.toml's, and the doctor says
# which Node it is and that the variable named it.
intel="path:$(dirname "$(dirname "$x64")")"
MISE_NODE_VERSION="$intel" .agents/skills/verify-rune-pele/scripts/doctor.sh > "$out/doctor-x64.txt" || fail "the doctor fails on x64 Node: $(tail -1 "$out/doctor-x64.txt")"
grep -qx "doctor: node v[0-9.]* x64 at $x64, the one the oracle runs" "$out/doctor-x64.txt" || fail "the doctor does not name the x64 Node: $(cat "$out/doctor-x64.txt")"
grep -q "^doctor: MISE_NODE_VERSION=$intel names this node" "$out/doctor-x64.txt" || fail "the doctor does not say MISE_NODE_VERSION names the Node"
cat "$out/doctor-x64.txt"

export INTEL_TRIG_X64_NODE="$x64" INTEL_TRIG_OUT="$out"
export INTEL_TRIG_SEED="${1:-0x1e7e1}" INTEL_TRIG_SAMPLES="${2:-10000000}"
cargo test -q --release -p pele-core --test intel_trig -- --ignored --nocapture --test-threads=1

cargo build -q -p pele-cli
set +e
MISE_NODE_VERSION="$intel" target/debug/pele parity --out "$out/parity-x64" > "$out/parity-x64.txt"
status=$?
set -e
cat "$out/parity-x64.txt"
echo "intel-trig: tables in $out/math.md and $out/pele-svgs.md, parity on x64 Node in $out/parity-x64.txt (exit $status)"
exit $status
