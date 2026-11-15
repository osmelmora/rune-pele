//! Seeded random wardley maps for the front-end and render differential tests. Each map mixes
//! the statements Pele's grammar takes with text that wraps, carries markup or entity codes, and
//! numbers on the edges of what Pele accepts.

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    pub fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    pub fn pick<'a>(&mut self, choices: &[&'a str]) -> &'a str {
        choices[self.below(choices.len())]
    }
}

const NAMES: &[&str] = &[
    "Tea",
    "Cup of Tea",
    "Water",
    "Kettle",
    "Power",
    "Business",
    "Public",
    "Hot Water",
    "Electric Kettle",
    "A",
    "B",
    "C",
    "x-ray",
    "R&D",
    "Ops (core)",
    "Data_Store",
    "Customer Experience Platform Services",
];

const QUOTED: &[&str] = &[
    "\"Tea\"",
    "\"<b>Bold</b> tea\"",
    "\"<i>slanted</i> and <b>bold</b>\"",
    "\"Line<br>Break\"",
    "\"two<br><br>gaps\"",
    "\"#35; hash &amp; more\"",
    "\"a long name that wraps when the map is narrow enough to need it\"",
    "\"\"",
    "'single'",
    "\"<code>mono</code> text\"",
    "\"<b>all bold</b>\"",
    "\"<sub>low</sub> and <sup>high</sup>\"",
];

fn fraction(rng: &mut Rng) -> String {
    match rng.below(12) {
        0 => "0.0".to_owned(),
        1 => "1.0".to_owned(),
        2 => format!("{}.{}", rng.below(101), rng.below(10)),
        3 => format!("0.{:03}", rng.below(1000)),
        _ => format!("0.{:02}", rng.below(100)),
    }
}

fn name(rng: &mut Rng, known: &[String]) -> String {
    if !known.is_empty() && rng.chance(2) {
        return known[rng.below(known.len())].clone();
    }
    if rng.chance(4) { rng.pick(QUOTED).to_owned() } else { rng.pick(NAMES).to_owned() }
}

fn int(rng: &mut Rng) -> String {
    match rng.below(8) {
        0 => "0".to_owned(),
        1 => "9".repeat(rng.below(30) + 1),
        _ => (rng.below(200) + 1).to_string(),
    }
}

fn label_offset(rng: &mut Rng) -> String {
    let sign = |rng: &mut Rng| if rng.chance(2) { "-" } else { "" };
    let (x, y) = (int(rng), int(rng));
    format!(" label [{}{x}, {}{y}]", sign(rng), sign(rng))
}

fn arrow(rng: &mut Rng) -> String {
    let arrows = ["->", "-->", "-.->", ">", "+>", "+<", "+<>", " -> ", "+'flow'>", "+'back'<", "+'both'<>", "+''>"];
    rng.pick(&arrows).to_owned()
}

/// One random wardley map.
pub fn source(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(8) {
        let grid = if rng.chance(2) { "    showGrid: true\n" } else { "" };
        let width = if rng.chance(2) { format!("    width: {}\n", rng.below(1400) + 100) } else { String::new() };
        let height = if rng.chance(3) { format!("    height: {}\n", rng.below(1200) + 100) } else { String::new() };
        let title = if rng.chance(3) { "title: Front matter title\n" } else { "" };
        out.push_str(&format!("---\n{title}config:\n  wardley-beta:\n{grid}{width}{height}---\n"));
    }
    out.push_str("wardley-beta\n");
    if rng.chance(2) {
        let titles =
            ["Tea Shop", "A map with a title wide enough to wrap at narrow widths", "<b>Bold</b> title", "#35;1"];
        out.push_str(&format!("title {}\n", rng.pick(&titles)));
    }
    if rng.chance(6) {
        out.push_str("accTitle: The map\naccDescr {\n  described\n   over lines\n}\n");
    }
    if rng.chance(5) {
        out.push_str(&format!("size [{}, {}]\n", int(rng), int(rng)));
    }
    if rng.chance(3) {
        let count = 2 + rng.below(4);
        let stages: Vec<String> = (0..count)
            .map(|i| {
                let mut stage = if rng.chance(4) { rng.pick(QUOTED).to_owned() } else { format!("Stage{i}") };
                if rng.chance(2) {
                    stage.push_str(&format!("@{}", fraction(rng)));
                }
                if rng.chance(4) {
                    stage.push_str(&format!(" / {}", rng.pick(&["Concept", "\"Second name\"", "x"])));
                }
                stage
            })
            .collect();
        out.push_str(&format!("evolution {}\n", stages.join(" -> ")));
    }
    let mut known: Vec<String> = Vec::new();
    for _ in 0..rng.below(3) {
        let anchor = name(rng, &[]);
        out.push_str(&format!("anchor {anchor} [{}, {}]\n", fraction(rng), fraction(rng)));
        known.push(anchor);
    }
    for _ in 0..rng.below(9) {
        let component = name(rng, &[]);
        out.push_str(&format!("component {component} [{}, {}]", fraction(rng), fraction(rng)));
        if rng.chance(4) {
            out.push_str(&label_offset(rng));
        }
        if rng.chance(4) {
            out.push_str(&format!(" ({})", rng.pick(&["build", "buy", "outsource", "market"])));
        }
        if rng.chance(5) {
            out.push_str(if rng.chance(2) { " inertia" } else { " (inertia)" });
        }
        out.push('\n');
        known.push(component);
    }
    let parents = known.clone();
    let pipelines = if parents.is_empty() || !rng.chance(3) { 0 } else { 1 + rng.below(2) };
    for _ in 0..pipelines {
        let parent = parents[rng.below(parents.len())].clone();
        out.push_str(&format!("pipeline {parent} {{\n"));
        for i in 0..1 + rng.below(3) {
            let member =
                if rng.chance(3) { rng.pick(QUOTED).to_owned() } else { format!("Part {}", ["A", "B", "C"][i]) };
            out.push_str(&format!("  component {member} [{}]", fraction(rng)));
            if rng.chance(4) {
                out.push_str(&label_offset(rng));
            }
            out.push('\n');
            known.push(member);
        }
        out.push_str("}\n");
    }
    for _ in 0..rng.below(8) {
        let from = name(rng, &known);
        let to = name(rng, &known);
        let from_port = if rng.chance(8) { "+<>" } else { "" };
        let to_port = if rng.chance(8) { rng.pick(&["+>", "+<"]) } else { "" };
        let label =
            if rng.chance(4) { rng.pick(&["; label", "; a longer label on the link", "; <b>bold</b>"]) } else { "" };
        let mut arrow = arrow(rng);
        if !from_port.is_empty() && arrow.starts_with("+") && !arrow.starts_with("+'") {
            arrow = "->".to_owned();
        }
        out.push_str(&format!("{from}{from_port} {arrow} {to}{to_port}{label}\n"));
    }
    for _ in 0..rng.below(3) {
        out.push_str(&format!("evolve {} {}\n", name(rng, &known), fraction(rng)));
    }
    for _ in 0..rng.below(3) {
        out.push_str(&format!("note {} [{}, {}]\n", rng.pick(QUOTED), fraction(rng), fraction(rng)));
    }
    if rng.chance(3) {
        out.push_str(&format!("annotations [{}, {}]\n", fraction(rng), fraction(rng)));
    }
    for _ in 0..rng.below(4) {
        let number = rng.below(12);
        let text = if rng.chance(5) { "\"\"" } else { rng.pick(QUOTED) };
        out.push_str(&format!("annotation {number},[{}, {}] {text}\n", fraction(rng), fraction(rng)));
    }
    for _ in 0..rng.below(3) {
        let kind = if rng.chance(2) { "accelerator" } else { "deaccelerator" };
        out.push_str(&format!("{kind} {} [{}, {}]\n", name(rng, &[]), fraction(rng), fraction(rng)));
    }
    if rng.chance(25) {
        out.push_str(rng.pick(&[
            "component Broken [150.5, 0.5]\n",
            "pipeline Nowhere {\n component X [0.5]\n}\n",
            "A B\n",
        ]));
    }
    out
}
