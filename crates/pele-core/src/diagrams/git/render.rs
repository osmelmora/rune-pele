//! Pele's `renderGit` (`src/diagrams/git/render.ts`): commits placed along the history, one lane
//! per branch, with the connectors between lanes routed clear of the commits and their labels.

use super::{Commit, CommitType, Direction, GitModel};
use crate::catalog::{Diagram, DrawContext, ParseContext};
use crate::input::{JsObject, JsText, JsValue};
use crate::measure::{Emphasis, Label, layout_label, plain_label};
use crate::scene::{
    Entity, EntityName, Facet, LabelLook, LayerSpec, LineCap, LineJoin, Number, Paint, PartId, PathProgram, Point,
    Role, Scene, SceneBuilder, SeriesSlot, Size, Style, TitleLayout,
};
use crate::{RenderError, fit, js};
use std::collections::HashMap;
use std::f64::consts::FRAC_1_SQRT_2 as HALF;

const DOT: f64 = 7.0;
const STEP: f64 = 44.0;
const LANE: f64 = 44.0;
// Shorter than a step, so a curve between lanes never runs through a commit on a lane it crosses.
const CURVE: f64 = 36.0;
const GAP: f64 = 8.0;
const LABEL_GAP: f64 = 20.0;
const TAIL: f64 = 20.0;
const NOTCH: f64 = 7.0;
const TAG_GAP: f64 = 6.0;

/// A commit as it is drawn: its lane, its place along the history, and its measured labels.
struct View<'a> {
    commit: &'a Commit,
    lane: usize,
    m: f64,
    label: Option<Label>,
    tags: Vec<Label>,
}

fn flag(config: &JsObject, key: &str, fallback: bool) -> bool {
    match config.get("gitGraph").and_then(JsValue::as_record).and_then(|git| git.get(key)) {
        Some(&JsValue::Bool(value)) => value,
        _ => fallback,
    }
}

fn tag_width(tag: &Label) -> f64 {
    NOTCH + 4.0 + tag.width + 6.0
}

fn series(lane: usize) -> Paint {
    Paint::Series(SeriesSlot::new(lane))
}

/// The edges that join two commits, as they are drawn.
struct EdgeDraw {
    class: &'static str,
    path: PathProgram,
    stroke: usize,
    dashed: bool,
    from: usize,
    to: usize,
    kind: &'static str,
}

/// The bounds of what is drawn, grown one box at a time.
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }
}

fn draw(model: &GitModel, cx: &DrawContext<'_>, direction: Direction) -> Result<Scene, RenderError> {
    let size = cx.options.typography.normal.size;
    let small = js::round(size * 0.75);
    let pad = cx.options.padding_or_default();
    let plain = Emphasis::default();

    let show_branches = flag(cx.config, "showBranches", true);
    let show_commit_label = flag(cx.config, "showCommitLabel", true);
    let rotate = flag(cx.config, "rotateCommitLabel", true);
    let parallel = flag(cx.config, "parallelCommits", false);
    let vertical = direction != Direction::LeftRight;
    let flip = direction == Direction::BottomTop;

    let lane_of: HashMap<&JsText, usize> = model.lanes.iter().enumerate().map(|(index, name)| (name, index)).collect();
    let lane_count = model.lanes.len();
    let medium = js::round(size * 0.875);
    // Mermaid writes branch names, tags, and commit ids as plain text, so tags in them show as written.
    let names: Vec<Label> = model
        .lanes
        .iter()
        .map(|name| plain_label(Some(if show_branches { name } else { &[] }), cx.measure, medium, 4000.0, plain))
        .collect();

    let mut sorted: Vec<&Commit> = model.commits.values().collect();
    sorted.sort_by_key(|commit| commit.seq);
    let mut views: Vec<View<'_>> = Vec::with_capacity(sorted.len());
    let mut by_id: HashMap<&JsText, usize> = HashMap::with_capacity(sorted.len());
    for commit in sorted {
        let labelled = show_commit_label
            && commit.kind != CommitType::CherryPick
            && (commit.kind != CommitType::Merge || commit.custom_id == Some(true));
        let tags = commit
            .tags
            .iter()
            .map(|tag| plain_label(Some(tag), cx.measure, small, 4000.0, plain))
            .filter(|label| !label.lines.is_empty())
            .collect();
        by_id.insert(&commit.id, views.len());
        views.push(View {
            commit,
            lane: lane_of.get(&commit.branch).copied().unwrap_or(0),
            m: 0.0,
            label: labelled.then(|| plain_label(Some(&commit.id), cx.measure, small, 4000.0, plain)),
            tags,
        });
    }
    let parent_of = |id: &JsText| by_id.get(id).copied();

    // Room each commit needs: along the main axis on either side, and across it towards the
    // previous lane (`lo`) and the next one (`hi`). Tags sit on one side of a lane and ids on the other.
    let count = views.len();
    let mut half = vec![0.0; count];
    let mut tag_half = vec![0.0; count];
    let mut lo = vec![DOT; lane_count];
    let mut hi = vec![DOT; lane_count];
    for (k, view) in views.iter().enumerate() {
        let (mut tags_main, mut tags_cross) = (0.0, 0.0);
        for tag in &view.tags {
            let w = tag_width(tag);
            if vertical {
                tags_main = js::max(tags_main, tag.height);
                tags_cross += w + 4.0;
            } else {
                tags_main = js::max(tags_main, w);
                tags_cross += tag.height + 3.0;
            }
        }
        let (mut label_main, mut label_cross) = (0.0, 0.0);
        if let Some(label) = view.label.as_ref().filter(|label| !label.lines.is_empty()) {
            if rotate {
                label_cross = 3.0 + HALF * (label.width + label.height);
            } else {
                label_main = if vertical { label.height } else { label.width };
                label_cross = 6.0 + if vertical { label.width } else { label.height };
            }
        }
        half[k] = js::max(js::max(DOT, tags_main / 2.0), label_main / 2.0);
        tag_half[k] = tags_main / 2.0;
        let tag_side = DOT + if view.tags.is_empty() { 0.0 } else { TAG_GAP + tags_cross };
        let label_side = DOT + label_cross;
        let lane = view.lane;
        lo[lane] = js::max(lo[lane], if vertical { label_side } else { tag_side });
        hi[lane] = js::max(hi[lane], if vertical { tag_side } else { label_side });
    }

    // In a left-to-right graph the tags stand over their commit. A connector that comes down from
    // a lane above joins the commit's lane this far before the commit, clear of the tags, and the
    // commit is placed far enough along to leave room for that.
    let mut lead = vec![0.0; count];
    if !vertical {
        for (k, view) in views.iter().enumerate() {
            if tag_half[k] == 0.0 {
                continue;
            }
            for id in &view.commit.parents {
                if let Some(parent) = parent_of(id)
                    && views[parent].lane < view.lane
                {
                    lead[k] = tag_half[k] + 4.0;
                }
            }
        }
    }
    let space = |p: usize, k: usize| {
        let turn = if lead[k] > 0.0 { half[p] + CURVE + lead[k] } else { 0.0 };
        js::max(js::max(STEP, half[p] + half[k] + GAP), turn)
    };

    for k in 0..count {
        let m = if !parallel {
            if k == 0 { 0.0 } else { views[k - 1].m + space(k - 1, k) }
        } else {
            // Mermaid's `parallelCommits`: a commit sits one step after its furthest parent, so
            // branches that grow side by side share positions instead of taking turns.
            let mut m = 0.0;
            for id in &views[k].commit.parents {
                let Some(parent) = parent_of(id).filter(|&parent| parent < k) else { continue };
                m = js::max(m, views[parent].m + space(parent, k));
            }
            m
        };
        views[k].m = m;
    }
    let mut end = 0.0;
    for view in &views {
        end = js::max(end, view.m);
    }

    let mut cross = vec![0.0; lane_count];
    for i in 0..lane_count {
        let name = &names[i];
        let reach = if name.lines.is_empty() {
            0.0
        } else if vertical {
            name.width / 2.0 + 8.0
        } else {
            name.height / 2.0 + 2.0
        };
        lo[i] = js::max(lo[i], reach);
        hi[i] = js::max(hi[i], reach);
        if i > 0 {
            cross[i] = cross[i - 1] + js::max(LANE, hi[i - 1] + lo[i] + GAP);
        }
    }

    let px = |m: f64, c: f64| if vertical { c } else { m };
    let py = |m: f64, c: f64| {
        if vertical { if flip { -m } else { m } } else { c }
    };
    let point = |m: f64, c: f64| [Number::num(px(m, c)), Number::num(py(m, c))];

    let mut bounds =
        Bounds { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY };

    // Each lane's line, and where its name sits.
    struct LaneDraw {
        line: PathProgram,
        tab: Option<(Point, f64, f64)>,
    }
    let mut lanes = Vec::new();
    if show_branches {
        for (i, name) in names.iter().enumerate() {
            let c = cross[i];
            let w = name.width + 16.0;
            let h = name.height + 4.0;
            let along = if vertical { h } else { w };
            let from = if name.lines.is_empty() { -LABEL_GAP } else { -LABEL_GAP - along };
            let centre = Point { x: px(from + along / 2.0, c), y: py(from + along / 2.0, c) };
            bounds.grow(px(-LABEL_GAP, c), py(-LABEL_GAP, c), px(-LABEL_GAP, c), py(-LABEL_GAP, c));
            bounds.grow(px(end + TAIL, c), py(end + TAIL, c), px(end + TAIL, c), py(end + TAIL, c));
            let [x0, y0] = point(-LABEL_GAP, c);
            let [x1, y1] = point(end + TAIL, c);
            let line = PathProgram::default().m(x0, y0).l(x1, y1);
            let tab = (!name.lines.is_empty()).then(|| {
                bounds.grow(centre.x - w / 2.0, centre.y - h / 2.0, centre.x + w / 2.0, centre.y + h / 2.0);
                (centre, w, h)
            });
            lanes.push(LaneDraw { line, tab });
        }
    }

    // Positions of the commits on each lane, to tell whether a connector may run along it.
    let mut taken: Vec<Vec<f64>> = vec![Vec::new(); lane_count];
    for view in &views {
        taken[view.lane].push(view.m);
    }
    if parallel {
        for list in &mut taken {
            list.sort_by(|a, b| crate::layout::num::by_difference(a - b));
        }
    }
    let busy = |lane: usize, from: f64, to: f64| {
        let list = &taken[lane];
        let (mut low, mut high) = (0, list.len());
        while low < high {
            let mid = (low + high) >> 1;
            if list[mid] <= from {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low < list.len() && list[low] < to
    };

    let mut curves = Vec::new();
    let mut lines = Vec::new();
    for (index, view) in views.iter().enumerate() {
        let commit = view.commit;
        let c2 = cross[view.lane];
        let m2 = view.m;
        for (j, id) in commit.parents.iter().enumerate() {
            let Some(p) = parent_of(id).filter(|&p| p != index) else { continue };
            let parent = &views[p];
            let c1 = cross[parent.lane];
            let m1 = parent.m;
            let [x, y] = point(m1, c1);
            let mut d = PathProgram::default().m(x, y);
            if parent.lane == view.lane {
                let [x, y] = point(m2, c2);
                let path = d.l(x, y);
                lines.push(EdgeDraw {
                    class: "pele-edge",
                    path,
                    stroke: view.lane,
                    dashed: false,
                    from: p,
                    to: index,
                    kind: "parent",
                });
                continue;
            }
            let merged = j > 0 && commit.kind == CommitType::Merge;
            let picked = j > 0 && !merged;
            if m2 <= m1 {
                let mid = (m1 + m2) / 2.0;
                d = d.c(point(mid, c1), point(mid, c2), point(m2, c2));
            } else if !merged && !busy(view.lane, m1, m2) {
                // Leaves the parent at once and runs along the child's lane.
                let s = js::min(CURVE, m2 - m1);
                let [x, y] = point(m2, c2);
                d = d.c(point(m1 + s / 2.0, c1), point(m1 + s / 2.0, c2), point(m1 + s, c2)).l(x, y);
            } else {
                // Turns in just before the child, or before its tags when it comes from their side.
                let to = if parent.lane < view.lane && m2 - lead[index] > m1 { m2 - lead[index] } else { m2 };
                let mut s = js::min(CURVE, to - m1);
                let mut along = c1;
                if !merged || busy(parent.lane, m1, m2) {
                    // Neither lane is free to run along. Between two lanes, past the ids of one and
                    // before the tags of the next, a strip stays empty, and the connector takes that.
                    along = if c1 > c2 { c2 + hi[view.lane] + GAP / 2.0 } else { c2 - lo[view.lane] - GAP / 2.0 };
                    s = js::min(CURVE, (to - m1) / 2.0);
                    d = d.c(point(m1 + s / 2.0, c1), point(m1 + s / 2.0, along), point(m1 + s, along));
                }
                let [x, y] = point(to - s, along);
                d = d.l(x, y).c(point(to - s / 2.0, along), point(to - s / 2.0, c2), point(to, c2));
                if to < m2 {
                    let [x, y] = point(m2, c2);
                    d = d.l(x, y);
                }
            }
            curves.push(EdgeDraw {
                class: if merged {
                    "pele-edge pele-edge-merge"
                } else if picked {
                    "pele-edge pele-edge-cherry-pick"
                } else {
                    "pele-edge"
                },
                path: d,
                stroke: if merged { parent.lane } else { view.lane },
                dashed: picked,
                from: p,
                to: index,
                kind: if merged {
                    "merge"
                } else if picked {
                    "cherry-pick"
                } else {
                    "parent"
                },
            });
        }
    }

    // Where each commit is, and the boxes of its labels, which grow the bounds in this order.
    for view in &views {
        let x = px(view.m, cross[view.lane]);
        let y = py(view.m, cross[view.lane]);
        bounds.grow(x - DOT - 1.0, y - DOT - 1.0, x + DOT + 1.0, y + DOT + 1.0);
        if let Some(label) = view.label.as_ref().filter(|label| !label.lines.is_empty()) {
            let (w, h) = (label.width, label.height);
            if rotate {
                let k = (HALF * h) / 2.0;
                let ax = if vertical { x - DOT - 3.0 - k } else { x + 3.0 };
                let ay = if vertical { y } else { y + DOT + 3.0 + k };
                let reach = HALF * w + k;
                bounds.grow(
                    ax - reach,
                    if flip { ay - reach } else { ay - k },
                    ax + k,
                    if flip { ay + k } else { ay + reach },
                );
            } else {
                let lx = if vertical { x - DOT - 6.0 - w / 2.0 } else { x };
                let ly = if vertical { y } else { y + DOT + 6.0 + h / 2.0 };
                bounds.grow(lx - w / 2.0, ly - h / 2.0, lx + w / 2.0, ly + h / 2.0);
            }
        }
        let mut offset = DOT + TAG_GAP;
        for tag in tag_order(&view.tags, vertical) {
            let w = tag_width(tag);
            let h = tag.height;
            let left = if vertical { x + offset } else { x - w / 2.0 };
            let ty = if vertical { y } else { y - offset - h / 2.0 };
            offset += if vertical { w + 4.0 } else { h + 3.0 };
            bounds.grow(left, ty - h / 2.0, left + w, ty + h / 2.0);
        }
    }

    if bounds.min_x > bounds.max_x {
        bounds.grow(0.0, 0.0, 0.0, 0.0);
    }
    let title =
        layout_label(model.title.as_deref().unwrap_or_default(), false, cx.measure, size, 4000.0, Emphasis::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let spread = bounds.max_x - bounds.min_x;
    let inner = js::max(spread, title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (bounds.max_y - bounds.min_y + title_height + 2.0 * pad).ceil();
    let dx = pad - bounds.min_x + (inner - spread) / 2.0;
    let dy = pad + title_height - bounds.min_y;

    let mut out = SceneBuilder::new(cx, Size { width, height }, model.accessible.clone());
    let title_at = Point { x: width / 2.0, y: pad + title.height / 2.0 };
    out.title(&TitleLayout { label: title, at: title_at });
    let commit_parts: Vec<PartId> = views
        .iter()
        .map(|view| {
            let commit = view.commit;
            let tooltip = written(commit).then_some(&commit.message);
            out.part(Entity::node(&commit.id).tooltip(tooltip))
        })
        .collect();

    out.layer(LayerSpec::group().translate(Point { x: dx, y: dy }).keep_empty(), |out| {
        let branches = LayerSpec::class("pele-branches").fill(Paint::None).font_size(Number::raw(medium));
        out.layer(branches, |out| {
            for (i, lane) in lanes.iter().enumerate() {
                let part = out.part(Entity::group(&model.lanes[i]));
                out.layer(LayerSpec::class("pele-branch").part(part, Facet::Shape).data_id(), |out| {
                    out.element(part, Facet::Shape)
                        .class("pele-lane")
                        .path(lane.line.clone())
                        .stroke(series(i))
                        .dash(&[Number::raw(2.0), Number::raw(4.0)])
                        .finish();
                    if let Some((centre, w, h)) = lane.tab {
                        out.element(part, Facet::Label)
                            .rect(
                                [Number::num(centre.x - w / 2.0), Number::num(centre.y - h / 2.0)],
                                [Number::num(w), Number::num(h)],
                                None,
                            )
                            .fill(series(i))
                            .style(Some(Style::rounded(None)))
                            .finish();
                        let ink = match cx.palette.ink(SeriesSlot::new(i)) {
                            Some(_) => Paint::OnSeries(SeriesSlot::new(i)),
                            None => Paint::Role(Role::Background),
                        };
                        let look = LabelLook::class("pele-label").fill(Some(ink));
                        out.label(part, Facet::Label, &names[i], centre, look);
                    }
                });
            }
        });

        let edges = LayerSpec::class("pele-edges").fill(Paint::None).stroke_width(Number::raw(2.0)).cap(LineCap::Round);
        out.layer(edges, |out| {
            let mut seen: HashMap<(usize, usize, &str), u32> = HashMap::new();
            for edge in curves.iter().chain(&lines) {
                let rank = seen.entry((edge.from, edge.to, edge.kind)).or_insert(0);
                let name = EntityName::Between {
                    from: views[edge.from].commit.id.to_vec(),
                    to: views[edge.to].commit.id.to_vec(),
                    kind: edge.kind.to_owned(),
                };
                let part = out.part(Entity::edge(&JsText::default(), name, *rank));
                *rank += 1;
                out.relation(part, commit_parts[edge.from], commit_parts[edge.to]);
                let shape = out
                    .element(part, Facet::Shape)
                    .class(edge.class)
                    .path(edge.path.clone())
                    .stroke(series(edge.stroke));
                if edge.dashed {
                    shape.dash(&[Number::raw(4.0), Number::raw(3.0)]).finish();
                } else {
                    shape.finish();
                }
            }
        });

        let commits = LayerSpec::class("pele-commits").font_size(Number::raw(small)).fill(Paint::Role(Role::Muted));
        out.layer(commits, |out| {
            for (index, view) in views.iter().enumerate() {
                draw_commit(out, commit_parts[index], view, cross[view.lane], &px, &py, rotate, vertical, flip);
            }
        });
    });
    out.finish()
}

/// Merges and cherry-picks carry a message Mermaid makes up; only a written one is worth showing.
fn written(commit: &Commit) -> bool {
    commit.kind < CommitType::Merge && !commit.message.is_empty()
}

/// Beside the lane the tags form a row; above it they stack, the first one furthest out.
fn tag_order(tags: &[Label], vertical: bool) -> Box<dyn Iterator<Item = &Label> + '_> {
    if vertical { Box::new(tags.iter()) } else { Box::new(tags.iter().rev()) }
}

#[allow(clippy::too_many_arguments, reason = "the commit and the placement rules of one draw")]
fn draw_commit(
    out: &mut SceneBuilder,
    part: PartId,
    view: &View<'_>,
    c: f64,
    px: &dyn Fn(f64, f64) -> f64,
    py: &dyn Fn(f64, f64) -> f64,
    rotate: bool,
    vertical: bool,
    flip: bool,
) {
    let commit = view.commit;
    let x = px(view.m, c);
    let y = py(view.m, c);
    let kind = commit.custom_type.unwrap_or(commit.kind);
    let merge = if commit.kind == CommitType::Merge && kind != CommitType::Merge { " pele-commit-merge" } else { "" };
    let class = format!("pele-commit pele-commit-{}{merge}", kind.class());
    out.layer(LayerSpec::class(&class).part(part, Facet::Shape).data_id(), |out| {
        if written(commit) {
            out.tooltip(part);
        }
        symbol(out, part, kind, x, y, series(view.lane));

        if let Some(label) = view.label.as_ref().filter(|label| !label.lines.is_empty()) {
            let (w, h) = (label.width, label.height);
            let back = |out: &mut SceneBuilder, lx: f64, ly: f64| {
                out.element(part, Facet::Label)
                    .rect(
                        [Number::num(lx - w / 2.0 - 2.0), Number::num(ly - h / 2.0 + 2.0)],
                        [Number::num(w + 4.0), Number::num(h - 4.0)],
                        Some(Number::raw(3.0)),
                    )
                    .fill(Paint::Role(Role::Background))
                    .finish();
            };
            if rotate {
                // The text ends beside the commit and trails back the way the history came.
                let k = (HALF * h) / 2.0;
                let ax = if vertical { x - DOT - 3.0 - k } else { x + 3.0 };
                let ay = if vertical { y } else { y + DOT + 3.0 + k };
                let turn = LayerSpec::class("pele-commit-label")
                    .rotate(if flip { 45.0 } else { -45.0 }, Point { x: ax, y: ay });
                out.layer(turn, |out| {
                    back(out, ax - w / 2.0, ay);
                    out.label(part, Facet::Label, label, Point { x: ax - w / 2.0, y: ay }, LabelLook::default());
                });
            } else {
                let lx = if vertical { x - DOT - 6.0 - w / 2.0 } else { x };
                let ly = if vertical { y } else { y + DOT + 6.0 + h / 2.0 };
                out.layer(LayerSpec::class("pele-commit-label"), |out| {
                    back(out, lx, ly);
                    out.label(part, Facet::Label, label, Point { x: lx, y: ly }, LabelLook::default());
                });
            }
        }

        let mut offset = DOT + TAG_GAP;
        for tag in tag_order(&view.tags, vertical) {
            let w = tag_width(tag);
            let h = tag.height;
            let left = if vertical { x + offset } else { x - w / 2.0 };
            let ty = if vertical { y } else { y - offset - h / 2.0 };
            offset += if vertical { w + 4.0 } else { h + 3.0 };
            out.layer(LayerSpec::class("pele-tag"), |out| {
                let outline = PathProgram::default()
                    .m(Number::num(left), Number::num(ty))
                    .l(Number::num(left + NOTCH), Number::num(ty - h / 2.0))
                    .h(Number::num(left + w))
                    .v(Number::num(ty + h / 2.0))
                    .h(Number::num(left + NOTCH))
                    .z();
                out.element(part, Facet::Stereotype)
                    .path(outline)
                    .fill(Paint::Role(Role::Surface))
                    .stroke(Paint::Role(Role::Border))
                    .join(LineJoin::Round)
                    .finish();
                out.element(part, Facet::Stereotype)
                    .circle_at([Some(Number::num(left + NOTCH)), Some(Number::num(ty))], Number::raw(1.5))
                    .fill(Paint::Role(Role::Border))
                    .finish();
                let at = Point { x: left + NOTCH + 4.0 + tag.width / 2.0, y: ty };
                let look = LabelLook::default().fill(Some(Paint::Role(Role::Foreground)));
                out.label(part, Facet::Stereotype, tag, at, look);
            });
        }
    });
}

/// Pele's `symbol`: the mark a commit of each type is drawn as.
fn symbol(out: &mut SceneBuilder, part: PartId, kind: CommitType, x: f64, y: f64, colour: Paint) {
    let at = [Some(Number::num(x)), Some(Number::num(y))];
    let background = Paint::Role(Role::Background);
    match kind {
        CommitType::Highlight => {
            out.element(part, Facet::Shape)
                .rect(
                    [Number::num(x - DOT - 1.0), Number::num(y - DOT - 1.0)],
                    [Number::raw(2.0 * DOT + 2.0), Number::raw(2.0 * DOT + 2.0)],
                    Some(Number::raw(2.0)),
                )
                .fill(colour)
                .finish();
            return;
        }
        CommitType::Merge => {
            out.element(part, Facet::Shape)
                .circle_at(at, Number::raw(DOT - 1.0))
                .fill(background)
                .stroke(colour)
                .stroke_width(Number::raw(2.0))
                .finish();
            out.element(part, Facet::Shape).circle_at(at, Number::raw(2.5)).fill(colour).finish();
            return;
        }
        CommitType::Normal | CommitType::Reverse | CommitType::CherryPick => {}
    }
    out.element(part, Facet::Shape).circle_at(at, Number::raw(DOT)).fill(colour).finish();
    match kind {
        CommitType::Reverse => {
            let cross = PathProgram::default()
                .m(Number::num(x - 3.0), Number::num(y - 3.0))
                .l_by(Number::raw(6.0), Number::raw(6.0))
                .m(Number::num(x - 3.0), Number::num(y + 3.0))
                .l_by(Number::raw(6.0), Number::raw(-6.0));
            out.element(part, Facet::Shape)
                .path(cross)
                .fill(Paint::None)
                .stroke(background)
                .stroke_width(Number::raw(1.5))
                .cap(LineCap::Round)
                .finish();
        }
        CommitType::CherryPick => {
            let stem = PathProgram::default()
                .m(Number::num(x - 2.6), Number::num(y + 1.0))
                .l(Number::num(x + 0.8), Number::num(y - 4.2))
                .l(Number::num(x + 2.6), Number::num(y + 1.0));
            out.element(part, Facet::Shape)
                .path(stem)
                .fill(Paint::None)
                .stroke(background)
                .join(LineJoin::Round)
                .finish();
            for cherry in [x - 2.6, x + 2.6] {
                out.element(part, Facet::Shape)
                    .circle_at([Some(Number::num(cherry)), Some(Number::num(y + 2.2))], Number::raw(1.8))
                    .fill(background)
                    .finish();
            }
        }
        _ => {}
    }
}

pub(crate) struct Git;

impl Diagram for Git {
    type Model = GitModel;

    fn parse(cx: &ParseContext<'_>) -> Result<GitModel, RenderError> {
        super::parse(cx)
    }

    /// Pele's `renderGit`: a graph that runs across may run down instead to fit the host.
    fn scene(model: &GitModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        if model.direction != Direction::LeftRight {
            return draw(model, cx, model.direction);
        }
        fit::turn_to_fit(cx.options, |down| {
            draw(model, cx, if down { Direction::TopBottom } else { Direction::LeftRight })
        })
    }
}
