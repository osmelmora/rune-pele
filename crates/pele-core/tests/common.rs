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
    if inputs.is_empty() {
        return Vec::new();
    }
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
        // Each source as Pele draws it by default, in a host 320 and 640 pixels wide, and in a host
        // with icons, the one kind where Pele draws an icon's box.
        let options = [json!({}), json!({ "maxWidth": 320 }), json!({ "maxWidth": 640 }), json!({ "icons": true })];
        let requests: Vec<String> = options
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
    let ours: BTreeMap<String, &Value> = inputs.iter().map(|input| input.to_string()).zip(&answers).collect();
    compare("random sources in a table with longer alternatives", &inputs, |input| ours[&input.to_string()].clone());
    assert!(reach.iter().all(|&count| count > 100), "some path is never reached: {reach:?}");
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
#[cfg_attr(
    target_arch = "x86_64",
    ignore = "x64 Node sets the sign bit of a NaN it computes, where the port does not; no drawing shows a NaN's bits"
)]
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
    // The ink of each of the eight series is the colour its fill names.
    let inks: Vec<Value> = found.fills[..8]
        .iter()
        .map(|fill| {
            fill.strip_prefix(" fill=\"").and_then(|ink| ink.strip_suffix('"')).map_or(Value::Null, |ink| json!(ink))
        })
        .collect();
    json!({ "style": units(style), "inks": inks, "fills": found.fills })
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
    let set = inputs.iter().filter(|input| !palette_ours(input)["style"].as_array().is_some_and(Vec::is_empty)).count();
    println!("{set} of the random palettes set a colour");
    compare("random palettes", &inputs, palette_ours);
    assert!(set > 1000, "only {set} palettes set a colour");
    compare("harvested palettes", harvested("palette"), palette_ours);
}

fn label_json(label: &probe::LabelProbe) -> Value {
    let run = |run: &probe::RunProbe| json!({ "text": run.text, "style": run.style, "width": number(run.width) });
    json!({
        "lines": label.lines.iter().map(|line| line.iter().map(run).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "widths": label.widths.iter().map(|&width| number(width)).collect::<Vec<_>>(),
        "width": number(label.width),
        "height": number(label.height),
        "size": number(label.size),
        "lineHeight": number(label.line_height),
    })
}

fn label_of(value: &Value) -> probe::LabelProbe {
    let run = |run: &Value| probe::RunProbe {
        text: units_of(&run["text"]),
        style: run["style"].as_u64().expect("style bits") as u8,
        width: double(&run["width"]),
    };
    let list = |value: &Value| value.as_array().expect("a list").clone();
    probe::LabelProbe {
        lines: list(&value["lines"]).iter().map(|line| list(line).iter().map(run).collect()).collect(),
        widths: list(&value["widths"]).iter().map(double).collect(),
        width: double(&value["width"]),
        height: double(&value["height"]),
        size: double(&value["size"]),
        line_height: double(&value["lineHeight"]),
    }
}

/// Pele's `Style` bits with at most one of sub and sup, the combinations a label can hold.
fn style(rng: &mut Rng) -> u8 {
    (rng.below(8) as u8) | [0, 0, 8, 16][rng.below(4)] | [0, 0, 0, 32][rng.below(4)]
}

const LABEL_PIECES: [&str; 34] = [
    "a",
    "word",
    " ",
    " ",
    "  ",
    "<br>",
    "<BR/>",
    "<br />",
    "<b>",
    "</b>",
    "<i>x</i>",
    "<",
    ">",
    "&amp;",
    "&#35;",
    "&nope;",
    "&",
    ":fa-user",
    "fa:fa-user",
    "\u{fb02}\u{b0}\u{b0}35\u{b6}\u{df}",
    "\u{fb02}\u{b0}amp\u{b6}\u{df}",
    "😀",
    "é",
    "\n",
    "\\n",
    "Supercalifragilistic",
    "long-word-without-spaces",
    "\t",
    "\u{3000}",
    "日本語",
    "*",
    "_",
    "x\u{301}",
    "\u{a0}",
];

const SIZES: [f64; 5] = [12.0, 13.5, 14.0, 16.0, 11.2];
const ROOMS: [f64; 11] = [0.0, 1.0, 8.0, 20.0, 40.0, 60.0, 100.0, 200.0, 4000.0, f64::INFINITY, -5.0];

fn optional_text(value: &Value) -> Option<Vec<u16>> {
    (!value.is_null()).then(|| units_of(value))
}

#[test]
fn plain_and_fit_labels_match_pele() {
    let mut rng = Rng(0xf17_1abe1);
    let mut inputs = Vec::new();
    for _ in 0..12_000 {
        let raw = if rng.below(20) == 0 { Value::Null } else { text_value(&rng.text(&LABEL_PIECES, 14)) };
        let helper = if rng.below(2) == 0 { "plain_label" } else { "fit_label" };
        let (size, max) = (*rng.pick(&SIZES), *rng.pick(&ROOMS));
        inputs.push(json!({ "fn": helper, "raw": raw, "size": bits(size), "max": bits(max), "base": rng.below(4) }));
    }
    let ours = |input: &Value| {
        let raw = optional_text(&input["raw"]);
        let (size, max, base) =
            (double(&input["size"]), double(&input["max"]), input["base"].as_u64().expect("base") as u8);
        label_json(&match input["fn"].as_str() {
            Some("plain_label") => probe::plain_label(raw.as_deref(), size, max, base),
            _ => probe::fit_label(raw.as_deref(), size, max, base),
        })
    };
    let cut = inputs
        .iter()
        .filter(|input| input["fn"] == "fit_label")
        .filter(|input| {
            let plain = json!({ "fn": "plain_label", "raw": input["raw"], "size": input["size"], "max": input["max"], "base": input["base"] });
            ours(&plain)["lines"].as_array().map(Vec::len) != ours(input)["lines"].as_array().map(Vec::len)
        })
        .count();
    println!("fitLabel cut words in {cut} labels");
    compare("random plainLabel and fitLabel calls", &inputs, ours);
    assert!(cut > 500, "fitLabel cut words in only {cut} labels");
    compare("harvested plainLabel calls", harvested("plain_label"), ours);
    compare("harvested fitLabel calls", harvested("fit_label"), ours);
}

#[test]
fn literal_labels_match_pele() {
    let pieces = [
        "a",
        "file.txt",
        " ",
        "  ",
        "\t",
        "\n",
        "\n\n",
        "/usr/local/bin/",
        "Supercalifragilisticexpialidocious",
        "😀",
        "é",
        "日本",
        "\u{a0}",
        "\u{3000}",
        "x ",
        " y",
        "<b>",
        "&amp;",
    ];
    let mut rng = Rng(0x11_7e2a1);
    let mut inputs = Vec::new();
    for _ in 0..12_000 {
        let text = rng.text(&pieces, 12);
        let (size, room) = (*rng.pick(&SIZES), *rng.pick(&ROOMS));
        inputs.push(json!({ "fn": "literal_label", "text": text_value(&text), "size": bits(size), "room": bits(room), "style": style(&mut rng) }));
    }
    let ours = |input: &Value| {
        let style = input["style"].as_u64().expect("style") as u8;
        label_json(&probe::literal_label(
            &units_of(&input["text"]),
            double(&input["size"]),
            double(&input["room"]),
            style,
        ))
    };
    let wrapped = inputs.iter().filter(|input| {
        let lines = ours(input)["lines"].as_array().map_or(0, Vec::len);
        lines > units_of(&input["text"]).iter().filter(|&&c| c == 10).count() + 1
    });
    let wrapped = wrapped.count();
    println!("literalLabel wrapped {wrapped} labels");
    compare("random literalLabel calls", &inputs, ours);
    assert!(wrapped > 2000, "literalLabel wrapped only {wrapped} labels");
    compare("harvested literalLabel calls", harvested("literal_label"), ours);
}

#[test]
fn break_labels_match_pele() {
    let mut rng = Rng(0xb2ea_c1ab);
    let mut inputs = Vec::new();
    for _ in 0..12_000 {
        let raw = rng.text(&LABEL_PIECES, 14);
        let size = *rng.pick(&SIZES);
        let label = if rng.below(4) == 0 {
            probe::literal_label(&raw, size, *rng.pick(&ROOMS), style(&mut rng))
        } else {
            probe::layout_label(&raw, rng.below(3) == 0, size, *rng.pick(&ROOMS), rng.below(4) as u8)
        };
        inputs.push(json!({ "fn": "break_label", "label": label_json(&label), "room": bits(*rng.pick(&ROOMS)) }));
    }
    let ours = |input: &Value| label_json(&probe::break_label(&label_of(&input["label"]), double(&input["room"])));
    let broken = inputs
        .iter()
        .filter(|input| {
            ours(input)["lines"].as_array().map(Vec::len) > input["label"]["lines"].as_array().map(Vec::len)
        })
        .count();
    println!("breakLabel broke {broken} labels");
    compare("random breakLabel calls", &inputs, ours);
    assert!(broken > 2000, "breakLabel broke only {broken} labels");
    compare("harvested breakLabel calls", harvested("break_label"), ours);
}

#[test]
fn fit_text_matches_pele() {
    let mut rng = Rng(0xf17_7e47);
    let mut inputs = Vec::new();
    for _ in 0..12_000 {
        let text = rng.text(&LABEL_PIECES, 8);
        let size = (8 + rng.below(13)) as f64;
        let min = (1 + rng.below(size as usize)) as f64;
        let max = *rng.pick(&[0.0, -1.0, 1.0, 5.0, 10.0, 30.0, 60.0, 100.0, 300.0, 12.5]);
        inputs.push(json!({ "fn": "fit_text", "text": text_value(&text), "max": bits(max), "size": bits(size), "min": bits(min), "style": style(&mut rng) }));
    }
    let ours = |input: &Value| {
        let style = input["style"].as_u64().expect("style") as u8;
        let fitted = probe::fit_text(
            &units_of(&input["text"]),
            double(&input["max"]),
            double(&input["size"]),
            double(&input["min"]),
            style,
        );
        fitted.map_or(
            Value::Null,
            |(text, size, width)| json!({ "text": text, "size": number(size), "width": number(width) }),
        )
    };
    // Fitted at its own size, shrunk, cut with an ellipsis, or not fitted at all.
    let mut reach = [0usize; 4];
    for input in &inputs {
        let answer = ours(input);
        let ellipsis = answer["text"].as_array().and_then(|text| text.last()).is_some_and(|last| last == 0x2026);
        let shrunk = !answer.is_null() && answer["size"] != input["size"];
        reach[if answer.is_null() {
            3
        } else if ellipsis {
            2
        } else if shrunk {
            1
        } else {
            0
        }] += 1;
    }
    println!("fitText kept {}, shrank {}, cut {} and gave up on {} texts", reach[0], reach[1], reach[2], reach[3]);
    compare("random fitText calls", &inputs, ours);
    assert!(reach.iter().all(|&count| count > 500), "fitText misses a path: {reach:?}");
    compare("harvested fitText calls", harvested("fit_text"), ours);
}

/// The attributes each caller of Pele's `leftLabelSvg` hands it.
fn left_label_attributes(rng: &mut Rng) -> Vec<(String, String)> {
    let pair = |name: &str, value: &str| (name.to_owned(), value.to_owned());
    let fills = ["var(--_m)", "var(--pele-fg-on-light,#1f1f1f)", "var(--pele-fg-on-dark,#fff)", "#ff0000", "red"];
    let mut attributes = match rng.below(5) {
        0 => vec![pair("class", "pele-cluster-label"), pair("font-weight", "var(--_hw)")],
        1 => vec![pair("class", "pele-c4-type"), pair("font-size", rng.pick(&["12", "11.2", "13.333333333333334"]))],
        2 => vec![pair("class", "pele-label")],
        3 => vec![pair("class", "pele-tree-description"), pair("font-style", "italic"), pair("fill", "var(--_m)")],
        _ => vec![
            pair("class", "pele-cluster-label"),
            pair("font-size", "11.2"),
            pair("font-weight", "var(--_hw)"),
            pair("fill", "var(--_m)"),
        ],
    };
    let extras = [
        pair("font-weight", "var(--_hw)"),
        pair("font-family", "var(--_fm)"),
        pair("fill", rng.pick(&fills)),
        pair("style", rng.pick(&["color:red", "fill:#f00;", "color:blue;font-weight:bold"])),
    ];
    for extra in extras {
        // An element has each attribute once.
        if rng.below(3) == 0 && attributes.iter().all(|(name, _)| *name != extra.0) {
            attributes.push(extra);
        }
    }
    attributes
}

fn attribute_pairs(value: &Value) -> Vec<(String, String)> {
    let pair =
        |item: &Value| (item[0].as_str().expect("a name").to_owned(), item[1].as_str().expect("a value").to_owned());
    value.as_array().expect("attribute pairs").iter().map(pair).collect()
}

fn left_label_ours(input: &Value) -> Value {
    let attributes = attribute_pairs(&input["attrs"]);
    let svg =
        probe::left_label_svg(&label_of(&input["label"]), double(&input["x"]), double(&input["top"]), &attributes)
            .unwrap_or_else(|attribute| panic!("no caller hands leftLabelSvg {attribute}"));
    json!(units(&svg))
}

#[test]
fn left_labels_write_as_pele_writes_them() {
    let pieces = [
        "a",
        " ",
        "  ",
        "<b>bold</b>",
        "<i>it</i>",
        "<code>m</code>",
        "<sub>s</sub>",
        "<u>u</u>",
        "\n",
        "<br>",
        "word",
        "&amp;",
        "\u{1}",
        "😀",
        "\"q\"",
    ];
    let mut rng = Rng(0x1ef7_1abe);
    let mut inputs = Vec::new();
    for _ in 0..8000 {
        let raw = rng.text(&pieces, 10);
        let size = *rng.pick(&SIZES);
        let mut label = match rng.below(3) {
            0 => probe::literal_label(&raw, size, *rng.pick(&ROOMS), style(&mut rng)),
            1 => probe::layout_label(&raw, rng.below(3) == 0, size, *rng.pick(&ROOMS), rng.below(4) as u8),
            _ => probe::break_label(
                &probe::layout_label(&raw, false, size, 4000.0, rng.below(8) as u8),
                *rng.pick(&ROOMS),
            ),
        };
        if rng.below(10) == 0 {
            label.lines.push(Vec::new());
            label.widths.push(0.0);
        }
        let (x, top) = ((rng.below(4000) as f64 - 1000.0) / 7.0, (rng.below(4000) as f64 - 1000.0) / 3.0);
        let attrs: Vec<Value> =
            left_label_attributes(&mut rng).into_iter().map(|(name, value)| json!([name, value])).collect();
        inputs.push(
            json!({ "fn": "left_label", "label": label_json(&label), "x": bits(x), "top": bits(top), "attrs": attrs }),
        );
    }
    let written = inputs.iter().filter(|input| !left_label_ours(input).as_array().is_some_and(Vec::is_empty)).count();
    println!("leftLabelSvg wrote text for {written} labels");
    compare("random leftLabelSvg calls", &inputs, left_label_ours);
    assert!(written > 6000, "leftLabelSvg wrote text for only {written} labels");
    compare("harvested leftLabelSvg calls", harvested("left_label"), left_label_ours);
}

#[test]
fn icons_write_as_pele_writes_them() {
    let names = ["fa:fa-user", "fa fa-book", "mdi:home", "&amp;", "&#35;", "<", ">", "\"", "'", "x\u{1}y", "😀", " "];
    let mut rng = Rng(0x1c0_75e7);
    let mut inputs = Vec::new();
    for _ in 0..5000 {
        let name = rng.text(&names, 3);
        let coordinate = |rng: &mut Rng| bits((rng.below(200_000) as f64 - 100_000.0) / 997.0);
        let (x, y, side) = (coordinate(&mut rng), coordinate(&mut rng), coordinate(&mut rng));
        inputs.push(json!({ "fn": "icon", "name": text_value(&name), "x": x, "y": y, "side": side }));
    }
    let ours = |input: &Value| {
        let svg = probe::icon_svg(
            &units_of(&input["name"]),
            double(&input["x"]),
            double(&input["y"]),
            double(&input["side"]),
        );
        json!(units(&svg))
    };
    compare("random iconSvg calls", &inputs, ours);
    compare("harvested iconSvg calls", harvested("icon"), ours);
}

/// The numbers of each `name="…"` in an SVG fragment, in order.
fn attribute_numbers(svg: &str, name: &str) -> Vec<f64> {
    let marker = format!(" {name}=\"");
    svg.match_indices(&marker)
        .map(|(at, _)| {
            let value = &svg[at + marker.len()..];
            value[..value.find('"').expect("the value ends")].parse().expect("a number")
        })
        .collect()
}

#[test]
fn the_walk_places_left_label_runs_where_the_svg_does() {
    let pieces = ["a", " ", "<b>bold</b>", "<i>it</i>", "<code>m</code>", "<u>u</u>", "\n", "word", "😀", "\u{1}"];
    let mut rng = Rng(0x3a1c_1ef7);
    let mut runs = 0;
    for _ in 0..2000 {
        let raw = rng.text(&pieces, 10);
        let label = probe::layout_label(&raw, false, *rng.pick(&SIZES), *rng.pick(&ROOMS), rng.below(8) as u8);
        let (x, top) = ((rng.below(4000) as f64 - 1000.0) / 7.0, (rng.below(4000) as f64 - 1000.0) / 3.0);
        let class = [("class".to_owned(), "pele-label".to_owned())];
        let svg = probe::left_label_svg(&label, x, top, &class).expect("a class is a look");
        let points: Vec<(f64, f64)> =
            attribute_numbers(&svg, "x").into_iter().zip(attribute_numbers(&svg, "y")).collect();
        // Each line the SVG writes starts at its text element's point, and each run follows the
        // runs before it on that line.
        // A line is written when a run has text XML keeps or emphasis its element does not carry.
        let texts = || label.lines.iter().flatten().filter(|run| !run.text.is_empty());
        let shared = if texts().next().is_some() { texts().fold(7, |shared, run| shared & run.style) } else { 0 };
        let kept = |run: &probe::RunProbe| {
            let keeps = |c: char| (c >= ' ' && !matches!(c, '\u{7f}' | '\u{fffe}' | '\u{ffff}')) || c == '\t';
            char::decode_utf16(run.text.iter().copied()).any(|c| c.is_ok_and(keeps))
        };
        let written: Vec<&Vec<probe::RunProbe>> = label
            .lines
            .iter()
            .filter(|line| line.iter().any(|run| kept(run) || run.style & !shared & 0x3f != 0))
            .collect();
        assert_eq!(written.len(), points.len(), "one text element per written line: {svg}");
        let mut want = Vec::new();
        for (line, &(left, baseline)) in written.iter().zip(&points) {
            let mut at = left;
            for run in line.iter() {
                if !run.text.is_empty() {
                    want.push((at, baseline, run.width));
                }
                at += run.width;
            }
        }
        let got: Vec<(f64, f64, f64)> = probe::left_label_runs(&label, x, top)
            .iter()
            .map(|run| (run.baseline.x, run.baseline.y, run.advance))
            .collect();
        assert_eq!(got, want, "{svg}");
        runs += got.len();
    }
    println!("the walk placed {runs} runs where the SVG does");
    assert!(runs > 3000, "the walk placed only {runs} runs");
}

fn outline_ours(input: &Value) -> Value {
    let src = units_of(&input["src"]);
    let kanban = input["kanban"].as_bool().expect("kanban or not");
    match input["fn"].as_str() {
        Some("outline_tokenize") => {
            let tokens = probe::outline_tokenize(&src, kanban);
            json!({
                "types": tokens.iter().map(|token| token.0).collect::<Vec<_>>(),
                "texts": tokens.iter().map(|token| token.1.clone()).collect::<Vec<_>>(),
                "starts": tokens.iter().map(|token| token.2).collect::<Vec<_>>(),
            })
        }
        _ => {
            let (calls, stop) = probe::outline_parse(&src, kanban);
            let call = |call: &probe::OutlineCall| match call {
                probe::OutlineCall::AddNode { level, id, descr, shape, shape_data } => json!({
                    "call": "addNode", "level": level, "id": id, "descr": descr, "type": shape, "shapeData": shape_data,
                }),
                probe::OutlineCall::Icon(icon) => json!({ "call": "decorateNode", "icon": icon, "class": null }),
                probe::OutlineCall::Class(class) => json!({ "call": "decorateNode", "icon": null, "class": class }),
            };
            let error = stop.map_or(Value::Null, |(at, detail, lexical)| syntax_error(&src, at, &detail, lexical));
            json!({ "calls": calls.iter().map(call).collect::<Vec<_>>(), "error": error })
        }
    }
}

/// Lines an outline holds: nodes with and without delimiters and metadata, decorations and
/// comments, each set in, with now and then a piece of noise.
fn outline_lines(rng: &mut Rng, kanban: bool, noise: &[&str]) -> Vec<u16> {
    let pairs = [
        ("[", "]"),
        ("(", ")"),
        ("((", "))"),
        ("))", "(("),
        (")", "("),
        ("{{", "}}"),
        ("(-", "-)"),
        ("-)", "(-"),
        ("(", "]"),
    ];
    let mut out = String::new();
    for _ in 0..1 + rng.below(8) {
        out.push_str(rng.pick(&["", "  ", "    ", "\t", "      ", " "]));
        match rng.below(7) {
            0 | 1 => out.push_str(rng.pick(&["root", "Item 1", "id", "a b", "x"])),
            2 | 3 => {
                let (open, close) = *rng.pick(&pairs);
                let descr = *rng.pick(&["text", "Two words", "\"quoted [x]\"", "\"`**md** b`\"", "a<br>b"]);
                let id = *rng.pick(&["", "id", "n1"]);
                out.push_str(&format!("{id}{open}{descr}{close}"));
            }
            4 => out.push_str(rng.pick(&["::icon(fa fa-book)", "::icon(mdi:home)", ":::urgent", ":::a b"])),
            5 => out.push_str("%% a comment"),
            _ => out.push_str(rng.pick(&["Task", "[Task]"])),
        }
        if kanban && rng.below(3) == 0 {
            out.push_str(rng.pick(&[
                "@{ shape: rect }",
                "@{ ticket: \"MC-1\", label: \"a\n   b\" }",
                "@{ priority: 'High' }",
                "@{}",
            ]));
        }
        if rng.below(12) == 0 {
            out.push_str(rng.pick(noise));
        }
        out.push_str(rng.pick(&["\n", "\n", "\r\n", "\n\n"]));
    }
    units(&out)
}

#[test]
fn outlines_tokenize_and_parse_as_pele_does() {
    let pieces = [
        "mindmap",
        "kanban",
        "MindMap",
        "\n",
        "\n",
        "\n",
        "  ",
        "    ",
        "\t",
        "\r\n",
        "root",
        "id",
        "Item 1",
        "[",
        "]",
        "(",
        ")",
        "((",
        "))",
        "{{",
        "}}",
        "}",
        "-)",
        "(-",
        "\"",
        "\"quoted\"",
        "`",
        "\"`**md**`\"",
        "::icon(fa fa-book)",
        "::icon(",
        ":::urgent",
        ":::",
        "::ICON(x)",
        "%% comment",
        "%%",
        "@{",
        "@{ shape: rect }",
        "@{ label: \"a\n  b\" }",
        "@{ ticket: \"x\" }",
        "^",
        "@",
        "x y",
        "é",
        "😀",
        "\u{a0}",
        "\u{2028}",
        "#",
        ":",
    ];
    let mut rng = Rng(0x0071_1e55);
    let mut inputs = Vec::new();
    for round in 0..12_000 {
        let kanban = rng.below(2) == 0;
        let mut src = if rng.below(5) == 0 { Vec::new() } else { units(if kanban { "kanban\n" } else { "mindmap\n" }) };
        if round % 2 == 0 {
            src.extend(rng.text(&pieces, 16));
        } else {
            src.extend(outline_lines(&mut rng, kanban, &pieces));
        }
        let helper = if rng.below(3) == 0 { "outline_tokenize" } else { "outline_parse" };
        inputs.push(json!({ "fn": helper, "src": text_value(&src), "kanban": kanban }));
    }
    // A parse that draws nodes, decorates one, carries metadata, or stops with each kind of error.
    let mut reach = [0usize; 5];
    for input in inputs.iter().filter(|input| input["fn"] == "outline_parse") {
        let answer = outline_ours(input);
        let calls = answer["calls"].as_array().expect("calls");
        reach[0] += usize::from(calls.iter().filter(|call| call["call"] == "addNode").count() > 1);
        reach[1] += usize::from(calls.iter().any(|call| call["call"] == "decorateNode"));
        reach[2] += usize::from(calls.iter().any(|call| !call["shapeData"].is_null() && call["call"] == "addNode"));
        let message = &answer["error"]["message"];
        let message = if message.is_null() { String::new() } else { String::from_utf16_lossy(&units_of(message)) };
        reach[3] += usize::from(message.starts_with("Parse error"));
        reach[4] += usize::from(message.starts_with("Lexical error"));
    }
    println!(
        "outlines with nodes {}, decorations {}, metadata {}, parse errors {}, lexical errors {}",
        reach[0], reach[1], reach[2], reach[3], reach[4]
    );
    compare("random outlines", &inputs, outline_ours);
    assert!(reach.iter().all(|&count| count > 100), "an outline path is never reached: {reach:?}");
    // Pele's own parse is the one place a render tokenizes an outline.
    let parses = harvested("outline_parse");
    assert!(parses.len() > 30, "only {} harvested outlines", parses.len());
    let tokenizations: Vec<Value> = parses
        .iter()
        .map(|input| json!({ "fn": "outline_tokenize", "src": input["src"], "kanban": input["kanban"] }))
        .collect();
    compare("harvested outline tokenizations", &tokenizations, outline_ours);
    compare("harvested outline parses", parses, outline_ours);
}

#[test]
fn theme_roles_match_pele() {
    use pele_core::scene::Role;
    let theme = ask(&[json!({ "fn": "theme" })]).remove(0);
    let root = theme["root"].as_str().expect("Pele's root style");
    let drawn = probe::palette(probe::Palette::None, "{}").svg;
    assert!(drawn.contains(root), "the root sets Pele's aliases: {drawn}");
    assert!(root.contains("--_c:"), "Pele's root sets the accent alias");
    assert_eq!(probe::role_paint(Role::Accent), "var(--_c)");
    assert_eq!(probe::role_paint(Role::Critical), theme["critical"].as_str().expect("Pele's critical colour"));
}
