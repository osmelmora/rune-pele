# Render a Mermaid file

A user hands `pele render` a Mermaid file and gets SVG back, from this port by default or from Pele 1.13.0 with `--engine pele`. For a ported type the two outputs are the same bytes. Pie and flowchart are the ported types today. A type this port has not ported yet fails loudly with exit 3 and names the type, rather than printing a wrong drawing. A source Pele rejects is rejected by this port with Pele's message.

## Sub-features

- `render-rune` renders with this port and prints the SVG to stdout.
- `render-pele` renders with Pele through the oracle and prints the SVG to stdout.
- `render-out` writes the SVG to the `-o` path instead of stdout.
- `render-not-ported` reports a type this port lacks, with exit 3.
- `render-rejected` reports a source that cannot be detected or parsed, with exit 65 from either engine.
- `render-usage` reports a missing file argument, an unknown engine or an unreadable file.

## How to get to it (user POV)

- `pele render <file.mmd>` from a terminal.
- `pele render <file.mmd> --engine pele` to see what Pele draws.
- `pele render <file.mmd> [--engine …] -o <out.svg>` to save the drawing.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.

- **Pele engine.** Run `$C $R render-pele target/debug/pele render tests/fixtures/pie-pets.mmd --engine pele`. Exit 0. `0.stdout` starts with `<svg xmlns="http://www.w3.org/2000/svg" class="pele pele-pie" viewBox="0 0 397 352"` and contains the label `Dogs`.
- **File output.** Run `$C $R render-out target/debug/pele render tests/fixtures/pie-pets.mmd --engine pele -o out/verify/$R/render-out/pie.svg`. Exit 0 and `0.stdout` is empty. The file is the stdout SVG without its trailing newline, so `diff <(cat out/verify/$R/render-out/pie.svg; echo) out/verify/$R/render-pele/0.stdout` prints nothing.
- **This port, ported type.** Run `$C $R render-rune target/debug/pele render tests/fixtures/pie-pets.mmd`. Exit 0 and `0.stderr` is empty. Stdout equals the `render-pele` stdout byte for byte, so `cmp out/verify/$R/render-rune/0.stdout out/verify/$R/render-pele/0.stdout` prints nothing. For another type, use a file of a type whose `pele parity` row has `not ported 0`.
- **This port, flowchart.** Run `$C $R render-rune target/debug/pele render tests/fixtures/flow.mmd`, then the same with `--engine pele`. Both exit 0 with empty stderr. Both stdout files start with `<svg xmlns="http://www.w3.org/2000/svg" class="pele pele-flowchart" viewBox="0 0 199 64"`, hold the labels `Start` and `End`, and are the same bytes, so `cmp` on the two prints nothing.
- **Not ported.** Run `$C $R render-not-ported target/debug/pele render tests/fixtures/not-ported.mmd`. Exit 3, stdout empty, stderr `not ported: rune-pele cannot render timeline yet`. When timeline is ported, give `not-ported.mmd` a source of a type that `pele parity` still shows as `not ported`.
- **Rejected source.** Write `hello there` to `out/verify/$R/hello.mmd`. Run `$C $R render-rejected target/debug/pele render out/verify/$R/hello.mmd --engine pele`. Exit 65, stderr `pele: PeleError: No diagram type detected for the given text.` Run it again without `--engine pele`. Exit 65, stdout empty, stderr `rune-pele: No diagram type detected for the given text.`
- **Rejected source of a ported type.** Write a `pie` line and then ` "a" 1` to `out/verify/$R/bad-pie.mmd`. Run `$C $R render-rejected target/debug/pele render out/verify/$R/bad-pie.mmd`, then again with `--engine pele`. Both exit 65. After `rune-pele: ` and `pele: PeleError: ` the two stderr files hold the same four lines: `Parse error on line 2:`, the source line, a line of dashes ending in `^`, and ``Expecting token of type ':' but found `1`.``
- **Usage errors.** Run `$C $R render-usage target/debug/pele render`, then `… render tests/fixtures/pie-pets.mmd --engine foo`, then `… render nope.mmd`. Exits 64, 64 and 66. Stderr is the usage text, then `unknown engine "foo"; use rune or pele`, then `nope.mmd: No such file or directory (os error 2)`.

## Gotchas

- Stdout output ends with a newline that `-o` output lacks. Compare files with that in mind, or compare `-o` outputs to each other.
- The `rune` engine asks the oracle for the type name only to word the not-ported message. A not-ported exit therefore still needs a working oracle. Rendering a ported type and rejecting a source need no oracle.
- A rejection from the `rune` engine is Pele's message after `rune-pele: `. The Pele engine prints JavaScript's `String(error)`, which puts `PeleError: ` before the same message.
- `--engine pele` uses the corpus test's pinned measurer (Arial metric tables) and clock (`2026-01-15T12:00Z`). Gantt output from another clock is not a parity failure.
- A run with no doctor pass can print Pele output from a stale oracle bundle. That bundle may not be Pele 1.13.0.
