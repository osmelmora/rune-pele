//! Seeded random XY charts for the front-end and render differential tests. Each chart mixes the
//! statements Pele's grammar takes, a host config that turns parts of the chart on and off, and
//! values on the edges of what Pele draws.

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

const WORDS: &[&str] = &[
    "Revenue",
    "\"Sales Revenue\"",
    "\"in $ (thousands)\"",
    "Months",
    "a-b_c",
    "\"<b>bold</b> title\"",
    "\"#35; &amp; more\"",
    "\"a long axis title that wraps when the chart is narrow enough to need it\"",
    "x",
];

const CATEGORIES: &[&str] = &[
    "jan",
    "feb",
    "\"mar\"",
    "Q1",
    "\"Quarter two\"",
    "\"a category name long enough to wrap\"",
    "\"<i>slanted</i>\"",
    "b",
    "\"Line<br>break\"",
];

fn value(rng: &mut Rng) -> String {
    match rng.below(14) {
        0 => format!("-{}", rng.below(100)),
        1 => format!("{}.{}", rng.below(1000), rng.below(100)),
        2 => format!("-{}.{}", rng.below(50), rng.below(10)),
        3 => format!("+{}", rng.below(20)),
        4 => format!(".{}", rng.below(99) + 1),
        5 => "0".to_owned(),
        6 => format!("{}", rng.below(5_000_000)),
        7 => format!("0.000{}", rng.below(9) + 1),
        _ => format!("{}", rng.below(120)),
    }
}

fn config(rng: &mut Rng) -> String {
    let mut lines = Vec::new();
    let flag = |rng: &mut Rng| if rng.chance(2) { "true" } else { "false" };
    if rng.chance(3) {
        lines.push(format!("    width: {}", rng.pick(&["300", "500", "900", "1200", "50", "4500"])));
    }
    if rng.chance(3) {
        lines.push(format!("    height: {}", rng.pick(&["200", "400", "700", "80"])));
    }
    if rng.chance(3) {
        lines.push("    showDataLabel: true".to_owned());
    }
    if rng.chance(6) {
        lines.push(format!("    showTitle: {}", flag(rng)));
    }
    if rng.chance(6) {
        lines.push(format!("    showLegend: {}", flag(rng)));
    }
    if rng.chance(6) {
        lines.push(format!("    chartOrientation: {}", rng.pick(&["horizontal", "vertical", "Horizontal"])));
    }
    for axis in ["xAxis", "yAxis"] {
        if rng.chance(5) {
            lines.push(format!("    {axis}:"));
            for key in ["showLabel", "showTick", "showTitle", "showAxisLine"] {
                if rng.chance(2) {
                    lines.push(format!("      {key}: {}", flag(rng)));
                }
            }
        }
    }
    let palette = if rng.chance(5) {
        "  themeVariables:\n    xyChart:\n      plotColorPalette: \"#ff0000, rgb(0, 0, 255), navy\"\n"
    } else {
        ""
    };
    if lines.is_empty() && palette.is_empty() {
        return String::new();
    }
    let chart = if lines.is_empty() { String::new() } else { format!("  xyChart:\n{}\n", lines.join("\n")) };
    format!("---\nconfig:\n{chart}{palette}---\n")
}

/// One random XY chart.
pub fn source(rng: &mut Rng) -> String {
    let mut out = String::new();
    if rng.chance(3) {
        out.push_str(&config(rng));
    } else if rng.chance(10) {
        out.push_str("---\ntitle: Front matter title\n---\n");
    }
    out.push_str(rng.pick(&["xychart", "xychart-beta", "xychart"]));
    if rng.chance(4) {
        out.push(' ');
        out.push_str(rng.pick(&["horizontal", "vertical", "horizontal", "Horizontal"]));
    }
    out.push('\n');
    let separator = if rng.chance(12) { ";" } else { "\n" };
    let mut statements = Vec::new();
    if rng.chance(2) {
        statements.push(format!("title {}", rng.pick(WORDS)));
    }
    if rng.chance(8) {
        statements.push("accTitle: The chart".to_owned());
        statements.push(if rng.chance(2) {
            "accDescr: one line".to_owned()
        } else {
            "accDescr {\n  two\n   lines\n}".to_owned()
        });
    }
    let categories = 1 + rng.below(9);
    match rng.below(5) {
        0 | 1 => {
            let names: Vec<&str> = (0..categories).map(|_| rng.pick(CATEGORIES)).collect();
            let title = if rng.chance(2) { format!("{} ", rng.pick(WORDS)) } else { String::new() };
            statements.push(format!("x-axis {title}[{}]", names.join(", ")));
        }
        2 => {
            let title = if rng.chance(2) { format!("{} ", rng.pick(WORDS)) } else { String::new() };
            let (low, high) = (value(rng), value(rng));
            statements.push(format!("x-axis {title}{low} --> {high}"));
        }
        3 => statements.push(format!("x-axis {}", rng.pick(WORDS))),
        _ => {}
    }
    match rng.below(4) {
        0 => {
            let title = if rng.chance(2) { format!("{} ", rng.pick(WORDS)) } else { String::new() };
            let (low, high) = (value(rng), value(rng));
            statements.push(format!("y-axis {title}{low} --> {high}"));
        }
        1 => statements.push(format!("y-axis {}", rng.pick(WORDS))),
        _ => {}
    }
    let series = if rng.chance(30) { 0 } else { 1 + rng.below(4) };
    for _ in 0..series {
        let kind = if rng.chance(2) { "bar" } else { "line" };
        let title = if rng.chance(2) { format!("{} ", rng.pick(WORDS)) } else { String::new() };
        let most = if rng.chance(6) { 30 } else { categories + 2 };
        let count = 1 + rng.below(most);
        let values: Vec<String> = (0..count)
            .map(|_| {
                let mut point = value(rng);
                if rng.chance(5) {
                    point.push_str(&format!(" {}", rng.pick(&["\"peak\"", "\"low point\"", "\"\"", "\"<b>x</b>\""])));
                }
                point
            })
            .collect();
        statements.push(format!("{kind} {title}[{}]", values.join(", ")));
    }
    if rng.chance(20) {
        statements.push(
            rng.pick(&["x-axis [", "line [1, 2", "bar \"open", "y-axis 1 -> 2", "%% a comment", "nonsense"]).to_owned(),
        );
    }
    out.push_str(&statements.join(separator));
    out
}
