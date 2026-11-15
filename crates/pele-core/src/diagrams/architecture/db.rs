//! Pele's `ArchitectureDb` and `populate`: Mermaid's checks on what the statements declare, with
//! its messages.

use super::parser::{ArchitectureAst, EndAst};
use super::{ArchitectureModel, End, Group, LayoutHint, Node, NodeKind, Service};
use crate::input::{JsText, Units, is_space};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
enum Registered {
    Node(usize),
    Group(usize),
}

impl Registered {
    fn name(self) -> &'static str {
        match self {
            Registered::Node(_) => "node",
            Registered::Group(_) => "group",
        }
    }
}

struct Db {
    nodes: Vec<Node>,
    groups: Vec<Group>,
    registered: HashMap<Vec<u16>, Registered>,
}

fn shown(id: &[u16]) -> String {
    String::from_utf16_lossy(id)
}

impl Db {
    /// Checks a new id and its parent, and answers the place of the parent group.
    fn register(&self, kind: &str, id: &[u16], parent: Option<&[u16]>) -> Result<Option<usize>, String> {
        if let Some(found) = self.registered.get(id) {
            return Err(format!("The {kind} id [{}] is already in use by another {}", shown(id), found.name()));
        }
        let Some(parent) = parent else {
            return Ok(None);
        };
        if id == parent {
            return Err(format!("The {kind} [{}] cannot be placed within itself", shown(id)));
        }
        match self.registered.get(parent) {
            None => Err(format!(
                "The {kind} [{}]'s parent does not exist. Please make sure the parent is created before this {kind}",
                shown(id)
            )),
            Some(Registered::Node(_)) => Err(format!("The {kind} [{}]'s parent is not a group", shown(id))),
            Some(&Registered::Group(group)) => Ok(Some(group)),
        }
    }

    fn add_node(&mut self, kind: &str, id: Vec<u16>, parent: Option<&[u16]>, node: NodeKind) -> Result<(), String> {
        let parent = self.register(kind, &id, parent)?;
        self.registered.insert(id.clone(), Registered::Node(self.nodes.len()));
        self.nodes.push(Node { id: id.into(), kind: node, parent });
        Ok(())
    }

    fn end(&self, end: &EndAst) -> End {
        let Some(&Registered::Node(node)) = self.registered.get(&end.id) else {
            unreachable!("an edge's ends are checked to be nodes")
        };
        End { node, side: end.side, into: end.into, via_group: end.via_group }
    }
}

/// `text.replace(/\n\s+/g, '\n')`
fn unindent(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        out.push(text[at]);
        let spaces = text.run(at + 1, is_space);
        at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
    }
    out
}

/// Mermaid's `populateDb`: every group, then services, junctions, edges, and alignments.
pub(super) fn populate(ast: ArchitectureAst, front_title: Option<&JsText>) -> Result<ArchitectureModel, String> {
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    let mut db = Db { nodes: Vec::new(), groups: Vec::new(), registered: HashMap::new() };

    for group in ast.groups {
        let parent = db.register("group", &group.id, group.parent.as_deref())?;
        db.registered.insert(group.id.clone(), Registered::Group(db.groups.len()));
        let (icon, title) = (group.icon.map(JsText::from), group.title.map(JsText::from));
        db.groups.push(Group { id: group.id.into(), icon, title, parent });
    }
    for service in ast.services {
        let kind = NodeKind::Service(Service {
            icon: service.icon.map(JsText::from),
            icon_text: service.icon_text.map(JsText::from),
            title: service.title.map(JsText::from),
        });
        db.add_node("service", service.id, service.parent.as_deref(), kind)?;
    }
    for junction in ast.junctions {
        db.add_node("junction", junction.id, junction.parent.as_deref(), NodeKind::Junction)?;
    }

    let mut edges = Vec::with_capacity(ast.edges.len());
    for edge in ast.edges {
        let ends = [("left", &edge.lhs), ("right", &edge.rhs)];
        for (side, end) in ends {
            if !db.registered.contains_key(&end.id) {
                return Err(format!(
                    "The {side}-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    shown(&end.id)
                ));
            }
        }
        // Mermaid lets a group id through the check above and then fails with a TypeError.
        for (side, end) in ends {
            if matches!(db.registered[&end.id], Registered::Group(_)) {
                return Err(format!(
                    "The {side}-hand id [{}] is a group. Add {{group}} to a service inside it instead.",
                    shown(&end.id)
                ));
            }
        }
        let (lhs, rhs) = (db.end(&edge.lhs), db.end(&edge.rhs));
        let parent = db.nodes[lhs.node].parent;
        if parent.is_some() && parent == db.nodes[rhs.node].parent {
            for (side, end) in ends {
                if end.via_group {
                    return Err(format!(
                        "The {side}-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                        shown(&end.id)
                    ));
                }
            }
        }
        edges.push(super::Edge { lhs, rhs, title: edge.title.map(JsText::from) });
    }

    let mut hints = Vec::with_capacity(ast.alignments.len());
    for alignment in ast.alignments {
        let direction = alignment.direction.name();
        let mut seen = HashSet::new();
        let mut members = Vec::with_capacity(alignment.members.len());
        for id in &alignment.members {
            let Some(&Registered::Node(node)) = db.registered.get(id) else {
                return Err(format!(
                    "align {direction} references [{}], which is not a service or junction",
                    shown(id)
                ));
            };
            if !seen.insert(node) {
                return Err(format!("align {direction} lists [{}] more than once", shown(id)));
            }
            members.push(node);
        }
        hints.push(LayoutHint { direction: alignment.direction, members });
    }

    Ok(ArchitectureModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        acc_title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(ast.acc_descr).map(|text| unindent(&text).into()),
        nodes: db.nodes,
        groups: db.groups,
        edges,
        hints,
    })
}
