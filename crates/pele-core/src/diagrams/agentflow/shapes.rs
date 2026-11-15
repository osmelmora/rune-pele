//! Agentflow shape names (Pele's `src/diagrams/agentflow/shapes.ts`): the names an author may
//! give a shape, and the few shapes agentflow draws.

use crate::diagrams::flowchart::ShapeName;
use crate::input::Units;

/// A shape name a node holds: one the bracket syntax gives, or one a source wrote that
/// `isValidShape` accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeShape {
    /// A flowchart shape name.
    Named(ShapeName),
    /// Mermaid looks a shape name up with `in` on a plain object, so `constructor` passes too.
    Constructor,
}

impl NodeShape {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            NodeShape::Named(name) => name.as_str(),
            NodeShape::Constructor => "constructor",
        }
    }

    /// Pele's `isValidShape`.
    pub(super) fn accepted(name: &[u16]) -> Option<NodeShape> {
        if name.is_str("constructor") {
            return Some(NodeShape::Constructor);
        }
        ShapeName::accepted(name).map(NodeShape::Named)
    }
}

/// Author-facing names for the shapes that carry meaning in an agentflow.
const SHAPE_ALIASES: [(&str, &str); 7] = [
    ("task", "roundedRect"),
    ("tool", "subroutine"),
    ("input", "lean-right"),
    ("decision", "diamond"),
    ("refdoc", "lin-doc"),
    ("action", "hexagon"),
    ("round", "rect"),
];

/// The shape an author-facing name stands for, where it is one.
pub(super) fn alias_of(name: &[u16]) -> Option<&'static str> {
    SHAPE_ALIASES.iter().find(|(alias, _)| name.is_str(alias)).map(|&(_, shape)| shape)
}

/// Pele's `resolveShapeAlias`.
pub(super) fn resolve_alias(name: &str) -> &str {
    SHAPE_ALIASES.iter().find(|(alias, _)| *alias == name).map_or(name, |&(_, shape)| shape)
}

/// Pele's `isToolDefinition`, for a node's shape name.
pub(super) fn is_tool(name: &str) -> bool {
    matches!(name, "subroutine" | "subprocess" | "subproc" | "framed-rectangle" | "tool")
}

const REMOVED_SHAPES: [&str; 27] = [
    "doc",
    "stadium",
    "terminal",
    "circle",
    "trapezoid",
    "inv_trapezoid",
    "inv-trapezoid",
    "doublecircle",
    "double-circle",
    "typeDeclaration",
    "procs",
    "lean_left",
    "lean-left",
    "in-out",
    "cylinder",
    "ellipse",
    "odd",
    "tag-rect",
    "tagged-rectangle",
    "delay",
    "half-rounded-rectangle",
    "lin-rect",
    "lined-rectangle",
    "win-pane",
    "window-pane",
    "curv-trap",
    "curved-trapezoid",
];

// Pele's list also names `connector`, which no node can hold: it is no shape `isValidShape`
// accepts and no bracket gives it.
const ALLOWED_SHAPES: [&str; 12] = [
    "roundedRect",
    "subroutine",
    "subprocess",
    "subproc",
    "framed-rectangle",
    "lean-right",
    "diamond",
    "lin-doc",
    "lined-document",
    "hexagon",
    "hex",
    "collapsedGroup",
];

pub(super) const DEFAULT_SHAPE: &str = "roundedRect";

/// What Pele's `normaliseNodeShapes` makes of a node's shape name.
pub(super) enum Normalised {
    /// A shape agentflow draws.
    Drawn(ShapeName),
    /// A shape Mermaid removed, which is drawn as the default and reported as an error.
    Removed,
    /// Any other shape, which is drawn as the default and reported as a warning.
    Unsupported,
}

pub(super) fn normalise(name: &str) -> Normalised {
    let name = if matches!(name, "squareRect" | "rect") { DEFAULT_SHAPE } else { name };
    if REMOVED_SHAPES.contains(&name) {
        return Normalised::Removed;
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    match ShapeName::accepted(&units).filter(|_| ALLOWED_SHAPES.contains(&name)) {
        Some(shape) => Normalised::Drawn(shape),
        None => Normalised::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_allowed_shape_is_one_the_flowchart_shapes_draw() {
        for name in ALLOWED_SHAPES {
            assert!(matches!(normalise(name), Normalised::Drawn(shape) if shape.as_str() == name), "{name}");
        }
        assert!(matches!(normalise("rect"), Normalised::Drawn(shape) if shape == ShapeName::ROUNDED_RECT));
        assert!(matches!(normalise("stadium"), Normalised::Removed));
        assert!(matches!(normalise("constructor"), Normalised::Unsupported));
    }
}
