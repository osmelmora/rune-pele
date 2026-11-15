//! Differential test for the four railroad front ends: for every source, this port's rule model
//! must come out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's
//! `railroad_front` op, and a source Pele rejects must be rejected here with Pele's message and
//! position. Each line of output counts one notation on its own.
//!
//! `tests/fixtures/railroad/harvest.json` holds the railroad sources of Pele's own test suite and
//! the inputs of its generated and mutated runs, written by `scripts/harvest-railroad.mjs`.

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/railroad/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
}

/// A source as one JSON string on one line, as the oracle reads it.
fn request(source: &str) -> String {
    let source = serde_json::to_string(source).expect("a string encodes");
    let source = source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    format!("railroad_front\t{source}")
}

fn code_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Unsupported => "unsupported-diagram",
        ErrorCode::Syntax => "syntax",
        ErrorCode::Semantic => "semantic",
        ErrorCode::Limit => "limit",
        _ => "unknown",
    }
}

/// The notation the first line that starts with a railroad keyword names, in any case, so that
/// front matter and directives before it do not count.
fn notation(source: &str) -> &'static str {
    let keyword = |line: &str| {
        let head = line.trim_start().to_ascii_lowercase();
        [
            ("railroad-ebnf-beta", "EBNF"),
            ("railroad-abnf-beta", "ABNF"),
            ("railroad-peg-beta", "PEG"),
            ("railroad-beta", "railroad"),
        ]
        .into_iter()
        .find_map(|(word, name)| head.starts_with(word).then_some(name))
    };
    source.lines().find_map(keyword).unwrap_or("no keyword")
}

#[derive(Default)]
struct Tally {
    identical: usize,
    rejected: usize,
    lexical: usize,
    other_types: usize,
    failures: Vec<String>,
}

/// Where two texts first differ, with what each holds there.
fn first_difference(ours: &str, theirs: &str) -> String {
    let at = ours.bytes().zip(theirs.bytes()).position(|(a, b)| a != b).unwrap_or(ours.len().min(theirs.len()));
    let around = |text: &str| {
        let bytes = text.as_bytes();
        String::from_utf8_lossy(&bytes[at.saturating_sub(80).min(bytes.len())..(at + 80).min(bytes.len())]).into_owned()
    };
    format!("first differing byte {at}\n    ours: {:?}\n    pele: {:?}", around(ours), around(theirs))
}

impl Tally {
    fn compare(&mut self, name: &str, source: &str, ours: Result<String, RenderError>, theirs: &str) {
        let problem = match ours {
            Ok(ours) if ours == theirs => {
                if theirs.starts_with("{\"type\":\"railroad\",") {
                    self.identical += 1;
                } else {
                    self.other_types += 1;
                }
                None
            }
            Ok(ours) => Some(first_difference(&ours, theirs)),
            Err(RenderError::Diagnostic(diagnostic)) => {
                let location = diagnostic.location.as_ref();
                let ours = json!({
                    "error": format!("PeleError: {}", diagnostic.message),
                    "code": code_name(diagnostic.code),
                    "type": diagnostic.kind.map(|kind| kind.name()),
                    "mermaidType": diagnostic.mermaid_type.map(|mermaid| mermaid.name()),
                    "line": location.map_or(0, |location| location.line),
                    "column": location.map_or(0, |location| location.column_utf16),
                    "snippet": location.map_or("", |location| location.snippet.as_str()),
                });
                let theirs: Value = serde_json::from_str(theirs).expect("the oracle answers JSON");
                if ours == theirs {
                    self.rejected += 1;
                    self.lexical += usize::from(diagnostic.message.starts_with("Lexical error"));
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Err(error) => Some(format!("ours fails with {error}, pele answers {}", &theirs[..theirs.len().min(200)])),
        };
        if let Some(problem) = problem {
            self.failures.push(format!("{name}: {problem}\n    source: {source:?}"));
        }
    }
}

/// Puts every source through both front ends, and prints one line per notation.
fn run(what: &str, sources: &[String]) -> BTreeMap<&'static str, Tally> {
    let requests: Vec<String> = sources.iter().map(|source| request(source)).collect();
    let answers = pele_oracle::ask(&requests);
    let mut tallies: BTreeMap<&'static str, Tally> = BTreeMap::new();
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let tally = tallies.entry(notation(source)).or_default();
        let name = format!("{what} {index}");
        match std::panic::catch_unwind(|| pele_core::railroad_probe::front_json(source, ParseOptions::default())) {
            Ok(ours) => tally.compare(&name, source, ours, theirs),
            Err(_) => tally.failures.push(format!("{name}: this port panics\n    source: {source:?}")),
        }
    }
    let mut failures = Vec::new();
    for (notation, tally) in &tallies {
        let total = tally.identical + tally.rejected + tally.other_types + tally.failures.len();
        println!(
            "{what}, {notation}: {total} inputs, {} models identical to Pele's, {} rejected with Pele's error ({} lexical), {} of another type, {} different",
            tally.identical,
            tally.rejected,
            tally.lexical,
            tally.other_types,
            tally.failures.len()
        );
        failures.extend(tally.failures.iter().take(6).cloned());
    }
    let different: usize = tallies.values().map(|tally| tally.failures.len()).sum();
    assert!(different == 0, "{different} of {} {what} differ from Pele:\n{}", sources.len(), failures.join("\n"));
    tallies
}

const NOTATIONS: [&str; 4] = ["railroad", "EBNF", "ABNF", "PEG"];

#[test]
fn harvested_sources_match_pele() {
    let tallies = run("harvested railroad sources", &harvest("sources"));
    for notation in NOTATIONS {
        let tally = &tallies[notation];
        assert!(tally.identical > 100, "the harvest holds {notation} grammars Pele reads");
        assert!(tally.rejected > 50, "the harvest holds {notation} grammars Pele rejects");
    }
}

#[test]
fn mutated_sources_match_pele() {
    let tallies = run("mutated railroad sources", &harvest("mutated"));
    for notation in NOTATIONS {
        assert!(tallies[notation].lexical > 0, "the mutations reach a lexical error in {notation}");
    }
}

/// Inputs for what the harvest reaches least: case, every escape, the repeats that overflow, the
/// error at each place a notation can stop, and comments that never close.
fn cases() -> Vec<String> {
    let mut out: Vec<String> = [
        "railroad-beta",
        "RAILROAD-BETA\nr = terminal(\"a\") ;",
        "Railroad-EBNF-Beta\nr = \"a\" ;",
        "  \n railroad-peg-beta\nR <- a ;",
        "railroad-beta-x\nr = terminal(\"a\") ;",
        "railroad-betax\nr = terminal(\"a\") ;",
        "railroad-beta%% c\nr = terminal(\"a\") ;",
        "railroad-beta\nr = terminal(\"a\\nb\\tc\\rd\\\\e\\\"f\\'g\\x\") ;",
        "railroad-beta\nr = terminal('it\\'s') ;",
        "railroad-beta\nr = terminal(\"\\",
        "railroad-beta\nr = terminal(\"a\\\nb\") ;",
        "railroad-beta\nr = terminal(\"\") ;",
        "railroad-beta\nr = choice(terminal(\"a\")) ;",
        "railroad-beta\nr = choice() ;",
        "railroad-beta\nr = optional() ;",
        "railroad-beta\nr = zeroOrMore(terminal(\"a\"), terminal(\"b\")) ;",
        "railroad-beta\nr = sequence(terminal(\"a\"),) ;",
        "railroad-beta\nr = sequence(terminal(\"a\") ;",
        "railroad-beta\nr = terminal(\"a\") ",
        "railroad-beta\nr = /* unclosed",
        "railroad-beta\n/**/ r = terminal(\"a\") ; /* */",
        "railroad-beta\nterminal = terminal(\"a\") ;",
        "railroad-beta\nterminals = nonterminal(\"choice\") ;",
        "railroad-beta\ntitle\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle \"\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle '\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle \"a\\\"\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle 'x\\ty'\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle a\ttab %% c\nr = terminal(\"a\") ;",
        "railroad-beta\ntitle-x = terminal(\"a\") ;",
        "railroad-beta\ntitle_x = terminal(\"a\") ;",
        "railroad-beta\naccTitle : spaced\naccDescr  :  one\nr = terminal(\"a\") ;",
        "railroad-beta\naccTitle\nr = terminal(\"a\") ;",
        "railroad-beta\naccDescr {\n  one\n\n   two  \n}\nr = terminal(\"a\") ;",
        "railroad-beta\naccDescr\n{ x }\nr = terminal(\"a\") ;",
        "railroad-beta\naccDescr { never closed\nr = terminal(\"a\") ;",
        "railroad-beta\naccDescr { never\naccDescr { closed\nr = terminal(\"a\") ;",
        "railroad-beta\naccDescrs = terminal(\"a\") ;",
        " railroad-beta\n  accTitle: x\nr = terminal(\"a\") ;",
        "railroad-ebnf-beta\nr = ;",
        "railroad-ebnf-beta\nr = a",
        "railroad-ebnf-beta\nr = ( ) ;",
        "railroad-ebnf-beta\nr = a | ;",
        "railroad-ebnf-beta\nr = | a ;",
        "railroad-ebnf-beta\nr = a , ;",
        "railroad-ebnf-beta\nr = [ a ) ;",
        "railroad-ebnf-beta\nr = { a ] ;",
        "railroad-ebnf-beta\nr = ( a ;",
        "railroad-ebnf-beta\nr = a ) ;",
        "railroad-ebnf-beta\nr = a - ;",
        "railroad-ebnf-beta\nr = a - - b ;",
        "railroad-ebnf-beta\nr = a -- b ;",
        "railroad-ebnf-beta\nr = a ---\nx: 1\n--- b ;",
        "railroad-ebnf-beta\nr = a?*+ ;",
        "railroad-ebnf-beta\nr = a - b - \"c\" ;",
        "railroad-ebnf-beta\nr = a - ( b | c )* ;",
        "railroad-ebnf-beta\nr = a* - [ b ] - { c }+ ;",
        "railroad-ebnf-beta\nr = ( a - b ) - c ;",
        "railroad-ebnf-beta\nr = a - ( ( b ) ) ;",
        "railroad-ebnf-beta\nr = a? b? ;",
        "railroad-ebnf-beta\nr = a ? b ? ;",
        "railroad-ebnf-beta\nr = ?   ? ;",
        "railroad-ebnf-beta\nr = ? a ; ? ;",
        "railroad-ebnf-beta\nr = ? \u{a0}x\u{2028} ? ;",
        "railroad-ebnf-beta\nr = ? \u{a0}\u{3000} ? ;",
        "railroad-ebnf-beta\nr ::= a ;\ns = b ;",
        "railroad-ebnf-beta\nr :: a ;",
        "railroad-ebnf-beta\nr = a (* c *) b ;",
        "railroad-ebnf-beta\nr = a (*) b *) c ;",
        "railroad-ebnf-beta\nr = a (*) b ;",
        "railroad-ebnf-beta\nr = a (* never closed ;",
        "railroad-ebnf-beta\nr = (* x *)(* y *) a ;\n(*\n*) s = b ;",
        "railroad-ebnf-beta\nr = 'it\\'s' \"\\\"\" \"\\n\" ;",
        "railroad-ebnf-beta\nr = a ;\ntitle Late",
        "railroad-ebnf-beta\nr = title ;",
        "railroad-ebnf-beta\ntitles = a ;",
        "railroad-ebnf-beta\ntitle first\ntitle second\nr = a ;",
        "railroad-ebnf-beta\ntitle   \"A \\\"quoted\\\"\\ttitle\"   \naccTitle:   Acc   title\naccDescr: One   line\nr = a ;",
        "---\ntitle: Front\n---\nrailroad-ebnf-beta\nr = a ;",
        "---\ntitle: Front\n---\nrailroad-ebnf-beta\ntitle Own\nr = a ;",
        "---\ntitle: Front\n---\nrailroad-ebnf-beta\ntitle\nr = a ;",
        "---\ntitle: Front\n---\nrailroad-ebnf-beta\ntitle \"\"\nr = a ;",
        "railroad-ebnf-beta\nr = \"#35;x &amp; #quot;\" #35; ;",
        "railroad-ebnf-beta\n__proto__ = constructor | toString hasOwnProperty ;\nconstructor = \"__proto__\" ;",
        "railroad-ebnf-beta\nr = \u{e9} ;",
        "railroad-ebnf-beta\nr = a\u{a0}b ;",
        "railroad-abnf-beta\nr = a /\n ;",
        "railroad-abnf-beta\nr = 2* ;",
        "railroad-abnf-beta\nr = a ; \n",
        "railroad-abnf-beta\nr = a ;",
        "railroad-abnf-beta\nr = a ;x\n b ;",
        "railroad-abnf-beta\nr = a ; a comment, so the rule goes on\n  b ;\ns = c ;",
        "railroad-abnf-beta\nr = *a 1*a 2*4a *4a 3a 1a 007a 0*1a *1\"a\" 0a 0*0a 1*1a ;",
        "railroad-abnf-beta\nr = 2*( a / b ) 3[ a ] *[ a ] 1*2( [ a ] ) ;",
        "railroad-abnf-beta\nr = 1*2*3a ;",
        "railroad-abnf-beta\nr = ** a ;",
        "railroad-abnf-beta\nr = 1000000000000000000000a 1000000000000000000000*a *1000000000000000000000a ;",
        "railroad-abnf-beta\nr = 123456789012345678901234567890*99999999999999999999999a ;",
        "railroad-abnf-beta\nr = %x41 %d65.66 %b1000001 %x30-39 %X4f %D1-2.3 %x1- %x ;",
        "railroad-abnf-beta\nr = %x1.2-3.4 ;",
        "railroad-abnf-beta\nr = %q1 ;",
        "railroad-abnf-beta\nr = \"a\\\" b ;",
        "railroad-abnf-beta\nr = \"unclosed ;",
        "railroad-abnf-beta\nr = ( a ] ;",
        "railroad-abnf-beta\nr = [ a ) ;",
        "railroad-abnf-beta\nr = ( a ;",
        "railroad-abnf-beta\nr = a ) ;",
        "railroad-abnf-beta\nr = rule-name DIGIT a1 A-1- ;",
        "railroad-abnf-beta\nr = _a ;",
        "railroad-abnf-beta\ntitle \"a\\tb\"\nr = a ;",
        "railroad-abnf-beta\ntitle-x = a ;\ntitles = b ;",
        "railroad-abnf-beta\ntitle_x\nr = a ;",
        "railroad-abnf-beta\n; only a comment\nr = a ;",
        "railroad-peg-beta\nR <- ;",
        "railroad-peg-beta\nR = a ;",
        "railroad-peg-beta\nR <- a ) ;",
        "railroad-peg-beta\nR <- ( a ;",
        "railroad-peg-beta\nR <- & ;",
        "railroad-peg-beta\nR <- !&a ;",
        "railroad-peg-beta\nR <- a** ;",
        "railroad-peg-beta\nR <- a?+ ;",
        "railroad-peg-beta\nR <- &a !\"x\" !. . &( a ) !( a b ) !a* &( a / b ) ( !a )+ ;",
        "railroad-peg-beta\nR <- !( ( a ) ) &( \"x\" ) !( . ) &( ? ) ;",
        "railroad-peg-beta\nR <- a / / b ;",
        "railroad-peg-beta\nR <- a # comment\n / b # more\n ;",
        "railroad-peg-beta\nR <- \"\\b\" '\\'' ;",
        "railroad-peg-beta\nR <- a <- b ;",
        "railroad-peg-beta\nR <- a < - b ;",
        "railroad-peg-beta\ntitle 'single'\naccDescr {\n   Two\n   lines\n}\nR <- a ;",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.push(format!("railroad-abnf-beta\nr = {}a ;", "9".repeat(400)));
    out.push(format!("railroad-abnf-beta\nr = {}*{}a {}b ;", "9".repeat(400), "9".repeat(400), "9".repeat(400)));
    out.push(format!("railroad-abnf-beta\nr = 0{}1a ;", "0".repeat(400)));
    out.push(format!("railroad-ebnf-beta\nr = {}a{} ;", "( ".repeat(300), " )".repeat(300)));
    out.push(format!("railroad-beta\nr = {}terminal(\"a\"){} ;", "optional(oneOrMore(".repeat(150), "))".repeat(150)));
    out.push(format!("railroad-abnf-beta\nr = {}a{} ;", "2*3[ ".repeat(200), " ]".repeat(200)));
    out.push(format!("railroad-peg-beta\nR <- {}a{} ;", "!( &( ".repeat(100), " ) )".repeat(100)));
    out.push(format!("railroad-ebnf-beta\nr = {} ;", "([{".repeat(100)));
    out.push(format!("railroad-ebnf-beta\nr = ?{} ;", " a".repeat(500)));
    out
}

#[test]
fn cases_match_pele() {
    run("railroad cases", &cases());
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

const STRINGS: &[&str] = &[
    "\"a\"",
    "'b'",
    "\"x y\"",
    "\"\\n\"",
    "\"\\\"\"",
    "\"q\\\\\"",
    "\"\"",
    "\"+\"",
    "\"\u{e9}\"",
    "'it\\'s'",
    "\"a  b\"",
    "\"?\"",
    "\";\"",
    "\"(*\"",
    "\" \"",
    "\"#35;\"",
    "\"<br>\"",
    "\"\\t\"",
];
const NAMES: &[&str] =
    &["r", "rule", "my_rule", "a-b", "A1", "terminals", "x_", "choice2", "digit", "Expr", "b", "titles", "title-x"];

fn several(rng: &mut Rng, max: usize, joints: &[&str], mut make: impl FnMut(&mut Rng) -> String) -> String {
    let count = 1 + rng.below(max);
    let joint = rng.pick(joints);
    (0..count).map(|_| make(rng)).collect::<Vec<_>>().join(joint)
}

fn ir(rng: &mut Rng, depth: usize) -> String {
    match rng.below(10) {
        _ if depth == 0 => format!("{}({})", rng.pick(&["terminal", "nonterminal", "special"]), rng.pick(STRINGS)),
        0..=3 => format!("{}({})", rng.pick(&["terminal", "nonterminal", "special"]), rng.pick(STRINGS)),
        4..=7 => {
            let head = rng.pick(&["sequence", "choice"]);
            format!("{head}({})", several(rng, 4, &[", ", ",", " , ", ",\n  "], |rng| ir(rng, depth - 1)))
        }
        _ => format!("{}( {} )", rng.pick(&["optional", "oneOrMore", "zeroOrMore"]), ir(rng, depth - 1)),
    }
}

fn ebnf(rng: &mut Rng, depth: usize) -> String {
    let primary = |rng: &mut Rng, depth: usize| -> String {
        if depth == 0 || rng.below(10) < 7 {
            return match rng.below(10) {
                0 => rng.pick(&["? any ?", "?x?", "? two words ?"]).to_owned(),
                1..=4 => rng.pick(STRINGS).to_owned(),
                _ => rng.pick(NAMES).to_owned(),
            };
        }
        let (open, close) = [("( ", " )"), ("[ ", " ]"), ("{ ", " }"), ("(", ")")][rng.below(4)];
        format!("{open}{}{close}", ebnf(rng, depth - 1))
    };
    several(rng, 3, &[" | ", "|"], |rng| {
        several(rng, 3, &[" ", " , ", ",", " (* c *) ", " /* c */ "], |rng| {
            let mut term = primary(rng, depth);
            while rng.chance(4) {
                match rng.below(5) {
                    0 => term.push('?'),
                    1 => term.push('*'),
                    2 => term.push('+'),
                    _ => {
                        let except = primary(rng, depth.saturating_sub(1));
                        term.push_str(&format!("{}{except}", rng.pick(&[" - ", "-"])));
                    }
                }
            }
            term
        })
    })
}

fn abnf(rng: &mut Rng, depth: usize) -> String {
    several(rng, 3, &[" / ", "/"], |rng| {
        several(rng, 3, &[" ", "  ", "\n  "], |rng| {
            let repeat = rng.pick(&["", "", "", "*", "1*", "2*3", "3", "*4", "0*1", "1", "0", "*1", "12*", "0*0"]);
            if depth == 0 || rng.below(10) < 7 {
                let value = rng.pick(&[
                    "\"s\"",
                    "\"a b\"",
                    "\"\"",
                    "%x41",
                    "%d1.2",
                    "%b0-1",
                    "%x30-39.2",
                    "name",
                    "DIGIT",
                    "rule-name",
                    "a1",
                ]);
                return format!("{repeat}{value}");
            }
            let (open, close) = [("( ", " )"), ("[ ", " ]"), ("(", ")")][rng.below(3)];
            format!("{repeat}{open}{}{close}", abnf(rng, depth - 1))
        })
    })
}

fn peg(rng: &mut Rng, depth: usize) -> String {
    several(rng, 3, &[" / ", "/"], |rng| {
        several(rng, 3, &[" ", "  ", " # c\n  "], |rng| {
            let head = rng.pick(&["", "", "", "&", "!"]);
            let tail = rng.pick(&["", "", "", "?", "*", "+"]);
            if depth == 0 || rng.below(10) < 7 {
                let choices = if rng.chance(2) { STRINGS } else { NAMES };
                let primary = if rng.chance(8) { ".".to_owned() } else { rng.pick(choices).to_owned() };
                return format!("{head}{primary}{tail}");
            }
            format!("{head}( {} ){tail}", peg(rng, depth - 1))
        })
    })
}

const HEADERS: &[&str] = &[
    "title T\n",
    "title \"Quoted \\\" t\"\n",
    "title 'q'\n",
    "accTitle: a  b\n",
    "accDescr: d\n",
    "accDescr {\n multi\n line\n}\n",
    "title\n",
    "%% a comment\n",
    "%%{init: {\"railroad\": {\"arcRadius\": 4}}}%%\n",
];

/// A random grammar in one notation, and now and then one edit that may break it.
fn random_grammar(rng: &mut Rng, notation: usize) -> String {
    let keyword = ["railroad-beta", "railroad-ebnf-beta", "railroad-abnf-beta", "railroad-peg-beta"][notation];
    let mut src = format!("{keyword}{}", rng.pick(&["\n", " ", "\n\n"]));
    while rng.chance(3) {
        src.push_str(rng.pick(HEADERS));
    }
    for _ in 0..rng.below(4) {
        let name = rng.pick(NAMES);
        let rule = match notation {
            0 => format!("{name} = {} ;", ir(rng, 4)),
            1 => format!("{name} {} {} ;", rng.pick(&["=", "::="]), ebnf(rng, 3)),
            2 => format!(
                "{} = {} ;{}",
                rng.pick(&["r", "rule", "a-b", "A1", "titles"]),
                abnf(rng, 3),
                rng.pick(&["", "", " c"])
            ),
            _ => format!("{name} <- {} ;", peg(rng, 3)),
        };
        src.push_str(&rule);
        src.push_str(rng.pick(&["\n", "\n\n", "\n%% c\n"]));
    }
    if rng.chance(4) && !src.is_empty() {
        let units: Vec<char> = src.chars().collect();
        let at = rng.below(units.len());
        let edit: String = match rng.below(3) {
            0 => units[..at].iter().chain(&units[(at + 1).min(units.len())..]).collect(),
            1 => {
                let piece = rng.pick(&[
                    "(", ")", ";", "\"", "|", "/", "*", "-", "?", "title ", "%%", "\n", "[", "}", "<-", "&", "2*",
                ]);
                units[..at].iter().collect::<String>() + piece + &units[at..].iter().collect::<String>()
            }
            _ => units[..at].iter().collect(),
        };
        return edit;
    }
    src
}

#[test]
fn random_grammars_match_pele() {
    let mut rng = Rng(0x7a11_70ad_5eed);
    let sources: Vec<String> = (0..4000).map(|i| random_grammar(&mut rng, i % 4)).collect();
    let tallies = run("random railroad grammars", &sources);
    for notation in NOTATIONS {
        let tally = &tallies[notation];
        assert!(tally.identical > 300, "many random {notation} grammars are read");
        assert!(tally.rejected > 50, "some random {notation} grammars are rejected");
    }
}
