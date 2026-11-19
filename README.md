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
mise install        # Rust, Node, Kani, hk, release-plz and cargo-semver-checks, pinned in mise.toml
mise run setup      # Pele at the pin, the oracle bundle, and the git hooks
mise run check      # fmt, clippy, tests, and the differential tests against Pele
mise exec -- cargo run -q -p pele-cli -- parity
```

`mise run prove` runs the Kani proofs for the number helpers. It takes about 4 minutes.

## Using rune-pele in your app

rune-pele is not on crates.io. An app depends on it from git at a release tag, the newest on the Releases page, and on Rune at the revision rune-pele pins, with Rune's patches:

```toml
[dependencies]
rune-pele = { git = "https://github.com/osmelmora/rune-pele", tag = "v0.1.0" }
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

A source can name icons, such as `logos:aws` for an architecture service or a treeview row. Pele draws them only for a page that gives it icons, so its SVG leaves such a box empty, and the view does too until the app gives it icons of its own: `ViewOptions::fit().icons(icons)`, where `icons` is a plain `fn(&str) -> Option<Icon>`. An `Icon` is SVG path data on a view box, filled or stroked, as an Iconify icon is: `Icon::new([0.0, 0.0, 24.0, 24.0])?.stroke("M4 12h16", 2.0, IconPaint::Current)?`. `IconPaint::Current` is the diagram's foreground, as `currentColor` is in Pele's SVG, and `IconPaint::Fixed` an icon's own colour. A name the function has no icon for is drawn as without icons. `examples/diagram.rs` gives three.

`examples/consumer` is such an app, in a workspace of its own. `mise run consumer` builds it, runs its tests, and checks that its patches and this section's match the workspace's, and `mise run check` runs it too. On Linux, Rune also needs the system libraries its README lists under "Linux".

## Releases

`pele-core` and `rune-pele` share one version and one tag, `v<version>`. Releases are made by [release-plz](https://release-plz.ieni.dev) from git tags, not crates.io:

- Each push to `main` updates a release pull request, which bumps the version and writes `CHANGELOG.md`. Merging it tags the release and makes its GitHub release.
- The bump comes from the commit messages, which follow [Conventional Commits](https://www.conventionalcommits.org): `fix:` is a patch, `feat:` a minor version, and `feat!:` or `fix!:` a breaking one. cargo-semver-checks also compares the public API with the last release and makes any breaking change a breaking bump. Before 1.0, a breaking bump moves the minor version.
- A commit that moves the Rune pin is `build(rune): ...`, because apps must move their Rune with it.

`mise run setup` installs the git hooks from `hk.pkl`. They check each commit message, and before each commit they run `cargo fmt` and keep each crate's `LICENSE` and `NOTICE` equal to the root's.

## Verification

Two project skills live in `.agents/skills/`. `.claude/skills` links to them.

- `verify-rune-pele` drives the `pele` CLI and the Rune view, and proves parity against Pele, with a feature map in `features/`.
- `prove-js-numerics` holds the Kani proofs that `Math.round` and Pele's `num()` match for every double.

## License

MIT. See `LICENSE`. Code ported from Pele carries Pele's MIT notice in `NOTICE`.
