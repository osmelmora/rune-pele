//! Differential test for the tree view front end: for every source, this port's tree must come
//! out as Pele 1.13.0's does, byte for byte in the JSON of the oracle's `treeview_front` op, and
//! a source Pele rejects must be rejected here with Pele's message, code and position, box
//! drawing included.
//!
//! `tests/fixtures/treeview/harvest.json` holds the tree view sources of Pele's test suite and
//! the inputs of its mutation runs, written by `scripts/harvest-treeview.mjs`.

#[path = "treeview/random.rs"]
mod random;

use pele_core::{ErrorCode, ParseOptions, RenderError};
use serde_json::{Value, json};

fn harvest(list: &str) -> Vec<String> {
    let path = pele_oracle::root().join("tests/fixtures/treeview/harvest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let harvest: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    harvest[list].as_array().expect("a list").iter().map(|item| item.as_str().expect("a string").to_owned()).collect()
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

/// How many sources were read alike, and how many rejected alike by what rejected them.
#[derive(Default)]
struct Counts {
    identical: usize,
    lexer: usize,
    parser: usize,
    box_drawing: usize,
    /// Rejected by the lexer or the parser in rewritten box drawing, so placed on the line as written.
    rewritten: usize,
    other: usize,
}

fn run(what: &str, sources: &[String]) -> Counts {
    let requests: Vec<String> = sources
        .iter()
        .map(|source| {
            let source = serde_json::to_string(source).expect("a string encodes");
            format!("treeview_front\t{}", source.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029"))
        })
        .collect();
    let answers = pele_oracle::ask(&requests);
    let (mut counts, mut failures) = (Counts::default(), Vec::new());
    for (index, (source, theirs)) in sources.iter().zip(&answers).enumerate() {
        let ours = std::panic::catch_unwind(|| pele_core::treeview_front_json(source, ParseOptions::default()));
        let problem = match ours {
            Err(_) => Some("this port panics".to_owned()),
            Ok(Ok(ours)) if ours == *theirs => {
                if theirs.starts_with("{\"type\":\"treeView\",") {
                    counts.identical += 1;
                } else {
                    counts.other += 1;
                }
                None
            }
            Ok(Ok(ours)) => Some(format!("models differ\n    ours: {ours}\n    pele: {theirs}")),
            Ok(Err(RenderError::Diagnostic(diagnostic))) => {
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
                    let message = &diagnostic.message;
                    if message.starts_with("Line ") {
                        counts.box_drawing += 1;
                    } else if source.contains(['├', '└', '┣', '┗']) {
                        counts.rewritten += 1;
                    } else if message.starts_with("Lexical") {
                        counts.lexer += 1;
                    } else {
                        counts.parser += 1;
                    }
                    None
                } else {
                    Some(format!("rejected differently\n    ours: {ours}\n    pele: {theirs}"))
                }
            }
            Ok(Err(error)) => Some(format!("ours fails with {error}, pele answers {theirs}")),
        };
        if let Some(problem) = problem {
            failures.push(format!(
                "{what} {index}: {problem}\n    source: {:?}",
                source.chars().take(300).collect::<String>()
            ));
        }
    }
    println!(
        "{what}: {} inputs, {} trees identical to Pele's, rejected with Pele's error: {} by the lexer, {} by the parser, {} by the box drawing, {} in rewritten box drawing; {} of another type, {} different",
        sources.len(),
        counts.identical,
        counts.lexer,
        counts.parser,
        counts.box_drawing,
        counts.rewritten,
        counts.other,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} {what} differ from Pele:\n{}",
        failures.len(),
        failures[..failures.len().min(8)].join("\n")
    );
    counts
}

#[test]
fn harvested_sources_match_pele() {
    let counts = run("harvested tree view sources", &harvest("sources"));
    assert!(counts.identical > 500, "the harvest holds trees Pele reads");
    assert!(counts.lexer + counts.parser > 20 && counts.box_drawing > 0, "the harvest holds trees Pele rejects");
    assert!(counts.rewritten > 0, "the harvest holds box drawing that is rewritten and then rejected");
}

#[test]
fn mutated_sources_match_pele() {
    let counts = run("mutated tree view sources", &harvest("mutated"));
    assert!(counts.identical > 1000 && counts.lexer + counts.parser > 500, "the mutations are read and rejected alike");
}

/// Pele's worst cases for tree views (`tests/treeview/stress.test.ts`) at a tenth of their size,
/// and what the harvest reaches least: box drawing that is rewritten, errors inside it, and
/// positions after tabs.
fn cases() -> Vec<String> {
    let n = 5000;
    let head = "treeView-beta\n";
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let mut out: Vec<String> = [
        "treeView-beta",
        "treeView-beta  \n  a\n",
        "treeView-beta\n  a\n  title\n  b\n",
        "treeView-beta\ntitle\na\n",
        "treeView-beta\ntitle  \naccTitle:\naccDescr: \na\n",
        "---\ntitle: Front\n---\ntreeView-beta\ntitle\na\n",
        "treeView-beta\n\"multi\nline\" name\n",
        "treeView-beta\n\"unclosed\n",
        "treeView-beta\n'a' 'b'\n",
        "treeView-beta\na/\n  b/\n    c\n  d\ne\n  f\n",
        "treeView-beta\n    a\n  b\nc\n",
        "treeView-beta\n\ta\n\t\tb\n \tc\n",
        "treeView-beta\na icon(bad!)\n",
        "treeView-beta\na icon(x:)\n",
        "treeView-beta\na :::\n",
        "treeView-beta\na ::: 1\n",
        "treeView-beta\na ## \n",
        "treeView-beta\na ##\u{2028}d\u{2028}\n",
        "treeView-beta\n├── a\n│   ├── b\n│   └── c\n└── d\n",
        "treeView-beta\n├── a\n│   ├── \n└── d\n",
        "treeView-beta\n├── a\n  b\n",
        "treeView-beta\n├── a\nb\n├── c ─ d\n",
        "treeView-beta\n├─── a\n├ b\n├b\n",
        "treeView-beta\n\t├── a\n\t│\t├── b\n",
        "treeView-beta\n  ├── a\n      ├── b :::\n",
        "treeView-beta\n├── a :::1\n",
        "treeView-beta\n├── a\n│   └── b icon(\n",
        "treeView-beta\n\t│\t├── a icon(!\n",
        "treeView-beta\n├── \"a\n│   └── b\n",
        "treeView-beta\n├── a\n%% ├── comment\ntitle T\n└── b\n",
        "treeView-beta\n┣━━ a\n┃   ┗━━ b\n┗━━ c\n",
        "treeView-beta\n│   │\n├── a\n│\n",
        "treeView-beta\nroot\n├── a\n├──\n",
        "treeView-beta\nroot ─ x\n",
        "  treeView-beta\n├── a\n",
        "treeView-beta x\n├── a\n",
        "%% c\ntreeView-beta\n├── a\n│   ├── b ## d\n",
        "treeView-beta\n├── a\n    \n├── b\n",
        "treeView-beta\n├── a\n  ├── b\n     ├── c\n   └── d\n",
        "treeView-beta\n├── a\n │ ├── b\n │ │  └── c\n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    out.push(head.to_owned() + &"a\n".repeat(n / 2));
    out.push(head.to_owned() + &repeat(300, &|i| " ".repeat(i) + "d/\n"));
    out.push(head.to_owned() + &repeat(310, &|i| "\t".repeat(i % 2) + &" ".repeat(i) + "x\n"));
    out.push(head.to_owned() + &"a".repeat(n));
    out.push(format!("{head}a{}b", " ".repeat(n)));
    out.push(format!("{head}{}b", format!("a{}", " ".repeat(99)).repeat(n / 100)));
    out.push(format!("{head}{}1", format!("a :::{}", " ".repeat(95)).repeat(n / 100)));
    out.push(format!("{head}a{}", " ::: 1 icon[ #".repeat(n / 14)));
    out.push(head.to_owned() + &format!("{}:\n", " ".repeat(98)).repeat(n / 100));
    out.push(head.to_owned() + &"accDescr {\n".repeat(n / 11));
    out.push(head.to_owned() + "a\n" + &" accDescr {\n".repeat(n / 12));
    out.push(head.to_owned() + &format!("accDescr{}", "\n".repeat(40)).repeat(n / 48));
    out.push(head.to_owned() + &"\"a\n".repeat(n / 3));
    out.push(format!("{head}a{}", " :::b icon(c) ## d\n x"[..13].repeat(n / 13)));
    out.push(format!("{head}a{}", " :::highlight".repeat(n / 13)));
    out.push(head.to_owned() + &repeat(250, &|i| format!(" f{i} :::highlight ## d{i}\n")));
    out.push(head.to_owned() + &"%% c\n".repeat(n / 5));
    out.push(format!("{head}a ## {}", "word ".repeat(n / 5)));
    out.push(format!("{head} {}", "#35;&amp;".repeat(n / 9)));
    out.push(head.to_owned() + &"├── a\n".repeat(n / 6));
    out.push(head.to_owned() + &repeat(300, &|i| "│".repeat(i) + "├─ x\n"));
    out.push(head.to_owned() + "├ a\n" + &format!("{}└ b\n", " ".repeat(490)).repeat(10));
    out.push(head.to_owned() + &"│   │   │\n".repeat(n / 10));
    out.push(head.to_owned() + &format!("{}├── a\n", "\t".repeat(40)).repeat(n / 46));
    out.push(head.to_owned() + &"a ─ b\n".repeat(n / 6));
    out
}

#[test]
fn cases_match_pele() {
    run("tree view cases", &cases());
}

#[test]
fn random_trees_match_pele() {
    let mut rng = random::Rng(0x7ee5_1ab0_0c55);
    let sources: Vec<String> = (0..3000).map(|_| random::source(&mut rng)).collect();
    let counts = run("random tree views", &sources);
    assert!(counts.identical > 2000, "most random trees are read");
    assert!(counts.lexer + counts.parser > 20 && counts.box_drawing > 5, "some random trees are rejected");
}
