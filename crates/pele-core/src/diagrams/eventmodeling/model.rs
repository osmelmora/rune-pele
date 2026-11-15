//! The event model Pele builds from the parsed frames, ported from `eventmodeling/model.ts`:
//! one lane per kind of entity and namespace, and the flows between frames.

use super::parser::{AstFrame, EventModelAst};
use crate::diagrams::railroad::close_indents;
use crate::diagrams::tokens::{acc_descr_value, acc_title_value, title_value};
use crate::input::{JsText, Units, is_space};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntityKind {
    Ui,
    Processor,
    Command,
    ReadModel,
    Event,
}

impl EntityKind {
    /// Pele's `KINDS`, keyed by the entity type a frame is written with.
    fn written(text: &[u16]) -> EntityKind {
        match String::from_utf16_lossy(text).as_str() {
            "ui" => EntityKind::Ui,
            "pcr" | "processor" => EntityKind::Processor,
            "rmo" | "readmodel" => EntityKind::ReadModel,
            "cmd" | "command" => EntityKind::Command,
            _ => EntityKind::Event,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            EntityKind::Ui => "ui",
            EntityKind::Processor => "processor",
            EntityKind::Command => "command",
            EntityKind::ReadModel => "readmodel",
            EntityKind::Event => "event",
        }
    }

    fn group(self) -> LaneGroup {
        match self {
            EntityKind::Ui | EntityKind::Processor => LaneGroup::Trigger,
            EntityKind::Event => LaneGroup::Stream,
            EntityKind::Command | EntityKind::ReadModel => LaneGroup::State,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LaneGroup {
    Trigger,
    State,
    Stream,
}

impl LaneGroup {
    pub(crate) fn name(self) -> &'static str {
        match self {
            LaneGroup::Trigger => "trigger",
            LaneGroup::State => "state",
            LaneGroup::Stream => "stream",
        }
    }

    /// Pele's `GROUPS`: the group's first index, its label, and the label's prefix for a namespace.
    fn defaults(self) -> (u32, &'static str, &'static str) {
        match self {
            LaneGroup::Trigger => (0, "UI/Automation", "UI/A: "),
            LaneGroup::State => (100, "Command/Read Model", "C/RM: "),
            LaneGroup::Stream => (200, "Events", "Stream: "),
        }
    }
}

pub(crate) struct Lane {
    /// Mermaid's ordering key.
    pub(crate) index: u32,
    pub(crate) label: JsText,
    pub(crate) group: LaneGroup,
    pub(crate) namespace: Option<JsText>,
}

pub(crate) struct EmBox {
    /// The frame number as written, such as `01`.
    pub(crate) id: JsText,
    pub(crate) kind: EntityKind,
    pub(crate) entity: JsText,
    pub(crate) name: JsText,
    pub(crate) namespace: Option<JsText>,
    pub(crate) reset: bool,
    /// The place of the box's lane in [`EventModel::lanes`].
    pub(crate) lane: usize,
    pub(crate) data: Option<JsText>,
    pub(crate) data_type: Option<JsText>,
}

pub(crate) struct Note {
    pub(crate) frame: JsText,
    pub(crate) text: JsText,
    pub(crate) data_type: Option<JsText>,
}

pub(crate) struct Specification {
    pub(crate) frame: JsText,
    pub(crate) given: Vec<JsText>,
    pub(crate) when: Vec<JsText>,
    pub(crate) then: Vec<JsText>,
}

/// A flow from one box to another, each by its place in [`EventModel::boxes`]. A frame that
/// names itself as its source flows into itself.
#[derive(Clone, Copy)]
pub(crate) struct Relation {
    pub(crate) source: usize,
    pub(crate) target: usize,
}

/// Pele's `EventModelingModel`.
pub(crate) struct EventModel {
    pub(crate) title: Option<JsText>,
    pub(crate) acc_title: Option<JsText>,
    pub(crate) acc_descr: Option<JsText>,
    pub(crate) boxes: Vec<EmBox>,
    /// Sorted from top to bottom.
    pub(crate) lanes: Vec<Lane>,
    pub(crate) relations: Vec<Relation>,
    pub(crate) notes: Vec<Note>,
    pub(crate) specifications: Vec<Specification>,
    pub(crate) entities: Vec<JsText>,
}

/// Pele's `blockText`: the text between the outer braces, or inside the quotes, without the
/// indentation its lines share.
fn block_text(raw: &[u16]) -> JsText {
    let open = raw.find_unit(u16::from(b'{'), 0);
    let close = raw.iter().rposition(|&c| c == u16::from(b'}'));
    let inner: &[u16] = match (open, close) {
        (Some(open), Some(close)) if close > open => &raw[open + 1..close],
        (None, _) if raw.len() >= 2 => &raw[1..raw.len() - 1],
        _ => &[],
    };
    let unreturned: Vec<u16> = inner.iter().copied().filter(|&c| c != 13).collect();
    let mut lines: Vec<&[u16]> = unreturned.split(|&c| c == 10).collect();
    let blank = |line: &[u16]| line.trim_js().is_empty();
    while lines.first().is_some_and(|line| blank(line)) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| blank(line)) {
        lines.pop();
    }
    let indent =
        lines.iter().filter(|line| !blank(line)).map(|line| line.iter().take_while(|&&c| is_space(c)).count()).min();
    let mut out = Vec::with_capacity(unreturned.len());
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push(10);
        }
        let from = indent.map_or(line.len(), |indent| indent.min(line.len()));
        out.extend_from_slice(line[from..].trim_end_js());
    }
    out.into()
}

/// Where a frame starts, for the error that names it twice.
pub(crate) struct Duplicate {
    pub(crate) name: JsText,
    pub(crate) offset: usize,
}

/// Pele's `buildEventModel`. `title` is what front matter sets.
pub(crate) fn build_event_model(ast: EventModelAst, title: Option<&JsText>) -> Result<EventModel, Duplicate> {
    let mut seen = HashSet::new();
    for frame in &ast.frames {
        if !seen.insert(&frame.name[..]) {
            return Err(Duplicate { name: frame.name.as_slice().into(), offset: frame.offset });
        }
    }

    let mut data: HashMap<&[u16], &[u16]> = HashMap::new();
    for entity in &ast.data {
        data.entry(&entity.name).or_insert(&entity.block);
    }

    // One lane per group, and one more per namespace within a group, in the order they are met.
    let mut lanes: Vec<Lane> = Vec::new();
    let mut lane_of: HashMap<(LaneGroup, Option<Vec<u16>>), usize> = HashMap::new();
    let mut next: HashMap<LaneGroup, u32> = HashMap::new();
    let mut lane = |kind: EntityKind, namespace: Option<&[u16]>| -> usize {
        let group = kind.group();
        *lane_of.entry((group, namespace.map(<[u16]>::to_vec))).or_insert_with(|| {
            let (first, label, prefix) = group.defaults();
            let mut index = first;
            let label: JsText = match namespace {
                None => label.into(),
                Some(namespace) => {
                    index = *next.get(&group).unwrap_or(&(first + 1));
                    next.insert(group, index + 1);
                    let mut text = JsText::from(prefix);
                    text.push_units(namespace);
                    text
                }
            };
            lanes.push(Lane { index, label, group, namespace: namespace.map(JsText::from) });
            lanes.len() - 1
        })
    };

    let mut boxes: Vec<EmBox> = Vec::new();
    let mut relations = Vec::new();
    for (index, frame) in ast.frames.iter().enumerate() {
        let AstFrame { reset, name, entity_type, entity, sources, data_reference, data_type, inline, .. } = frame;
        let parts: Vec<&[u16]> = entity.split(|&c| c == u16::from(b'.')).collect();
        let namespace = (parts.len() == 2).then(|| parts[0]);
        let kind = EntityKind::written(entity_type);
        let mut text = inline.as_deref().map(block_text);
        if let Some(block) = data_reference.as_deref().and_then(|reference| data.get(reference)) {
            text = Some(block_text(block));
        }
        let lane = lane(kind, namespace);
        boxes.push(EmBox {
            id: name.as_slice().into(),
            kind,
            entity: entity.as_slice().into(),
            name: if parts.len() == 2 { parts[1].into() } else { entity.as_slice().into() },
            namespace: namespace.map(JsText::from),
            reset: *reset,
            lane,
            data: text,
            data_type: data_type.as_deref().map(JsText::from),
        });

        if !sources.is_empty() {
            // Sources connect in frame order, and only frames that already have a box.
            let wanted: HashSet<&[u16]> = sources.iter().map(Vec::as_slice).collect();
            for (source, earlier) in boxes.iter().enumerate() {
                if wanted.contains(&earlier.id[..]) {
                    relations.push(Relation { source, target: index });
                }
            }
        } else if index > 0 && !reset {
            // Otherwise the nearest earlier frame in another lane feeds this one.
            if let Some(source) = (0..index).rev().find(|&i| boxes[i].lane != lane) {
                relations.push(Relation { source, target: index });
            }
        }
    }

    // Sorted by index, the lanes keep the order they were met in where indexes tie.
    let mut order: Vec<usize> = (0..lanes.len()).collect();
    order.sort_by_key(|&lane| lanes[lane].index);
    let mut place = vec![0; lanes.len()];
    for (sorted, &lane) in order.iter().enumerate() {
        place[lane] = sorted;
    }
    for item in &mut boxes {
        item.lane = place[item.lane];
    }
    let mut slots: Vec<Option<Lane>> = lanes.into_iter().map(Some).collect();
    let lanes = order.iter().map(|&lane| slots[lane].take().expect("each lane once")).collect();

    let title = ast
        .title
        .as_deref()
        .map(title_value)
        .filter(|title| !title.is_empty())
        .map(JsText::from)
        .or_else(|| title.cloned());
    let blocks = |names: &[Vec<u16>]| names.iter().map(|name| name.as_slice().into()).collect();
    Ok(EventModel {
        title,
        acc_title: ast.acc_title.as_deref().map(|token| acc_title_value(token).trim_start_js().into()),
        acc_descr: ast.acc_descr.as_deref().map(|token| close_indents(&acc_descr_value(token))),
        boxes,
        lanes,
        relations,
        notes: ast
            .notes
            .iter()
            .map(|note| Note {
                frame: note.frame.as_slice().into(),
                text: block_text(&note.block),
                data_type: note.data_type.as_deref().map(JsText::from),
            })
            .collect(),
        specifications: ast
            .specifications
            .iter()
            .map(|spec| Specification {
                frame: spec.frame.as_slice().into(),
                given: blocks(&spec.given),
                when: blocks(&spec.when),
                then: blocks(&spec.then),
            })
            .collect(),
        entities: ast.entities.iter().map(|name| name.as_slice().into()).collect(),
    })
}
