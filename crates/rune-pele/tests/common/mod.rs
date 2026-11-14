//! PNGs land in `$RUNE_PELE_OUT`, or in `out/verify/rune-spike` at the workspace root when it is
//! unset.

use std::path::PathBuf;

use rune_pele::capture::Pixels;

pub fn out_dir() -> PathBuf {
    std::env::var_os("RUNE_PELE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/verify/rune-spike"))
}

pub fn save(pixels: &Pixels, name: &str) {
    let path = out_dir().join(name);
    pixels.write_png(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
