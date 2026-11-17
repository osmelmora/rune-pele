//! Lowers a Scene to Rune elements: one keyed element per drawing, in paint order.
//!
//! The lowering reads only the Scene's paint walk, and nothing in it depends on the diagram
//! type. A group becomes an element of its own only where it does something Rune does per
//! element: fade as a whole, lead somewhere, show a tooltip or let the pointer through. A turn is
//! applied to what the group holds: shapes are turned point by point, text by Rune.
//!
//! A [`Drawing`] is made once per layout and zoom, with every outline flattened and every run
//! shaped and placed. Painting it only looks its paints up, so a palette change shapes nothing.
//!
//! A zoom multiplies the drawing's geometry before anything is flattened, snapped or cut into
//! bands, so that edges land on the device pixels they are drawn on, and sets its text at the
//! zoomed size in its zoomed box. Rune's own `El::scale` would move pixels already snapped at
//! the unzoomed size between device pixels instead.
//!
//! A fill along a gradient is bands of one colour each, cut on device pixels: Rune fills a
//! polygon in one colour, and runs a gradient only over an element's own box.
//!
//! [`Drawing::new`] is also the one place that says a Scene cannot be drawn here, as an
//! [`Unsupported`]. The view then draws the source another way; it never draws part of a Scene.

use std::f32::consts::TAU;
use std::ops::Range;

use pele_core::scene::{
    AnchorKind, Draw, Geometry, Group, LineCap as SceneCap, LineJoin, LinkRef, Paint, PaintStyle, PaintVisit,
    PathCommand, ResolvedPaints, Rgba, Role, Size, StrokeStyle,
};
use pele_core::{DiagramKind, Scene};
use rune::color::{srgb_decode, srgb_encode};
use rune::shape::LineCap;
use rune::tok::OpticalSize;
use rune::{Color, Cursor, El, Key, KeyEvent, Mods, Shadow, Shape, Span, TextStyle, Tx, canvas, el};

use crate::Event;
use crate::bands::bands;
use crate::fill::{Crossing, fill};
use crate::geometry::{Affine, Arc, Outline, Point, Polyline, Segment};
use crate::icon::{Icon, IconPaint};
use crate::stroke::{Cap, Join, StrokeLook, Strokes, stroke};
use crate::text::{Anchor, BaselinePoint, Baselines, style as text_style};

/// The kinds whose Scenes are drawn here. A kind pele-core ports joins the list once its Scenes
/// have been drawn side by side with Pele's.
pub const NATIVE: [DiagramKind; 33] = [
    DiagramKind::Pie,
    DiagramKind::Flowchart,
    DiagramKind::Class,
    DiagramKind::State,
    DiagramKind::Er,
    DiagramKind::Requirement,
    DiagramKind::Usecase,
    DiagramKind::Swimlane,
    DiagramKind::Agentflow,
    DiagramKind::Architecture,
    DiagramKind::TreeView,
    DiagramKind::GitGraph,
    DiagramKind::Radar,
    DiagramKind::Timeline,
    DiagramKind::Xychart,
    DiagramKind::QuadrantChart,
    DiagramKind::Sequence,
    DiagramKind::Gantt,
    DiagramKind::Block,
    DiagramKind::C4,
    DiagramKind::Railroad,
    DiagramKind::Info,
    DiagramKind::Cynefin,
    DiagramKind::Wardley,
    DiagramKind::Venn,
    DiagramKind::Packet,
    DiagramKind::Eventmodeling,
    DiagramKind::Treemap,
    DiagramKind::Mindmap,
    DiagramKind::Journey,
    DiagramKind::Kanban,
    DiagramKind::Ishikawa,
    DiagramKind::Sankey,
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
    /// A fill along a gradient that does not run level in the drawing, or of a shape with a
    /// hole: neither is cut into the upright bands a gradient is drawn in.
    Gradient,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unsupported::Kind(kind) => write!(f, "{} diagrams are not drawn natively yet", kind.name()),
            Unsupported::Style(style) => write!(f, "the style `{style}` cannot be drawn natively"),
            Unsupported::CrossingFill => f.write_str("a fill whose outline crosses itself cannot be drawn natively"),
            Unsupported::Image => f.write_str("pictures are not drawn natively yet"),
            Unsupported::Gradient => {
                f.write_str("a gradient that is not level, or fills a shape with a hole, cannot be drawn natively")
            }
        }
    }
}

/// What an app's handler hears from a drawing.
pub(crate) type Handler<S> = fn(&mut S, Event, &mut Tx);

/// The app's icons: the icon it draws for a name, or none.
pub(crate) type Icons = fn(&str) -> Option<Icon>;

/// How far a flattened curve may stray from the true one, in device pixels. Rune draws a
/// stroke one segment at a time, each antialiased on its own, so a curve cut finer than it
/// needs comes out bolder at every joint.
const FLATNESS: f64 = 0.25;

/// How far the flattened edge of a fill with no stroke may stray from the true one, in device
/// pixels. Its edge is what shows, and two such fills that share an edge are each cut on their
/// own, so between them lies a gap or an overlap of up to twice this. A fill's corners cost
/// nothing where its sides meet, as a stroke's do.
const BARE_FLATNESS: f64 = 0.05;

/// The least distance worth drawing, in device pixels: see [`stroke`].
const VISIBLE: f64 = 0.05;

/// Half the width of the sliver over a hole's slit, in device pixels: see [`fill`].
const SLIVER: f64 = 0.75;

/// How wide a band of a fill with a curved edge and no stroke is, in device pixels: see
/// [`bands`]. Rune's fan of a band is whole while the band's own cut sees all of it, which a
/// band this narrow nearly always does.
const BAND: f64 = 16.0;

/// The most bands a fill is cut into along its gradient. Two stops are at most 255 levels
/// apart, so a band differs from the next by two levels at the most.
const MOST_BANDS: f64 = 128.0;

/// The least turn of a corner that Rune softens whole, in radians: 15 degrees. Beyond the
/// line of a side that turns away by this much, the soft edge it loses is under two pixels long.
const SHARP: f64 = 0.26;

/// How wide a box the pointer finds a fill in is, in points: see [`spots`].
const SPOT: f64 = 16.0;

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
    /// How many icon boxes the items hold.
    slots: usize,
    /// The diagram's name and description for assistive technology.
    title: Option<String>,
    description: Option<String>,
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
    Icon(IconSlot),
}

/// The box of an icon, which the app's icons draw in by name when it is painted. Where they have
/// no icon by its name, it shows Pele's built-in glyph for the name, or nothing.
struct IconSlot {
    key: String,
    name: String,
    /// The box, in the coordinates `to_drawing` maps to the drawing's.
    origin: pele_core::scene::Point,
    size: Size,
    to_drawing: Affine,
    zoom: f64,
    /// Device pixels a point.
    scale: f64,
    opacity: f32,
    /// Its place among the drawing's slots, which are numbered in paint order.
    index: usize,
    glyph: Option<Box<Item>>,
}

/// An app's icon as drawn in its slot.
struct DrawnIcon {
    /// The box round its coats, or `None` where it draws nothing.
    frame: Option<Frame>,
    /// Each coat's figure, its paint, and whether it is stroked rather than filled.
    coats: Vec<(Figure, IconPaint, bool)>,
}

/// What a drawing is painted with.
struct Brush<'a, S> {
    paints: &'a ResolvedPaints,
    on_event: Option<Handler<S>>,
    /// The app's icon drawn in each slot, or `None` where it has none by the slot's name. Empty
    /// where the app gives no icons.
    icons: Vec<Option<DrawnIcon>>,
}

impl<S> Brush<'_, S> {
    /// The app's icons drawn in `slots`.
    fn drawn(&self, slots: &Range<usize>) -> impl Iterator<Item = &DrawnIcon> {
        self.icons.get(slots.clone()).unwrap_or_default().iter().flatten()
    }
}

/// The least box that holds both.
fn union(a: Frame, b: Frame) -> Frame {
    let at = [a.at[0].min(b.at[0]), a.at[1].min(b.at[1])];
    let end = [0, 1].map(|axis| (a.at[axis] + a.size[axis]).max(b.at[axis] + b.size[axis]));
    Frame { at, size: [end[0] - at[0], end[1] - at[1]] }
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
    /// The boxes the pointer finds the figure in.
    spots: Vec<Frame>,
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

/// What a shape is filled with: one paint, or the paints a gradient starts and ends in.
#[derive(Clone, Copy)]
enum Fill {
    Flat(Paint),
    Along([Paint; 2]),
}

/// What a coat is painted in: one colour, or the colours a gradient starts and ends in.
#[derive(Clone, Copy)]
enum Ink {
    Flat(Color),
    Along([Color; 2]),
}

impl Ink {
    /// The colour `along` the way from a gradient's start to its end, as SVG steps between two
    /// stops: each sRGB channel and the alpha on its own.
    fn at(self, along: f32) -> Color {
        match self {
            Ink::Flat(colour) => colour,
            Ink::Along([from, to]) => {
                let step = |from: f32, to: f32| from + (to - from) * along;
                let channel = |from: f32, to: f32| srgb_decode(step(srgb_encode(from), srgb_encode(to)));
                Color {
                    r: channel(from.r, to.r),
                    g: channel(from.g, to.g),
                    b: channel(from.b, to.b),
                    a: step(from.a, to.a),
                }
            }
        }
    }

    fn seen(self) -> bool {
        match self {
            Ink::Flat(colour) => colour.a > 0.0,
            Ink::Along(ends) => ends.iter().any(|end| end.a > 0.0),
        }
    }
}

struct ShapeItem {
    key: String,
    figure: Figure,
    fill: Fill,
    fill_opacity: f32,
    /// The stroke's paint and its `stroke-opacity`.
    stroke: Option<(Paint, f32)>,
    opacity: f32,
}

enum Op {
    Fill(Vec<[f32; 2]>),
    /// A band of a fill along a gradient, and how far along the gradient it lies, from 0 to 1.
    Band {
        points: Vec<[f32; 2]>,
        along: f32,
    },
    Round {
        points: Vec<[f32; 2]>,
        closed: bool,
        width: f32,
    },
    Butt {
        points: Vec<[f32; 2]>,
        width: f32,
    },
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
    /// The box round what it draws without the app's icons: `None` where it holds only icon
    /// boxes with no glyph. The app's icons widen it when it is painted.
    frame: Option<Frame>,
    /// The icon boxes it holds, at any depth.
    slots: Range<usize>,
    children: Vec<Item>,
    /// Where the pointer finds a group that only shows a tooltip: the boxes of what it draws.
    spots: Vec<Frame>,
    opacity: f32,
    link: Option<NamedLink>,
    tooltip: Option<String>,
    inert: bool,
}

/// Where a link leads, and what assistive technology names it by: the text it shows, or its
/// address where it shows none.
struct NamedLink {
    to: LinkRef,
    name: String,
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
        Drawing::zoomed(kind, scene, baselines, scale, 1.0)
    }

    /// [`Drawing::new`] at `zoom` times the size the Scene is laid out at: its geometry, strokes
    /// and text, and the boxes the pointer finds its links and tooltips in.
    pub fn zoomed(
        kind: DiagramKind,
        scene: &Scene,
        baselines: &mut Baselines,
        scale: f32,
        zoom: f32,
    ) -> Result<Drawing, Unsupported> {
        if !NATIVE.contains(&kind) {
            return Err(Unsupported::Kind(kind));
        }
        if let Some(style) = scene.unread_styles().into_iter().next() {
            return Err(Unsupported::Style(style));
        }
        let (scale, zoom) = (f64::from(scale), f64::from(zoom));
        let mut open = vec![Open { to_drawing: Affine::scale(zoom), element: None, children: Vec::new() }];
        let mut slots = 0;
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
                    let mut item = lower_draw(draw, &group.to_drawing, zoom, baselines, scale)?;
                    if let Some(Item::Icon(slot)) = &mut item {
                        slot.index = slots;
                        slots += 1;
                    }
                    group.children.extend(item);
                }
            }
        }
        let size = scene.size();
        let root = open.pop().expect("the root stays open");
        Ok(Drawing {
            size: [(size.width * zoom) as f32, (size.height * zoom) as f32],
            items: root.children,
            slots,
            title: scene.accessible_title(),
            description: scene.accessible_description(),
        })
    }

    /// The drawing as keyed elements in paint order, in `paints`' colours, under a figure with
    /// the diagram's name and description. A link tells `on_event` when it is activated. Each
    /// icon box shows the icon `icons` gives for its name, where it gives one.
    pub(crate) fn el<S: 'static>(
        &self,
        paints: &ResolvedPaints,
        on_event: Option<Handler<S>>,
        icons: Option<Icons>,
    ) -> El {
        let mut drawn: Vec<Option<DrawnIcon>> = Vec::new();
        if let Some(icons) = icons {
            drawn.resize_with(self.slots, || None);
            for slot in slots(&self.items) {
                drawn[slot.index] = icons(&slot.name).map(|icon| slot.drawn(&icon));
            }
        }
        let brush = Brush { paints, on_event, icons: drawn };
        let children = self.items.iter().filter_map(|item| item.el(&brush, [0.0; 2]));
        // The drawing is one picture: nothing in it moves to the pixel grid on its own, so that
        // an edge still ends on the border it runs to.
        let mut figure = el().size(self.size[0], self.size[1]).pixel_snap(false).role("figure").children(children);
        if let Some(title) = &self.title {
            figure = figure.label(title.as_str());
        }
        if let Some(description) = &self.description {
            figure = figure.description(description.as_str());
        }
        figure
    }

    /// Only the drawing's text, every run in `ink` and without its outline, where the drawing
    /// places it: for a check of where its glyphs land.
    pub fn text_only(&self, ink: Color) -> El {
        fn runs(items: &[Item], ink: Color, into: &mut Vec<El>) {
            for item in items {
                match item {
                    Item::Text(text) => into.extend(text.run.placed(Some(ink), None, [0.0; 2])),
                    Item::Group(group) => runs(&group.children, ink, into),
                    Item::Shape(_) | Item::Icon(_) => {}
                }
            }
        }
        let mut texts = Vec::new();
        runs(&self.items, ink, &mut texts);
        el().size(self.size[0], self.size[1]).pixel_snap(false).children(texts)
    }

    /// The drawing's size in points.
    pub fn size(&self) -> [f32; 2] {
        self.size
    }

    /// What the drawing asks of Rune without the app's icons.
    pub fn census(&self) -> Census {
        fn count(items: &[Item], depth: usize, census: &mut Census) {
            for item in items {
                match item {
                    Item::Icon(slot) => {
                        if let Some(glyph) = &slot.glyph {
                            count(std::slice::from_ref(glyph), depth, census);
                        }
                        continue;
                    }
                    Item::Group(group) if group.frame.is_none() => continue,
                    _ => {}
                }
                census.elements += 1;
                census.deepest = census.deepest.max(depth);
                match item {
                    Item::Shape(shape) => census.shape_ops += shape.figure.ops(),
                    Item::Text(_) | Item::Icon(_) => {}
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
    /// The box round what the item draws without the app's icons, where it draws anything so.
    fn frame(&self) -> Option<Frame> {
        match self {
            Item::Shape(shape) => Some(shape.figure.frame),
            Item::Text(text) => Some(text.run.bounds()),
            Item::Group(group) => group.frame,
            Item::Icon(slot) => slot.glyph.as_ref()?.frame(),
        }
    }

    /// The boxes the pointer finds the item in without the app's icons.
    fn spots(&self) -> Vec<Frame> {
        match self {
            Item::Shape(shape) => shape.figure.spots.clone(),
            Item::Icon(slot) => slot.glyph.as_ref().map(|glyph| glyph.spots()).unwrap_or_default(),
            Item::Text(_) | Item::Group(_) => self.frame().into_iter().collect(),
        }
    }

    /// The icon boxes the item holds, at any depth.
    fn slots(&self) -> Range<usize> {
        match self {
            Item::Icon(slot) => slot.index..slot.index + 1,
            Item::Group(group) => group.slots.clone(),
            Item::Shape(_) | Item::Text(_) => 0..0,
        }
    }

    /// The element, placed inside an element whose top left is at `origin`.
    fn el<S: 'static>(&self, brush: &Brush<S>, origin: [f32; 2]) -> Option<El> {
        match self {
            Item::Shape(shape) => shape.el(brush.paints, origin),
            Item::Text(text) => text.el(brush.paints, origin),
            Item::Group(group) => group.el(brush, origin),
            Item::Icon(slot) => match brush.icons.get(slot.index) {
                Some(Some(icon)) => icon.el(brush.paints, origin, &slot.key, slot.opacity),
                _ => slot.glyph.as_ref()?.el(brush, origin),
            },
        }
    }
}

/// The icon boxes among `items`, at any depth, in paint order.
fn slots(items: &[Item]) -> Vec<&IconSlot> {
    let mut found = Vec::new();
    for item in items {
        match item {
            Item::Icon(slot) => found.push(slot),
            Item::Group(group) => found.extend(slots(&group.children)),
            Item::Shape(_) | Item::Text(_) => {}
        }
    }
    found
}

impl IconSlot {
    /// `icon` fitted to the box and lowered as every other shape is: zoomed, moved and turned
    /// with the box, and flattened for the device pixels it is drawn on. An icon is drawn whole
    /// or not at all, as a Scene is, so one with a fill whose contours cross draws nothing.
    fn drawn(&self, icon: &Icon) -> DrawnIcon {
        let mut coats = Vec::new();
        for coat in icon.placed(self.origin, self.size) {
            let look = coat.stroke.map(|width| StrokeLook {
                width: width * self.zoom,
                cap: Cap::Round,
                join: Join::Round,
                dash: Vec::new(),
            });
            let outline = Outline::lower(&path(&coat.commands)).transformed(&self.to_drawing);
            match Figure::new(&outline, look.is_none(), look.as_ref(), self.scale as f32) {
                Ok(Some(figure)) => coats.push((figure, coat.paint, look.is_some())),
                Ok(None) => {}
                Err(Crossing) => return DrawnIcon { frame: None, coats: Vec::new() },
            }
        }
        let frame = coats.iter().map(|(figure, ..)| figure.frame).reduce(union);
        DrawnIcon { frame, coats }
    }
}

impl DrawnIcon {
    /// The icon as one element keyed as its box, placed inside an element whose top left is at
    /// `origin`. `currentColor` is the diagram's foreground, which Pele's SVG sets as the colour
    /// its icon boxes inherit.
    fn el(&self, paints: &ResolvedPaints, origin: [f32; 2], key: &str, opacity: f32) -> Option<El> {
        let frame = self.frame?;
        let ink = |paint: IconPaint| match paint {
            IconPaint::Current => paints.get(Paint::Role(Role::Foreground)).map(colour),
            IconPaint::Fixed(colour) => Some(colour),
        };
        let coats = self.coats.iter().filter_map(|(figure, paint, stroked)| {
            let ink = ink(*paint);
            let element = if *stroked { figure.painted(None, ink) } else { figure.painted(ink.map(Ink::Flat), None) }?;
            Some(figure.frame.placed(element, frame.at))
        });
        Some(frame.placed(el().key(key).children(coats), origin).layer_opacity(opacity))
    }
}

impl Frame {
    fn placed(&self, element: El, origin: [f32; 2]) -> El {
        element.absolute(self.at[0] - origin[0], self.at[1] - origin[1]).size(self.size[0], self.size[1])
    }
}

impl Coat {
    /// The coat's ops in `ink`, added to `shape`.
    fn paint(&self, mut shape: Shape, ink: Ink) -> Shape {
        let colour = ink.at(0.0);
        for op in &self.ops {
            shape = match op {
                Op::Fill(points) => shape.fill(points.clone(), colour),
                Op::Band { points, along } => shape.fill(points.clone(), ink.at(*along)),
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
        let lines = flattened(outline, look, scale);
        let mut fills = if filled { fill(&lines, SLIVER / scale)? } else { Default::default() };
        // A fill with a hole is one polygon through a slit a hair wide, which is left whole.
        if look.is_none() && !fills.overlaps {
            fills.polygons = fills.polygons.into_iter().flat_map(|polygon| in_bands(polygon, scale)).collect();
        }
        Ok(Figure::of(&fills.polygons, None, fills.overlaps, &lines, look, scale))
    }

    /// `outline` filled along a level gradient that starts at the x `axis[0]` and ends at
    /// `axis[1]`, and stroked in `look`. The fill is upright bands, each as far along the
    /// gradient as its middle is: one to a device pixel, or wider so that at most 128 lie along
    /// the gradient.
    pub fn along(
        outline: &Outline,
        axis: [f64; 2],
        look: Option<&StrokeLook>,
        scale: f32,
    ) -> Result<Option<Figure>, Unsupported> {
        let scale = f64::from(scale);
        let lines = flattened(outline, look, scale);
        let fills = fill(&lines, SLIVER / scale).map_err(|Crossing| Unsupported::CrossingFill)?;
        if fills.overlaps {
            return Err(Unsupported::Gradient);
        }
        let [from, to] = axis;
        let step = ((to - from).abs() * scale / MOST_BANDS).ceil().clamp(1.0, BAND);
        let (mut pieces, mut places) = (Vec::new(), Vec::new());
        for polygon in &fills.polygons {
            for band in bands(polygon, &columns(polygon, step, scale)).ok_or(Unsupported::Gradient)? {
                places.push(((band.middle - from) / (to - from)).clamp(0.0, 1.0) as f32);
                pieces.push(band.outline);
            }
        }
        Ok(Figure::of(&pieces, Some(&places), false, &lines, look, scale))
    }

    /// The figure of a fill's `pieces`, each as far along a gradient as `places` says where
    /// there is one, under the stroke of `lines` in `look`.
    fn of(
        pieces: &[Vec<Point>],
        places: Option<&[f32]>,
        overlaps: bool,
        lines: &[Polyline],
        look: Option<&StrokeLook>,
        scale: f64,
    ) -> Option<Figure> {
        let strokes = look.map(|look| stroke(lines, look, VISIBLE / scale)).unwrap_or_default();
        let width = look.map_or(0.0, |look| look.width);
        let [low, high] = extent(pieces, &strokes, width / 2.0)?;
        // A point of room, so that an antialiased edge is not cut off at the box.
        let frame = frame([low[0] - 1.0, low[1] - 1.0], [high[0] + 1.0, high[1] + 1.0], scale);
        let local = |points: &[Point]| -> Vec<[f32; 2]> {
            let at = frame.at.map(f64::from);
            points.iter().map(|p| [(p[0] - at[0]) as f32, (p[1] - at[1]) as f32]).collect()
        };
        let piece = |(index, piece): (usize, &Vec<Point>)| match places {
            Some(places) => Op::Band { points: local(piece), along: places[index] },
            None => Op::Fill(local(piece)),
        };
        let fill = (!pieces.is_empty()).then(|| Coat { ops: pieces.iter().enumerate().map(piece).collect(), overlaps });
        let width = width as f32;
        let butt = strokes.butt.iter().map(|points| Op::Butt { points: local(points), width });
        let round =
            strokes.round.iter().map(|line| Op::Round { points: local(&line.points), closed: line.closed, width });
        let patches = strokes.patches.iter().map(|patch| Op::Fill(local(patch)));
        let stroke = (!strokes.is_empty())
            .then(|| Coat { ops: butt.chain(round).chain(patches).collect(), overlaps: strokes.overlaps() });
        let spots = if pieces.is_empty() { vec![frame] } else { spots(pieces, f64::from(width) / 2.0) };
        Some(Figure { frame, shape: FigureShape::Coats { fill, stroke }, spots })
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
        let shape = FigureShape::Box(BoxShape { radius: radius as f32, filled, border });
        Some(Figure { frame, shape, spots: vec![frame] })
    }

    /// The fill in `fill`, then the stroke in `stroke` over it, as SVG paints one element, in
    /// the figure's place in the drawing.
    pub fn el(&self, fill: Option<Color>, stroke: Option<Color>) -> Option<El> {
        self.painted(fill.map(Ink::Flat), stroke).map(|element| self.frame.placed(element, [0.0; 2]))
    }

    /// [`Figure::el`] for a figure filled along a gradient, from `ends[0]` to `ends[1]`.
    pub fn el_along(&self, ends: [Color; 2], stroke: Option<Color>) -> Option<El> {
        self.painted(Some(Ink::Along(ends)), stroke).map(|element| self.frame.placed(element, [0.0; 2]))
    }

    fn painted(&self, fill: Option<Ink>, stroke: Option<Color>) -> Option<El> {
        let seen = |ink: Option<Ink>| ink.filter(|ink| ink.seen());
        let (fill, stroke) = (seen(fill), seen(stroke.map(Ink::Flat)));
        match &self.shape {
            FigureShape::Coats { fill: filled, stroke: stroked } => {
                let coats = [(filled, fill), (stroked, stroke)];
                self.coats(coats.into_iter().filter_map(|(coat, ink)| Some((coat.as_ref()?, ink?))).collect())
            }
            FigureShape::Box(shape) => {
                let (fill, stroke) = (fill.map(|ink| ink.at(0.0)), stroke.map(|ink| ink.at(0.0)));
                let fill = fill.filter(|_| shape.filled);
                let border = shape.border.as_ref().and_then(|border| Some((border, stroke?)));
                self.boxes(shape.radius, fill, border)
            }
        }
    }

    /// A translucent coat whose pieces overlap is drawn opaque in an element of its own, which
    /// fades it once.
    fn coats(&self, coats: Vec<(&Coat, Ink)>) -> Option<El> {
        let coats: Vec<(&Coat, Ink, f32)> = coats
            .into_iter()
            .map(|(coat, ink)| {
                // The bands of a fill along a gradient lie side by side.
                let Ink::Flat(colour) = ink else { return (coat, ink, 1.0) };
                let once = coat.overlaps && colour.a < 1.0;
                let drawn = Color { a: if once { 1.0 } else { colour.a }, ..colour };
                (coat, Ink::Flat(drawn), if once { colour.a } else { 1.0 })
            })
            .collect();
        if coats.is_empty() {
            return None;
        }
        if coats.iter().all(|&(_, _, fade)| fade == 1.0) {
            return Some(canvas(coats.iter().fold(Shape::new(), |shape, (coat, ink, _)| coat.paint(shape, *ink))));
        }
        let whole = Frame { at: [0.0; 2], ..self.frame };
        Some(el().children(coats.iter().map(|(coat, ink, fade)| {
            whole.placed(canvas(coat.paint(Shape::new(), *ink)), [0.0; 2]).layer_opacity(*fade)
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
        let fill = match self.fill {
            Fill::Flat(paint) => faded(paint, self.fill_opacity).map(Ink::Flat),
            Fill::Along(ends) => {
                Some(Ink::Along(ends.map(|end| faded(end, self.fill_opacity).unwrap_or(Color::TRANSPARENT))))
            }
        };
        let element = self.figure.painted(fill, stroke)?;
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
        // Keyed by what it draws without the app's icons, where it draws anything so.
        let first = children.iter().find(|child| child.frame().is_some()).or(children.first())?;
        let key = match first {
            Item::Shape(shape) => &shape.key,
            Item::Text(text) => &text.key,
            Item::Group(inner) => &inner.key,
            Item::Icon(slot) => &slot.key,
        };
        let key = format!("{key}~group");
        let held = children.iter().map(Item::slots).filter(|slots| !slots.is_empty());
        let slots = held.reduce(|a, b| a.start.min(b.start)..a.end.max(b.end)).unwrap_or(0..0);
        let frames = children.iter().filter_map(Item::frame);
        let (low, high) = frames.fold(([f64::MAX; 2], [f64::MIN; 2]), |(low, high), frame| {
            let at = frame.at.map(f64::from);
            let end = [at[0] + f64::from(frame.size[0]), at[1] + f64::from(frame.size[1])];
            ([low[0].min(at[0]), low[1].min(at[1])], [high[0].max(end[0]), high[1].max(end[1])])
        });
        let spots = match (&group.tooltip, &group.link) {
            (Some(_), None) => children.iter().flat_map(Item::spots).collect(),
            _ => Vec::new(),
        };
        let link = group.link.map(|to| {
            let shown = shown_text(&children);
            NamedLink { name: if shown.is_empty() { to.href.clone() } else { shown }, to }
        });
        Some(GroupItem {
            key,
            frame: (low[0] <= high[0]).then(|| frame(low, high, scale)),
            slots,
            children,
            spots,
            opacity: group.opacity as f32,
            link,
            tooltip: group.tooltip,
            inert: group.untouchable,
        })
    }

    /// The group's element, as wide as what it draws and the app's icons in it: the pointer finds
    /// its link and its tooltip on an icon too. Nothing, where it draws nothing.
    fn el<S: 'static>(&self, brush: &Brush<S>, origin: [f32; 2]) -> Option<El> {
        let icons: Vec<Frame> = brush.drawn(&self.slots).filter_map(|icon| icon.frame).collect();
        let frame = self.frame.into_iter().chain(icons.iter().copied()).reduce(union)?;
        let children = self.children.iter().filter_map(|child| child.el(brush, frame.at));
        // A tooltip in several boxes is on an empty element over each, and the group's own box
        // shows none.
        let tooltip = self.tooltip.as_deref();
        let spots: Vec<Frame> = match self.link {
            None if tooltip.is_some() => self.spots.iter().chain(&icons).copied().collect(),
            _ => self.spots.clone(),
        };
        let several = spots.iter().filter(|_| spots.len() > 1);
        let several = several.filter_map(|spot| Some(spot.placed(el().help(tooltip?), frame.at)));
        let children: Vec<El> = children.chain(several).collect();
        let mut element =
            frame.placed(el().key(self.key.as_str()), origin).children(children).layer_opacity(self.opacity);
        if self.inert {
            element = element.inert();
        }
        if let Some(tooltip) = tooltip.filter(|_| spots.len() <= 1) {
            element = element.help(tooltip);
        }
        let on_event = brush.on_event;
        if let (Some(link), Some(on_event)) = (&self.link, on_event) {
            let (pressed, entered) = (link.to.clone(), link.to.clone());
            // Rune presses a focused element on Space only. Return is the element's to take.
            let enter = move |state: &mut S, key: &KeyEvent, tx: &mut Tx| {
                let follows = key.key == Key::Enter && key.mods == Mods::default();
                if follows && !tx.key_repeat() {
                    on_event(state, Event::Activate(entered.clone()), tx);
                }
                follows
            };
            element = element
                .on_press_tx(move |state: &mut S, tx: &mut Tx| on_event(state, Event::Activate(pressed.clone()), tx))
                .on_key(enter)
                .focusable(true)
                .role("link")
                .label(link.name.as_str())
                .cursor(Cursor::Pointer);
        }
        Some(element)
    }
}

/// The text `items` show, run after run, with one space between words.
fn shown_text(items: &[Item]) -> String {
    fn runs<'a>(items: &'a [Item], into: &mut Vec<&'a str>) {
        for item in items {
            match item {
                Item::Text(text) => into.push(&text.run.content),
                Item::Group(group) => runs(&group.children, into),
                Item::Shape(_) | Item::Icon(_) => {}
            }
        }
    }
    let mut shown = Vec::new();
    runs(items, &mut shown);
    shown.iter().flat_map(|run| run.split_whitespace()).collect::<Vec<_>>().join(" ")
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

/// `style` set `zoom` times as large, in the design of its own size: a font with optical sizes
/// would otherwise draw a smaller size wider and looser than the size its box was measured at.
fn zoomed(style: TextStyle, zoom: f64) -> TextStyle {
    TextStyle { size: style.size * zoom as f32, optical_size: OpticalSize::Fixed(style.size), ..style }
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

/// `stroke` as drawn at `zoom`: its width and dashes zoomed with what it strokes.
fn look(stroke: &StrokeStyle, zoom: f64) -> StrokeLook {
    StrokeLook {
        width: stroke.width * zoom,
        cap: if stroke.cap == SceneCap::Round { Cap::Round } else { Cap::Butt },
        join: if stroke.join == LineJoin::Round { Join::Round } else { Join::Miter },
        dash: stroke.dash.iter().map(|length| length * zoom).collect(),
    }
}

/// The boxes round a fill's `pieces`, one to each upright strip [`SPOT`] wide that a piece's
/// middle lies in, with `reach` of room for a stroke. Rune finds the pointer in an element's
/// box, and one box round a slanted band takes in whatever lies beside the band.
fn spots(pieces: &[Vec<Point>], reach: f64) -> Vec<Frame> {
    let mut strips: std::collections::BTreeMap<i64, [Point; 2]> = std::collections::BTreeMap::new();
    for piece in pieces {
        let ends = |axis: usize, pick: fn(f64, f64) -> f64, from: f64| piece.iter().map(|p| p[axis]).fold(from, pick);
        let low = [ends(0, f64::min, f64::MAX) - reach, ends(1, f64::min, f64::MAX) - reach];
        let high = [ends(0, f64::max, f64::MIN) + reach, ends(1, f64::max, f64::MIN) + reach];
        let strip = ((low[0] + high[0]) / 2.0 / SPOT).floor() as i64;
        let around = strips.entry(strip).or_insert([low, high]);
        *around = [
            [around[0][0].min(low[0]), around[0][1].min(low[1])],
            [around[1][0].max(high[0]), around[1][1].max(high[1])],
        ];
    }
    let boxed = |[low, high]: [Point; 2]| Frame {
        at: low.map(|c| c as f32),
        size: [(high[0] - low[0]) as f32, (high[1] - low[1]) as f32],
    };
    strips.into_values().map(boxed).collect()
}

/// `outline` as polylines for a window at `scale`: as fine as its edge shows.
fn flattened(outline: &Outline, look: Option<&StrokeLook>, scale: f64) -> Vec<Polyline> {
    let flatness = if look.is_some() { FLATNESS } else { BARE_FLATNESS };
    outline.flatten(flatness / scale)
}

/// Whether a corner of `polygon` turns by less than [`SHARP`]: a point of a flattened curve.
fn curved(polygon: &[Point]) -> bool {
    let n = polygon.len();
    (0..n).any(|i| {
        let (a, b, c) = (polygon[i], polygon[(i + 1) % n], polygon[(i + 2) % n]);
        let (to, on) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
        (to[0] * on[1] - to[1] * on[0]).atan2(to[0] * on[0] + to[1] * on[1]).abs() < SHARP
    })
}

/// The device pixel columns `step` pixels apart that stand inside `polygon`, as x in its own
/// units, or the one nearest its middle where it is narrower than that.
fn columns(polygon: &[Point], step: f64, scale: f64) -> Vec<f64> {
    let ends = |pick: fn(f64, f64) -> f64, from: f64| polygon.iter().map(|p| p[0] * scale).fold(from, pick);
    let (least, most) = (ends(f64::min, f64::MAX), ends(f64::max, f64::MIN));
    let first = (least / step).floor() + 1.0;
    let count = ((most / step).ceil() - first).max(0.0) as usize;
    let within: Vec<f64> = (0..count).map(|i| (first + i as f64) * step / scale).collect();
    let middle = ((least + most) / 2.0).round();
    if within.is_empty() && least < middle && middle < most { vec![middle / scale] } else { within }
}

/// A polygon with a curved edge as the bands Rune softens right, and any other as it is.
fn in_bands(polygon: Vec<Point>, scale: f64) -> Vec<Vec<Point>> {
    if !curved(&polygon) {
        return vec![polygon];
    }
    match bands(&polygon, &columns(&polygon, BAND, scale)) {
        Some(bands) => bands.into_iter().map(|band| band.outline).collect(),
        None => vec![polygon],
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

/// `draw` in the drawing's points, which `to_drawing` maps its coordinates to: `zoom` times the
/// Scene's, moved and turned by the groups around it.
fn lower_draw(
    draw: Draw,
    to_drawing: &Affine,
    zoom: f64,
    baselines: &mut Baselines,
    scale: f64,
) -> Result<Option<Item>, Unsupported> {
    // The box of an icon, which the app's icons draw in by name when it is painted. Pele's SVG
    // leaves it empty for its host, or draws a built-in glyph in it: the path Pele's SVG draws in
    // the box, faded with the box.
    if let Geometry::Icon { name, origin, size, glyph } = draw.geometry {
        let opacity = draw.style.opacity;
        let glyph = match glyph {
            Some(glyph) => {
                let (outline, stroke) = glyph.drawn_in(origin, size);
                let style =
                    PaintStyle { fill: Paint::None, fill_opacity: 1.0, stroke: Some(stroke), opacity, gradient: None };
                let draw = Draw { key: draw.key.clone(), geometry: Geometry::Path(outline), style, ..draw };
                lower_draw(draw, to_drawing, zoom, baselines, scale)?.map(Box::new)
            }
            None => None,
        };
        return Ok(Some(Item::Icon(IconSlot {
            key: draw.key.to_string(),
            name,
            origin,
            size,
            to_drawing: *to_drawing,
            zoom,
            scale,
            opacity: opacity as f32,
            index: 0,
            glyph,
        })));
    }
    if !draw.geometry.renders() {
        return Ok(None);
    }
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
        Geometry::Icon { .. } => unreachable!("an icon's box is a slot"),
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
            let halo = style.stroke.as_ref().map(|stroke| (stroke.width * zoom / 2.0) as f32);
            let turn = to_drawing.turn() as f32;
            let at = BaselinePoint { x, y, anchor };
            let lowered =
                TextRun::new(run.text, zoomed(text_style(&run.font), zoom), at, turn, halo, run.underline, baselines);
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
    let look = style.stroke.as_ref().map(|stroke| look(stroke, zoom));
    let filled = style.fill != Paint::None;
    // Where the gradient of a fill starts and ends in the drawing, and the paints it runs between.
    let graded = match style.gradient.filter(|_| filled) {
        Some(gradient) => {
            let [from, to] = [gradient.from, gradient.to].map(|end| to_drawing.apply(point(end)));
            if from[1] != to[1] {
                return Err(Unsupported::Gradient);
            }
            Some(([from[0], to[0]], gradient.stops))
        }
        None => None,
    };
    let (fill, axis) = match graded {
        // A gradient with no length is its last paint throughout.
        Some((axis, [_, last])) if axis[0] == axis[1] => (Fill::Flat(last), None),
        Some((axis, ends)) => (Fill::Along(ends), Some(axis)),
        None => (Fill::Flat(style.fill), None),
    };
    let upright = boxed.filter(|_| axis.is_none() && to_drawing.is_upright()).and_then(|(origin, size, radius)| {
        Figure::boxed(to_drawing.apply(origin), size.map(|side| side * zoom), radius * zoom, filled, look.as_ref())
    });
    let figure = match (upright, axis) {
        (Some(figure), _) => Some(figure),
        (None, Some(axis)) => {
            let outline = Outline::lower(&program).transformed(to_drawing);
            Figure::along(&outline, axis, look.as_ref(), scale as f32)?
        }
        (None, None) => {
            let outline = Outline::lower(&program).transformed(to_drawing);
            Figure::new(&outline, filled, look.as_ref(), scale as f32).map_err(|Crossing| Unsupported::CrossingFill)?
        }
    };
    Ok(figure.map(|figure| {
        Item::Shape(ShapeItem {
            key,
            figure,
            fill,
            fill_opacity: style.fill_opacity as f32,
            stroke: style.stroke.map(|stroke| (stroke.paint, stroke.opacity as f32)),
            opacity: style.opacity as f32,
        })
    }))
}
