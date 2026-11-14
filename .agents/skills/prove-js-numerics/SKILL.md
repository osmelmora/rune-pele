---
name: prove-js-numerics
description: Kani proofs that rune-pele's JavaScript number helpers (`Math.round` and Pele's `num()` coordinate printer in crates/pele-core/src/js.rs) match JavaScript and Pele 1.13.0 for every double. Use when a change touches crates/pele-core/src/js.rs, moves the Pele pin, or a parity diff shows a coordinate off by one hundredth.
---

# Prove the JS numerics

Every number in Pele's SVG goes through `num()` in `vendor/pele/src/svg/builder.ts`. That function computes `Math.round(value * 100)` and then splits the result into whole units and hundredths with double arithmetic. rune-pele reimplements it in `crates/pele-core/src/js.rs`. If the two disagree on even one input, a coordinate prints one hundredth off and byte-parity with Pele is lost for every diagram that hits that value. Random tests rarely reach the inputs that break it, because exact half-way ties such as `-0.125` are a vanishing fraction of all doubles.

This skill checks, for every double, that the Rust code makes the same decisions as JavaScript and Pele. The tool is [Kani](https://model-checking.github.io/kani/) 0.68.0. It is a bounded model checker for Rust that runs on the real code and reasons about `f64` bit for bit. There are no loops here, so its "bounded" is a full proof over all inputs. Kani was chosen because the question is "does this code behave for every input". That question is about the code itself, not about an ordering of events. Kani runs on the Rust source as written and installs through `mise`.

## The properties

Each one is a harness in the `proofs` module at the bottom of `crates/pele-core/src/js.rs`.

| Harness | Property | Harm if it breaks | Time |
| --- | --- | --- | --- |
| `round_is_math_round_for_every_double` | `round(x)` has the same bits as ECMAScript `Math.round(x)` for every double: the nearer integer, the larger one on a tie, `-0` for x in [-0.5, -0]. All NaNs count as one value. | Negative half-way coordinates print one hundredth off. Rust's `f64::round` gives `num(-0.125)` = `-0.13`, where Pele prints `-0.12`. | ~10 s |
| `num_branch_matches_pele` | For NaN and every nonzero integral double with \|n\| ≥ 1e11, `split_hundredths(n)` takes the `String(n / 100)` path exactly when Pele's `num()` does. | Huge or non-finite values print in a different format from Pele. | <1 s |
| `num_split_matches_pele_for_positive_n` | For every integral n in [1, 1e11), `split_hundredths(n)` gives the same whole part and hundredths as Pele's `Math.floor(n / 100)` and `n - q * 100` on doubles. | A coordinate prints with wrong digits. | ~2.5 min |
| `num_split_matches_pele_for_negative_n` | The same for n in (-1e11, -1], including the sign. | A negative coordinate prints with wrong digits. | ~3.5 min |

Together these cover every input of `write_num`. `n = round(value * 100.0)` is integral or NaN or infinite, and the zero case prints `0` in both implementations by inspection. The two split harnesses quantify over `h: u64` and use `h as f64`. Every integral double with magnitude below 1e11 is exactly such a value, so nothing in that range is skipped.

`spec_round` is the ECMAScript definition, written out separately from `round`. `pele_split` is a line-for-line transcription of Pele's `num()` branches.

## Coverage record

- Rust code under proof is `crates/pele-core/src/js.rs`, functions `round`, `write_num` and `split_hundredths`, and the `Hundredths` struct.
- The Pele code transcribed is `vendor/pele/src/svg/builder.ts`, function `num` (lines 33–47 at Pele `3a2f075ee0c3d914f0fad3545fc0f193693975ac`, tag 1.13.0). Its transcription is `pele_split` in the proofs module.
- The spec transcribed is ECMAScript `Math.round` (ECMA-262, the `Math.round` clause), as `spec_round` in the proofs module.
- Last confirmed in sync at rune-pele commit `SYNC_COMMIT_PENDING`.

Move the sync commit forward only after you have read the covered code at the new commit, confirmed that the transcriptions still match it, and seen `mise run prove` pass.

## Does my change touch this?

Run `git diff <sync commit> --stat -- crates/pele-core/src/js.rs scripts/pele-pin.txt`. If either file appears, this skill applies.

- If `scripts/pele-pin.txt` moved, also run `git -C vendor/pele diff <old pin> <new pin> -- src/svg/builder.ts`. Any change to `num` means `pele_split` must be re-transcribed before the proofs mean anything.
- If only `to_string` or `to_fixed` changed, the proofs do not cover them. Run `mise exec -- cargo test -p pele-core --test oracle` instead (see "Not covered" below).

## Run it

- **Everything.** Run `mise run prove`. It runs `cargo kani -p pele-core -j --output-format terse`, which takes about 4 minutes on an M-series Mac with the harnesses in parallel. It passes when the last line reads `Complete - 4 successfully verified harnesses, 0 failures, 4 total.`
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
- **Out of scope.** The diff touches only `to_string` or `to_fixed`. Kani never sees those functions. Rely on the oracle tests.

## Updating after an intended change

When a change to `js.rs` or the Pele pin is deliberate:

1. Re-read `num` in `vendor/pele/src/svg/builder.ts` at the current pin. Update `pele_split` if it changed, and update the line numbers in the coverage record.
2. If `write_num` gained a new path, add a harness that covers it. Then break that path on purpose, confirm the harness fails, and restore it.
3. Run `mise run prove` and `mise exec -- cargo test -p pele-core --test oracle`. Both must pass.
4. Set the sync commit in the coverage record to the commit that contains the change.

## Not covered

- **`to_string` and `to_fixed`.** `to_string` wraps the `ryu-js` crate and `to_fixed` reads Rust's exact decimal expansion. The question there is "same digits for every double", which is too large for Kani's string reasoning. Differential tests check both against the oracle on more than 200,000 doubles per run (`crates/pele-core/tests/oracle.rs`). That is sampling, not proof.
- **`value * 100.0`.** Rust and V8 both multiply binary64 with round-to-nearest-even. That is assumed identical, not proven.
- **Integer printing.** `write!(out, "{whole}")` and `".{f:02}"` are trusted to print decimal digits. The oracle tests exercise them.
- **Other callers.** The proofs cover the helpers, not each diagram's use of them. A diagram that formats a number some other way is outside this skill. Its parity shows up in `pele parity` instead.
