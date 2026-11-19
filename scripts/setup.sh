#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
pin=$(cat "$root/scripts/pele-pin.txt")
if [ ! -d "$root/vendor/pele/.git" ]; then
  git clone -q https://github.com/obsidianmd/pele "$root/vendor/pele"
fi
git -C "$root/vendor/pele" fetch -q origin "$pin" 2>/dev/null || true
git -C "$root/vendor/pele" -c advice.detachedHead=false checkout -q "$pin"
test "$(git -C "$root/vendor/pele" rev-parse HEAD)" = "$pin"
cd "$root/harness"
npm ci --silent --no-audit --no-fund
node build.mjs
echo "pele $(git -C "$root/vendor/pele" describe --tags) at $pin, oracle at harness/dist/oracle.mjs"

cd "$root"
hk install --mise
