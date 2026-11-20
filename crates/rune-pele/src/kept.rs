//! What a view shows while its source fails after one that drew: that drawing, in the look
//! [`KeptLook`] names.

use rune::{Color, El, TextStyle, col, el, text};

/// How a view shows the last drawing that drew while its source fails.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeptLook {
    /// As Pele's `mount` shows it: the drawing faded to 0.4 opacity as one layer, with the
    /// error's first and last lines over its bottom edge in the palette's critical colour, and
    /// the whole message as that line's tooltip.
    #[default]
    DimmedWithError,
    /// The drawing as it was drawn, with nothing over it. The app reads the error from
    /// [`Diagram::error`](crate::Diagram::error), as it may want to while a source streams in.
    Unchanged,
}

/// The opacity Pele's `mark` gives a drawing whose text since then cannot be drawn.
const DIMMED: f32 = 0.4;

/// The error's size as a share of the view's font size, and its line's height as a share of
/// its own size: Pele's `OVER_STYLE`, `font-size:0.8em` and `height:1.3em;line-height:1.3`.
const ERROR_SIZE: f32 = 0.8;
const ERROR_LINE: f32 = 1.3;

/// A parse error's first line says where it is, and its last line what is wrong: Pele's
/// `summary`.
pub(crate) fn summary(message: &str) -> String {
    let lines: Vec<&str> = message.split('\n').filter(|line| !line.trim().is_empty()).collect();
    match lines.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [first, .., last] => format!("{first} {last}"),
    }
}

impl KeptLook {
    /// `drawing`, keyed `key`, shown in this look while `message` says why the source fails.
    /// `font_size` is the view's, and `critical` the palette's critical colour.
    pub(crate) fn el(self, drawing: El, key: &str, message: &str, font_size: f32, critical: Color) -> El {
        match self {
            KeptLook::Unchanged => drawing,
            KeptLook::DimmedWithError => {
                let size = ERROR_SIZE * font_size;
                let line = ERROR_LINE * size;
                let error = text(summary(message))
                    .key("error")
                    .font(TextStyle::new(size, 400.0).lh(line))
                    .color(critical)
                    .truncate()
                    .help(message)
                    .pin(Some(0.0), Some(-line), Some(0.0), None)
                    .h(line);
                // As Pele does, the error comes after the drawing and is pulled up over its
                // bottom edge by its own height, taking no room: from an edge of no height that
                // follows the drawing, wherever a parent that stretches puts the bottom.
                let edge = el().h(0.0).child(error);
                col().key(key).child(el().layer_opacity(DIMMED).child(drawing)).child(edge)
            }
        }
    }
}
