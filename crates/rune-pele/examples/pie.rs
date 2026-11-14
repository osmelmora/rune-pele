//! A pie drawn natively by rune-pele. It follows the system's dark mode, and its button switches
//! between two palettes without laying the pie out again.
//!
//! `cargo run -p rune-pele --example pie` opens the window. `-- --snap pie.png` renders it
//! headless; add `--theme light` or `--theme dark`, and `--scene hues` for the second palette.

use rune::tok::{self, type_};
use rune::{AppSpec, Cx, El, col, el, oklch, text};
use rune_pele::{Diagram, FontCatalog, Palette, ViewOptions};

const SOURCE: &str = include_str!("../../../tests/fixtures/pie-pets.mmd");

struct State {
    pie: Diagram,
    hues: bool,
}

fn init() -> State {
    State { pie: Diagram::new("pets", FontCatalog::rune()), hues: false }
}

/// Pele's series in Rune's theme, or eight hues around the colour wheel at one lightness.
fn palette(hues: bool, dark: bool) -> Palette {
    let themed = Palette::from_tokens(&tok::theme(dark));
    if !hues {
        return themed;
    }
    let lightness = if dark { 0.7 } else { 0.62 };
    (0..8).fold(themed, |palette, i| palette.with_series(i, oklch(lightness, 0.14, 25.0 + 45.0 * i as f32)))
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = cx.system_dark();
    let t = tok::theme(dark);
    let name = if s.hues { "Palette: hues" } else { "Palette: Pele" };
    let button = el()
        .key("palette")
        .role("button")
        .label("Switch palette")
        .focusable(true)
        .padding(10.0)
        .radius(8.0)
        .fill(t.card)
        .on_press(|s: &mut State| s.hues = !s.hues)
        .child(text(name).font(type_::BODY).color(t.text));
    let options = ViewOptions::new(440.0).palette(palette(s.hues, dark));
    let pie = s.pie.view(cx, SOURCE, options).unwrap_or_else(|error| text(error.to_string()).color(t.text));
    col().w_full().h_full().fill(t.bg).padding(20.0).gap(16.0).child(button).child(pie)
}

fn main() {
    rune::run(AppSpec::<State> {
        scenes: &[("hues", |s| s.hues = true)],
        ..AppSpec::new("Pie", (480.0, 480.0), init, view)
    });
}
