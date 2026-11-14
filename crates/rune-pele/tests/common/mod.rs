//! PNGs land in `$RUNE_PELE_OUT`, or in `out/verify/<run>` at the workspace root when it is unset.

#![allow(dead_code, reason = "each test binary uses its own part of this module")]

use std::path::PathBuf;

use rune_pele::capture::Pixels;

pub fn out_dir_for(run: &str) -> PathBuf {
    std::env::var_os("RUNE_PELE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/verify").join(run))
}

pub fn out_dir() -> PathBuf {
    out_dir_for("rune-spike")
}

pub fn save_in(run: &str, pixels: &Pixels, name: &str) {
    let path = out_dir_for(run).join(name);
    pixels.write_png(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

pub fn save(pixels: &Pixels, name: &str) {
    save_in("rune-spike", pixels, name);
}
