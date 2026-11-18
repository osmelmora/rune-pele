//! The app's icons: an icon `ViewOptions::icons` gives for a name is drawn in the box the diagram
//! leaves for it, as Pele's SVG draws its host's icon there, at every zoom and in a fit. A name it
//! gives none for is drawn as without it, and another function paints again and lays nothing out.
//!
//! Pele's SVG with the same icons is Pele's own SVG, from the oracle, with each icon's markup in
//! its box, where Pele's `iconSlot` and `iconSvg` put what the host's `icons` resolver answers.
//! The oracle takes render options as JSON, which cannot carry the resolver, a function.

mod common;

use common::{around_pixel, as_a_page_reads, compare, in_a_row, marked, rasterise};
use pele_core::scene::{Geometry, PaintVisit};
use pele_oracle::PeleRender;
use rune::color::hex;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::{Capturer, Pixels, Window};
use rune_pele::{Diagram, Event, FontCatalog, Icon, IconError, IconPaint, Palette, ViewOptions};

const RUN: &str = "rune-graph";
const SCALE: f32 = 2.0;
const ORANGE: &str = "#f58518";
const BLUE: &str = "#4c78a8";
/// Pele's foreground, which `currentColor` is in its SVG.
const INK: &str = "#1f1f1f";

/// A bolt, stroked 2 wide on a grid 24 across, as Pele's own icon box strokes what it is given.
const BOLT: &str = "M13 2 4 14h7l-1 8 9-12h-7z";
/// A ring turned off the axes, on a view box twice as wide as high, under a bar.
const BADGE_RING: &str = "M4 8A12 5 20 0 0 28 8A12 5 20 0 0 4 8Z";
const BADGE_BAR: &str = "M8 8h16";

fn colour(spelling: &str) -> Color {
    hex(spelling).expect("a hex colour")
}

fn levels(spelling: &str) -> [u8; 3] {
    let byte = |i: usize| u8::from_str_radix(&spelling[i..i + 2], 16).expect("hex");
    [byte(1), byte(3), byte(5)]
}

fn near(got: [u8; 4], want: [u8; 3], within: u8) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= within)
}

fn bolt() -> Icon {
    Icon::new([0.0, 0.0, 24.0, 24.0]).and_then(|icon| icon.stroke(BOLT, 2.0, IconPaint::Current)).expect("an icon")
}

fn badge() -> Icon {
    Icon::new([0.0, 0.0, 32.0, 16.0])
        .and_then(|icon| icon.fill(BADGE_RING, IconPaint::Fixed(colour(ORANGE))))
        .and_then(|icon| icon.stroke(BADGE_BAR, 2.0, IconPaint::Current))
        .expect("an icon")
}

/// The app's icons in these tests. `database` is one of architecture's built-in glyphs.
fn app_icons(name: &str) -> Option<Icon> {
    match name {
        "app:bolt" => Some(bolt()),
        "app:badge" | "database" => Some(badge()),
        _ => None,
    }
}

/// What Pele's host answers its `icons` resolver for each name [`app_icons`] knows: the same
/// icons, as markup drawn in Pele's icon box, which strokes in `currentColor` 2 wide with round
/// caps and joins unless told otherwise.
fn host_markup(name: &str) -> Option<String> {
    match name {
        "app:bolt" => Some(format!("<path d=\"{BOLT}\"/>")),
        "app:badge" | "database" => Some(format!(
            "<svg viewBox=\"0 0 32 16\"><path d=\"{BADGE_RING}\" fill=\"{ORANGE}\" stroke=\"none\"/>\
             <path d=\"{BADGE_BAR}\" stroke-width=\"2\"/></svg>"
        )),
        _ => None,
    }
}

/// Another app's icons, which draw the bolt as a blue square.
fn other_icons(name: &str) -> Option<Icon> {
    let square =
        Icon::new([0.0, 0.0, 24.0, 24.0]).and_then(|icon| icon.fill("M4 4h16v16H4z", IconPaint::Fixed(colour(BLUE))));
    (name == "app:bolt").then(|| square.expect("an icon"))
}

/// Pele's `svg` as Pele draws it when its host answers `markup`: each icon box whose name it
/// answers for holds the answer in place of what it held, as `icons?.(name) ?? builtinIcon(name)`
/// puts it there.
fn with_host_icons(svg: &str, markup: fn(&str) -> Option<String>) -> String {
    const BOX: &str = "<svg class=\"pele-icon\" data-icon=\"";
    let (mut out, mut rest) = (String::new(), svg);
    while let Some(at) = rest.find(BOX) {
        let name_at = at + BOX.len();
        let name_end = name_at + rest[name_at..].find('"').expect("a closed name");
        let inside = name_end + rest[name_end..].find('>').expect("a closed tag") + 1;
        let close = inside + rest[inside..].find("</svg>").expect("a closed box");
        out.push_str(&rest[..inside]);
        out.push_str(&markup(&rest[name_at..name_end]).unwrap_or_else(|| rest[inside..close].to_owned()));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// Pele's own SVG of `source`, and its size.
fn pele(source: &str) -> (String, (f32, f32)) {
    let drawn = pele_oracle::render_all(&[source.to_owned()]);
    let PeleRender::Svg { svg, width, height, .. } = &drawn[0] else { panic!("Pele draws it: {drawn:?}") };
    (svg.clone(), (width.ceil() as f32, height.ceil() as f32))
}

/// Pele's `svg` with the host's icons as a page reads it, for resvg. The root's style sets the
/// colour an icon's `currentColor` is, after custom properties, and resvg reads no declaration
/// that follows one, as a page does, so the colour is also the root's attribute.
fn page(svg: &str) -> String {
    let page = as_a_page_reads(&with_host_icons(svg, host_markup));
    let style = &page[page.find(" style=\"").expect("the root's style") + 8..];
    let current = style.split(['"', ';']).find_map(|declaration| declaration.strip_prefix("color:"));
    let current = current.expect("the root sets the colour icons are drawn in").to_owned();
    page.replacen("<svg ", &format!("<svg color=\"{current}\" "), 1)
}

/// An architecture diagram with an icon of the app's in a group's box and a service's, one in
/// place of a built-in glyph, a built-in glyph the app has no icon for, and a name nothing knows.
const ARCHITECTURE: &str = "architecture-beta\n    group cloud(app:badge)[Cloud]\n    \
                            service api(app:bolt)[API] in cloud\n    service db(database)[Store] in cloud\n    \
                            service web(server)[Web] in cloud\n    service queue(app:unknown)[Queue] in cloud\n    \
                            api:R -- L:db";

/// A flowchart's icon shapes, framed and not, one a link and one with a tooltip. The tooltip's
/// label is two runs, so the pointer finds the tooltip in a box round each.
const FLOWCHART: &str = "flowchart LR\n    S@{ icon: \"app:bolt\", form: \"square\", label: \"Square\" }\n    \
                         A@{ icon: \"app:badge\", label: \"Linked\" }\n    B@{ icon: \"app:badge\", label: \"Tipped<br>badge\" }\n    \
                         S --> A --> B\n    click A \"https://example.com/badge\"\n    click B callback \"Badge tip\"";

/// A treeview row with an icon of the app's in place of the file glyph Pele draws for a name it
/// has no icon for, and one with Pele's file glyph.
const TREEVIEW: &str = "---\nconfig:\n  treeView:\n    showIcons: true\n---\ntreeView-beta\n    src/\n        \
                        index.ts icon(app:bolt)\n        legacy.js";

struct Viewer {
    diagram: Diagram,
    source: String,
    options: ViewOptions,
    size: (f32, f32),
    followed: Vec<String>,
}

fn on_event(viewer: &mut Viewer, event: Event, _: &mut rune::Tx) {
    if let Event::Activate(link) = event {
        viewer.followed.push(link.href);
    }
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let drawn = s.diagram.view_with(cx, &s.source, s.options.clone(), on_event).expect("it draws");
    el().size(s.size.0, s.size.1).fill(Color::WHITE).child(drawn)
}

fn as_pele() -> ViewOptions {
    ViewOptions::natural().pele_metrics().palette(Palette::pele())
}

fn window<'a>(capturer: &'a Capturer, source: &str, options: ViewOptions, size: (f32, f32)) -> Window<'a, Viewer> {
    let viewer = Viewer {
        diagram: Diagram::new("icons", FontCatalog::rune()),
        source: source.to_owned(),
        options,
        size,
        followed: Vec::new(),
    };
    capturer.window(viewer, view, size, Env::new(false, SCALE))
}

/// An icon box of a Scene: its name, its corner in the drawing and its side, in points.
struct Slot {
    name: String,
    at: [f64; 2],
    side: f64,
}

impl Slot {
    /// A point of the box, as a share of its side, in device pixels at `zoom`.
    fn pixel(&self, [x, y]: [f64; 2], zoom: f64) -> (u32, u32) {
        let density = zoom * f64::from(SCALE);
        (((self.at[0] + x * self.side) * density) as u32, ((self.at[1] + y * self.side) * density) as u32)
    }

    /// The box and a point round it, in device pixels at `zoom`.
    fn around(&self, image: &Pixels, zoom: f64) -> Pixels {
        let reach = ((self.side / 2.0 + 1.0) * zoom * f64::from(SCALE)) as u32;
        around_pixel(image, self.pixel([0.5, 0.5], zoom), reach)
    }
}

fn slots(diagram: &Diagram) -> Vec<Slot> {
    let scene = diagram.scene().expect("drawn natively");
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(draw) => match draw.geometry {
                Geometry::Icon { name, origin, size, .. } => {
                    Some(Slot { name, at: [draw.origin.x + origin.x, draw.origin.y + origin.y], side: size.width })
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Holds each box an app's icon is drawn in to Pele's SVG with the host's icons, drawn by resvg
/// at the same density, by the limits the corpus is held to. Returns the names compared.
fn as_pele_draws_each_icon(name: &str, ours: &Pixels, theirs: &Pixels, slots: &[Slot], zoom: f64) -> Vec<String> {
    let density = zoom * f64::from(SCALE);
    let mut compared = Vec::new();
    for slot in slots.iter().filter(|slot| app_icons(&slot.name).is_some()) {
        let (mine, pele) = (slot.around(ours, zoom), slot.around(theirs, zoom));
        let (apart, differ) = compare(&mine, &pele, &[], density);
        let file = format!("icons/{name}-{}@{density}x.png", slot.name.replace(':', "-"));
        common::save_in(RUN, &in_a_row(&[&mine, &pele, &marked(&mine, &differ)], 8), &file);
        println!(
            "{name} {}@{density}x: patch {} px, {:.2} unexplained in 10,000, flat {} levels",
            slot.name,
            apart.largest_blob,
            apart.share(),
            apart.flat_error
        );
        // Under 2 pixels a point the corpus's 1x rules hold: see `common::over`.
        let (sharp, other) = common::over(&apart, density);
        assert!(other.is_empty(), "{name} {}@{density}x: {other:?}", slot.name);
        assert!(density < 2.0 || sharp.is_empty(), "{name} {}@{density}x: {sharp:?}", slot.name);
        compared.push(slot.name.clone());
    }
    compared
}

/// The pixels of `image` within `within` levels of `colour`.
fn inked(image: &Pixels, colour: &str, within: u8) -> usize {
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| near(image.at(x, y), levels(colour), within))
        .count()
}

#[test]
fn an_icon_the_app_gives_is_drawn_in_its_box_as_pele_draws_it_with_that_icon() {
    let capturer = Capturer::new();
    let sources = [
        ("architecture", ARCHITECTURE, ["app:badge", "app:bolt", "database"].as_slice()),
        ("flowchart", FLOWCHART, &["app:bolt", "app:badge", "app:badge"]),
        ("treeview", TREEVIEW, &["app:bolt"]),
    ];
    for (name, source, drawn) in sources {
        let (svg, size) = pele(source);
        let mut window = window(&capturer, source, as_pele().icons(app_icons), size);
        let ours = window.capture();
        let theirs = rasterise(&page(&svg), SCALE);
        let slots = slots(&window.state().diagram);
        let compared = as_pele_draws_each_icon(name, &ours, &theirs, &slots, 1.0);
        assert_eq!(compared, drawn, "{name}: the boxes of the app's icons, in paint order");
        for slot in slots.iter().filter(|slot| app_icons(&slot.name).is_some()) {
            let mine = slot.around(&ours, 1.0);
            // A bolt's foot, stroked square on in the foreground, and the ring's orange left of
            // the badge's bar.
            let (ink, at) = match slot.name.as_str() {
                "app:bolt" => (INK, [7.5 / 24.0, 14.0 / 24.0]),
                _ => (ORANGE, [5.0 / 32.0, 0.5]),
            };
            let (x, y) = slot.pixel(at, 1.0);
            let got = ours.at(x, y);
            assert!(near(got, levels(ink), 3), "{name} {}: {ink} at {at:?}: {got:?}", slot.name);
            assert!(inked(&mine, ink, 3) > 20, "{name} {}: drawn in {ink}", slot.name);
        }
        if name == "architecture" {
            // The app's icon for `database` is drawn in place of Pele's glyph: nothing is left
            // of the glyph's top, where the badge leaves the service's fill showing.
            let store = slots.iter().find(|slot| slot.name == "database").expect("the store");
            let (top, fill) = (store.pixel([12.0 / 24.0, 3.0 / 24.0], 1.0), store.pixel([0.05, 0.05], 1.0));
            let (got, want) = (ours.at(top.0, top.1), ours.at(fill.0, fill.1));
            assert_eq!(got, want, "the glyph's top is gone, and the fill shows");
        }
    }
}

#[test]
fn a_name_the_app_has_no_icon_for_is_drawn_as_without_icons() {
    let capturer = Capturer::new();
    let (_, size) = pele(ARCHITECTURE);
    let with = window(&capturer, ARCHITECTURE, as_pele().icons(app_icons), size).capture();
    let mut bare = window(&capturer, ARCHITECTURE, as_pele(), size);
    let without = bare.capture();
    let slots = slots(&bare.state().diagram);
    let same = |slot: &Slot| slot.around(&with, 1.0).rgba == slot.around(&without, 1.0).rgba;
    for slot in &slots {
        match slot.name.as_str() {
            // A name nothing knows leaves its box empty, as Pele's SVG does.
            "app:unknown" => {
                assert!(same(slot), "the unknown name's box is as without icons");
                // Inside the box, the service's fill is all there is.
                let reach = (slot.side * 0.45 * f64::from(SCALE)) as u32;
                let inside = around_pixel(&with, slot.pixel([0.5, 0.5], 1.0), reach);
                let fill = inside.at(0, 0);
                let drawn = (0..inside.height).flat_map(|y| (0..inside.width).map(move |x| (x, y)));
                assert!(drawn.clone().all(|(x, y)| inside.at(x, y) == fill), "nothing is drawn in it, over {fill:?}");
            }
            // A built-in glyph the app has no icon for is Pele's glyph, as without icons.
            "server" => {
                assert!(same(slot), "the server's glyph is as without icons");
                assert!(inked(&slot.around(&with, 1.0), INK, 3) > 20, "and it is drawn");
            }
            // The test sees an icon the app draws.
            _ => assert!(!same(slot), "{}: the app's icon is drawn", slot.name),
        }
    }
    assert_eq!(slots.len(), 5, "the group's box and four services'");
}

/// How wide the bolt's foot, a level stroke 2 units of its 24 wide, is drawn in `image` at `zoom`,
/// in device pixels: the coverage of the column of pixels across it, summed, from the fill behind
/// it to the foreground. It holds with the comparison below 2 pixels a point, where the corpus's
/// rules let a stroke a pixel too wide pass.
fn foot_width(image: &Pixels, slots: &[Slot], zoom: f64) -> (f64, f64) {
    let bolt = slots.iter().find(|slot| slot.name == "app:bolt").expect("the bolt");
    // From under the bolt's slanted side, which comes within a unit of the foot above it, to
    // the fill under the foot.
    let (x, top) = bolt.pixel([7.5 / 24.0, 12.5 / 24.0], zoom);
    let (_, bottom) = bolt.pixel([7.5 / 24.0, 17.0 / 24.0], zoom);
    let (behind, ink) = (f64::from(image.at(x, bottom)[0]), f64::from(levels(INK)[0]));
    let width = (top..bottom).map(|y| ((behind - f64::from(image.at(x, y)[0])) / (behind - ink)).clamp(0.0, 1.0)).sum();
    (width, 2.0 / 24.0 * bolt.side * zoom * f64::from(SCALE))
}

#[test]
fn an_icon_follows_the_zoom_and_the_fit() {
    let capturer = Capturer::new();
    let (svg, size) = pele(ARCHITECTURE);
    let natural = page(&svg);
    for zoom in [0.75f32, 0.5] {
        let at = (size.0 * zoom, size.1 * zoom);
        let mut window = window(&capturer, ARCHITECTURE, as_pele().icons(app_icons).zoom(zoom), at);
        let ours = window.capture();
        assert_eq!(window.state().diagram.zoom(), Some(zoom));
        let theirs = rasterise(&natural, SCALE * zoom);
        let slots = slots(&window.state().diagram);
        let compared = as_pele_draws_each_icon("zoom", &ours, &theirs, &slots, f64::from(zoom));
        assert_eq!(compared.len(), 3, "the bolt, and the badge twice: {compared:?}");
        let (width, want) = foot_width(&ours, &slots, f64::from(zoom));
        println!("zoom {zoom}: the bolt's foot is {width:.2} px wide, for {want} px");
        assert!((width - want).abs() < 0.35, "zoom {zoom}: the bolt's foot is {width:.2} px wide, for {want}");
    }
    // A fit narrower than the diagram lays it out narrower, as Pele lays it out for a `maxWidth`,
    // and draws what is still too wide smaller. 128 points is one of the fit's steps, which are
    // whole points a twelfth of an octave apart, so it lays the diagram out for that width.
    let room = 128.0;
    let options = ViewOptions::fit().pele_metrics().palette(Palette::pele()).icons(app_icons);
    let mut window = window(&capturer, ARCHITECTURE, options, (room, size.1 * 2.0));
    window.capture();
    let ours = window.capture();
    let diagram = &window.state().diagram;
    let zoom = diagram.zoom().expect("drawn natively");
    let scene = diagram.scene().expect("drawn natively");
    println!("fitted into {room} points: zoom {zoom}, laid out {:?}, at its own size {size:?}", scene.size());
    let drawn = pele_oracle::render_all_with(&[(ARCHITECTURE.to_owned(), format!("{{\"maxWidth\":{room}}}"))]);
    let PeleRender::Svg { svg, width, .. } = &drawn[0] else { panic!("Pele draws it: {drawn:?}") };
    assert_eq!(scene.size().width, *width, "laid out as Pele lays it out for {room} points");
    assert!(*width < f64::from(size.0) && zoom < 1.0, "laid out narrower, and drawn smaller: {width} at {zoom}");
    let theirs = rasterise(&page(svg), SCALE * zoom);
    let slots = slots(diagram);
    let compared = as_pele_draws_each_icon("fit", &ours, &theirs, &slots, f64::from(zoom));
    assert_eq!(compared.len(), 3, "{compared:?}");
    let (width, want) = foot_width(&ours, &slots, f64::from(zoom));
    println!("fitted at {zoom}: the bolt's foot is {width:.2} px wide, for {want} px");
    assert!((width - want).abs() < 0.35, "fitted at {zoom}: the bolt's foot is {width:.2} px wide, for {want}");
}

#[test]
fn another_icons_function_paints_again_and_lays_nothing_out() {
    let capturer = Capturer::new();
    let (_, size) = pele(ARCHITECTURE);
    let mut window = window(&capturer, ARCHITECTURE, as_pele().icons(app_icons), size);
    let first = window.capture();
    let bolt = slots(&window.state().diagram).into_iter().find(|slot| slot.name == "app:bolt").expect("the bolt");
    let before = window.state().diagram.stats();
    window.update(|viewer| viewer.options = as_pele().icons(other_icons));
    let other = window.capture();
    let after = window.state().diagram.stats();
    println!("before {before:?}\nafter  {after:?}");
    assert_eq!(
        (after.layouts, after.lowerings, after.measurements),
        (before.layouts, before.lowerings, before.measurements)
    );
    assert_eq!(after.paints, before.paints + 1, "painted again");
    assert_eq!(inked(&bolt.around(&first, 1.0), BLUE, 3), 0, "no blue before");
    assert!(inked(&bolt.around(&other, 1.0), BLUE, 3) > 100, "the other function's square is drawn");

    // The same function again is the memo's: nothing is painted.
    window.update(|viewer| viewer.options = as_pele().icons(other_icons));
    window.capture();
    assert_eq!(window.state().diagram.stats().paints, after.paints, "the same icons paint nothing again");

    // No icons paint again, and the bolt's box is empty again.
    window.update(|viewer| viewer.options = as_pele());
    let none = window.capture();
    let last = window.state().diagram.stats();
    assert_eq!((last.layouts, last.lowerings, last.paints), (before.layouts, before.lowerings, after.paints + 1));
    assert_eq!(
        inked(&bolt.around(&none, 1.0), BLUE, 3) + inked(&bolt.around(&none, 1.0), INK, 3),
        0,
        "the box is empty"
    );
}

#[test]
fn a_link_and_a_tooltip_are_found_on_the_app_s_icon() {
    let capturer = Capturer::new();
    let (_, size) = pele(FLOWCHART);
    let mut report = Vec::new();
    for icons in [true, false] {
        let options = if icons { as_pele().icons(app_icons) } else { as_pele() };
        let mut window = window(&capturer, FLOWCHART, options, size);
        window.capture();
        let badges: Vec<Slot> =
            slots(&window.state().diagram).into_iter().filter(|slot| slot.name == "app:badge").collect();
        let centre = |slot: &Slot| ((slot.at[0] + slot.side / 2.0) as f32, (slot.at[1] + slot.side / 2.0) as f32);
        let (linked, tipped) = (centre(&badges[0]), centre(&badges[1]));
        window.press(linked.0, linked.1);
        window.rest(tipped.0, tipped.1, 2.0);
        let shown = window.runtime().help_tooltip().map(|(text, _)| text.to_owned());
        let followed = window.state().followed.clone();
        report.push(format!("icons {icons}: pressed {followed:?}, tooltip {shown:?}"));
        if icons {
            assert_eq!(followed, ["https://example.com/badge"], "the linked badge is pressed");
            assert_eq!(shown.as_deref(), Some("Badge tip"), "the tipped badge shows its tooltip");
        } else {
            // An empty box is no part of the link, as an empty `<svg>` is not in a page.
            assert!(
                followed.is_empty() && shown.is_none(),
                "without icons the boxes are empty: {followed:?} {shown:?}"
            );
        }
    }
    println!("{}", report.join("\n"));
}

#[test]
fn path_data_is_read_as_svg_reads_it() {
    let view_box = [0.0, 0.0, 24.0, 24.0];
    let read = |data: &str| Icon::new(view_box).and_then(|icon| icon.fill(data, IconPaint::Current));
    let alike = [
        // Relative commands, implicit lines after a move, and lines along an axis.
        ("m1 2 3 4h1v-1H0V0l1 1z", "M1 2L4 6L5 6L5 5L0 5L0 0L1 1Z"),
        // A curve after a close starts where the closed subpath started.
        ("M1 2h3zc1 1 2 2 3 3", "M1 2L4 2ZM1 2C2 3 3 4 4 5"),
        // A smooth cubic mirrors the last handle, and a smooth quad the last control point.
        ("M0 0c1 1 2 2 3 3s1 1 2 2", "M0 0C1 1 2 2 3 3C4 4 4 4 5 5"),
        ("M0 0q1 0 1 1t1 1", "M0 0Q1 0 1 1Q1 2 2 2"),
        // Without a curve of its kind before it, a smooth curve's first handle is where it starts.
        ("M0 0L1 1S2 2 3 3", "M0 0L1 1C1 1 2 2 3 3"),
        // Numbers and flags need no separator where SVG needs none.
        ("M.5.5-1e1,2E-1a2 3 0 1110 0", "M0.5 0.5L-10 0.2A2 3 0 1 1 0 0.2"),
    ];
    for (spelled, plainly) in alike {
        assert_eq!(read(spelled), read(plainly), "{spelled} reads as {plainly}");
    }
    assert_ne!(read("M0 0l1 1"), read("M0 0L2 2"), "and the test can tell two paths apart");
    let wrong = |data: &str, at: usize| assert_eq!(read(data), Err(IconError::PathData { at }), "{data}");
    wrong("L1 1", 0);
    wrong("M1 1 L2", 7);
    wrong("M1 1x", 4);
    wrong("M0 0A1 1 0 2 0 1 1", 11);
    wrong("M0 0z 1 1", 6);
    assert_eq!(Icon::new([0.0, 0.0, 0.0, 24.0]), Err(IconError::ViewBox));
    assert_eq!(
        Icon::new(view_box).and_then(|icon| icon.stroke("M0 0h1", 0.0, IconPaint::Current)),
        Err(IconError::StrokeWidth)
    );
}
