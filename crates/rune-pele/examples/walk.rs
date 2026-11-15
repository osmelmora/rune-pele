//! The paint walk of one Mermaid source, one line per group and drawing: what the Rune view
//! reads. `cargo run -p rune-pele --example walk -- <file.mmd> [--native]` lays out with Pele's
//! metrics, as its SVG does.

use pele_core::scene::{Geometry, PaintVisit};

fn main() {
    let path = std::env::args().nth(1).expect("usage: walk <file.mmd>");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let parity = pele_core::parity_scene(&source).unwrap_or_else(|error| panic!("{path}: {error}"));
    let scene = parity.scene();
    println!("size {:?}; unread styles {:?}", scene.size(), scene.unread_styles());
    let mut depth = 0;
    for visit in scene.paint() {
        let pad = "  ".repeat(depth);
        match visit {
            PaintVisit::Begin(group) => {
                let part = group.part.as_ref().map(|part| format!("{:?} {:?}", part.entity.kind(), part.facet));
                println!(
                    "{pad}group {part:?} translate ({}, {}) rotate {:?} opacity {} link {:?} tooltip {:?} untouchable {}",
                    group.translate.x,
                    group.translate.y,
                    group.rotate,
                    group.opacity,
                    group.link,
                    group.tooltip,
                    group.untouchable
                );
                depth += 1;
            }
            PaintVisit::End => depth -= 1,
            PaintVisit::Draw(draw) => {
                let geometry = match &draw.geometry {
                    Geometry::Text(run) => {
                        format!("text {:?} at {:?} {:?} {:?}", run.text, run.baseline, run.anchor, run.font)
                    }
                    other => format!("{other:?}"),
                };
                println!(
                    "{pad}{} origin ({}, {})\n{pad}    {geometry}\n{pad}    {:?}",
                    draw.key, draw.origin.x, draw.origin.y, draw.style
                );
            }
        }
    }
}
