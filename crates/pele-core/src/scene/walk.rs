//! The paint walk: the Scene in paint order, each drawing with the values it is drawn with.
//!
//! An SVG element inherits its paint, stroke, font size and anchor from the groups around it, and
//! a group's translation moves everything inside it. The walk resolves both, and reads each
//! node's style declarations where they say something it can read without a page. So an
//! interpreter reads a drawing's effective style and where its origin sits, and never Pele's
//! attributes.

use super::style::{Meaning, Radius, Style};
use super::{
    AnchorKind, Aspect, Document, EntityKey, EntityKind, EntityName, Facet, LineCap, LineJoin, Node, NodeContent,
    NodeId, Number, Paint, PartId, PathProgram, PathStep, PlacedText, Point, Properties, Role, Scene, Size,
    TextEmission, TextWeight, Transform,
};
use crate::measure::{Emphasis, Font, FontFamily, FontWeight, Label, SMALLER, Script};
use std::collections::HashMap;

/// What one entity draws in one role, wherever in paint order that is. A pie section's slice
/// and its legend row are two parts of one entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PartKey {
    pub entity: EntityKey,
    pub facet: Facet,
}

/// One drawing of a part. A part's text can be several runs, each its own fragment.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FragmentKey {
    pub part: PartKey,
    pub detail: FragmentDetail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FragmentDetail {
    /// A shape, which is drawn whole. Of a part that draws several, the first.
    Whole,
    /// A further shape of a part that draws several, counted from 1 in paint order.
    Piece(u32),
    /// One run of a label: its line, and its place on the line.
    TextRun { line: u32, run: u32 },
    /// One run of a further label of a part that draws several, such as the cells of a table:
    /// the label, counted from 1 in paint order, then the run's line and its place on the line.
    LaterTextRun { label: u32, line: u32, run: u32 },
}

/// One step of [`Scene::paint`].
#[derive(Clone, Debug, PartialEq)]
pub enum PaintVisit {
    /// A group opens. Everything up to its `End` is drawn inside it.
    Begin(Group),
    Draw(Draw),
    End,
}

/// A group of drawings that share paint order and what they inherit.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The part the group is, where it is one.
    pub part: Option<PartKey>,
    /// The group's own translation. Each [`Draw`] already carries the sum of its groups'.
    pub translate: Point,
    /// A turn the group's whole drawing takes. Its [`Draw`]s and text runs are placed as before
    /// the turn, so an interpreter turns them itself.
    pub rotate: Option<Rotation>,
    /// How opaque the group is as a whole, composited once.
    pub opacity: f64,
    /// Where the group leads when it is activated.
    pub link: Option<LinkRef>,
    /// The text shown while the pointer rests on anything the group draws.
    pub tooltip: Option<String>,
    /// Whether the pointer passes through the group to what is under it.
    pub untouchable: bool,
}

/// A turn by `degrees`, clockwise as SVG's are, about `centre`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub degrees: f64,
    pub centre: Point,
}

/// Where a link leads.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkRef {
    /// An address, or for a link inside the host the name of the note it leads to.
    pub href: String,
    /// Whether the host follows the link itself, to one of its own notes.
    pub internal: bool,
    /// The browsing context a source asks for, such as `_blank`.
    pub target: Option<&'static str>,
}

/// One drawing, in the diagram's coordinates once `origin` is added to its geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub key: FragmentKey,
    /// Where the geometry's `(0, 0)` lies in the diagram: the translations of its groups.
    pub origin: Point,
    pub geometry: Geometry,
    pub style: PaintStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Path(Vec<PathCommand>),
    Rect {
        origin: Point,
        size: Size,
        radius: f64,
    },
    Circle {
        centre: Point,
        radius: f64,
    },
    Ellipse {
        centre: Point,
        radii: Size,
    },
    /// A closed outline through the points.
    Polygon(Vec<Point>),
    Text(BaselineRun),
    /// A picture the host loads from an address its policy has passed.
    Image {
        href: String,
        origin: Point,
        size: Size,
        fit: ImageFit,
    },
    /// The box of an icon the host draws by name, in the colour of the text around it.
    Icon {
        name: String,
        origin: Point,
        size: Size,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFit {
    /// Stretched to the box.
    Stretch,
    /// Whole and centred, at its own proportions.
    Meet,
}

/// A path command with absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    Move(Point),
    Line(Point),
    Quad {
        control: Point,
        end: Point,
    },
    Cubic {
        first: Point,
        second: Point,
        end: Point,
    },
    /// SVG's endpoint arc with no rotation.
    Arc {
        rx: f64,
        ry: f64,
        large: bool,
        sweep: bool,
        end: Point,
    },
    Close,
}

/// The paint a drawing has after inheritance.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintStyle {
    pub fill: Paint,
    /// How opaque the fill is, apart from its colour's own alpha.
    pub fill_opacity: f64,
    pub stroke: Option<StrokeStyle>,
    /// How opaque the drawing is as a whole: its fill and stroke composited, then faded.
    pub opacity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrokeStyle {
    pub paint: Paint,
    pub width: f64,
    pub join: LineJoin,
    pub cap: LineCap,
    /// Dashes and the gaps between them, in turn. Empty for a solid line.
    pub dash: Vec<f64>,
    pub opacity: f64,
}

/// One run of text, placed as SVG places it: `anchor` says which point of its `advance` sits at
/// `baseline.x`, and `baseline.y` is its alphabetic baseline. The text is already broken into
/// lines; an interpreter never wraps it again.
#[derive(Clone, Debug, PartialEq)]
pub struct BaselineRun {
    pub text: String,
    pub baseline: Point,
    pub anchor: AnchorKind,
    /// The run's width as layout measured it.
    pub advance: f64,
    pub font: Font,
    /// A subscript or superscript is set in a smaller `font`; how far it shifts is the
    /// interpreter's choice, as it is a browser's.
    pub script: Script,
    pub underline: bool,
}

/// Pele's default for `--pele-radius`, which a shape with the theme's corner radius is drawn
/// with until an interpreter brings its own.
const THEME_RADIUS: f64 = 4.0;

/// What a node inherits from the groups around it, starting from what the root element sets.
#[derive(Clone)]
struct Inherited {
    origin: Point,
    fill: Paint,
    fill_opacity: f64,
    stroke: Paint,
    stroke_width: f64,
    stroke_opacity: f64,
    dash: Vec<f64>,
    cap: LineCap,
    join: LineJoin,
    font_size: f64,
    anchor: AnchorKind,
    weight: Option<TextWeight>,
    mono: bool,
    /// What the style declarations around a text say of it, which win over its attributes.
    declared: Declared,
}

/// What style declarations say of the text they apply to, each where one says so.
#[derive(Clone, Copy, Default)]
struct Declared {
    bold: Option<bool>,
    italic: Option<bool>,
    mono: Option<bool>,
    underline: Option<bool>,
}

/// CSS's `font-weight` as Pele measures it: bold from 600 up.
fn declared_bold(value: &str) -> Option<bool> {
    match value {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        _ => value.parse::<f64>().ok().filter(|weight| (1.0..=1000.0).contains(weight)).map(|weight| weight >= 600.0),
    }
}

fn declared_italic(value: &str) -> Option<bool> {
    match value {
        "italic" | "oblique" => Some(true),
        "normal" => Some(false),
        _ => None,
    }
}

/// Only the generic family is one every interpreter has.
fn declared_mono(value: &str) -> Option<bool> {
    (value == "monospace").then_some(true)
}

fn declared_underline(value: &str) -> Option<bool> {
    match value {
        "underline" => Some(true),
        "none" => Some(false),
        _ => None,
    }
}

fn declared_cap(value: &str) -> Option<LineCap> {
    match value {
        "butt" => Some(LineCap::Butt),
        "round" => Some(LineCap::Round),
        _ => None,
    }
}

fn declared_join(value: &str) -> Option<LineJoin> {
    match value {
        "miter" => Some(LineJoin::Miter),
        "round" => Some(LineJoin::Round),
        _ => None,
    }
}

/// A declaration's value as CSS compares keywords: trimmed and in lowercase.
fn keyword(written: &str) -> String {
    written.trim().to_ascii_lowercase()
}

/// Whether the walk draws what one declaration says. `meaning` is what the node's style means
/// where a builder read it; the rest is read here.
fn reads(style: &Style, meaning: &Meaning, property: &str, written: &str) -> bool {
    let value = keyword(written);
    let nothing = matches!(value.as_str(), "0" | "0px" | "normal");
    match property {
        "fill" => meaning.fill.is_some(),
        "stroke" => meaning.stroke.is_some(),
        "stroke-width" => meaning.stroke_width.is_some(),
        "opacity" => meaning.opacity.is_some(),
        "fill-opacity" => meaning.fill_opacity.is_some(),
        "stroke-opacity" => meaning.stroke_opacity.is_some(),
        "stroke-dasharray" => meaning.dash.is_some(),
        "rx" => meaning.radius.is_some(),
        "font-size" => meaning.font_size.is_some(),
        "font-weight" => declared_bold(&value).is_some(),
        "font-style" => declared_italic(&value).is_some(),
        "font-family" => declared_mono(&value).is_some(),
        "text-decoration" => declared_underline(&value).is_some(),
        "stroke-linecap" => declared_cap(&value).is_some(),
        "stroke-linejoin" => declared_join(&value).is_some(),
        "stroke-dashoffset" | "letter-spacing" | "word-spacing" => nothing,
        // A corner is drawn with one radius.
        "ry" => style.declarations.iter().any(|other| other.property == "rx" && keyword(&other.written) == value),
        _ => false,
    }
}

impl Inherited {
    /// The root `<svg>`: `fill="var(--_fg)"` and the font size of its `font` style.
    fn root(document: &Document) -> Self {
        Inherited {
            origin: Point { x: 0.0, y: 0.0 },
            fill: Paint::Role(Role::Foreground),
            fill_opacity: 1.0,
            stroke: Paint::None,
            stroke_width: 1.0,
            stroke_opacity: 1.0,
            dash: Vec::new(),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            font_size: document.font_size,
            anchor: AnchorKind::Start,
            weight: None,
            mono: false,
            declared: Declared::default(),
        }
    }

    /// A style declaration wins over the attribute of the same node, as it does in CSS.
    fn within(mut self, properties: &Properties, styled: &Meaning) -> Self {
        if let Some(Transform::Translate(x, y)) = properties.transform {
            self.origin = Point { x: self.origin.x + x.effective(), y: self.origin.y + y.effective() };
        }
        self.fill = styled.fill.or(properties.fill).unwrap_or(self.fill);
        self.fill_opacity = styled
            .fill_opacity
            .or(properties.fill_opacity.map(|opacity| opacity.effective()))
            .unwrap_or(self.fill_opacity);
        self.stroke = styled.stroke.or(properties.stroke).unwrap_or(self.stroke);
        self.stroke_width =
            styled.stroke_width.or(properties.stroke_width.map(|width| width.effective())).unwrap_or(self.stroke_width);
        self.stroke_opacity = styled.stroke_opacity.unwrap_or(self.stroke_opacity);
        if let Some(dash) = &styled.dash {
            self.dash = dash.clone();
        } else if !properties.dash.is_empty() {
            self.dash = properties.dash.iter().map(Number::effective).collect();
        }
        self.cap = properties.cap.unwrap_or(self.cap);
        self.join = properties.join.unwrap_or(self.join);
        for declaration in properties.style.iter().flat_map(|style| &style.declarations) {
            let value = keyword(&declaration.written);
            match declaration.property.as_str() {
                "stroke-linecap" => self.cap = declared_cap(&value).unwrap_or(self.cap),
                "stroke-linejoin" => self.join = declared_join(&value).unwrap_or(self.join),
                "font-weight" => self.declared.bold = declared_bold(&value).or(self.declared.bold),
                "font-style" => self.declared.italic = declared_italic(&value).or(self.declared.italic),
                "font-family" => self.declared.mono = declared_mono(&value).or(self.declared.mono),
                "text-decoration" => self.declared.underline = declared_underline(&value).or(self.declared.underline),
                _ => {}
            }
        }
        self.font_size =
            styled.font_size.or(properties.font_size.map(|size| size.effective())).unwrap_or(self.font_size);
        self.anchor = properties.anchor.unwrap_or(self.anchor);
        self.weight = properties.weight.or(self.weight);
        self.mono |= properties.mono;
        self
    }

    fn style(&self, opacity: f64) -> PaintStyle {
        let stroke = (self.stroke != Paint::None).then(|| StrokeStyle {
            paint: self.stroke,
            width: self.stroke_width,
            join: self.join,
            cap: self.cap,
            dash: self.dash.clone(),
            opacity: self.stroke_opacity,
        });
        PaintStyle { fill: self.fill, fill_opacity: self.fill_opacity, stroke, opacity }
    }

    /// The font of a run with emphasis `style`. `own` is the part of that emphasis the run
    /// carries itself, which a declaration around it does not change; the rest is written on
    /// the text element, where a declaration wins over it.
    fn font(&self, style: Emphasis, own: Emphasis) -> Font {
        let declared = self.declared;
        let bold = own.bold || declared.bold.unwrap_or(style.bold || self.weight.is_some());
        let mono = own.mono || declared.mono.unwrap_or(style.mono || self.mono);
        Font {
            family: if mono { FontFamily::Mono } else { FontFamily::Sans },
            size: if style.script == Script::Normal { self.font_size } else { self.font_size / SMALLER },
            weight: if bold { FontWeight::Bold } else { FontWeight::Normal },
            italic: own.italic || declared.italic.unwrap_or(style.italic),
        }
    }

    fn underline(&self, style: Emphasis, own: Emphasis) -> bool {
        own.underline || self.declared.underline.unwrap_or(style.underline)
    }
}

/// How far left of its anchor point a run of width `advance` starts.
fn lead(anchor: AnchorKind, advance: f64) -> f64 {
    match anchor {
        AnchorKind::Start => 0.0,
        AnchorKind::Middle => advance / 2.0,
        AnchorKind::End => advance,
    }
}

/// Text as it is drawn: without the characters XML cannot hold, which Pele drops.
pub(super) fn drawable(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied()).flatten().filter(|&c| !super::xml_drops(c)).collect()
}

fn at(pair: &[Number; 2]) -> Point {
    Point { x: pair[0].effective(), y: pair[1].effective() }
}

impl PathProgram {
    /// The path with every command absolute, from the numbers that are written. A relative
    /// command and a mirrored handle are worked out from what is written before them.
    pub(crate) fn commands(&self) -> Vec<PathCommand> {
        let mut out = Vec::with_capacity(self.steps.len());
        let mut here = Point { x: 0.0, y: 0.0 };
        let mut start = here;
        // The handle a smooth curve mirrors: the last curve's, or the point itself.
        let mut handle = here;
        let by = |from: Point, pair: &[Number; 2]| Point {
            x: from.x + pair[0].effective(),
            y: from.y + pair[1].effective(),
        };
        for step in &self.steps {
            let mut mirrored = None;
            let command = match step {
                PathStep::Move(to) => {
                    start = at(to);
                    PathCommand::Move(start)
                }
                PathStep::Line(to) => PathCommand::Line(at(to)),
                PathStep::Horizontal(x) => PathCommand::Line(Point { x: x.effective(), y: here.y }),
                PathStep::Vertical(y) => PathCommand::Line(Point { x: here.x, y: y.effective() }),
                PathStep::Cubic([first, second, end]) => {
                    PathCommand::Cubic { first: at(first), second: at(second), end: at(end) }
                }
                PathStep::Quad([control, end]) => {
                    mirrored = Some(at(control));
                    PathCommand::Quad { control: at(control), end: at(end) }
                }
                PathStep::Arc { radii, large, sweep, end } => PathCommand::Arc {
                    rx: radii[0].effective(),
                    ry: radii[1].effective(),
                    large: *large,
                    sweep: *sweep,
                    end: at(end),
                },
                PathStep::Close => PathCommand::Close,
                PathStep::LineBy(to) => PathCommand::Line(by(here, to)),
                PathStep::HorizontalBy(x) => PathCommand::Line(Point { x: here.x + x.effective(), y: here.y }),
                PathStep::QuadBy([control, end]) => {
                    mirrored = Some(by(here, control));
                    PathCommand::Quad { control: by(here, control), end: by(here, end) }
                }
                PathStep::SmoothQuadBy(end) => {
                    let control = Point { x: 2.0 * here.x - handle.x, y: 2.0 * here.y - handle.y };
                    mirrored = Some(control);
                    PathCommand::Quad { control, end: by(here, end) }
                }
                PathStep::ArcBy { radii, large, sweep, end } => PathCommand::Arc {
                    rx: radii[0].effective(),
                    ry: radii[1].effective(),
                    large: *large,
                    sweep: *sweep,
                    end: by(here, end),
                },
                PathStep::VerticalBy(y) => PathCommand::Line(Point { x: here.x, y: here.y + y.effective() }),
                PathStep::CloseBy => PathCommand::Close,
            };
            here = match command {
                PathCommand::Move(to) | PathCommand::Line(to) => to,
                PathCommand::Quad { end, .. } | PathCommand::Cubic { end, .. } | PathCommand::Arc { end, .. } => end,
                PathCommand::Close => start,
            };
            handle = mirrored.unwrap_or(here);
            out.push(command);
        }
        out
    }
}

/// How many shapes and how many labels each part has drawn so far, so that each drawing of a
/// part has a key of its own. A part's later labels, such as a class's members or a table's
/// cells, are counted by label rather than by line, so a label that wraps differently leaves the
/// keys of the labels after it alone.
#[derive(Default)]
struct Pieces {
    shapes: HashMap<(PartId, Facet), u32>,
    labels: HashMap<(PartId, Facet), u32>,
}

impl Scene {
    /// Every drawing in paint order, inside the groups that hold it.
    pub fn paint(&self) -> impl Iterator<Item = PaintVisit> + '_ {
        let mut out = Vec::new();
        let mut pieces = Pieces::default();
        let root = Inherited::root(&self.document);
        for &id in &self.roots {
            self.visit(id, &root, &mut pieces, &mut out);
        }
        out.into_iter()
    }

    /// The style declarations the walk does not draw, each as `property:value`: a colour only a
    /// page can read, a font family, a line through text. Where there are any, an interpreter
    /// that reads only the walk draws something other than the SVG shows.
    pub fn unread_styles(&self) -> Vec<String> {
        let mut unread = Vec::new();
        for node in &self.arena {
            let Some(style) = &node.properties.style else { continue };
            for declaration in &style.declarations {
                if !reads(style, &node.styled, &declaration.property, &declaration.written) {
                    unread.push(format!("{}:{}", declaration.property, declaration.written));
                }
            }
        }
        unread
    }

    fn part_key(&self, node: &Node) -> Option<PartKey> {
        let part = node.part?;
        let facet = node.facet.expect("a part of an entity has a facet");
        Some(PartKey { entity: self.entity(part).key.clone(), facet })
    }

    fn visit(&self, id: NodeId, outer: &Inherited, pieces: &mut Pieces, out: &mut Vec<PaintVisit>) {
        let node = self.node(id);
        let inherited = outer.clone().within(&node.properties, &node.styled);
        let opacity = node.styled.opacity.or(node.properties.opacity.map(|opacity| opacity.effective())).unwrap_or(1.0);
        let mut shape = |out: &mut Vec<PaintVisit>, geometry: Geometry| {
            let part = self.part_key(node).expect("every drawing is a part of an entity");
            let drawn = pieces.shapes.entry((node.part.expect("a part"), part.facet)).or_insert(0);
            let detail = if *drawn == 0 { FragmentDetail::Whole } else { FragmentDetail::Piece(*drawn) };
            *drawn += 1;
            let key = FragmentKey { part, detail };
            out.push(PaintVisit::Draw(Draw {
                key,
                origin: inherited.origin,
                geometry,
                style: inherited.style(opacity),
            }));
        };
        let group = |out: &mut Vec<PaintVisit>, link: Option<LinkRef>, children: &[NodeId]| {
            let (translate, rotate) = match node.properties.transform {
                Some(Transform::Translate(x, y)) => (Point { x: x.effective(), y: y.effective() }, None),
                Some(Transform::Rotate { degrees, centre }) => {
                    let centre = Point { x: centre[0].effective(), y: centre[1].effective() };
                    (Point { x: 0.0, y: 0.0 }, Some(Rotation { degrees: degrees.effective(), centre }))
                }
                None => (Point { x: 0.0, y: 0.0 }, None),
            };
            let untouchable = node.properties.untouchable;
            let part = self.part_key(node);
            // Pele writes a tooltip as the first thing in the group it is for.
            let tooltip = children.iter().find_map(|&child| {
                let child = self.node(child);
                let entity = matches!(child.content, NodeContent::Tooltip).then_some(child.part?)?;
                Some(drawable(&crate::input::decode_entities(self.entity(entity).tooltip.as_ref()?)))
            });
            out.push(PaintVisit::Begin(Group { part, translate, rotate, opacity, link, tooltip, untouchable }));
        };
        match &node.content {
            NodeContent::Group(children) => {
                group(out, None, children);
                for &child in children {
                    self.visit(child, &inherited, pieces, out);
                }
                out.push(PaintVisit::End);
            }
            NodeContent::Link { link, children } => {
                let href = link.note.as_ref().or(link.href.as_ref()).expect("a link leads somewhere");
                let link = LinkRef { href: drawable(href), internal: link.note.is_some(), target: link.target };
                group(out, Some(link), children);
                for &child in children {
                    self.visit(child, &inherited, pieces, out);
                }
                out.push(PaintVisit::End);
            }
            NodeContent::Path(path) => shape(out, Geometry::Path(path.commands())),
            NodeContent::Rect { x, y, width, height, rx } => {
                let radius = match node.styled.radius {
                    Some(Radius::Theme) => THEME_RADIUS,
                    Some(Radius::Px(radius)) => radius,
                    None => rx.map_or(0.0, |rx| rx.effective()),
                };
                let geometry = Geometry::Rect {
                    origin: Point { x: x.effective(), y: y.effective() },
                    size: Size { width: width.effective(), height: height.effective() },
                    radius,
                };
                shape(out, geometry);
            }
            NodeContent::Circle { cx, cy, radius } => {
                let centre = Point { x: cx.map_or(0.0, |cx| cx.effective()), y: cy.map_or(0.0, |cy| cy.effective()) };
                shape(out, Geometry::Circle { centre, radius: radius.effective() });
            }
            NodeContent::Ellipse { rx, ry } => {
                let radii = Size { width: rx.effective(), height: ry.effective() };
                shape(out, Geometry::Ellipse { centre: Point { x: 0.0, y: 0.0 }, radii });
            }
            NodeContent::Polygon(points) => shape(out, Geometry::Polygon(points.iter().map(at).collect())),
            NodeContent::Image { href, x, y, width, height, aspect } => {
                let geometry = Geometry::Image {
                    href: drawable(href),
                    origin: Point { x: x.effective(), y: y.effective() },
                    size: Size { width: width.effective(), height: height.effective() },
                    fit: match aspect {
                        Aspect::Stretch => ImageFit::Stretch,
                        Aspect::Meet => ImageFit::Meet,
                    },
                };
                shape(out, geometry);
            }
            NodeContent::Icon { name, x, y, size } => {
                let geometry = Geometry::Icon {
                    name: drawable(&crate::input::decode_entities(name)),
                    origin: Point { x: x.effective(), y: y.effective() },
                    size: Size { width: size.effective(), height: size.effective() },
                };
                shape(out, geometry);
            }
            NodeContent::Tooltip => {}
            NodeContent::Text(text) => {
                let part = self.part_key(node).expect("every drawing is a part of an entity");
                let label = pieces.labels.entry((node.part.expect("a part"), part.facet)).or_insert(0);
                let label = std::mem::replace(label, *label + 1);
                for (line, run, baseline) in runs(text, &inherited) {
                    let detail = match label {
                        0 => FragmentDetail::TextRun { line, run },
                        label => FragmentDetail::LaterTextRun { label, line, run },
                    };
                    let key = FragmentKey { part: part.clone(), detail };
                    let geometry = Geometry::Text(baseline);
                    let mut style = inherited.style(opacity);
                    if let TextEmission::Painted { fills, .. } = &text.emission
                        && let Some(&Some(fill)) = fills.get(run as usize)
                    {
                        style.fill = fill;
                    }
                    out.push(PaintVisit::Draw(Draw { key, origin: inherited.origin, geometry, style }));
                }
            }
        }
    }
}

/// The runs a text element draws, numbered by line and by place on the line.
fn runs(text: &PlacedText, inherited: &Inherited) -> Vec<(u32, u32, BaselineRun)> {
    let label = &text.label;
    match &text.emission {
        TextEmission::Plain { x, y } => {
            let line = &label.lines[0];
            let units: Vec<u16> = line.runs.iter().flat_map(|run| run.text.iter().copied()).collect();
            // One run, whose emphasis an attribute or a style of the text element writes.
            let style = line.runs.first().map_or(Emphasis::default(), |run| run.style);
            let run = BaselineRun {
                text: drawable(&units),
                baseline: Point { x: x.effective(), y: y.effective() },
                anchor: inherited.anchor,
                advance: line.width,
                font: inherited.font(style, Emphasis::default()),
                script: Script::Normal,
                underline: inherited.underline(style, Emphasis::default()),
            };
            vec![(0, 0, run)]
        }
        // A line's first run carries the line's point and the anchor places the whole line, so
        // each run starts where the runs before it end.
        TextEmission::Line { x, y, shared } => line_runs(label, &[[*x, *y]], *shared, inherited),
        TextEmission::Rich { lines, shared } => line_runs(label, lines, *shared, inherited),
        TextEmission::Painted { x, y, shared, .. } => line_runs(label, &[[*x, *y]], *shared, inherited),
    }
}

/// The runs of lines whose first run carries the line's point. What every run shares is written
/// once on the text element, so a run's own emphasis is only what it adds to that.
fn line_runs(
    label: &Label,
    lines: &[[Number; 2]],
    shared: Emphasis,
    inherited: &Inherited,
) -> Vec<(u32, u32, BaselineRun)> {
    let mut out = Vec::new();
    for (index, (line, at)) in label.lines.iter().zip(lines).enumerate() {
        let mut left = at[0].effective() - lead(inherited.anchor, line.width);
        for (place, run) in line.runs.iter().enumerate() {
            if !run.text.is_empty() {
                let own = Emphasis {
                    bold: run.style.bold && !shared.bold,
                    italic: run.style.italic && !shared.italic,
                    mono: run.style.mono && !shared.mono,
                    underline: run.style.underline,
                    ..Emphasis::default()
                };
                let baseline = BaselineRun {
                    text: drawable(&run.text),
                    baseline: Point { x: left, y: at[1].effective() },
                    anchor: AnchorKind::Start,
                    advance: run.width,
                    font: inherited.font(run.style, own),
                    script: run.style.script,
                    underline: inherited.underline(run.style, own),
                };
                out.push((index as u32, place as u32, baseline));
            }
            left += run.width;
        }
    }
    out
}

fn write_name(f: &mut std::fmt::Formatter<'_>, name: &[u16]) -> std::fmt::Result {
    for unit in char::decode_utf16(name.iter().copied()) {
        match unit {
            Ok(c @ ('\\' | '/' | '~' | '#')) => write!(f, "\\{c}")?,
            Ok(c) => write!(f, "{c}")?,
            Err(lone) => write!(f, "\\u{{{:x}}}", lone.unpaired_surrogate())?,
        }
    }
    Ok(())
}

/// A key as one line of text, distinct for distinct keys: `section:Dogs/legend/0.0`, or for an
/// edge with no name `edge:A~B~arrow_point normal#1/shape`. In a name, `\`, `/`, `~` and `#` are
/// escaped, and a lone surrogate is written `\u{d800}`.
impl std::fmt::Display for FragmentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entity = &self.part.entity;
        f.write_str(match entity.kind() {
            EntityKind::Diagram => "diagram:",
            EntityKind::Section => "section:",
            EntityKind::Node => "node:",
            EntityKind::Group => "group:",
            EntityKind::Edge => "edge:",
            EntityKind::Note => "note:",
        })?;
        match entity.name() {
            EntityName::Named(name) => write_name(f, name)?,
            EntityName::Between { from, to, kind } => {
                write_name(f, from)?;
                f.write_str("~")?;
                write_name(f, to)?;
                write!(f, "~{kind}")?;
            }
        }
        if entity.duplicate() > 0 {
            write!(f, "#{}", entity.duplicate())?;
        }
        f.write_str(match self.part.facet {
            Facet::Shape => "/shape",
            Facet::Label => "/label",
            Facet::Legend => "/legend",
            Facet::Title => "/title",
            Facet::Casing => "/casing",
            Facet::StartMarker => "/start",
            Facet::EndMarker => "/end",
            Facet::Halo => "/halo",
            Facet::Annotation => "/annotation",
            Facet::Attribute => "/attribute",
            Facet::Method => "/method",
            Facet::StartLabel => "/start-label",
            Facet::EndLabel => "/end-label",
            Facet::Stereotype => "/stereotype",
            Facet::Row => "/row",
        })?;
        match self.detail {
            FragmentDetail::Whole => Ok(()),
            FragmentDetail::Piece(piece) => write!(f, "/{piece}"),
            FragmentDetail::TextRun { line, run } => write!(f, "/{line}.{run}"),
            FragmentDetail::LaterTextRun { label, line, run } => write!(f, "/{label}/{line}.{run}"),
        }
    }
}
