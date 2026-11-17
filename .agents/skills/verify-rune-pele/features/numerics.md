# JavaScript numerics

Every coordinate Pele writes passes through JavaScript number rules: `Math.round` (ties toward +∞), Pele's `num()` (hundredths, trailing zeros dropped), `String(x)` and `toFixed`. Users of this port get the same digits Pele prints. On a half-hundredth such as `-0.125`, that is `-0.12`, where Rust's own rounding gives `-0.13`.

## Sub-features

- `numerics-round` gives `pele_core::js::round` the same bits as `Math.round`. All NaN results count as one value.
- `numerics-num` makes `pele_core::js::num` print the same string as Pele's `num()`.
- `numerics-string` makes `pele_core::js::to_string` print the same string as `String(x)`.
- `numerics-fixed` makes `pele_core::js::to_fixed` print the same string as `x.toFixed(d)` for d in 0, 1, 2, 3, 20 and 100.
- `numerics-exponential` makes `pele_core::js::to_exponential` print the same string as `x.toExponential(d)` for every d in 0 to 21, and 50 and 100. Treemap's `e` values use it.
- `numerics-precision` makes `pele_core::js::to_precision` print the same string as `x.toPrecision(d)` for every d in 1 to 21, and 50 and 100. Treemap's values with no type use it.
- `numerics-trig` keeps `pele_core::js::sin` and `cos` within one ulp of `Math.sin` and `Math.cos`.
- `numerics-pow` keeps `pele_core::js::pow` within one ulp of `Math.pow`.

## How to get to it (user POV)

- A crate user calls `pele_core::js::{round, num, write_num, to_string, to_fixed, to_exponential, to_precision, sin, cos, pow, max, min}`.
- Every SVG this port writes prints its numbers through these functions.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.

- **Differential run.** Run `$C $R numerics cargo test -p pele-core --test oracle -- --nocapture`. Exit 0. Stdout reports `9 passed`: one test per sub-feature, with `sin` and `cos` apart. The trig and pow tests each print `<n> of <total> bit-equal to Node, the rest one ulp away`. On darwin arm64 with Node 26.13.1 that is 300,834 of 304,334 for `sin`. Each test sends more than 200,000 doubles to the oracle. Those are edge cases, every k/200 and k/8 for |k| ≤ 4000 with their neighbouring doubles, and seeded random bit patterns and coordinates. The exponential and precision tests print `<op>: <n> compared, <t> of them exact ties, 0 differ`: on darwin arm64 with Node 26.13.1 that is 215,776 with 946 ties and 210,952 with 889 ties. Their inputs are signed zeros, non-finite values, the extremes, each power of ten from 1e-324 to 1e308 with its neighbours, exact decimal ties and runs of nines, each at every precision, then 100,000 seeded random doubles at a random precision each.
- **Spot check.** Run `$C $R numerics-spot sh -c "printf 'num\t3fc0000000000000\nnum\tbfc0000000000000\n' | node harness/dist/oracle.mjs"`. Stdout is `0.13` then `-0.12`. These are Pele's answers for `0.125` and `-0.125`.
- **Failure shape.** A mismatch fails with `<n> of <total> differ:` followed by up to 20 lines `op(x) bits <hex>: pele "<want>", ours "<got>"`. Report the op, the hex bits and both strings.

## Gotchas

- Inputs travel as IEEE-754 bit patterns in hex. A decimal literal copied from a failure line can parse to a neighbouring double. Reproduce with the bits.
- The oracle quiets signalling NaN payloads. The round test compares NaN as a class for that reason. Do not "fix" that by comparing payloads.
- These tests sample. The `prove-js-numerics` skill has the Kani proofs that cover every double for `round` and for `num`'s branch and digit split. `to_string`, `to_fixed`, and the digits and layout of `to_exponential` and `to_precision` are covered by sampling only; Kani proves only the rounding of their significant digits.
- `sin`, `cos` and `pow` are not bit-equal to the pinned Node. Its V8 fuses multiply-adds inside its fdlibm on arm64 and calls the platform's `pow`. A result one ulp away changes a coordinate only when the true value is within an ulp of a rounding tie. `parity-harvest` in `parity.md` is the test that would show such a coordinate.
