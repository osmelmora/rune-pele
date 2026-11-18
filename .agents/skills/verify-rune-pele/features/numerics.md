# JavaScript numerics

Every coordinate Pele writes passes through JavaScript number rules: `Math.round` (ties toward +∞), Pele's `num()` (hundredths, trailing zeros dropped), `String(x)` and `toFixed`. Users of this port get the same digits Pele prints. On a half-hundredth such as `-0.125`, that is `-0.12`, where Rust's own rounding gives `-0.13`.

## Sub-features

- `numerics-round` gives `pele_core::js::round` the same bits as `Math.round`. All NaN results count as one value.
- `numerics-num` makes `pele_core::js::num` print the same string as Pele's `num()`.
- `numerics-string` makes `pele_core::js::to_string` print the same string as `String(x)`.
- `numerics-fixed` makes `pele_core::js::to_fixed` print the same string as `x.toFixed(d)` for d in 0, 1, 2, 3, 20 and 100.
- `numerics-exponential` makes `pele_core::js::to_exponential` print the same string as `x.toExponential(d)` for every d in 0 to 21, and 50 and 100. Treemap's `e` values use it.
- `numerics-precision` makes `pele_core::js::to_precision` print the same string as `x.toPrecision(d)` for every d in 1 to 21, and 50 and 100. Treemap's values with no type use it.
- `numerics-trig` makes `pele_core::js::sin`, `cos` and `log10` V8's fdlibm in plain arithmetic: the very bits of `Math.sin`, `Math.cos` and `Math.log10` on Node for x64, and within one ulp of them on the pinned arm64 Node, with the same whole part. `js::ten_to` gives the very bits of `10 ** n` for a whole `n` on both.
- `numerics-pow` keeps `pele_core::js::pow` within one ulp of `Math.pow`, and gives its very bits where ECMAScript names the answer: NaN for any base to a NaN power and for ±1 to ±∞, where C's `pow` gives 1.
- `numerics-intel` measures Pele's maths on Node built for x64 (an Intel Mac) against the pinned arm64 Node and against the port, and Pele's answers over the corpus and the fixtures on both.

## How to get to it (user POV)

- A crate user calls `pele_core::js::{round, num, write_num, to_string, to_fixed, to_exponential, to_precision, sin, cos, log10, ten_to, pow, max, min}`.
- Every SVG this port writes prints its numbers through these functions.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.

- **Differential run.** Run `$C $R numerics cargo test -p pele-core --test oracle -- --nocapture`. Exit 0. Stdout reports `13 passed`: one test per sub-feature, with `sin` and `cos` apart, `pow` twice, and `sin`, `cos` and `log10` again where fdlibm changes course. The trig and pow tests each print `<n> of <total> bit-equal to Node, the rest one ulp away`. On darwin arm64 with Node 26.13.1 that is 302,863 of 304,334 for `sin` and 303,117 for `cos`. The three `*_where_fdlibm_changes_course` tests print `<op>, <group>: <n> of <total> bit-equal to Node` for each group of `tests/js/fdlibm_edges.rs`: special values and subnormals, under 2^-27, the kernels' and reductions' bounds, n·π/2, huge arguments, powers of two and ten, the bounds of `log`'s series, and the sets of the Intel entry. Every answer is bit-equal where no rounding takes part and within one ulp elsewhere, with the same `floor`. Each test sends more than 200,000 doubles to the oracle. Those are edge cases, every k/200 and k/8 for |k| ≤ 4000 with their neighbouring doubles, and seeded random bit patterns and coordinates. The second pow test prints `pow where ECMAScript names the answer: 189 pairs`: each special value and ±1, ±1/4, ±4 and ±5e-324 to each special power and to ±1, ±2, ±3 and ±1/2, and the largest doubles and the two beside 1 to each special power, every answer exact and bit-equal to Node's. The exponential and precision tests print `<op>: <n> compared, <t> of them exact ties, 0 differ`: on darwin arm64 with Node 26.13.1 that is 215,776 with 946 ties and 210,952 with 889 ties. Their inputs are signed zeros, non-finite values, the extremes, each power of ten from 1e-324 to 1e308 with its neighbours, exact decimal ties and runs of nines, each at every precision, then 100,000 seeded random doubles at a random precision each.
- **Spot check.** Run `$C $R numerics-spot sh -c "printf 'num\t3fc0000000000000\nnum\tbfc0000000000000\n' | node harness/dist/oracle.mjs"`. Stdout is `0.13` then `-0.12`. These are Pele's answers for `0.125` and `-0.125`.
- **Intel run.** Run `$C $R numerics-intel mise run intel-trig`. Exit 0 in about four minutes. It needs Rosetta 2. `mise run intel-node` installs Node 26.13.1 for x64 under `out/intel-node` and prints its path. The doctor then runs with `MISE_NODE_VERSION=path:<that folder>` and must name `x64` and the variable. `out/verify/intel-trig/math.md` holds a row per function, port and group of inputs: `x64 vs port`, `arm64 vs port` and `x64 vs arm64`, over the edges and 10,000,000 seeded random inputs (`mise run intel-trig -- <seed> <count>` for others). It also holds `--jitless` against the JIT, all 0, and Pele's ink on every `#rrggbb`, `0 of 16777216`. `pele-svgs.md` holds `| total | 379779 | 379778 | 1 |`, where the one is `venn_layout`'s unsettled relaxation. `parity-x64.txt` reads `total 522 522 0 0 0`. The run fails if a port marked `same_on_x64` differs from x64 Node, if an ink flips, or if a corpus or fixture source draws apart.
- **Failure shape.** A mismatch fails with `<n> of <total> differ:` followed by up to 20 lines `op(x) bits <hex>: pele "<want>", ours "<got>"`. Report the op, the hex bits and both strings.

## Gotchas

- Inputs travel as IEEE-754 bit patterns in hex. A decimal literal copied from a failure line can parse to a neighbouring double. Reproduce with the bits.
- The oracle quiets signalling NaN payloads. The round test compares NaN as a class for that reason. Do not "fix" that by comparing payloads.
- These tests sample. The `prove-js-numerics` skill has the Kani proofs that cover every double for `round` and for `num`'s branch and digit split. `to_string`, `to_fixed`, and the digits and layout of `to_exponential` and `to_precision` are covered by sampling only; Kani proves only the rounding of their significant digits.
- `sin`, `cos`, `log10` and `pow` are not bit-equal to the pinned Node. The port's `sin`, `cos` and `log10` are V8's fdlibm, which Node for arm64 is built to fuse multiply-adds inside; it calls the platform's `pow`. A result one ulp away changes a coordinate only when the true value is within an ulp of a rounding tie. `parity-harvest` in `parity.md` is the test that would show such a coordinate.
- Node for x64 is V8's fdlibm with no fused operations: on `sin`, `cos` and `log10` it is the port bit for bit, and a copy of the port with `mul_add` wherever clang fuses (`-ffp-contract=on`) is arm64 Node bit for bit on 10,000,000 random inputs each (decision log, "2026-11-18 V8's trig and log10"). libm's `sin`, `cos` and `log10` are off from both, because the libm crate follows FreeBSD's later msun. On `acos`, `asin`, `log2` and `atan2` within ±2^60, x64 Node is libm. `Math.pow` is the operating system's `pow` (`--use-std-math-pow`), so it follows the machine, not V8.
- Breaks for `numerics-pow`: `mise exec -- .agents/skills/prove-js-numerics/scripts/break-numerics.sh pow-of-one-to-nan-is-one pow-of-one-to-infinity-is-one` drops each half of `pow`'s guard, and `pow_matches_math_pow_where_it_names_the_answer` fails with `1 of 189 differ:` and `4 of 189 differ:`.
- Breaks for `numerics-trig`: `mise exec -- .agents/skills/prove-js-numerics/scripts/break-numerics.sh sin-takes-the-wrong-quadrant huge-negative-keeps-its-sign cancellation-takes-one-piece-of-pi log10-scales-subnormals-wrong` each fail a `*_where_fdlibm_changes_course` test with `<n> differ:`. A last bit of a coefficient is beyond the arm64 test, which allows an ulp: `sin-coefficient-off-in-the-last-bit` and `libm-log10-put-back` are caught only by intel-trig's x64 comparison.
- Breaks for `numerics-intel`, each caught by `mise exec -- env INTEL_TRIG_X64_NODE=$(mise run -q intel-node) INTEL_TRIG_SAMPLES=10000 cargo test -q --release -p pele-core --test intel_trig the_maths -- --ignored`, which then fails with `what the decision log measured no longer holds`. Make `asin` in `scripts/intel-trig.mjs` call `Math.acos`, and `libm::asin` is listed. Send inputs with `to_be_bytes` in `node_answers`, and `libm::acos` is listed. Add `1e-7` to `even(&ours)` in `ink_on_every_hex_colour`, and 4 colours are listed. Drop `$node_arch` from the doctor's Node line, and `mise run intel-trig` stops with `intel-trig: FAIL the doctor does not name the x64 Node`.
