//! Seeded random gantt charts for `gantt_front.rs` and `gantt_parity.rs`: every statement the
//! grammar has, tasks that refer to each other forwards, backwards, in cycles and to nothing,
//! dates in the chart's format and out of it, and the settings that change scheduling and drawing.

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

/// A date format and how to write a day and a time of day in it.
struct Format {
    template: &'static str,
    write: fn(i64, u32, u32, u32, u32) -> String,
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn epoch_ms(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    ((era * 146_097 + doe - 719_468) * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60) * 1000
}

const FORMATS: &[Format] = &[
    Format { template: "YYYY-MM-DD", write: |y, m, d, _, _| format!("{y:04}-{m:02}-{d:02}") },
    Format { template: "YYYY-MM-DD HH:mm", write: |y, m, d, h, mi| format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}") },
    Format { template: "DD.MM.YYYY", write: |y, m, d, _, _| format!("{d:02}.{m:02}.{y:04}") },
    Format { template: "x", write: |y, m, d, h, mi| epoch_ms(y, m, d, h, mi).to_string() },
    Format { template: "X", write: |y, m, d, h, mi| (epoch_ms(y, m, d, h, mi) / 1000).to_string() },
    Format { template: "HH:mm", write: |_, _, _, h, mi| format!("{h:02}:{mi:02}") },
    Format { template: "MMM D, YYYY", write: |y, m, d, _, _| format!("{} {d}, {y}", MONTHS[m as usize - 1]) },
    Format { template: "YYYYMMDD", write: |y, m, d, _, _| format!("{y:04}{m:02}{d:02}") },
    Format { template: "", write: |y, m, d, _, _| format!("{y:04}-{m:02}-{d:02}") },
];

fn days_in(year: i64, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A day of a chart: mostly near the parity clock's January 2026, sometimes around a leap day,
/// a month end or far away.
fn day(rng: &mut Rng) -> (i64, u32, u32) {
    let year = match rng.below(10) {
        0 => *rng.pick(&[2024, 2000, 1900, 2100, 1999]),
        1 => 1970 + rng.below(80) as i64,
        _ => 2025 + rng.below(3) as i64,
    };
    let month = 1 + rng.below(12) as u32;
    let last = days_in(year, month);
    let day = if rng.chance(25) { last - rng.below(2) as u32 } else { 1 + rng.below(last as usize) as u32 };
    (year, month, day)
}

const WORDS: &[&str] = &[
    "Design",
    "Build",
    "Test",
    "Ship",
    "Research",
    "A task",
    "Another task",
    "Review <br> notes",
    "**bold** work",
    "A very long task name that will not fit inside its bar at all",
    "Café",
    "日本語",
    "x",
    "Q&A",
    "#35; hash",
];

const IDS: &[&str] = &["a1", "a2", "b1", "des1", "t", "x-y", "z_9", "m1", "v1", "crit1"];

const UNITS: &[&str] = &["d", "w", "h", "m", "s", "ms", "M", "y"];

fn duration(rng: &mut Rng) -> String {
    let unit = *rng.pick(UNITS);
    let amount = match unit {
        "ms" | "s" | "m" => rng.below(5000),
        "h" => rng.below(300),
        "M" | "y" => rng.below(4),
        _ => rng.below(40),
    };
    if rng.chance(10) { format!("{amount}.5{unit}") } else { format!("{amount}{unit}") }
}

/// A random gantt chart as source text.
pub fn random_gantt(rng: &mut Rng) -> String {
    let format = rng.pick(FORMATS);
    let mut out = String::new();
    if rng.chance(15) {
        out.push_str("---\n");
        if rng.chance(50) {
            out.push_str("title: Front matter title\n");
        }
        if rng.chance(70) {
            out.push_str("displayMode: compact\n");
        }
        out.push_str("---\n");
    }
    out.push_str("gantt\n");
    if !format.template.is_empty() || rng.chance(20) {
        out.push_str(&format!(
            "    dateFormat {}\n",
            if format.template.is_empty() { "YYYY-MM-DD" } else { format.template }
        ));
    }
    let write = |rng: &mut Rng| {
        let (y, m, d) = day(rng);
        let (h, mi) = (rng.below(24) as u32, rng.below(60) as u32);
        (format.write)(y, m, d, h, mi)
    };
    let mut header: Vec<String> = Vec::new();
    if rng.chance(30) {
        header.push(format!(
            "axisFormat {}",
            rng.pick(&["%Y-%m-%d", "%b %e", "%d/%m", "%H:%M", "%a %d", "%W", "%j", "%Y %U %V", "week %V", "%-m/%-d"])
        ));
    }
    if rng.chance(20) {
        header.push(format!(
            "tickInterval {}",
            rng.pick(&[
                "1day",
                "1week",
                "2day",
                "1month",
                "6hour",
                "30minute",
                "1hour",
                "3month",
                "10second",
                "1millisecond",
                "1year",
                "0day"
            ])
        ));
    }
    if rng.chance(35) {
        let mut list: Vec<String> = Vec::new();
        for _ in 0..1 + rng.below(3) {
            list.push(match rng.below(4) {
                0 => "weekends".to_owned(),
                1 => (*rng.pick(&["monday", "Friday", "sunday", "saturday", "wednesday"])).to_owned(),
                2 => write(rng),
                _ => {
                    let (y, m, d) = day(rng);
                    format!("{y:04}-{m:02}-{d:02}")
                }
            });
        }
        header.push(format!("excludes {}", list.join(if rng.chance(50) { " " } else { ", " })));
    }
    if rng.chance(10) {
        let (y, m, d) = day(rng);
        header.push(format!("includes {y:04}-{m:02}-{d:02}"));
    }
    if rng.chance(15) {
        header.push("inclusiveEndDates".to_owned());
    }
    if rng.chance(15) {
        header.push("topAxis".to_owned());
    }
    if rng.chance(20) {
        header.push(format!(
            "todayMarker {}",
            rng.pick(&["off", "stroke-width:5px,stroke:#0f0,opacity:0.5", "stroke:red", "on"])
        ));
    }
    if rng.chance(10) {
        header.push(format!("weekday {}", rng.pick(&["monday", "tuesday", "friday", "sunday"])));
    }
    if rng.chance(10) {
        header.push(format!("weekend {}", rng.pick(&["friday", "saturday"])));
    }
    if rng.chance(40) {
        header.push(format!("title {}", rng.pick(WORDS)));
    }
    if rng.chance(10) {
        header.push("accTitle: An accessible title".to_owned());
    }
    if rng.chance(10) {
        header.push("accDescr: A description".to_owned());
    }
    for _ in 0..header.len() {
        let line = header.remove(rng.below(header.len()));
        out.push_str(&format!("    {line}\n"));
    }

    let mut ids: Vec<String> = Vec::new();
    let sections = if rng.chance(20) { 0 } else { 1 + rng.below(4) };
    for s in 0..sections.max(1) {
        if sections > 0 {
            out.push_str(&format!(
                "    section {}\n",
                if rng.chance(10) { "Section" } else { ["Planning", "Build", "Release", "Ops"][s] }
            ));
        }
        for _ in 0..1 + rng.below(5) {
            let mut fields: Vec<String> = Vec::new();
            for tag in ["crit", "done", "active", "milestone", "vert"] {
                if rng.chance(if tag == "vert" { 5 } else { 12 }) {
                    fields.push(tag.to_owned());
                }
            }
            let shape = rng.below(10);
            if shape < 6 {
                let id = (*rng.pick(IDS)).to_owned();
                ids.push(id.clone());
                fields.push(id);
            }
            if shape != 9 {
                let start = match rng.below(10) {
                    0 | 1 if !ids.is_empty() => format!("after {}", rng.pick(&ids)),
                    2 => format!("after {} {}", rng.pick(IDS), rng.pick(IDS)),
                    3 => (*rng.pick(&[
                        "2014-01-06",
                        "Jan 6 2026",
                        "2026/01/06",
                        "tomorrow",
                        "2026-1-6",
                        "1/2/2026 10:00",
                    ]))
                    .to_owned(),
                    _ => write(rng),
                };
                fields.push(start);
            }
            let end = match rng.below(10) {
                0 => write(rng),
                1 => format!("until {}", rng.pick(IDS)),
                2 => (*rng.pick(&["soon", "0d", "1d", "99999d"])).to_owned(),
                _ => duration(rng),
            };
            fields.push(end);
            out.push_str(&format!("    {} :{}\n", rng.pick(WORDS), fields.join(", ")));
        }
    }
    if rng.chance(15) && !ids.is_empty() {
        let id = rng.pick(&ids).clone();
        out.push_str(&match rng.below(3) {
            0 => format!("    click {id} href \"https://example.com/{id}\"\n"),
            1 => format!("    click {id} call alert(\"{id}\")\n"),
            _ => format!("    click {id},nothing href \"javascript:alert(1)\"\n"),
        });
    }
    out
}
