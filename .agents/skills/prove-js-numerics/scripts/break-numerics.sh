#!/bin/sh
# Breaks the significant digits of `toExponential` and `toPrecision`, where `pow` answers as
# ECMAScript does, and V8's fdlibm `sin`, `cos` and `log10`, in crates/pele-core/src/js.rs one way
# at a time, and shows that the Kani proof, the oracle test, the property test or intel-trig's
# check of each then fails.
# usage: .agents/skills/prove-js-numerics/scripts/break-numerics.sh [name...]   (under `mise exec --`)
# Each break is one exact edit of js.rs, undone before the next. With names, only those breaks
# run. Exits 1 if a break does not apply any more, or if its check does not fail with it.
set -u
cd "$(dirname "$0")/../../../.."
logs="$PWD/out/verify/break-numerics"
mkdir -p "$logs"
file=crates/pele-core/src/js.rs
if ! cargo build -q -p pele-core --tests > "$logs/unbroken.log" 2>&1; then
  echo "the unbroken tree does not build, so no break can be told apart (see $logs/unbroken.log)"
  exit 1
fi
failed=0

# Puts js.rs back as it was. cargo goes by modification times, and the copy kept aside is older
# than the build of the broken file, so without the touch cargo would keep that build.
restore() {
  mv "$file.unbroken" "$file"
  touch "$file"
}

# The x64 Node intel-trig compares with, installed on first use, and a small sample, so that one
# run takes seconds. Its tables go to the break's log folder, not out/verify/intel-trig.
x64_node() {
  [ -n "${x64:-}" ] || x64=$(scripts/intel-node.sh) || exit 1
  echo "$x64"
}

# break_one <name> <kani harness | oracle test | property test | intel-trig test> <from> <to>
# A name ending in `_matches_to_exponential`, `_matches_to_precision` or
# `_where_fdlibm_changes_course`, or starting `pow_matches_math_pow`, is a test of
# tests/oracle.rs; one of tests/fdlibm_properties.rs's is that; `the_maths_pele_calls` is
# intel-trig's comparison with x64 Node; any other is a Kani harness.
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
    *_matches_to_exponential | *_matches_to_precision | pow_matches_math_pow* | *_where_fdlibm_changes_course)
      cargo test -q -p pele-core --test oracle "$check" > "$logs/$name.log" 2>&1
      caught='differ:' ;;
    sin_is_odd_and_cos_is_even | sin_and_cos_of_a_finite_* | the_log10_of_every_*)
      cargo test -q -p pele-core --test fdlibm_properties "$check" > "$logs/$name.log" 2>&1
      caught='test result: FAILED' ;;
    the_maths_pele_calls*)
      INTEL_TRIG_X64_NODE=$(x64_node) INTEL_TRIG_OUT="$logs/intel-trig-$name" INTEL_TRIG_SAMPLES=100000 \
        cargo test -q --release -p pele-core --test intel_trig -- --ignored "$check" > "$logs/$name.log" 2>&1
      caught='no longer holds' ;;
    *)
      cargo kani -p pele-core --harness "$check" --output-format terse > "$logs/$name.log" 2>&1
      caught='VERIFICATION:- FAILED' ;;
  esac
  if grep -q "$caught" "$logs/$name.log"; then
    echo "break $name: $check fails, as it should:"
    grep -E "Failed Checks|differ:|^(exponential|precision|pow|sin|cos|log10)\(|arguments (where|outside)|powers of ten give|^\`Math" "$logs/$name.log" | head -3 | sed 's/^/    /'
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
break_one sin-of-infinity-is-infinity sin_and_cos_are_nan_for_nan_and_the_infinities \
  "if ix >= 0x7ff0_0000 {
            return f64::NAN;
        }
        let (n, [y0, y1]) = rem_pio2(x);
        match n & 3 {
            0 => kernel_sin(y0, y1, true)," \
  "if ix >= 0x7ff0_0000 {
            return x;
        }
        let (n, [y0, y1]) = rem_pio2(x);
        match n & 3 {
            0 => kernel_sin(y0, y1, true),"
break_one tiny-sin-loses-its-sign under_2_to_the_minus_27_sin_is_x_and_cos_is_one \
  "if high(x) & 0x7fff_ffff < 0x3e40_0000 {
            return x;" \
  "if high(x) & 0x7fff_ffff < 0x3e40_0000 {
            return x.abs();"
break_one cos-adds-the-half-square up_to_a_quarter_turn_sin_and_cos_lie_in_minus_one_to_one \
  'return 1.0 - (0.5 * z - (z * r - x * y));' \
  'return 1.0 + (0.5 * z - (z * r - x * y));'
break_one log10-of-one-is-minus-zero log10_answers_ecmascript_at_its_special_values \
  "if hx == 0x3ff0_0000 && lx == 0 {
            return 0.0;" \
  "if hx == 0x3ff0_0000 && lx == 0 {
            return -0.0;"
break_one sin-takes-the-wrong-quadrant sin_matches_math_sin_where_fdlibm_changes_course \
  '1 => kernel_cos(y0, y1),' \
  '1 => -kernel_cos(y0, y1),'
break_one huge-negative-keeps-its-sign sin_matches_math_sin_where_fdlibm_changes_course \
  'if hx < 0 { (-n, [-y0, -y1]) } else { (n, [y0, y1]) }
    }' \
  '(n, [y0, y1])
    }'
break_one cancellation-takes-one-piece-of-pi cos_matches_math_cos_where_fdlibm_changes_course \
  'if j - ((high(y0) >> 20) & 0x7ff) > 16 {' \
  'if j - ((high(y0) >> 20) & 0x7ff) > 160 {'
break_one log10-scales-subnormals-wrong log10_matches_math_log10_where_fdlibm_changes_course \
  "            k -= 54;
            x *= TWO54;
            hx = (x.to_bits() >> 32) as i32;
        }
        if hx >= 0x7ff0_0000 {
            return x + x;
        }
        let lx" \
  "            k -= 53;
            x *= TWO54;
            hx = (x.to_bits() >> 32) as i32;
        }
        if hx >= 0x7ff0_0000 {
            return x + x;
        }
        let lx"
break_one medium-tail-keeps-its-sign sin_is_odd_and_cos_is_even \
  'return if hx < 0 { (-n, [-y0, -y1]) } else { (n, [y0, y1]) };' \
  'return if hx < 0 { (-n, [-y0, y1]) } else { (n, [y0, y1]) };'
break_one cos-kernel-adds-the-quarter sin_and_cos_of_a_finite_argument_lie_in_minus_one_to_one \
  'let a = 1.0 - qx;' \
  'let a = 1.0 + qx;'
break_one log10-biases-the-exponent-by-1022 the_log10_of_every_normal_power_of_ten_is_its_exponent \
  "k += (hx >> 20) - 1023;
        let i = ((k as u32" \
  "k += (hx >> 20) - 1022;
        let i = ((k as u32"
break_one sin-coefficient-off-in-the-last-bit the_maths_pele_calls_on_x64_and_arm64_node_and_in_the_port \
  'const S1: f64 = f64::from_bits(0xbfc5_5555_5555_5549);' \
  'const S1: f64 = f64::from_bits(0xbfc5_5555_5555_5548);'
break_one libm-log10-put-back the_maths_pele_calls_on_x64_and_arm64_node_and_in_the_port \
  'pub fn log10(x: f64) -> f64 {
    fdlibm::log10(x)' \
  'pub fn log10(x: f64) -> f64 {
    libm::log10(x)'

exit "$failed"
