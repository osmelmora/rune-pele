//! Seeded random sankey diagrams for `sankey_front.rs`, `sankey_layout.rs` and `sankey_parity.rs`:
//! CSV rows with bare and quoted fields, escaped quotes, blank fields, blank lines, byte order
//! marks and both line ends; flows in chains, fans, meshes and cycles; and every setting of the
//! `sankey` config that changes the drawing.

use serde_json::{Value, json};

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
    pub fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

const NAMES: &[&str] = &[
    "Coal",
    "Electricity grid",
    "Losses",
    "Homes",
    "Industry",
    "Bio-conversion",
    "Solar",
    "a",
    "b",
    "Doход \u{1f3e0}",
    "\u{53ce}\u{5165}",
    "#35; & co",
    "&amp;lt;x",
    "with \"quotes\"",
    "comma, inside",
    "x",
    " padded ",
    "Thermal generation",
];

fn field(rng: &mut Rng, name: &str) -> String {
    // An empty field is no field to Mermaid's grammar, so an empty name is written blank.
    if name.is_empty() {
        return rng.pick(&[" ", "\" \"", "  "]).to_string();
    }
    let needs_quotes = name.contains([',', '"']);
    if needs_quotes || rng.chance(20) { format!("\"{}\"", name.replace('"', "\"\"")) } else { name.to_owned() }
}

fn value(rng: &mut Rng) -> String {
    match rng.below(20) {
        0 => "0".to_owned(),
        1 => "-3".to_owned(),
        2 => "1e3".to_owned(),
        3 => "12.5%".to_owned(),
        4 => "0.001".to_owned(),
        5 => "\"7\"".to_owned(),
        6 => "1e308".to_owned(),
        7 => "999999999999999.5".to_owned(),
        8 => format!("{}.{}", rng.below(100), rng.below(1000)),
        _ => (1 + rng.below(400)).to_string(),
    }
}

/// The rows of one random graph: names as they are written, then rows that join them.
fn rows(rng: &mut Rng) -> Vec<(String, String, String)> {
    let count = 2 + rng.below(9);
    let names: Vec<String> = (0..count)
        .map(|i| match rng.below(20) {
            0 => String::new(),
            1..=6 => rng.pick(NAMES).to_string(),
            _ => format!("{}{i}", rng.pick(NAMES)),
        })
        .collect();
    let mut out = Vec::new();
    let shape = *rng.pick(&[0, 0, 1, 1, 2, 2, 3, 4]);
    let links = 1 + rng.below(14);
    for k in 0..links {
        let (from, to) = match shape {
            // A chain, sometimes skipping ahead.
            0 => {
                let from = k % (count - 1);
                (from, (from + 1 + rng.below(2)).min(count - 1))
            }
            // A fan out of one node and into another.
            1 => (0, 1 + rng.below(count - 1)),
            // A mesh that only runs forward.
            2 | 3 => {
                let from = rng.below(count - 1);
                (from, from + 1 + rng.below(count - 1 - from))
            }
            // Anything, cycles included.
            _ => (rng.below(count), rng.below(count)),
        };
        out.push((names[from].clone(), names[to].clone(), value(rng)));
    }
    out
}

/// One random sankey diagram.
pub fn random_sankey(rng: &mut Rng) -> String {
    let mut config = Vec::new();
    let settings: &[(&str, &[&str])] = &[
        ("width", &["300", "900", "100", "1"]),
        ("height", &["150", "600", "40"]),
        ("nodeWidth", &["24", "1", "200"]),
        ("nodePadding", &["0", "40"]),
        ("showValues", &["false", "true"]),
        ("prefix", &["\"$\"", "\"&euro;\"", "3"]),
        ("suffix", &["\" M\"", "\" &amp;lt;\""]),
        ("labelStyle", &["outlined", "legacy"]),
        ("nodeAlignment", &["left", "right", "center", "justify", "x"]),
        ("linkColor", &["source", "target", "gradient", "\"#a1a1a1\"", "\"url(#x)\"", "\"red;blue\""]),
    ];
    for (key, values) in settings {
        if rng.chance(15) {
            config.push(format!("    {key}: {}", rng.pick(values)));
        }
    }
    let rows = rows(rng);
    if rng.chance(15) {
        config.push("    nodeColors:".to_owned());
        for (from, to, _) in rows.iter().take(3) {
            let name = if rng.chance(50) { from } else { to };
            let colour = rng.pick(&["\"#123456\"", "red", "\"url(#x)\"", "\"red&blue\"", "'x\"y'", "\"rgb(1, 2, 3)\""]);
            config.push(format!("      {}: {colour}", serde_json::to_string(name.trim()).expect("a name encodes")));
        }
    }
    let mut out = String::new();
    if !config.is_empty() || rng.chance(10) {
        out.push_str("---\n");
        if rng.chance(30) {
            out.push_str(
                rng.pick(&["title: Energy flows\n", "title: A title much wider than any chart could be at all\n"]),
            );
        }
        if !config.is_empty() {
            out.push_str("config:\n  sankey:\n");
            for line in &config {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("---\n");
    }
    out.push_str(if rng.chance(2) { "SANKEY" } else { rng.pick(&["sankey", "sankey-beta", "sankey "]) });
    let end = if rng.chance(10) { "\r\n" } else { "\n" };
    out.push_str(end);
    if rng.chance(20) {
        out.push_str(end);
    }
    for (index, (from, to, value)) in rows.iter().enumerate() {
        if rng.chance(5) {
            out.push('\u{feff}');
        }
        if rng.chance(8) {
            out.push_str("  ");
        }
        out.push_str(&format!("{},{},{value}", field(rng, from), field(rng, to)));
        if rng.chance(8) {
            out.push_str(" \t");
        }
        if index + 1 < rows.len() || rng.chance(50) {
            out.push_str(end);
        }
        if rng.chance(8) {
            out.push_str(rng.pick(&["\n", "%% a comment\n", "\r\n"]));
        }
    }
    out
}

/// Render options a host may give: a width the chart may be narrowed to, other sizes, and an id
/// prefix, which decides whether links are drawn with gradients.
pub fn random_options(rng: &mut Rng) -> Value {
    let mut options = serde_json::Map::new();
    if rng.chance(40) {
        options.insert("maxWidth".to_owned(), json!(*rng.pick(&[120.0, 240.0, 320.0, 480.0, 640.0, 900.0])));
    }
    if rng.chance(10) {
        options.insert("fontSize".to_owned(), json!(*rng.pick(&[12.0, 13.5, 20.0])));
    }
    if rng.chance(10) {
        options.insert("padding".to_owned(), json!(*rng.pick(&[0.0, 20.5])));
    }
    if rng.chance(35) {
        options.insert("idPrefix".to_owned(), json!(*rng.pick(&["p1", "p_1", "a-b", "p 1", "p\"1", ""])));
    }
    Value::Object(options)
}

/// Random flows for the layout on its own: their links as indices and the bits of each value.
pub fn random_flows(rng: &mut Rng) -> (usize, Vec<(usize, usize, f64)>) {
    let nodes = 1 + rng.below(14);
    let count = rng.below(30);
    let cyclic = rng.chance(8);
    let mut links = Vec::with_capacity(count);
    for _ in 0..count {
        let (from, to) = if cyclic || nodes == 1 {
            (rng.below(nodes), rng.below(nodes))
        } else {
            let from = rng.below(nodes - 1);
            (from, from + 1 + rng.below(nodes - 1 - from))
        };
        let value = match rng.below(16) {
            0 => 0.0,
            1 => -2.0,
            2 => f64::NAN,
            3 => f64::INFINITY,
            4 => 1e308,
            5 => 5e-324,
            6 => 0.1 + 0.2,
            _ => (1 + rng.below(1000)) as f64 / (1 + rng.below(7)) as f64,
        };
        links.push((from, to, value));
    }
    (nodes, links)
}
