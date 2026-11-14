//! Pele's `FlowDb` (`src/diagrams/flowchart/db.ts`): what each statement records in the model.

use super::shapes::ShapeName;
use super::{
    ArrowKind, Direction, FlowClassDef, FlowEdge, FlowNode, FlowSubgraph, FlowText, FlowchartModel, LabelType, Link,
    LinkTarget, Stroke,
};
use crate::input::url::safe_url;
use crate::input::{
    JsObject, JsText, JsValue, Units, is_space, parse_yaml, split_unit, string_to_number, to_lowercase,
};
use crate::{Diagnostic, ErrorCode, RenderError};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

const fn unit(c: u8) -> u16 {
    c as u16
}

fn units(text: &str) -> JsText {
    text.into()
}

fn error(code: ErrorCode, message: String) -> RenderError {
    RenderError::Diagnostic(Diagnostic::new(code, message))
}

/// `text.startsWith('"') && text.endsWith('"') ? text.substring(1, text.length - 1) : text`.
/// For a lone `"`, `substring(1, 0)` is the `"` itself.
fn unquote(text: &[u16]) -> JsText {
    let quoted = text.first() == Some(&unit(b'"')) && text.last() == Some(&unit(b'"'));
    if quoted && text.len() >= 2 { text[1..text.len() - 1].into() } else { text.into() }
}

/// A link as its start and end are written, with any text and id it carries.
pub(super) struct LinkInfo {
    pub(super) link: Link,
    pub(super) text: Option<FlowText>,
    pub(super) id: Option<JsText>,
}

/// Pele's `destructEndLink`: the arrowheads, stroke and length a whole link gives.
fn end_link(raw: &[u16]) -> (ArrowKind, Stroke, usize) {
    let text = raw.trim_js();
    let mut line = &text[..text.len() - 1];
    let starts = |c: u8| text.first() == Some(&unit(c));
    let (single, double) = match u8::try_from(text[text.len() - 1]) {
        Ok(b'x') => (ArrowKind::Cross, ArrowKind::DoubleCross),
        Ok(b'>') => (ArrowKind::Point, ArrowKind::DoublePoint),
        Ok(b'o') => (ArrowKind::Circle, ArrowKind::DoubleCircle),
        _ => (ArrowKind::Open, ArrowKind::Open),
    };
    let opener = match single {
        ArrowKind::Cross => b'x',
        ArrowKind::Point => b'<',
        _ => b'o',
    };
    let arrow = if single != ArrowKind::Open && starts(opener) {
        line = &line[1..];
        double
    } else {
        single
    };
    let mut stroke = Stroke::Normal;
    let mut length = line.len().wrapping_sub(1);
    if line.first() == Some(&unit(b'=')) {
        stroke = Stroke::Thick;
    }
    if line.first() == Some(&unit(b'~')) {
        stroke = Stroke::Invisible;
    }
    let dots = line.iter().filter(|&&c| c == unit(b'.')).count();
    if dots > 0 {
        stroke = Stroke::Dotted;
        length = dots;
    }
    (arrow, stroke, length)
}

/// Pele's `destructStartLink`: the arrowhead and stroke the opening part of a link gives.
fn start_link(raw: &[u16]) -> (ArrowKind, Stroke) {
    let mut text = raw.trim_js();
    let arrow = match text.first().and_then(|&c| u8::try_from(c).ok()) {
        Some(b'<') => ArrowKind::Point,
        Some(b'x') => ArrowKind::Cross,
        Some(b'o') => ArrowKind::Circle,
        _ => ArrowKind::Open,
    };
    if arrow != ArrowKind::Open {
        text = &text[1..];
    }
    let mut stroke = Stroke::Normal;
    if text.contains(&unit(b'=')) {
        stroke = Stroke::Thick;
    }
    if text.contains(&unit(b'.')) {
        stroke = Stroke::Dotted;
    }
    (arrow, stroke)
}

/// Pele's `destructLink`, for a link token and the start token of a link with text.
pub(super) fn destruct_link(end: &[u16], start: Option<&[u16]>) -> Link {
    let (end_arrow, end_stroke, length) = end_link(end);
    let Some(start) = start.filter(|start| !start.is_empty()) else {
        return Link::Valid { arrow: end_arrow, stroke: end_stroke, length };
    };
    let (start_arrow, start_stroke) = start_link(start);
    if start_stroke != end_stroke {
        return Link::Invalid;
    }
    let arrow = match (start_arrow, end_arrow) {
        (ArrowKind::Open, end) => end,
        (ArrowKind::Point, ArrowKind::Point) => ArrowKind::DoublePoint,
        (ArrowKind::Cross, ArrowKind::Cross) => ArrowKind::DoubleCross,
        (ArrowKind::Circle, ArrowKind::Circle) => ArrowKind::DoubleCircle,
        _ => return Link::Invalid,
    };
    Link::Valid { arrow, stroke: start_stroke, length }
}

/// `linkStyle`'s positions: `default`, or edge indexes as written.
pub(super) enum LinkPositions {
    Default,
    Indexes(Vec<JsText>),
}

/// A subgraph's header. A named subgraph without a bracketed title is its own title.
pub(super) enum SubgraphHeader {
    Unnamed,
    Named { id: FlowText, title: Option<FlowText> },
}

/// What a statement inside a subgraph contributes to its member list.
pub(super) enum DocItem {
    Text(JsText),
    Ids(Vec<JsText>),
    Dir(Direction),
}

/// What one `addVertex` call carries besides the id.
#[derive(Default)]
pub(super) struct VertexUpdate {
    pub(super) text: Option<FlowText>,
    pub(super) shape: Option<ShapeName>,
    pub(super) styles: Option<Vec<JsText>>,
    pub(super) props: Option<JsObject>,
    pub(super) metadata: Option<Vec<u16>>,
}

pub(super) struct FlowDb {
    pub(super) model: FlowchartModel,
    members: HashSet<JsText>,
    edge_by_id: HashMap<JsText, usize>,
    pair_count: HashMap<JsText, usize>,
    sub_count: usize,
    inherit_dir: bool,
    max_edges: usize,
}

impl FlowDb {
    pub(super) fn new(inherit_dir: bool, max_edges: usize) -> Self {
        FlowDb {
            model: FlowchartModel {
                direction: Direction::TB,
                nodes: IndexMap::new(),
                edges: Vec::new(),
                subgraphs: Vec::new(),
                classes: IndexMap::new(),
                tooltips: IndexMap::new(),
                default_edge_style: None,
                default_interpolate: None,
                title: None,
                acc_title: None,
                acc_descr: None,
                subgraph_index: HashMap::new(),
            },
            members: HashSet::new(),
            edge_by_id: HashMap::new(),
            pair_count: HashMap::new(),
            sub_count: 0,
            inherit_dir,
            max_edges,
        }
    }

    /// Pele's `addVertex`. Metadata applies to a subgraph or an edge of that id when there is one.
    pub(super) fn add_vertex(&mut self, id: &[u16], update: VertexUpdate) -> Result<(), RenderError> {
        if id.trim_js().is_empty() {
            return Ok(());
        }
        let doc = match &update.metadata {
            None => None,
            Some(metadata) => {
                let mut yaml = Vec::with_capacity(metadata.len() + 4);
                if metadata.contains(&10) {
                    yaml.extend_from_slice(metadata);
                    yaml.push(10);
                } else {
                    yaml.extend("{\n".encode_utf16());
                    yaml.extend_from_slice(metadata);
                    yaml.extend("\n}".encode_utf16());
                }
                let parsed = parse_yaml(&yaml).map_err(|message| {
                    let id = String::from_utf16_lossy(id);
                    error(ErrorCode::Syntax, format!("Metadata of \"{id}\" is not valid YAML. {message}"))
                })?;
                match parsed {
                    JsValue::Object(doc) => Some(doc),
                    _ => None,
                }
            }
        };

        if let (Some(&at), Some(doc)) = (self.model.subgraph_index.get(id), &doc) {
            let subgraph = &mut self.model.subgraphs[at];
            subgraph.metadata = Some(subgraph.metadata.take().unwrap_or_default().spread(doc));
            return Ok(());
        }

        if let Some(&at) = self.edge_by_id.get(id) {
            let edge = &mut self.model.edges[at];
            if let Some(doc) = &doc {
                if let Some(value) = doc.get("animate") {
                    edge.animate = Some(value.clone());
                }
                if let Some(value) = doc.get("animation") {
                    edge.animation = Some(value.clone());
                }
                if let Some(value) = doc.get("curve") {
                    edge.interpolate = Some(value.clone());
                }
            }
            return Ok(());
        }

        let fresh = !self.model.nodes.contains_key(id);
        let vertex = self.model.nodes.entry(id.into()).or_insert_with(|| FlowNode {
            id: id.into(),
            text: id.into(),
            label_type: LabelType::Text,
            shape: None,
            styles: Vec::new(),
            classes: Vec::new(),
            props: JsObject::default(),
            link: None,
            link_target: None,
            icon: None,
            form: None,
            pos: None,
            img: None,
            constraint: None,
            asset_width: None,
            asset_height: None,
        });
        if let Some(text) = update.text {
            vertex.label_type = text.kind;
            vertex.text = unquote(text.text.trim_js());
        }
        if let Some(shape) = update.shape {
            vertex.shape = Some(shape);
        }
        vertex.styles.extend(update.styles.into_iter().flatten());
        if let Some(props) = update.props {
            if fresh {
                vertex.props = props;
            } else {
                vertex.props.assign(&props);
            }
        }

        let Some(doc) = doc else {
            return Ok(());
        };
        let truthy = |name: &str| doc.get(name).filter(|value| value.truthy());
        let text = |name: &str| truthy(name).map(JsValue::to_js_string);
        if let Some(shape) = text("shape") {
            let name = shape.to_display();
            if *to_lowercase(&shape) != *shape || shape.contains(&unit(b'_')) {
                let message = format!("No such shape: {name}. Shape names should be lowercase.");
                return Err(error(ErrorCode::Semantic, message));
            }
            let Some(accepted) = ShapeName::accepted(&shape) else {
                return Err(error(ErrorCode::Semantic, format!("No such shape: {name}.")));
            };
            vertex.shape = Some(accepted);
        }
        let label = doc.get("label").and_then(JsValue::as_text);
        let blank_label = label.is_none_or(|label| label.trim_js().is_empty());
        if let Some(text) = text("label") {
            vertex.text = text;
            vertex.label_type = LabelType::of(doc.get("labelType"));
        }
        if let Some(icon) = text("icon") {
            vertex.icon = Some(icon);
            if blank_label && *vertex.text == *id {
                vertex.text = JsText::default();
            }
        }
        if let Some(form) = text("form") {
            vertex.form = Some(form);
        }
        if let Some(pos) = text("pos") {
            vertex.pos = Some(pos);
        }
        if let Some(img) = text("img") {
            vertex.img = Some(img);
            if blank_label && *vertex.text == *id {
                vertex.text = JsText::default();
            }
        }
        if let Some(constraint) = text("constraint") {
            vertex.constraint = Some(constraint);
        }
        if let Some(width) = truthy("w") {
            vertex.asset_width = Some(to_number(width));
        }
        if let Some(height) = truthy("h") {
            vertex.asset_height = Some(to_number(height));
        }
        Ok(())
    }

    /// Pele's `addSingleLink`.
    fn add_single_link(
        &mut self,
        start: &[u16],
        end: &[u16],
        info: &LinkInfo,
        id: Option<&[u16]>,
    ) -> Result<(), RenderError> {
        let (text, label_type) = match &info.text {
            Some(text) => (unquote(text.text.trim_js()), text.kind),
            None => (JsText::default(), LabelType::Text),
        };
        let link = match info.link {
            Link::Valid { arrow, stroke, length } => Link::Valid { arrow, stroke, length: length.min(10) },
            Link::Invalid => Link::Invalid,
        };
        let mut pair = start.to_vec();
        pair.push(0);
        pair.extend_from_slice(end);
        let pair = JsText::from(pair);
        let count = self.pair_count.get(&pair).copied().unwrap_or(0);
        let (id, is_user_defined_id) = match id.filter(|id| !id.is_empty() && !self.edge_by_id.contains_key(*id)) {
            Some(id) => (id.into(), true),
            None => {
                let rank = if count == 0 { 0 } else { count + 1 };
                let mut id = units("L_");
                id.push_units(start);
                id.push(unit(b'_'));
                id.push_units(end);
                id.push_str(&format!("_{rank}"));
                (id, false)
            }
        };
        if self.model.edges.len() >= self.max_edges {
            let message = format!("Edge limit exceeded. The limit is {}.", self.max_edges);
            return Err(error(ErrorCode::Limit, message));
        }
        self.edge_by_id.entry(id.clone()).or_insert(self.model.edges.len());
        self.model.edges.push(FlowEdge {
            start: start.into(),
            end: end.into(),
            link,
            text,
            label_type,
            style: None,
            classes: Vec::new(),
            id,
            is_user_defined_id,
            interpolate: self.model.default_interpolate.clone().map(JsValue::String),
            animate: None,
            animation: None,
        });
        self.pair_count.insert(pair, count + 1);
        Ok(())
    }

    /// Pele's `addLink`: an edge from every start to every end. Only the edge from the last
    /// start to the first end takes the link's id.
    pub(super) fn add_link(&mut self, starts: &[JsText], ends: &[JsText], info: &LinkInfo) -> Result<(), RenderError> {
        let id = info.id.as_ref().map(|id| {
            let mut id = id.to_vec();
            if let Some(at) = id.iter().position(|&c| c == unit(b'@')) {
                id.remove(at);
            }
            id
        });
        let last_start = starts.last().expect("a statement has a node");
        let first_end = ends.first().expect("a statement has a node");
        for start in starts {
            for end in ends {
                let own = start == last_start && end == first_end;
                self.add_single_link(start, end, info, id.as_deref().filter(|_| own))?;
            }
        }
        Ok(())
    }

    /// Pele's `edgeAt`, which reports an index past the last edge.
    fn edge_at(&mut self, position: &[u16]) -> Result<&mut FlowEdge, RenderError> {
        let count = self.model.edges.len();
        let index = string_to_number(position);
        if index < count as f64 {
            return Ok(&mut self.model.edges[index as usize]);
        }
        let position = String::from_utf16_lossy(position);
        let last = count as i64 - 1;
        Err(error(
            ErrorCode::Semantic,
            format!(
                "The index {position} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {last}. (Help: Ensure that the index is within the range of existing edges.)"
            ),
        ))
    }

    /// Pele's `updateLinkInterpolate`.
    pub(super) fn update_link_interpolate(
        &mut self,
        positions: &LinkPositions,
        curve: &[u16],
    ) -> Result<(), RenderError> {
        match positions {
            LinkPositions::Default => self.model.default_interpolate = Some(curve.into()),
            LinkPositions::Indexes(indexes) => {
                for position in indexes {
                    self.edge_at(position)?.interpolate = Some(JsValue::String(curve.into()));
                }
            }
        }
        Ok(())
    }

    /// Pele's `updateLink`. The edges a statement names share one style list, which gains
    /// `fill:none` when it sets no fill.
    pub(super) fn update_link(&mut self, positions: &LinkPositions, mut style: Vec<JsText>) -> Result<(), RenderError> {
        let indexes = match positions {
            LinkPositions::Default => {
                self.model.default_edge_style = Some(style);
                return Ok(());
            }
            LinkPositions::Indexes(indexes) => indexes,
        };
        if !style.is_empty() && !style.iter().any(|s| s.starts_at(0, "fill")) {
            style.push(units("fill:none"));
        }
        for position in indexes {
            self.edge_at(position)?.style = Some(style.clone());
        }
        Ok(())
    }

    /// Pele's `addClass`.
    pub(super) fn add_class(&mut self, ids: &[u16], styles: &[JsText]) {
        let mut joined: Vec<u16> = Vec::new();
        for (index, style) in styles.iter().enumerate() {
            if index > 0 {
                joined.push(unit(b','));
            }
            joined.extend_from_slice(style);
        }
        let mut style: Vec<u16> = Vec::with_capacity(joined.len());
        let mut at = 0;
        while at < joined.len() {
            // `\,` keeps its comma, every other comma separates.
            if joined[at] == unit(b'\\') && joined.get(at + 1) == Some(&unit(b',')) {
                style.extend([0xa7, 0xa7, 0xa7]);
                at += 2;
            } else {
                style.push(if joined[at] == unit(b',') { unit(b';') } else { joined[at] });
                at += 1;
            }
        }
        let restored = restore_commas(&style);
        let style: Vec<JsText> = split_unit(&restored, unit(b';')).into_iter().map(JsText::from).collect();
        for id in split_unit(ids, unit(b',')) {
            let class = self.model.classes.entry(id.into()).or_insert_with(|| FlowClassDef {
                id: id.into(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for s in &style {
                if s.has("color") {
                    class.text_styles.push(replace_first(s, "fill", "bgFill"));
                }
                class.styles.push(s.clone());
            }
        }
    }

    /// Pele's `setDirection`.
    pub(super) fn set_direction(&mut self, dir: &[u16]) {
        let d = dir.trim_js();
        let mut direction = match () {
            _ if d.is_str("LR") => Direction::LR,
            _ if d.is_str("RL") => Direction::RL,
            _ if d.is_str("BT") => Direction::BT,
            _ if d.is_str("BR") => Direction::BR,
            _ => Direction::TB,
        };
        if d.has("<") {
            direction = Direction::RL;
        }
        if d.has("^") {
            direction = Direction::BT;
        }
        if d.has(">") {
            direction = Direction::LR;
        }
        if d.has("v") {
            direction = Direction::TB;
        }
        self.model.direction = direction;
    }

    /// Pele's `setClass`, for nodes, edges and subgraphs alike.
    pub(super) fn set_class(&mut self, ids: &[u16], class_name: &[u16]) {
        for id in split_unit(ids, unit(b',')) {
            if let Some(node) = self.model.nodes.get_mut(id) {
                node.classes.push(class_name.into());
            }
            if let Some(&at) = self.edge_by_id.get(id) {
                self.model.edges[at].classes.push(class_name.into());
            }
            if let Some(&at) = self.model.subgraph_index.get(id) {
                self.model.subgraphs[at].classes.push(class_name.into());
            }
        }
    }

    pub(super) fn set_tooltip(&mut self, ids: &[u16], tooltip: &[u16]) {
        for id in split_unit(ids, unit(b',')) {
            self.model.tooltips.insert(id.into(), tooltip.into());
        }
    }

    /// Pele's `setLink`: the URL goes through the URL policy, and the nodes become clickable.
    pub(super) fn set_link(&mut self, ids: &[u16], link: &[u16], target: Option<LinkTarget>) {
        let url = link.trim_js();
        let safe = (!url.is_empty()).then(|| safe_url(url));
        for id in split_unit(ids, unit(b',')) {
            if let Some(vertex) = self.model.nodes.get_mut(id) {
                vertex.link = safe.clone();
                vertex.link_target = target;
            }
        }
        self.set_class(ids, &units("clickable"));
    }

    /// Callbacks are never run: the node is only marked clickable, as in Mermaid's strict mode.
    pub(super) fn set_click_event(&mut self, ids: &[u16]) {
        self.set_class(ids, &units("clickable"));
    }

    pub(super) fn set_acc_title(&mut self, text: &[u16]) {
        self.model.acc_title = Some(text.trim_start_js().into());
    }

    /// `text.replace(/\n\s+/g, '\n')`.
    pub(super) fn set_acc_description(&mut self, text: &[u16]) {
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            let spaces = text.run(at + 1, is_space);
            at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
        }
        self.model.acc_descr = Some(out.into());
    }

    /// Pele's `addSubGraph`: the subgraph a closed `subgraph … end` describes, merged into an
    /// earlier one of the same id. Returns the id.
    pub(super) fn add_subgraph(&mut self, header: SubgraphHeader, list: Vec<DocItem>) -> JsText {
        let (id, title, label_type) = match &header {
            SubgraphHeader::Unnamed => (None, JsText::default(), LabelType::Markdown),
            SubgraphHeader::Named { id, title: None } => {
                // A name with whitespace in it is only a title.
                let named = (!id.text.iter().any(|&c| is_space(c))).then(|| id.text.trim_js().into());
                (named, id.text.clone(), id.kind)
            }
            SubgraphHeader::Named { id, title: Some(title) } => {
                (Some(id.text.trim_js().into()), title.text.clone(), title.kind)
            }
        };

        let mut seen = HashSet::new();
        let mut node_list: Vec<JsText> = Vec::new();
        let mut dir = None;
        let mut keep = |entry: JsText, seen: &mut HashSet<JsText>| {
            if !entry.trim_js().is_empty() && seen.insert(entry.clone()) {
                node_list.push(entry);
            }
        };
        for entry in list {
            match entry {
                DocItem::Text(text) => keep(text, &mut seen),
                DocItem::Ids(ids) => ids.into_iter().for_each(|item| keep(item, &mut seen)),
                DocItem::Dir(value) => dir = Some(value),
            }
        }
        if dir.is_none() && self.inherit_dir {
            dir = Some(self.model.direction);
        }

        let id: JsText = id.unwrap_or_else(|| units(&format!("subGraph{}", self.sub_count)));
        self.sub_count += 1;

        // A node belongs to the first subgraph that lists it, and a subgraph never contains itself.
        let nodes: Vec<JsText> =
            node_list.into_iter().filter(|node| !self.members.contains(node) && *node != id).collect();
        self.members.extend(nodes.iter().cloned());

        match self.model.subgraph_index.get(&id) {
            Some(&at) => self.model.subgraphs[at].nodes.extend(nodes),
            None => {
                self.model.subgraph_index.insert(id.clone(), self.model.subgraphs.len());
                self.model.subgraphs.push(FlowSubgraph {
                    id: id.clone(),
                    nodes,
                    title: title.trim_js().into(),
                    classes: Vec::new(),
                    dir,
                    label_type,
                    metadata: None,
                });
            }
        }
        id
    }
}

/// `.replace(/§§§/g, ',')`.
fn restore_commas(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        if text[at..].starts_with(&[0xa7, 0xa7, 0xa7]) {
            out.push(unit(b','));
            at += 3;
        } else {
            out.push(text[at]);
            at += 1;
        }
    }
    out
}

/// `text.replace(from, to)` for a string pattern: the first occurrence only.
fn replace_first(text: &[u16], from: &str, to: &str) -> JsText {
    match text.find_str(from, 0) {
        Some(at) => {
            let mut out = JsText::from(&text[..at]);
            out.push_str(to);
            out.push_units(&text[at + from.len()..]);
            out
        }
        None => text.into(),
    }
}

/// `Number(value)`.
fn to_number(value: &JsValue) -> f64 {
    match value {
        JsValue::Null => 0.0,
        JsValue::Bool(value) => f64::from(u8::from(*value)),
        JsValue::Number(value) => *value,
        JsValue::String(text) => string_to_number(text),
        JsValue::Array(_) => string_to_number(&value.to_js_string()),
        JsValue::Object(_) => f64::NAN,
    }
}
