# Using rune-pele from another project

An app outside this workspace depends on `rune-pele` by path, and on Rune at the revision rune-pele pins with Rune's four patches, as the README's "Using rune-pele in your app" says. A dependency's `[patch]` does not reach the workspace that depends on it, so without the patches the app does not resolve. `examples/consumer` is such an app, in a workspace of its own: it draws a flowchart with `Diagram::view_with` and `ViewOptions::fit()`.

## Sub-features

- `consumer-build` resolves and builds the app on its own, with its own lock and patches, into the workspace's target directory, and lints it.
- `consumer-view` draws the app's view headless 390 and 1200 points wide: natively, inside its padding, with a link that tells the app.
- `consumer-manifest` holds the app's patches and the README's to the workspace's, and every Rune revision they name to rune-pele's.
- `consumer-unpatched` shows the same app without the patches does not resolve.

## How to get to it (user POV)

- A developer copies the README's manifest into their app, makes a `Diagram` with `Diagram::new(id, FontCatalog::rune())`, and calls `diagram.view_with(cx, &source, ViewOptions::fit(), on_event)` in the view.
- `mise exec -- cargo run --manifest-path examples/consumer/Cargo.toml --target-dir target` opens the app's window.
- `mise run consumer` builds, lints and tests the app. `mise run check` runs it last.

## Driving it with the pele CLI

Preconditions:

- The baseline in `README.md` holds, and `doctor.sh` printed `doctor: ok`.
- `R` is this run's id and `C` is `.agents/skills/verify-rune-pele/scripts/capture.sh`.
- A GPU adapter is available. The app's test draws headless without a window.

- **Build and test the app.** Run `$C $R consumer-build mise run consumer`. Exit 0. Stdout has `running 2 tests` and `test result: ok. 2 passed`: `the_app_draws_its_diagram_natively_within_a_phone_and_a_desktop` and `the_app_pins_and_patches_rune_as_rune_pele_does_and_the_readme_says`.
- **Without the patches.** Copy the app aside without its patch table, and resolve it:

  ```sh
  mkdir -p out/verify/$R/unpatched && cp -R examples/consumer/src out/verify/$R/unpatched/
  sed -e '/^\[patch.crates-io\]/,$d' -e "s|\.\./\.\./crates/rune-pele|$PWD/crates/rune-pele|" \
    examples/consumer/Cargo.toml > out/verify/$R/unpatched/Cargo.toml
  $C $R consumer-unpatched cargo check --offline --manifest-path out/verify/$R/unpatched/Cargo.toml --target-dir target
  ```

  Exit 101. Stderr starts ``error: failed to select a version for `parley_engine`.`` and has the line `` package `rune` depends on `parley_engine` with feature `line-dictionaries` but `parley_engine` does not have that feature. ``
- **Without winit's patch.** Write the manifest again with `-e '/^winit = /d'` in place of the first `-e`, so that only winit's patch is gone, and run `$C $R consumer-unpatched-winit cargo check --offline --manifest-path out/verify/$R/unpatched/Cargo.toml --target-dir out/verify/$R/unpatched/target`. On macOS, exit 101 after Rune's dependencies are checked, about a minute cold, and stderr has ``cannot find function `take_ime_clauses` in module `winit::platform::macos` ``.

## Gotchas

- The app builds into the workspace's `target/`, with Rune's features as the app sets them. The first run builds `rune`, `rune-mermaid`, `pele-core` and `rune-pele` once more for those features, about a minute; a warm run builds nothing new.
- Cargo reads `.cargo/config.toml` from the directory it runs in, not from the manifest's, so the task passes `--target-dir target`. A `cargo` run inside `examples/consumer` without it builds into `examples/consumer/target`, which that directory's `.gitignore` leaves out.
- A Rune bump changes the revision in four places: `crates/rune-pele/Cargo.toml`, the workspace's `[patch.crates-io]`, the app's manifest and the README. `consumer-manifest` fails until they agree, and the app's `Cargo.lock` then updates on its next build.
- wgpu-hal's and accesskit_windows' patches change Windows only, which this recipe does not reach. Rune falls back without wgpu-hal's (`crates/rune/src/gpu/pick.rs`).
