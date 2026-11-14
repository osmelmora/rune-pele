//! Lowers a Scene to what Rune's public `Shape` and text elements draw: endpoint arcs become
//! cubic Béziers, a fill plus a stroke become ordered shape ops, and text lands on its baseline.
//!
//! A [`Drawing`] is made once per layout, with every outline built and every run shaped and
//! placed. Painting it only looks its paints up, so a palette change shapes nothing.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use pele_core::Scene;
use pele_core::scene::{AnchorKind, Draw, Geometry, Paint, PaintVisit, PathCommand, ResolvedPaints, Rgba};
use rune::shape::{LineCap, Path};
use rune::{Color, El, Shape, Span, TextStyle, canvas, el, rich};

use crate::text::{Anchor, Baselines, style};

/// A point in the drawing's user units, y down.
pub type Point = [f64; 2];

/// One absolute command of an SVG path program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Cubic(Cubic),
    Arc(Arc),
    Close,
}

/// SVG's endpoint arc, `A rx,ry rotation large-arc sweep x,y`, with `rotation` in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub radii: [f64; 2],
    pub rotation: f64,
    pub large: bool,
    pub sweep: bool,
    pub to: Point,
}

/// A cubic Bézier from the current point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cubic {
    pub c1: Point,
    pub c2: Point,
    pub to: Point,
}

/// An arc-free path: what a program lowers to and what Rune's `Path` builds from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    Move(Point),
    Line(Point),
    Cubic(Cubic),
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    edges: Vec<Edge>,
}

/// A stroke as Pele's slices draw it: round joins, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Color,
}

/// Paint that Rune's public shapes cannot draw correctly yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A fill of more than one contour (a full donut, a shape with holes). `Shape::fill_path`
    /// fills each contour on its own, so the hole would be painted over.
    CompoundFill,
    /// A drawing no ported view lowers yet: a curve, a polygon, an ellipse, a picture or an icon.
    Drawing,
}

/// The cubics that draw `arc` from `from`, each spanning at most 90 degrees, per the SVG 1.1
/// implementation notes F.6.5 and F.6.6. A zero radius draws a straight line; an arc that ends
/// where it starts draws nothing.
pub fn arc_edges(from: Point, arc: &Arc) -> Vec<Edge> {
    let to = arc.to;
    if from == to {
        return Vec::new();
    }
    let (mut rx, mut ry) = (arc.radii[0].abs(), arc.radii[1].abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![Edge::Line(to)];
    }
    let (sin, cos) = arc.rotation.to_radians().sin_cos();
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let scale = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if scale > 1.0 {
        rx *= scale.sqrt();
        ry *= scale.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let sign = if arc.large == arc.sweep { -1.0 } else { 1.0 };
    let k = sign * (num / den).max(0.0).sqrt();
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let centre = [cos * cx1 - sin * cy1 + (from[0] + to[0]) / 2.0, sin * cx1 + cos * cy1 + (from[1] + to[1]) / 2.0];
    let start = ((y1 - cy1) / ry).atan2((x1 - cx1) / rx);
    let mut sweep = ((-y1 - cy1) / ry).atan2((-x1 - cx1) / rx) - start;
    if arc.sweep && sweep < 0.0 {
        sweep += TAU;
    } else if !arc.sweep && sweep > 0.0 {
        sweep -= TAU;
    }
    let pieces = (sweep.abs() / FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let place = |u: f64, v: f64| [centre[0] + rx * cos * u - ry * sin * v, centre[1] + rx * sin * u + ry * cos * v];
    (0..pieces)
        .map(|i| {
            let (a, b) = (start + step * i as f64, start + step * (i + 1) as f64);
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            let end = if i + 1 == pieces { to } else { place(cb, sb) };
            Edge::Cubic(Cubic { c1: place(ca - t * sa, sa + t * ca), c2: place(cb + t * sb, sb - t * cb), to: end })
        })
        .collect()
}

impl Outline {
    pub fn lower(program: &[Segment]) -> Outline {
        let mut edges = Vec::with_capacity(program.len());
        let (mut at, mut start) = ([0.0; 2], [0.0; 2]);
        for segment in program {
            match *segment {
                Segment::Move(p) => {
                    edges.push(Edge::Move(p));
                    (at, start) = (p, p);
                }
                Segment::Line(p) => {
                    edges.push(Edge::Line(p));
                    at = p;
                }
                Segment::Cubic(c) => {
                    edges.push(Edge::Cubic(c));
                    at = c.to;
                }
                Segment::Arc(arc) => {
                    edges.extend(arc_edges(at, &arc));
                    at = arc.to;
                }
                Segment::Close => {
                    edges.push(Edge::Close);
                    at = start;
                }
            }
        }
        Outline { edges }
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Bakes a `translate(dx, dy)` group transform into the coordinates.
    pub fn translated(&self, [dx, dy]: Point) -> Outline {
        let p = |[x, y]: Point| [x + dx, y + dy];
        let edges = self
            .edges
            .iter()
            .map(|edge| match *edge {
                Edge::Move(a) => Edge::Move(p(a)),
                Edge::Line(a) => Edge::Line(p(a)),
                Edge::Cubic(c) => Edge::Cubic(Cubic { c1: p(c.c1), c2: p(c.c2), to: p(c.to) }),
                Edge::Close => Edge::Close,
            })
            .collect();
        Outline { edges }
    }

    fn contours(&self) -> usize {
        self.edges.iter().filter(|edge| matches!(edge, Edge::Move(_))).count()
    }

    /// The least box holding every point and control point, which holds the curve too.
    fn bounds(&self) -> Option<[Point; 2]> {
        let points = self.edges.iter().flat_map(|edge| match *edge {
            Edge::Move(a) | Edge::Line(a) => vec![a],
            Edge::Cubic(c) => vec![c.c1, c.c2, c.to],
            Edge::Close => vec![],
        });
        points.fold(None, |bounds, [x, y]| {
            let [low, high] = bounds.unwrap_or([[x, y]; 2]);
            Some([[low[0].min(x), low[1].min(y)], [high[0].max(x), high[1].max(y)]])
        })
    }

    fn path(&self) -> Path {
        let f = |[x, y]: Point| [x as f32, y as f32];
        self.edges.iter().fold(Path::new(), |path, edge| match *edge {
            Edge::Move(a) => path.move_to(f(a)),
            Edge::Line(a) => path.line_to(f(a)),
            Edge::Cubic(c) => path.cubic_to(f(c.c1), f(c.c2), f(c.to)),
            Edge::Close => path.close(),
        })
    }

    /// The fill, then the stroke over it, as SVG paints one `<path>`.
    pub fn paint(&self, fill: Option<Color>, stroke: Option<Stroke>) -> Result<Shape, Unsupported> {
        if fill.is_some() && self.contours() > 1 {
            return Err(Unsupported::CompoundFill);
        }
        let path = self.path();
        let shape = fill.map_or_else(Shape::new, |colour| Shape::new().fill_path(&path, colour));
        Ok(match stroke {
            Some(s) => shape.stroke_path(&path, s.width, LineCap::Round, s.color),
            None => shape,
        })
    }
}

/// The point at `angle` radians clockwise from twelve o'clock, as Pele's pie measures angles.
fn polar(angle: f64, radius: f64) -> Point {
    [angle.sin() * radius, -angle.cos() * radius]
}

/// Pele's pie slice with no hole, about the origin: `M0,0 L… A… Z`.
pub fn wedge(start: f64, end: f64, radius: f64) -> Vec<Segment> {
    let arc = Arc { radii: [radius; 2], rotation: 0.0, large: end - start > PI, sweep: true, to: polar(end, radius) };
    vec![Segment::Move([0.0, 0.0]), Segment::Line(polar(start, radius)), Segment::Arc(arc), Segment::Close]
}

/// Pele's donut slice: the outer arc clockwise, then the inner arc back.
pub fn ring_wedge(start: f64, end: f64, outer: f64, inner: f64) -> Vec<Segment> {
    let large = end - start > PI;
    let out = Arc { radii: [outer; 2], rotation: 0.0, large, sweep: true, to: polar(end, outer) };
    let back = Arc { radii: [inner; 2], rotation: 0.0, large, sweep: false, to: polar(start, inner) };
    vec![
        Segment::Move(polar(start, outer)),
        Segment::Arc(out),
        Segment::Line(polar(end, inner)),
        Segment::Arc(back),
        Segment::Close,
    ]
}

/// A Scene as Rune elements to be, with its paints still to resolve.
pub struct Drawing {
    size: [f32; 2],
    items: Vec<Item>,
}

enum Item {
    Shape(ShapeItem),
    Text(TextItem),
}

/// A shape in a box of its own, its outline relative to the box.
struct ShapeItem {
    key: String,
    at: [f32; 2],
    size: [f32; 2],
    outline: Outline,
    area: Area,
    fill: Paint,
    stroke: Option<(Paint, f32)>,
}

/// How a fill covers what its outline encloses.
enum Area {
    /// One contour, which `Shape::fill_path` fills.
    Simple,
    /// Two concentric circles winding opposite ways, a full donut: the band between them is a
    /// stroke of the circle halfway between, `width` wide. Rune strokes a polyline one segment at
    /// a time, so round caps fill the wedge between segments; a translucent fill would show
    /// where they overlap.
    Ring { middle: Outline, width: f32 },
}

struct TextItem {
    key: String,
    content: String,
    style: TextStyle,
    /// The top-left of the run's box.
    at: [f32; 2],
    fill: Paint,
    underline: bool,
}

impl Drawing {
    /// Lowers `scene` in paint order, shaping its text with `baselines` at their device scale.
    /// Groups flatten: each drawing already carries its groups' translation.
    pub fn new(scene: &Scene, baselines: &mut Baselines, scale: f32) -> Result<Drawing, Unsupported> {
        let mut items = Vec::new();
        for visit in scene.paint() {
            match visit {
                PaintVisit::Begin(group) => {
                    debug_assert_eq!(
                        group.opacity, 1.0,
                        "group opacity needs a Rune layer; no ported type has one yet"
                    );
                }
                PaintVisit::End => {}
                PaintVisit::Draw(draw) => items.extend(lower_draw(draw, baselines, scale)?),
            }
        }
        let size = scene.size();
        Ok(Drawing { size: [size.width as f32, size.height as f32], items })
    }

    /// The drawing as keyed sibling elements in paint order, in `paints`' colours.
    pub fn el(&self, paints: &ResolvedPaints) -> El {
        el().size(self.size[0], self.size[1]).children(self.items.iter().filter_map(|item| item.el(paints)))
    }
}

pub(crate) fn colour(rgba: Rgba) -> Color {
    let [r, g, b] = rgba.linear_srgb.map(|c| c as f32);
    Color { r, g, b, a: rgba.alpha as f32 }
}

impl Item {
    fn el(&self, paints: &ResolvedPaints) -> Option<El> {
        let resolve = |paint: Paint| paints.get(paint).map(colour);
        match self {
            Item::Shape(shape) => {
                let fill = resolve(shape.fill);
                let stroke = shape.stroke.and_then(|(paint, width)| Some(Stroke { width, color: resolve(paint)? }));
                let drawn = match &shape.area {
                    Area::Simple => {
                        shape.outline.paint(fill, stroke).expect("an outline lowered as simple has one contour")
                    }
                    Area::Ring { middle, width } => {
                        let body = fill.map_or_else(Shape::new, |c| {
                            Shape::new().stroke_path(&middle.path(), *width, LineCap::Round, c)
                        });
                        match stroke {
                            Some(s) => body.stroke_path(&shape.outline.path(), s.width, LineCap::Round, s.color),
                            None => body,
                        }
                    }
                };
                let [x, y] = shape.at;
                Some(canvas(drawn).key(shape.key.as_str()).absolute(x, y).size(shape.size[0], shape.size[1]))
            }
            Item::Text(run) => {
                let fill = resolve(run.fill)?;
                let element = if run.underline {
                    rich(run.content.as_str(), vec![Span::new(0, run.content.len()).underline(fill)])
                } else {
                    rune::text(run.content.as_str())
                };
                Some(element.font(run.style).color(fill).key(run.key.as_str()).absolute(run.at[0], run.at[1]))
            }
        }
    }
}

fn point(origin: pele_core::scene::Point, p: pele_core::scene::Point) -> Point {
    [origin.x + p.x, origin.y + p.y]
}

/// Whole circles, as `[centre, radius]`, where every contour of `commands` is one: a move, then
/// two half-turn arcs of one radius back to the start.
fn circles(commands: &[PathCommand]) -> Option<Vec<(Point, f64)>> {
    commands
        .split_inclusive(|c| *c == PathCommand::Close)
        .map(|contour| match *contour {
            [
                PathCommand::Move(start),
                PathCommand::Arc { rx, ry, end: opposite, .. },
                PathCommand::Arc { rx: rx2, ry: ry2, end, .. },
                PathCommand::Close,
            ] if rx == ry && rx2 == rx && ry2 == ry && end == start => {
                let centre = [(start.x + opposite.x) / 2.0, (start.y + opposite.y) / 2.0];
                Some((centre, rx))
            }
            _ => None,
        })
        .collect()
}

fn lower_draw(draw: Draw, baselines: &mut Baselines, scale: f32) -> Result<Option<Item>, Unsupported> {
    let key = draw.key.to_string();
    let origin = draw.origin;
    let commands = match draw.geometry {
        Geometry::Path(commands) => commands,
        Geometry::Rect { origin: corner, size, radius } => rect(corner, size.width, size.height, radius),
        Geometry::Circle { centre, radius } => circle(centre, radius),
        Geometry::Ellipse { .. } | Geometry::Polygon(_) | Geometry::Image { .. } | Geometry::Icon { .. } => {
            return Err(Unsupported::Drawing);
        }
        Geometry::Text(run) => {
            let style = style(&run.font);
            let line = baselines.line(&run.text, style);
            let anchor = match run.anchor {
                AnchorKind::Start => Anchor::Start,
                AnchorKind::Middle => Anchor::Middle,
                AnchorKind::End => Anchor::End,
            };
            // A browser's own choice for `baseline-shift: sub` and `super`, as a share of the
            // smaller size the run is set in.
            let shift = match run.script {
                pele_core::Script::Normal => 0.0,
                pele_core::Script::Sub => 0.3 * run.font.size,
                pele_core::Script::Super => -0.45 * run.font.size,
            };
            let [x, y] = point(origin, run.baseline);
            let at = [x as f32 - anchor.fraction() * line.advance, (y + shift) as f32 - line.baseline];
            let item = TextItem { key, content: run.text, style, at, fill: draw.style.fill, underline: run.underline };
            return Ok(Some(Item::Text(item)));
        }
    };
    if commands.iter().any(|c| matches!(c, PathCommand::Quad { .. } | PathCommand::Cubic { .. })) {
        return Err(Unsupported::Drawing);
    }
    let segment = |c: &PathCommand| match *c {
        PathCommand::Move(p) => Segment::Move(point(origin, p)),
        PathCommand::Line(p) => Segment::Line(point(origin, p)),
        PathCommand::Arc { rx, ry, large, sweep, end } => {
            Segment::Arc(Arc { radii: [rx, ry], rotation: 0.0, large, sweep, to: point(origin, end) })
        }
        PathCommand::Close => Segment::Close,
        PathCommand::Quad { .. } | PathCommand::Cubic { .. } => unreachable!("curves are refused above"),
    };
    let outline = Outline::lower(&commands.iter().map(segment).collect::<Vec<_>>());
    let stroke = draw.style.stroke.map(|stroke| (stroke.paint, stroke.width as f32));
    let area = match circles(&commands).as_deref() {
        _ if outline.contours() <= 1 || draw.style.fill == Paint::None => Area::Simple,
        Some(&[(centre, outer), (other, inner)]) if centre == other => {
            let middle = (outer + inner) / 2.0;
            let ring = circle(pele_core::scene::Point { x: centre[0], y: centre[1] }, middle);
            let middle = Outline::lower(&ring.iter().map(segment).collect::<Vec<_>>());
            Area::Ring { middle, width: (outer - inner).abs() as f32 }
        }
        _ => return Err(Unsupported::CompoundFill),
    };
    let Some([low, high]) = outline.bounds() else { return Ok(None) };
    // The box starts on a device pixel, so the shapes either side of a seam move together.
    let pad = stroke.map_or(0.0, |(_, width)| f64::from(width) / 2.0) + 1.0;
    let scale = f64::from(scale);
    let at = [((low[0] - pad) * scale).floor() / scale, ((low[1] - pad) * scale).floor() / scale];
    let size = [((high[0] + pad - at[0]) * scale).ceil() / scale, ((high[1] + pad - at[1]) * scale).ceil() / scale];
    let local = |o: &Outline| o.translated([-at[0], -at[1]]);
    let area = match area {
        Area::Ring { middle, width } => Area::Ring { middle: local(&middle), width },
        simple => simple,
    };
    Ok(Some(Item::Shape(ShapeItem {
        key,
        at: at.map(|c| c as f32),
        size: size.map(|c| c as f32),
        outline: local(&outline),
        area,
        fill: draw.style.fill,
        stroke,
    })))
}

fn circle(centre: pele_core::scene::Point, r: f64) -> Vec<PathCommand> {
    let at = |dy: f64| pele_core::scene::Point { x: centre.x, y: centre.y + dy };
    vec![
        PathCommand::Move(at(-r)),
        PathCommand::Arc { rx: r, ry: r, large: true, sweep: true, end: at(r) },
        PathCommand::Arc { rx: r, ry: r, large: true, sweep: true, end: at(-r) },
        PathCommand::Close,
    ]
}

/// SVG's `<rect>` with one corner radius, which it clamps to half of each side.
fn rect(corner: pele_core::scene::Point, w: f64, h: f64, radius: f64) -> Vec<PathCommand> {
    let r = radius.max(0.0).min(w / 2.0).min(h / 2.0);
    let p = |x: f64, y: f64| pele_core::scene::Point { x: corner.x + x, y: corner.y + y };
    let arc = |end| PathCommand::Arc { rx: r, ry: r, large: false, sweep: true, end };
    vec![
        PathCommand::Move(p(r, 0.0)),
        PathCommand::Line(p(w - r, 0.0)),
        arc(p(w, r)),
        PathCommand::Line(p(w, h - r)),
        arc(p(w - r, h)),
        PathCommand::Line(p(r, h)),
        arc(p(0.0, h - r)),
        PathCommand::Line(p(0.0, r)),
        arc(p(r, 0.0)),
        PathCommand::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cubics(edges: &[Edge]) -> Vec<Cubic> {
        edges
            .iter()
            .map(|edge| match edge {
                Edge::Cubic(c) => *c,
                other => panic!("an arc lowers to cubics only, got {other:?}"),
            })
            .collect()
    }

    fn sample(from: Point, c: &Cubic, t: f64) -> Point {
        let u = 1.0 - t;
        let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        let axis = |i: usize| b0 * from[i] + b1 * c.c1[i] + b2 * c.c2[i] + b3 * c.to[i];
        [axis(0), axis(1)]
    }

    /// Every sampled point of `edges`, starting at `from`.
    fn samples(from: Point, edges: &[Edge]) -> Vec<Point> {
        let mut at = from;
        let mut out = Vec::new();
        for c in cubics(edges) {
            out.extend((0..=16).map(|i| sample(at, &c, i as f64 / 16.0)));
            at = c.to;
        }
        out
    }

    /// How far `p` is from the ellipse with these radii and rotation about `centre`, relative to
    /// its size: 0 on the ellipse.
    fn off_ellipse(p: Point, centre: Point, [rx, ry]: [f64; 2], rotation: f64) -> f64 {
        let (sin, cos) = rotation.to_radians().sin_cos();
        let (dx, dy) = (p[0] - centre[0], p[1] - centre[1]);
        let (u, v) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        ((u / rx).powi(2) + (v / ry).powi(2)).sqrt() - 1.0
    }

    fn circle_arc(to: Point, r: f64, large: bool, sweep: bool) -> Arc {
        Arc { radii: [r, r], rotation: 0.0, large, sweep, to }
    }

    #[test]
    fn a_quarter_circle_is_one_cubic_on_the_circle_ending_exactly_at_its_endpoint() {
        let edges = arc_edges([100.0, 0.0], &circle_arc([0.0, 100.0], 100.0, false, true));
        let [c] = cubics(&edges)[..] else { panic!("one cubic for 90 degrees: {edges:?}") };
        assert_eq!(c.to, [0.0, 100.0]);
        let worst = samples([100.0, 0.0], &edges)
            .iter()
            .map(|&p| off_ellipse(p, [0.0; 2], [100.0; 2], 0.0).abs())
            .fold(0.0, f64::max);
        assert!(worst < 3e-4, "a 90-degree cubic stays within 0.03% of the radius: {worst}");
    }

    #[test]
    fn no_cubic_spans_more_than_ninety_degrees() {
        for (degrees, want) in [(45.0, 1), (90.0, 1), (90.5, 2), (180.0, 2), (284.4, 4), (359.0, 4)] {
            let end: f64 = f64::to_radians(degrees);
            let edges = arc_edges(polar(0.0, 50.0), &circle_arc(polar(end, 50.0), 50.0, degrees > 180.0, true));
            assert_eq!(cubics(&edges).len(), want, "{degrees} degrees");
        }
    }

    #[test]
    fn the_flags_choose_which_of_the_four_arcs_through_two_points() {
        let (from, to, r) = ([0.0, 0.0], [10.0, 0.0], 10.0);
        let below = |p: &Point| p[1] > 1e-9;
        for (large, sweep, centre_y) in [
            (false, true, 75f64.sqrt()),
            (false, false, -75f64.sqrt()),
            (true, true, -75f64.sqrt()),
            (true, false, 75f64.sqrt()),
        ] {
            let edges = arc_edges(from, &circle_arc(to, r, large, sweep));
            let points = samples(from, &edges);
            for p in &points {
                assert!(
                    off_ellipse(*p, [5.0, centre_y], [r, r], 0.0).abs() < 3e-4,
                    "large {large} sweep {sweep}: {p:?}"
                );
            }
            let mid = points[points.len() / 2];
            let span = if large { "long" } else { "short" };
            assert_eq!(below(&mid), (centre_y > 0.0) == large, "{span} way round, sweep {sweep}: {mid:?}");
        }
    }

    #[test]
    fn radii_too_small_to_reach_grow_to_a_half_ellipse() {
        let edges = arc_edges(
            [0.0, 0.0],
            &Arc { radii: [1.0, 2.0], rotation: 0.0, large: false, sweep: true, to: [10.0, 0.0] },
        );
        for p in samples([0.0, 0.0], &edges) {
            assert!(off_ellipse(p, [5.0, 0.0], [5.0, 10.0], 0.0).abs() < 3e-4, "{p:?}");
        }
        assert_eq!(cubics(&edges).len(), 2);
    }

    #[test]
    fn a_rotated_ellipse_arc_stays_on_its_ellipse() {
        let (centre, radii, rotation) = ([30.0, -20.0], [40.0, 15.0], 30.0);
        let (sin, cos) = f64::to_radians(rotation).sin_cos();
        let on = |a: f64| {
            let (u, v) = (radii[0] * a.cos(), radii[1] * a.sin());
            [centre[0] + cos * u - sin * v, centre[1] + sin * u + cos * v]
        };
        let (from, to) = (on(0.3), on(2.9));
        let edges = arc_edges(from, &Arc { radii, rotation, large: false, sweep: true, to });
        assert_eq!(cubics(&edges).last().map(|c| c.to), Some(to));
        for p in samples(from, &edges) {
            assert!(off_ellipse(p, centre, radii, rotation).abs() < 3e-4, "{p:?}");
        }
    }

    #[test]
    fn a_zero_radius_is_a_line_and_a_zero_length_arc_is_nothing() {
        assert_eq!(arc_edges([0.0, 0.0], &circle_arc([5.0, 5.0], 0.0, false, true)), vec![Edge::Line([5.0, 5.0])]);
        assert_eq!(arc_edges([5.0, 5.0], &circle_arc([5.0, 5.0], 3.0, false, true)), vec![]);
    }

    #[test]
    fn a_wedge_runs_from_the_centre_round_the_rim_clockwise_and_back() {
        let share = 0.79 * TAU;
        let outline = Outline::lower(&wedge(0.0, share, 120.0));
        let edges = outline.edges();
        assert_eq!(edges[..2], [Edge::Move([0.0, 0.0]), Edge::Line(polar(0.0, 120.0))]);
        assert_eq!(edges.last(), Some(&Edge::Close));
        let rim = samples(polar(0.0, 120.0), &edges[2..edges.len() - 1]);
        assert!(rim.iter().all(|&p| off_ellipse(p, [0.0; 2], [120.0; 2], 0.0).abs() < 3e-4));
        let first = rim[1];
        assert!(first[0] > 0.0 && first[1] < 0.0, "clockwise from twelve heads right first: {first:?}");
        assert_eq!(rim.last().copied(), Some(polar(share, 120.0)));
    }

    #[test]
    fn a_ring_wedge_is_one_contour_out_along_the_rim_and_back_along_the_hole() {
        let (start, end) = (0.25 * TAU, 0.75 * TAU);
        let outline = Outline::lower(&ring_wedge(start, end, 100.0, 60.0));
        assert_eq!(outline.contours(), 1);
        let edges = outline.edges();
        let line = edges.iter().position(|e| matches!(e, Edge::Line(_))).expect("the cut from rim to hole");
        assert_eq!(edges[line], Edge::Line(polar(end, 60.0)));
        let outer = samples(polar(start, 100.0), &edges[1..line]);
        let inner = samples(polar(end, 60.0), &edges[line + 1..edges.len() - 1]);
        assert!(outer.iter().all(|&p| off_ellipse(p, [0.0; 2], [100.0; 2], 0.0).abs() < 3e-4));
        assert!(inner.iter().all(|&p| off_ellipse(p, [0.0; 2], [60.0; 2], 0.0).abs() < 3e-4));
        assert!(outer[outer.len() / 2][1] > 99.9, "the outer arc passes six o'clock");
        assert_eq!(inner.last().copied(), Some(polar(start, 60.0)));
    }

    #[test]
    fn a_fill_with_a_hole_is_refused_rather_than_painted_over() {
        let ring = |r: f64, sweep: bool| {
            let arc = |to| Segment::Arc(Arc { radii: [r, r], rotation: 0.0, large: true, sweep, to });
            [Segment::Move([0.0, -r]), arc([0.0, r]), arc([0.0, -r]), Segment::Close]
        };
        let donut: Vec<Segment> = ring(100.0, true).into_iter().chain(ring(60.0, false)).collect();
        let outline = Outline::lower(&donut);
        assert_eq!(outline.paint(Some(Color::BLACK), None), Err(Unsupported::CompoundFill));
        assert!(outline.paint(None, Some(Stroke { width: 1.0, color: Color::BLACK })).is_ok(), "strokes have no holes");
    }
}
