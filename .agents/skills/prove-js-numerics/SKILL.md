---
name: prove-js-numerics
description: Kani proofs that rune-pele's JavaScript number helpers (`Math.round`, Pele's `num()` coordinate printer, the rounding of `toExponential` and `toPrecision`, and the special values and range of V8's fdlibm `sin`, `cos` and `log10` in crates/pele-core/src/js.rs) match JavaScript and Pele 1.13.0 for every input they name. Use when a change touches crates/pele-core/src/js.rs, moves the Pele pin, or a parity diff shows a coordinate off by one hundredth.
---

# Prove the JS numerics

Every number in Pele's SVG goes through `num()` in `vendor/pele/src/svg/builder.ts`. That function computes `Math.round(value * 100)` and then splits the result into whole units and hundredths with double arithmetic. rune-pele reimplements it in `crates/pele-core/src/js.rs`. If the two disagree on even one input, a coordinate prints one hundredth off and byte-parity with Pele is lost for every diagram that hits that value. Random tests rarely reach the inputs that break it, because exact half-way ties such as `-0.125` are a vanishing fraction of all doubles.

This skill checks, for every double, that the Rust code makes the same decisions as JavaScript and Pele. The tool is [Kani](https://model-checking.github.io/kani/) 0.68.0. It is a bounded model checker for Rust that runs on the real code and reasons about `f64` bit for bit. The `num` and `round` harnesses have no loops, so their "bounded" is a full proof over all inputs. The significant-digit harness loops over at most 21 digits, the most treemap keeps, and its unwind of 23 covers every pass, so it is a full proof up to that length. The fdlibm harnesses take arguments that reach no loop: every loop of the port is in `__kernel_rem_pio2` and the large branch of `__ieee754_rem_pio2`. So they unwind once, and Kani's unwinding checks, which are on by default, prove that no loop runs. Kani was chosen because the question is "does this code behave for every input". That question is about the code itself, not about an ordering of events. Kani runs on the Rust source as written and installs through `mise`.

## The properties

Each one is a harness in the `proofs` module at the bottom of `crates/pele-core/src/js.rs`.

| Harness | Property | Harm if it breaks | Time |
| --- | --- | --- | --- |
| `round_is_math_round_for_every_double` | `round(x)` has the same bits as ECMAScript `Math.round(x)` for every double: the nearer integer, the larger one on a tie, `-0` for x in [-0.5, -0]. All NaNs count as one value. | Negative half-way coordinates print one hundredth off. Rust's `f64::round` gives `num(-0.125)` = `-0.13`, where Pele prints `-0.12`. | ~10 s |
| `num_branch_matches_pele` | For NaN and every nonzero integral double with \|n\| ≥ 1e11, `split_hundredths(n)` takes the `String(n / 100)` path exactly when Pele's `num()` does. | Huge or non-finite values print in a different format from Pele. | <1 s |
| `num_split_matches_pele_for_positive_n` | For every integral n in [1, 1e11), `split_hundredths(n)` gives the same whole part and hundredths as Pele's `Math.floor(n / 100)` and `n - q * 100` on doubles. | A coordinate prints with wrong digits. | ~2.5 min |
| `num_split_matches_pele_for_negative_n` | The same for n in (-1e11, -1], including the sign. | A negative coordinate prints with wrong digits. | ~3.5 min |
| `sin_and_cos_are_nan_for_nan_and_the_infinities` | For every NaN and both infinities, `sin` and `cos` answer NaN. | A non-finite angle draws as a number where Pele writes `NaN`. | ~6 s |
| `under_2_to_the_minus_27_sin_is_x_and_cos_is_one` | For every double under 2^-27 in magnitude, ±0 and the subnormals among them, `sin(x)` has the bits of `x` and `cos(x)` those of 1. | A tiny angle loses its sign or is rounded, where V8's kernels answer it as it is. | ~6 s |
| `up_to_a_quarter_turn_sin_and_cos_lie_in_minus_one_to_one` | For every double up to about π/4 in magnitude, which fdlibm hands to its kernels unreduced, `sin` and `cos` lie in [-1, 1]. | A kernel's polynomial or its cancellation is wrong, and a pie slice or a radar point leaves its circle. | ~2 min |
| `log10_answers_ecmascript_at_its_special_values` | `log10` is NaN for NaN and every negative double, -∞ at ±0, +0 at 1 and +∞ at +∞. | An axis or a treemap reads a number where Pele reads NaN or an infinity, or `-0` for `log10(1)`. | ~2 s |
| `significant_digits_round_to_the_nearest_and_ties_to_the_larger` | For every run of 1 to 21 significant digits, every digit after them and whether anything nonzero follows it, `round_significant` gives the spec's `n` of `toExponential` and `toPrecision`: the nearer of the two numbers with as many digits, the larger on a tie, with a carry past the first digit moving the exponent. | A treemap value prints a digit off where the next digit is a 5, such as `1.2345678e3` with `.3e` as `1.234e+3` where Pele prints `1.235e+3`, or a run of nines loses its carry. | ~20 s |

Together these cover every input of `write_num`. `n = round(value * 100.0)` is integral or NaN or infinite, and the zero case prints `0` in both implementations by inspection. The two split harnesses quantify over `h: u64` and use `h as f64`. Every integral double with magnitude below 1e11 is exactly such a value, so nothing in that range is skipped.

`spec_round` is the ECMAScript definition, written out separately from `round`. `pele_split` is a line-for-line transcription of Pele's `num()` branches.

The fdlibm harnesses state ECMAScript's special values and V8's own early returns, not a transcription of V8: the port is V8's code, and the oracle tests and `mise run intel-trig` hold its bits to Node's. Three properties ran past 9 minutes in Kani and are kept out: `sin` odd and `cos` even up to π/4, the same from there to 2^19·π/2, and both in [-1, 1] from π/4 to 2^19·π/2. `tests/fdlibm_properties.rs` samples them, over fdlibm's edges and a million doubles of any bits, huge ones among them.

The significant-digit harness states the spec's rule as distances. Past the kept digits, the exact value holds `next` + ε tenths of the last kept place, with ε in [0, 1). The harness measures both distances in twentieths of that place. A nonzero ε decides every comparison as ε = 1/2 does, because `next` is whole, so one bit stands for all of them. The expected digits are the kept ones plus one or nothing, by schoolbook addition from the right (`add`). A first form compared the digits as `u128` values and ran past ten minutes; digit by digit it takes seconds. The harness covers the rounding step only: which digits `significant` reads and how `to_exponential` and `to_precision` lay them out are the oracle tests' (see "Not covered").

## Coverage record

- Rust code under proof is `crates/pele-core/src/js.rs`, functions `round`, `write_num`, `split_hundredths` and `round_significant`, and the `Hundredths` struct; and `sin`, `cos` and `log10`, with the `fdlibm` module they call.
- The V8 code the `fdlibm` module ports is `src/base/ieee754.cc` of V8 14.6.202.34, the V8 of Node 26.13.1 (`deps/v8/src/base/ieee754.cc` at Node's tag `v26.13.1`, sha256 `badd30a2643ed785dcfbbce42f95c2522ccea3605cd39e2933467688aae1de14`): `sin`, `cos`, `__ieee754_rem_pio2`, `__kernel_rem_pio2` at precision 2, `__kernel_sin`, `__kernel_cos`, `log10` and `log`. Node builds it without `V8_USE_LIBM_TRIG_FUNCTIONS`, so `Math.sin` and `Math.cos` are these and not glibc's (its `--v8-options` has no `--use-libm-trig-functions`).
- The Pele code transcribed is `vendor/pele/src/svg/builder.ts`, function `num` (lines 33–47 at Pele `3a2f075ee0c3d914f0fad3545fc0f193693975ac`, tag 1.13.0). Its transcription is `pele_split` in the proofs module.
- The spec transcribed is ECMAScript `Math.round` (ECMA-262, the `Math.round` clause), as `spec_round` in the proofs module.
- The spec stated for `round_significant` is the choice of `n` and `e` in ECMA-262's `Number.prototype.toExponential` (step 10.a) and `Number.prototype.toPrecision` (step 10.a): the `n` of as many digits whose value is nearest the exact value, the larger if two are. The harness states it as the two distances and `add`.
- Last confirmed in sync at rune-pele commit `f4b5d68`, on branch `fdlibm-trig`, which ported V8's `sin` and `cos`, moved `log10` and `ten_to` into `js.rs`, and added the four fdlibm harnesses. Before it, `3fa75f0`, which guarded `pow` and left the covered code as it was, `c3978d0`, which added `round_significant` and its harness, and `1e016df22c6baa8c31b73793a189d5c348d3e89d`.

Move the sync commit forward only after you have read the covered code at the new commit, confirmed that the transcriptions still match it, and seen `mise run prove` pass.

## Does my change touch this?

Run `git diff <sync commit> --stat -- crates/pele-core/src/js.rs scripts/pele-pin.txt`. If either file appears, this skill applies.

- If `scripts/pele-pin.txt` moved, also run `git -C vendor/pele diff <old pin> <new pin> -- src/svg/builder.ts`. Any change to `num` means `pele_split` must be re-transcribed before the proofs mean anything.
- If `mise.toml`'s Node moved, diff `deps/v8/src/base/ieee754.cc` between the two Node tags, and check `node --v8-options` still has no `--use-libm-trig-functions`. A change to the functions the `fdlibm` module ports means porting it again, then `mise run intel-trig`.
- If only `to_string`, `to_fixed`, `significant` or the layout of `to_exponential` and `to_precision` changed, the proofs do not cover them. Run `mise exec -- cargo test -p pele-core --test oracle` instead (see "Not covered" below).

## Run it

- **Everything.** Run `mise run prove`. It runs `cargo kani -p pele-core -j --output-format terse`, which takes about 3 to 4 minutes on an M-series Mac with the harnesses in parallel, and longer while other builds share the machine. It passes when the last line reads `Complete - 9 successfully verified harnesses, 0 failures, 9 total.`
- **One harness.** Run `mise exec -- cargo kani -p pele-core --harness <name>`.
- **A failing input as a value.** Add `-Z concrete-playback --concrete-playback=print` to the one-harness command. Kani prints the input bytes with a comment showing the value, for example `// -0.5`.

Kani's first run per machine downloads its bundle into `~/.kani`. That is `mise install` followed by any `cargo kani`. Expect a minute or two.

## Reading a result

A failure prints `Failed Checks: <assertion>`, the harness and line, and `VERIFICATION:- FAILED`. Get the input with concrete playback, then say it in the code's words.

- **Example 1.** Round was changed to `x.round()`. Kani's input was `-0.5`. In plain words, `Math.round(-0.5)` is `-0`, but the new `round` gives `-1`, so `num(-0.005)` would print `-0.01` where Pele prints `0`.
- **Example 2.** The split was changed to compute `whole` as `(n.abs() / 100.0).round()`. Kani's input was `n = 2097152`. In plain words, Pele prints the coordinate `20971.52` as `20971.52`, but the new split prints `20972.52`.

To tell a real bug from a modeling error, decide which side moved:

- **Real bug.** The Rust code changed and the transcriptions still match `builder.ts` and ECMA-262. Report the counterexample. Do not edit the harness to make it pass.
- **Modeling error.** `pele_split` or `spec_round` no longer says what its source says. The usual cause is a Pele pin bump that changed `num`. Fix the transcription to match the source line by line, rerun, and only then judge the Rust code.
- **Out of scope.** The diff touches only `to_string`, `to_fixed`, `significant` or the layout of `to_exponential` and `to_precision`. Kani never sees those. Rely on the oracle tests.

## Updating after an intended change

When a change to `js.rs` or the Pele pin is deliberate:

1. Re-read `num` in `vendor/pele/src/svg/builder.ts` at the current pin. Update `pele_split` if it changed, and update the line numbers in the coverage record.
2. If `write_num` gained a new path, add a harness that covers it. Then break that path on purpose, confirm the harness fails, and restore it. `scripts/break-numerics.sh [name...]` (under `mise exec --`) does that for the significant digits: it breaks the rounding, the carry, the digit count and the layout one at a time, and shows the harness or the oracle test of each fail. It also drops each half of `pow`'s guard and shows its oracle test fail, and breaks the fdlibm `sin`, `cos` and `log10` 13 ways: a NaN, a sign, a half square and `log10(1)`'s zero for their harnesses, a quadrant, a sign on huge arguments, the cancellation's second piece of π/2 and the subnormal scale for the oracle tests, a tail's sign, a quarter added and a biased exponent for `tests/fdlibm_properties.rs`, and a last bit of `sin`'s `S1` and `libm::log10` put back for intel-trig's comparison with x64 Node. Add a break there for a new path.
3. Run `mise run prove`, `mise exec -- cargo test -p pele-core --test oracle --test fdlibm_properties` and, for a change to the `fdlibm` module, `mise run intel-trig`. All must pass.
4. Set the sync commit in the coverage record to the commit that contains the change.

## Not covered

- **`to_string`, `to_fixed`, and the digits and layout of `to_exponential` and `to_precision`.** `to_string` wraps the `ryu-js` crate. `to_fixed` and `significant` read Rust's exact decimal expansion (`{:.1100}`), and `to_precision` places the point or the exponent. Only the rounding step of the last two is proved. The question there is "same digits for every double", which is too large for Kani's string reasoning. Differential tests check them against the oracle on more than 200,000 inputs per run (`crates/pele-core/tests/oracle.rs`); for `to_exponential` and `to_precision` that is every power of ten, exact ties and runs of nines at every precision up to 21, and 100,000 seeded random doubles. That is sampling, not proof.
- **The bits of `sin`, `cos` and `log10`.** They are V8's fdlibm in plain arithmetic, which Node on x64 is bit for bit and Node on arm64 is with fused multiply-adds. Kani proves only the special values, the arguments under 2^-27 and the range up to π/4. The bits are held to Node by sampling: `tests/oracle.rs` against the pinned arm64 Node, bit for bit where no rounding takes part and within one ulp elsewhere, on more than 300,000 inputs per function and on the groups of `tests/js/fdlibm_edges.rs`; and `mise run intel-trig` against x64 Node bit for bit, on the same groups and 10,000,000 random inputs each. The arm64 test cannot see a last-bit error in the port, such as a coefficient one bit off: only the x64 comparison can. Oddness, evenness and the range past π/4 are `tests/fdlibm_properties.rs`'s, sampled. See `numerics-trig` in `.agents/skills/verify-rune-pele/features/numerics.md`.
- **`pow`.** It wraps the `libm` crate, and Node calls the platform's `pow`. So it is within one ulp of Node, not bit-equal. The oracle tests measure the gap on more than 300,000 inputs. Where ECMAScript and C name different answers, `pow` guards the ECMAScript one, NaN for any base to a NaN power and for ±1 to ±∞, and `pow_matches_math_pow_where_it_names_the_answer` holds every such answer to Node's bits. For a whole power of ten, `ten_to` parses `1e{n}` instead, which is Node's on both machines.
- **`max` and `min`.** Unit tests in `js.rs` hold them to ECMAScript's NaN and signed-zero rules.
- **`value * 100.0`.** Rust and V8 both multiply binary64 with round-to-nearest-even. That is assumed identical, not proven.
- **Integer printing.** `write!(out, "{whole}")` and `".{f:02}"` are trusted to print decimal digits. The oracle tests exercise them.
- **Other callers.** The proofs cover the helpers, not each diagram's use of them. A diagram that formats a number some other way is outside this skill. Its parity shows up in `pele parity` instead.
