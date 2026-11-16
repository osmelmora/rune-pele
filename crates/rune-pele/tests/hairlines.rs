//! Which of the pixels that tell a 1x drawing from Pele's lie along a slanted or curved stroke.
//!
//! At one device pixel a point Rune takes a stroke's coverage from the distance to its centre
//! line and resvg from the area it covers, and on a slanted or curved stroke the two differ by up
//! to a tenth of full coverage. `DIVERGENCES.md` accepts that. This takes each corpus source of
//! the types below that is over the corpus test's first two limits at 1x, and sorts each of its
//! unexplained pixels by the nearest stroke in the Scene's own geometry: along a slanted or curved
//! stroke, along a level or upright one, or near no stroke at all. Only the first is the
//! accepted class.

mod common;

use std::cell::RefCell;
use std::fmt::Write as _;

use common::{around_pixel, compare, in_a_row, magnified, marked, out_dir_for, rasterise, text_boxes};
use pele_core::scene::{Geometry, PaintVisit, PathCommand};
use pele_oracle::PeleRender;
use rune::{Color, Cx, El, Env, el};
use rune_pele::capture::Capturer;
use rune_pele::geometry::{Affine, Arc, Cubic, Outline, Point, Segment};
use rune_pele::{Diagram, FontCatalog, Palette, ViewError, ViewOptions};

const TYPES: [&str; 16] = [
    "sequence",
    "gantt",
    "block",
    "c4",
    "railroad",
    "info",
    "cynefin",
    "wardley",
    "venn",
    "packet",
    "eventmodeling",
    "treemap",
    "mindmap",
    "journey",
    "kanban",
    "ishikawa",
];

/// The corpus test's first two limits at 1x.
const LARGEST_PATCH: usize = 3;
const MOST_UNEXPLAINED: f64 = 5.0;

/// How far from a stroke's edge a pixel's centre may lie and still be one the stroke covers part
/// of: half a pixel's diagonal, and a little for the neighbours the comparison looks at.
const REACH: f64 = 1.0;

struct Viewer {
    diagram: Diagram,
    source: String,
    size: (f32, f32),
    error: RefCell<Option<ViewError>>,
}

fn view(s: &Viewer, cx: &mut Cx<Viewer>) -> El {
    let root = el().size(s.size.0, s.size.1).fill(Color::WHITE);
    let options = ViewOptions::natural().pele_metrics().palette(Palette::pele());
    match s.diagram.view(cx, &s.source, options) {
        Ok(drawn) => root.child(drawn),
        Err(error) => {
            s.error.replace(Some(error));
            root
        }
    }
}

/// One straight piece of a stroke's centre line, in the drawing's points.
struct Piece {
    from: Point,
    to: Point,
    half_width: f64,
    key: String,
}

impl Piece {
    fn distance(&self, [x, y]: Point) -> f64 {
        let [dx, dy] = [self.to[0] - self.from[0], self.to[1] - self.from[1]];
        let length = dx * dx + dy * dy;
        let t = if length == 0.0 {
            0.0
        } else {
            (((x - self.from[0]) * dx + (y - self.from[1]) * dy) / length).clamp(0.0, 1.0)
        };
        (x - self.from[0] - t * dx).hypot(y - self.from[1] - t * dy)
    }

    fn level_or_upright(&self) -> bool {
        self.from[0] == self.to[0] || self.from[1] == self.to[1]
    }
}

fn point(p: pele_core::scene::Point) -> Point {
    [p.x, p.y]
}

fn closed(points: &[Point]) -> Vec<Segment> {
    let mut program: Vec<Segment> = points.iter().map(|&p| Segment::Line(p)).collect();
    program[0] = Segment::Move(points[0]);
    program.push(Segment::Close);
    program
}

/// The outline SVG strokes for `geometry`, as the lowering builds it.
fn outline(geometry: &Geometry) -> Option<Vec<Segment>> {
    let arc =
        |radii: [f64; 2], large: bool, to: Point| Segment::Arc(Arc { radii, rotation: 0.0, large, sweep: true, to });
    Some(match geometry {
        Geometry::Path(commands) => commands
            .iter()
            .map(|command| match *command {
                PathCommand::Move(p) => Segment::Move(point(p)),
                PathCommand::Line(p) => Segment::Line(point(p)),
                PathCommand::Quad { control, end } => Segment::Quad { control: point(control), to: point(end) },
                PathCommand::Cubic { first, second, end } => {
                    Segment::Cubic(Cubic { c1: point(first), c2: point(second), to: point(end) })
                }
                PathCommand::Arc { rx, ry, large, sweep, end } => {
                    Segment::Arc(Arc { radii: [rx, ry], rotation: 0.0, large, sweep, to: point(end) })
                }
                PathCommand::Close => Segment::Close,
            })
            .collect(),
        Geometry::Rect { origin, size, radius } => {
            let ([x, y], [w, h]) = (point(*origin), [size.width, size.height]);
            let r = radius.max(0.0).min(w / 2.0).min(h / 2.0);
            if r == 0.0 {
                closed(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]])
            } else {
                vec![
                    Segment::Move([x + r, y]),
                    Segment::Line([x + w - r, y]),
                    arc([r, r], false, [x + w, y + r]),
                    Segment::Line([x + w, y + h - r]),
                    arc([r, r], false, [x + w - r, y + h]),
                    Segment::Line([x + r, y + h]),
                    arc([r, r], false, [x, y + h - r]),
                    Segment::Line([x, y + r]),
                    arc([r, r], false, [x + r, y]),
                    Segment::Close,
                ]
            }
        }
        Geometry::Circle { centre, radius } => {
            let [cx, cy] = point(*centre);
            let radii = [*radius; 2];
            vec![
                Segment::Move([cx, cy - radius]),
                arc(radii, true, [cx, cy + radius]),
                arc(radii, true, [cx, cy - radius]),
            ]
        }
        Geometry::Ellipse { centre, radii } => {
            let [cx, cy] = point(*centre);
            let radii = [radii.width, radii.height];
            vec![
                Segment::Move([cx, cy - radii[1]]),
                arc(radii, true, [cx, cy + radii[1]]),
                arc(radii, true, [cx, cy - radii[1]]),
            ]
        }
        Geometry::Polygon(points) => closed(&points.iter().map(|&p| point(p)).collect::<Vec<_>>()),
        Geometry::Text(_) | Geometry::Image { .. } | Geometry::Icon { .. } => return None,
    })
}

/// Every stroke of `scene` as straight pieces of its centre line, in the drawing's points.
fn strokes(scene: &pele_core::Scene) -> Vec<Piece> {
    let mut maps = vec![Affine::IDENTITY];
    let mut pieces = Vec::new();
    for visit in scene.paint() {
        match visit {
            PaintVisit::Begin(group) => {
                let own = match group.rotate {
                    Some(turn) => Affine::rotate_about(turn.degrees, [turn.centre.x, turn.centre.y]),
                    None => Affine::translate([group.translate.x, group.translate.y]),
                };
                maps.push(maps.last().expect("the root").then(&own));
            }
            PaintVisit::End => {
                maps.pop();
            }
            PaintVisit::Draw(draw) => {
                let (Some(stroke), Some(program)) = (&draw.style.stroke, outline(&draw.geometry)) else { continue };
                let outline = Outline::lower(&program).transformed(maps.last().expect("the root"));
                for line in outline.flatten(0.02) {
                    let n = line.points.len();
                    let ends = if line.closed { n } else { n.saturating_sub(1) };
                    for i in 0..ends {
                        pieces.push(Piece {
                            from: line.points[i],
                            to: line.points[(i + 1) % n],
                            half_width: stroke.width / 2.0,
                            key: draw.key.to_string(),
                        });
                    }
                }
            }
        }
    }
    pieces
}

/// What one unexplained pixel lies on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
enum Along {
    SlantedOrCurved,
    LevelOrUpright,
    Nothing,
}

#[test]
#[ignore = "a measurement, not a check: run it in release with --ignored --nocapture"]
fn which_unexplained_pixels_at_1x_lie_along_slanted_or_curved_strokes() {
    let capturer = Capturer::new();
    let mut report = String::from(
        "Each corpus source over the first two limits at 1x, its unexplained pixels sorted by the nearest stroke\n\
         in the Scene's geometry within half its width and a pixel: slanted or curved, level or upright, none.\n\n\
         type          source  patch  share  slanted  level  none  widths of the slanted\n",
    );
    let mut others = Vec::new();
    for kind in TYPES {
        let sources: Vec<String> = pele_oracle::corpus()
            .into_iter()
            .filter(|(name, _)| name == kind)
            .flat_map(|(_, sources)| sources)
            .collect();
        let pele = pele_oracle::render_all(&sources);
        for (index, (source, pele)) in sources.iter().zip(&pele).enumerate() {
            let PeleRender::Svg { svg, width, height, .. } = pele else { panic!("{kind} {index}: {pele:?}") };
            let size = (width.ceil() as f32, height.ceil() as f32);
            let viewer = Viewer {
                diagram: Diagram::new("diagram", FontCatalog::rune()),
                source: source.clone(),
                size,
                error: RefCell::new(None),
            };
            let mut window = capturer.window(viewer, view, size, Env::new(false, 1.0));
            let ours = window.capture();
            assert_eq!(*window.state().error.borrow(), None, "{kind} {index}");
            let scene = window.state().diagram.scene().expect("a native scene");
            let theirs = rasterise(&common::as_a_page_reads(svg), 1.0);
            let (apart, unexplained) = compare(&ours, &theirs, &text_boxes(&scene), 1.0);
            if apart.largest_blob <= LARGEST_PATCH && apart.share() <= MOST_UNEXPLAINED {
                continue;
            }
            let pieces = strokes(&scene);
            let mut counts = std::collections::BTreeMap::<Along, usize>::new();
            let mut widths = std::collections::BTreeSet::new();
            let mut shown = std::collections::BTreeSet::new();
            for &(x, y) in &unexplained {
                let centre = [f64::from(x) + 0.5, f64::from(y) + 0.5];
                let nearest = pieces
                    .iter()
                    .map(|piece| (piece.distance(centre) - piece.half_width, piece))
                    .filter(|(beyond, _)| *beyond <= REACH)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                let along = match nearest {
                    None => Along::Nothing,
                    Some((_, piece)) if piece.level_or_upright() => Along::LevelOrUpright,
                    Some((_, piece)) => {
                        widths.insert(format!("{}", piece.half_width * 2.0));
                        Along::SlantedOrCurved
                    }
                };
                *counts.entry(along).or_default() += 1;
                if along != Along::SlantedOrCurved {
                    let near =
                        nearest.map_or("-".to_owned(), |(beyond, piece)| format!("{} {beyond:.2} beyond", piece.key));
                    let drawing = nearest.map_or("none".to_owned(), |(_, piece)| piece.key.replace('/', "_"));
                    if shown.insert(drawing.clone()) {
                        let near = |image: &rune_pele::capture::Pixels| around_pixel(image, (x, y), 12);
                        let close = in_a_row(&[&near(&ours), &near(&theirs), &near(&marked(&ours, &unexplained))], 2);
                        common::save_in(
                            "rune-graph",
                            &magnified(&close, 12),
                            &format!("hairlines/{kind}/{index:03}-{drawing}.png"),
                        );
                        let grid = |image: &rune_pele::capture::Pixels| {
                            (y.saturating_sub(2)..=(y + 2).min(image.height - 1))
                                .map(|row| {
                                    (x.saturating_sub(3)..=(x + 3).min(image.width - 1))
                                        .map(|column| {
                                            format!("{:>3}", image.at(column, row)[..3].iter().min().expect("rgb"))
                                        })
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .collect::<Vec<_>>()
                        };
                        let (left, right) = (grid(&ours), grid(&theirs));
                        let rows = left.iter().zip(&right).map(|(l, r)| format!("    {l}   |   {r}"));
                        others.push(format!(
                            "{kind} {index} around ({x}, {y}), darkest channel, ours | Pele's:\n{}",
                            rows.collect::<Vec<_>>().join("\n")
                        ));
                    }
                    others.push(format!("{kind} {index} ({x}, {y}) {along:?}: {near}"));
                }
            }
            let count = |along| counts.get(&along).copied().unwrap_or(0);
            writeln!(
                report,
                "{kind:<13} {index:>6} {:>6} {:>6.2} {:>8} {:>6} {:>5}  {}",
                apart.largest_blob,
                apart.share(),
                count(Along::SlantedOrCurved),
                count(Along::LevelOrUpright),
                count(Along::Nothing),
                widths.into_iter().collect::<Vec<_>>().join(" ")
            )
            .unwrap();
        }
    }
    writeln!(report, "\npixels along no slanted or curved stroke:\n{}", others.join("\n")).unwrap();
    print!("{report}");
    std::fs::create_dir_all(out_dir_for("rune-graph")).unwrap();
    std::fs::write(out_dir_for("rune-graph").join("hairlines.txt"), &report).unwrap();
}
