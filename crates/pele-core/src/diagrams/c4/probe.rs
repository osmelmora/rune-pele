//! Entry points that let `tests/c4_layout.rs` hold `layout` and `route` to Pele on their own,
//! before anything is drawn. Each takes plain values and answers in plain values, so the types
//! of the port stay private to the crate.

use super::layout::{self, Texts};
use super::route::{self, Box, Ends, Form, Lanes};
use crate::measure::ArialMetrics;
use crate::{ParseOptions, RenderError, js};

/// A box as the route test gives it: its corner, its size and the name of its shape.
#[derive(Clone, Debug)]
pub struct ProbeBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub form: &'static str,
}

/// One call of `route`, with the boxes named by their place in the list.
#[derive(Clone, Debug)]
pub struct RouteCall {
    pub a: usize,
    pub b: usize,
    pub shift: f64,
    /// The shapes to steer clear of, or `None` to skip that check.
    pub others: Option<Vec<usize>>,
    pub start_trim: f64,
    pub end_trim: f64,
}

/// What `route` answers: the path as written, the start and end tips as x, y, dx, dy, the
/// stretch for the label as x0, y0, x1, y1, how the label leans and its room, and the corners.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteAnswer {
    pub d: String,
    pub start: [f64; 4],
    pub end: [f64; 4],
    pub stretch: [f64; 4],
    pub nx: f64,
    pub ny: f64,
    pub lean: f64,
    pub room: f64,
    pub points: Vec<f64>,
}

fn form(name: &str) -> Form {
    match name {
        "person" => Form::Person,
        "cyl" => Form::Cyl,
        "h-cyl" => Form::HCyl,
        _ => Form::Rect,
    }
}

/// Frames `frames` in one set of lanes, then routes each call in turn through it, as a drawing
/// routes its relations. Answers each route, and how many routes were squeezed.
pub fn route(boxes: &[ProbeBox], frames: &[usize], calls: &[RouteCall]) -> (Vec<RouteAnswer>, usize) {
    let boxes: Vec<Box> = boxes.iter().map(|b| Box { x: b.x, y: b.y, w: b.w, h: b.h, form: form(b.form) }).collect();
    let mut lanes = Lanes::default();
    for &frame in frames {
        lanes.frame(&boxes[frame]);
    }
    let answers = calls
        .iter()
        .map(|call| {
            let ends =
                Ends { boxes: &boxes, a: call.a, b: call.b, start_trim: call.start_trim, end_trim: call.end_trim };
            let r = route::route(&ends, call.shift, call.others.as_deref(), &mut lanes);
            let mut d = String::new();
            crate::svg::write_path(&r.path.d, &mut d);
            let tip = |tip: crate::graph::edges::Tip| [tip.x, tip.y, tip.dx, tip.dy];
            RouteAnswer {
                d,
                start: tip(r.path.start),
                end: tip(r.path.end),
                stretch: [r.x0, r.y0, r.x1, r.y1],
                nx: r.nx,
                ny: r.ny,
                lean: r.lean,
                room: r.room,
                points: r.points,
            }
        })
        .collect();
    (answers, lanes.squeezed)
}

/// A laid out shape: its place among the shapes, its form, its box as x, y, w, h, the height of
/// its text, and the width and height of each of its labels.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeProbe {
    pub shape: usize,
    pub form: &'static str,
    pub frame: [f64; 4],
    pub text_h: f64,
    pub labels: Vec<[f64; 2]>,
}

/// A laid out boundary that is drawn, in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupProbe {
    pub boundary: usize,
    pub frame: [f64; 4],
    pub head: f64,
    pub head_w: f64,
    pub head_x: f64,
    pub kids: Vec<usize>,
    pub labels: Vec<[f64; 2]>,
    pub nodes: Vec<NodeProbe>,
}

/// Pele's `layoutC4` of a source, at `font_size`, with `room` and `gap` as a tight drawing gives
/// them. A source that is no C4 diagram, or that Pele rejects, answers its error.
pub fn layout(source: &str, font_size: f64, room: Option<f64>, gap: f64) -> Result<Vec<GroupProbe>, RenderError> {
    let options = ParseOptions::default();
    let front = crate::front(source, &options)?;
    let origin = crate::Origin { source };
    let model = super::parse(&front.context(&origin, options.limits))?;
    let texts = Texts { measurer: &ArialMetrics, size: font_size, small: js::round(font_size * 0.8125) };
    let laid = layout::layout(&model, &texts, room, gap);
    let frame = |b: &Box| [b.x, b.y, b.w, b.h];
    let sizes = |labels: &[crate::measure::Label]| labels.iter().map(|label| [label.width, label.height]).collect();
    Ok(laid
        .order
        .iter()
        .map(|&g| {
            let group = &laid.groups[g];
            GroupProbe {
                boundary: g,
                frame: frame(&laid.boxes[group.at]),
                head: group.head,
                head_w: group.head_w,
                head_x: group.head_x,
                kids: group.kids.clone(),
                labels: sizes(&group.labels),
                nodes: group
                    .nodes
                    .iter()
                    .map(|node| NodeProbe {
                        shape: node.shape,
                        form: laid.boxes[node.at].form.name(),
                        frame: frame(&laid.boxes[node.at]),
                        text_h: node.text_h,
                        labels: sizes(&node.labels),
                    })
                    .collect(),
            }
        })
        .collect())
}
