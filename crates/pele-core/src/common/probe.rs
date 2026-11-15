//! Entry points that let `tests/common.rs` hold the shared helpers to Pele. Each takes plain
//! values, calls the crate's own helper, and answers in plain values, so the helpers' types stay
//! private to the crate.

use crate::catalog::DrawContext;
use crate::diagrams::scan;
use crate::diagrams::tokens::{self, Matcher, TokenType};
use crate::input::{self, JsObject, JsValue, Units};
use crate::measure::ArialMetrics;
use crate::scene::{Accessible, Paint, PaletteRule, Scene, SceneBuilder, SeriesSlot, Size, SourcePalette};
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
