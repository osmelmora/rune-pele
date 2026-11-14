//! Headless pixels of an `El` tree, drawn by the renderer a Rune window draws with, for tests and
//! verification runs to measure and to save as PNG.

use std::path::Path;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer, write_png};
use rune::{Cx, El, Env, Runtime};

/// One-second frames a capture runs after the first while anything still moves. A tree that has
/// not come to rest by then is read back as it stands.
const SETTLE_FRAMES: usize = 8;

/// Device pixels, sRGB-encoded RGBA8, rows top to bottom.
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Pixels {
    pub fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }

    /// Writes the pixels as a PNG, creating `path`'s directory.
    pub fn write_png(&self, path: &Path) -> std::io::Result<()> {
        write_png(path, self.width, self.height, &self.rgba)
    }
}

/// A headless GPU, kept across captures because making one costs far more than a frame.
pub struct Capturer {
    gpu: Gpu,
}

impl Default for Capturer {
    fn default() -> Self {
        Self::new()
    }
}

impl Capturer {
    pub fn new() -> Self {
        Capturer { gpu: Gpu::headless() }
    }

    /// `root` laid out in a `size`-point viewport and drawn at `scale` device pixels per point,
    /// blending in sRGB as Rune apps do by default.
    pub fn capture(&self, root: El, size: (f32, f32), scale: f32) -> Pixels {
        let mut rt = Runtime::new(root, view, size, Env::new(false, scale));
        rt.set_srgb_blend(true);
        let mut moving = rt.frame(0.0);
        for _ in 0..SETTLE_FRAMES {
            if !moving {
                break;
            }
            moving = rt.frame(1.0);
        }
        let (width, height) = ((size.0 * scale).round() as u32, (size.1 * scale).round() as u32);
        let format = OFFSCREEN_FORMAT.remove_srgb_suffix();
        let mut renderer = Renderer::new(&self.gpu.device, format);
        renderer.set_srgb_blend(true);
        let target = Offscreen::with_format(&self.gpu.device, width, height, format);
        renderer.prepare(&self.gpu.device, &self.gpu.queue, &mut rt.text.atlas, &rt.instances, (width, height));
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        renderer.draw(&mut encoder, &target.view, rt.clear_color());
        target.copy(&mut encoder);
        renderer.submit(&self.gpu.queue, encoder);
        Pixels { width, height, rgba: target.read(&self.gpu.device) }
    }
}

fn view(root: &El, _: &mut Cx<El>) -> El {
    root.clone()
}
