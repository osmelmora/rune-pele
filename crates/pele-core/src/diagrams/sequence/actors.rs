//! The kinds of participant and how each is drawn, ported from Pele's
//! `src/diagrams/sequence/actors.ts`.

use crate::input::Units;
use crate::js;
use crate::measure::Label;
use crate::scene::{
    Facet, LabelLook, LineLook, Number, Paint, PartId, PathProgram, Point, Role, SceneBuilder, ShapeLook,
};

const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const GLYPH: f64 = 34.0;
const GLYPH_GAP: f64 = 4.0;
const STACK: f64 = 5.0;
const CAP: f64 = 6.0;

/// A kind of participant Pele draws. A kind it does not know is drawn as a participant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ActorKind {
    Participant,
    Actor,
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

impl ActorKind {
    const ALL: [ActorKind; 8] = [
        ActorKind::Participant,
        ActorKind::Actor,
        ActorKind::Boundary,
        ActorKind::Control,
        ActorKind::Entity,
        ActorKind::Database,
        ActorKind::Collections,
        ActorKind::Queue,
    ];

    /// Pele's `actorKind`.
    pub(super) fn of(named: &[u16]) -> Self {
        ActorKind::ALL.into_iter().find(|kind| named.is_str(kind.name())).unwrap_or(ActorKind::Participant)
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            ActorKind::Participant => "participant",
            ActorKind::Actor => "actor",
            ActorKind::Boundary => "boundary",
            ActorKind::Control => "control",
            ActorKind::Entity => "entity",
            ActorKind::Database => "database",
            ActorKind::Collections => "collections",
            ActorKind::Queue => "queue",
        }
    }

    /// A figure stands above its label. The other kinds hold the label inside.
    pub(super) fn is_figure(self) -> bool {
        matches!(
            self,
            ActorKind::Actor | ActorKind::Boundary | ActorKind::Control | ActorKind::Entity | ActorKind::Database
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ActorSize {
    pub(super) w: f64,
    pub(super) h: f64,
}

/// Pele's `actorSize`. `tight` halves the space beside a label.
pub(super) fn actor_size(kind: ActorKind, label: &Label, min_w: f64, min_h: f64, tight: bool) -> ActorSize {
    if kind.is_figure() {
        let under = if label.height > 0.0 { GLYPH_GAP + label.height } else { 0.0 };
        return ActorSize { w: js::max(40.0, label.width + 8.0), h: GLYPH + under };
    }
    let padding_x = if tight { 8.0 } else { PAD_X };
    let (extra, stacked) = match kind {
        ActorKind::Collections => (STACK, STACK),
        ActorKind::Queue => (2.0 * CAP, 0.0),
        _ => (0.0, 0.0),
    };
    ActorSize {
        w: js::max(min_w, label.width + 2.0 * padding_x) + extra,
        h: js::max(min_h, label.height + 2.0 * PAD_Y) + stacked,
    }
}

fn figure(out: &mut SceneBuilder, part: PartId, kind: ActorKind, cx: f64, top: f64) {
    let shape = ShapeLook::new(Role::Surface, Role::Border, None).plain();
    let line = LineLook::new(Role::Border, None).open();
    let x = |dx: f64| Number::num(cx + dx);
    let y = |dy: f64| Number::num(top + dy);
    let path = PathProgram::default;
    let circle = |out: &mut SceneBuilder, dx: f64, dy: f64, radius: f64| {
        let centre = [Some(x(dx)), Some(y(dy))];
        out.element(part, Facet::Shape).circle_at(centre, Number::raw(radius)).look(shape.clone()).finish();
    };
    let stroked = |out: &mut SceneBuilder, d: PathProgram| {
        out.element(part, Facet::Shape).path(d).look(line.clone()).finish();
    };
    match kind {
        ActorKind::Actor => {
            circle(out, 0.0, 7.0, 6.0);
            let d = path()
                .m(x(0.0), y(13.0))
                .v(y(24.0))
                .m(x(-10.0), y(17.0))
                .h(x(10.0))
                .m(x(-8.0), y(34.0))
                .l(x(0.0), y(24.0))
                .l(x(8.0), y(34.0));
            stroked(out, d);
        }
        ActorKind::Boundary => {
            stroked(out, path().m(x(-19.0), y(6.0)).v(y(28.0)).m(x(-19.0), y(17.0)).h(x(-8.0)));
            circle(out, 3.0, 17.0, 11.0);
        }
        ActorKind::Control => {
            circle(out, 0.0, 21.0, 11.0);
            stroked(out, path().m(x(5.0), y(5.0)).l(x(-1.0), y(10.0)).l(x(5.0), y(15.0)));
        }
        ActorKind::Entity => {
            circle(out, 0.0, 15.0, 11.0);
            stroked(out, path().m(x(-11.0), y(30.0)).h(x(11.0)));
        }
        _ => {
            let (l, r) = (x(-14.0), x(14.0));
            let radii = [Number::raw(14.0), Number::raw(5.0)];
            let body =
                path().m(l, y(7.0)).v(y(27.0)).a(radii, false, [r, y(27.0)]).v(y(7.0)).a(radii, false, [l, y(7.0)]).z();
            out.element(part, Facet::Shape).path(body).look(shape.clone()).finish();
            stroked(out, path().m(l, y(7.0)).a(radii, false, [r, y(7.0)]));
        }
    }
}

/// Pele's `drawActor`: a participant whose bounding box has its top centre at (`cx`, `top`).
/// `ink` is the fill of a figure's label, which sits on whatever is behind the figure.
#[allow(clippy::too_many_arguments, reason = "the arguments of Pele's drawActor, plus the part it draws")]
pub(super) fn draw_actor(
    out: &mut SceneBuilder,
    part: PartId,
    kind: ActorKind,
    cx: f64,
    top: f64,
    size: ActorSize,
    label: &Label,
    ink: Option<Role>,
) {
    let ActorSize { w, h } = size;
    let text = || LabelLook::class("pele-label");
    if kind.is_figure() {
        figure(out, part, kind, cx, top);
        let at = Point { x: cx, y: top + GLYPH + GLYPH_GAP + label.height / 2.0 };
        out.label(part, Facet::Label, label, at, text().fill(ink.map(Paint::Role)));
        return;
    }
    let shape = ShapeLook::new(Role::Surface, Role::Border, None);
    let left = cx - w / 2.0;
    let rect = |out: &mut SceneBuilder, x: f64, y: f64, width: f64, height: f64| {
        out.element(part, Facet::Shape)
            .rect([Number::num(x), Number::num(y)], [Number::num(width), Number::num(height)], None)
            .look(shape.rounded())
            .finish();
    };
    match kind {
        ActorKind::Collections => {
            let (bw, bh) = (w - STACK, h - STACK);
            rect(out, left + STACK, top, bw, bh);
            rect(out, left, top + STACK, bw, bh);
            out.label(part, Facet::Label, label, Point { x: left + bw / 2.0, y: top + STACK + bh / 2.0 }, text());
        }
        ActorKind::Queue => {
            let (l, r) = (Number::num(left + CAP), Number::num(left + w - CAP));
            let (t, b) = (Number::num(top), Number::num(top + h));
            let radii = [Number::raw(CAP), Number::num(h / 2.0)];
            let body = PathProgram::default().m(l, t).h(r).a(radii, true, [r, b]).h(l).a(radii, true, [l, t]).z();
            out.element(part, Facet::Shape).path(body).look(shape.plain()).finish();
            let cap = PathProgram::default().m(r, t).a(radii, false, [r, b]);
            out.element(part, Facet::Shape).path(cap).look(LineLook::new(Role::Border, None).open()).finish();
            out.label(part, Facet::Label, label, Point { x: cx - CAP / 2.0, y: top + h / 2.0 }, text());
        }
        _ => {
            rect(out, left, top, w, h);
            out.label(part, Facet::Label, label, Point { x: cx, y: top + h / 2.0 }, text());
        }
    }
}
