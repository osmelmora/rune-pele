//! Entry points that let `tests/venn_layout.rs` hold the venn geometry to Pele on its own, before
//! anything is scaled or drawn. Each answers in the words of an oracle op, every number as the
//! bits of its double, so the types of the port stay private to the crate.

use super::layout::{self, Circle, Libm, OutlineStep, Trig};
use crate::catalog::{DrawContext, PreparedModel};
use crate::layout::num::hypot;
use crate::measure::ArialMetrics;
use crate::scene::SourcePalette;
use crate::{LayoutOptions, ParityScene, ParseOptions, RenderError, Rendered, prepare, svg};
use indexmap::IndexMap;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// One call the layout makes of a `Math` function [`Trig`] answers: its name, and the bits of
/// its arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TrigCall {
    pub op: &'static str,
    pub args: Vec<u64>,
}

/// Answers each call from a table of Node's answers, and libm's where the table has none,
/// noting each call it had to answer that way.
struct Answered<'a> {
    answers: &'a HashMap<TrigCall, f64>,
    missed: RefCell<Vec<TrigCall>>,
}

impl Answered<'_> {
    fn answer(&self, op: &'static str, args: &[f64], fallback: impl Fn() -> f64) -> f64 {
        let call = TrigCall { op, args: args.iter().map(|x| x.to_bits()).collect() };
        match self.answers.get(&call) {
            Some(&answer) => answer,
            None => {
                self.missed.borrow_mut().push(call);
                fallback()
            }
        }
    }
}

impl Trig for Answered<'_> {
    fn sin(&self, x: f64) -> f64 {
        self.answer("sin", &[x], || Libm.sin(x))
    }

    fn cos(&self, x: f64) -> f64 {
        self.answer("cos", &[x], || Libm.cos(x))
    }

    fn acos(&self, x: f64) -> f64 {
        self.answer("acos", &[x], || Libm.acos(x))
    }

    fn atan2(&self, y: f64, x: f64) -> f64 {
        self.answer("atan2", &[y, x], || Libm.atan2(y, x))
    }
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn quoted(text: &[u16]) -> String {
    let mut out = String::new();
    super::json::string(&mut out, text);
    out
}

fn written(steps: Option<Vec<OutlineStep>>) -> String {
    let mut d = String::new();
    let pair = |d: &mut String, [x, y]: [f64; 2]| {
        let _ = write!(d, "{},{}", bits(x), bits(y));
    };
    for step in steps.unwrap_or_default() {
        match step {
            OutlineStep::Move(to) => {
                d.push('M');
                pair(&mut d, to);
            }
            OutlineStep::Line(to) => {
                d.push('L');
                pair(&mut d, to);
            }
            OutlineStep::Arc { r, large, end } => {
                let _ = write!(d, "A{},{} 0 {} 1 ", bits(r), bits(r), u8::from(large));
                pair(&mut d, end);
            }
            OutlineStep::Close => d.push('Z'),
        }
    }
    d
}

/// The oracle's `venn_geometry`: the circles, which circles touch, and for each region its
/// circles, the circles that cut into it, whether its text goes across them, its spot and its
/// outline. `None` for a source that is no venn diagram Pele parses.
pub fn geometry(source: &str) -> Option<String> {
    geometry_with(source, &Libm)
}

/// [`geometry`] with each `Math` call answered from `answers`, which hold Node's answers, and
/// the calls the table did not answer.
pub fn geometry_answered(source: &str, answers: &HashMap<TrigCall, f64>) -> (Option<String>, Vec<TrigCall>) {
    let trig = Answered { answers, missed: RefCell::new(Vec::new()) };
    let geometry = geometry_with(source, &trig);
    (geometry, trig.missed.into_inner())
}

fn geometry_with(source: &str, trig: &dyn Trig) -> Option<String> {
    let prepared = prepare(source, ParseOptions::default()).ok()?;
    let PreparedModel::Venn(model) = &prepared.model else {
        return None;
    };
    let circles = layout::layout_circles(&model.subsets, trig);
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); circles.len()];
    for a in 0..circles.len() {
        for b in a + 1..circles.len() {
            if hypot(circles[a].x - circles[b].x, circles[a].y - circles[b].y) < circles[a].r + circles[b].r {
                neighbors[a].push(b);
                neighbors[b].push(a);
            }
        }
    }
    let mut stated = IndexMap::new();
    for subset in &model.subsets {
        stated.insert(&subset.sets, &subset.sets);
    }
    let mut regions = Vec::new();
    for given in stated.values() {
        let mut seen = HashSet::new();
        let inside: Vec<usize> = given
            .iter()
            .filter(|id| seen.insert(*id))
            .map(|id| circles.iter().position(|c| &c.id == id).expect("every set has a circle"))
            .collect();
        let mut smallest = inside[0];
        for &i in &inside {
            if circles[i].r < circles[smallest].r {
                smallest = i;
            }
        }
        let others: Vec<usize> = neighbors[smallest].iter().copied().filter(|i| !inside.contains(i)).collect();
        let mut spot = layout::find_spot(&circles, &inside, &others, trig);
        let across = !others.is_empty() && spot.margin < circles[smallest].r * 0.2;
        if across {
            spot = layout::find_spot(&circles, &inside, &[], trig);
        }
        let outline = if inside.len() > 1 {
            let shared: Vec<Circle> = inside.iter().map(|&i| circles[i].clone()).collect();
            written(layout::region_path(&shared, trig))
        } else {
            String::new()
        };
        let list = |list: &[usize]| list.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
        regions.push(format!(
            "{{\"inside\":[{}],\"others\":[{}],\"across\":{across},\"spot\":[\"{}\",\"{}\",\"{}\"],\"outline\":\"{outline}\"}}",
            list(&inside),
            list(&others),
            bits(spot.x),
            bits(spot.y),
            bits(spot.margin)
        ));
    }
    let circles: Vec<String> = circles
        .iter()
        .map(|c| format!("[{},\"{}\",\"{}\",\"{}\"]", quoted(&c.id), bits(c.x), bits(c.y), bits(c.r)))
        .collect();
    let neighbors: Vec<String> = neighbors
        .iter()
        .map(|list| format!("[{}]", list.iter().map(usize::to_string).collect::<Vec<_>>().join(",")))
        .collect();
    Some(format!(
        "{{\"circles\":[{}],\"neighbors\":[{}],\"regions\":[{}]}}",
        circles.join(","),
        neighbors.join(","),
        regions.join(",")
    ))
}

/// Pele's render of a source with its default options, with each `Math` call of the layout
/// answered from `answers`, which hold Node's answers, and the calls the table did not answer.
pub fn render_answered(
    source: &str,
    answers: &HashMap<TrigCall, f64>,
) -> (Result<Rendered, RenderError>, Vec<TrigCall>) {
    let trig = Answered { answers, missed: RefCell::new(Vec::new()) };
    let rendered = prepare(source, ParseOptions::default()).and_then(|prepared| {
        let PreparedModel::Venn(model) = &prepared.model else {
            return Err(RenderError::NotPorted);
        };
        let policy = prepared.found.kind.port().expect("venn is ported");
        let options = LayoutOptions::default();
        let cx = DrawContext {
            kind: prepared.found.kind,
            measure: &ArialMetrics,
            options: &options,
            config: &prepared.config,
            palette: &SourcePalette::from_config(policy.palette, &prepared.config),
            responsive: true,
            limits: prepared.limits,
        };
        // Node answers what Pele computes, so the drawing is one Pele makes.
        svg::write(&ParityScene { scene: super::render::draw(model, &cx, &trig)? })
    });
    (rendered, trig.missed.into_inner())
}

/// The oracle's `venn_region`: the outline of the region circles given as x, y and r share,
/// every number as its bits.
pub fn region(circles: &[[f64; 3]]) -> String {
    let circles: Vec<Circle> = circles.iter().map(|&[x, y, r]| Circle { id: Default::default(), x, y, r }).collect();
    written(layout::region_path(&circles, &Libm))
}

/// `Math.acos` as the venn layout computes it.
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

/// `Math.atan2` as the venn layout computes it.
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}
