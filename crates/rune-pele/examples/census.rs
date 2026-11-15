//! What the Scenes of the graph types ask an interpreter to draw, counted from the paint walk
//! over Pele's documentation corpus and every source harvested from Pele's tests.
//!
//! `cargo run -p rune-pele --example census` prints one row per feature and one column per type.
//! The Rune view's lowering is written against this list, so rerun it when a type is ported.

use std::collections::{BTreeMap, BTreeSet};

use pele_core::scene::{AnchorKind, Draw, Geometry, LineCap, LineJoin, Paint, PaintVisit, PathCommand, Point, Scene};
use pele_core::{FontFamily, FontWeight, Script};

const TYPES: [&str; 7] = ["flowchart", "class", "state", "er", "requirement", "usecase", "swimlane"];

fn sources(kind: &str) -> Vec<String> {
    let corpus = pele_oracle::corpus().into_iter().filter(|(name, _)| name == kind).flat_map(|(_, sources)| sources);
    let path = pele_oracle::root().join(format!("tests/fixtures/{kind}/harvest.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: serde_json::Value = serde_json::from_str(&text).expect("the harvest is JSON");
    let lists = harvest.as_object().expect("an object").values().filter_map(|list| list.as_array());
    let harvested = lists.flatten().filter_map(|source| source.as_str().map(str::to_owned));
    corpus.chain(harvested.collect::<Vec<_>>()).collect()
}

/// A contour as a closed ring of points, its curves sampled.
fn rings(commands: &[PathCommand]) -> Vec<Vec<[f64; 2]>> {
    let p = |point: Point| [point.x, point.y];
    let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut here = [0.0; 2];
    for command in commands {
        let ring = match command {
            PathCommand::Move(to) => {
                rings.push(vec![p(*to)]);
                here = p(*to);
                continue;
            }
            PathCommand::Close => continue,
            _ => rings.last_mut().expect("a path starts with a move"),
        };
        let end = match *command {
            PathCommand::Line(to) => p(to),
            PathCommand::Quad { control, end } => {
                let (c, e) = (p(control), p(end));
                ring.extend((1..8).map(|i| {
                    let t = f64::from(i) / 8.0;
                    let u = 1.0 - t;
                    [0, 1].map(|k| u * u * here[k] + 2.0 * u * t * c[k] + t * t * e[k])
                }));
                e
            }
            PathCommand::Cubic { first, second, end } => {
                let (a, b, e) = (p(first), p(second), p(end));
                ring.extend((1..8).map(|i| {
                    let t = f64::from(i) / 8.0;
                    let u = 1.0 - t;
                    [0, 1].map(|k| {
                        u * u * u * here[k] + 3.0 * u * u * t * a[k] + 3.0 * u * t * t * b[k] + t * t * t * e[k]
                    })
                }));
                e
            }
            PathCommand::Arc { rx, ry, large, sweep, end } => {
                let arc = rune_pele::geometry::Arc { radii: [rx, ry], rotation: 0.0, large, sweep, to: p(end) };
                let mut at = here;
                for edge in rune_pele::geometry::arc_edges(here, &arc) {
                    if let rune_pele::geometry::Edge::Cubic(c) = edge {
                        ring.extend((1..=8).map(|i| {
                            let t = f64::from(i) / 8.0;
                            let u = 1.0 - t;
                            [0, 1].map(|k| {
                                u * u * u * at[k]
                                    + 3.0 * u * u * t * c.c1[k]
                                    + 3.0 * u * t * t * c.c2[k]
                                    + t * t * t * c.to[k]
                            })
                        }));
                        at = c.to;
                    }
                }
                ring.pop();
                p(end)
            }
            PathCommand::Move(_) | PathCommand::Close => unreachable!("handled above"),
        };
        ring.push(end);
        here = end;
    }
    rings
}

fn area(ring: &[[f64; 2]]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn inside(point: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut winding = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > point[1]) != (b[1] > point[1]) && point[0] < a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            winding = !winding;
        }
    }
    winding
}

/// How the contours of one filled path lie: apart, one a hole in another, or anything else.
fn contours(commands: &[PathCommand]) -> &'static str {
    let rings: Vec<Vec<[f64; 2]>> = rings(commands).into_iter().filter(|ring| ring.len() >= 3).collect();
    let mut found = BTreeSet::new();
    for (i, a) in rings.iter().enumerate() {
        for b in &rings[i + 1..] {
            let (a_in_b, b_in_a) = (a.iter().all(|&p| inside(p, b)), b.iter().all(|&p| inside(p, a)));
            let apart = !a.iter().any(|&p| inside(p, b)) && !b.iter().any(|&p| inside(p, a));
            found.insert(if apart {
                "apart"
            } else if (a_in_b || b_in_a) && (area(a) > 0.0) != (area(b) > 0.0) {
                "hole"
            } else if a_in_b || b_in_a {
                "nested the same way round"
            } else {
                "crossing"
            });
        }
    }
    match found.iter().copied().collect::<Vec<_>>()[..] {
        [] => "one contour",
        [only] => only,
        _ => "mixed",
    }
}

#[derive(Default)]
struct Census {
    counts: BTreeMap<String, [usize; TYPES.len()]>,
}

impl Census {
    fn add(&mut self, column: usize, name: impl Into<String>, by: usize) {
        if by > 0 {
            self.counts.entry(name.into()).or_default()[column] += by;
        }
    }

    fn max(&mut self, column: usize, name: &str, value: usize) {
        let cell = &mut self.counts.entry(name.to_owned()).or_default()[column];
        *cell = (*cell).max(value);
    }
}

fn paint(paint: Paint) -> &'static str {
    match paint {
        Paint::None => "none",
        Paint::Role(_) => "a role",
        Paint::Series(_) => "a series",
        Paint::OnSeries(_) => "ink on a series",
        Paint::Source(_) => "a source colour",
        Paint::Mix { .. } => "a mix of two roles",
    }
}

fn number(value: f64) -> String {
    format!("{value}")
}

fn shape(census: &mut Census, column: usize, draw: &Draw, alpha: &dyn Fn(Paint) -> f64) {
    let mut count = |name: String, by: bool| census.add(column, name, usize::from(by));
    let style = &draw.style;
    let filled = style.fill != Paint::None;
    count(format!("fill: {}", paint(style.fill)), true);
    if filled {
        count(format!("fill-opacity {}", number(style.fill_opacity)), style.fill_opacity != 1.0);
        count("fill: its colour is translucent".to_owned(), alpha(style.fill) < 1.0);
    }
    count(format!("opacity {} on a drawing", number(style.opacity)), style.opacity != 1.0);
    if let Some(stroke) = &style.stroke {
        count(format!("stroke: {}", paint(stroke.paint)), true);
        count(format!("stroke width {}", number(stroke.width)), true);
        count(format!("stroke dash {:?}", stroke.dash), !stroke.dash.is_empty());
        let cap = if stroke.cap == LineCap::Round { "round" } else { "butt" };
        let join = if stroke.join == LineJoin::Round { "round" } else { "miter" };
        count(format!("stroke cap {cap}, join {join}"), true);
        count(format!("stroke dashed with cap {cap}"), !stroke.dash.is_empty());
        count(format!("stroke-opacity {}", number(stroke.opacity)), stroke.opacity != 1.0);
        count("stroke: its colour is translucent".to_owned(), alpha(stroke.paint) < 1.0);
        count("stroke and fill both translucent".to_owned(), {
            let stroke_alpha = alpha(stroke.paint) * stroke.opacity;
            filled && stroke_alpha < 1.0 && alpha(style.fill) * style.fill_opacity < 1.0
        });
    }
    match &draw.geometry {
        Geometry::Path(commands) => {
            count("path".to_owned(), true);
            let has = |find: fn(&PathCommand) -> bool| commands.iter().any(find);
            count("path with cubics".to_owned(), has(|c| matches!(c, PathCommand::Cubic { .. })));
            count("path with quadratics".to_owned(), has(|c| matches!(c, PathCommand::Quad { .. })));
            count("path with arcs".to_owned(), has(|c| matches!(c, PathCommand::Arc { .. })));
            count("path with large arcs".to_owned(), has(|c| matches!(c, PathCommand::Arc { large: true, .. })));
            count(
                "path with elliptical arcs".to_owned(),
                has(|c| matches!(c, PathCommand::Arc { rx, ry, .. } if rx != ry)),
            );
            let moves = commands.iter().filter(|c| matches!(c, PathCommand::Move(_))).count();
            let closes = commands.iter().filter(|c| matches!(c, PathCommand::Close)).count();
            count("path of several subpaths".to_owned(), moves > 1);
            count("path filled".to_owned(), filled);
            count("path filled and left open".to_owned(), filled && closes < moves);
            count("path stroked".to_owned(), style.stroke.is_some());
            if filled {
                count(format!("filled path contours: {}", contours(commands)), true);
            }
        }
        Geometry::Rect { radius, .. } => {
            count("rect".to_owned(), true);
            count("rect with round corners".to_owned(), *radius > 0.0);
        }
        Geometry::Circle { .. } => count("circle".to_owned(), true),
        Geometry::Ellipse { .. } => count("ellipse".to_owned(), true),
        Geometry::Polygon(points) => {
            count("polygon".to_owned(), true);
            count(format!("polygon of {} points", points.len()), true);
        }
        Geometry::Image { .. } => count("image".to_owned(), true),
        Geometry::Icon { .. } => count("icon".to_owned(), true),
        Geometry::Text(_) => unreachable!("text is counted apart"),
    }
}

fn text(census: &mut Census, column: usize, draw: &Draw, turned: bool) {
    let Geometry::Text(run) = &draw.geometry else { unreachable!("called for text") };
    let mut count = |name: String, by: bool| census.add(column, name, usize::from(by));
    count("text run".to_owned(), true);
    let anchor = match run.anchor {
        AnchorKind::Start => "start",
        AnchorKind::Middle => "middle",
        AnchorKind::End => "end",
    };
    count(format!("text anchored at its {anchor}"), true);
    count("text bold".to_owned(), run.font.weight == FontWeight::Bold);
    count("text italic".to_owned(), run.font.italic);
    count("text monospace".to_owned(), run.font.family == FontFamily::Mono);
    count("text underlined".to_owned(), run.underline);
    count("text sub or superscript".to_owned(), run.script != Script::Normal);
    count(format!("text size {}", number(run.font.size)), true);
    count(format!("text fill: {}", paint(draw.style.fill)), true);
    count("text stroked (a halo)".to_owned(), draw.style.stroke.is_some());
    count("text with opacity".to_owned(), draw.style.opacity != 1.0 || draw.style.fill_opacity != 1.0);
    count("text turned".to_owned(), turned);
    count("text empty".to_owned(), run.text.is_empty());
}

fn walk(census: &mut Census, column: usize, scene: &Scene) {
    let paints = scene.resolve_paints(&pele_core::scene::Palette::pele(), false);
    let alpha = |paint: Paint| paints.get(paint).map_or(1.0, |colour| colour.alpha);
    let (mut depth, mut drawings) = (0usize, 0usize);
    // Each open group: whether it or a group around it turns, fades or links.
    let mut open: Vec<[bool; 3]> = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => {
                depth += 1;
                census.max(column, "deepest group nesting", depth);
                let [turned, faded, linked] = open.last().copied().unwrap_or_default();
                census.add(column, "group", 1);
                census.add(column, "group that is a link", usize::from(group.link.is_some()));
                census.add(column, "group that is a link inside a link", usize::from(group.link.is_some() && linked));
                if let Some(link) = &group.link {
                    census.add(column, format!("link target {:?}", link.target), 1);
                    census.add(column, "link to a note of the host", usize::from(link.internal));
                }
                census.add(column, "group the pointer passes through", usize::from(group.untouchable));
                if group.opacity != 1.0 {
                    census.add(column, format!("group opacity {}", number(group.opacity)), 1);
                    census.add(column, "group opacity inside group opacity", usize::from(faded));
                }
                if let Some(rotation) = group.rotate {
                    census.add(column, format!("group turned {} degrees", number(rotation.degrees)), 1);
                    census.add(column, "group turned inside a turned group", usize::from(turned));
                }
                open.push([
                    turned || group.rotate.is_some(),
                    faded || group.opacity != 1.0,
                    linked || group.link.is_some(),
                ]);
            }
            PaintVisit::End => {
                depth -= 1;
                open.pop();
            }
            PaintVisit::Draw(draw) => {
                drawings += 1;
                let [turned, faded, _] = open.last().copied().unwrap_or_default();
                census.add(column, "drawing", 1);
                census.add(column, "drawing in a turned group", usize::from(turned));
                census.add(column, "drawing in a faded group", usize::from(faded));
                if matches!(draw.geometry, Geometry::Text(_)) {
                    text(census, column, &draw, turned);
                } else {
                    shape(census, column, &draw, &alpha);
                }
            }
        }
    }
    census.max(column, "most drawings in one source", drawings);
    let unread = scene.unread_styles();
    census.add(column, "source with a style the walk does not read", usize::from(!unread.is_empty()));
    for declaration in unread {
        let property = declaration.split(':').next().unwrap_or_default().to_owned();
        census.add(column, format!("unread style: {property}"), 1);
    }
    let tooltips = scene.entities().filter(|entity| scene.tooltip(entity).is_some()).count();
    census.add(column, "entity with a tooltip", tooltips);
}

fn main() {
    let mut census = Census::default();
    for (column, kind) in TYPES.iter().enumerate() {
        for source in sources(kind) {
            census.add(column, "source", 1);
            if let Ok(scene) = pele_core::parity_scene(&source) {
                census.add(column, "source that draws", 1);
                walk(&mut census, column, scene.scene());
            }
        }
    }
    let width = census.counts.keys().map(String::len).max().unwrap_or(0);
    print!("{:width$}", "");
    for kind in TYPES {
        print!(" {kind:>11}");
    }
    println!();
    for (name, counts) in &census.counts {
        print!("{name:width$}");
        for count in counts {
            print!(" {count:>11}");
        }
        println!();
    }
}
