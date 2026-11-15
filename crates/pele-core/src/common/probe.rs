//! Entry points that let `tests/common.rs` hold the shared helpers to Pele. Each takes plain
//! values, calls the crate's own helper, and answers in plain values, so the helpers' types stay
//! private to the crate.

use crate::Script;
use crate::catalog::DrawContext;
use crate::diagrams::tokens::{self, Matcher, TokenType};
use crate::diagrams::{outline, scan};
use crate::input::{self, JsObject, JsValue, Units};
use crate::measure::{self, ArialMetrics, Emphasis, Label, LabelLine, LabelRun};
use crate::scene::{
    Accessible, Entity, Facet, LabelLook, Number, Paint, PaletteRule, Role, Scene, SceneBuilder, SeriesSlot, Size,
    SourcePalette, Style, TextWeight,
};
use crate::scene::{BaselineRun, Draw, Geometry, PaintVisit};
use crate::{DiagramKind, LayoutOptions, Limits, ParityScene, fit, svg};

/// The classes of `util/chars.ts` a UTF-16 code unit is in, one bit each: `isSpace` 1, `isBlank`
/// 2, `isLineEnd` 4, `isDigit` 8, `isLetter` 16, `isHex` 32 and `isWord` 64.
pub fn char_classes(c: u16) -> u8 {
    let tests: [fn(u16) -> bool; 7] = [
        input::is_space,
        input::is_blank,
        input::is_line_end,
        input::is_digit,
        input::is_letter,
        input::is_hex,
        input::is_word,
    ];
    tests.iter().enumerate().map(|(bit, test)| u8::from(test(c)) << bit).sum()
}

/// One call of a lexing helper on a text.
#[derive(Clone, Debug)]
pub enum ScanCall {
    LineEnd(usize),
    KeywordAt(usize, String),
    WordAt(usize, String),
    SkipTrivia(usize),
    Accessibility(usize),
}

/// What a lexing helper answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanAnswer {
    At(usize),
    Is(bool),
    /// Where an accessibility statement ends, and each token as its place in Pele's numbering,
    /// its start and its end.
    Statement {
        end: usize,
        tokens: Vec<[usize; 3]>,
    },
}

/// `lineEnd`, `keywordAt`, `wordAt`, `skipTrivia` or `accessibility` on `src`.
pub fn scan(src: &[u16], call: &ScanCall) -> ScanAnswer {
    match call {
        ScanCall::LineEnd(at) => ScanAnswer::At(scan::line_end(src, *at)),
        ScanCall::KeywordAt(at, word) => ScanAnswer::Is(scan::keyword_at(src, *at, word)),
        ScanCall::WordAt(at, word) => ScanAnswer::Is(scan::word_at(src, *at, word)),
        ScanCall::SkipTrivia(at) => ScanAnswer::At(scan::skip_trivia(src, *at)),
        ScanCall::Accessibility(at) => {
            let mut tokens = Vec::new();
            let end = scan::accessibility(src, *at, |part, start, end| tokens.push([part.offset(), start, end]));
            ScanAnswer::Statement { end, tokens }
        }
    }
}

/// The ends a fresh scanner of Pele's `accDescr()` answers at each position in turn.
pub fn acc_descr(src: &[u16], positions: &[usize]) -> Vec<Option<usize>> {
    let Matcher::Scanner(make) = tokens::acc_descr_named("ACC_DESCR").matcher else {
        unreachable!("accDescr is a scanner");
    };
    let mut scanner = make();
    positions.iter().map(|&at| scanner(src, at)).collect()
}

/// The name suffixes that Pele's types give `titleBefore`.
#[derive(Clone, Copy, Debug)]
pub enum TitleRest {
    /// `[-\w]*\w`
    DashedName,
    /// `\w`
    Word,
    /// `[-./\w]*[-\w]`
    RefName,
    /// `(?!%%)[^\n\r]`
    LineText,
}

fn rest(rest: TitleRest) -> fn(&[u16], usize) -> bool {
    match rest {
        TitleRest::DashedName => tokens::dashed_name,
        TitleRest::Word => tokens::word_unit,
        TitleRest::RefName => tokens::ref_name,
        TitleRest::LineText => tokens::line_text,
    }
}

fn lex_id(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, |c| c == u16::from(b'-') || input::is_word(c)))
}

const LEX_YAML: usize = 6;
const LEX_ID: usize = 12;

/// The token table of the oracle's `lex` op, in the forms Pele's Langium types build theirs.
fn lex_table(title: TitleRest) -> [TokenType; 14] {
    [
        tokens::keyword("radar-beta").longer(&[LEX_ID]),
        tokens::token("axis", Matcher::Literal("axis")).longer(&[LEX_ID]),
        tokens::token("-", Matcher::Literal("-")).longer(&[LEX_YAML]),
        tokens::title_before(rest(title)),
        tokens::acc_descr_named("ACC_DESCR"),
        tokens::ACC_TITLE,
        tokens::YAML,
        tokens::DIRECTIVE,
        tokens::NEWLINE,
        tokens::WHITESPACE,
        tokens::SINGLE_LINE_COMMENT,
        tokens::STRING,
        tokens::token("ID", Matcher::Pattern(lex_id)),
        tokens::token(":", Matcher::Literal(":")),
    ]
}

/// Where Pele's `titleBefore(rest)` matching at `at` ends.
pub fn title_before(title: TitleRest, src: &[u16], at: usize) -> Option<usize> {
    tokens::title_ending(src, at, rest(title))
}

/// The visible tokens of `src` in the `lex` table, each as its kind, start and end, with the end
/// of input as kind `None`; or where no token type matches.
pub fn lex(title: TitleRest, src: &[u16]) -> Result<Vec<(Option<usize>, usize, usize)>, usize> {
    let tokens = tokens::tokenize(src, &lex_table(title)).map_err(|error| error.at)?;
    Ok((0..tokens.kinds.len()).map(|i| (tokens.kinds[i], tokens.starts[i], tokens.ends[i])).collect())
}

/// Pele's `capWidth` for a host that gives `max_width`, with `around` left out where it is.
pub fn cap_width(max_width: Option<f64>, natural: f64, least: f64, around: Option<f64>) -> f64 {
    let options = LayoutOptions { max_width, ..LayoutOptions::default() };
    fit::cap_width(&options, natural, least, around.unwrap_or(0.0))
}

/// Draws a Scene with Pele's parity metrics and default options.
fn drawn(palette: &SourcePalette, draw: impl FnOnce(&mut SceneBuilder)) -> Scene {
    let options = LayoutOptions::default();
    let config = JsObject::default();
    let cx = DrawContext {
        kind: DiagramKind::Pie,
        measure: &ArialMetrics,
        options: &options,
        config: &config,
        palette,
        responsive: true,
        limits: Limits::default(),
    };
    let mut out =
        SceneBuilder::new(&cx, Size { width: 100.0, height: 100.0 }, Accessible { title: None, description: None });
    draw(&mut out);
    out.finish().expect("a drawing finishes")
}

/// Where a type's theme variables set its series colours, as the crate's `PaletteRule` says.
#[derive(Clone, Copy, Debug)]
pub enum Palette {
    None,
    Slots(&'static str, usize),
    Repeated(&'static str, usize),
    PlotColors,
}

/// What a source palette writes.
#[derive(Clone, Debug)]
pub struct PaletteProbe {
    /// The SVG of an empty drawing, whose root's style ends in the palette.
    pub svg: String,
    /// Pele's `seriesInkFill(i)` for each `i` from 0 to 23.
    pub fills: Vec<String>,
}

/// The palette that `config`, a JSON object, sets under `rule`.
pub fn palette(rule: Palette, config: &str) -> PaletteProbe {
    let rule = match rule {
        Palette::None => PaletteRule::None,
        Palette::Slots(prefix, first) => PaletteRule::Slots(prefix, first),
        Palette::Repeated(prefix, first) => PaletteRule::Repeated(prefix, first),
        Palette::PlotColors => PaletteRule::PlotColors,
    };
    let config = match input::parse_json(&config.encode_utf16().collect::<Vec<_>>()) {
        Ok(JsValue::Object(config)) => config,
        _ => JsObject::default(),
    };
    let scene = drawn(&SourcePalette::from_config(rule, &config), |_| {});
    let fills = (0..24)
        .map(|index| {
            let ink = svg::paint(&scene, Paint::OnSeries(SeriesSlot::new(index)));
            ink.map_or_else(String::new, |ink| format!(" fill=\"{ink}\""))
        })
        .collect();
    let svg = svg::write(&ParityScene { scene }).expect("an empty drawing is in its limit").svg;
    PaletteProbe { svg, fills }
}

/// Pele's `Style` bits for an emphasis: bold 1, italic 2, mono 4, sub 8, sup 16, underline 32.
fn style_bits(style: Emphasis) -> u8 {
    let script = match style.script {
        Script::Normal => 0,
        Script::Sub => 8,
        Script::Super => 16,
    };
    u8::from(style.bold)
        | u8::from(style.italic) << 1
        | u8::from(style.mono) << 2
        | script
        | u8::from(style.underline) << 5
}

/// An emphasis from Pele's `Style` bits. Sub wins over sup, as it does where Pele writes them.
fn emphasis(bits: u8) -> Emphasis {
    let script = if bits & 8 != 0 {
        Script::Sub
    } else if bits & 16 != 0 {
        Script::Super
    } else {
        Script::Normal
    };
    Emphasis { bold: bits & 1 != 0, italic: bits & 2 != 0, mono: bits & 4 != 0, underline: bits & 32 != 0, script }
}

/// One run of a measured label.
#[derive(Clone, Debug, PartialEq)]
pub struct RunProbe {
    pub text: Vec<u16>,
    /// Pele's `Style` bits.
    pub style: u8,
    pub width: f64,
}

/// A measured label, as Pele's `Label` holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelProbe {
    pub lines: Vec<Vec<RunProbe>>,
    pub widths: Vec<f64>,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub line_height: f64,
}

impl From<&Label> for LabelProbe {
    fn from(label: &Label) -> Self {
        let run = |run: &LabelRun| RunProbe { text: run.text.to_vec(), style: style_bits(run.style), width: run.width };
        LabelProbe {
            lines: label.lines.iter().map(|line| line.runs.iter().map(run).collect()).collect(),
            widths: label.lines.iter().map(|line| line.width).collect(),
            width: label.width,
            height: label.height,
            size: label.size,
            line_height: label.line_height,
        }
    }
}

impl From<&LabelProbe> for Label {
    fn from(probe: &LabelProbe) -> Self {
        let run = |run: &RunProbe| LabelRun {
            text: run.text.as_slice().into(),
            style: emphasis(run.style),
            width: run.width,
        };
        let lines = probe
            .lines
            .iter()
            .zip(&probe.widths)
            .map(|(runs, &width)| LabelLine { runs: runs.iter().map(run).collect(), width })
            .collect();
        Label { lines, width: probe.width, height: probe.height, size: probe.size, line_height: probe.line_height }
    }
}

/// Pele's `layoutLabel` with its parity metrics. `raw` is empty where Pele's is `undefined`.
pub fn layout_label(raw: &[u16], markdown: bool, size: f64, max: f64, base: u8) -> LabelProbe {
    (&measure::layout_label(raw, markdown, &ArialMetrics, size, max, emphasis(base))).into()
}

/// Pele's `plainLabel` with its parity metrics. `raw` is `None` where Pele's is `undefined`.
pub fn plain_label(raw: Option<&[u16]>, size: f64, max: f64, base: u8) -> LabelProbe {
    (&measure::plain_label(raw, &ArialMetrics, size, max, emphasis(base))).into()
}

/// Pele's `fitLabel` with its parity metrics.
pub fn fit_label(raw: Option<&[u16]>, size: f64, max: f64, base: u8) -> LabelProbe {
    (&measure::fit_label(raw, &ArialMetrics, size, max, emphasis(base))).into()
}

/// Pele's `literalLabel` with its parity metrics.
pub fn literal_label(text: &[u16], size: f64, room: f64, style: u8) -> LabelProbe {
    (&measure::literal_label(text, &ArialMetrics, size, room, emphasis(style))).into()
}

/// Pele's `breakLabel` with its parity metrics.
pub fn break_label(label: &LabelProbe, room: f64) -> LabelProbe {
    (&measure::break_label(&label.into(), &ArialMetrics, room)).into()
}

/// Pele's `fitText` with its parity metrics: the text, its size and its width.
pub fn fit_text(text: &[u16], max: f64, size: f64, min: f64, style: u8) -> Option<(Vec<u16>, f64, f64)> {
    let fitted = measure::fit_text(text, max, size, min, &ArialMetrics, emphasis(style))?;
    Some((fitted.text.to_vec(), fitted.size, fitted.width))
}

const ROLES: [Role; 9] = [
    Role::Background,
    Role::Foreground,
    Role::Border,
    Role::OnLight,
    Role::OnDark,
    Role::Line,
    Role::Surface,
    Role::SurfaceAlt,
    Role::Muted,
];

/// The look a type gives a label for the attributes it hands Pele's `leftLabelSvg`, or the
/// attribute no type hands it.
fn label_look(out: &mut SceneBuilder, attributes: &[(String, String)]) -> Result<LabelLook, String> {
    let mut look = LabelLook::default();
    for (index, (name, value)) in attributes.iter().enumerate() {
        if attributes[..index].iter().any(|(given, _)| given == name) {
            return Err(format!("{name} twice"));
        }
        look = match (name.as_str(), value.as_str()) {
            ("class", class) if index == 0 => LabelLook::class(class),
            ("font-weight", "var(--_hw)") => look.weight(TextWeight::Heading),
            ("font-weight", "var(--_tw)") => look.weight(TextWeight::Title),
            ("font-weight", "var(--_w)") => look.weight(TextWeight::Bold),
            ("font-size", size) => look.font_size(Number::raw(size.parse().map_err(|_| format!("font-size {size}"))?)),
            ("font-style", "italic") => look.italic(),
            ("font-family", "var(--_fm)") => look.mono(),
            ("fill", fill) => match ROLES.iter().find(|&&role| svg::role(role) == fill) {
                Some(&role) => look.fill(Some(Paint::Role(role))),
                None => look.fill(Some(out.colour(&fill.encode_utf16().collect::<Vec<_>>(), Role::Foreground))),
            },
            ("style", declarations) => {
                let mut style = if declarations.ends_with(';') { Style::terminated() } else { Style::unterminated() };
                for declaration in declarations.split(';').filter(|declaration| !declaration.is_empty()) {
                    let (property, value) =
                        declaration.split_once(':').ok_or_else(|| format!("style {declarations}"))?;
                    style.declare_written(property, value.to_owned());
                }
                look.style(Some(&style))
            }
            _ => return Err(format!("{name}=\"{value}\"")),
        };
    }
    Ok(look)
}

/// Pele's `leftLabelSvg` as this port's Scene writes it, or the attribute this port's caller
/// would never pass.
pub fn left_label_svg(label: &LabelProbe, x: f64, top: f64, attributes: &[(String, String)]) -> Result<String, String> {
    let mut look = Err(String::new());
    let scene = drawn(&SourcePalette::default(), |out| {
        look = label_look(out, attributes).map(|look| {
            let part = out.part(Entity::diagram());
            out.left_label(part, Facet::Label, &label.into(), x, top, look);
        });
    });
    look.map(|()| svg::write_fragment(&scene))
}

/// Pele's `iconSvg` for a host with no icons, as this port's Scene writes it.
pub fn icon_svg(name: &[u16], x: f64, y: f64, side: f64) -> String {
    let scene = drawn(&SourcePalette::default(), |out| {
        let part = out.part(Entity::diagram());
        out.icon(part, &name.into(), [Number::num(x), Number::num(y)], Number::num(side));
    });
    svg::write_fragment(&scene)
}

/// The runs the Scene's paint walk gives for a label `leftLabelSvg` draws, in paint order, as an
/// interpreter without SVG reads them.
pub fn left_label_runs(label: &LabelProbe, x: f64, top: f64) -> Vec<BaselineRun> {
    let scene = drawn(&SourcePalette::default(), |out| {
        let part = out.part(Entity::diagram());
        out.left_label(part, Facet::Label, &label.into(), x, top, LabelLook::class("pele-label"));
    });
    scene
        .paint()
        .filter_map(|visit| match visit {
            PaintVisit::Draw(Draw { geometry: Geometry::Text(run), .. }) => Some(run),
            _ => None,
        })
        .collect()
}

/// One token of the outline grammar: its place in Pele's numbering, its text and its start.
pub fn outline_tokenize(src: &[u16], kanban: bool) -> Vec<(u8, Vec<u16>, usize)> {
    outline::tokenize(src, kanban)
        .into_iter()
        .map(|token| (token.kind as u8, token.text.to_vec(), token.start))
        .collect()
}

/// A call the outline grammar's actions make on a db.
#[derive(Clone, Debug, PartialEq)]
pub enum OutlineCall {
    AddNode { level: usize, id: Vec<u16>, descr: Vec<u16>, shape: u8, shape_data: Option<Vec<u16>> },
    Icon(Vec<u16>),
    Class(Vec<u16>),
}

struct Recording(Vec<OutlineCall>);

impl outline::OutlineDb for Recording {
    type Error = std::convert::Infallible;

    fn add_node(&mut self, node: outline::OutlineNode<'_>) -> Result<(), Self::Error> {
        self.0.push(OutlineCall::AddNode {
            level: node.level,
            id: node.id.to_vec(),
            descr: node.descr.to_vec(),
            shape: node.shape as u8,
            shape_data: node.shape_data.map(<[u16]>::to_vec),
        });
        Ok(())
    }

    fn decorate_node(&mut self, decoration: outline::Decoration<'_>) {
        self.0.push(match decoration {
            outline::Decoration::Icon(icon) => OutlineCall::Icon(icon.to_vec()),
            outline::Decoration::Class(class) => OutlineCall::Class(class.to_vec()),
        });
    }
}

/// Where the outline grammar stopped: the offset, Pele's detail, and whether no token matched.
pub type OutlineStop = (usize, String, bool);

/// Pele's `parseOutline` on a db that records each call, with where it stopped, if it did.
pub fn outline_parse(src: &[u16], kanban: bool) -> (Vec<OutlineCall>, Option<OutlineStop>) {
    let mut db = Recording(Vec::new());
    let stop = match outline::parse_outline(src, &mut db, kanban) {
        Ok(()) => None,
        Err(outline::OutlineError::Syntax(error)) => Some((error.at, error.detail, error.lexical)),
        Err(outline::OutlineError::Db(never)) => match never {},
    };
    (db.0, stop)
}
