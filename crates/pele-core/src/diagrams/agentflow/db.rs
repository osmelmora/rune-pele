//! Pele's `AgentflowDb` (`src/diagrams/agentflow/db.ts`): what each statement records in the
//! model, and where each statement sits in the source.

use super::lexer::Place;
use super::shapes::{self, NodeShape};
use super::{
    AgentEdge, AgentGraph, AgentNode, AgentSubgraph, AgentflowModel, Arrow, ElementMapping, ElementPosition, Link,
    StatementType, Stroke, json,
};
use crate::catalog::ParseContext;
use crate::diagrams::flowchart::{Direction, FlowClassDef, FlowText, LabelType, LinkTarget};
use crate::input::url::safe_url;
use crate::input::{
    JsObject, JsText, JsValue, Units, is_digit, is_space, parse_yaml, split_unit, string_to_number, to_lowercase,
};
use crate::{Diagnostic, ErrorCode, Location, RenderError};
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet};

const fn unit(c: u8) -> u16 {
    c as u16
}

fn units(text: &str) -> JsText {
    text.into()
}

/// `text.startsWith('"') && text.endsWith('"') ? text.substring(1, text.length - 1) : text`.
/// For a lone `"`, `substring(1, 0)` is the `"` itself.
fn unquote(text: &[u16]) -> JsText {
    let quoted = text.first() == Some(&unit(b'"')) && text.last() == Some(&unit(b'"'));
    if quoted && text.len() >= 2 { text[1..text.len() - 1].into() } else { text.into() }
}

/// Pele's `labelType`: a value that names one of the three, or markdown.
fn label_type(value: Option<&JsValue>) -> LabelType {
    let name = |text: &str| value.and_then(JsValue::as_text).is_some_and(|value| value.is_str(text));
    if name("string") {
        LabelType::String
    } else if name("text") {
        LabelType::Text
    } else {
        LabelType::Markdown
    }
}

/// A span of the source as Jison reports it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Loc {
    pub(super) first: Place,
    pub(super) last: Place,
}

/// A link as its start and end are written, with any text and id it carries.
pub(super) struct LinkInfo {
    pub(super) link: Link,
    pub(super) text: Option<FlowText>,
    pub(super) id: Option<JsText>,
}

/// Pele's `destructEndLink`: the arrowhead, stroke and length a whole link gives.
fn end_link(raw: &[u16]) -> (Arrow, Stroke, usize) {
    let text = raw.trim_js();
    let (arrow, line) = match text.last().and_then(|&c| u8::try_from(c).ok()) {
        Some(b'x') => (Arrow::Cross, &text[..text.len() - 1]),
        Some(b'>') => (Arrow::Point, &text[..text.len() - 1]),
        Some(b'-' | b'.') => (Arrow::Open, text),
        _ => (Arrow::Open, &text[..text.len().saturating_sub(1)]),
    };
    let dots = line.iter().filter(|&&c| c == unit(b'.')).count();
    if dots > 0 { (arrow, Stroke::Dotted, dots) } else { (arrow, Stroke::Normal, line.len().wrapping_sub(1)) }
}

/// Pele's `destructLink`, for a link token and the start token of a link with text. The first
/// half of a split arrow only says whether the line is dotted.
pub(super) fn destruct_link(end: &[u16], start: Option<&[u16]>) -> Link {
    let (arrow, stroke, length) = end_link(end);
    if let Some(start) = start.filter(|start| !start.is_empty()) {
        let start_stroke = if start.contains(&unit(b'.')) { Stroke::Dotted } else { Stroke::Normal };
        if start_stroke != stroke {
            return Link::Invalid;
        }
    }
    Link::Valid { arrow, stroke, length }
}

/// `linkStyle`'s positions: `default`, or edge indexes as written.
pub(super) enum LinkPositions {
    Default,
    Indexes(Vec<JsText>),
}

/// What a statement inside a block contributes to the block's member list.
pub(super) enum DocItem {
    Text(JsText),
    Ids(Vec<JsText>),
    Dir(Direction),
}

/// What one `addVertex` call carries besides the id.
#[derive(Default)]
pub(super) struct VertexUpdate {
    pub(super) text: Option<FlowText>,
    pub(super) shape: Option<NodeShape>,
    pub(super) styles: Option<Vec<JsText>>,
    pub(super) props: Option<JsObject>,
    /// The body of a metadata block, and where the block sits.
    pub(super) metadata: Option<(Vec<u16>, Loc)>,
}

/// `/:\s*[>|][\d+-]*\s*$/`: a key whose value is a block scalar.
fn opens_block_scalar(code: &[u16]) -> bool {
    let end = code.trim_end_js().len();
    let indicator = code[..end].iter().rposition(|&c| !(is_digit(c) || c == unit(b'+') || c == unit(b'-')));
    let Some(at) = indicator.filter(|&at| code[at] == unit(b'>') || code[at] == unit(b'|')) else {
        return false;
    };
    code[..at].trim_end_js().last() == Some(&unit(b':'))
}

/// `code.replace(/,([\t ]*)$/, '$1')`
fn without_trailing_comma(code: &[u16]) -> Vec<u16> {
    let end = code.iter().rposition(|&c| c != 32 && c != 9).map_or(0, |last| last + 1);
    let mut out = code.to_vec();
    if end > 0 && code[end - 1] == unit(b',') {
        out.remove(end - 1);
    }
    out
}

/// Pele's `stripLineTrailingCommas`: removes the comma that ends a line of a multi-line metadata
/// body, which block YAML does not allow. Commas inside quoted scalars, flow collections and
/// block scalars are content and stay.
fn strip_line_trailing_commas(body: &[u16]) -> Vec<u16> {
    let indent_of = |line: &[u16]| line.len() - line.trim_start_js().len();
    let mut in_single = false;
    let mut in_double = false;
    let mut flow_depth = 0usize;
    let mut block_scalar_indent: Option<usize> = None;
    let mut out: Vec<Vec<u16>> = Vec::new();
    for line in split_unit(body, 10) {
        if let Some(indent) = block_scalar_indent {
            if line.trim_js().is_empty() || indent_of(line) > indent {
                out.push(line.to_vec());
                continue;
            }
            block_scalar_indent = None;
        }
        let mut comment_start = None;
        let mut i = 0;
        while i < line.len() {
            let ch = line[i];
            if in_double {
                if ch == unit(b'\\') {
                    i += 1;
                } else if ch == unit(b'"') {
                    in_double = false;
                }
            } else if in_single {
                if ch == unit(b'\'') {
                    in_single = false;
                }
            } else if ch == unit(b'"') {
                in_double = true;
            } else if ch == unit(b'\'') {
                in_single = true;
            } else if ch == unit(b'#') && (i == 0 || line[i - 1] == 32 || line[i - 1] == 9) {
                comment_start = Some(i);
                break;
            } else if ch == unit(b'[') || ch == unit(b'{') {
                flow_depth += 1;
            } else if ch == unit(b']') || ch == unit(b'}') {
                flow_depth = flow_depth.saturating_sub(1);
            }
            i += 1;
        }
        if in_single || in_double || flow_depth > 0 {
            out.push(line.to_vec());
            continue;
        }
        let (code, comment) = line.split_at(comment_start.unwrap_or(line.len()));
        if opens_block_scalar(code) {
            block_scalar_indent = Some(indent_of(line));
            out.push(line.to_vec());
            continue;
        }
        let mut stripped = without_trailing_comma(code);
        stripped.extend_from_slice(comment);
        out.push(stripped);
    }
    out.join(&10)
}

/// Pele's `stripPrototypeKeys`: drops `__proto__`, `constructor` and `prototype` keys at every
/// level, so that a consumer merging metadata into its own objects cannot be made to pollute a
/// prototype.
fn strip_prototype_keys(value: JsValue) -> JsValue {
    match value {
        JsValue::Array(items) => JsValue::Array(items.into_iter().map(strip_prototype_keys).collect()),
        JsValue::Object(object) => {
            let mut out = JsObject::default();
            for (key, entry) in object.iter() {
                if !["__proto__", "constructor", "prototype"].iter().any(|unsafe_key| key.is_str(unsafe_key)) {
                    out.set(key.clone(), strip_prototype_keys(entry.clone()));
                }
            }
            JsValue::Object(out)
        }
        other => other,
    }
}

/// Pele's `merge`: `Object.assign(into ?? {}, doc)`. Mermaid spreads the parsed YAML whatever
/// it is, so a list adds its indexes as keys and a string its characters.
fn merge(into: &mut Option<JsObject>, doc: &JsValue) {
    let target = into.get_or_insert_with(JsObject::default);
    let index = |at: usize| JsText::from(at.to_string().as_str());
    match doc {
        JsValue::Object(object) => target.assign(object),
        JsValue::Array(items) => {
            for (at, item) in items.iter().enumerate() {
                target.set(index(at), item.clone());
            }
        }
        JsValue::String(text) => {
            for (at, &c) in text.iter().enumerate() {
                target.set(index(at), JsValue::String(JsText::from(vec![c])));
            }
        }
        JsValue::Null | JsValue::Bool(_) | JsValue::Number(_) => {}
    }
}

/// Pele's `errorAt`: the line and column of the source where a metadata block starts. An error
/// about metadata carries them without the line's text.
fn error_at(cx: &ParseContext<'_>, loc: Loc) -> Location {
    let source = cx.text;
    let mut start = 0;
    for _ in 1..loc.first.line {
        start = source.find_unit(10, start).map_or(0, |newline| newline + 1);
    }
    let location = cx.origin.locate(source, start + loc.first.column);
    Location { snippet: String::new(), ..location }
}

fn located(code: ErrorCode, message: String, at: Location) -> RenderError {
    RenderError::Diagnostic(Diagnostic { location: Some(at), ..Diagnostic::new(code, message) })
}

pub(super) struct AgentflowDb<'a, 'c> {
    pub(super) model: AgentflowModel,
    cx: &'a ParseContext<'c>,
    global_nodes: HashSet<JsText>,
    members: HashSet<JsText>,
    edge_by_id: HashMap<JsText, usize>,
    pair_count: HashMap<JsText, usize>,
    sub_count: usize,
    /// The mappings of bare references, each of which becomes an attachment if a metadata block
    /// follows it.
    bare_mappings: HashSet<usize>,
    inherit_dir: bool,
}

impl<'a, 'c> AgentflowDb<'a, 'c> {
    pub(super) fn new(cx: &'a ParseContext<'c>, inherit_dir: bool) -> Self {
        AgentflowDb {
            model: AgentflowModel {
                direction: Direction::TB,
                nodes: IndexMap::new(),
                edges: Vec::new(),
                subgraphs: Vec::new(),
                connectors: IndexSet::new(),
                classes: IndexMap::new(),
                tooltips: IndexMap::new(),
                default_edge_style: None,
                default_interpolate: None,
                mappings: Vec::new(),
                diagnostics: Vec::new(),
                title: None,
                acc_title: None,
                acc_descr: None,
                graph: AgentGraph::default(),
                subgraph_index: HashMap::new(),
            },
            cx,
            global_nodes: HashSet::new(),
            members: HashSet::new(),
            edge_by_id: HashMap::new(),
            pair_count: HashMap::new(),
            sub_count: 0,
            bare_mappings: HashSet::new(),
            inherit_dir,
        }
    }

    /// Pele's `metadata`: the YAML a metadata block holds. The single-line form is a flow
    /// mapping without its braces, and the multi-line form is block YAML, whose lines may end in
    /// the commas of the single-line form.
    fn metadata(&self, id: &[u16], source: &[u16], loc: Loc) -> Result<JsValue, RenderError> {
        let multiline = source.contains(&10);
        let yaml = |body: &[u16], braced: bool| {
            let mut text: Vec<u16> = if braced { "{\n".encode_utf16().collect() } else { Vec::new() };
            text.extend_from_slice(body);
            text.push(10);
            if braced {
                text.push(unit(b'}'));
            }
            parse_yaml(&text)
        };
        let parsed = match yaml(source, !multiline) {
            Ok(parsed) => parsed,
            Err(message) => {
                // Block YAML rejects the comma-separated style, so try once more without the commas.
                let stripped = if multiline { strip_line_trailing_commas(source) } else { source.to_vec() };
                let retried = if stripped == source { None } else { yaml(&stripped, false).ok() };
                match retried {
                    Some(parsed) => parsed,
                    None => {
                        let at = error_at(self.cx, loc);
                        let message = format!(
                            "Metadata of \"{}\" is not valid YAML. {message} ({}:{})",
                            String::from_utf16_lossy(id),
                            at.line,
                            at.column_utf16
                        );
                        return Err(located(ErrorCode::Syntax, message, at));
                    }
                }
            }
        };
        Ok(strip_prototype_keys(match parsed {
            JsValue::Null => JsValue::Object(JsObject::default()),
            parsed => parsed,
        }))
    }

    /// Pele's `addVertex`. Metadata applies to a flow, a connector or an edge of that id when
    /// there is one.
    pub(super) fn add_vertex(&mut self, id: &[u16], update: VertexUpdate) -> Result<(), RenderError> {
        if id.trim_js().is_empty() {
            return Ok(());
        }
        let mut doc = None;
        let mut authored_shape = None;
        let loc = update.metadata.as_ref().map(|&(_, loc)| loc);
        if let Some((metadata, loc)) = &update.metadata {
            let mut parsed = self.metadata(id, metadata, *loc)?;
            if let JsValue::Object(fields) = &mut parsed
                && let Some(JsValue::String(shape)) = fields.get("shape").cloned()
            {
                if let Some(alias) = shapes::alias_of(&shape) {
                    fields.set("shape".into(), JsValue::String(alias.into()));
                }
                authored_shape = Some(shape);
            }
            doc = Some(parsed);
        }
        let told = doc.as_ref().filter(|doc| doc.truthy());

        if let (Some(&at), Some(doc)) = (self.model.subgraph_index.get(id), told) {
            merge(&mut self.model.subgraphs[at].metadata, doc);
            return Ok(());
        }
        if let (true, Some(doc)) = (self.model.connectors.contains(id), told) {
            merge(&mut self.model.nodes[id].metadata, doc);
            return Ok(());
        }
        // Mermaid reserves this id and creates no node for it.
        if id.is_str("connectors") {
            return Ok(());
        }
        if let Some(&at) = self.edge_by_id.get(id) {
            let edge = &mut self.model.edges[at];
            if let Some(fields) = doc.as_ref().and_then(JsValue::as_record) {
                if let Some(value) = fields.get("animate") {
                    edge.animate = Some(value.clone());
                }
                if let Some(value) = fields.get("animation") {
                    edge.animation = Some(value.clone());
                }
                if let Some(value) = fields.get("curve") {
                    edge.interpolate = Some(value.clone());
                }
            }
            if let Some(doc) = told {
                merge(&mut edge.metadata, doc);
            }
            return Ok(());
        }

        let cx = self.cx;
        let vertex = self.model.nodes.entry(id.into()).or_insert_with(|| AgentNode {
            id: id.into(),
            text: id.into(),
            label_type: LabelType::Text,
            shape: None,
            styles: Vec::new(),
            classes: Vec::new(),
            props: None,
            link: None,
            link_target: None,
            metadata: None,
            is_connector: false,
        });
        if let Some(text) = update.text {
            vertex.label_type = text.kind;
            vertex.text = unquote(text.text.trim_js());
        }
        if let Some(shape) = update.shape {
            vertex.shape = Some(shape);
        }
        vertex.styles.extend(update.styles.into_iter().flatten());
        match &mut vertex.props {
            None => vertex.props = Some(update.props.unwrap_or_default()),
            Some(props) => {
                if let Some(more) = &update.props {
                    props.assign(more);
                }
            }
        }

        let Some(doc) = told else {
            return Ok(());
        };
        merge(&mut vertex.metadata, doc);
        let Some(fields) = doc.as_record() else {
            return Ok(());
        };
        let mut shape_error = None;
        if let Some(shape) = fields.get("shape").filter(|shape| shape.truthy()) {
            match (&authored_shape, shape) {
                (Some(authored), JsValue::String(shape)) => {
                    if *to_lowercase(authored) != **authored || authored.contains(&unit(b'_')) {
                        let name = authored.to_display();
                        shape_error = Some(format!("No such shape: {name}. Shape names should be lowercase."));
                    } else if let Some(accepted) = NodeShape::accepted(shape) {
                        vertex.shape = Some(accepted);
                    } else {
                        shape_error = Some(format!("No such shape: {}.", shape.to_display()));
                    }
                }
                _ => shape_error = Some(format!("No such shape: {}.", json::stringify(shape))),
            }
        }
        if let Some(message) = shape_error {
            let at = error_at(cx, loc.expect("metadata has a place"));
            return Err(located(ErrorCode::Semantic, message, at));
        }
        if let Some(label) = fields.get("label").filter(|label| label.truthy()) {
            vertex.text = label.to_js_string();
            vertex.label_type = label_type(fields.get("labelType"));
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
        let max_edges = self.cx.limits.max_edges;
        if self.model.edges.len() >= max_edges {
            let message = format!("Edge limit exceeded. The limit is {max_edges}.");
            return Err(RenderError::Diagnostic(Diagnostic::new(ErrorCode::Limit, message)));
        }
        self.edge_by_id.entry(id.clone()).or_insert(self.model.edges.len());
        self.model.edges.push(AgentEdge {
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
            metadata: None,
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
    fn edge_at(&mut self, position: &[u16]) -> Result<&mut AgentEdge, RenderError> {
        let count = self.model.edges.len();
        let index = string_to_number(position);
        if index < count as f64 {
            return Ok(&mut self.model.edges[index as usize]);
        }
        let position = String::from_utf16_lossy(position);
        let last = count as i64 - 1;
        Err(self.cx.semantic(format!(
            "The index {position} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {last}. (Help: Ensure that the index is within the range of existing edges.)"
        )))
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
        let joined = styles.iter().map(|style| style.to_vec()).collect::<Vec<_>>().join(&unit(b','));
        // `\,` keeps its comma, and every other comma separates. Pele swaps the kept comma for
        // three section signs on the way, which no style can hold, so one pass reads the same.
        let mut style: Vec<Vec<u16>> = vec![Vec::new()];
        let mut at = 0;
        while at < joined.len() {
            let last = style.last_mut().expect("one style");
            if joined[at] == unit(b'\\') && joined.get(at + 1) == Some(&unit(b',')) {
                last.push(unit(b','));
                at += 2;
            } else if joined[at] == unit(b',') || joined[at] == unit(b';') {
                style.push(Vec::new());
                at += 1;
            } else {
                last.push(joined[at]);
                at += 1;
            }
        }
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
                class.styles.push(s.as_slice().into());
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

    /// Pele's `setClass`, for nodes, edges and flows alike.
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

    /// Pele's `addSubGraph`: the flow a closed `flow … end` describes, merged into an earlier
    /// one of the same id. Returns the id.
    pub(super) fn add_subgraph(
        &mut self,
        id: Option<&FlowText>,
        list: Vec<DocItem>,
        title: Option<&FlowText>,
    ) -> JsText {
        let id: Option<JsText> = id.map(|id| id.text.trim_js().into());
        let title_text: JsText = title.map_or(JsText::default(), |title| title.text.trim_js().into());

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

        // A flow never contains itself, a globally scoped node, or a node another flow already holds.
        let nodes: Vec<JsText> = node_list
            .into_iter()
            .filter(|node| *node != id && !self.global_nodes.contains(node) && !self.members.contains(node))
            .collect();
        self.members.extend(nodes.iter().cloned());

        match self.model.subgraph_index.get(&id) {
            Some(&at) => {
                // A repeated id adds its members to the first flow; its title and direction win.
                let existing = &mut self.model.subgraphs[at];
                existing.nodes.extend(nodes);
                if !title_text.is_empty() {
                    existing.title = title_text;
                }
                if dir.is_some() {
                    existing.dir = dir;
                }
            }
            None => {
                self.model.subgraph_index.insert(id.clone(), self.model.subgraphs.len());
                let label_type = title.map_or(LabelType::Markdown, |title| title.kind);
                self.model.subgraphs.push(AgentSubgraph {
                    id: id.clone(),
                    nodes,
                    title: title_text,
                    classes: Vec::new(),
                    dir,
                    label_type,
                    metadata: None,
                });
            }
        }
        id
    }

    /// Pele's `addGlobal`: every node named inside a `global` block stays out of the flows that
    /// mention it.
    pub(super) fn add_global(&mut self, list: Vec<DocItem>) {
        let mut release = false;
        let mut add = |item: &JsText| {
            let id: JsText = item.trim_js().into();
            if id.is_empty() {
                return;
            }
            release |= self.members.remove(&id);
            self.global_nodes.insert(id);
        };
        for entry in &list {
            match entry {
                DocItem::Text(text) => add(text),
                DocItem::Ids(ids) => ids.iter().for_each(&mut add),
                DocItem::Dir(_) => {}
            }
        }
        if release {
            for subgraph in &mut self.model.subgraphs {
                subgraph.nodes.retain(|node| !self.global_nodes.contains(node));
            }
        }
    }

    /// Pele's `addConnector`. Returns the connector's id, which is empty where it has none.
    pub(super) fn add_connector(&mut self, text: &FlowText, title: Option<&FlowText>) -> JsText {
        let id: JsText = text.text.trim_js().into();
        if id.is_empty() {
            return id;
        }
        let title: JsText = title.map_or(JsText::default(), |title| title.text.trim_js().into());
        if self.model.connectors.insert(id.clone()) {
            let created = AgentNode {
                id: id.clone(),
                text: if title.is_empty() { id.clone() } else { title },
                label_type: LabelType::Text,
                shape: None,
                styles: Vec::new(),
                classes: Vec::new(),
                props: None,
                link: None,
                link_target: None,
                metadata: None,
                is_connector: true,
            };
            self.model.nodes.insert(id.clone(), created);
        } else if !title.is_empty() {
            self.model.nodes[&id].text = title;
        }
        id
    }

    /// Pele's `position`. Mermaid builds its parser without ranges, so the indexes Pele also
    /// holds are always zero and are not kept here.
    fn position(&self, loc: Loc) -> ElementPosition {
        ElementPosition {
            start_line: loc.first.line + self.cx.line_offset,
            start_column: loc.first.column,
            end_line: loc.last.line + self.cx.line_offset,
            end_column: loc.last.column,
        }
    }

    fn push_mapping(&mut self, id: JsText, kind: StatementType, position: ElementPosition) {
        self.model.mappings.push(ElementMapping { id, kind, position });
    }

    pub(super) fn add_vertex_mapping(&mut self, id: &[u16], shaped: bool, loc: Loc) {
        if id.is_empty() {
            return;
        }
        self.push_mapping(id.into(), StatementType::Vertex, self.position(loc));
        if !shaped {
            self.bare_mappings.insert(self.model.mappings.len() - 1);
        }
    }

    /// Pele's `extendVertexMapping`: widens a node's span over the metadata block that follows it.
    pub(super) fn extend_vertex_mapping(&mut self, id: &[u16], loc: Loc) {
        if id.is_empty() {
            return;
        }
        let end = self.position(loc);
        let found = self.model.mappings.iter().rposition(|m| m.kind == StatementType::Vertex && *m.id == *id);
        let Some(at) = found else {
            self.push_mapping(id.into(), StatementType::Attachment, end);
            return;
        };
        let mapping = &mut self.model.mappings[at];
        if self.bare_mappings.remove(&at) {
            mapping.kind = StatementType::Attachment;
        }
        let position = &mut mapping.position;
        if (end.end_line, end.end_column) > (position.end_line, position.end_column) {
            position.end_line = end.end_line;
            position.end_column = end.end_column;
        }
    }

    pub(super) fn add_edge_mapping(&mut self, to_nodes: &[JsText], loc: Loc) {
        let ids: Vec<Vec<u16>> = to_nodes.iter().filter(|id| !id.is_empty()).map(|id| id.to_vec()).collect();
        let id: JsText = if ids.is_empty() { units("edge") } else { ids.join(&unit(b'>')).into() };
        self.push_mapping(id, StatementType::Edge, self.position(loc));
    }

    /// Pele's `blockMapping`: a block from where its keyword starts to where its last token ends.
    pub(super) fn add_block_mapping(&mut self, id: &[u16], kind: StatementType, start: Loc, end: Loc) {
        if id.is_empty() {
            return;
        }
        let (start, end) = (self.position(start), self.position(end));
        let position = ElementPosition { end_line: end.end_line, end_column: end.end_column, ..start };
        self.push_mapping(id.into(), kind, position);
    }
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
