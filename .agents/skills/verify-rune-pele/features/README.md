# rune-pele verification map

This directory is the maintained source for verifying what users of rune-pele can observe. Read the index before driving the CLI or the Rune example, then use the matching feature file as the recipe.

## Baseline preconditions

- `mise install` and `mise run setup` have run in this checkout.
- `.agents/skills/verify-rune-pele/scripts/doctor.sh` prints `doctor: ok`.
- `target/debug/pele` is built from the working tree under test (`mise exec -- cargo build -p pele-cli`).
- `target/debug/examples/pie` and `target/debug/examples/diagram` are built from the same tree (`mise exec -- cargo build -p rune-pele --example pie --example diagram`).
- The fixtures `tests/fixtures/pie-pets.mmd` and `tests/fixtures/flow.mmd` (ported types: a pie and a flowchart) are unchanged. Every type of Pele 1.13.0 is ported and drawn natively, so there is no not-ported or not-opened fixture.
- Each run picks a fresh `<run-id>` and writes only under `out/verify/<run-id>/`.

## Driving conventions

- Run every command through `mise exec --` or `capture.sh`, from the repository root.
- Treat commands as literal. Flags are exact.
- Pele's output is the expected value. Never hand-edit an expected SVG, and never edit `vendor/pele`.
- Two runs may proceed side by side when their `<run-id>` and `--out` differ. Nothing else is shared except the read-only `vendor/pele` and `harness/dist`.

## Proof and skip reporting

- CLI proof is the command, stdout, stderr and exit code, all captured by `capture.sh`.
- Parity proof is both SVGs and the `report.json` row from the same run.
- Record the feature ID with every artifact. `capture.sh` puts it in the path.
- Report an unreachable path with the command tried and the unmet precondition.
- A sub-feature verified through one entry point is not verified through another.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then has exactly four H2 sections in this order:

1. `Sub-features` lists short IDs with one line each.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with the pele CLI` starts with `Preconditions:` and pairs each action with an exact command and an observable result.
4. `Gotchas` lists traps that waste or invalidate a run.

## Features

- [Render a Mermaid file](./render.md) covers both engines, file output, the not-ported path and bad input.
- [Corpus parity report](./parity.md) covers the scoreboard, per-type runs, the files it writes, its exit gate and the wider differential test of each ported type.
- [JavaScript numerics](./numerics.md) covers `Math.round`, Pele's `num()`, `String(x)`, `toFixed`, and how close `sin`, `cos` and `pow` are, against the oracle.
- [Native Rune view](./rune-view.md) covers every type `lower::NATIVE` lists, drawn as Rune elements: the examples' `--snap`, the palette switch, the headless product tests, the lowerings, links and tooltips, the fallback, and Pele's SVG through resvg side by side.
- [Using rune-pele from another project](./consumer.md) covers an app in a workspace of its own: its build with Rune's patches, its view, its manifest against the workspace's and the README's, and the failure without the patches. It also covers the API an app relies on: the documented items, the builders, the types that can grow, the oldest Rust and cargo-semver-checks against the last release.
