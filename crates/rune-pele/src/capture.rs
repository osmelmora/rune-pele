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
        fn view(root: &El, _: &mut Cx<El>) -> El {
            root.clone()
        }
        self.window(root, view, size, Env::new(false, scale)).capture()
    }

    /// An app's `view` of `state` in a headless window, which keeps its state, memos and element
    /// identity from one capture to the next as a real window does.
    pub fn window<S: 'static>(
        &self,
        state: S,
        view: fn(&S, &mut Cx<S>) -> El,
        size: (f32, f32),
        env: Env,
    ) -> Window<'_, S> {
        let mut runtime = Runtime::new(state, view, size, env);
        runtime.set_srgb_blend(true);
        let (width, height) = ((size.0 * env.scale).round() as u32, (size.1 * env.scale).round() as u32);
        let format = OFFSCREEN_FORMAT.remove_srgb_suffix();
        // One renderer for the window's life: it holds the glyphs the runtime's atlas has
        // already sent it, as a window's renderer does.
        let mut renderer = Renderer::new(&self.gpu.device, format);
        renderer.set_srgb_blend(true);
        renderer.attach_text(&mut runtime.text.atlas);
        let target = Offscreen::with_format(&self.gpu.device, width, height, format);
        runtime.frame(0.0);
        Window { capturer: self, runtime, renderer, target, size: (width, height) }
    }
}

/// A headless window: see [`Capturer::window`].
pub struct Window<'a, S: 'static> {
    capturer: &'a Capturer,
    runtime: Runtime<S>,
    renderer: Renderer,
    target: Offscreen,
    /// In device pixels.
    size: (u32, u32),
}

impl<S: 'static> Window<'_, S> {
    /// Changes the state as a handler would, for the next capture to show.
    pub fn update(&mut self, change: impl FnOnce(&mut S)) {
        self.runtime.update(|state, _| change(state));
    }

    pub fn state(&self) -> &S {
        &self.runtime.state
    }

    pub fn runtime(&self) -> &Runtime<S> {
        &self.runtime
    }

    /// Presses and releases the pointer at a point of the window, then runs the handlers that
    /// queued.
    pub fn press(&mut self, x: f32, y: f32) {
        self.runtime.pointer_move(x, y);
        self.runtime.pointer_down(x, y);
        self.runtime.pointer_up(x, y);
        self.runtime.frame(0.0);
    }

    /// Rests the pointer at a point of the window for `seconds`, as long as a tooltip waits.
    pub fn rest(&mut self, x: f32, y: f32, seconds: f64) {
        self.runtime.pointer_move(x, y);
        self.runtime.frame(0.0);
        self.runtime.frame(seconds);
    }

    /// The window once what moves has come to rest.
    pub fn capture(&mut self) -> Pixels {
        let gpu = &self.capturer.gpu;
        let rt = &mut self.runtime;
        let mut moving = rt.frame(1.0);
        for _ in 0..SETTLE_FRAMES {
            if !moving {
                break;
            }
            moving = rt.frame(1.0);
        }
        let (width, height) = self.size;
        let (renderer, target) = (&mut self.renderer, &self.target);
        renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, (width, height));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.draw(&mut encoder, &target.view, rt.clear_color());
        target.copy(&mut encoder);
        renderer.submit(&gpu.queue, encoder);
        Pixels { width, height, rgba: target.read(&gpu.device) }
    }
}
