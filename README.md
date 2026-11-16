# rune-pele

Mermaid diagrams for [Rune](https://github.com/anomalyco/rune) apps. This is a native Rust port of [Pele](https://github.com/obsidianmd/pele), the fast Mermaid to SVG renderer, pinned at Pele 1.13.0.

The goal is Pele's output, byte for byte, in a parity mode that uses Pele's own text metrics. On top of that come a Rune adapter with live themes and streaming layouts. All 33 of Pele 1.13.0's diagram types are ported. Their output equals Pele's on the documentation corpus and on every source of theirs in Pele's own tests. `pele parity` shows where the port stands.

## Layout

| Path | What it is |
| --- | --- |
| `crates/pele-core` | Rune-free core: the input boundary, the diagram catalog, text measurement, the Scene, the SVG writer and the ported types. `layout` and `graph` hold what the types that draw nodes and edges share. `js` holds the JavaScript number rules every Pele coordinate goes through. |
| `crates/pele-oracle` | Asks Pele, built from source, what it would output. Tests and the CLI use it. |
| `crates/pele-cli` | The `pele` binary: `pele render` and `pele parity`. |
| `crates/rune-pele` | The Rune adapter, and the only crate that depends on Rune. `Diagram::view` draws a Scene as native Rune elements for pie and the eight graph types, and hands any other type to Rune's own `rune-mermaid`. `examples/diagram.rs` opens any Mermaid file this way, and `examples/pie.rs` shows a palette switch. |
| `harness/` | `oracle.ts`, bundled by esbuild into `harness/dist/oracle.mjs` and run by the pinned Node. |
| `vendor/pele` | Pele's source at `scripts/pele-pin.txt`. Gitignored, and fetched by `mise run setup`. |
| `tests/fixtures` | Sources for the CLI recipes, and one folder per ported type with the sources harvested from Pele's tests by `scripts/harvest-<type>.mjs`. |

## Start

```sh
mise install        # Rust 1.100.0, Node 26.13.1, Kani 0.68.0
mise run setup      # Pele at the pin, then the oracle bundle
mise run check      # fmt, clippy, tests, and the differential tests against Pele
mise exec -- cargo run -q -p pele-cli -- parity
```

`mise run prove` runs the Kani proofs for the number helpers. It takes about 4 minutes.

## Verification

Two project skills live in `.agents/skills/`. `.claude/skills` links to them.

- `verify-rune-pele` drives the `pele` CLI and the Rune view, and proves parity against Pele, with a feature map in `features/`.
- `prove-js-numerics` holds the Kani proofs that `Math.round` and Pele's `num()` match for every double.

## License

MIT. See `LICENSE`. Code ported from Pele carries Pele's MIT notice in `NOTICE`.
