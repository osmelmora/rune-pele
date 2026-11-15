//! Differential test for drawing event models: every source must come out of this port as it
//! comes out of Pele 1.13.0, byte for byte and with the same width and height, and a source Pele
//! rejects must be rejected here with Pele's message.
//!
//! `tests/fixtures/eventmodeling/harvest.json` holds the event modeling sources of Pele's test
//! suite and the inputs of its mutation runs, written by `scripts/harvest-eventmodeling.mjs`. A
//! model wider than a host under 640 pixels is drawn down instead, which the widths reach. The
//! random models must reach every drawing path in [`PATHS`].

#[path = "eventmodeling/random.rs"]
mod random;
#[path = "standalone/parity.rs"]
mod standalone;

use pele_core::ParseOptions;
use serde_json::{Value, json};
use standalone::{Case, Parity, Reached, WIDTHS, attribute, harvest, plain, redrawn_to_fit, tags};

/// The model this port reads, which `eventmodeling_front.rs` holds to Pele's.
fn model(source: &str) -> Value {
    pele_core::eventmodeling_front_json(source, ParseOptions::default())
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or(Value::Null)
}

fn down(svg: &str) -> bool {
    svg.contains("class=\"pele-em-frame\"")
}

/// The path of every flow.
fn flows(svg: &str) -> Vec<&str> {
    svg.split("<g class=\"pele-edge\" data-id=\"")
        .skip(1)
        .filter_map(|rest| {
            let at = rest.find(" d=\"")? + 4;
            Some(&rest[at..at + rest[at..].find('"')?])
        })
        .collect()
}

/// The numbers of a path, in order.
fn numbers(d: &str) -> Vec<f64> {
    d.split(|c: char| c.is_ascii_alphabetic() || c == ',' || c == ' ')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().expect("a number"))
        .collect()
}

/// The texts of every `tspan` of the elements of a class.
fn rows<'a>(svg: &'a str, class: &str) -> Vec<&'a str> {
    let open = format!("class=\"{class}\"");
    svg.split(&open)
        .skip(1)
        .flat_map(|rest| {
            let text = &rest[..rest.find("</text>").unwrap_or(0)];
            text.split("<tspan").skip(1).filter_map(|span| Some(&span[span.find('>')? + 1..span.find("</tspan>")?]))
        })
        .collect()
}

fn relations(c: &Case) -> Vec<(u64, u64)> {
    model(c.source)["relations"]
        .as_array()
        .map(|list| list.iter().map(|pair| (pair[0].as_u64().unwrap(), pair[1].as_u64().unwrap())).collect())
        .unwrap_or_default()
}

/// The drawing paths a source can go through, each with what Pele's SVG or the model shows it did.
const PATHS: &[(&str, Reached)] = &[
    ("a UI frame", |c| c.svg.contains("pele-em-ui")),
    ("a processor frame", |c| c.svg.contains("pele-em-processor")),
    ("a command frame", |c| c.svg.contains("pele-em-command")),
    ("a read model frame", |c| c.svg.contains("pele-em-readmodel")),
    ("an event frame", |c| c.svg.contains("pele-em-event")),
    ("a reset frame", |c| c.svg.contains(" pele-em-reset\"")),
    ("the UI lane", |c| c.svg.contains("data-id=\"UI/Automation\"")),
    ("the command and read model lane", |c| c.svg.contains("data-id=\"Command/Read Model\"")),
    ("the events lane", |c| c.svg.contains("data-id=\"Events\"")),
    ("a UI namespace's lane", |c| c.svg.contains("data-id=\"UI/A: ")),
    ("a command namespace's lane", |c| c.svg.contains("data-id=\"C/RM: ")),
    ("a stream namespace's lane", |c| c.svg.contains("data-id=\"Stream: ")),
    ("across, a flow along a lane", |c| {
        !down(c.svg) && flows(c.svg).iter().any(|d| d.contains('L') && numbers(d)[2] > numbers(d)[0])
    }),
    ("across, a flow straight down", |c| {
        !down(c.svg) && flows(c.svg).iter().any(|d| d.contains('V') && numbers(d)[2] > numbers(d)[1])
    }),
    ("across, a flow straight up", |c| {
        !down(c.svg) && flows(c.svg).iter().any(|d| d.contains('V') && numbers(d)[2] < numbers(d)[1])
    }),
    ("across, a flow in an S", |c| flows(c.svg).iter().any(|d| d.contains('C') && numbers(d)[6] > numbers(d)[0])),
    ("a frame that flows into itself, left undrawn", |c| relations(c).iter().any(|(s, t)| s == t)),
    ("data", |c| c.svg.contains("class=\"pele-em-data\"")),
    ("across, data wrapped at 48 characters", |c| {
        !down(c.svg) && rows(c.svg, "pele-em-data").iter().any(|row| row.chars().count() == 48)
    }),
    ("data with an emoji", |c| rows(c.svg, "pele-em-data").iter().any(|row| row.contains('😀'))),
    ("data with an entity code shown as its character", |c| {
        c.source.contains("#35;") && rows(c.svg, "pele-em-data").iter().any(|row| row.contains('#'))
    }),
    ("data from a block", |c| c.source.contains("[[") && c.svg.contains("class=\"pele-em-data\"")),
    ("a data row left out for having no text", |c| {
        model(c.source)["boxes"].as_array().is_some_and(|boxes| {
            boxes.iter().any(|item| item["data"].as_str().is_some_and(|data| data.contains("\n\n")))
        })
    }),
    ("a note", |c| c.svg.contains("class=\"pele-em-note\"")),
    ("a note for a frame with no box", |c| {
        let model = model(c.source);
        let ids: Vec<&str> =
            model["boxes"].as_array().map(|b| b.iter().filter_map(|b| b["id"].as_str()).collect()).unwrap_or_default();
        model["notes"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|n| !ids.contains(&n["frame"].as_str().unwrap())))
    }),
    ("across, a note wrapped at 80 characters", |c| {
        !down(c.svg) && c.svg.contains("pele-em-note") && c.svg.contains("tspan") && {
            let notes: Vec<&str> = c.svg.split("class=\"pele-em-note\"").skip(1).collect();
            notes.iter().any(|note| {
                note.split("<tspan x=").skip(1).any(|span| {
                    span.find('>').and_then(|at| {
                        span[at + 1..].find("</tspan>").map(|end| span[at + 1..at + 1 + end].chars().count())
                    }) == Some(80)
                })
            })
        }
    }),
    ("a specification", |c| c.svg.contains("class=\"pele-em-specification\"")),
    ("a specification with a when", |c| c.svg.contains(">When  ")),
    ("cards with no lanes", |c| c.svg.contains("pele-em-note") && !c.svg.contains("pele-em-lane")),
    ("nothing to draw", |c| {
        !c.svg.contains("pele-em-lane") && !c.svg.contains("pele-em-note") && !c.svg.contains("pele-em-spec")
    }),
    ("a title", |c| c.svg.contains("class=\"pele-title\"")),
    ("an accessible title and description", |c| {
        c.svg.contains("aria-labelledby=") && c.svg.contains("aria-describedby=")
    }),
    ("drawn down", |c| down(c.svg)),
    ("down, a flow between neighbouring cards", |c| down(c.svg) && flows(c.svg).iter().any(|d| !d.contains('H'))),
    ("down, a flow on a rail", |c| down(c.svg) && flows(c.svg).iter().any(|d| d.contains('H'))),
    ("down, flows on two rails", |c| {
        let mut rails: Vec<f64> = flows(c.svg).iter().filter(|d| d.contains('H')).map(|d| numbers(d)[2]).collect();
        rails.sort_by(f64::total_cmp);
        rails.dedup();
        down(c.svg) && rails.len() > 1
    }),
    ("down, a lane heading broken beside an arrow", |c| {
        down(c.svg) && c.svg.contains("</text><text class=\"pele-cluster-label\"")
    }),
    ("down, a card's heading that wraps", |c| {
        down(c.svg)
            && ["pele-em-note", "pele-em-specification"].iter().any(|class| {
                c.svg.split(&format!("class=\"{class}\"")).skip(1).any(|card| {
                    let head = &card[card.find("<text").unwrap_or(0)..];
                    head[..head.find("</text>").unwrap_or(0)].matches("<tspan x=").count() > 1
                })
            })
    }),
    ("down, a card wider than the host", |c| {
        down(c.svg)
            && tags(c.svg, "<svg ")
                .first()
                .and_then(|svg| attribute(svg, "width"))
                .is_some_and(|w| w.parse::<f64>().unwrap() > 320.0)
    }),
];

const EVENTMODELING: Parity = Parity { kind: "eventmodeling", paths: PATHS };

#[test]
fn harvested_sources_match_pele() {
    let tally = EVENTMODELING
        .run("harvested", &plain(harvest("eventmodeling", "sources")))
        .finish("harvested event modeling sources");
    assert!(tally.identical > 500, "the harvest holds models Pele draws");
    assert!(tally.rejected > 100, "the harvest holds models Pele rejects");
}

#[test]
fn harvested_sources_match_pele_at_each_width() {
    let sources = harvest("eventmodeling", "sources");
    for width in WIDTHS {
        let cases: Vec<(String, Value)> =
            sources.iter().map(|source| (source.clone(), json!({ "maxWidth": width }))).collect();
        EVENTMODELING.run("harvested", &cases).finish(&format!("harvested event modeling sources at maxWidth {width}"));
        let redrawn = redrawn_to_fit(&cases);
        println!("maxWidth {width} makes Pele draw {redrawn} of the harvested sources another way");
    }
}

#[test]
fn mutated_sources_match_pele() {
    EVENTMODELING.run("mutated", &plain(harvest("eventmodeling", "mutated"))).finish("mutated event modeling sources");
}

/// Host options the harvest does not set, and Pele's worst cases for event modeling
/// (`tests/eventmodeling/stress.test.ts`) with the source limit lifted as that test lifts it.
fn cases() -> Vec<(String, Value)> {
    const N: usize = 50_000;
    let head = "eventmodeling\n";
    let id = |i: usize| format!("{:03}", i % 1000);
    let kinds = ["ui", "cmd", "evt", "rmo", "pcr"];
    let repeat = |count: usize, line: &dyn Fn(usize) -> String| (0..count).map(line).collect::<String>();
    let shop = "eventmodeling title Shop\ntf 01 ui CartUI\ntf 02 cmd AddItem { productId: 7 }\ntf 03 evt ItemAdded ->> 01\n\
        tf 04 rmo CartView ->> 02\nrf 05 pcr Cleanup ->> 03 ->> 02\nnote 02 {\n  a note\n}\ngwt 03 given evt ItemAdded then rmo CartView\n";
    let mut out = vec![
        (shop.to_owned(), json!({})),
        (shop.to_owned(), json!({ "fontSize": 12 })),
        (shop.to_owned(), json!({ "fontSize": 21, "padding": 0 })),
        (shop.to_owned(), json!({ "maxWidth": 320, "fontSize": 20 })),
        (shop.to_owned(), json!({ "maxWidth": 100 })),
        (shop.to_owned(), json!({ "maxWidth": 320, "padding": 40 })),
        (shop.to_owned(), json!({ "maxWidth": 320, "autoDirection": false })),
        (shop.to_owned(), json!({ "maxWidth": 700, "directionBreakpoint": 1000 })),
        (shop.to_owned(), json!({ "maxWidth": 300, "directionBreakpoint": 250 })),
        (shop.to_owned(), json!({ "idPrefix": "x\"y", "responsive": false })),
        (shop.to_owned(), json!({ "config": { "eventmodeling": { "useMaxWidth": false } } })),
        (shop.to_owned(), json!({ "outputLimit": 100 })),
        (shop.to_owned(), json!({ "limit": 10 })),
    ];
    for source in [
        repeat(1000, &|i| format!("tf {} {} E{i}\n", id(i), kinds[i % 5])),
        repeat(1000, &|i| format!("tf {} evt E{i}\n", id(i))),
        repeat(1000, &|i| format!("tf {} evt N{i}.E\n", id(i))),
        repeat(1000, &|i| {
            format!("tf {} evt E{i}{}\n", id(i), repeat(i.min(9), &|k| format!(" ->> {}", id(i - k - 1))))
        }),
        format!("tf 001 cmd A\ntf 002 evt B{}\n", " ->> 001".repeat(9000)),
        "tf 01 evt A\n".repeat(5000),
        repeat(500, &|i| format!("tf {} evt E{i} [[D{i}]]\n", id(i)))
            + &repeat(500, &|i| format!("data D{i} {{\n  a: {i}\n  b: true\n}}\n")),
        format!("tf 01 evt A [[D]]\ndata D {{\n{}}}\n", "a: 1\n".repeat(10000)),
        format!("tf 01 evt A {{ {} }}\n", "x".repeat(N)),
        format!("tf 01 evt A {}{}\n", "{".repeat(N / 2), "}".repeat(N / 2)),
        "tf 01 evt A\n".to_owned()
            + &repeat(1000, &|i| format!("note 01 {{\n n{i}\n}}\ngwt 01 given evt A then evt B\n")),
        format!("tf 01 evt A\ngwt 01 given{} then{}\n", " evt A".repeat(5000), " evt B".repeat(5000)),
        format!("tf 01 evt {}z\n", "a.".repeat(10000)),
        format!("tf 01 evt {}\n", "a".repeat(N)),
    ] {
        out.push((format!("{head}{source}"), json!({ "limit": 1_000_000_000u64 })));
    }
    out.push((format!("{head}tf 01 ui A\ntf 02 cmd {}\n", "B".repeat(400)), json!({ "maxWidth": 320 })));
    out.push((
        format!(
            "{head}{}",
            repeat(40, &|i| format!(
                "tf {} {} E{i}{}\n",
                id(i),
                kinds[i % 5],
                if i > 3 { format!(" ->> {}", id(i - 3)) } else { String::new() }
            ))
        ),
        json!({ "maxWidth": 320 }),
    ));
    out
}

#[test]
fn cases_match_pele() {
    EVENTMODELING.run("case", &cases()).finish("event modeling cases");
}

#[test]
fn random_models_match_pele() {
    let mut rng = random::Rng(0x5eed_e7e7_0042);
    let mut cases = Vec::new();
    for _ in 0..4000 {
        let source = random::source(&mut rng);
        let options = match rng.below(8) {
            0 | 1 => json!({ "maxWidth": *rng.pick(&[200.0, 320.0, 480.0, 600.0]) }),
            2 => json!({ "maxWidth": 320, "fontSize": *rng.pick(&[12.0, 14.0, 20.0]) }),
            3 => json!({ "fontSize": *rng.pick(&[11.0, 13.5, 18.0]), "padding": *rng.pick(&[0.0, 4.0, 16.0]) }),
            4 => json!({ "maxWidth": *rng.pick(&[900.0, 1600.0]) }),
            _ => json!({}),
        };
        cases.push((source, options));
    }
    let tally = EVENTMODELING.run("random", &cases).finish("random event models");
    let widths: Vec<(String, Value)> =
        cases.iter().filter(|(_, options)| options.get("maxWidth").is_some()).cloned().collect();
    println!("their widths make Pele draw {} of the random event models another way", redrawn_to_fit(&widths));
    let missed = EVENTMODELING.missed(&tally);
    println!("reach {} of {} named drawing paths", PATHS.len() - missed.len(), PATHS.len());
    assert!(missed.is_empty(), "no random event model reaches {missed:?}");
}
