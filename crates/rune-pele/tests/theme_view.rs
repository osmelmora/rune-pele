//! How the side-by-side reads the colours a source sets in its own theme config from Pele's SVG.

mod common;

use common::{as_a_page_reads, rasterise};

#[test]
fn a_page_reads_a_variable_the_root_declares_and_the_fallback_of_any_other() {
    // Pele's spelling: private aliases and a source's series colours on the root, each shape's
    // colour a variable with Pele's own colour as its fallback.
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 30 10\" width=\"30\" height=\"10\" \
               style=\"--_bg:var(--pele-bg,#fff);font:16px sans-serif;--pele-series-1:#ff0000\">\
               <rect width=\"10\" height=\"10\" fill=\"var(--pele-series-1,#4c78a8)\"/>\
               <rect x=\"10\" width=\"10\" height=\"10\" fill=\"var(--pele-series-2,#f58518)\"/>\
               <rect x=\"20\" width=\"10\" height=\"10\" fill=\"#000\"/>\
               <rect x=\"20\" width=\"10\" height=\"10\" fill=\"var(--_bg)\"/></svg>";
    let page = rasterise(&as_a_page_reads(svg), 1.0);
    assert_eq!(page.at(5, 5), [255, 0, 0, 255], "a series the root declares is in the declared colour");
    assert_eq!(page.at(15, 5), [0xf5, 0x85, 0x18, 255], "a series the root leaves alone is in its fallback");
    assert_eq!(page.at(25, 5), [255, 255, 255, 255], "a private alias reads through to its variable's fallback");
}
