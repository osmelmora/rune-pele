//! The shapes a node can be drawn as, ported from Pele's `src/svg/shapes.ts`. Each has a
//! size for a label, a drawing centred on its node's origin, how far its outline falls short of
//! its bounding box on each side, and how much of a side edges may spread along.

use crate::diagrams::flowchart::Shape;
use crate::js;
use crate::layout::{CNode, RoutePoint};
use crate::scene::{
    Facet, LayerSpec, LineLook, Look, Number, Paint, PartId, PathProgram, Point, Role, SceneBuilder, ShapeLook,
};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShapeSize {
    pub(crate) w: f64,
    pub(crate) h: f64,
    /// Vertical shift of the label from the shape's centre.
    pub(crate) dy: f64,
}

const PAD_X: f64 = 18.0;
const PAD_Y: f64 = 12.0;
// Kept clear at each end of a straight side, for the corner.
const CORNER: f64 = 12.0;

/// A side of a node's box, as Pele numbers them.
pub(crate) const TOP: i32 = 0;
pub(crate) const RIGHT: i32 = 1;
pub(crate) const BOTTOM: i32 = 2;
pub(crate) const LEFT: i32 = 3;

type Sizer = fn(f64, f64) -> ShapeSize;
type Drawer = fn(&mut Pen<'_>, f64, f64);
/// Distance from the bounding box to the outline on a side. The last number is how far along
/// the side from its middle, to the right or down.
type Inset = fn(f64, f64, i32, f64) -> f64;
/// How much of a side edges may spread along, centred on its middle: the top and bottom, or
/// the left and right when the flow runs sideways.
type Span = fn(f64, f64, bool) -> f64;

#[derive(Clone, Copy)]
struct ShapeDef {
    size: Sizer,
    draw: Drawer,
    inset: Option<Inset>,
    span: Option<Span>,
    no_label: bool,
    /// Ignores style fills.
    bare: bool,
}

/// What draws a shape: the Scene, the part the shape belongs to, and the node's two looks.
pub(crate) struct Pen<'a> {
    out: &'a mut SceneBuilder,
    part: PartId,
    a: &'a ShapeLook,
    line: &'a LineLook,
}

fn n(value: f64) -> Number {
    Number::num(value)
}

fn raw(value: f64) -> Number {
    Number::raw(value)
}

fn d() -> PathProgram {
    PathProgram::default()
}

impl Pen<'_> {
    fn rect_at(&mut self, origin: [f64; 2], size: [f64; 2], rx: Option<Number>, look: Look) {
        self.out.element(self.part, Facet::Shape).rect(origin.map(n), size.map(n), rx).look(look).finish();
    }

    /// Pele's `rect`: centred on the origin.
    fn rect(&mut self, w: f64, h: f64, rx: Option<Number>, look: Look) {
        self.rect_at([-w / 2.0, -h / 2.0], [w, h], rx, look);
    }

    fn poly(&mut self, points: &[f64], look: Look) {
        let points = points.chunks(2).map(|pair| [n(pair[0]), n(pair[1])]).collect();
        self.out.element(self.part, Facet::Shape).polygon(points).look(look).finish();
    }

    fn path(&mut self, path: PathProgram, look: Look) {
        self.out.element(self.part, Facet::Shape).path(path).look(look).finish();
    }

    fn circle(&mut self, radius: Number, look: Look) {
        self.out.element(self.part, Facet::Shape).circle(radius).look(look).finish();
    }

    fn circle_at(&mut self, centre: [Option<Number>; 2], radius: Number, look: Look) {
        self.out.element(self.part, Facet::Shape).circle_at(centre, radius).look(look).finish();
    }

    /// A document's outline: straight on three sides and a wave along the bottom.
    fn wave(&mut self, w: f64, h: f64) {
        let x = w / 2.0;
        let top = -h / 2.0;
        let base = h / 2.0 - 10.0;
        let q = w / 4.0;
        let path = d()
            .m(n(-x), n(top))
            .h(n(x))
            .v(n(base))
            .q_by([n(-q), raw(-12.0)], [n(-2.0 * q), raw(0.0)])
            .t_by(n(-2.0 * q), raw(0.0))
            .z();
        self.path(path, self.a.plain());
    }

    /// One curly brace at `x`, opening towards `dir`.
    fn brace(&mut self, x: f64, h: f64, dir: f64) {
        let y = h / 2.0;
        let k = 6.0 * dir;
        let path = d()
            .m(n(x), n(-y))
            .q_by([n(k), raw(0.0)], [n(k), raw(6.0)])
            .v(raw(-6.0))
            .q_by([raw(0.0), raw(6.0)], [n(k), raw(6.0)])
            .q_by([n(-k), raw(0.0)], [n(-k), raw(6.0)])
            .v(n(y - 6.0))
            .q_by([raw(0.0), raw(6.0)], [n(-k), raw(6.0)]);
        self.path(path, self.line.open());
    }
}

fn flat(w: f64, h: f64, sideways: bool) -> f64 {
    (if sideways { h } else { w }) - 2.0 * CORNER
}

/// Straight along the top and bottom except for `cut` at each end, and not straight at the sides.
fn flat_across(w: f64, sideways: bool, cut: f64) -> f64 {
    if sideways { 0.0 } else { w - 2.0 * cut - 2.0 * CORNER }
}

/// How far an ellipse's outline has fallen away from its bounding box, `t` from the middle of a
/// side `half` long.
fn bulge(t: f64, half: f64, depth: f64) -> f64 {
    let k = js::min(1.0, t.abs() / half);
    depth * (1.0 - (1.0 - k * k).sqrt())
}

fn round_inset(w: f64, h: f64, side: i32, t: f64) -> f64 {
    if side & 1 != 0 { bulge(t, h / 2.0, w / 2.0) } else { bulge(t, w / 2.0, h / 2.0) }
}

fn round_span(w: f64, h: f64, sideways: bool) -> f64 {
    (if sideways { h } else { w }) * 0.6
}

fn cap_radius(w: f64) -> f64 {
    js::min(12.0, js::max(6.0, w / 14.0))
}

fn cap_inset(w: f64, _h: f64, side: i32, t: f64) -> f64 {
    if side & 1 != 0 { 0.0 } else { bulge(t, w / 2.0, cap_radius(w)) }
}

fn lean_inset(_w: f64, h: f64, side: i32, _t: f64) -> f64 {
    if side & 1 != 0 { h / 8.0 } else { 0.0 }
}

fn lean_span(w: f64, h: f64, sideways: bool) -> f64 {
    flat_across(w, sideways, h / 4.0)
}

fn doc_inset(_w: f64, _h: f64, side: i32, _t: f64) -> f64 {
    if side == BOTTOM { 10.0 } else { 0.0 }
}

fn quarter_inset(w: f64, _h: f64, side: i32, _t: f64) -> f64 {
    if side & 1 != 0 { w / 4.0 } else { 0.0 }
}

fn sized(w: f64, h: f64, dy: f64) -> ShapeSize {
    ShapeSize { w, h, dy }
}

fn box_size(tw: f64, th: f64) -> ShapeSize {
    sized(js::max(tw + 2.0 * PAD_X, 2.0 * PAD_X + 12.0), th + 2.0 * PAD_Y, 0.0)
}

/// As wide as its label and padding, and half its height more.
fn slanted_size(tw: f64, th: f64) -> ShapeSize {
    let h = th + 2.0 * PAD_Y;
    sized(tw + 2.0 * PAD_X + h / 2.0, h, 0.0)
}

fn tri_size(tw: f64, th: f64, flip: bool) -> ShapeSize {
    let w = js::max(1.6 * tw + 40.0, 80.0);
    let h = (th + 10.0) / (1.0 - (tw + 8.0) / w);
    let dy = h / 2.0 - 10.0 - th / 2.0;
    sized(w, h, if flip { -dy } else { dy })
}

fn brace_size(tw: f64, th: f64) -> ShapeSize {
    sized(tw + 2.0 * PAD_X + 12.0, th + 2.0 * PAD_Y, 0.0)
}

fn doc_size(tw: f64, th: f64) -> ShapeSize {
    sized(tw + 2.0 * PAD_X, th + 2.0 * PAD_Y + 10.0, -5.0)
}

fn rounded_rect(pen: &mut Pen<'_>, w: f64, h: f64) {
    pen.rect(w, h, None, pen.a.rounded());
}

/// A cylinder's body and the front of its top rim.
fn cylinder(pen: &mut Pen<'_>, w: f64, h: f64) -> (f64, f64, f64) {
    let rx = w / 2.0;
    let ry = cap_radius(w);
    let top = -h / 2.0 + ry;
    let bottom = h / 2.0 - ry;
    let body = d()
        .m(n(-rx), n(top))
        .a([n(rx), n(ry)], true, [n(rx), n(top)])
        .v(n(bottom))
        .a([n(rx), n(ry)], true, [n(-rx), n(bottom)])
        .z();
    pen.path(body, pen.a.plain());
    (rx, ry, top)
}

const BASE: ShapeDef =
    ShapeDef { size: box_size, draw: rounded_rect, inset: None, span: None, no_label: false, bare: false };

/// Pele's `SHAPES` table. A shape that is not in it is drawn as a rectangle, with no inset and
/// its edges at the middle of each side.
fn def(shape: Shape) -> Option<ShapeDef> {
    Some(match shape {
        Shape::Rect => ShapeDef { span: Some(flat), ..BASE },
        Shape::Rounded => ShapeDef {
            span: Some(flat),
            draw: |pen, w, h| pen.rect(w, h, Some(n(js::min(12.0, h / 2.0))), pen.a.plain()),
            ..BASE
        },
        Shape::Stadium => ShapeDef {
            inset: Some(|_w, h, side, t| if side & 1 != 0 { bulge(t, h / 2.0, h / 2.0) } else { 0.0 }),
            span: Some(|w, h, sideways| if sideways { h * 0.6 } else { w - h }),
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                sized(tw + h * 0.5 + 2.0 * PAD_X - 8.0, h, 0.0)
            },
            draw: |pen, w, h| pen.rect(w, h, Some(n(h / 2.0)), pen.a.plain()),
            ..BASE
        },
        Shape::FrRect => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(tw + 2.0 * PAD_X + 16.0, th + 2.0 * PAD_Y, 0.0),
            draw: |pen, w, h| {
                let x = w / 2.0 - 8.0;
                rounded_rect(pen, w, h);
                let bars = d().m(n(-x), n(-h / 2.0)).v(n(h / 2.0)).m(n(x), n(-h / 2.0)).v(n(h / 2.0));
                pen.path(bars, pen.line.open());
            },
            ..BASE
        },
        Shape::Cyl => ShapeDef {
            inset: Some(cap_inset),
            span: Some(|w, h, sideways| if sideways { h - 2.0 * CORNER - 12.0 } else { w * 0.6 }),
            size: |tw, th| {
                let w = js::max(tw + 2.0 * PAD_X, 56.0);
                let ry = cap_radius(w);
                sized(w, th + 2.0 * PAD_Y + 2.0 * ry, ry / 2.0)
            },
            draw: |pen, w, h| {
                let (rx, ry, top) = cylinder(pen, w, h);
                let rim = d().m(n(-rx), n(top)).a([n(rx), n(ry)], false, [n(rx), n(top)]);
                pen.path(rim, pen.line.open());
            },
            ..BASE
        },
        Shape::Circle => ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| {
                let d = js::max((tw * tw + th * th).sqrt() + 16.0, 44.0);
                sized(d, d, 0.0)
            },
            draw: |pen, w, _h| pen.circle(n(w / 2.0), pen.a.plain()),
            ..BASE
        },
        Shape::DblCirc => ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| {
                let d = js::max((tw * tw + th * th).sqrt() + 26.0, 54.0);
                sized(d, d, 0.0)
            },
            draw: |pen, w, _h| {
                pen.circle(n(w / 2.0), pen.a.plain());
                pen.circle(n(w / 2.0 - 5.0), pen.line.open());
            },
            ..BASE
        },
        Shape::Ellipse => ShapeDef {
            inset: Some(round_inset),
            span: Some(round_span),
            size: |tw, th| sized(tw * 1.3 + 2.0 * PAD_X, th * 1.2 + 2.0 * PAD_Y, 0.0),
            draw: |pen, w, h| {
                pen.out.element(pen.part, Facet::Shape).ellipse(n(w / 2.0), n(h / 2.0)).look(pen.a.plain()).finish();
            },
            ..BASE
        },
        Shape::Diam => ShapeDef {
            inset: Some(|w, h, side, t| if side & 1 != 0 { (t.abs() * w) / h } else { (t.abs() * h) / w }),
            span: Some(round_span),
            size: |tw, th| {
                let a = tw + 16.0;
                let b = th + 8.0;
                sized(a + 1.6 * b, a / 1.6 + b, 0.0)
            },
            draw: |pen, w, h| pen.poly(&[0.0, -h / 2.0, w / 2.0, 0.0, 0.0, h / 2.0, -w / 2.0, 0.0], pen.a.plain()),
            ..BASE
        },
        Shape::Hex => ShapeDef {
            inset: Some(|_w, _h, side, t| if side & 1 != 0 { t.abs() / 2.0 } else { 0.0 }),
            span: Some(|w, h, sideways| if sideways { h * 0.5 } else { w - h / 2.0 - CORNER }),
            size: slanted_size,
            draw: |pen, w, h| {
                let m = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x + m, -y, x - m, -y, x, 0.0, x - m, y, -x + m, y, -x, 0.0], pen.a.plain());
            },
            ..BASE
        },
        Shape::Odd => ShapeDef {
            inset: Some(|_w, h, side, _t| if side == LEFT { h / 4.0 } else { 0.0 }),
            span: Some(|w, _h, sideways| flat_across(w, sideways, 0.0)),
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                sized(tw + 2.0 * PAD_X + h / 4.0, h, 0.0)
            },
            draw: |pen, w, h| {
                let notch = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x, -y, x, -y, x, y, -x, y, -x + notch, 0.0], pen.a.plain());
            },
            ..BASE
        },
        Shape::LeanR => ShapeDef {
            inset: Some(lean_inset),
            span: Some(lean_span),
            size: slanted_size,
            draw: |pen, w, h| {
                let s = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x + s, -y, x, -y, x - s, y, -x, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::LeanL => ShapeDef {
            inset: Some(lean_inset),
            span: Some(lean_span),
            size: slanted_size,
            draw: |pen, w, h| {
                let s = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x, -y, x - s, -y, x, y, -x + s, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::TrapB => ShapeDef {
            inset: Some(lean_inset),
            span: Some(lean_span),
            size: slanted_size,
            draw: |pen, w, h| {
                let s = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x + s, -y, x - s, -y, x, y, -x, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::TrapT => ShapeDef {
            inset: Some(lean_inset),
            span: Some(lean_span),
            size: slanted_size,
            draw: |pen, w, h| {
                let s = h / 4.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x, -y, x, -y, x - s, y, -x + s, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::Text => ShapeDef {
            bare: true,
            span: Some(flat),
            size: |tw, th| sized(tw + 8.0, th + 8.0, 0.0),
            draw: |pen, w, h| pen.rect(w, h, None, Look::unseen()),
            ..BASE
        },
        Shape::SmCirc => ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| sized(14.0, 14.0, 0.0),
            draw: |pen, w, _h| pen.circle(n(w / 2.0), pen.a.plain()),
            ..BASE
        },
        Shape::FCirc => ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| sized(14.0, 14.0, 0.0),
            draw: |pen, w, _h| pen.circle(n(w / 2.0), pen.line.filled(Paint::Role(Role::Line))),
            ..BASE
        },
        Shape::FrCirc => ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| sized(18.0, 18.0, 0.0),
            draw: |pen, w, _h| {
                pen.circle(n(w / 2.0), pen.a.plain());
                pen.circle(n(w / 2.0 - 4.0), pen.line.filled(Paint::Role(Role::Line)));
            },
            ..BASE
        },
        Shape::Fork => ShapeDef {
            no_label: true,
            span: Some(|w, h, sideways| (if sideways { h } else { w }) - 8.0),
            size: |_, _| sized(72.0, 8.0, 0.0),
            draw: |pen, w, h| pen.rect(w, h, Some(raw(2.0)), pen.line.filled(Paint::Role(Role::Line))),
            ..BASE
        },
        Shape::NotchRect => ShapeDef {
            span: Some(flat),
            draw: |pen, w, h| {
                let c = 10.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x + c, -y, x, -y, x, y, -x, y, -x, -y + c], pen.a.plain());
            },
            ..BASE
        },
        Shape::LinRect => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y, 0.0),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                pen.path(d().m(n(-w / 2.0 + 8.0), n(-h / 2.0)).v(n(h / 2.0)), pen.line.open());
            },
            ..BASE
        },
        Shape::DivRect => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(tw + 2.0 * PAD_X, th + 2.0 * PAD_Y + 8.0, 4.0),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                pen.path(d().m(n(-w / 2.0), n(-h / 2.0 + 8.0)).h(n(w / 2.0)), pen.line.open());
            },
            ..BASE
        },
        Shape::WinPane => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y + 8.0, 4.0),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                let panes =
                    d().m(n(-w / 2.0 + 8.0), n(-h / 2.0)).v(n(h / 2.0)).m(n(-w / 2.0), n(-h / 2.0 + 8.0)).h(n(w / 2.0));
                pen.path(panes, pen.line.open());
            },
            ..BASE
        },
        Shape::Tri => ShapeDef {
            inset: Some(quarter_inset),
            size: |tw, th| tri_size(tw, th, false),
            draw: |pen, w, h| pen.poly(&[0.0, -h / 2.0, w / 2.0, h / 2.0, -w / 2.0, h / 2.0], pen.a.plain()),
            ..BASE
        },
        Shape::FlipTri => ShapeDef {
            inset: Some(quarter_inset),
            size: |tw, th| tri_size(tw, th, true),
            draw: |pen, w, h| pen.poly(&[-w / 2.0, -h / 2.0, w / 2.0, -h / 2.0, 0.0, h / 2.0], pen.a.plain()),
            ..BASE
        },
        Shape::SlRect => ShapeDef {
            inset: Some(|_w, _h, side, _t| if side == TOP { 5.0 } else { 0.0 }),
            size: |tw, th| sized(tw + 2.0 * PAD_X, th + 2.0 * PAD_Y + 10.0, 5.0),
            draw: |pen, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x, -y + 10.0, x, -y, x, y, -x, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::NotchPent => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(tw + 2.0 * PAD_X, th + 2.0 * PAD_Y, 0.0),
            draw: |pen, w, h| {
                let c = 10.0;
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x + c, -y, x - c, -y, x, -y + c, x, y, -x, y, -x, -y + c], pen.a.plain());
            },
            ..BASE
        },
        Shape::Hourglass => ShapeDef {
            no_label: true,
            inset: Some(|w, _h, side, _t| if side & 1 != 0 { w / 2.0 } else { 0.0 }),
            size: |_, _| sized(32.0, 40.0, 0.0),
            draw: |pen, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                pen.poly(&[-x, -y, x, -y, -x, y, x, y], pen.a.plain());
            },
            ..BASE
        },
        Shape::Delay => ShapeDef {
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                sized(tw + 2.0 * PAD_X + h / 4.0, h, 0.0)
            },
            draw: |pen, w, h| {
                let r = h / 2.0;
                let outline = d()
                    .m(n(-w / 2.0), n(-r))
                    .h(n(w / 2.0 - r))
                    .a([n(r), n(r)], true, [n(w / 2.0 - r), n(r)])
                    .h(n(-w / 2.0))
                    .z();
                pen.path(outline, pen.a.plain());
            },
            ..BASE
        },
        Shape::HCyl => ShapeDef {
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                let rx = js::min(12.0, h / 4.0);
                sized(tw + 2.0 * PAD_X + 2.0 * rx, h, 0.0)
            },
            draw: |pen, w, h| {
                let ry = h / 2.0;
                let rx = js::min(12.0, h / 4.0);
                let l = -w / 2.0 + rx;
                let r = w / 2.0 - rx;
                let body = d()
                    .m(n(l), n(-ry))
                    .h(n(r))
                    .a([n(rx), n(ry)], true, [n(r), n(ry)])
                    .h(n(l))
                    .a([n(rx), n(ry)], true, [n(l), n(-ry)])
                    .z();
                pen.path(body, pen.a.plain());
                pen.path(d().m(n(r), n(-ry)).a([n(rx), n(ry)], false, [n(r), n(ry)]), pen.line.open());
            },
            ..BASE
        },
        Shape::CrossCirc => ShapeDef {
            no_label: true,
            inset: Some(round_inset),
            span: Some(round_span),
            size: |_, _| sized(44.0, 44.0, 0.0),
            draw: |pen, w, _h| {
                let r = w / 2.0;
                let k = r * FRAC_1_SQRT_2;
                pen.circle(n(r), pen.a.plain());
                pen.path(d().m(n(-k), n(-k)).l(n(k), n(k)).m(n(k), n(-k)).l(n(-k), n(k)), pen.line.open());
            },
            ..BASE
        },
        Shape::StRect => ShapeDef {
            span: Some(|w, h, sideways| (if sideways { h } else { w }) - 2.0 * CORNER - 8.0),
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y + 8.0, 4.0),
            draw: |pen, w, h| {
                for i in 0..3 {
                    let x = -w / 2.0 + f64::from(2 - i) * 4.0;
                    let y = -h / 2.0 + f64::from(i) * 4.0;
                    pen.rect_at([x, y], [w - 8.0, h - 8.0], None, pen.a.rounded());
                }
            },
            ..BASE
        },
        Shape::Doc => ShapeDef { inset: Some(doc_inset), size: doc_size, draw: |pen, w, h| pen.wave(w, h), ..BASE },
        Shape::Flag => ShapeDef {
            inset: Some(|_w, _h, side, _t| if side & 1 != 0 { 0.0 } else { 8.0 }),
            size: |tw, th| sized(tw + 2.0 * PAD_X, th + 2.0 * PAD_Y + 16.0, 0.0),
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0 - 8.0;
                let q = w / 4.0;
                let tape = d()
                    .m(n(-x), n(-y))
                    .q_by([n(q), raw(-10.0)], [n(2.0 * q), raw(0.0)])
                    .t_by(n(2.0 * q), raw(0.0))
                    .v(n(y))
                    .q_by([n(-q), raw(10.0)], [n(-2.0 * q), raw(0.0)])
                    .t_by(n(-2.0 * q), raw(0.0))
                    .z();
                pen.path(tape, pen.a.plain());
            },
            ..BASE
        },
        Shape::Datastore => ShapeDef {
            span: Some(flat),
            draw: |pen, w, h| {
                let (x, y) = (w / 2.0, h / 2.0);
                pen.rect(w, h, None, pen.a.borderless());
                pen.path(d().m(n(-x), n(-y)).h(n(x)).m(n(-x), n(y)).h(n(x)), pen.line.open());
            },
            ..BASE
        },
        Shape::Folder => ShapeDef {
            size: |tw, th| sized(js::max(tw + 2.0 * PAD_X, 64.0), th + 2.0 * PAD_Y + 8.0, 4.0),
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0;
                let tab = js::min(w * 0.4, 48.0);
                let outline =
                    d().m(n(-x), n(-y)).h(n(-x + tab)).l(n(-x + tab + 8.0), n(-y + 8.0)).h(n(x)).v(n(y)).h(n(-x)).z();
                pen.path(outline, pen.a.plain());
            },
            ..BASE
        },
        Shape::Bucket => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 20.0, th + 2.0 * PAD_Y + 10.0, 3.0),
            inset: Some(|_w, h, side, _t| if side & 1 != 0 { h / 12.0 } else { 0.0 }),
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0;
                let ry = 5.0;
                let taper = h / 6.0;
                let body = d()
                    .m(n(-x), n(-y + ry))
                    .a([n(x), n(ry)], true, [n(x), n(-y + ry)])
                    .l(n(x - taper), n(y - ry))
                    .a([n(x - taper), n(ry)], true, [n(-x + taper), n(y - ry)])
                    .z();
                pen.path(body, pen.a.plain());
                pen.path(d().m(n(-x), n(-y + ry)).a([n(x), n(ry)], false, [n(x), n(-y + ry)]), pen.line.open());
            },
            ..BASE
        },
        Shape::Console => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(js::max(tw + 2.0 * PAD_X, 72.0), th + 2.0 * PAD_Y + 12.0, 6.0),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                let prompt = d()
                    .m(n(-w / 2.0), n(-h / 2.0 + 12.0))
                    .h(n(w / 2.0))
                    .m(n(-w / 2.0 + 7.0), n(-h / 2.0 + 4.0))
                    .l_by(raw(3.0), raw(2.5))
                    .l_by(raw(-3.0), raw(2.5))
                    .m(n(-w / 2.0 + 13.0), n(-h / 2.0 + 9.0))
                    .h_by(raw(4.0));
                pen.path(prompt, pen.line.open());
            },
            ..BASE
        },
        Shape::Browser => ShapeDef {
            span: Some(flat),
            size: |tw, th| sized(js::max(tw + 2.0 * PAD_X, 72.0), th + 2.0 * PAD_Y + 12.0, 6.0),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                pen.path(d().m(n(-w / 2.0), n(-h / 2.0 + 12.0)).h(n(w / 2.0)), pen.line.open());
                for i in 0..3 {
                    let centre = [Some(n(-w / 2.0 + 8.0 + f64::from(i) * 7.0)), Some(n(-h / 2.0 + 6.0))];
                    pen.circle_at(centre, raw(1.5), pen.line.open());
                }
            },
            ..BASE
        },
        Shape::Person => ShapeDef {
            size: |tw, th| sized(js::max(tw + 2.0 * PAD_X, 56.0), th + 2.0 * PAD_Y + 26.0, 13.0),
            draw: |pen, w, h| {
                let top = -h / 2.0;
                pen.rect_at([-w / 2.0, top + 26.0], [w, h - 26.0], Some(raw(12.0)), pen.a.plain());
                pen.circle_at([None, Some(n(top + 14.0))], raw(13.0), pen.a.plain());
            },
            ..BASE
        },
        Shape::Bang => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 28.0, th + 2.0 * PAD_Y + 24.0, 0.0),
            inset: Some(|_w, _h, _side, _t| 3.0),
            draw: |pen, w, h| {
                let spikes = 14.0;
                let mut points = Vec::new();
                for i in 0..28 {
                    let angle = (PI * f64::from(i)) / spikes - PI / 2.0;
                    let k = if i % 2 == 0 { 1.0 } else { 0.82 };
                    points.extend([js::cos(angle) * (w / 2.0) * k, js::sin(angle) * (h / 2.0) * k]);
                }
                pen.poly(&points, pen.a.plain());
            },
            ..BASE
        },
        Shape::Cloud => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 32.0, th + 2.0 * PAD_Y + 24.0, 0.0),
            inset: Some(|_w, _h, side, _t| if side & 1 != 0 { 2.0 } else { 1.0 }),
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0;
                let r = y * 0.55;
                let outline = d()
                    .m(n(-x + r), n(y))
                    .a([n(r), n(r)], true, [n(-x + r * 0.6), n(-y * 0.1)])
                    .a([n(x * 0.42), n(y * 0.7)], true, [n(-x * 0.1), n(-y * 0.72)])
                    .a([n(x * 0.4), n(y * 0.62)], true, [n(x * 0.62), n(-y * 0.42)])
                    .a([n(r * 1.05), n(r * 1.05)], true, [n(x - r), n(y)])
                    .z();
                pen.path(outline, pen.a.plain());
            },
            ..BASE
        },
        Shape::Brace => ShapeDef {
            bare: true,
            size: brace_size,
            draw: |pen, w, h| {
                pen.rect(w, h, None, Look::unseen());
                pen.brace(-w / 2.0 + 12.0, h, -1.0);
            },
            ..BASE
        },
        Shape::BraceR => ShapeDef {
            bare: true,
            size: brace_size,
            draw: |pen, w, h| {
                pen.rect(w, h, None, Look::unseen());
                pen.brace(w / 2.0 - 12.0, h, 1.0);
            },
            ..BASE
        },
        Shape::Braces => ShapeDef {
            bare: true,
            size: |tw, th| sized(tw + 2.0 * PAD_X + 24.0, th + 2.0 * PAD_Y, 0.0),
            draw: |pen, w, h| {
                pen.rect(w, h, None, Look::unseen());
                pen.brace(-w / 2.0 + 12.0, h, -1.0);
                pen.brace(w / 2.0 - 12.0, h, 1.0);
            },
            ..BASE
        },
        Shape::Bolt => ShapeDef {
            no_label: true,
            size: |_, _| sized(36.0, 56.0, 0.0),
            inset: Some(quarter_inset),
            draw: |pen, w, h| {
                let points = [
                    w * 0.15,
                    -h / 2.0,
                    -w / 2.0,
                    h * 0.08,
                    -w * 0.05,
                    h * 0.08,
                    -w * 0.15,
                    h / 2.0,
                    w / 2.0,
                    -h * 0.08,
                    w * 0.05,
                    -h * 0.08,
                ];
                pen.poly(&points, pen.a.plain());
            },
            ..BASE
        },
        Shape::LinCyl => ShapeDef {
            inset: Some(cap_inset),
            span: Some(|w, h, sideways| if sideways { h - 2.0 * CORNER - 20.0 } else { w * 0.6 }),
            size: |tw, th| {
                let w = js::max(tw + 2.0 * PAD_X, 56.0);
                let ry = cap_radius(w);
                sized(w, th + 2.0 * PAD_Y + 2.0 * ry + 8.0, ry / 2.0 + 4.0)
            },
            draw: |pen, w, h| {
                let (rx, ry, top) = cylinder(pen, w, h);
                let arc = |path: PathProgram, y: f64| path.m(n(-rx), n(y)).a([n(rx), n(ry)], false, [n(rx), n(y)]);
                pen.path(arc(arc(d(), top), top + 8.0), pen.line.open());
            },
            ..BASE
        },
        Shape::CurvTrap => ShapeDef {
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                sized(tw + 2.0 * PAD_X + h * 0.6, h, 0.0)
            },
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0;
                let k = h * 0.3;
                let outline = d()
                    .m(n(-x + k), n(-y))
                    .h(n(x - k))
                    .a([n(k), n(y)], true, [n(x - k), n(y)])
                    .h(n(-x + k))
                    .l(n(-x), raw(0.0))
                    .z();
                pen.path(outline, pen.a.plain());
            },
            ..BASE
        },
        Shape::Docs => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y + 18.0, -1.0),
            inset: Some(doc_inset),
            draw: |pen, w, h| {
                for i in 0..3 {
                    let offset = Point { x: f64::from(1 - i) * 4.0, y: f64::from(i - 1) * 4.0 };
                    let (part, a, line) = (pen.part, pen.a, pen.line);
                    pen.out.layer(LayerSpec::group().translate(offset), |out| {
                        Pen { out, part, a, line }.wave(w - 8.0, h - 8.0);
                    });
                }
            },
            ..BASE
        },
        Shape::BowRect => ShapeDef {
            size: |tw, th| {
                let h = th + 2.0 * PAD_Y;
                sized(tw + 2.0 * PAD_X + h * 0.4, h, 0.0)
            },
            inset: Some(|_w, h, side, _t| if side == RIGHT { h * 0.2 } else { 0.0 }),
            draw: |pen, w, h| {
                let x = w / 2.0;
                let y = h / 2.0;
                let k = h * 0.2;
                let outline = d()
                    .m(n(-x + k), n(-y))
                    .h(n(x))
                    .a([n(k), n(y)], false, [n(x), n(y)])
                    .h(n(-x + k))
                    .a([n(k), n(y)], true, [n(-x + k), n(-y)])
                    .z();
                pen.path(outline, pen.a.plain());
            },
            ..BASE
        },
        Shape::TagRect => ShapeDef {
            span: Some(flat),
            draw: |pen, w, h| {
                rounded_rect(pen, w, h);
                pen.path(d().m(n(w / 2.0 - 12.0), n(h / 2.0)).l(n(w / 2.0), n(h / 2.0 - 12.0)), pen.line.open());
            },
            ..BASE
        },
        Shape::TagDoc => ShapeDef {
            size: doc_size,
            inset: Some(doc_inset),
            draw: |pen, w, h| {
                pen.wave(w, h);
                let tag = d().m(n(w / 2.0 - 12.0), n(h / 2.0 - 14.0)).l(n(w / 2.0), n(h / 2.0 - 26.0));
                pen.path(tag, pen.line.open());
            },
            ..BASE
        },
        Shape::LinDoc => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y + 10.0, -5.0),
            inset: Some(doc_inset),
            draw: |pen, w, h| {
                pen.wave(w, h);
                pen.path(d().m(n(-w / 2.0 + 8.0), n(-h / 2.0)).v(n(h / 2.0 - 6.0)), pen.line.open());
            },
            ..BASE
        },
        Shape::CollapsedGroup => ShapeDef {
            size: |tw, th| sized(tw + 2.0 * PAD_X + 8.0, th + 2.0 * PAD_Y + 8.0, -2.0),
            draw: |pen, w, h| {
                pen.rect_at([-w / 2.0 + 6.0, -h / 2.0 + 8.0], [w - 12.0, h - 8.0], None, pen.a.rounded());
                pen.rect_at([-w / 2.0, -h / 2.0], [w, h - 4.0], None, pen.a.rounded());
            },
            ..BASE
        },
        _ => return None,
    })
}

pub(crate) fn shape_size(shape: Shape, tw: f64, th: f64) -> ShapeSize {
    (def(shape).unwrap_or(BASE).size)(tw, th)
}

/// Draws `shape`, `w` by `h` and centred on its group's origin. `a` paints its outline and
/// `line` the lines inside it.
pub(crate) fn draw_shape(
    out: &mut SceneBuilder,
    part: PartId,
    shape: Shape,
    w: f64,
    h: f64,
    a: &ShapeLook,
    line: &LineLook,
) {
    (def(shape).unwrap_or(BASE).draw)(&mut Pen { out, part, a, line }, w, h);
}

fn shape_inset(shape: Shape, w: f64, h: f64, side: i32, t: f64) -> f64 {
    def(shape).and_then(|def| def.inset).map_or(0.0, |inset| inset(w, h, side, t))
}

/// How much of a side of the shape edges may spread along, when the flow runs down or, if
/// `sideways`, across.
pub(crate) fn shape_span(shape: Shape, w: f64, h: f64, sideways: bool) -> f64 {
    js::max(0.0, def(shape).and_then(|def| def.span).map_or(0.0, |span| span(w, h, sideways)))
}

pub(crate) fn shape_has_label(shape: Shape) -> bool {
    !def(shape).is_some_and(|def| def.no_label)
}

/// Whether the shape takes the fill of its node's style, and so whether a label on it needs
/// ink that contrasts with that fill.
pub(crate) fn shape_filled(shape: Shape) -> bool {
    !def(shape).unwrap_or(BASE).bare
}

/// The side of a node's layout box that a route end lies on, or -1 for none.
pub(crate) fn end_side(at: &RoutePoint, c: &CNode) -> i32 {
    let dx = at.x - c.x;
    let dy = at.y - c.y;
    if (dy.abs() - c.h / 2.0).abs() < 0.5 {
        return if dy < 0.0 { TOP } else { BOTTOM };
    }
    if (dx.abs() - c.w / 2.0).abs() < 0.5 {
        return if dx < 0.0 { LEFT } else { RIGHT };
    }
    -1
}

/// Moves a route end from a node's layout box onto the outline of the shape drawn in it, which
/// is `w` by `h`.
pub(crate) fn inset_route(at: &mut RoutePoint, shape: Shape, w: f64, h: f64, c: &CNode) {
    if c.is_group {
        return;
    }
    let side = end_side(at, c);
    if side == TOP || side == BOTTOM {
        let amount = shape_inset(shape, w, h, side, at.x - c.x) + (c.h - h) / 2.0;
        at.y += if side == TOP { amount } else { -amount };
    } else if side >= 0 {
        let amount = shape_inset(shape, w, h, side, at.y - c.y) + (c.w - w) / 2.0;
        at.x += if side == LEFT { amount } else { -amount };
    }
}
