//! Seeded random radar charts for `radar_front.rs` and `radar_parity.rs`: axes with and without
//! labels, curves of bare values and of values by axis, every option in and out of range, the
//! chart's own config and palette, and now and then a fragment that the grammar rejects.

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

const AXES: [&str; 12] =
    ["speed", "power", "a", "b-c", "x_1", "range", "Accuracy", "titles", "max-out", "circles", "trueNorth", "z9"];
const LABELS: [&str; 10] = [
    "Speed",
    "A much longer axis label",
    "",
    "#35; code &amp; more",
    "<b>bold</b> label",
    "two<br>lines",
    "é ü",
    "中文",
    "x \\\"quoted\\\"",
    "Power",
];
const CURVES: [&str; 6] = ["alpha", "beta", "c1", "team-a", "d", "e"];
const FRAGMENTS: [&str; 18] = [
    "{", "}", "[", "]", ":", ",", "\"", "axis", "curve", "ticks", "max", "1.2.3", "true", "polygon", "\n", " ", "-",
    "title",
];

fn value(rng: &mut Rng) -> String {
    match rng.below(12) {
        0 => "0".to_owned(),
        1 => format!("{}.{}", rng.below(100), rng.below(100)),
        2 => format!("{}", 100 + rng.below(1000)),
        3 => "99999999999999999999".to_owned(),
        4 if rng.chance(10) => "9".repeat(400),
        _ => format!("{}", rng.below(11)),
    }
}

fn config(rng: &mut Rng) -> String {
    let mut lines = Vec::new();
    for (key, values) in [
        ("width", &["200", "480", "0", "5000", "\"x\""][..]),
        ("height", &["160", "600", "1"][..]),
        ("marginLeft", &["0", "40", "3000"][..]),
        ("marginRight", &["0", "60"][..]),
        ("marginTop", &["0", "30"][..]),
        ("marginBottom", &["0", "30"][..]),
        ("axisScaleFactor", &["0.5", "2", "11"][..]),
        ("axisLabelFactor", &["0.8", "1.5", "-1"][..]),
        ("curveTension", &["0", "0.5", "1", "2"][..]),
    ] {
        if rng.chance(10) {
            lines.push(format!("  radar:\n    {key}: {}", rng.pick(values)));
        }
    }
    let mut out = String::new();
    if !lines.is_empty() {
        // Each line restates the section; YAML keeps the last, so give each its own config.
        out.push_str(&format!("---\nconfig:\n{}\n---\n", lines[rng.below(lines.len())]));
    }
    if rng.chance(10) {
        let colours = ["#ff0000", "rebeccapurple", "rgb(1, 2, 3)", "notacolor"];
        out.push_str(&format!(
            "%%{{init: {{\"themeVariables\": {{\"cScale0\": \"{}\", \"cScale2\": \"{}\"}}}}}}%%\n",
            rng.pick(&colours),
            rng.pick(&colours)
        ));
    }
    out
}

/// A random radar chart, mostly one Pele draws.
pub fn source(rng: &mut Rng) -> String {
    let mut out = config(rng);
    out.push_str(rng.pick(&["radar-beta\n", "radar-beta\n", "radar-beta:\n", "radar-beta :\n"]));
    if rng.chance(20) {
        out.push_str(&format!(
            "title {}\n",
            rng.pick(&["Skills", "A <i>chart</i>", "A chart title long enough to wrap when the host is narrow"])
        ));
    }
    if rng.chance(10) {
        out.push_str("accTitle: Radar\naccDescr {\n  values\n   by axis\n}\n");
    }
    let count = rng.below(9);
    let mut axes: Vec<String> = Vec::new();
    for _ in 0..count {
        let name =
            if !axes.is_empty() && rng.chance(4) { rng.pick(&axes).clone() } else { rng.pick(&AXES).to_string() };
        axes.push(name);
    }
    if !axes.is_empty() {
        let written: Vec<String> = axes
            .iter()
            .map(|name| if rng.chance(30) { format!("{name}[\"{}\"]", rng.pick(&LABELS)) } else { name.clone() })
            .collect();
        let split = rng.below(written.len()) + 1;
        out.push_str(&format!("axis {}\n", written[..split].join(", ")));
        if split < written.len() {
            out.push_str(&format!("axis {}\n", written[split..].join(rng.pick(&[", ", ","]))));
        }
    }
    for index in 0..rng.below(11) {
        let name = rng.pick(&CURVES).to_string();
        let label = if rng.chance(30) { format!("[\"{}\"]", rng.pick(&LABELS)) } else { String::new() };
        let length = if axes.is_empty() || rng.chance(10) { 1 + rng.below(6) } else { axes.len() };
        let separator = *rng.pick(&[", ", ",", ",\n  "]);
        let entries: Vec<String> = if !axes.is_empty() && rng.chance(30) {
            let mut named: Vec<String> =
                axes.iter().map(|axis| format!("{axis}{} {}", rng.pick(&[":", ""]), value(rng))).collect();
            if rng.chance(15) {
                named.pop();
            }
            if rng.chance(30) {
                named.reverse();
            }
            named
        } else {
            (0..length).map(|_| value(rng)).collect()
        };
        if entries.is_empty() {
            continue;
        }
        let open = if rng.chance(15) { "{\n  " } else { "{" };
        out.push_str(&format!("curve {name}{index}{label}{open}{}}}\n", entries.join(separator)));
    }
    for _ in 0..rng.below(4) {
        let option = match rng.below(6) {
            0 => format!("showLegend {}", rng.pick(&["true", "false"])),
            1 => format!("ticks {}", rng.pick(&["0", "1", "3", "2.5", "10", "40"])),
            2 => format!("max {}", rng.pick(&["1", "5", "10", "100.5", "0"])),
            3 => format!("min {}", rng.pick(&["0", "1", "5", "10"])),
            _ => format!("graticule {}", rng.pick(&["circle", "polygon"])),
        };
        out.push_str(&option);
        out.push_str(if rng.chance(20) { ", " } else { "\n" });
    }
    if out.ends_with(", ") {
        out.push_str("ticks 4\n");
    }
    if rng.chance(6) {
        let at = rng.below(out.len() + 1);
        let at = (0..=at).rev().find(|&i| out.is_char_boundary(i)).unwrap_or(0);
        out.insert_str(at, rng.pick(&FRAGMENTS));
    }
    out
}
