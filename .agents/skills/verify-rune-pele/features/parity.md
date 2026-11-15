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

- **Whole corpus.** Run `$C $R parity-all target/debug/pele parity --out out/verify/$R/parity`. Exit 0 unless a ported source differs. Stdout has one row per type from `agentflow` to `xychart`, 33 rows, then a `total` row with `522` sources at Pele 1.13.0. With pie and flowchart ported the `pie` row is `2 2 0 0 0`, the `flowchart` row is `114 114 0 0 0`, every other row has all its sources under `not ported`, and the total is `522 116 0 406 0`. `out/verify/$R/parity/report.json` has the same counts.
- **One type.** Run `$C $R parity-type target/debug/pele parity --type pie --out out/verify/$R/parity-pie`. Stdout has a `pie` row and a `total` row that both read `2 2 0 0 0`. With `--type flowchart --out out/verify/$R/parity-flowchart` both rows read `114 114 0 0 0`.
- **Files.** List `out/verify/$R/parity-pie/pie/`. For each index `i` from 0 to sources-1 there is `<i>.mmd` and `<i>.pele.svg`. `<i>.ours.svg` exists exactly when this port rendered the source. For pie both exist, and `cmp <i>.pele.svg <i>.ours.svg` prints nothing. `<i>.pele.svg` equals `pele render <i>.mmd --engine pele -o …` byte for byte.
- **Gate.** For a type this port renders, `different` must be 0 and the exit 0. A nonzero `different` with exit 1 is a parity regression. Report the first differing index and run `diff out/verify/$R/parity/<type>/<i>.pele.svg out/verify/$R/parity/<type>/<i>.ours.svg`. A source Pele renders and this port rejects also counts as `different`, and stderr names it with the message.
- **Wider test of a ported type.** Run `$C $R parity-harvest cargo test -q -p pele-core --test pie_parity -- --nocapture`. Exit 0 and `4 passed`. Stdout has one line per set that ends in `0 different`: `harvested pie sources: 323 inputs`, `mutated pie sources: 4822 inputs`, `pie cases: 223 inputs` and `random pies: 3000 inputs`. A failure lists each input with its first differing byte, or with both rejection messages.
- **Wider test of flowchart.** Run `$C $R parity-harvest cargo test -q -p pele-core --test flowchart_parity -- --nocapture`. Exit 0 and `5 passed`. Stdout has eight lines that end in `0 different`: `flowchart cases: 80 inputs`, `harvested flowchart sources: 2536 inputs`, the same 2536 once for each `maxWidth` of 320, 480, 640 and 900, `mutated flowchart sources: 8702 inputs` and `random flowcharts: 4000 inputs`. That is 25,462 inputs. It also prints how many inputs each width makes Pele draw another way, which is how many go through the tight and turned attempts: 36, 14, 7 and 3 of the harvest, and 1370 of the random flowcharts. The random test fails when no output reaches one of the draw paths it names, such as a casing or a struck group title.
- **Wider test of usecase.** Run `$C $R parity-harvest cargo test -q -p pele-core --test usecase_parity -- --nocapture`. Exit 0 and `6 passed`. Stdout has ten lines that end in `0 different`: `hand-written use case sources: 59 inputs`, `harvested use case sources: 4936 inputs`, the same 4936 once for each `maxWidth` of 320, 480, 640 and 900, `mutated use case sources: 10927 inputs`, `worst use case sources: 22 inputs` and `random use case diagrams: 4000 inputs`. That is 39,688 inputs. It also prints how many inputs each width makes Pele draw another way, which is how many are turned to run down: 404 and 56 of the harvest at 320 and 480, none at 640 and 900, and 336 of the random diagrams. The random test fails when no output reaches one of the 50 draw paths it names, such as a hollow marker showing the fill under it or a boundary title moved clear of an edge.
- **Front end of usecase.** Run `$C $R parity-harvest cargo test -q -p pele-core --test usecase_front -- --nocapture`. Exit 0 and `4 passed`. Stdout has four lines that end in `0 different`, 21,922 inputs in all, each with its rejections counted by kind: `syntax, lexer`, `syntax, parser`, `syntax, json` and `semantic, model`. The harvest line must show more than 10 of each kind.
- **The usecase tests can fail.** Run `$C $R parity-harvest node scripts/break-usecase.mjs`. Exit 0. Stdout has five lines that start `red`, three for the front end and two for drawing, and two that start `green`. A line that starts `UNNOTICED` names a break no test caught.
- **Harvest is current.** Run `$C $R parity-harvest node scripts/harvest-pie.mjs`, then `git status --short tests/fixtures/pie`. The status is empty at the Pele pin the harvest was made at, which `harvest.json` records as `pele`. The flowchart harvest has `scripts/harvest-flowchart.mjs` and `tests/fixtures/flowchart`.
- **Usecase harvest is current.** Run `$C $R parity-harvest node scripts/harvest-usecase.mjs`, then `git status --short tests/fixtures/usecase`. The status is empty at the Pele pin `harvest.json` records. The run prints `4936 sources and 10927 mutated inputs`.
- **Unknown type.** Run `$C $R parity-unknown-type target/debug/pele parity --type nope`. Exit 64, stderr names `vendor/pele/tests/corpus`.

## Gotchas

- Without `--out` the files go to `out/parity/`, which every run overwrites. Proof belongs under `out/verify/<run-id>/`.
- `identical` requires byte-equal SVG and equal width and height. Same-looking SVG with a different attribute order is a difference.
- `pele errors` counts sources Pele itself rejects at 1.13.0. They are neither passes nor failures of this port.
- The corpus files are `-docs.json` only. `agentflow-reference.json` is not part of the count.
- The corpus has 2 pie sources. The claim that pie is ported rests on `parity-harvest`, which has 8,368.
- The flowchart harvest holds small charts: a `maxWidth` changes few of them. The random flowcharts carry the width attempts, the routes with wide turns and the fills under edges. A change to those needs the whole `flowchart_parity` run, not the harvest alone.
- The corpus has 24 usecase sources. The claim that usecase is ported rests on `usecase_parity` and `usecase_front`.
- A `json` node's error carries the message of V8's `JSON.parse`, so a change of the Node pin can change it. `usecase_front` shows that as `rejected differently` on its `syntax, json` inputs; `crates/pele-core/src/diagrams/usecase/json.rs` names the V8 source it follows.
- A message of a usecase error can hold half of a surrogate pair, where V8 quotes one character of an emoji. This port's message is a Rust string and shows U+FFFD there, and the `usecase_front` op of the oracle does the same to Pele's.
- After a change of the Pele pin, rerun `scripts/harvest-pie.mjs` and `scripts/harvest-flowchart.mjs`, and commit the new `harvest.json` files before reading `parity-harvest`.
- The cases with render options go through the oracle's `render_with` op. A new option needs a line in `render_with` of the type's test (`crates/pele-core/tests/pie_parity.rs` or `flowchart_parity.rs`), which fails on an option it does not map.
- The run makes one `node` call for the whole corpus. It takes a few seconds, so a long stall points at the oracle.
