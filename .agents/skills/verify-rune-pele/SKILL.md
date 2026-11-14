---
name: verify-rune-pele
description: Drive rune-pele's `pele` CLI the way a porter or consumer does, rendering Mermaid through this port and through Pele 1.13.0 and proving byte-parity on Pele's documentation corpus. Use after any change to crates/, harness/, mise.toml or the Pele pin, before claiming a diagram type, numeric helper or CLI behavior works.
---

# Verify rune-pele

rune-pele is a Rust port of Pele (the Mermaid to SVG renderer, pinned at 1.13.0). Users touch it through the `pele` CLI today. The Rune adapter crate does not exist yet. When it lands, add its surface to this skill and to `features/`.

The truth source is Pele itself. `harness/oracle.ts` imports Pele's TypeScript source from `vendor/pele` and is bundled to `harness/dist/oracle.mjs`. Every parity claim compares our output with what that oracle prints. Never compare against a hand-written expected SVG.

All paths below are relative to the repository root. Read `features/README.md` before driving anything.

## Launch

There is no server. Launch means a pinned toolchain, Pele at the pinned commit, a built oracle and a built binary.

1. `mise install`. This installs the Rust, Node and Kani versions pinned in `mise.toml`.
2. `mise run setup`. This clones or checks out `vendor/pele` at `scripts/pele-pin.txt`, runs `npm ci` in `harness/`, and bundles the oracle. Ready when it prints `pele 1.13.0 at 3a2f075…, oracle at harness/dist/oracle.mjs`.
3. `mise exec -- cargo build -p pele-cli`. The binary is `target/debug/pele`.

Run every command through `mise exec --` or a `mise run` task. A bare `node` or `cargo` may be a different version, and the oracle's numbers then stop meaning Pele 1.13.0.

Teardown is the Cleanup section. Nothing keeps running between commands.

## Doctor

Run `.agents/skills/verify-rune-pele/scripts/doctor.sh` first, and again whenever a result looks wrong. It is read-only. It checks four things:

- Node and rustc match `mise.toml`.
- `vendor/pele` is at the pin with no local edits.
- The oracle bundle's first-line stamp matches the sha256 of `harness/oracle.ts` and the pin, and it answers `num(0.125)=0.13 num(-0.125)=-0.12`.
- `pele-cli` builds.

It ends with `doctor: ok` or stops at the first `doctor: FAIL <reason>` with the fix. Do not drive a checkout the doctor fails.

## Drive

Drive the CLI. Each command is a user path.

- `pele render <file.mmd>`. Renders with this port (`--engine rune`, the default) and prints the SVG.
- `pele render <file.mmd> --engine pele`. Renders with Pele through the oracle.
- `-o <out.svg>` writes the SVG to a file instead of stdout.
- `pele parity [--type <type>] [--out <dir>]`. Renders Pele's documentation corpus (`vendor/pele/tests/corpus/<type>-docs.json`) through both engines. It prints a per-type table (sources, identical, different, not ported, pele errors) and writes `<dir>/<type>/<i>.mmd`, `<i>.pele.svg`, `<i>.ours.svg` and `<dir>/report.json`.

Exit codes are part of the contract:

| Exit | Meaning |
| --- | --- |
| 0 | Success. |
| 1 | `parity` found a ported source whose SVG differs from Pele's. |
| 3 | `render --engine rune` hit a diagram type that is not ported. |
| 64 | Usage error. |
| 65 | The engine rejected the source: Pele, or this port with Pele's message. |
| 66 | Unreadable input file. |

Run it as `mise exec -- target/debug/pele …` after building, or `mise exec -- cargo run -q -p pele-cli -- …`.

The recipe for each feature is in `features/`.

## Evidence

Capture every proof with the helper, so the command, stdout, stderr and exit code survive:

```sh
.agents/skills/verify-rune-pele/scripts/capture.sh <run-id> <feature-id> <command> [args...]
```

For example, `capture.sh 2026-11-14-pie render-pele target/debug/pele render tests/fixtures/pie-pets.mmd --engine pele`. Proof lands in `out/verify/<run-id>/<feature-id>/<n>.{cmd,stdout,stderr,exit}`, and the helper passes the exit code through. Give `pele parity` an `--out out/verify/<run-id>/parity` so its SVGs and `report.json` sit with the rest of the proof.

Proof standards:

- Exercise the CLI, not internal functions. A unit test passing does not prove the CLI behavior it covers.
- A parity claim needs both sides on disk: `<i>.pele.svg` and `<i>.ours.svg` from the same run, plus the `report.json` row. "Identical" means byte-equal SVG and equal width and height.
- A "type X is ported" claim needs `pele parity --type X` with `different 0` and `not ported 0`. Name any `pele errors` rows, since Pele rejected those sources itself. It also needs the type's wider differential test passing, such as `cargo test -p pele-core --test pie_parity` for pie.
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
