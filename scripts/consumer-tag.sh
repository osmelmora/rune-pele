#!/bin/sh
# Builds and tests a throwaway app under out/consumer-tag/ that depends on rune-pele by git at a
# release tag, as the README's "Using rune-pele in your app" says, and draws one diagram. Cargo
# runs with nothing on PATH but the pinned Rust's cargo and rustc and the system's basic tools,
# so the app needs no vendor/, no Node and no oracle.
# usage: scripts/consumer-tag.sh [tag [url]]   (under `mise exec --`, or `mise run consumer-tag`)
# The tag is $RUNE_PELE_TAG, or else the newest v* tag of this repository. The URL is
# $RUNE_PELE_URL, or else this repository as a file:// URL, which needs no GitHub login.
set -eu
cd "$(dirname "$0")/.."
repo=$(cd "$(git rev-parse --git-common-dir)" && pwd)
tag=${1:-${RUNE_PELE_TAG:-$(git tag --list 'v*' --sort=-v:refname | head -n 1)}}
url=${2:-${RUNE_PELE_URL:-file://$repo}}
if [ -z "$tag" ]; then
  echo "consumer-tag: no v* tag in $repo; release tags are made on GitHub, so fetch them: git fetch --tags" >&2
  exit 1
fi

out=$PWD/out/consumer-tag
rm -rf "$out/app" "$out/tagged" "$out/bin"
git -c advice.detachedHead=false clone -q --depth 1 --branch "$tag" "$url" "$out/tagged"
echo "rune-pele $tag at $(git -C "$out/tagged" rev-parse --short HEAD), from $url"
for absent in vendor harness/dist harness/node_modules; do
  if [ -e "$out/tagged/$absent" ]; then
    echo "consumer-tag: the tag's tree has $absent" >&2
    exit 1
  fi
done
echo "the tag's tree has no vendor/ and no oracle (harness/dist, harness/node_modules)"

# The Rune the tag pins, and its patches (four to v0.2.2, eight after), which an app repeats. The app names Rune itself,
# as every tag's README says: `rune_pele::rune` is newer than v0.1.0.
rune=$(sed -n 's/^rune = { git = "\([^"]*\)", rev = "\([^"]*\)".*/\1 \2/p' "$out/tagged/crates/rune-pele/Cargo.toml" | head -n 1)
patches=$(sed -n '/^\[patch\.crates-io\]$/,/^\[/{/^\[patch\.crates-io\]$/p;/ = {/p;}' "$out/tagged/Cargo.toml")
mkdir -p "$out/app/src"
cat > "$out/app/Cargo.toml" << MANIFEST
[package]
name = "rune-pele-consumer-tag"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dependencies]
rune-pele = { git = "$url", tag = "$tag" }
rune = { git = "${rune% *}", rev = "${rune#* }", default-features = false, features = ["embedded-fonts"] }

$patches
MANIFEST
# The versions the tag was tested with, so that the build takes no crate the tag never built.
cp "$out/tagged/Cargo.lock" "$out/app/Cargo.lock"
cat > "$out/app/src/lib.rs" << 'APP'
use rune::{Color, Cx, El, el, text};
use rune_pele::{Diagram, ViewOptions};

pub struct App {
    pub diagram: Diagram,
}

pub fn view(app: &App, cx: &mut Cx<App>) -> El {
    let source = "flowchart LR\n  A[Released tag] --> B[An app draws it]\n";
    let drawn = app.diagram.view(cx, source, ViewOptions::fit()).unwrap_or_else(|error| text(error.to_string()));
    el().w_full().fill(Color::WHITE).child(drawn)
}

#[cfg(test)]
mod tests {
    use rune::Env;
    use rune_pele::capture::Capturer;
    use rune_pele::{Diagram, FontCatalog, RenderStatus};

    #[test]
    fn an_app_on_the_tag_draws_a_diagram_natively() {
        let app = super::App { diagram: Diagram::new("diagram", FontCatalog::rune()) };
        let capturer = Capturer::new();
        let mut window = capturer.window(app, super::view, (480.0, 200.0), Env::new(false, 2.0));
        let pixels = window.capture();
        let diagram = &window.state().diagram;
        assert_eq!(diagram.status(), RenderStatus::Pele, "drawn natively: {:?}", diagram.error());
        let page = pixels.at(0, 0);
        let ink = pixels.rgba.chunks(4).filter(|pixel| *pixel != page).count();
        assert!(ink > 5_000, "the drawing shows: {ink} pixels differ from the page");
    }
}
APP

mkdir -p "$out/bin"
sysroot=$(rustc --print sysroot)
ln -s "$sysroot/bin/cargo" "$sysroot/bin/rustc" "$out/bin/"
path=$out/bin:/usr/bin:/bin:/usr/sbin:/sbin
echo "PATH=$path"
env -i PATH="$path" sh -c 'for tool in node npm mise; do if command -v $tool > /dev/null; then echo "$tool is on PATH"; exit 1; fi; done'
echo "node, npm and mise are not on PATH; cargo is $(env -i PATH="$path" HOME="$HOME" cargo --version)"
env -i PATH="$path" HOME="$HOME" ${CARGO_HOME:+CARGO_HOME="$CARGO_HOME"} \
  cargo test --lib --manifest-path "$out/app/Cargo.toml" --target-dir "$out/target"
