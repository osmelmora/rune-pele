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

rune-pele is not on crates.io. An app depends on it from git at a release tag, the newest on the Releases page, with Rune's patches. On `v0.1.0` it also depends on Rune, at the revision rune-pele pins:

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

- **Rune's revision** is the one `crates/rune-pele/Cargo.toml` pins. `Diagram::view` takes that Rune's `Cx` and returns its `El`. An app's `rune` at another revision is a second `rune`, whose types rune-pele's do not take and whose features miss the Rune rune-pele draws with, so bump the two together.
- **Rune from rune-pele.** From the first release after `v0.1.0`, rune-pele re-exports its Rune as `rune_pele::rune`, and its `embedded-fonts` feature turns on Rune's. An app that names Rune only as `rune_pele::rune` always has rune-pele's Rune, and needs no `rune` line:

  ```toml
  rune-pele = { git = "https://github.com/osmelmora/rune-pele", tag = "<a tag after v0.1.0>", features = ["embedded-fonts"] }
  ```

  `examples/consumer` does this. Cargo sets a dependency's features only through the features that dependency declares, and rune-pele declares one for Rune, `embedded-fonts`, the font `FontCatalog::rune()` measures with. An app that wants others of Rune's features, such as its Metal backend, accessibility or SVG images, keeps its own `rune` line as above, at the same revision: two lines that name one revision are one `rune`, built with the features of both.
- **The four patches** are the `[patch.crates-io]` table of this repository's `Cargo.toml`, which is Rune's own (Rune's `crates/rune/README.md`, "Depending on Rune"). A dependency's `[patch]` does not reach the workspace that depends on it, so the app repeats them, from the same revision. Without `parley_engine`'s, Cargo cannot resolve `rune`: `package rune depends on parley_engine with feature line-dictionaries but parley_engine does not have that feature`. Without winit's, `rune` does not compile on macOS, which calls the input-method functions only Rune's winit has. wgpu-hal's lets Rune pick a Windows GPU without starting every adapter, and accesskit_windows' gives Windows screen readers headings and landmarks.
- **Fonts.** rune-pele turns Rune's default features off, so the app chooses them, with its own `rune` line or with rune-pele's `embedded-fonts`. `FontCatalog::rune()` lays labels out in Inter, which Rune embeds with its `embedded-fonts` feature. Without it, and without fonts of the app's own, text falls back to the system's fonts: on macOS, labels are laid out and drawn in a system sans instead of Inter. An app that gives Rune its own fonts in `AppSpec::fonts` and `AppSpec::families` gives `FontCatalog::new` the same bytes and families, and can leave `embedded-fonts` off.
- **The fallback.** From the first release after `v0.1.0`, rune-pele's `fallback` feature, on by default, has Rune's rune-mermaid draw a source the native view does not. An app that turns rune-pele's default features off, `default-features = false, features = ["embedded-fonts"]`, builds without rune-mermaid. Such a source is then an error, `ViewError::Geometry`, or `ViewError::NoFallback` for a type pele-core has not ported, as when rune-mermaid cannot draw it, and after a source that drew the view keeps that drawing. Every public item is in both builds, so the app's code compiles either way: `Engine::RuneMermaid` and `RenderStatus::RuneMermaid` are then never reported, and `Diagram::fallback()` is always `None`. `examples/consumer` built for release without it has one crate fewer (187 of 188) and a binary 1,133,040 bytes (3.8%) smaller. Its clean build was no faster in two runs each: 79 s and 84 s with it, 75 s and 95 s without.
- **The API.** From the first release after `v0.1.0`, an app relies on what `cargo doc` shows of the two crates. An item marked `#[doc(hidden)]` is public only for rune-pele or the workspace's tests, its doc says so, and any release may change it. Options are built with methods, as `ViewOptions::fit().zoom(0.75)` and `LayoutOptions::default().max_width(640.0)`. The types that will grow are `#[non_exhaustive]`, so a match on `ViewError`, `RenderStatus` or `Engine` has a wildcard arm. `rune_pele::pele_core` names the types rune-pele's own use, such as the `RenderError` in `ViewError::Core`, with no `pele-core` line of the app's own.
- **Rust.** Both crates build on Rust 1.95 and later, the `rust-version` each names. `mise run msrv` checks it, and `mise run check` runs that.

Make one `Diagram` per diagram and keep it in the app's state. In the view, `diagram.view_with(cx, &source, ViewOptions::fit(), on_event)` draws the source as wide as its parent, and `on_event` hears a link pressed. A source the native lowering cannot draw, which `DIVERGENCES.md` lists, is drawn by Rune's rune-mermaid with the `fallback` feature, which draws no links or tooltips and ignores the zoom. `diagram.engine()` says which drew the drawing shown, and what it can do.

A source can name icons, such as `logos:aws` for an architecture service or a treeview row. Pele draws them only for a page that gives it icons, so its SVG leaves such a box empty, and the view does too until the app gives it icons of its own: `ViewOptions::fit().icons(icons)`, where `icons` is a plain `fn(&str) -> Option<Icon>`. An `Icon` is SVG path data on a view box, filled or stroked, as an Iconify icon is: `Icon::new([0.0, 0.0, 24.0, 24.0])?.stroke("M4 12h16", 2.0, IconPaint::Current)?`. `IconPaint::Current` is the diagram's foreground, as `currentColor` is in Pele's SVG, and `IconPaint::Fixed` an icon's own colour. A name the function has no icon for is drawn as without icons. `examples/diagram.rs` gives three.

`examples/consumer` is such an app, in a workspace of its own, that takes Rune as `rune_pele::rune`. `mise run consumer` builds it and runs its tests. They check that its patches and this section's match the workspace's, that its Rune has the features it asks for and no others, and that rune-mermaid draws on rune-pele's default features. `mise run check` runs it too. On Linux, Rune also needs the system libraries its README lists under "Linux".

`mise run consumer-tag [tag [url]]` writes an app with the manifest above under `out/consumer-tag/`, on the newest `v*` tag of this repository unless given one, fetched from this repository as a `file://` URL unless given another. It builds the app and runs a test that draws one diagram, with only the pinned Rust's `cargo` and `rustc` on PATH: an app needs no `vendor/`, no Node and no oracle. Release tags are made on GitHub, so a checkout has them after `git fetch --tags`.

## Releases

`pele-core` and `rune-pele` share one version and one tag, `v<version>`. Releases are made by [release-plz](https://release-plz.ieni.dev) from git tags, not crates.io:

- Each push to `main` updates a release pull request, which bumps the version and writes `CHANGELOG.md`. Merging it tags the release and makes its GitHub release.
- The bump comes from the commit messages, which follow [Conventional Commits](https://www.conventionalcommits.org): `fix:` and `feat:` are compatible changes, and `feat!:` or `fix!:` a breaking one. cargo-semver-checks also compares the public API with the last release and makes any breaking change a breaking bump. Before 1.0 a compatible change moves the patch version and a breaking one the minor version: 0.1.0 becomes 0.1.1 or 0.2.0.
- A commit that moves the Rune pin is `build(rune): ...`, because apps must move their Rune with it.

`mise run setup` installs the git hooks from `hk.pkl`. They check each commit message, and before each commit they run `cargo fmt` and keep each crate's `LICENSE` and `NOTICE` equal to the root's.

## Verification

Two project skills live in `.agents/skills/`. `.claude/skills` links to them.

- `verify-rune-pele` drives the `pele` CLI and the Rune view, and proves parity against Pele, with a feature map in `features/`.
- `prove-js-numerics` holds the Kani proofs that `Math.round` and Pele's `num()` match for every double.

## License

MIT. See `LICENSE`. Code ported from Pele carries Pele's MIT notice in `NOTICE`.
