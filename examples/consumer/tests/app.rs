//! The app as its user finds it, drawn headless at a phone's width and at a desktop's, and the
//! manifest it needs, held to the one rune-pele is built with.

use std::collections::BTreeSet;
use std::process::Command;

use rune_pele::capture::Capturer;
use rune_pele::rune::Env;
use rune_pele::{Engine, RenderStatus};
use rune_pele_consumer::{init, view};

const PADDING: f32 = 16.0;

#[test]
fn the_app_draws_its_diagram_natively_within_a_phone_and_a_desktop() {
    let capturer = Capturer::new();
    for width in [390.0, 1200.0] {
        let mut window = capturer.window(init(), view, (width, 700.0), Env::new(false, 2.0));
        let pixels = window.capture();
        let diagram = &window.state().diagram;
        assert_eq!(diagram.status(), RenderStatus::Pele, "{width}: drawn natively: {:?}", diagram.error());
        assert_eq!(diagram.engine(), Some(Engine::Pele), "{width}");

        let elements = window.runtime().elements();
        let drawn = elements.iter().find(|element| element.key.as_deref() == Some("diagram")).expect("the drawing");
        let right = drawn.rect[0] + drawn.rect[2];
        assert!(right <= width - PADDING + 0.5, "{width}: the drawing fits inside the padding: {:?}", drawn.rect);
        let page = pixels.at(0, 0);
        let ink = pixels.rgba.chunks(4).filter(|pixel| *pixel != page).count();
        assert!(ink > 10_000, "{width}: the drawing shows: {ink} pixels differ from the page");

        let link = elements.iter().find(|element| element.role == "link").expect("D is a link");
        let [x, y, w, h] = link.rect;
        window.press(x + w / 2.0, y + h / 2.0);
        let followed = window.state().followed.as_ref().map(|link| link.href.as_str());
        assert_eq!(followed, Some("https://example.org/guide"), "{width}: a press on D tells the app");
    }
}

#[test]
fn on_rune_pele_s_default_features_rune_mermaid_draws_what_the_native_view_does_not() {
    let mut app = init();
    app.source.push_str("  style A text-decoration:line-through\n");
    let capturer = Capturer::new();
    let mut window = capturer.window(app, view, (720.0, 480.0), Env::new(false, 1.0));
    window.capture();
    let diagram = &window.state().diagram;
    assert_eq!(diagram.status(), RenderStatus::RuneMermaid, "{:?}", diagram.error());
    assert_eq!(diagram.engine(), Some(Engine::RuneMermaid));
}

/// The entries of every `[patch.crates-io]` table in a manifest, or in a README's TOML blocks.
fn patches(text: &str) -> Vec<String> {
    let mut within = false;
    let mut entries = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') || line.starts_with("```") {
            within = line == "[patch.crates-io]";
        } else if within && line.contains(" = {") {
            entries.push(line.to_owned());
        }
    }
    entries
}

/// Every Rune rev a text names.
fn revs(text: &str) -> BTreeSet<String> {
    let after = text.split("rev = \"").skip(1);
    after.map(|rest| rest[..rest.find('"').expect("a closed rev")].to_owned()).collect()
}

#[test]
fn the_app_pins_and_patches_rune_as_rune_pele_does_and_the_readme_says() {
    let read = |path: &str| {
        let path = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    let (workspace, app, readme) = (read("Cargo.toml"), read("examples/consumer/Cargo.toml"), read("README.md"));
    let adapter = read("crates/rune-pele/Cargo.toml");

    let patched = patches(&workspace);
    assert_eq!(patched.len(), 4, "Rune's four patches: {patched:?}");
    assert_eq!(patches(&app), patched, "the app patches what the workspace patches");
    assert_eq!(patches(&readme), patched, "the README shows the workspace's patches");

    let pinned = revs(&adapter);
    assert_eq!(pinned.len(), 1, "rune-pele pins one Rune rev: {pinned:?}");
    for (name, text) in [("Cargo.toml", &workspace), ("the app", &app), ("README.md", &readme)] {
        assert_eq!(revs(text), pinned, "{name} names the rev rune-pele pins");
    }
}

#[test]
fn the_app_s_rune_has_the_features_it_turns_on_through_rune_pele_and_no_others() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let args = ["tree", "--offline", "--manifest-path", manifest, "-p", "rune", "--depth", "0", "-e", "normal"];
    let tree = Command::new(env!("CARGO")).args(args).args(["-f", "{f}"]).output().expect("cargo runs");
    assert!(tree.status.success(), "cargo tree: {}", String::from_utf8_lossy(&tree.stderr));
    let features = String::from_utf8(tree.stdout).expect("cargo prints UTF-8");
    let features: Vec<&str> = features.trim().split(',').collect();
    assert_eq!(features, ["embedded-fonts"], "the app's Rune, as Cargo resolves it");
}
