//! Differential tests for the helpers several diagram types share: every answer must equal
//! Pele 1.13.0's in the pinned Node.
//!
//! Each helper meets two sets of inputs. Seeded random ones reach its branches. Harvested ones are
//! every input the importing types gave the helper while Pele rendered the sources of
//! `tests/fixtures/common/harvest.json`, which `scripts/harvest-common.mjs` writes and the oracle's
//! `common_record` op watches. Both go to the oracle's `common` op (`harness/common.ts`).

use pele_core::common::probe::{self, ScanAnswer, ScanCall};
use pele_core::common::{is_unicode_letter, parse_generic_types};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    /// Up to `most` pieces strung together, each a string or one lone UTF-16 code unit.
    fn text(&mut self, pieces: &[&str], most: usize) -> Vec<u16> {
        let mut out = Vec::new();
        for _ in 0..self.below(most + 1) {
            match self.below(40) {
                0 => out.push(0xd83d),
                1 => out.push(0xde00),
                _ => out.extend(self.pick(pieces).encode_utf16()),
            }
        }
        out
    }
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// A text for the `common` op, which reads `{"$units": [...]}` as the string of those code units.
fn text_value(text: &[u16]) -> Value {
    json!({ "$units": text })
}

fn units_of(value: &Value) -> Vec<u16> {
    match value {
        Value::String(text) => units(text),
        Value::Array(items) => items.iter().map(|unit| unit.as_u64().expect("a code unit") as u16).collect(),
        Value::Object(units) if units.contains_key("$units") => units_of(&units["$units"]),
        other => panic!("not a text: {other}"),
    }
}

fn line(op: &str, value: &Value) -> String {
    // Node ends a request line at U+2028 and U+2029 as well, which JSON leaves unescaped.
    let json = value.to_string().replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("{op}\t{json}")
}

/// Asks the oracle's `common` op once per input.
fn ask(inputs: &[Value]) -> Vec<Value> {
    let requests: Vec<String> = inputs.iter().map(|input| line("common", input)).collect();
    pele_oracle::ask(&requests)
        .iter()
        .map(|answer| serde_json::from_str(answer).expect("the oracle answers JSON"))
        .collect()
}

/// Every distinct input the importing types gave the helpers while Pele rendered the harvest, by
/// the `fn` of the `common` op that takes it.
fn harvested(helper: &str) -> &'static [Value] {
    static INPUTS: OnceLock<BTreeMap<String, Vec<Value>>> = OnceLock::new();
    let inputs = INPUTS.get_or_init(|| {
        let path = pele_oracle::root().join("tests/fixtures/common/harvest.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let harvest: Value = serde_json::from_str(&text).expect("the harvest is JSON");
        let sources = harvest["sources"].as_array().expect("a list of sources");
        // Each source as Pele draws it by default, and in a host 320 and 640 pixels wide.
        let requests: Vec<String> = [json!({}), json!({ "maxWidth": 320 }), json!({ "maxWidth": 640 })]
            .iter()
            .flat_map(|options| {
                sources.iter().map(move |source| format!("{}\t{options}", line("common_record", source)))
            })
            .collect();
        let mut seen = HashSet::new();
        let mut by_helper: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for answer in pele_oracle::ask(&requests) {
            let record: Value = serde_json::from_str(&answer).expect("the oracle answers JSON");
            for input in record["inputs"].as_array().expect("a list of inputs") {
                if seen.insert(input.to_string()) {
                    by_helper
                        .entry(input["fn"].as_str().expect("a helper").to_owned())
                        .or_default()
                        .push(input.clone());
                }
            }
        }
        by_helper
    });
    inputs.get(helper).map_or(&[], Vec::as_slice)
}

/// Compares each input's answer, and fails with the differences.
fn compare(what: &str, inputs: &[Value], ours: impl Fn(&Value) -> Value) {
    let theirs = ask(inputs);
    let mut differences = Vec::new();
    for (input, theirs) in inputs.iter().zip(&theirs) {
        let ours = ours(input);
        if ours != *theirs {
            differences.push(format!("{input}\n  ours {ours}\n  Pele {theirs}"));
        }
    }
    println!("{what}: {} inputs, {} different", inputs.len(), differences.len());
    let shown: Vec<&String> = differences.iter().take(20).collect();
    assert!(differences.is_empty(), "{what}: {} of {} differ:\n{shown:#?}", differences.len(), inputs.len());
}

/// Short strings over the characters `parseGenericTypes` reacts to, plus a few it must carry
/// through untouched, including a character outside the BMP.
fn generic_inputs() -> Vec<String> {
    let pieces = ["~", "~", "~", ",", ",", " ", "a", "List", "int", "<", ">", "é", "😀", "~~", ",~"];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut inputs: Vec<String> = ["", "~", ",", "~,~", "List~int~", "Map~K, V~", "~a~b~", "a~b,c~d", "~,~,~", "x~~"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for _ in 0..20_000 {
        let len = rng.below(12);
        inputs.push((0..len).map(|_| pieces[rng.below(pieces.len())]).collect());
    }
    inputs
}

#[test]
fn parse_generic_types_matches_pele() {
    let inputs = generic_inputs();
    let requests: Vec<String> =
        inputs.iter().map(|s| format!("generics\t{}", serde_json::to_string(s).expect("a string encodes"))).collect();
    let answers = pele_oracle::ask(&requests);
    let mut differences = Vec::new();
    for (input, answer) in inputs.iter().zip(&answers) {
        let theirs: String = serde_json::from_str(answer).expect("a JSON string");
        let ours = parse_generic_types(input);
        if ours != theirs {
            differences.push(format!("{input:?}: ours {ours:?}, Pele {theirs:?}"));
        }
    }
    assert!(differences.is_empty(), "{} of {} differ:\n{}", differences.len(), inputs.len(), differences.join("\n"));
}

#[test]
fn is_unicode_letter_matches_pele_for_every_code_unit() {
    let answer = pele_oracle::ask(&["unicode_letters".to_owned()]).remove(0);
    let theirs: Vec<u16> = serde_json::from_str(&answer).expect("a JSON array of code units");
    let ours: Vec<u16> = (0..=u16::MAX).filter(|&code| is_unicode_letter(code)).collect();
    assert!(theirs.len() > 40_000, "Pele accepted only {} code units; the op is wrong", theirs.len());
    assert_eq!(ours, theirs);
}

#[test]
fn char_classes_match_pele_for_every_code_unit() {
    let theirs = ask(&[json!({ "fn": "chars" })]).remove(0);
    let ours: Vec<u8> = (0..=u16::MAX).map(probe::char_classes).collect();
    assert_eq!(theirs.as_array().map(Vec::len), Some(0x10000), "one class set per code unit");
    assert_eq!(json!(ours), theirs);
}

fn scan_call(call: &Value) -> ScanCall {
    let at = call[1].as_u64().expect("a position") as usize;
    let word = || call[2].as_str().expect("a word").to_owned();
    match call[0].as_str().expect("a scan function") {
        "lineEnd" => ScanCall::LineEnd(at),
        "keywordAt" => ScanCall::KeywordAt(at, word()),
        "wordAt" => ScanCall::WordAt(at, word()),
        "skipTrivia" => ScanCall::SkipTrivia(at),
        "accessibility" => ScanCall::Accessibility(at),
        other => panic!("no scan function {other}"),
    }
}

fn scan_answer(answer: &ScanAnswer) -> Value {
    match answer {
        ScanAnswer::At(at) => json!(at),
        ScanAnswer::Is(is) => json!(is),
        ScanAnswer::Statement { end, tokens } => json!({ "end": end, "tokens": tokens }),
    }
}

/// Each call of a `scan` input, run on its text.
fn scan_ours(input: &Value) -> Value {
    let src = units_of(&input["src"]);
    let calls = input["calls"].as_array().expect("a list of calls");
    Value::Array(calls.iter().map(|call| scan_answer(&probe::scan(&src, &scan_call(call)))).collect())
}

#[test]
fn scan_helpers_match_pele() {
    let pieces = [
        "accDescr", "accdescr", "ACCTITLE", "accTitle", "acc", ":", " : ", "{", "}", "{ ", "%", "%%", "%{", "#", " ",
        "\t", "\n", "\r", "\r\n", "\u{a0}", "\u{2028}", "\u{feff}", "x", "-", "journey", "Title", "section",
        "timeline", "LR", "td", "é", "😀", "M",
    ];
    let words =
        ["accdescr", "acctitle", "journey", "title", "section", "timeline", "lr", "td", "x-axis", "a b", "classdef"];
    let mut rng = Rng(0x5ca1_ab1e_0dd5_eed5);
    let mut inputs = Vec::new();
    let mut count = 0;
    for _ in 0..1500 {
        let src = rng.text(&pieces, 14);
        let mut calls = Vec::new();
        for at in 0..=src.len() + 1 {
            calls.push(json!(["lineEnd", at, null]));
            calls.push(json!(["skipTrivia", at, null]));
            calls.push(json!(["accessibility", at, null]));
            calls.push(json!(["keywordAt", at, rng.pick(&words)]));
            calls.push(json!(["wordAt", at, rng.pick(&words)]));
        }
        count += calls.len();
        inputs.push(json!({ "fn": "scan", "src": text_value(&src), "calls": calls }));
    }
    compare(&format!("random scan calls ({count} calls)"), &inputs, scan_ours);
    let real = harvested("scan");
    let calls: usize = real.iter().map(|input| input["calls"].as_array().map_or(0, Vec::len)).sum();
    assert!(calls > 1000, "the journey and timeline lexers make only {calls} scan calls on the harvest");
    compare(&format!("harvested scan calls ({calls} calls)"), real, scan_ours);
}

fn is_space(c: u16) -> bool {
    probe::char_classes(c) & 1 != 0
}

/// Pele's `syntaxError` message, line and column for an offset into a text that is parsed as it
/// is, as a helper called outside a render sees it.
fn syntax_error(src: &[u16], at: usize, detail: &str, lexical: bool) -> Value {
    let end = src.iter().rposition(|&c| !is_space(c)).map_or(0, |last| last + 1);
    // An error at or past the last content stays on the last content line.
    let offset = if end > 0 && at >= end { end } else { at };
    let line_start = src[..offset.min(src.len())].iter().rposition(|&c| c == 10).map_or(0, |at| at + 1);
    let line = 1 + src[..line_start].iter().filter(|&&c| c == 10).count();
    let line_end = src[offset.min(src.len())..].iter().position(|&c| c == 10).map_or(src.len(), |at| offset + at);
    let column = offset - line_start + 1;
    let mut snippet = src[line_start..line_end].to_vec();
    snippet.extend(units(&format!("\n{}^", "-".repeat(column - 1))));
    let mut message = units(&if lexical {
        format!("Lexical error on line {line}. Unrecognized text.\n")
    } else {
        format!("Parse error on line {line}:\n")
    });
    message.extend(snippet);
    if !lexical {
        message.extend(units(&format!("\n{detail}")));
    }
    json!({ "message": message, "line": line, "column": column })
}

#[test]
fn acc_descr_scanner_matches_pele() {
    let pieces =
        ["accDescr", "accDescr", " ", "\t", ":", "{", "}", "\n", "\r", "%%", "x", "accDescr {", "accDescr:", "\u{a0}"];
    let mut rng = Rng(0xacc_de5c);
    let mut inputs = Vec::new();
    for round in 0..3000 {
        let src = rng.text(&pieces, 10);
        let positions: Vec<usize> = if round % 2 == 0 {
            (0..=src.len()).collect()
        } else {
            (0..=src.len()).filter(|_| rng.below(3) == 0).collect()
        };
        inputs.push(json!({ "fn": "acc_descr", "src": text_value(&src), "at": positions }));
    }
    let ours = |input: &Value| {
        let src = units_of(&input["src"]);
        let at: Vec<usize> = input["at"]
            .as_array()
            .expect("positions")
            .iter()
            .map(|at| at.as_u64().expect("a position") as usize)
            .collect();
        json!(probe::acc_descr(&src, &at))
    };
    compare("random accDescr scans", &inputs, ours);
    let harvest: Vec<Value> = harvested_sources()
        .iter()
        .filter(|source| source.contains("accDescr"))
        .map(|source| json!({ "fn": "acc_descr", "src": source, "at": (0..=units(source).len()).collect::<Vec<_>>() }))
        .collect();
    assert!(harvest.len() > 20, "only {} harvested sources hold an accDescr", harvest.len());
    compare("harvested sources with accDescr, scanned at every position", &harvest, ours);
}

/// The sources of the harvest, as written.
fn harvested_sources() -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/common/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).expect("the harvest is JSON");
    harvest["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|source| source.as_str().expect("a source").to_owned())
        .collect()
}

const TITLE_RESTS: [(probe::TitleRest, &str); 4] = [
    (probe::TitleRest::DashedName, r"[-\w]*\w"),
    (probe::TitleRest::Word, r"\w"),
    (probe::TitleRest::RefName, r"[-./\w]*[-\w]"),
    (probe::TitleRest::LineText, r"(?!%%)[^\n\r]"),
];

fn title_rest(input: &Value) -> probe::TitleRest {
    let pattern = input["rest"].as_str().expect("a rest");
    TITLE_RESTS.iter().find(|(_, known)| *known == pattern).expect("a rest Pele's types use").0
}

#[test]
fn title_before_matches_pele() {
    let pieces =
        ["title", "title", "Title", " ", "\t", "-", ".", "/", "_", "a", "1", "%%", "%", "\n", "\r", "x-y", "é", "--"];
    let mut rng = Rng(0x717_1eb4);
    let mut inputs = Vec::new();
    for _ in 0..4000 {
        let src = rng.text(&pieces, 6);
        for at in 0..=src.len() {
            let pattern = rng.pick(&TITLE_RESTS).1;
            inputs.push(json!({ "fn": "title_before", "rest": pattern, "src": text_value(&src), "at": at }));
        }
    }
    compare("random titleBefore matches", &inputs, |input| {
        let at = input["at"].as_u64().expect("a position") as usize;
        json!(probe::title_before(title_rest(input), &units_of(&input["src"]), at))
    });
}

#[test]
fn longer_alternatives_lex_as_pele_does() {
    let pieces = [
        "radar-beta",
        "radar-beta",
        "axis",
        "axisX",
        "axis-",
        "-",
        "---\n",
        "--- \n",
        "---",
        "title",
        "title x",
        "titleX",
        "title-y",
        "title.z",
        "accDescr: d",
        "accDescr {",
        "accDescr { a }",
        "}",
        "accTitle: t",
        "%%{init}%%",
        "%% c",
        "\n",
        "\r\n",
        " ",
        "\t",
        "\"s\"",
        "'q'",
        "id-1",
        "_",
        ":",
        "é",
        "%",
    ];
    let mut rng = Rng(0x1_0e6e5);
    let mut inputs = Vec::new();
    for _ in 0..6000 {
        let src = rng.text(&pieces, 12);
        inputs.push(json!({ "fn": "lex", "rest": rng.pick(&TITLE_RESTS).1, "src": text_value(&src) }));
    }
    let answers: Vec<Value> = inputs
        .iter()
        .map(|input| {
            let src = units_of(&input["src"]);
            match probe::lex(title_rest(input), &src) {
                Ok(tokens) => json!({
                    "kinds": tokens.iter().map(|token| token.0.map_or(-1, |kind| kind as i64)).collect::<Vec<_>>(),
                    "starts": tokens.iter().map(|token| token.1).collect::<Vec<_>>(),
                    "ends": tokens.iter().map(|token| token.2).collect::<Vec<_>>(),
                }),
                Err(at) => json!({ "error": syntax_error(&src, at, "", true) }),
            }
        })
        .collect();
    // A name that starts with a keyword is the longer alternative winning.
    let mut reach = [0usize; 3];
    for (input, answer) in inputs.iter().zip(&answers) {
        let src = units_of(&input["src"]);
        let Some(kinds) = answer["kinds"].as_array() else { continue };
        for (i, kind) in kinds.iter().enumerate() {
            let start = answer["starts"][i].as_u64().expect("a start") as usize;
            let starts_with = |word: &str| src[start..].starts_with(&units(word));
            reach[0] += usize::from(kind == 12 && (starts_with("axis") || starts_with("radar-beta")));
            reach[1] += usize::from(kind == 1 || kind == 0);
            reach[2] += usize::from(kind == 12 && starts_with("title"));
        }
    }
    println!("names over a keyword {}, keywords {}, names over a title {}", reach[0], reach[1], reach[2]);
    assert!(reach.iter().all(|&count| count > 100), "some path is never reached: {reach:?}");
    let ours: BTreeMap<String, &Value> = inputs.iter().map(|input| input.to_string()).zip(&answers).collect();
    compare("random sources in a table with longer alternatives", &inputs, |input| ours[&input.to_string()].clone());
}

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn double(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().expect("a double as hex"), 16).expect("16 hex digits"))
}

fn optional(value: &Value) -> Option<f64> {
    (!value.is_null()).then(|| double(value))
}

/// A double as the oracle answers it, with every NaN spelled as JavaScript's one NaN.
fn number(value: f64) -> String {
    if value.is_nan() { bits(f64::NAN) } else { bits(value) }
}

#[test]
fn cap_width_matches_pele() {
    let mut rng = Rng(0xca9_3d7);
    let width = |rng: &mut Rng| match rng.below(8) {
        0 => f64::NAN,
        1 => 0.0,
        2 => -(rng.below(500) as f64),
        3 => f64::INFINITY,
        _ => rng.below(200_000) as f64 / 100.0,
    };
    let mut inputs = Vec::new();
    for _ in 0..20_000 {
        let max_width = if rng.below(4) == 0 { Value::Null } else { json!(bits(width(&mut rng))) };
        let around = if rng.below(3) == 0 { Value::Null } else { json!(bits(width(&mut rng))) };
        let (natural, least) = (bits(width(&mut rng)), bits(width(&mut rng)));
        inputs.push(
            json!({ "fn": "cap_width", "maxWidth": max_width, "natural": natural, "least": least, "around": around }),
        );
    }
    let ours = |input: &Value| {
        let natural = double(&input["natural"]);
        let width = probe::cap_width(
            optional(&input["maxWidth"]),
            natural,
            double(&input["least"]),
            optional(&input["around"]),
        );
        json!(number(width))
    };
    compare("random capWidth calls", &inputs, ours);
    compare("harvested capWidth calls", harvested("cap_width"), ours);
}

/// Pele's `PALETTES`, and xychart's list, as this port's rules. Kept beside the type names so
/// that the oracle, which reads Pele's own table, checks each one.
const PALETTES: [(&str, probe::Palette); 10] = [
    ("pie", probe::Palette::Slots("pie", 1)),
    ("gitGraph", probe::Palette::Slots("git", 0)),
    ("timeline", probe::Palette::Slots("cScale", 0)),
    ("radar", probe::Palette::Slots("cScale", 0)),
    ("treemap", probe::Palette::Slots("cScale", 0)),
    ("mindmap", probe::Palette::Slots("cScale", 1)),
    ("venn", probe::Palette::Repeated("venn", 1)),
    ("xychart", probe::Palette::PlotColors),
    ("journey", probe::Palette::None),
    ("flowchart", probe::Palette::None),
];

/// What our palette writes, in the form of the oracle's answer: the style the root appends, and
/// the fill of text on each series.
fn palette_ours(input: &Value) -> Value {
    let kind = input["type"].as_str().expect("a type");
    let rule = PALETTES.iter().find(|(name, _)| *name == kind).map_or(probe::Palette::None, |(_, rule)| *rule);
    let found = probe::palette(rule, &input["config"].to_string());
    let style = found.svg.split("font-family:var(--pele-font,sans-serif)").nth(1).expect("a root style");
    let style = &style[..style.find('"').expect("the style ends")];
    json!({ "style": units(style), "fills": found.fills })
}

fn palette_theirs(answer: &Value) -> Value {
    json!({ "style": answer["style"], "fills": answer["fills"] })
}

#[test]
fn palette_matches_pele() {
    let colours = [
        json!("#4c78a8"),
        json!("#ABCDEF"),
        json!("#12"),
        json!("  red  "),
        json!("grey"),
        json!("Gray"),
        json!("nope"),
        json!("rgb(1, 2, 3)"),
        json!("rgb(1,\u{b}2,3)"),
        json!("rgba(0,0,0,0.2)"),
        json!("hsl(120, 100%, 25%)"),
        json!("var(--pele-accent)"),
        json!("var(--other)"),
        json!("url(x)"),
        json!(""),
        json!(null),
        json!(5),
        json!(true),
        json!("red;x"),
        json!("rgb(1,2,3"),
        json!(")("),
        json!("color-mix(in srgb, red 20%, blue)"),
        json!("light-dark(#fff, #000)"),
        json!("transparent"),
        json!("a".repeat(201)),
        json!(["red"]),
        json!("#11223380"),
        json!("rgb(0 128 255 / 50%)"),
    ];
    let mut rng = Rng(0x9a1e_77e5);
    let mut inputs = Vec::new();
    for _ in 0..4000 {
        let (kind, rule) = PALETTES[rng.below(PALETTES.len())];
        let prefix = match rule {
            probe::Palette::Slots(prefix, _) | probe::Palette::Repeated(prefix, _) => prefix,
            _ => *rng.pick(&["pie", "cScale", "venn", "git"]),
        };
        let mut variables = serde_json::Map::new();
        for _ in 0..rng.below(10) {
            variables.insert(format!("{prefix}{}", rng.below(11)), rng.pick(&colours).clone());
        }
        if rng.below(3) == 0 {
            let list: Vec<String> =
                (0..rng.below(6)).map(|_| rng.pick(&colours).as_str().unwrap_or("#000").to_owned()).collect();
            let list = if rng.below(6) == 0 { json!(7) } else { json!(list.join(rng.pick(&[",", ", ", " ,"]))) };
            variables.insert("xyChart".to_owned(), json!({ "plotColorPalette": list }));
        }
        let config = match rng.below(12) {
            0 => json!({ "themeVariables": "red" }),
            1 => json!({ "themeVariables": [variables] }),
            _ => json!({ "themeVariables": variables }),
        };
        inputs.push(json!({ "fn": "palette", "type": kind, "config": config }));
    }
    let theirs = ask(&inputs);
    let mut differences = Vec::new();
    let mut set = 0;
    for (input, theirs) in inputs.iter().zip(&theirs) {
        set += usize::from(!theirs["style"].as_array().is_some_and(Vec::is_empty));
        if palette_ours(input) != palette_theirs(theirs) {
            differences.push(format!("{input}\n  ours {}\n  Pele {theirs}", palette_ours(input)));
        }
    }
    println!("random palettes: {} inputs, {set} setting a colour, {} different", inputs.len(), differences.len());
    assert!(set > 1000, "only {set} palettes set a colour");
    assert!(differences.is_empty(), "{} differ:\n{:#?}", differences.len(), &differences[..differences.len().min(10)]);
    let real = harvested("palette");
    let theirs = ask(real);
    let differing =
        real.iter().zip(&theirs).filter(|(input, theirs)| palette_ours(input) != palette_theirs(theirs)).count();
    println!("harvested palettes: {} inputs, {differing} different", real.len());
    assert_eq!(differing, 0);
}
