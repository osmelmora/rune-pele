# JavaScript numerics

Every coordinate Pele writes passes through JavaScript number rules: `Math.round` (ties toward +∞), Pele's `num()` (hundredths, trailing zeros dropped), `String(x)` and `toFixed`. Users of this port get the same digits Pele prints. On a half-hundredth such as `-0.125`, that is `-0.12`, where Rust's own rounding gives `-0.13`.

## Sub-features

- `numerics-round` gives `pele_core::js::round` the same bits as `Math.round`. All NaN results count as one value.
- `numerics-num` makes `pele_core::js::num` print the same string as Pele's `num()`.
- `numerics-string` makes `pele_core::js::to_string` print the same string as `String(x)`.
- `numerics-fixed` makes `pele_core::js::to_fixed` print the same string as `x.toFixed(d)` for d in 0, 1, 2, 3, 20 and 100.

## How to get to it (user POV)

- A crate user calls `pele_core::js::{round, num, write_num, to_string, to_fixed}`.
- Every SVG this port writes prints its numbers through these functions.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.

- **Differential run.** Run `$C $R numerics cargo test -p pele-core --test oracle`. Exit 0. Stdout reports `4 passed`, one test per sub-feature. Each test sends more than 200,000 doubles to the oracle. Those are edge cases, every k/200 and k/8 for |k| ≤ 4000 with their neighbouring doubles, and seeded random bit patterns and coordinates.
- **Spot check.** Run `$C $R numerics-spot sh -c "printf 'num\t3fc0000000000000\nnum\tbfc0000000000000\n' | node harness/dist/oracle.mjs"`. Stdout is `0.13` then `-0.12`. These are Pele's answers for `0.125` and `-0.125`.
- **Failure shape.** A mismatch fails with `<n> of <total> differ:` followed by up to 20 lines `op(x) bits <hex>: pele "<want>", ours "<got>"`. Report the op, the hex bits and both strings.

## Gotchas

- Inputs travel as IEEE-754 bit patterns in hex. A decimal literal copied from a failure line can parse to a neighbouring double. Reproduce with the bits.
- The oracle quiets signalling NaN payloads. The round test compares NaN as a class for that reason. Do not "fix" that by comparing payloads.
- These tests sample. The `prove-js-numerics` skill has the Kani proofs that cover every double for `round` and for `num`'s branch and digit split. `to_string` and `to_fixed` are covered by sampling only.
