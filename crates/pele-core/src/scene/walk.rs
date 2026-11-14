//! The paint walk: the Scene in paint order, each drawing with the values it is drawn with.
//!
//! An SVG element inherits its paint, stroke, font size and anchor from the groups around it, and
//! a group's translation moves everything inside it. The walk resolves both, so an interpreter
//! reads a drawing's effective style and where its origin sits, and never Pele's attributes.

use super::{
    AnchorKind, Document, EntityKey, Facet, LineJoin, Node, NodeContent, NodeId, Paint, PathStep, PlacedText, Point,
    Properties, Role, Scene, Size, TextEmission, TextWeight, Transform,
};
use crate::measure::{Emphasis, Font, FontFamily, FontWeight, SMALLER, Script};

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
    /// A shape, which is drawn whole.
    Whole,
    /// One run of a label: its line, and its place on the line.
    TextRun { line: u32, run: u32 },
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
    /// How opaque the group is as a whole, composited once.
    pub opacity: f64,
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
    Rect { origin: Point, size: Size, radius: f64 },
    Circle { centre: Point, radius: f64 },
    Text(BaselineRun),
}

/// A path command with absolute coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCommand {
    Move(Point),
    Line(Point),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintStyle {
    pub fill: Paint,
    pub stroke: Option<StrokeStyle>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    pub paint: Paint,
    pub width: f64,
    pub join: LineJoin,
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

/// What a node inherits from the groups around it, starting from what the root element sets.
#[derive(Clone, Copy)]
struct Inherited {
    origin: Point,
    fill: Paint,
    stroke: Paint,
    stroke_width: f64,
    join: LineJoin,
    font_size: f64,
    anchor: AnchorKind,
    weight: Option<TextWeight>,
}

impl Inherited {
    /// The root `<svg>`: `fill="var(--_fg)"` and the font size of its `font` style.
    fn root(document: &Document) -> Self {
        Inherited {
            origin: Point { x: 0.0, y: 0.0 },
            fill: Paint::Role(Role::Foreground),
            stroke: Paint::None,
            stroke_width: 1.0,
            join: LineJoin::Miter,
            font_size: document.font_size,
            anchor: AnchorKind::Start,
            weight: None,
        }
    }

    fn within(mut self, properties: &Properties) -> Self {
        if let Some(Transform::Translate(x, y)) = properties.transform {
            self.origin = Point { x: self.origin.x + x.effective(), y: self.origin.y + y.effective() };
        }
        self.fill = properties.fill.unwrap_or(self.fill);
        self.stroke = properties.stroke.unwrap_or(self.stroke);
        self.stroke_width = properties.stroke_width.map_or(self.stroke_width, |width| width.effective());
        self.join = properties.join.unwrap_or(self.join);
        self.font_size = properties.font_size.map_or(self.font_size, |size| size.effective());
        self.anchor = properties.anchor.unwrap_or(self.anchor);
        self.weight = properties.weight.or(self.weight);
        self
    }

    fn style(&self) -> PaintStyle {
        let stroke = (self.stroke != Paint::None).then_some(StrokeStyle {
            paint: self.stroke,
            width: self.stroke_width,
            join: self.join,
        });
        PaintStyle { fill: self.fill, stroke }
    }

    fn font(&self, style: Emphasis) -> Font {
        Font {
            family: if style.mono { FontFamily::Mono } else { FontFamily::Sans },
            size: if style.script == Script::Normal { self.font_size } else { self.font_size / SMALLER },
            weight: if style.bold || self.weight.is_some() { FontWeight::Bold } else { FontWeight::Normal },
            italic: style.italic,
        }
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
fn drawable(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied()).flatten().filter(|&c| !super::xml_drops(c)).collect()
}

impl Scene {
    /// Every drawing in paint order, inside the groups that hold it.
    pub fn paint(&self) -> impl Iterator<Item = PaintVisit> + '_ {
        let mut out = Vec::new();
        let root = Inherited::root(&self.document);
        for &id in &self.roots {
            self.visit(id, root, &mut out);
        }
        out.into_iter()
    }

    fn part_key(&self, node: &Node) -> Option<PartKey> {
        let part = node.part?;
        let facet = node.facet.expect("a part of an entity has a facet");
        Some(PartKey { entity: self.entity(part).key.clone(), facet })
    }

    fn visit(&self, id: NodeId, outer: Inherited, out: &mut Vec<PaintVisit>) {
        let node = self.node(id);
        let inherited = outer.within(&node.properties);
        let draw = |out: &mut Vec<PaintVisit>, detail: FragmentDetail, geometry: Geometry| {
            let part = self.part_key(node).expect("every drawing is a part of an entity");
            let key = FragmentKey { part, detail };
            out.push(PaintVisit::Draw(Draw { key, origin: inherited.origin, geometry, style: inherited.style() }));
        };
        match &node.content {
            NodeContent::Group(children) => {
                let translate = match node.properties.transform {
                    Some(Transform::Translate(x, y)) => Point { x: x.effective(), y: y.effective() },
                    None => Point { x: 0.0, y: 0.0 },
                };
                out.push(PaintVisit::Begin(Group { part: self.part_key(node), translate, opacity: 1.0 }));
                for &child in children {
                    self.visit(child, inherited, out);
                }
                out.push(PaintVisit::End);
            }
            NodeContent::Path(path) => {
                let at = |pair: &[super::Number; 2]| Point { x: pair[0].effective(), y: pair[1].effective() };
                let commands = path
                    .steps
                    .iter()
                    .map(|step| match step {
                        PathStep::Move(to) => PathCommand::Move(at(to)),
                        PathStep::Line(to) => PathCommand::Line(at(to)),
                        PathStep::Arc { radii, large, sweep, end } => PathCommand::Arc {
                            rx: radii[0].effective(),
                            ry: radii[1].effective(),
                            large: *large,
                            sweep: *sweep,
                            end: at(end),
                        },
                        PathStep::Close => PathCommand::Close,
                    })
                    .collect();
                draw(out, FragmentDetail::Whole, Geometry::Path(commands));
            }
            NodeContent::Rect { x, y, width, height, rx } => {
                let geometry = Geometry::Rect {
                    origin: Point { x: x.effective(), y: y.effective() },
                    size: Size { width: width.effective(), height: height.effective() },
                    radius: rx.effective(),
                };
                draw(out, FragmentDetail::Whole, geometry);
            }
            NodeContent::Circle { radius } => {
                let geometry = Geometry::Circle { centre: Point { x: 0.0, y: 0.0 }, radius: radius.effective() };
                draw(out, FragmentDetail::Whole, geometry);
            }
            NodeContent::Text(text) => {
                for (line, run, baseline) in runs(text, &inherited) {
                    draw(out, FragmentDetail::TextRun { line, run }, Geometry::Text(baseline));
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
            let run = BaselineRun {
                text: drawable(&units),
                baseline: Point { x: x.effective(), y: y.effective() },
                anchor: inherited.anchor,
                advance: line.width,
                font: inherited.font(Emphasis::default()),
                script: Script::Normal,
                underline: false,
            };
            vec![(0, 0, run)]
        }
        // A line's first run carries the line's point and the anchor places the whole line, so
        // each run starts where the runs before it end.
        TextEmission::Rich { lines, .. } => {
            let mut out = Vec::new();
            for (index, (line, at)) in label.lines.iter().zip(lines).enumerate() {
                let mut left = at[0].effective() - lead(inherited.anchor, line.width);
                for (place, run) in line.runs.iter().enumerate() {
                    if !run.text.is_empty() {
                        let baseline = BaselineRun {
                            text: drawable(&run.text),
                            baseline: Point { x: left, y: at[1].effective() },
                            anchor: AnchorKind::Start,
                            advance: run.width,
                            font: inherited.font(run.style),
                            script: run.style.script,
                            underline: run.style.underline,
                        };
                        out.push((index as u32, place as u32, baseline));
                    }
                    left += run.width;
                }
            }
            out
        }
    }
}

/// A key as one line of text, distinct for distinct keys: `section:Dogs/legend/0.0`. In the
/// identity, `\` and `/` are escaped, and a lone surrogate is written `\u{d800}`.
impl std::fmt::Display for FragmentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let entity = &self.part.entity;
        f.write_str(match entity.kind {
            super::EntityKind::Diagram => "diagram:",
            super::EntityKind::Section => "section:",
        })?;
        for unit in char::decode_utf16(entity.identity.iter().copied()) {
            match unit {
                Ok(c @ ('\\' | '/')) => write!(f, "\\{c}")?,
                Ok(c) => write!(f, "{c}")?,
                Err(lone) => write!(f, "\\u{{{:x}}}", lone.unpaired_surrogate())?,
            }
        }
        f.write_str(match self.part.facet {
            Facet::Shape => "/shape",
            Facet::Label => "/label",
            Facet::Legend => "/legend",
            Facet::Title => "/title",
        })?;
        match self.detail {
            FragmentDetail::Whole => Ok(()),
            FragmentDetail::TextRun { line, run } => write!(f, "/{line}.{run}"),
        }
    }
}
