//! The one list of diagram kinds: what Pele calls each and how Mermaid detects it.
//!
//! The detectors are Pele's `DETECTORS` (`src/detect.ts`). Each regular expression there is a
//! start-of-text match, so it is written as the words it may start with and what must follow.

use crate::diagrams::tokens::SyntaxError;
use crate::diagrams::{self};
use crate::input::{JsObject, JsText, Units, is_space, is_word};
use crate::measure::TextMeasurer;
use crate::scene::{PaletteRule, Scene, SourcePalette};
use crate::{Diagnostic, ErrorCode, LayoutOptions, Limits, Origin, RenderError};
use std::sync::LazyLock;

/// What must follow a detector's word.
#[derive(Clone, Copy)]
enum End {
    Anything,
    /// `\b`
    WordBoundary,
    /// `(?:\s|$)`
    Space,
    /// `(?:[\s:]|$)`
    SpaceOrColon,
}

/// `^\s*(?:word|word)…`, where the first word that is followed by `end` matches.
#[derive(Clone, Copy)]
struct Pattern {
    words: &'static [&'static str],
    caseless: bool,
    end: End,
}

const fn starts(words: &'static [&'static str]) -> Pattern {
    Pattern { words, caseless: false, end: End::Anything }
}

impl Pattern {
    const fn caseless(mut self) -> Self {
        self.caseless = true;
        self
    }

    const fn then(mut self, end: End) -> Self {
        self.end = end;
        self
    }

    fn matches(&self, text: &[u16]) -> bool {
        let start = text.run(0, is_space);
        self.words.iter().any(|word| {
            let end = start + word.len();
            let found = text.get(start..end).is_some_and(|found| {
                found.iter().zip(word.bytes()).all(|(&unit, byte)| {
                    unit == u16::from(byte)
                        || (self.caseless && u8::try_from(unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(&byte)))
                })
            });
            let next = text.at(end);
            found
                && match self.end {
                    End::Anything => true,
                    End::WordBoundary => next.is_none_or(|c| !is_word(c)),
                    End::Space => next.is_none_or(is_space),
                    End::SpaceOrColon => next.is_none_or(|c| is_space(c) || c == u16::from(b':')),
                }
        })
    }
}

struct Detector {
    rank: u8,
    mermaid: &'static str,
    kind: DiagramKind,
    pattern: Pattern,
}

/// The diagram type as Mermaid reports it, such as `flowchart-v2` for `graph TD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MermaidType {
    name: &'static str,
}

impl MermaidType {
    pub fn name(self) -> &'static str {
        self.name
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Detected {
    pub(crate) kind: DiagramKind,
    pub(crate) mermaid: MermaidType,
}

/// What a diagram's parser is given.
pub(crate) struct ParseContext<'a> {
    /// The preprocessed source with its entity codes hidden, ending in a line break.
    pub(crate) text: &'a [u16],
    /// The title front matter sets.
    pub(crate) title: Option<&'a JsText>,
    pub(crate) origin: &'a Origin<'a>,
    /// The source's config laid over the host's.
    pub(crate) config: &'a JsObject,
    pub(crate) limits: Limits,
}

impl ParseContext<'_> {
    /// Pele's `syntaxError`: the message names the line and shows it with a caret.
    pub(crate) fn syntax(&self, error: SyntaxError) -> RenderError {
        let location = self.origin.locate(self.text, error.at);
        let message = if error.lexical {
            format!("Lexical error on line {}. Unrecognized text.\n{}", location.line, location.snippet)
        } else {
            format!("Parse error on line {}:\n{}\n{}", location.line, location.snippet, error.detail)
        };
        RenderError::Diagnostic(Diagnostic { location: Some(location), ..Diagnostic::new(ErrorCode::Syntax, message) })
    }

    pub(crate) fn semantic(&self, message: String) -> RenderError {
        RenderError::Diagnostic(Diagnostic::new(ErrorCode::Semantic, message))
    }
}

/// What a diagram's draw step is given. It holds no drawing state: each attempt builds its own.
pub(crate) struct DrawContext<'a> {
    pub(crate) kind: DiagramKind,
    pub(crate) measure: &'a dyn TextMeasurer,
    pub(crate) options: &'a LayoutOptions,
    /// The source's config laid over the host's.
    pub(crate) config: &'a JsObject,
    pub(crate) palette: &'a SourcePalette,
    /// Whether the drawing scales down with its container.
    pub(crate) responsive: bool,
    pub(crate) limits: Limits,
}

/// What Pele's pipeline needs to know about a ported kind besides its parser and draw step.
#[derive(Clone, Copy)]
pub(crate) struct PortPolicy {
    /// The kind's section of Mermaid's config.
    pub(crate) section: &'static str,
    pub(crate) palette: PaletteRule,
}

/// One diagram type's whole pipeline. A module that implements it is a port.
pub(crate) trait Diagram {
    type Model;
    fn parse(cx: &ParseContext<'_>) -> Result<Self::Model, RenderError>;
    /// Draws the model. A type that adapts to the width owns its sequence of attempts, and the
    /// model is the same for each.
    fn scene(model: &Self::Model, cx: &DrawContext<'_>) -> Result<Scene, RenderError>;
}

/// One registration per kind. `rank` is the detector's position in Mermaid's order, where the
/// first match wins. `port` names the module that renders the kind, once there is one, with
/// the kind's config section and palette variables.
macro_rules! diagram_catalog {
    ($($kind:ident => {
        name: $name:literal,
        $(port: { diagram: $port:ty, section: $section:literal, palette: $palette:expr },)?
        detectors: [$(($rank:literal, $mermaid:literal, $pattern:expr)),+ $(,)?] $(,)?
    }),+ $(,)?) => {
        /// A diagram type, named as Pele names it.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum DiagramKind {
            $($kind),+
        }

        impl DiagramKind {
            pub const ALL: &[DiagramKind] = &[$(DiagramKind::$kind),+];

            /// Pele's `DiagramType` string, which is also the root element's class suffix.
            pub fn name(self) -> &'static str {
                match self {
                    $(DiagramKind::$kind => $name),+
                }
            }
        }

        impl DiagramKind {
            /// The policy of the kind's port, if it has one.
            pub(crate) fn port(self) -> Option<PortPolicy> {
                match self {
                    $($(DiagramKind::$kind => Some(PortPolicy { section: $section, palette: $palette }),)?)+
                    _ => None,
                }
            }
        }

        /// What a ported kind's parser made of a source.
        #[allow(clippy::large_enum_variant, reason = "a source has one model, so the larger kinds cost nothing")]
        pub(crate) enum PreparedModel {
            $($($kind(<$port as Diagram>::Model),)?)+
        }

        /// Parses with the kind's port, or reports that it has none.
        pub(crate) fn parse(kind: DiagramKind, cx: &ParseContext<'_>) -> Result<PreparedModel, RenderError> {
            match kind {
                $($(DiagramKind::$kind => <$port as Diagram>::parse(cx).map(PreparedModel::$kind),)?)+
                _ => Err(RenderError::NotPorted),
            }
        }

        /// Draws with the port that parsed the model.
        pub(crate) fn scene(model: &PreparedModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
            match model {
                $($(PreparedModel::$kind(model) => <$port as Diagram>::scene(model, cx),)?)+
            }
        }

        static DETECTORS: LazyLock<Vec<Detector>> = LazyLock::new(|| {
            let mut detectors = vec![
                $($(Detector { rank: $rank, mermaid: $mermaid, kind: DiagramKind::$kind, pattern: $pattern }),+),+
            ];
            detectors.sort_by_key(|detector| detector.rank);
            detectors
        });
    };
}

diagram_catalog! {
    Flowchart => {
        name: "flowchart",
        port: { diagram: diagrams::flowchart::Flowchart, section: "flowchart", palette: PaletteRule::None },
        detectors: [
            (0, "flowchart-elk", starts(&["flowchart-elk"])),
            (14, "flowchart-v2", starts(&["graph", "flowchart"])),
        ],
    },
    Mindmap => { name: "mindmap", detectors: [(1, "mindmap", starts(&["mindmap"]))] },
    Architecture => { name: "architecture", detectors: [(2, "architecture", starts(&["architecture"]))] },
    Agentflow => { name: "agentflow", detectors: [
        (3, "agentflow", starts(&["agentflow-beta"]).then(End::WordBoundary)),
    ] },
    C4 => { name: "c4", detectors: [
        (4, "c4", starts(&["C4Context", "C4Container", "C4Component", "C4Dynamic", "C4Deployment"])),
    ] },
    Kanban => { name: "kanban", detectors: [(5, "kanban", starts(&["kanban"]))] },
    Class => {
        name: "class",
        port: { diagram: diagrams::class::Class, section: "class", palette: PaletteRule::None },
        detectors: [(6, "classDiagram", starts(&["classDiagram"]))],
    },
    Er => { name: "er", detectors: [(7, "er", starts(&["erDiagram"]))] },
    Gantt => { name: "gantt", detectors: [(8, "gantt", starts(&["gantt"]))] },
    Info => { name: "info", detectors: [(9, "info", starts(&["info"]))] },
    Pie => {
        name: "pie",
        port: { diagram: diagrams::pie::Pie, section: "pie", palette: PaletteRule::Slots("pie", 1) },
        detectors: [(10, "pie", starts(&["pie"]))],
    },
    Requirement => { name: "requirement", detectors: [(11, "requirement", starts(&["requirement"]))] },
    Sequence => { name: "sequence", detectors: [(12, "sequence", starts(&["sequenceDiagram"]))] },
    Swimlane => { name: "swimlane", detectors: [
        (13, "swimlane", starts(&["swimlane-beta"]).then(End::WordBoundary)),
    ] },
    Timeline => { name: "timeline", detectors: [(15, "timeline", starts(&["timeline"]))] },
    GitGraph => { name: "gitGraph", detectors: [(16, "gitGraph", starts(&["gitGraph"]))] },
    State => {
        name: "state",
        port: { diagram: diagrams::state::State, section: "state", palette: PaletteRule::None },
        detectors: [(17, "stateDiagram", starts(&["stateDiagram"]))],
    },
    Journey => { name: "journey", detectors: [(18, "journey", starts(&["journey"]))] },
    QuadrantChart => { name: "quadrantChart", detectors: [(19, "quadrantChart", starts(&["quadrantChart"]))] },
    Sankey => { name: "sankey", detectors: [(20, "sankey", starts(&["sankey"]))] },
    Packet => { name: "packet", detectors: [(21, "packet", starts(&["packet"]))] },
    Xychart => { name: "xychart", detectors: [(22, "xychart", starts(&["xychart"]))] },
    Block => { name: "block", detectors: [(23, "block", starts(&["block"]))] },
    Eventmodeling => { name: "eventmodeling", detectors: [(24, "eventmodeling", starts(&["eventmodeling"]))] },
    TreeView => { name: "treeView", detectors: [(25, "treeView", starts(&["treeView-beta"]))] },
    Radar => { name: "radar", detectors: [(26, "radar", starts(&["radar-beta"]))] },
    Ishikawa => { name: "ishikawa", detectors: [
        (27, "ishikawa", starts(&["ishikawa-beta", "ishikawa"]).caseless().then(End::WordBoundary)),
    ] },
    Treemap => { name: "treemap", detectors: [(28, "treemap", starts(&["treemap"]))] },
    Railroad => { name: "railroad", detectors: [
        (29, "railroad", starts(&["railroad-beta"]).caseless()),
        (30, "railroadEbnf", starts(&["railroad-ebnf-beta"]).caseless()),
        (31, "railroadAbnf", starts(&["railroad-abnf-beta"]).caseless()),
        (32, "railroadPeg", starts(&["railroad-peg-beta"]).caseless()),
    ] },
    Venn => { name: "venn", detectors: [(33, "venn", starts(&["venn-beta"]))] },
    Wardley => { name: "wardley", detectors: [(34, "wardley", starts(&["wardley-beta"]).caseless())] },
    Cynefin => { name: "cynefin", detectors: [(35, "cynefin", starts(&["cynefin-beta"]).then(End::SpaceOrColon))] },
    Usecase => { name: "usecase", detectors: [(36, "usecase", starts(&["usecase-beta"]).then(End::Space))] },
}

/// Pele's `detectDiagram`, on text that preprocessing has cleaned.
pub(crate) fn detect(cleaned: &[u16]) -> Option<Detected> {
    DETECTORS
        .iter()
        .find(|detector| detector.pattern.matches(cleaned))
        .map(|detector| Detected { kind: detector.kind, mermaid: MermaidType { name: detector.mermaid } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_every_kind_and_detector_once() {
        assert_eq!(DiagramKind::ALL.len(), 33);
        let ranks: Vec<u8> = DETECTORS.iter().map(|detector| detector.rank).collect();
        assert_eq!(ranks, (0..=36).collect::<Vec<u8>>());
    }

    /// Every detector word with the endings that tell the patterns apart, asked of Pele's detectors.
    #[test]
    fn detect_matches_pele() {
        let mut probes = vec![String::new(), "hello there".to_owned(), "  \n".to_owned()];
        for detector in DETECTORS.iter() {
            for word in detector.pattern.words {
                let cased =
                    [word.to_string(), word.to_uppercase(), word.to_lowercase(), word[..word.len() - 1].to_owned()];
                for stem in cased {
                    for ending in [
                        "", " ", "\n", ":", "x", "X", "_", "-", "-beta", "-betax", "-beta:", "-beta-", "Diagram",
                        "\u{e9}", "\u{a0}",
                    ] {
                        probes.push(format!("{stem}{ending}"));
                        probes.push(format!(" \n\t{stem}{ending} TD\n a --> b"));
                    }
                }
            }
        }
        let requests: Vec<String> = probes
            .iter()
            .map(|probe| {
                format!("detect\t{}", format!("{probe:?}").replace("\\u{e9}", "\\u00e9").replace("\\u{a0}", "\\u00a0"))
            })
            .collect();
        let theirs = pele_oracle::ask(&requests);
        assert!(probes.len() > 3000);
        for (probe, theirs) in probes.iter().zip(&theirs) {
            let units: Vec<u16> = probe.encode_utf16().collect();
            let ours = detect(&units).map_or("null".to_owned(), |found| {
                format!("{{\"type\":\"{}\",\"mermaidType\":\"{}\"}}", found.kind.name(), found.mermaid.name())
            });
            assert_eq!(&ours, theirs, "detection of {probe:?}");
        }
    }
}
