//! Pele's `parseTreemap` (`treemap/parser.ts`): rows of sections, leaves and class definitions,
//! told apart by their tokens alone, as Mermaid's treemap grammar hides its line breaks.

use crate::diagrams::tokens::{
    ACC_DESCR, ACC_TITLE, Matcher, Reader, SyntaxError, TokenType, acc_descr_value, acc_title_value, hidden,
    title_before, title_value, token, tokenize, word_unit,
};
use crate::input::{Units, decimal, is_blank, is_digit, is_space, is_word};

const ACC_DESCR_KIND: usize = 0;
const ACC_TITLE_KIND: usize = 1;
const TITLE_KIND: usize = 2;
const KEYWORD: usize = 3;
const CLASS_DEF: usize = 4;
const STYLE_SEPARATOR: usize = 5;
const SEPARATOR: usize = 6;
const COMMA: usize = 7;
const INDENTATION: usize = 8;
const ID: usize = 12;
const NUMBER: usize = 13;
const STRING: usize = 14;

/// Mermaid's treemap terminals. Every run of spaces or tabs is an `INDENTATION` token, since that
/// comes before `WS`, and line breaks are hidden, so the indentation of a row is whatever
/// whitespace precedes it.
static TOKENS: [TokenType; 15] = [
    ACC_DESCR,
    ACC_TITLE,
    title_before(word_unit),
    token("TREEMAP_KEYWORD", Matcher::Pattern(keyword)),
    token("CLASS_DEF", Matcher::Pattern(class_def_end)),
    token("STYLE_SEPARATOR", Matcher::Literal(":::")),
    token("SEPARATOR", Matcher::Literal(":")),
    token("COMMA", Matcher::Literal(",")),
    token("INDENTATION", Matcher::Pattern(blanks)),
    hidden("WS", Matcher::Pattern(blanks)),
    hidden("ML_COMMENT", Matcher::Pattern(comment)),
    hidden("NL", Matcher::Pattern(newline)),
    token("ID2", Matcher::Pattern(id)),
    token("NUMBER2", Matcher::Pattern(number)),
    token("STRING2", Matcher::Pattern(string)),
];

/// `treemap-beta|treemap`
fn keyword(src: &[u16], at: usize) -> Option<usize> {
    if src.starts_at(at, "treemap-beta") { Some(at + 12) } else { src.starts_at(at, "treemap").then_some(at + 7) }
}

/// What `classDef\s+([a-zA-Z_][a-zA-Z0-9_]+)(?:\s+([^;\r\n]*))?(?:;)?` matches at `at`: where the
/// match ends, the class name, and the declarations where any whitespace follows the name.
struct ClassDefMatch {
    end: usize,
    name: (usize, usize),
    styles: Option<(usize, usize)>,
}

fn class_def(src: &[u16], at: usize) -> Option<ClassDefMatch> {
    if !src.starts_at(at, "classDef") {
        return None;
    }
    let name = src.run(at + 8, is_space);
    let first = src.at(name)?;
    if name == at + 8 || !(is_word(first) && !is_digit(first)) {
        return None;
    }
    let name_end = src.run(name + 1, is_word);
    if name_end == name + 1 {
        return None;
    }
    let gap = src.run(name_end, is_space);
    let (mut end, styles) = if gap > name_end {
        let styles_end = src.run(gap, |c| c != 59 && c != 13 && c != 10);
        (styles_end, Some((gap, styles_end)))
    } else {
        (name_end, None)
    };
    end += usize::from(src.at(end) == Some(59));
    Some(ClassDefMatch { end, name: (name, name_end), styles })
}

fn class_def_end(src: &[u16], at: usize) -> Option<usize> {
    class_def(src, at).map(|found| found.end)
}

/// `[ \t]{1,}`
fn blanks(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, is_blank))
}

/// `\%\%[^\n]*`
fn comment(src: &[u16], at: usize) -> Option<usize> {
    src.starts_at(at, "%%").then(|| src.run(at + 2, |c| c != 10))
}

/// `\r?\n`
fn newline(src: &[u16], at: usize) -> Option<usize> {
    let line_feed = at + usize::from(src.at(at) == Some(13));
    (src.at(line_feed) == Some(10)).then_some(line_feed + 1)
}

/// `[a-zA-Z_][a-zA-Z0-9_]*`
fn id(src: &[u16], at: usize) -> Option<usize> {
    src.at(at).filter(|&c| is_word(c) && !is_digit(c)).map(|_| src.run(at + 1, is_word))
}

/// `[0-9_\.\,]+`
fn number(src: &[u16], at: usize) -> Option<usize> {
    Some(src.run(at, |c| is_digit(c) || matches!(c, 95 | 46 | 44)))
}

/// `"[^"]*"|'[^']*'`
fn string(src: &[u16], at: usize) -> Option<usize> {
    let quote = src.at(at).filter(|&c| c == 34 || c == 39)?;
    src.find_unit(quote, at + 1).map(|close| close + 1)
}

/// A row of the diagram, at the indentation that precedes it.
pub(crate) enum Row {
    Section { indent: Option<usize>, name: Vec<u16>, class_selector: Option<Vec<u16>> },
    Leaf { indent: Option<usize>, name: Vec<u16>, value: f64, class_selector: Option<Vec<u16>> },
    ClassDef { name: Vec<u16>, style_text: Option<Vec<u16>> },
}

#[derive(Default)]
pub(crate) struct TreemapAst {
    pub(crate) title: Option<Vec<u16>>,
    pub(crate) acc_title: Option<Vec<u16>>,
    pub(crate) acc_descr: Option<Vec<u16>>,
    pub(crate) rows: Vec<Row>,
}

/// `parseFloat` of text that holds only digits, `_` and `.`: the longest prefix that is a
/// decimal number, or NaN where there is none.
fn parse_float(text: &[u16]) -> f64 {
    let whole = text.run(0, is_digit);
    let fraction = if text.at(whole) == Some(46) { text.run(whole + 1, is_digit) } else { whole };
    if whole == 0 && fraction <= 1 {
        return f64::NAN;
    }
    decimal(&text[..fraction])
}

/// Pele's `parseTreemap`.
pub(crate) fn parse(src: &[u16]) -> Result<TreemapAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &TOKENS)?, &TOKENS);
    let mut ast = TreemapAst::default();
    r.expect(KEYWORD)?;
    while let Some(kind) = r.kind() {
        match kind {
            ACC_DESCR_KIND => {
                ast.acc_descr = Some(acc_descr_value(r.take()));
                continue;
            }
            ACC_TITLE_KIND => {
                ast.acc_title = Some(acc_title_value(r.take()));
                continue;
            }
            TITLE_KIND => {
                ast.title = Some(title_value(r.take()));
                continue;
            }
            INDENTATION | STRING | CLASS_DEF => {}
            _ => return Err(r.fail("a node, a class definition, or a title")),
        }
        let indent = (kind == INDENTATION).then(|| r.take().len());
        if r.kind() == Some(CLASS_DEF) {
            let text = r.take();
            let found = class_def(text, 0).expect("a CLASS_DEF token matches its own pattern");
            let name = text[found.name.0..found.name.1].to_vec();
            let style_text = found.styles.map(|(from, to)| text[from..to].to_vec()).filter(|text| !text.is_empty());
            ast.rows.push(Row::ClassDef { name, style_text });
            continue;
        }
        let quoted = r.expect(STRING)?;
        let name = quoted[1..quoted.len() - 1].to_vec();
        // A leaf is a name followed by a colon or comma, with or without whitespace before it.
        let after = usize::from(r.peek(0) == Some(INDENTATION));
        let leaf = matches!(r.peek(after), Some(SEPARATOR | COMMA));
        let value = if leaf {
            for _ in 0..=after {
                r.skip();
            }
            r.accept(INDENTATION);
            let digits: Vec<u16> = r.expect(NUMBER)?.iter().copied().filter(|&c| c != 44).collect();
            Some(parse_float(&digits))
        } else {
            None
        };
        let class_selector = if r.accept(STYLE_SEPARATOR) { Some(r.expect(ID)?.to_vec()) } else { None };
        ast.rows.push(match value {
            Some(value) => Row::Leaf { indent, name, value, class_selector },
            None => Row::Section { indent, name, class_selector },
        });
    }
    Ok(ast)
}
