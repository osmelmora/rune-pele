#!/bin/sh
# Puts a call that panics on wasm32-unknown-unknown back into pele-core or rune-pele, one at a
# time, and shows that `mise run web-check` then fails.
# usage: examples/gallery/web/break-web-check.sh [name...]   (under `mise exec --`)
# Each break is one exact edit, undone before the next. With names, only those breaks run. Exits 1
# if a break does not apply any more, or if the check still passes with it.
set -u
cd "$(dirname "$0")/../../.."
logs=out/verify/break-web-check
mkdir -p "$logs"
if ! mise run -q web-check > "$logs/unbroken.log" 2>&1; then
  echo "web-check fails on the unbroken tree: see $logs/unbroken.log"
  exit 1
fi
failed=0

# break_one <name> <file> <from> <to>
break_one() {
  name=$1 file=$2 from=$3 to=$4
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"
  if [ $? -ne 0 ]; then
    mv "$file.unbroken" "$file"
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
    return
  fi
  if mise run -q web-check > "$logs/$name.log" 2>&1; then
    echo "break $name: web-check still PASSES with it"
    failed=1
  else
    echo "break $name: web-check fails, as it should:"
    grep -m1 -E 'disallowed method|\.\.\. FAIL' "$logs/$name.log" | sed 's/^ */    /'
  fi
  # The restored file keeps its old time, older than the build of the broken one, so without the
  # touch cargo would keep that build.
  mv "$file.unbroken" "$file"
  touch "$file"
}

only="$*"
view=crates/rune-pele/src/lib.rs
gantt=crates/pele-core/src/diagrams/gantt/render.rs

break_one rune-pele-std-instant $view \
  "            let started = Instant::now();
            let painted" \
  "            let started = std::time::Instant::now();
            let painted"
break_one rune-pele-std-system-time $view \
  "let since = SystemTime::now().duration_since(UNIX_EPOCH);" \
  "let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);"
break_one pele-core-std-clock-on-the-web $gantt \
  "    js_sys::Date::now()" \
  "    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0.0, |since| since.as_millis() as f64)"
# The lint silenced: the clock test, run in Node, still sees the panic.
break_one pele-core-std-clock-unlinted $gantt \
  "    js_sys::Date::now()" \
  "    #[allow(clippy::disallowed_methods)]
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0.0, |since| since.as_millis() as f64)"

exit "$failed"
