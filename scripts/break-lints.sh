#!/bin/sh
# Breaks each rule the lints hold one way at a time, and shows that its check then fails: clippy's
# numerics rule for pele-core, #[must_use] on builders, Debug on public types, the ban on unsafe
# code, ShellCheck on the scripts, and cargo-deny's sources and licences.
# usage: scripts/break-lints.sh [name...]   (under `mise exec --`)
# Each break is one exact edit, undone before the next, with Cargo.lock put back too. With names,
# only those breaks run. The clippy breaks lint only the crate they edit, with the flags and
# clippy.toml `mise run check` gives it. Exits 1 if a check fails on the unbroken tree, if a break
# does not apply any more, or if its check still passes with it.
set -u
cd "$(dirname "$0")/.." || exit
logs=out/verify/break-lints
mkdir -p "$logs"
failed=0

# run <check>: the check a break names.
run() {
  case $1 in
    pele-core) cargo clippy -q -p pele-core --lib -- -D warnings ;;
    rune-pele) cargo clippy -q -p rune-pele --lib -- -D warnings ;;
    lint) scripts/lint.sh ;;
    deny) cargo deny check ;;
  esac
}

for check in pele-core rune-pele lint deny; do
  if ! run "$check" > "$logs/unbroken-$check.log" 2>&1; then
    echo "$check fails on the unbroken tree: see $logs/unbroken-$check.log"
    exit 1
  fi
done

# Two crates for the cargo-deny breaks, made here so that they need no network: one in a git
# repository of its own, which is no source deny.toml allows, and one under the GPL.
crates=$PWD/$logs/crates
rm -rf "${crates:?}"
mkdir -p "$crates/elsewhere/src" "$crates/copyleft/src"
printf '[package]\nname = "elsewhere"\nversion = "0.1.0"\nedition = "2024"\nlicense = "MIT"\n' \
  > "$crates/elsewhere/Cargo.toml"
printf '[package]\nname = "copyleft"\nversion = "0.1.0"\nedition = "2024"\nlicense = "GPL-3.0-only"\n' \
  > "$crates/copyleft/Cargo.toml"
: > "$crates/elsewhere/src/lib.rs"
: > "$crates/copyleft/src/lib.rs"
git -C "$crates/elsewhere" init -q
git -C "$crates/elsewhere" add .
git -C "$crates/elsewhere" -c user.name=break -c user.email=break@localhost commit -q -m elsewhere

# break_one <name> <file> <from> <to> <check>
break_one() {
  name=$1 file=$2 from=$3 to=$4 check=$5
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  cp Cargo.lock Cargo.lock.unbroken
  if ! FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"; then
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
  elif run "$check" > "$logs/$name.log" 2>&1; then
    echo "break $name: $check still PASSES with it"
    failed=1
  else
    echo "break $name: $check fails, as it should:"
    grep -m1 -E '^error|SC[0-9]+' "$logs/$name.log" | sed 's/^ */    /'
  fi
  # The restored files keep their old times, older than the build of the broken ones, so without
  # the touch cargo would keep that build.
  mv "$file.unbroken" "$file"
  mv Cargo.lock.unbroken Cargo.lock
  touch "$file" Cargo.lock
}

only="$*"
cli=crates/pele-cli/Cargo.toml

break_one numerics-direct-sin crates/pele-core/src/diagrams/venn/layout.rs \
  "        js::sin(x)" \
  "        x.sin()" \
  pele-core
break_one builder-without-must-use crates/rune-pele/src/lib.rs \
  "    #[must_use]
    pub fn zoom(mut self, zoom: f32) -> Self {" \
  "    pub fn zoom(mut self, zoom: f32) -> Self {" \
  rune-pele
break_one public-type-without-debug crates/pele-core/src/diagrams/sankey/layout.rs \
  "#[derive(Debug)]
pub struct Flows<'a> {" \
  "pub struct Flows<'a> {" \
  pele-core
break_one unsafe-block crates/pele-core/src/js.rs \
  "pub fn round(x: f64) -> f64 {
" \
  "pub fn round(x: f64) -> f64 {
    unsafe {}
" \
  pele-core
# shellcheck disable=SC2016 # the script's own text, with its variables unexpanded
break_one unguarded-rm-rf scripts/consumer-tag.sh \
  'rm -rf "${out:?}/app" "${out:?}/tagged" "${out:?}/bin"' \
  'rm -rf "${out:?}/app" "${out:?}/tagged" "$out/bin"' \
  lint
break_one git-dependency-from-another-source $cli \
  'serde_json = "1"
' \
  "serde_json = \"1\"
elsewhere = { git = \"file://$crates/elsewhere\" }
" \
  deny
break_one copyleft-dependency $cli \
  'serde_json = "1"
' \
  "serde_json = \"1\"
copyleft = { path = \"$crates/copyleft\" }
" \
  deny

exit "$failed"
