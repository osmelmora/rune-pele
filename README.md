# rune-pele

Mermaid diagrams for [Rune](https://github.com/anomalyco/rune) apps. This is a native Rust port of [Pele](https://github.com/obsidianmd/pele), the fast Mermaid to SVG renderer, pinned at Pele 1.13.0.

The goal is Pele's output, byte for byte, in a parity mode that uses Pele's own text metrics. On top of that come a Rune adapter with live themes and streaming layouts. All 33 of Pele 1.13.0's diagram types are ported. Their output equals Pele's on the documentation corpus and on every source of theirs in Pele's own tests. `pele parity` shows where the port stands.

## Layout

| Path | What it is |
| --- | --- |
| `crates/pele-core` | Rune-free core: the input boundary, the diagram catalog, text measurement, the Scene, the SVG writer and the ported types. `layout` and `graph` hold what the types that draw nodes and edges share. `js` holds the JavaScript number rules every Pele coordinate goes through. |
| `crates/pele-oracle` | Asks Pele, built from source, what it would output. Tests and the CLI use it. |
| `crates/pele-cli` | The `pele` binary: `pele render` and `pele parity`. |
| `crates/rune-pele` | The Rune adapter, and the only crate that depends on Rune. `Diagram::view` draws a Scene as native Rune elements for all 33 types, which `lower::NATIVE` lists, and hands a source whose Scene it cannot draw to Rune's own `rune-mermaid`. `ViewOptions::fit` fits a diagram to the width its view has, and `ViewOptions::zoom` draws it larger or smaller. While a source fails after one that drew, as one still streaming in does, it keeps showing that drawing, fitted as it would be drawn now, dimmed with the error over it as Pele's `mount` does, or unchanged with `KeptLook::Unchanged`. `examples/diagram.rs` opens any Mermaid file this way, fitted to its window, and with `--stream` types it in a few characters a frame. `examples/pie.rs` shows a palette switch. |
| `harness/` | `oracle.ts`, bundled by esbuild into `harness/dist/oracle.mjs` and run by the pinned Node. |
| `vendor/pele` | Pele's source at `scripts/pele-pin.txt`. Gitignored, and fetched by `mise run setup`. |
| `tests/fixtures` | Sources for the CLI recipes, and one folder per ported type with the sources harvested from Pele's tests by `scripts/harvest-<type>.mjs`. |
| `examples/consumer` | An app in a workspace of its own that depends on rune-pele, as "Using rune-pele in your app" says. |

## Start

```sh
mise install        # Rust 1.100.0, Node 26.13.1, Kani 0.68.0
mise run setup      # Pele at the pin, then the oracle bundle
mise run check      # fmt, clippy, tests, and the differential tests against Pele
mise exec -- cargo run -q -p pele-cli -- parity
```

`mise run prove` runs the Kani proofs for the number helpers. It takes about 4 minutes.

## Using rune-pele in your app

rune-pele is not on crates.io. An app depends on it by path, here from a checkout beside the app, and on Rune at the revision rune-pele pins, with Rune's patches:

```toml
[dependencies]
rune-pele = { path = "../rune-pele/crates/rune-pele" }
rune = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21", default-features = false, features = ["embedded-fonts"] }

[patch.crates-io]
winit = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
parley_engine = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
wgpu-hal = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
accesskit_windows = { git = "https://github.com/anomalyco/rune", rev = "165f87a4700a5515b03d4de04876aa2e64a30c21" }
```

- **Rune's revision** is the one `crates/rune-pele/Cargo.toml` pins. `Diagram::view` takes that Rune's `Cx` and returns its `El`. An app on another revision builds a second `rune`, whose types rune-pele's do not take, so bump the two together.
- **The four patches** are the `[patch.crates-io]` table of this repository's `Cargo.toml`, which is Rune's own (Rune's `crates/rune/README.md`, "Depending on Rune"). A dependency's `[patch]` does not reach the workspace that depends on it, so the app repeats them, from the same revision. Without `parley_engine`'s, Cargo cannot resolve `rune`: `package rune depends on parley_engine with feature line-dictionaries but parley_engine does not have that feature`. Without winit's, `rune` does not compile on macOS, which calls the input-method functions only Rune's winit has. wgpu-hal's lets Rune pick a Windows GPU without starting every adapter, and accesskit_windows' gives Windows screen readers headings and landmarks.
- **Fonts.** rune-pele turns Rune's default features off, so the app's `rune` dependency chooses them, and Rune's defaults do too. Text needs a font: `FontCatalog::rune()` lays labels out in Inter, which Rune embeds with its `embedded-fonts` feature. Without it, and without fonts of the app's own, text draws nothing. An app that gives Rune its own fonts in `AppSpec::fonts` and `AppSpec::families` gives `FontCatalog::new` the same bytes and families, and can leave `embedded-fonts` off.

Make one `Diagram` per diagram and keep it in the app's state. In the view, `diagram.view_with(cx, &source, ViewOptions::fit(), on_event)` draws the source as wide as its parent, and `on_event` hears a link pressed. A source the native lowering cannot draw, which `DIVERGENCES.md` lists, is drawn by Rune's rune-mermaid, which draws no links or tooltips and ignores the zoom. `diagram.engine()` says which drew the drawing shown, and what it can do.

`examples/consumer` is such an app, in a workspace of its own. `mise run consumer` builds it, runs its tests, and checks that its patches and this section's match the workspace's, and `mise run check` runs it too. On Linux, Rune also needs the system libraries its README lists under "Linux".

## Verification

Two project skills live in `.agents/skills/`. `.claude/skills` links to them.

- `verify-rune-pele` drives the `pele` CLI and the Rune view, and proves parity against Pele, with a feature map in `features/`.
- `prove-js-numerics` holds the Kani proofs that `Math.round` and Pele's `num()` match for every double.

## License

MIT. See `LICENSE`. Code ported from Pele carries Pele's MIT notice in `NOTICE`.
