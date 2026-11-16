//! Lowers a Scene to Rune elements: one keyed element per drawing, in paint order.
//!
//! The lowering reads only the Scene's paint walk, and nothing in it depends on the diagram
//! type. A group becomes an element of its own only where it does something Rune does per
//! element: fade as a whole, lead somewhere, show a tooltip or let the pointer through. A turn is
//! applied to what the group holds: shapes are turned point by point, text by Rune.
//!
//! A [`Drawing`] is made once per layout, with every outline flattened and every run shaped and
//! placed. Painting it only looks its paints up, so a palette change shapes nothing.
//!
//! [`Drawing::new`] is also the one place that says a Scene cannot be drawn here, as an
//! [`Unsupported`]. The view then draws the source another way; it never draws part of a Scene.

use std::f32::consts::TAU;

use pele_core::scene::{
    AnchorKind, Draw, Geometry, Group, LineCap as SceneCap, LineJoin, LinkRef, Paint, PaintVisit, PathCommand,
    ResolvedPaints, Rgba, StrokeStyle,
};
use pele_core::{DiagramKind, Scene};
use rune::shape::LineCap;
use rune::{Color, Cursor, El, Shadow, Shape, Span, TextStyle, Tx, canvas, el};

use crate::Event;
use crate::fill::{Crossing, fill};
use crate::geometry::{Affine, Arc, Outline, Point, Polyline, Segment};
use crate::stroke::{Cap, Join, StrokeLook, Strokes, stroke};
use crate::text::{Anchor, BaselinePoint, Baselines, style as text_style};

/// The kinds whose Scenes are drawn here. A kind pele-core ports joins the list once its Scenes
/// have been drawn side by side with Pele's.
pub const NATIVE: [DiagramKind; 9] = [
    DiagramKind::Pie,
    DiagramKind::Flowchart,
    DiagramKind::Class,
    DiagramKind::State,
    DiagramKind::Er,
    DiagramKind::Requirement,
    DiagramKind::Usecase,
    DiagramKind::Swimlane,
    DiagramKind::Agentflow,
];

/// Why a Scene is not drawn here.
#[derive(Clone, Debug, PartialEq)]
pub enum Unsupported {
    /// The diagram type is not among [`NATIVE`].
    Kind(DiagramKind),
    /// A style declaration the Scene's walk does not read, as `property:value`: a colour only a
    /// page can resolve, a font family, a line through text.
    Style(String),
    /// A fill whose outline crosses itself or another contour of its path.
    CrossingFill,
    /// A picture the host would have to load.
    Image,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unsupported::Kind(kind) => write!(f, "{} diagrams are not drawn natively yet", kind.name()),
            Unsupported::Style(style) => write!(f, "the style `{style}` cannot be drawn natively"),
            Unsupported::CrossingFill => f.write_str("a fill whose outline crosses itself cannot be drawn natively"),
            Unsupported::Image => f.write_str("pictures are not drawn natively yet"),
        }
    }
}

/// What an app's handler hears from a drawing.
pub(crate) type Handler<S> = fn(&mut S, Event, &mut Tx);

/// How far a flattened curve may stray from the true one, in device pixels. Rune draws a
/// stroke one segment at a time, each antialiased on its own, so a curve cut finer than it
/// needs comes out bolder at every joint.
const FLATNESS: f64 = 0.25;

/// The least distance worth drawing, in device pixels: see [`stroke`].
const VISIBLE: f64 = 0.05;

/// Half the width of the sliver over a hole's slit, in device pixels: see [`fill`].
const SLIVER: f64 = 0.75;

/// The copies of a text that draw its outline, set round a circle of the outline's reach.
const HALO_COPIES: usize = 12;

/// A browser's own choice for `baseline-shift: sub` and `super`, as a share of the smaller size
/// the run is set in.
const SUB_SHIFT: f64 = 0.3;
const SUPER_SHIFT: f64 = -0.45;

/// A Scene as Rune elements to be, with its paints still to resolve.
pub struct Drawing {
    size: [f32; 2],
    items: Vec<Item>,
}

/// How much a drawing asks of Rune.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Census {
    /// Keyed elements: one per drawing, and one per group that is an element.
    pub elements: usize,
    /// Strokes and fills in the elements' shapes.
    pub shape_ops: usize,
    /// How deep elements nest under the drawing's root.
    pub deepest: usize,
}

enum Item {
    Shape(ShapeItem),
    Text(TextItem),
    Group(GroupItem),
}

/// An element's box in the drawing's points.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    at: [f32; 2],
    size: [f32; 2],
}

/// A box on device pixels that holds the points from `low` to `high`, so that elements either
/// side of a shared edge move together.
fn frame(low: Point, high: Point, scale: f64) -> Frame {
    let at = [(low[0] * scale).floor() / scale, (low[1] * scale).floor() / scale];
    let size = [(high[0] * scale).ceil() / scale - at[0], (high[1] * scale).ceil() / scale - at[1]];
    Frame { at: at.map(|c| c as f32), size: size.map(|c| c as f32) }
}

/// One SVG shape as what Rune draws, in a box of its own with its geometry relative to the box.
/// Its colours come when it is painted.
pub struct Figure {
    frame: Frame,
    shape: FigureShape,
}

enum FigureShape {
    /// Strokes and fills in a canvas: any outline.
    Coats { fill: Option<Coat>, stroke: Option<Coat> },
    /// A rectangle or circle that stands upright with a solid border, as Rune's own box: one
    /// primitive with exact edges, where a stroked outline is a segment per side and per step
    /// round each corner.
    Box(BoxShape),
}

/// A box whose frame is the outer edge of its border.
struct BoxShape {
    /// The corner radius of the outline, which runs down the middle of the border.
    radius: f32,
    filled: bool,
    border: Option<Border>,
}

struct Border {
    width: f32,
    /// The radius of the border's outer edge.
    radius: f32,
}

/// One paint of a figure: its fill or its stroke.
struct Coat {
    ops: Vec<Op>,
    /// Whether the ops overlap, so that a translucent colour must be drawn opaque and faded once.
    overlaps: bool,
}

struct ShapeItem {
    key: String,
    figure: Figure,
    fill: Paint,
    fill_opacity: f32,
    /// The stroke's paint and its `stroke-opacity`.
    stroke: Option<(Paint, f32)>,
    opacity: f32,
}

enum Op {
    Fill(Vec<[f32; 2]>),
    Round { points: Vec<[f32; 2]>, closed: bool, width: f32 },
    Butt { points: Vec<[f32; 2]>, width: f32 },
}

/// One run of text as what Rune draws, placed as SVG places it. Its colours come when it is
/// painted.
pub struct TextRun {
    content: String,
    style: TextStyle,
    /// The run's box before any turn: its advance by its line's height.
    frame: Frame,
    underline: bool,
    /// How far SVG's stroke on the text reaches past the glyphs.
    halo: Option<f32>,
    /// A turn about the run's anchor point on its baseline, in radians clockwise.
    turn: Option<Turn>,
}

struct TextItem {
    key: String,
    run: TextRun,
    fill: Paint,
    fill_opacity: f32,
    /// The paint of SVG's stroke on the text.
    halo: Option<Paint>,
    opacity: f32,
}

#[derive(Clone, Copy)]
struct Turn {
    radians: f32,
    pivot: [f32; 2],
}

/// A group that is an element: its children are placed inside it.
struct GroupItem {
    key: String,
    frame: Frame,
    children: Vec<Item>,
    opacity: f32,
    link: Option<LinkRef>,
    tooltip: Option<String>,
    inert: bool,
}

/// A group of the walk while its drawings are being lowered.
struct Open {
    /// The map from the group's coordinates to the drawing's.
    to_drawing: Affine,
    /// The group as the walk gave it, where it becomes an element.
    element: Option<Group>,
    children: Vec<Item>,
}

fn is_element(group: &Group) -> bool {
    group.opacity != 1.0 || group.link.is_some() || group.tooltip.is_some() || group.untouchable
}

/// The map from `group`'s coordinates to those outside it: its move, then its turn. The turn's
/// centre is outside the group, so a group that does both turns about the point it moves its
/// drawing to, as SVG's `translate(x,y) rotate(degrees)` does.
pub fn group_map(group: &Group) -> Affine {
    let moved = Affine::translate([group.translate.x, group.translate.y]);
    match group.rotate {
        Some(turn) => Affine::rotate_about(turn.degrees, [turn.centre.x, turn.centre.y]).then(&moved),
        None => moved,
    }
}

impl Drawing {
    /// Lowers `scene` in paint order, shaping its text with `baselines` at their device scale.
    pub fn new(
        kind: DiagramKind,
        scene: &Scene,
        baselines: &mut Baselines,
        scale: f32,
    ) -> Result<Drawing, Unsupported> {
        if !NATIVE.contains(&kind) {
            return Err(Unsupported::Kind(kind));
        }
        if let Some(style) = scene.unread_styles().into_iter().next() {
            return Err(Unsupported::Style(style));
        }
        let scale = f64::from(scale);
        let mut open = vec![Open { to_drawing: Affine::IDENTITY, element: None, children: Vec::new() }];
        for visit in scene.paint() {
            match visit {
                PaintVisit::Begin(group) => {
                    let outer = open.last().expect("the root stays open").to_drawing;
                    let to_drawing = outer.then(&group_map(&group));
                    let element = is_element(&group).then_some(group);
                    open.push(Open { to_drawing, element, children: Vec::new() });
                }
                PaintVisit::End => {
                    let closed = open.pop().expect("an end closes a group");
                    let outer = &mut open.last_mut().expect("the root stays open").children;
                    match closed.element {
                        None => outer.extend(closed.children),
                        Some(group) => outer.extend(GroupItem::around(group, closed.children, scale).map(Item::Group)),
                    }
                }
                PaintVisit::Draw(draw) => {
                    let group = open.last_mut().expect("the root stays open");
                    group.children.extend(lower_draw(draw, &group.to_drawing, baselines, scale)?);
                }
            }
        }
        let size = scene.size();
        let root = open.pop().expect("the root stays open");
        Ok(Drawing { size: [size.width as f32, size.height as f32], items: root.children })
    }

    /// The drawing as keyed elements in paint order, in `paints`' colours. A link tells
    /// `on_event` when it is activated.
    pub(crate) fn el<S: 'static>(&self, paints: &ResolvedPaints, on_event: Option<Handler<S>>) -> El {
        let children = self.items.iter().filter_map(|item| item.el(paints, [0.0; 2], on_event));
        // The drawing is one picture: nothing in it moves to the pixel grid on its own, so that
        // an edge still ends on the border it runs to.
        el().size(self.size[0], self.size[1]).pixel_snap(false).children(children)
    }

    pub fn census(&self) -> Census {
        fn count(items: &[Item], depth: usize, census: &mut Census) {
            for item in items {
                census.elements += 1;
                census.deepest = census.deepest.max(depth);
                match item {
                    Item::Shape(shape) => census.shape_ops += shape.figure.ops(),
                    Item::Text(_) => {}
                    Item::Group(group) => count(&group.children, depth + 1, census),
                }
            }
        }
        let mut census = Census::default();
        count(&self.items, 1, &mut census);
        census
    }
}

pub(crate) fn colour(rgba: Rgba) -> Color {
    let [r, g, b] = rgba.linear_srgb.map(|c| c as f32);
    Color { r, g, b, a: rgba.alpha as f32 }
}

impl Item {
    fn frame(&self) -> Frame {
        match self {
            Item::Shape(shape) => shape.figure.frame,
            Item::Text(text) => text.run.bounds(),
            Item::Group(group) => group.frame,
        }
    }

    /// The element, placed inside an element whose top left is at `origin`.
    fn el<S: 'static>(&self, paints: &ResolvedPaints, origin: [f32; 2], on_event: Option<Handler<S>>) -> Option<El> {
        match self {
            Item::Shape(shape) => shape.el(paints, origin),
            Item::Text(text) => text.el(paints, origin),
            Item::Group(group) => Some(group.el(paints, origin, on_event)),
        }
    }
}

impl Frame {
    fn placed(&self, element: El, origin: [f32; 2]) -> El {
        element.absolute(self.at[0] - origin[0], self.at[1] - origin[1]).size(self.size[0], self.size[1])
    }
}

impl Coat {
    /// The coat's ops in `colour`, added to `shape`.
    fn paint(&self, mut shape: Shape, colour: Color) -> Shape {
        for op in &self.ops {
            shape = match op {
                Op::Fill(points) => shape.fill(points.clone(), colour),
                Op::Round { points, closed, width } => {
                    shape.stroke_with(points.clone(), *closed, *width, LineCap::Round, colour)
                }
                Op::Butt { points, width } => shape.stroke_with(points.clone(), false, *width, LineCap::Butt, colour),
            };
        }
        shape
    }
}

impl Figure {
    /// `outline`, filled where `filled` and stroked in `look`, for a window at `scale` device
    /// pixels a point. Nothing, where that draws nothing.
    pub fn new(
        outline: &Outline,
        filled: bool,
        look: Option<&StrokeLook>,
        scale: f32,
    ) -> Result<Option<Figure>, Crossing> {
        let scale = f64::from(scale);
        let lines: Vec<Polyline> = outline.flatten(FLATNESS / scale);
        let fills = if filled { fill(&lines, SLIVER / scale)? } else { Default::default() };
        let strokes = look.map(|look| stroke(&lines, look, VISIBLE / scale)).unwrap_or_default();
        let width = look.map_or(0.0, |look| look.width);
        let Some([low, high]) = extent(&fills.polygons, &strokes, width / 2.0) else { return Ok(None) };
        // A point of room, so that an antialiased edge is not cut off at the box.
        let frame = frame([low[0] - 1.0, low[1] - 1.0], [high[0] + 1.0, high[1] + 1.0], scale);
        let local = |points: &[Point]| -> Vec<[f32; 2]> {
            let at = frame.at.map(f64::from);
            points.iter().map(|p| [(p[0] - at[0]) as f32, (p[1] - at[1]) as f32]).collect()
        };
        let fill = (!fills.polygons.is_empty()).then(|| Coat {
            ops: fills.polygons.iter().map(|polygon| Op::Fill(local(polygon))).collect(),
            overlaps: fills.overlaps,
        });
        let width = width as f32;
        let butt = strokes.butt.iter().map(|points| Op::Butt { points: local(points), width });
        let round =
            strokes.round.iter().map(|line| Op::Round { points: local(&line.points), closed: line.closed, width });
        let patches = strokes.patches.iter().map(|patch| Op::Fill(local(patch)));
        let stroke = (!strokes.is_empty())
            .then(|| Coat { ops: butt.chain(round).chain(patches).collect(), overlaps: strokes.overlaps() });
        Ok(Some(Figure { frame, shape: FigureShape::Coats { fill, stroke } }))
    }

    /// SVG's `<rect>` at `origin` with one corner radius, or with a radius of half its side its
    /// `<circle>`, as one box. Nothing, where the border is dashed: a box's border is solid.
    pub fn boxed(
        origin: Point,
        size: [f64; 2],
        radius: f64,
        filled: bool,
        look: Option<&StrokeLook>,
    ) -> Option<Figure> {
        let look = look.filter(|look| look.width > 0.0);
        if look.is_some_and(|look| !look.dash.is_empty()) {
            return None;
        }
        let radius = radius.max(0.0).min(size[0] / 2.0).min(size[1] / 2.0);
        let half = look.map_or(0.0, |look| look.width / 2.0);
        let frame = Frame {
            at: [(origin[0] - half) as f32, (origin[1] - half) as f32],
            size: [(size[0] + 2.0 * half) as f32, (size[1] + 2.0 * half) as f32],
        };
        // A mitred corner is square outside and in. Any other is round outside by half the
        // border's width more than the outline, and inside by as much less.
        let border = look.map(|look| Border {
            width: look.width as f32,
            radius: if radius > 0.0 || look.join == Join::Round { (radius + half) as f32 } else { 0.0 },
        });
        Some(Figure { frame, shape: FigureShape::Box(BoxShape { radius: radius as f32, filled, border }) })
    }

    /// The fill in `fill`, then the stroke in `stroke` over it, as SVG paints one element, in
    /// the figure's place in the drawing.
    pub fn el(&self, fill: Option<Color>, stroke: Option<Color>) -> Option<El> {
        self.painted(fill, stroke).map(|element| self.frame.placed(element, [0.0; 2]))
    }

    fn painted(&self, fill: Option<Color>, stroke: Option<Color>) -> Option<El> {
        let seen = |colour: Option<Color>| colour.filter(|colour| colour.a > 0.0);
        match &self.shape {
            FigureShape::Coats { fill: filled, stroke: stroked } => {
                let coats = [(filled, seen(fill)), (stroked, seen(stroke))];
                self.coats(coats.into_iter().filter_map(|(coat, colour)| Some((coat.as_ref()?, colour?))).collect())
            }
            FigureShape::Box(shape) => {
                let fill = seen(fill).filter(|_| shape.filled);
                let border = shape.border.as_ref().and_then(|border| Some((border, seen(stroke)?)));
                self.boxes(shape.radius, fill, border)
            }
        }
    }

    /// A translucent coat whose pieces overlap is drawn opaque in an element of its own, which
    /// fades it once.
    fn coats(&self, coats: Vec<(&Coat, Color)>) -> Option<El> {
        let coats: Vec<(&Coat, Color, f32)> = coats
            .into_iter()
            .map(|(coat, colour)| {
                let once = coat.overlaps && colour.a < 1.0;
                (coat, Color { a: if once { 1.0 } else { colour.a }, ..colour }, if once { colour.a } else { 1.0 })
            })
            .collect();
        if coats.is_empty() {
            return None;
        }
        if coats.iter().all(|&(_, _, fade)| fade == 1.0) {
            return Some(canvas(
                coats.iter().fold(Shape::new(), |shape, (coat, colour, _)| coat.paint(shape, *colour)),
            ));
        }
        let whole = Frame { at: [0.0; 2], ..self.frame };
        Some(el().children(coats.iter().map(|(coat, colour, fade)| {
            whole.placed(canvas(coat.paint(Shape::new(), *colour)), [0.0; 2]).layer_opacity(*fade)
        })))
    }

    /// The border is a ring just inside the box, as wide as the stroke. The fill is a box of
    /// its own that ends at the outline, as SVG's fill does: a fill out to the border's outer
    /// edge would tint that edge where it is antialiased, and show through the outer half of a
    /// translucent border.
    fn boxes(&self, radius: f32, fill: Option<Color>, border: Option<(&Border, Color)>) -> Option<El> {
        let ring = |border: &Border, colour: Color| {
            let ring = Shadow { spread: border.width, color: colour, inset: true, ..Shadow::default() };
            el().radius(border.radius).box_shadow(ring)
        };
        match (fill, border) {
            (None, None) => None,
            (Some(fill), None) => Some(el().radius(radius).fill(fill)),
            (None, Some((border, colour))) => Some(ring(border, colour)),
            (Some(fill), Some((border, colour))) => {
                let half = border.width / 2.0;
                let within = Frame { at: [half; 2], size: self.frame.size.map(|side| side - border.width) };
                let whole = Frame { at: [0.0; 2], ..self.frame };
                let fill = within.placed(el().radius(radius).fill(fill), [0.0; 2]);
                Some(el().child(fill).child(whole.placed(ring(border, colour), [0.0; 2])))
            }
        }
    }

    fn ops(&self) -> usize {
        match &self.shape {
            FigureShape::Coats { fill, stroke } => {
                [fill, stroke].into_iter().flatten().map(|coat| coat.ops.len()).sum()
            }
            FigureShape::Box(_) => 0,
        }
    }
}

impl ShapeItem {
    fn el(&self, paints: &ResolvedPaints, origin: [f32; 2]) -> Option<El> {
        let faded = |paint: Paint, opacity: f32| {
            let colour = colour(paints.get(paint)?);
            Some(Color { a: colour.a * opacity, ..colour })
        };
        let stroke = self.stroke.and_then(|(paint, opacity)| faded(paint, opacity));
        let element = self.figure.painted(faded(self.fill, self.fill_opacity), stroke)?;
        Some(self.figure.frame.placed(element.key(self.key.as_str()), origin).layer_opacity(self.opacity))
    }
}

impl TextRun {
    /// `content` in `style` with `anchor` at `at` on its baseline, turned `turn` radians
    /// clockwise about that point. `halo` is how far SVG's stroke on the text reaches past the
    /// glyphs: half the stroke's width.
    pub fn new(
        content: String,
        style: TextStyle,
        at: BaselinePoint,
        turn: f32,
        halo: Option<f32>,
        underline: bool,
        baselines: &mut Baselines,
    ) -> TextRun {
        let line = baselines.line(&content, style);
        let frame = Frame {
            at: [at.x - at.anchor.fraction() * line.advance, at.y - line.baseline],
            size: [line.advance, line.height],
        };
        let turn = (turn != 0.0).then_some(Turn { radians: turn, pivot: [at.x, at.y] });
        TextRun { content, style, frame, underline, halo, turn }
    }

    /// The box the run is drawn in: its own, turned, with room for its halo.
    fn bounds(&self) -> Frame {
        let reach = self.halo.unwrap_or(0.0);
        let (low, high) = (
            [self.frame.at[0] - reach, self.frame.at[1] - reach],
            [self.frame.at[0] + self.frame.size[0] + reach, self.frame.at[1] + self.frame.size[1] + reach],
        );
        let Some(turn) = self.turn else {
            return Frame { at: low, size: [high[0] - low[0], high[1] - low[1]] };
        };
        let (sin, cos) = turn.radians.sin_cos();
        let corners = [[low[0], low[1]], [high[0], low[1]], [high[0], high[1]], [low[0], high[1]]].map(|[x, y]| {
            let (dx, dy) = (x - turn.pivot[0], y - turn.pivot[1]);
            [turn.pivot[0] + cos * dx - sin * dy, turn.pivot[1] + sin * dx + cos * dy]
        });
        let least = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::MAX, f32::min);
        let most = |axis: usize| corners.iter().map(|c| c[axis]).fold(f32::MIN, f32::max);
        Frame { at: [least(0), least(1)], size: [most(0) - least(0), most(1) - least(1)] }
    }

    fn text(&self, colour: Color, at: [f32; 2]) -> El {
        let run = rune::text(self.content.as_str()).font(self.style).color(colour).absolute(at[0], at[1]);
        if self.underline { run.spans(vec![Span::new(0, self.content.len()).underline(colour)]) } else { run }
    }

    /// The run in `ink` on its baseline, over its halo in `halo`.
    pub fn el(&self, ink: Option<Color>, halo: Option<Color>) -> Option<El> {
        self.placed(ink, halo, [0.0; 2])
    }

    /// A plain run is one text element. A halo is copies of the run in the halo's colour round
    /// a circle, under the run, and a turn is Rune's, about the run's anchor point: both are an
    /// element around the run.
    fn placed(&self, ink: Option<Color>, halo: Option<Color>, origin: [f32; 2]) -> Option<El> {
        let halo = self.halo.and_then(|reach| Some((halo?, reach)));
        if ink.is_none() && halo.is_none() {
            return None;
        }
        let [x, y] = self.frame.at;
        if halo.is_none() && self.turn.is_none() {
            return Some(self.text(ink?, [x - origin[0], y - origin[1]]));
        }
        let reach = halo.map_or(0.0, |(_, reach)| reach);
        let around = Frame { at: [x - reach, y - reach], size: self.frame.size.map(|side| side + 2.0 * reach) };
        let copies = halo.into_iter().flat_map(|(colour, reach)| {
            (0..HALO_COPIES).map(move |i| {
                let (sin, cos) = (i as f32 * TAU / HALO_COPIES as f32).sin_cos();
                self.text(colour, [reach + cos * reach, reach + sin * reach])
            })
        });
        let element = around.placed(el().children(copies.chain(ink.map(|ink| self.text(ink, [reach; 2])))), origin);
        Some(match self.turn {
            Some(turn) => {
                let pivot =
                    [0, 1].map(|axis| (turn.pivot[axis] - around.at[axis]) / around.size[axis].max(f32::EPSILON));
                element.rotate(turn.radians).origin(pivot[0], pivot[1])
            }
            None => element,
        })
    }
}

impl TextItem {
    fn el(&self, paints: &ResolvedPaints, origin: [f32; 2]) -> Option<El> {
        let ink = paints.get(self.fill).map(colour).map(|c| Color { a: c.a * self.fill_opacity, ..c });
        let halo = self.halo.and_then(|paint| paints.get(paint)).map(colour);
        Some(self.run.placed(ink, halo, origin)?.key(self.key.as_str()).layer_opacity(self.opacity))
    }
}

impl GroupItem {
    /// The element of a group around what it holds, or nothing where it holds nothing.
    fn around(group: Group, children: Vec<Item>, scale: f64) -> Option<GroupItem> {
        let first = children.first()?;
        let key = match first {
            Item::Shape(shape) => &shape.key,
            Item::Text(text) => &text.key,
            Item::Group(inner) => &inner.key,
        };
        let key = format!("{key}~group");
        let frames = children.iter().map(Item::frame);
        let (low, high) = frames.fold(([f64::MAX; 2], [f64::MIN; 2]), |(low, high), frame| {
            let at = frame.at.map(f64::from);
            let end = [at[0] + f64::from(frame.size[0]), at[1] + f64::from(frame.size[1])];
            ([low[0].min(at[0]), low[1].min(at[1])], [high[0].max(end[0]), high[1].max(end[1])])
        });
        Some(GroupItem {
            key,
            frame: frame(low, high, scale),
            children,
            opacity: group.opacity as f32,
            link: group.link,
            tooltip: group.tooltip,
            inert: group.untouchable,
        })
    }

    fn el<S: 'static>(&self, paints: &ResolvedPaints, origin: [f32; 2], on_event: Option<Handler<S>>) -> El {
        let children = self.children.iter().filter_map(|child| child.el(paints, self.frame.at, on_event));
        let mut element =
            self.frame.placed(el().key(self.key.as_str()), origin).children(children).layer_opacity(self.opacity);
        if self.inert {
            element = element.inert();
        }
        if let Some(tooltip) = &self.tooltip {
            element = element.help(tooltip.as_str());
        }
        if let (Some(link), Some(on_event)) = (&self.link, on_event) {
            let activated = link.clone();
            element = element
                .on_press_tx(move |state: &mut S, tx: &mut Tx| on_event(state, Event::Activate(activated.clone()), tx))
                .role("link")
                .label(link.href.as_str())
                .cursor(Cursor::Pointer);
        }
        element
    }
}

fn point(p: pele_core::scene::Point) -> Point {
    [p.x, p.y]
}

fn path(commands: &[PathCommand]) -> Vec<Segment> {
    commands
        .iter()
        .map(|command| match *command {
            PathCommand::Move(p) => Segment::Move(point(p)),
            PathCommand::Line(p) => Segment::Line(point(p)),
            PathCommand::Quad { control, end } => Segment::Quad { control: point(control), to: point(end) },
            PathCommand::Cubic { first, second, end } => {
                Segment::Cubic(crate::geometry::Cubic { c1: point(first), c2: point(second), to: point(end) })
            }
            PathCommand::Arc { rx, ry, large, sweep, end } => {
                Segment::Arc(Arc { radii: [rx, ry], rotation: 0.0, large, sweep, to: point(end) })
            }
            PathCommand::Close => Segment::Close,
        })
        .collect()
}

/// SVG's `<ellipse>`, and with equal radii its `<circle>`: two half turns from the top.
fn ellipse([cx, cy]: Point, radii: [f64; 2]) -> Vec<Segment> {
    let half = |to: Point| Segment::Arc(Arc { radii, rotation: 0.0, large: true, sweep: true, to });
    let (top, bottom) = ([cx, cy - radii[1]], [cx, cy + radii[1]]);
    vec![Segment::Move(top), half(bottom), half(top), Segment::Close]
}

/// SVG's `<rect>` with one corner radius, which it clamps to half of each side.
fn rect([x, y]: Point, [w, h]: [f64; 2], radius: f64) -> Vec<Segment> {
    let r = radius.max(0.0).min(w / 2.0).min(h / 2.0);
    if r == 0.0 {
        return polygon(&[[x, y], [x + w, y], [x + w, y + h], [x, y + h]]);
    }
    let arc = |to: Point| Segment::Arc(Arc { radii: [r, r], rotation: 0.0, large: false, sweep: true, to });
    vec![
        Segment::Move([x + r, y]),
        Segment::Line([x + w - r, y]),
        arc([x + w, y + r]),
        Segment::Line([x + w, y + h - r]),
        arc([x + w - r, y + h]),
        Segment::Line([x + r, y + h]),
        arc([x, y + h - r]),
        Segment::Line([x, y + r]),
        arc([x + r, y]),
        Segment::Close,
    ]
}

fn polygon(points: &[Point]) -> Vec<Segment> {
    let mut program: Vec<Segment> = points.iter().map(|&p| Segment::Line(p)).collect();
    if let Some(first) = program.first_mut() {
        *first = Segment::Move(points[0]);
        program.push(Segment::Close);
    }
    program
}

fn look(stroke: &StrokeStyle) -> StrokeLook {
    StrokeLook {
        width: stroke.width,
        cap: if stroke.cap == SceneCap::Round { Cap::Round } else { Cap::Butt },
        join: if stroke.join == LineJoin::Round { Join::Round } else { Join::Miter },
        dash: stroke.dash.clone(),
    }
}

/// The least and greatest corner of what `fills` and `strokes` cover.
fn extent(fills: &[Vec<Point>], strokes: &Strokes, half: f64) -> Option<[Point; 2]> {
    let exact = fills.iter().chain(&strokes.patches).flatten().map(|&p| (p, 0.0));
    let lines = strokes.round.iter().map(|line| &line.points).chain(&strokes.butt).flatten().map(|&p| (p, half));
    exact.chain(lines).fold(None, |bounds, ([x, y], reach)| {
        let [low, high] = bounds.unwrap_or([[x, y]; 2]);
        Some([[low[0].min(x - reach), low[1].min(y - reach)], [high[0].max(x + reach), high[1].max(y + reach)]])
    })
}

fn lower_draw(
    draw: Draw,
    to_drawing: &Affine,
    baselines: &mut Baselines,
    scale: f64,
) -> Result<Option<Item>, Unsupported> {
    let key = draw.key.to_string();
    let style = draw.style;
    // The corner, size and corner radius of a rectangle or circle, which is a box where it
    // stands upright.
    let boxed = match &draw.geometry {
        Geometry::Rect { origin, size, radius } => Some((point(*origin), [size.width, size.height], *radius)),
        Geometry::Circle { centre, radius } => {
            Some(([centre.x - radius, centre.y - radius], [2.0 * radius; 2], *radius))
        }
        _ => None,
    };
    let program = match draw.geometry {
        Geometry::Path(commands) => path(&commands),
        Geometry::Rect { origin, size, radius } => rect(point(origin), [size.width, size.height], radius),
        Geometry::Circle { centre, radius } => ellipse(point(centre), [radius; 2]),
        Geometry::Ellipse { centre, radii } => ellipse(point(centre), [radii.width, radii.height]),
        Geometry::Polygon(points) => polygon(&points.into_iter().map(point).collect::<Vec<_>>()),
        Geometry::Image { .. } => return Err(Unsupported::Image),
        // The box of an icon the host draws by name. Pele's SVG leaves it empty for its host,
        // and no host fills it here yet.
        Geometry::Icon { .. } => return Ok(None),
        Geometry::Text(run) => {
            if run.text.is_empty() {
                return Ok(None);
            }
            let anchor = match run.anchor {
                AnchorKind::Start => Anchor::Start,
                AnchorKind::Middle => Anchor::Middle,
                AnchorKind::End => Anchor::End,
            };
            let shift = match run.script {
                pele_core::Script::Normal => 0.0,
                pele_core::Script::Sub => SUB_SHIFT * run.font.size,
                pele_core::Script::Super => SUPER_SHIFT * run.font.size,
            };
            let [x, y] = to_drawing.apply([run.baseline.x, run.baseline.y + shift]).map(|c| c as f32);
            let halo = style.stroke.as_ref().map(|stroke| (stroke.width / 2.0) as f32);
            let turn = to_drawing.turn() as f32;
            let at = BaselinePoint { x, y, anchor };
            let lowered = TextRun::new(run.text, text_style(&run.font), at, turn, halo, run.underline, baselines);
            return Ok(Some(Item::Text(TextItem {
                key,
                run: lowered,
                fill: style.fill,
                fill_opacity: style.fill_opacity as f32,
                halo: style.stroke.map(|stroke| stroke.paint),
                opacity: style.opacity as f32,
            })));
        }
    };
    let look = style.stroke.as_ref().map(look);
    let filled = style.fill != Paint::None;
    let upright = boxed.filter(|_| to_drawing.is_translation()).and_then(|(origin, size, radius)| {
        Figure::boxed(to_drawing.apply(origin), size, radius, filled, look.as_ref())
    });
    let figure = match upright {
        Some(figure) => Some(figure),
        None => {
            let outline = Outline::lower(&program).transformed(to_drawing);
            Figure::new(&outline, filled, look.as_ref(), scale as f32).map_err(|Crossing| Unsupported::CrossingFill)?
        }
    };
    Ok(figure.map(|figure| {
        Item::Shape(ShapeItem {
            key,
            figure,
            fill: style.fill,
            fill_opacity: style.fill_opacity as f32,
            stroke: style.stroke.map(|stroke| (stroke.paint, stroke.opacity as f32)),
            opacity: style.opacity as f32,
        })
    }))
}
