---
name: verify-rune-pele
description: Drive rune-pele's `pele` CLI and its native Rune view the way a porter or consumer does, rendering Mermaid through this port and through Pele 1.13.0, proving byte-parity on Pele's documentation corpus, and capturing the Rune drawing headless. Use after any change to crates/, harness/, mise.toml or the Pele pin, before claiming a diagram type, numeric helper, CLI behavior or Rune view works.
---

# Verify rune-pele

rune-pele is a Rust port of Pele (the Mermaid to SVG renderer, pinned at 1.13.0). Users touch it in two places. The `pele` CLI renders SVG. The `rune-pele` crate draws the same Scene as native Rune elements. Its `diagram` example opens any Mermaid file that way, and its `pie` example shows a palette switch, in a window or headless.

The truth source is Pele itself. `harness/oracle.ts` imports Pele's TypeScript source from `vendor/pele` and is bundled to `harness/dist/oracle.mjs`. Every parity claim compares our output with what that oracle prints. Never compare against a hand-written expected SVG.

All paths below are relative to the repository root. Read `features/README.md` before driving anything.

## Launch

There is no server. Launch means a pinned toolchain, Pele at the pinned commit, a built oracle and a built binary.

1. `mise install`. This installs the Rust, Node and Kani versions pinned in `mise.toml`.
2. `mise run setup`. This clones or checks out `vendor/pele` at `scripts/pele-pin.txt`, runs `npm ci` in `harness/`, and bundles the oracle. Ready when it prints `pele 1.13.0 at 3a2f075…, oracle at harness/dist/oracle.mjs`.
3. `mise exec -- cargo build -p pele-cli`. The binary is `target/debug/pele`.
4. `mise exec -- cargo build -p rune-pele --example pie --example diagram`. The examples are `target/debug/examples/pie` and `target/debug/examples/diagram`. Run it bare for a window, or with `--snap <png>` to draw one frame offscreen and exit. Headless runs need a GPU adapter but no display.

Run every command through `mise exec --` or a `mise run` task. A bare `node` or `cargo` may be a different version, and the oracle's numbers then stop meaning Pele 1.13.0.

Teardown is the Cleanup section. Nothing keeps running between commands. A window the example opens closes with it.

## Doctor

Run `.agents/skills/verify-rune-pele/scripts/doctor.sh` first, and again whenever a result looks wrong. It is read-only. It checks five things:

- The Node that `mise which node` names, which is the one the oracle runs, and rustc match `mise.toml`.
- `vendor/pele` is at the pin with no local edits.
- The oracle bundle's first-line stamp matches the sha256 of `harness/oracle.ts` and the pin, and it answers `num(0.125)=0.13 num(-0.125)=-0.12`.
- `pele-cli` builds.
- The rune-pele `pie` and `diagram` examples build.

It ends with `doctor: ok` or stops at the first `doctor: FAIL <reason>` with the fix. Do not drive a checkout the doctor fails.

The Node line names the Node's architecture, `arm64` for the pinned one. When `MISE_NODE_VERSION` is set, the next line says that it, not `mise.toml`, names the Node. `mise run intel-trig` sets it on purpose to run the oracle on the same version built for x64. Anywhere else, unset it.

## Drive

Drive the CLI. Each command is a user path.

- `pele render <file.mmd>`. Renders with this port (`--engine rune`, the default) and prints the SVG.
- `pele render <file.mmd> --engine pele`. Renders with Pele through the oracle.
- `-o <out.svg>` writes the SVG to a file instead of stdout.
- `pele parity [--type <type>] [--out <dir>] [--corpus <dir>] [--expect-pele-errors <n>]`. Renders Pele's documentation corpus (`vendor/pele/tests/corpus/<type>-docs.json`) through both engines. It prints a per-type table (sources, identical, different, not ported, pele errors) and writes `<dir>/<type>/<i>.mmd`, `<i>.pele.svg`, `<i>.ours.svg` and `<dir>/report.json`.
- Every `<type>-docs.json` the Pele pin holds in `tests/corpus` must be there and hold sources; git names the set at the pin. `--corpus` reads them from another folder, such as a copy to break. Each run empties its `--out` first, and only a folder that is new, empty or marked `.pele-parity` by an earlier run.

Exit codes are part of the contract:

| Exit | Meaning |
| --- | --- |
| 0 | Success. |
| 1 | `parity` failed its gate: a source differs from Pele's, this port does not render one, Pele rejected other than `--expect-pele-errors` sources (default 0), or nothing was compared. |
| 3 | `render --engine rune` hit a diagram type that is not ported. |
| 64 | Usage error: an unknown command or option, an option with no value, or a wrong argument. |
| 65 | The engine rejected the source: Pele, or this port with Pele's message. |
| 66 | Unreadable input file. For `parity`, a corpus file that is missing, not a JSON array of sources, or empty. |
| 70 | The oracle could not answer: it is not built, its Node is not the version `mise.toml` pins, or it failed. |
| 73 | The output cannot be written: the `-o` file, or a `parity --out` that is a file, cannot be made, or is a folder with files that `pele parity` did not write. |

Run it as `mise exec -- target/debug/pele …` after building, or `mise exec -- cargo run -q -p pele-cli -- …`.

Drive the Rune view through the `pie` example and the headless tests. Each is a user path for an app that embeds rune-pele.

- `target/debug/examples/pie --snap <png> [--theme light|dark] [--scene hues] [--script <json>]`. Draws the example's first frame, or its frame after the script, to a PNG at 2× by default, and exits 0.
- `cargo test -p rune-pele --test pie_view -- --nocapture`. Captures pie-pets through `Diagram::view` at 1× and 2×, measures it in pixels, and writes PNGs and reports to `$RUNE_PELE_OUT` or `out/verify/pie-rune/`.
- `cargo test -p pele-core --test scene_walk`. Reads the Scene as the Rune view reads it.
- `mise run rune-corpus`. Draws every corpus source of the eight graph types natively in release, beside Pele's SVG through resvg, and holds each to the pixel limits in `features/rune-view.md`. `mise run check` skips it, because it takes minutes unoptimised.

The recipe for each feature is in `features/`.

## Evidence

Capture every proof with the helper, so the command, stdout, stderr and exit code survive:

```sh
.agents/skills/verify-rune-pele/scripts/capture.sh <run-id> <feature-id> <command> [args...]
```

For example, `capture.sh 2026-11-14-pie render-pele target/debug/pele render tests/fixtures/pie-pets.mmd --engine pele`. Proof lands in `out/verify/<run-id>/<feature-id>/<n>.{cmd,stdout,stderr,exit}`, and the helper passes the exit code through. Give `pele parity` an `--out out/verify/<run-id>/parity` so its SVGs and `report.json` sit with the rest of the proof.

Proof standards:

- Exercise the CLI, not internal functions. A unit test passing does not prove the CLI behavior it covers.
- A Rune view claim needs the PNGs on disk and opened. It also needs the product test's numbers from the same run, not from an earlier one.
- A parity claim needs both sides on disk: `<i>.pele.svg` and `<i>.ours.svg` from the same run, plus the `report.json` row. "Identical" means byte-equal SVG and equal width and height.
- A "type X is ported" claim needs `pele parity --type X` with `different 0` and `not ported 0`. Name any `pele errors` rows, since Pele rejected those sources itself. It also needs the type's wider differential tests passing: `cargo test -p pele-core --test <type>_parity` and, where it exists, `--test <type>_front`.
- A numeric claim needs `mise exec -- cargo test -p pele-core --test oracle` passing. Those tests diff against the oracle on more than 200,000 doubles.
- When an SVG must be looked at, open both files side by side. A byte diff (`diff <i>.pele.svg <i>.ours.svg`) names the first divergent attribute. Report it in the feature's words.

## Cleanup

Runs leave no processes behind. Each oracle call is a short-lived `node` child of the command that spawned it.

- Remove scratch inputs you created outside `out/verify/`.
- Remove `out/parity/` if you ran `pele parity` without `--out`. It is regenerated on every run.
- Keep `out/verify/<run-id>/`. It is the proof, and `/out` is gitignored so it never reaches a commit.
- Never touch `vendor/pele`. Restoring it to the pin is `mise run setup`, not a manual checkout.

## Helpers

- `scripts/doctor.sh`. A read-only readiness check, run as `.agents/skills/verify-rune-pele/scripts/doctor.sh`.
- `scripts/capture.sh <run-id> <feature-id> <command> [args...]`. Runs a command under `mise exec` and keeps its proof.
- `mise run check`. Runs fmt, clippy with `-D warnings`, and every test including the oracle diffs.
- `scripts/harvest-pie.mjs`. Rewrites `tests/fixtures/pie/harvest.json` from the pie sources in `vendor/pele/tests`, run as `mise exec -- node scripts/harvest-pie.mjs`.
- `mise run prove`. Runs the Kani proofs. The `prove-js-numerics` skill owns how to read them.
- `mise run msrv`. Checks pele-core and rune-pele on the Rust their `rust-version` names. `mise run check` runs it.
- `mise run ci`. Runs what CI runs on Linux arm64 (`.github/workflows/ci.yml`): `mise run check` with the workflows linted first, then `pele parity` held to `522 522 0 0 0`.
- `mise run actionlint`. Lints every workflow, with ShellCheck on its shell steps. `scripts/break-ci.sh` (under `mise exec --`) shows that it catches a break.
