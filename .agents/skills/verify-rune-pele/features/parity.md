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

- **Whole corpus.** Run `$C $R parity-all target/debug/pele parity --out out/verify/$R/parity`. Exit 0 unless a ported source differs. Stdout has one row per type from `agentflow` to `xychart`, 33 rows, then a `total` row with `522` sources at Pele 1.13.0. With pie, flowchart, class and state ported the `pie` row is `2 2 0 0 0`, `flowchart` is `114 114 0 0 0`, `class` is `38 38 0 0 0` and `state` is `22 22 0 0 0`. Every other row has all its sources under `not ported`, and the total is `522 176 0 346 0`. `out/verify/$R/parity/report.json` has the same counts.
- **One type.** Run `$C $R parity-type target/debug/pele parity --type pie --out out/verify/$R/parity-pie`. Stdout has a `pie` row and a `total` row that both read `2 2 0 0 0`. With `--type flowchart --out out/verify/$R/parity-flowchart` both rows read `114 114 0 0 0`. With `--type class --out out/verify/$R/parity-class` both rows read `38 38 0 0 0`.
- **Files.** List `out/verify/$R/parity-pie/pie/`. For each index `i` from 0 to sources-1 there is `<i>.mmd` and `<i>.pele.svg`. `<i>.ours.svg` exists exactly when this port rendered the source. For pie both exist, and `cmp <i>.pele.svg <i>.ours.svg` prints nothing. `<i>.pele.svg` equals `pele render <i>.mmd --engine pele -o …` byte for byte.
- **Gate.** For a type this port renders, `different` must be 0 and the exit 0. A nonzero `different` with exit 1 is a parity regression. Report the first differing index and run `diff out/verify/$R/parity/<type>/<i>.pele.svg out/verify/$R/parity/<type>/<i>.ours.svg`. A source Pele renders and this port rejects also counts as `different`, and stderr names it with the message.
- **Wider test of a ported type.** Run `$C $R parity-harvest cargo test -q -p pele-core --test pie_parity -- --nocapture`. Exit 0 and `4 passed`. Stdout has one line per set that ends in `0 different`: `harvested pie sources: 323 inputs`, `mutated pie sources: 4822 inputs`, `pie cases: 223 inputs` and `random pies: 3000 inputs`. A failure lists each input with its first differing byte, or with both rejection messages.
- **Wider test of flowchart.** Run `$C $R parity-harvest cargo test -q -p pele-core --test flowchart_parity -- --nocapture`. Exit 0 and `5 passed`. Stdout has eight lines that end in `0 different`: `flowchart cases: 80 inputs`, `harvested flowchart sources: 2536 inputs`, the same 2536 once for each `maxWidth` of 320, 480, 640 and 900, `mutated flowchart sources: 8702 inputs` and `random flowcharts: 4000 inputs`. That is 25,462 inputs. It also prints how many inputs each width makes Pele draw another way, which is how many go through the tight and turned attempts: 36, 14, 7 and 3 of the harvest, and 1370 of the random flowcharts. The random test fails when no output reaches one of the draw paths it names, such as a casing or a struck group title.
- **Wider test of class.** Run `$C $R parity-harvest cargo test -q -p pele-core --test class_parity -- --nocapture`. Exit 0 and `5 passed`. Stdout has eight lines that end in `0 different`: `class cases: 30 inputs`, `harvested class sources: 13597 inputs`, the same 13597 once for each `maxWidth` of 320, 480, 640 and 900, `mutated class sources: 7736 inputs` and `random class diagrams: 4000 inputs`. That is 79,751 inputs. It prints how many inputs each width makes Pele draw another way: 256, 9, 5 and 3 of the harvest, and 763 of the random class diagrams. The random test fails when no output reaches one of its 35 named drawing paths, such as a lollipop, a struck namespace title or a static member, and it prints the paths it reached.
- **Class front end.** Run `$C $R parity-harvest cargo test -q -p pele-core --test class_front -- --nocapture`. Exit 0 and `6 passed`. Each of the six lines ends in `0 different`: the harvest (13597), the mutated inputs (7736), 23 hand-written cases, 8 host-config cases, 6000 random members and 3000 random class diagrams. The JSON compared is the oracle's `class_front` op (`harness/class.ts`).
- **State, one type.** Run `$C $R parity-type target/debug/pele parity --type state --out out/verify/$R/parity-state`. The `state` row and the `total` row both read `22 22 0 0 0`.
- **Wider test of state.** Run `$C $R parity-harvest cargo test -q -p pele-core --test state_parity -- --nocapture`. Exit 0 and `6 passed`. Stdout has nine lines that end in `0 different`: `hand-written state diagrams: 61 inputs`, `harvested state sources: 2449 inputs`, the same 2449 once for each `maxWidth` of 320, 480, 640 and 900, `mutated state sources: 8723 inputs`, `random state diagrams: 4000 inputs` and `Pele's worst-case state diagrams: 29 inputs`. That is 25,058 inputs. Only a diagram that runs across reacts to the width, so Pele draws 2, 1, 0 and 0 of the harvest another way at those widths. The random test prints `reach 32 of 32 named drawing paths` and fails when no output reaches one of them, such as regions in outlines, a transition anchored inside its composite or a casing.
- **State front end.** Run `$C $R parity-harvest cargo test -q -p pele-core --test state_front -- --nocapture`. Exit 0 and `4 passed`. Four lines end in `0 different`: `hand-written state diagrams: 37 inputs`, `harvested state sources: 2449 inputs`, `mutated state sources: 8723 inputs` and `random state diagrams: 3000 inputs`. Each compares this port's statement tree, db and graph with the oracle's `state_front` op as JSON, or Pele's error with its code, line, column and snippet.
- **Agentflow, one type.** Run `$C $R parity-type target/debug/pele parity --type agentflow --out out/verify/$R/parity-agentflow`. The `agentflow` row and the `total` row both read `12 12 0 0 0`.
- **Wider test of agentflow.** Run `$C $R parity-harvest cargo test -q -p pele-core --test agentflow_parity -- --nocapture`. Exit 0 and `7 passed`. Stdout has ten lines that end in `0 different`: `hand-written agentflows: 58 inputs`, `agentflow reference sources: 239 inputs`, `harvested agentflow sources: 4761 inputs`, the same 4761 once for each `maxWidth` of 320, 480, 640 and 900, `mutated agentflow sources: 13728 inputs`, `random agentflows: 4000 inputs` and `Pele's worst-case agentflows: 34 inputs`. That is 41,864 inputs. Only an agentflow that runs across reacts to the width, so Pele draws 35, 13, 0 and 0 of the harvest another way at those widths. The random test prints `reach 43 of 43 named drawing paths` and fails when no output reaches one of them, such as a collapsed flow, an edge between a node and the flow around it, or a casing. The run takes about two minutes, most of it in the worst cases.
- **Agentflow front end.** Run `$C $R parity-harvest cargo test -q -p pele-core --test agentflow_front -- --nocapture`. Exit 0 and `6 passed`. Six lines end in `0 different`: `agentflows with parse options: 5 inputs`, `hand-written agentflows: 53 inputs`, `agentflow reference sources: 239 inputs`, `harvested agentflow sources: 4761 inputs`, `mutated agentflow sources: 13728 inputs` and `random agentflows: 4000 inputs`. That is 22,786 inputs. Each compares this port's model, graph and semantic model with the oracle's `agentflow_front` op (`harness/agentflow.ts`) as JSON, statement positions and diagnostics included, or Pele's error with its code, line, column and snippet. The random test prints how many accepted sources hold each of 28 things a model can hold, and fails when one is never held.
- **Harvest is current.** Run `$C $R parity-harvest node scripts/harvest-pie.mjs`, then `git status --short tests/fixtures/pie`. The status is empty at the Pele pin the harvest was made at, which `harvest.json` records as `pele`. Each other ported type has its own pair, `scripts/harvest-<type>.mjs` and `tests/fixtures/<type>`: flowchart, class and state.
- **Unknown type.** Run `$C $R parity-unknown-type target/debug/pele parity --type nope`. Exit 64, stderr names `vendor/pele/tests/corpus`.

## Gotchas

- Without `--out` the files go to `out/parity/`, which every run overwrites. Proof belongs under `out/verify/<run-id>/`.
- `identical` requires byte-equal SVG and equal width and height. Same-looking SVG with a different attribute order is a difference.
- `pele errors` counts sources Pele itself rejects at 1.13.0. They are neither passes nor failures of this port.
- The corpus files are `-docs.json` only. `agentflow-reference.json` is not part of the count.
- The corpus has 2 pie sources. The claim that pie is ported rests on `parity-harvest`, which has 8,368.
- The flowchart harvest holds small charts: a `maxWidth` changes few of them. The random flowcharts carry the width attempts, the routes with wide turns and the fills under edges. A change to those needs the whole `flowchart_parity` run, not the harvest alone.
- After a change of the Pele pin, rerun `scripts/harvest-pie.mjs` and `scripts/harvest-flowchart.mjs`, and commit the new `harvest.json` files before reading `parity-harvest`.
- The cases with render options go through the oracle's `render_with` op. A new option needs a line in `render_with` of the type's test (`crates/pele-core/tests/pie_parity.rs` or `flowchart_parity.rs`), which fails on an option it does not map.
- A class style list cannot start with a hyphenated property such as `font-size`: Pele's grammar reads the hyphen as `MINUS` and rejects the line. The random class diagrams keep to lists Pele parses, so a style path reaches the renderer.
- `class_parity.rs` maps the `monospace` render option, which sets class members in the monospace font.
- Agentflow has no `---` link: two or more dashes with no arrowhead open an edge label, which swallows the lines after it. A shape named in metadata has to be lowercase, so no source can name `collapsedGroup`. The random agentflows of `agentflow_parity.rs` keep to what Pele renders.
- Agentflow is the one type parsed with its `%%` comment lines kept (`DiagramKind::keeps_comments`), because its model holds the line and column of each statement. A change to preprocessing or to `Origin::locate` needs `agentflow_front` as well as the other front-end tests.
- `tests/fixtures/agentflow/harvest.json` also holds `reference`, the sources of `agentflow-reference.json`. They are extra inputs and no part of the 522.
- Each type's oracle ops live in `harness/<type>.ts`. The bundle's stamp hashes every `harness/*.ts` in name order, so the doctor fails after a change to any of them until `mise run setup` rebuilds it.
- The run makes one `node` call for the whole corpus. It takes a few seconds, so a long stall points at the oracle.
