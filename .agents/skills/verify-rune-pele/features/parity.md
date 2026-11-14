# Corpus parity report

`pele parity` is the scoreboard for the port. It renders every example in Pele's documentation corpus through Pele and through this port. It then reports per diagram type how many outputs are byte-identical, how many differ, and how many this port cannot render yet. It also leaves both SVGs on disk for inspection.

## Sub-features

- `parity-all` reports every corpus type and a total row.
- `parity-type` limits the run to one type with `--type`.
- `parity-files` writes each source, Pele's SVG, our SVG when there is one, and `report.json`.
- `parity-gate` exits 1 when any source this port renders differs from Pele.
- `parity-unknown-type` rejects a type with no corpus file.
- `parity-harvest` renders every source of a ported type that Pele's own tests hold, and this port's own cases, through both engines.

## How to get to it (user POV)

- `pele parity` from a terminal.
- `pele parity --type <type> --out <dir>` for one type into a chosen folder.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.

- **Whole corpus.** Run `$C $R parity-all target/debug/pele parity --out out/verify/$R/parity`. Exit 0 unless a ported source differs. Stdout has one row per type from `agentflow` to `xychart`, 33 rows, then a `total` row with `522` sources at Pele 1.13.0. With pie ported the `pie` row is `2 2 0 0 0`, every other row has all its sources under `not ported`, and the total is `522 2 0 520 0`. `out/verify/$R/parity/report.json` has the same counts.
- **One type.** Run `$C $R parity-type target/debug/pele parity --type pie --out out/verify/$R/parity-pie`. Stdout has a `pie` row and a `total` row that both read `2 2 0 0 0`.
- **Files.** List `out/verify/$R/parity-pie/pie/`. For each index `i` from 0 to sources-1 there is `<i>.mmd` and `<i>.pele.svg`. `<i>.ours.svg` exists exactly when this port rendered the source. For pie both exist, and `cmp <i>.pele.svg <i>.ours.svg` prints nothing. `<i>.pele.svg` equals `pele render <i>.mmd --engine pele -o …` byte for byte.
- **Gate.** For a type this port renders, `different` must be 0 and the exit 0. A nonzero `different` with exit 1 is a parity regression. Report the first differing index and run `diff out/verify/$R/parity/<type>/<i>.pele.svg out/verify/$R/parity/<type>/<i>.ours.svg`. A source Pele renders and this port rejects also counts as `different`, and stderr names it with the message.
- **Wider test of a ported type.** Run `$C $R parity-harvest cargo test -q -p pele-core --test pie_parity -- --nocapture`. Exit 0 and `4 passed`. Stdout has one line per set that ends in `0 different`: `harvested pie sources: 323 inputs`, `mutated pie sources: 4822 inputs`, `pie cases: 223 inputs` and `random pies: 3000 inputs`. A failure lists each input with its first differing byte, or with both rejection messages.
- **Harvest is current.** Run `$C $R parity-harvest node scripts/harvest-pie.mjs`, then `git status --short tests/fixtures/pie`. The status is empty at the Pele pin the harvest was made at, which `harvest.json` records as `pele`.
- **Unknown type.** Run `$C $R parity-unknown-type target/debug/pele parity --type nope`. Exit 64, stderr names `vendor/pele/tests/corpus`.

## Gotchas

- Without `--out` the files go to `out/parity/`, which every run overwrites. Proof belongs under `out/verify/<run-id>/`.
- `identical` requires byte-equal SVG and equal width and height. Same-looking SVG with a different attribute order is a difference.
- `pele errors` counts sources Pele itself rejects at 1.13.0. They are neither passes nor failures of this port.
- The corpus files are `-docs.json` only. `agentflow-reference.json` is not part of the count.
- The corpus has 2 pie sources. The claim that pie is ported rests on `parity-harvest`, which has 8,368.
- After a change of the Pele pin, rerun `scripts/harvest-pie.mjs` and commit the new `harvest.json` before reading `parity-harvest`.
- The pie cases with render options go through the oracle's `render_with` op. A new option needs a line in `render_with` of `crates/pele-core/tests/pie_parity.rs`, which fails on an option it does not map.
- The run makes one `node` call for the whole corpus. It takes a few seconds, so a long stall points at the oracle.
