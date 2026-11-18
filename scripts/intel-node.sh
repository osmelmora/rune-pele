#!/bin/sh
# Installs the Node version mise.toml pins, built for x86-64, beside the arm64 one, and prints its
# path. mise installs it under out/intel-node with its own data folder, so the pinned Node is left
# as it is, and Rosetta 2 runs it. mise checks the tarball against nodejs.org's SHASUMS256.txt and
# that file's signature; this script checks the kept tarball against SHASUMS256.txt again.
# usage: mise run intel-node
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
fail() { echo "intel-node: FAIL $1" >&2; exit 1; }
version=$(sed -n 's/^node = "\(.*\)"/\1/p' "$root/mise.toml")
export MISE_DATA_DIR="$root/out/intel-node/mise" MISE_CACHE_DIR="$root/out/intel-node/cache"
export MISE_ARCH=x64 MISE_NODE_VERIFY=true MISE_ALWAYS_KEEP_DOWNLOAD=true
downloads="$MISE_DATA_DIR/downloads/node/$version"
tarball="node-v$version-darwin-x64.tar.gz"
if [ -f "$downloads/$tarball" ] && [ -f "$downloads/SHASUMS256.txt" ]; then
  mise install -q "node@$version" >&2
else
  mise install -q --force "node@$version" >&2
fi
(cd "$downloads" && grep " $tarball\$" SHASUMS256.txt | shasum -a 256 -c - >&2) || fail "$tarball does not match SHASUMS256.txt"
node="$(mise where "node@$version")/bin/node"
said=$("$node" -p 'process.version + " " + process.arch') || fail "$node does not run; is Rosetta 2 installed?"
[ "$said" = "v$version x64" ] || fail "$node says '$said', want 'v$version x64'"
echo "intel-node: $node is Node v$version for x64" >&2
echo "$node"
