#!/bin/sh
# Builds the gallery for the web into out/gallery-web with Rune's own scripts/web-build.sh, from
# the Rune checkout Cargo fetched at the pinned rev, into the workspace's one target directory.
# Arguments go to web-build.sh, such as --dev or --opt.
set -eu
root=$(cd "$(dirname "$0")/../../.." && pwd)
gallery="$root/examples/gallery"
out="$root/out/gallery-web"
rune=$(cargo metadata -q --format-version 1 --manifest-path "$gallery/Cargo.toml" | python3 -c '
import json, os, sys
packages = json.load(sys.stdin)["packages"]
rune = next(p for p in packages if p["name"] == "rune" and (p["source"] or "").startswith("git+"))
print(os.path.dirname(os.path.dirname(os.path.dirname(rune["manifest_path"]))))
')
mkdir -p "$out"
# The page of our own: web-build.sh keeps an index.html without its generated-by marker.
cp "$gallery/web/index.html" "$out/index.html"
cd "$gallery"
CARGO_TARGET_DIR="$root/target" "$rune/scripts/web-build.sh" rune-pele-gallery --out "$out" "$@"
# Rune fetches its full Inter faces from beside app.js once the first frame is up. web-build.sh
# finds them through the build's dep-info, which lists path dependencies only, so with Rune as a
# git dependency it copies none: copy those of the Rune the build embedded.
cp "$rune"/crates/rune/assets/web/Inter-*.ttf "$out/"
