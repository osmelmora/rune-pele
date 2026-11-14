//! Pie charts: parse, model, layout and draw, ported from Pele's `src/diagrams/pie`.

use super::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, Matcher, NEWLINE, Reader, SINGLE_LINE_COMMENT, STRING, Scanner, SyntaxError,
    TITLE, TokenType, WHITESPACE, YAML, acc_descr_value, acc_title_value, keyword, string_value, title_value, token,
    tokenize,
};
use crate::RenderError;
use crate::catalog::{Diagram, ParseContext};
use crate::input::{JsText, Units, decimal, is_digit, is_space};
use crate::js;
use indexmap::IndexMap;

const SHOW_DATA: usize = 0;
const PIE: usize = 1;
const COLON: usize = 2;
const NUMBER: usize = 3;
const ACC_DESCR_KIND: usize = 4;
const ACC_TITLE_KIND: usize = 5;
const TITLE_KIND: usize = 6;
const STRING_KIND: usize = 7;
const NEWLINE_KIND: usize = 8;

/// Answers as `NUMBER_PIE`'s pattern does:
/// `(?:-?[0-9]+\.[0-9]+(?!\.))|(?:-?(0|[1-9][0-9]*)(?!\.))`.
/// The pattern reads a whole run of digits before it finds there is no fraction, and would do so
/// again from every digit of a long run of zeros, so the run is remembered.
fn number_scanner() -> Scanner {
    let dot = Some(u16::from(b'.'));
    let (mut from, mut end, mut fraction) = (0, 0, None);
    Box::new(move |src, p| {
        let at = p + usize::from(src.at(p) == Some(u16::from(b'-')));
        if at < from || at >= end {
            from = at;
            end = src.run(at, is_digit);
            fraction = None;
            if end > at && src.at(end) == dot {
                let stop = src.run(end + 1, is_digit);
                let length = stop - end - 1;
                if length > 0 && src.at(stop) != dot {
                    fraction = Some(stop);
                } else if length > 1 {
                    fraction = Some(stop - 1);
                }
            }
        }
        if end == at {
            return None;
        }
        if fraction.is_some() {
            return fraction;
        }
        if src.at(at) == Some(u16::from(b'0')) {
            return (src.at(at + 1) != dot).then_some(at + 1);
        }
        if src.at(end) != dot {
            return Some(end);
        }
        (end - 1 > at).then_some(end - 1)
    })
}

static PIE_TOKENS: [TokenType; 13] = [
    keyword("showData"),
    keyword("pie"),
    token(":", Matcher::Literal(":")),
    token("NUMBER_PIE", Matcher::Scanner(number_scanner)),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Default)]
struct PieAst {
    show_data: bool,
    title: Option<Vec<u16>>,
    acc_title: Option<Vec<u16>>,
    acc_descr: Option<Vec<u16>>,
    sections: Vec<(Vec<u16>, f64)>,
}

/// Pele's `parsePie`.
fn parse_pie(src: &[u16]) -> Result<PieAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &PIE_TOKENS)?, &PIE_TOKENS);
    let mut ast = PieAst::default();

    fn end_of_line(r: &mut Reader<'_>) -> Result<(), SyntaxError> {
        match r.kind() {
            None => Ok(()),
            Some(NEWLINE_KIND) => {
                while r.accept(NEWLINE_KIND) {}
                Ok(())
            }
            Some(_) => Err(r.fail("a line break or the end of input")),
        }
    }

    while r.accept(NEWLINE_KIND) {}
    r.expect(PIE)?;
    ast.show_data = r.accept(SHOW_DATA);
    while let Some(kind) = r.kind() {
        match kind {
            NEWLINE_KIND => r.skip(),
            ACC_DESCR_KIND => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                end_of_line(&mut r)?;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(r.take()));
                end_of_line(&mut r)?;
            }
            TITLE_KIND => {
                ast.title = Some(title_value(r.take()));
                end_of_line(&mut r)?;
            }
            STRING_KIND => {
                let label = string_value(r.take());
                r.expect(COLON)?;
                let value = decimal(r.expect(NUMBER)?);
                ast.sections.push((label, value));
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a section, a title, or a line break")),
        }
    }
    Ok(ast)
}

#[cfg_attr(not(test), expect(dead_code, reason = "the draw step in the following commit reads the model"))]
pub(crate) struct PieModel {
    /// Slices in source order. Of two sections with one label, the first is kept.
    sections: IndexMap<JsText, f64>,
    show_data: bool,
    title: Option<JsText>,
    acc_title: Option<JsText>,
    acc_descr: Option<JsText>,
}

/// Pele's `buildPie`. The error is its message for a negative value.
fn build_pie(ast: PieAst, front_title: Option<&JsText>) -> Result<PieModel, String> {
    let mut sections = IndexMap::new();
    for (label, value) in ast.sections {
        if value < 0.0 {
            return Err(format!(
                "\"{}\" has invalid value: {}. Negative values are not allowed in pie charts. All slice values must be >= 0.",
                String::from_utf16_lossy(&label),
                js::to_string(value)
            ));
        }
        sections.entry(JsText::from(label)).or_insert(value);
    }
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    // `accDescr.replace(/\n\s+/g, '\n')`
    let unindent = |text: Vec<u16>| {
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            out.push(text[at]);
            let spaces = text.run(at + 1, is_space);
            at = if text[at] == 10 && spaces > at + 1 { spaces } else { at + 1 };
        }
        out
    };
    Ok(PieModel {
        sections,
        show_data: ast.show_data,
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.cloned()),
        acc_title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
        acc_descr: filled(ast.acc_descr).map(|text| unindent(text).into()),
    })
}

pub(crate) struct Pie;

impl Diagram for Pie {
    type Model = PieModel;

    fn parse(cx: &ParseContext<'_>) -> Result<PieModel, RenderError> {
        let ast = parse_pie(cx.text).map_err(|error| cx.syntax(error))?;
        build_pie(ast, cx.title).map_err(|message| cx.semantic(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{JsValue, parse_json};

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn text(value: Option<&JsText>) -> String {
        value.map_or("null".to_owned(), |text| format!("{text:?}"))
    }

    /// Our outcome for one source in the words of the oracle's `model` op.
    fn ours(source: &str) -> String {
        match crate::prepare_pie_for_test(source) {
            Ok(model) => {
                let sections: Vec<String> =
                    model.sections.iter().map(|(label, value)| format!("{label:?}={:016x}", value.to_bits())).collect();
                format!(
                    "title {} accTitle {} accDescr {} showData {} sections {}",
                    text(model.title.as_ref()),
                    text(model.acc_title.as_ref()),
                    text(model.acc_descr.as_ref()),
                    model.show_data,
                    sections.join(",")
                )
            }
            Err(error) => format!("PeleError: {error}"),
        }
    }

    fn theirs(answer: &str) -> String {
        let JsValue::Object(answer) = parse_json(&units(answer)).ok().expect("the oracle answers JSON") else {
            panic!("the oracle answers an object")
        };
        if let Some(error) = answer.get("error") {
            return error.to_js_string().to_display();
        }
        let field = |name: &str| text(answer.get(name).and_then(JsValue::as_text));
        let JsValue::Array(sections) = answer.get("sections").expect("a pie model has sections") else {
            panic!("sections is an array")
        };
        let sections: Vec<String> = sections
            .iter()
            .map(|pair| {
                let JsValue::Array(pair) = pair else { panic!("a section is a pair") };
                format!("{:?}={}", pair[0].as_text().expect("a label"), pair[1].to_js_string().to_display())
            })
            .collect();
        format!(
            "title {} accTitle {} accDescr {} showData {} sections {}",
            field("title"),
            field("accTitle"),
            field("accDescr"),
            answer.get("showData").is_some_and(JsValue::truthy),
            sections.join(",")
        )
    }

    fn sources() -> Vec<String> {
        let mut out: Vec<String> = [
            "pie",
            "  pie  ",
            "\tpie\tshowData\t",
            "\n\n  pie showData title T\n\"a\":1\n\"b\":2.5",
            "pie title Pets adopted by volunteers\n    \"Dogs\" : 386\n    \"Cats\" : 85\n    \"Rats\" : 15\n",
            "pie\n accTitle: at\n accDescr { multi\n line }\n title t\n \"a\" : 1",
            "pie\n accTitle :   spaced   out   \n accDescr:   one   line   %% comment\n title   a    title   %% c\n",
            "pie\naccDescr {\n   first line  \n\n\n   second   line\t\tthird  \n \r\n  }\n\"a\": 1",
            "pie\naccDescr{}\naccTitle:\ntitle\n\"a\":1",
            "pie\naccDescr {never closed\n\"a\":1",
            "pie\naccDescr\n\"a\":1",
            "---\ntitle: front\n---\npie\n\"a\":1",
            "---\ntitle: front\n---\npie title own\n\"a\":1",
            "---\ntitle: front\n---\npie title\n\"a\":1",
            "%%{init: {\"theme\":\"dark\"}}%%\npie title A %% c\n\"a\":1 %% d\n",
            "pie showData\n \"A\" : 1\n \"A\" : 2\n \"B\" : 3",
            "pie\n 'single' : 1\n \"esc \\\" \\n \\t \\\\ \\x \\0 end\" : 2\n \"multi\nline\" : 3",
            "pie\n \"#35;x &amp;y\" : -1",
            "pie\n \"dogs\" : -60.67",
            "pie\n \"zero\" : -0\n \"big\" : 123456789012345678901234567890\n \"small\" : 0.000001\n \"frac\" : 10.50",
            "pie\n \"a\" : 01",
            "pie\n \"a\" : 1.",
            "pie\n \"a\" : .5",
            "pie\n \"a\" : 1e3",
            "pie\n \"a\" : 1.5.2",
            "pie\n \"a\" : 1..2",
            "pie\n \"a\" : 0.5.",
            "pie\n \"a\" : 00",
            "pie\n \"a\" : 0.0",
            "pie\n \"a\" : --1",
            "pie\n \"a\" : 1 2",
            "pie\n \"a\" 1",
            "pie\n \"a\" :",
            "pie\n \"a\"",
            "pie\n \"a\" : 1 \"b\" : 2",
            "pie \"a\" : 1",
            "pie title x \"a\" : 1",
            "pie showData showData",
            "pie showDatax",
            "piex",
            "pie%% comment\n\"a\":1",
            "pie x",
            "pie\n x",
            "pie\n\"unterminated",
            "pie\n\"bad \\\nescape\" : 1",
            "pie\n: 1",
            "pie\n1",
            "pie\ntitlex",
            "pie\n title\ttabbed\n\"a\":1",
            "pie\n\"a\":1\n\n\n",
            "pie\n\"a\":1\n  %% trailing comment\n\n",
            "pie\n\"a\":1 %% then\n\"b\"",
            "pie\n---\nyaml: here\n---\n\"a\":1",
            "pie\n---\nnever closed\n\"a\":1",
            "pie\n--- x\n\"a\":1",
            "pie\n%%{ directive }%%\n\"a\":1",
            "pie\n  %%{ unclosed\n\"a\":1",
            "showData pie",
            "\"a\": 1",
            "pie title #35; is a hash; and #quot;quoted#quot;\n\"Issue #35;\": 1\n\"A #amp; B\": 2",
            "pie title style a: b #c; d; e\n\"x\": 1",
            "pie\n\"\u{1f600} emoji\" : 1\n\"\u{e9}\" x",
            "pie\r\n\"a\" : 1\r\"b\" : 2\r\n",
            "pie\n\u{a0}\"a\": 1",
            "pie\n\"a\"\u{a0}: 1",
            "pie\n  \n\t\n\"a\":1\n  ",
            "pie accTitle: on the pie line\n\"a\":1",
            "pie title one\ntitle two\naccTitle: one\naccTitle: two\n\"a\":1",
            "pie\n\"a\":1\n\"b\":2\n\"c\"",
            "pie\n\"a\":1\n}",
            "%% only a comment\n",
            "pie\n\"a\":1\n\"b\": x #35; y",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        out.push(format!("pie\n\"a\": {}", "0".repeat(300)));
        out.push(format!("pie\n\"a\": {}", "9".repeat(400)));
        out
    }

    #[test]
    fn pie_model_matches_pele() {
        let sources = sources();
        let requests: Vec<String> =
            sources.iter().map(|source| format!("model\t{}", crate::json_string(source))).collect();
        for (source, answer) in sources.iter().zip(pele_oracle::ask(&requests)) {
            assert_eq!(ours(source), theirs(&answer), "model of {source:?}");
        }
    }
}
