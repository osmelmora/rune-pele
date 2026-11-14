//! Flowchart shape names (Pele's `src/diagrams/flowchart/shapes.ts`): every name a source may
//! give a shape, and the shape each one draws.

use crate::input::Units;

macro_rules! shapes {
    (
        documented: [$($doc:ident => [$($name:literal),+]),+ $(,)?],
        internal: [$($internal:ident => $internal_name:literal),+ $(,)?],
        syntax_only: [$($syntax:ident => $syntax_name:literal),+ $(,)?] $(,)?
    ) => {
        /// A shape as the drawing step knows it: Pele's `canonicalShape` of a name.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum Shape {
            $($doc,)+
            $($internal,)+
            $($syntax,)+
        }

        impl Shape {
            /// The canonical name, which is the shape's documented short name.
            pub(crate) fn name(self) -> &'static str {
                match self {
                    $(Shape::$doc => shapes!(@first $($name),+),)+
                    $(Shape::$internal => $internal_name,)+
                    $(Shape::$syntax => $syntax_name,)+
                }
            }
        }

        /// Every name `isValidShape` accepts, in Pele's table order, with its shape.
        static ACCEPTED: &[(&str, Shape)] = &[
            $($(($name, Shape::$doc),)+)+
            $(($internal_name, Shape::$internal),)+
        ];
    };
    (@first $first:literal $(, $rest:literal)*) => { $first };
}

shapes! {
    documented: [
        Rect => ["rect", "proc", "process", "rectangle", "squareRect"],
        Rounded => ["rounded", "event", "roundedRect"],
        Stadium => ["stadium", "terminal", "pill"],
        FrRect => ["fr-rect", "subprocess", "subproc", "framed-rectangle", "subroutine"],
        Cyl => ["cyl", "db", "database", "cylinder"],
        Datastore => ["datastore", "data-store"],
        Folder => ["folder", "directory"],
        Bucket => ["bucket"],
        Console => ["console"],
        Browser => ["browser"],
        Person => ["person"],
        Circle => ["circle", "circ"],
        Bang => ["bang"],
        Cloud => ["cloud"],
        Diam => ["diam", "decision", "diamond", "question"],
        Hex => ["hex", "hexagon", "prepare"],
        LeanR => ["lean-r", "lean-right", "in-out", "lean_right"],
        LeanL => ["lean-l", "lean-left", "out-in", "lean_left"],
        TrapB => ["trap-b", "priority", "trapezoid-bottom", "trapezoid"],
        TrapT => ["trap-t", "manual", "trapezoid-top", "inv-trapezoid", "inv_trapezoid"],
        DblCirc => ["dbl-circ", "double-circle", "doublecircle"],
        Text => ["text"],
        NotchRect => ["notch-rect", "card", "notched-rectangle"],
        LinRect => ["lin-rect", "lined-rectangle", "lined-process", "lin-proc", "shaded-process"],
        SmCirc => ["sm-circ", "start", "small-circle", "stateStart"],
        FrCirc => ["fr-circ", "stop", "framed-circle", "stateEnd"],
        Fork => ["fork", "join", "forkJoin"],
        Hourglass => ["hourglass", "collate"],
        Brace => ["brace", "comment", "brace-l"],
        BraceR => ["brace-r"],
        Braces => ["braces"],
        Bolt => ["bolt", "com-link", "lightning-bolt"],
        Doc => ["doc", "document"],
        Delay => ["delay", "half-rounded-rectangle"],
        HCyl => ["h-cyl", "das", "horizontal-cylinder"],
        LinCyl => ["lin-cyl", "disk", "lined-cylinder"],
        CurvTrap => ["curv-trap", "curved-trapezoid", "display"],
        DivRect => ["div-rect", "div-proc", "divided-rectangle", "divided-process"],
        Tri => ["tri", "extract", "triangle"],
        WinPane => ["win-pane", "internal-storage", "window-pane"],
        FCirc => ["f-circ", "junction", "filled-circle"],
        NotchPent => ["notch-pent", "loop-limit", "notched-pentagon"],
        FlipTri => ["flip-tri", "manual-file", "flipped-triangle"],
        SlRect => ["sl-rect", "manual-input", "sloped-rectangle"],
        Docs => ["docs", "documents", "st-doc", "stacked-document"],
        StRect => ["st-rect", "procs", "processes", "stacked-rectangle"],
        BowRect => ["bow-rect", "stored-data", "bow-tie-rectangle"],
        CrossCirc => ["cross-circ", "summary", "crossed-circle"],
        TagDoc => ["tag-doc", "tagged-document"],
        TagRect => ["tag-rect", "tagged-rectangle", "tag-proc", "tagged-process"],
        Flag => ["flag", "paper-tape"],
        Odd => ["odd", "rect_left_inv_arrow"],
        LinDoc => ["lin-doc", "lined-document"],
    ],
    internal: [
        State => "state",
        Choice => "choice",
        Note => "note",
        Composite => "composite",
        RectWithTitle => "rectWithTitle",
        LabelRect => "labelRect",
        BlockArrow => "block_arrow",
        CollapsedGroup => "collapsedGroup",
        IconSquare => "iconSquare",
        IconCircle => "iconCircle",
        Icon => "icon",
        IconRounded => "iconRounded",
        ImageSquare => "imageSquare",
        Anchor => "anchor",
        KanbanItem => "kanbanItem",
        MindmapCircle => "mindmapCircle",
        DefaultMindmapNode => "defaultMindmapNode",
        ClassBox => "classBox",
        ErBox => "erBox",
        RequirementBox => "requirementBox",
        UsecaseActor => "usecaseActor",
        UsecaseActorHollow => "usecaseActorHollow",
        UsecaseActorAwesome => "usecaseActorAwesome",
        UsecaseActorIcon => "usecaseActorIcon",
        UsecaseBusiness => "usecaseBusiness",
        UsecaseEllipse => "usecaseEllipse",
        UsecaseJsonTable => "usecaseJsonTable",
    ],
    syntax_only: [Ellipse => "ellipse"],
}

/// A shape name as the model holds it: one a source wrote, one the bracket syntax gives, or one
/// the flow graph picks. Each is a name `canonicalShape` maps to a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShapeName {
    name: &'static str,
    shape: Shape,
}

impl ShapeName {
    const fn new(name: &'static str, shape: Shape) -> Self {
        ShapeName { name, shape }
    }

    pub(crate) const SQUARE: ShapeName = ShapeName::new("square", Shape::Rect);
    pub(crate) const DOUBLECIRCLE: ShapeName = ShapeName::new("doublecircle", Shape::DblCirc);
    pub(crate) const CIRCLE: ShapeName = ShapeName::new("circle", Shape::Circle);
    pub(crate) const ROUND: ShapeName = ShapeName::new("round", Shape::Rounded);
    pub(crate) const ELLIPSE: ShapeName = ShapeName::new("ellipse", Shape::Ellipse);
    pub(crate) const STADIUM: ShapeName = ShapeName::new("stadium", Shape::Stadium);
    pub(crate) const SUBROUTINE: ShapeName = ShapeName::new("subroutine", Shape::FrRect);
    pub(crate) const RECT: ShapeName = ShapeName::new("rect", Shape::Rect);
    pub(crate) const CYLINDER: ShapeName = ShapeName::new("cylinder", Shape::Cyl);
    pub(crate) const HEXAGON: ShapeName = ShapeName::new("hexagon", Shape::Hex);
    pub(crate) const DIAMOND: ShapeName = ShapeName::new("diamond", Shape::Diam);
    pub(crate) const ODD: ShapeName = ShapeName::new("odd", Shape::Odd);
    pub(crate) const LEAN_RIGHT: ShapeName = ShapeName::new("lean_right", Shape::LeanR);
    pub(crate) const TRAPEZOID: ShapeName = ShapeName::new("trapezoid", Shape::TrapB);
    pub(crate) const LEAN_LEFT: ShapeName = ShapeName::new("lean_left", Shape::LeanL);
    pub(crate) const INV_TRAPEZOID: ShapeName = ShapeName::new("inv_trapezoid", Shape::TrapT);
    pub(crate) const SQUARE_RECT: ShapeName = ShapeName::new("squareRect", Shape::Rect);
    pub(crate) const ROUNDED_RECT: ShapeName = ShapeName::new("roundedRect", Shape::Rounded);
    pub(crate) const IMAGE_SQUARE: ShapeName = ShapeName::new("imageSquare", Shape::ImageSquare);
    pub(crate) const ICON: ShapeName = ShapeName::new("icon", Shape::Icon);
    pub(crate) const ICON_CIRCLE: ShapeName = ShapeName::new("iconCircle", Shape::IconCircle);
    pub(crate) const ICON_SQUARE: ShapeName = ShapeName::new("iconSquare", Shape::IconSquare);
    pub(crate) const ICON_ROUNDED: ShapeName = ShapeName::new("iconRounded", Shape::IconRounded);
    pub(crate) const COLLAPSED_GROUP: ShapeName = ShapeName::new("collapsedGroup", Shape::CollapsedGroup);

    /// Pele's `isValidShape`: a documented name, an alias or an internal name.
    pub(crate) fn accepted(name: &[u16]) -> Option<ShapeName> {
        ACCEPTED.iter().find(|(accepted, _)| name.is_str(accepted)).map(|&(name, shape)| ShapeName::new(name, shape))
    }

    pub(crate) fn as_str(self) -> &'static str {
        self.name
    }

    /// Pele's `canonicalShape`.
    pub(crate) fn shape(self) -> Shape {
        self.shape
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name the model can hold, with the shape it draws, asked of Pele's `canonicalShape`
    /// and `isValidShape`.
    #[test]
    fn shape_names_match_pele() {
        let mut names: Vec<ShapeName> = ACCEPTED.iter().map(|&(name, shape)| ShapeName::new(name, shape)).collect();
        names.extend([ShapeName::SQUARE, ShapeName::ROUND, ShapeName::ELLIPSE]);
        let requests: Vec<String> =
            names.iter().map(|name| format!("canonical_shape\t{}", crate::json_string(name.as_str()))).collect();
        for (name, theirs) in names.iter().zip(pele_oracle::ask(&requests)) {
            let units: Vec<u16> = name.as_str().encode_utf16().collect();
            let ours = format!("[{:?},{}]", name.shape().name(), ShapeName::accepted(&units).is_some());
            assert_eq!(ours, theirs, "shape of {}", name.as_str());
        }
    }
}
