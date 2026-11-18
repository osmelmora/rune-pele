#!/bin/sh
# Breaks the significant digits of `toExponential` and `toPrecision`, and where `pow` answers as
# ECMAScript does, in crates/pele-core/src/js.rs one way at a time, and shows that the Kani proof
# or the oracle test of each then fails.
# usage: .agents/skills/prove-js-numerics/scripts/break-numerics.sh [name...]   (under `mise exec --`)
# Each break is one exact edit of js.rs, undone before the next. With names, only those breaks
# run. Exits 1 if a break does not apply any more, or if its check does not fail with it.
set -u
cd "$(dirname "$0")/../../../.."
logs=out/verify/break-numerics
mkdir -p "$logs"
file=crates/pele-core/src/js.rs
failed=0

# Puts js.rs back as it was. cargo goes by modification times, and the copy kept aside is older
# than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$file.unbroken" "$file"
  touch "$file"
}

# break_one <name> <kani harness | oracle test> <from> <to>
# A name ending in `_matches_to_exponential` or `_matches_to_precision`, or starting
# `pow_matches_math_pow`, is a test of tests/oracle.rs; any other is a Kani harness.
break_one() {
  name=$1 check=$2 from=$3 to=$4
  if [ -n "$only" ] && ! printf ' %s ' "$only" | grep -q " $name "; then
    return
  fi
  cp "$file" "$file.unbroken"
  FROM=$from TO=$to perl -0pi -e 'BEGIN { $n = 0 } $n += s/\Q$ENV{FROM}\E/$ENV{TO}/; END { exit($n == 1 ? 0 : 3) }' "$file"
  if [ $? -ne 0 ]; then
    restore
    echo "break $name: NOT APPLIED, the text to break is gone from $file"
    failed=1
    return
  fi
  case $check in
    *_matches_to_exponential | *_matches_to_precision | pow_matches_math_pow*)
      cargo test -q -p pele-core --test oracle "$check" > "$logs/$name.log" 2>&1
      caught='differ:' ;;
    *)
      cargo kani -p pele-core --harness "$check" --output-format terse > "$logs/$name.log" 2>&1
      caught='VERIFICATION:- FAILED' ;;
  esac
  if grep -q "$caught" "$logs/$name.log"; then
    echo "break $name: $check fails, as it should:"
    grep -E "Failed Checks|differ:|^(exponential|precision|pow)\(" "$logs/$name.log" | head -3 | sed 's/^/    /'
  else
    echo "break $name: $check does not fail with it; see $logs/$name.log"
    failed=1
  fi
  restore
}

only="$*"
proof=significant_digits_round_to_the_nearest_and_ties_to_the_larger

break_one tie-rounds-down $proof \
  "if next < b'5' {" \
  "if next <= b'5' {"
break_one carry-keeps-the-nines $proof \
  "kept[at + 1..].fill(b'0');" \
  "kept[at + 1..].fill(b'9');"
break_one carry-adds-no-digit $proof \
  "kept[0] = b'1';
            true" \
  "kept[0] = b'1';
            false"
break_one tie-rounds-down-against-node to_exponential_matches_to_exponential \
  "if next < b'5' {" \
  "if next <= b'5' {"
break_one exponential-keeps-a-digit-too-many to_exponential_matches_to_exponential \
  'significant(x.abs(), digits + 1);' \
  'significant(x.abs(), digits + 2);'
break_one precision-keeps-a-digit-too-many to_precision_matches_to_precision \
  'significant(x.abs(), digits);' \
  'significant(x.abs(), digits + 1);'
break_one precision-writes-1e-7-without-an-exponent to_precision_matches_to_precision \
  'if exponent < -6 || exponent >= digits as i32 {' \
  'if exponent < -7 || exponent >= digits as i32 {'
break_one pow-of-one-to-nan-is-one pow_matches_math_pow_where_it_names_the_answer \
  'if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {' \
  'if base.abs() == 1.0 && exponent.is_infinite() {'
break_one pow-of-one-to-infinity-is-one pow_matches_math_pow_where_it_names_the_answer \
  'if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {' \
  'if exponent.is_nan() {'

exit "$failed"
