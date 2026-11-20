//! The icons an app gives a view to draw in the boxes a diagram leaves for icons by name.
//!
//! An [`Icon`] is what an Iconify icon is: SVG path data on a view box, each path filled or
//! stroked. Its paths are read once, into the Scene's own path commands, so the lowering draws
//! them as it draws every other shape. Pele draws a resolver's icon in a box 24 units across, with
//! `fill="none" stroke="currentColor" stroke-width="2"` and round caps and joins for the icon to
//! inherit: [`Icon::stroke`] on a view box 24 across with a width of 2 is that frame's look.

use pele_core::scene::{PathCommand, Point, Size};
use rune::Color;

use crate::geometry::{self, Edge};

/// A vector icon: SVG path data on a view box, filled or stroked, as an Iconify icon is. It is
/// fitted to its box as SVG fits a `viewBox`: whole, as large as fits, and centred.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    view_box: ViewBox,
    /// Painted in order, each over those before it.
    coats: Vec<IconCoat>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ViewBox {
    origin: Point,
    size: Size,
}

/// One path of an icon, filled or stroked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IconCoat {
    pub(crate) commands: Vec<PathCommand>,
    pub(crate) paint: IconPaint,
    /// The stroke's width, or `None` for a fill by SVG's nonzero rule.
    pub(crate) stroke: Option<f64>,
}

/// The colour of an icon's fill or stroke.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum IconPaint {
    /// SVG's `currentColor`. Pele's SVG sets the diagram's foreground as the colour every icon box
    /// inherits, so this is the palette's [`Role::Foreground`](crate::Role::Foreground), in light
    /// and in dark mode.
    Current,
    /// A colour of the icon's own, as a logo has. It is drawn as it is in every palette, as Pele's
    /// SVG draws the colours an icon's markup names.
    Fixed(Color),
}

/// Why an icon could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IconError {
    /// The view box's width or height is not a positive number.
    ViewBox,
    /// A stroke's width is not a positive number.
    StrokeWidth,
    /// The path data is not SVG's: it goes wrong at byte `at`.
    PathData {
        /// The byte of the path data where it goes wrong.
        at: usize,
    },
}

impl std::fmt::Display for IconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IconError::ViewBox => f.write_str("an icon's view box must be a positive number wide and high"),
            IconError::StrokeWidth => f.write_str("an icon's stroke must be a positive number wide"),
            IconError::PathData { at } => write!(f, "an icon's path data is not SVG's at byte {at}"),
        }
    }
}

impl std::error::Error for IconError {}

impl Icon {
    /// An icon with nothing drawn yet, on `view_box`: `[x, y, width, height]` in the icon's own
    /// units, as SVG's `viewBox` attribute gives them.
    pub fn new(view_box: [f64; 4]) -> Result<Icon, IconError> {
        let [x, y, width, height] = view_box;
        let positive = |length: f64| length.is_finite() && length > 0.0;
        if !(x.is_finite() && y.is_finite() && positive(width) && positive(height)) {
            return Err(IconError::ViewBox);
        }
        Ok(Icon { view_box: ViewBox { origin: Point { x, y }, size: Size { width, height } }, coats: Vec::new() })
    }

    /// `data`, SVG path data, filled in `paint` by SVG's nonzero rule, over what the icon draws
    /// so far.
    pub fn fill(self, data: &str, paint: IconPaint) -> Result<Icon, IconError> {
        self.coat(data, paint, None)
    }

    /// `data`, SVG path data, stroked `width` units of the view box wide in `paint` with round
    /// caps and joins, as Pele's icon box strokes, over what the icon draws so far.
    pub fn stroke(self, data: &str, width: f64, paint: IconPaint) -> Result<Icon, IconError> {
        if !(width.is_finite() && width > 0.0) {
            return Err(IconError::StrokeWidth);
        }
        self.coat(data, paint, Some(width))
    }

    fn coat(mut self, data: &str, paint: IconPaint, stroke: Option<f64>) -> Result<Icon, IconError> {
        self.coats.push(IconCoat { commands: path_data(data)?, paint, stroke });
        Ok(self)
    }

    /// The icon's coats in a box at `origin`, `size` across: the view box fitted to the box as
    /// SVG's default `preserveAspectRatio` fits it, whole, as large as fits and centred, and each
    /// stroke's width with it.
    pub(crate) fn placed(&self, origin: Point, size: Size) -> Vec<IconCoat> {
        let ViewBox { origin: from, size: of } = self.view_box;
        let scale = (size.width / of.width).min(size.height / of.height);
        let left = origin.x + (size.width - of.width * scale) / 2.0 - from.x * scale;
        let top = origin.y + (size.height - of.height * scale) / 2.0 - from.y * scale;
        let fit = |p: Point| Point { x: left + p.x * scale, y: top + p.y * scale };
        let commands = |coat: &IconCoat| {
            coat.commands
                .iter()
                .map(|&command| match command {
                    PathCommand::Move(to) => PathCommand::Move(fit(to)),
                    PathCommand::Line(to) => PathCommand::Line(fit(to)),
                    PathCommand::Quad { control, end } => PathCommand::Quad { control: fit(control), end: fit(end) },
                    PathCommand::Cubic { first, second, end } => {
                        PathCommand::Cubic { first: fit(first), second: fit(second), end: fit(end) }
                    }
                    PathCommand::Arc { rx, ry, large, sweep, end } => {
                        PathCommand::Arc { rx: rx * scale, ry: ry * scale, large, sweep, end: fit(end) }
                    }
                    PathCommand::Close => PathCommand::Close,
                })
                .collect()
        };
        self.coats
            .iter()
            .map(|coat| IconCoat {
                commands: commands(coat),
                paint: coat.paint,
                stroke: coat.stroke.map(|width| width * scale),
            })
            .collect()
    }
}

/// SVG path data as absolute commands. A command after a close starts its subpath with a move to
/// where the closed one started, as SVG does. An arc turned off the axes is the cubics the
/// lowering would draw it with, since the Scene's arc has no turn.
fn path_data(data: &str) -> Result<Vec<PathCommand>, IconError> {
    let mut reader = Reader { data: data.as_bytes(), at: 0 };
    let mut out = Vec::new();
    let (mut here, mut start) = ([0.0; 2], [0.0; 2]);
    // The handle the last command left to mirror for a smooth one, and whether it was a cubic's.
    let mut handle: Option<([f64; 2], bool)> = None;
    let mut closed = false;
    let mut command: Option<u8> = None;
    reader.skip_separators();
    while !reader.done() {
        let begin = reader.at;
        let letter = match reader.letter() {
            Some(letter) => letter,
            // A number after a command repeats it, and after a move it is a line.
            None => match command {
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(letter) if !letter.eq_ignore_ascii_case(&b'z') => letter,
                _ => return Err(reader.wrong()),
            },
        };
        if command.is_none() && !letter.eq_ignore_ascii_case(&b'm') {
            return Err(IconError::PathData { at: begin });
        }
        command = Some(letter);
        let relative = letter.is_ascii_lowercase();
        let point = |reader: &mut Reader, here: [f64; 2]| -> Result<[f64; 2], IconError> {
            let [x, y] = [reader.number()?, reader.number()?];
            Ok(if relative { [here[0] + x, here[1] + y] } else { [x, y] })
        };
        let upper = letter.to_ascii_uppercase();
        if closed && upper != b'M' && upper != b'Z' {
            out.push(PathCommand::Move(at(start)));
        }
        closed = false;
        let mut smooth = None;
        match upper {
            b'M' => {
                here = point(&mut reader, here)?;
                start = here;
                out.push(PathCommand::Move(at(here)));
            }
            b'L' => {
                here = point(&mut reader, here)?;
                out.push(PathCommand::Line(at(here)));
            }
            b'H' => {
                let x = reader.number()?;
                here[0] = if relative { here[0] + x } else { x };
                out.push(PathCommand::Line(at(here)));
            }
            b'V' => {
                let y = reader.number()?;
                here[1] = if relative { here[1] + y } else { y };
                out.push(PathCommand::Line(at(here)));
            }
            b'C' | b'S' => {
                let first = match upper {
                    b'C' => point(&mut reader, here)?,
                    _ => mirrored(here, handle.filter(|&(_, cubic)| cubic)),
                };
                let second = point(&mut reader, here)?;
                here = point(&mut reader, here)?;
                out.push(PathCommand::Cubic { first: at(first), second: at(second), end: at(here) });
                smooth = Some((second, true));
            }
            b'Q' | b'T' => {
                let control = match upper {
                    b'Q' => point(&mut reader, here)?,
                    _ => mirrored(here, handle.filter(|&(_, cubic)| !cubic)),
                };
                here = point(&mut reader, here)?;
                out.push(PathCommand::Quad { control: at(control), end: at(here) });
                smooth = Some((control, false));
            }
            b'A' => {
                let radii = [reader.number()?.abs(), reader.number()?.abs()];
                let rotation = reader.number()?;
                let (large, sweep) = (reader.flag()?, reader.flag()?);
                let from = here;
                here = point(&mut reader, here)?;
                out.extend(arc(from, geometry::Arc { radii, rotation, large, sweep, to: here }));
            }
            b'Z' => {
                here = start;
                closed = true;
                out.push(PathCommand::Close);
            }
            _ => return Err(reader.wrong()),
        }
        handle = smooth;
        reader.skip_separators();
    }
    Ok(out)
}

fn at([x, y]: [f64; 2]) -> Point {
    Point { x, y }
}

/// The first handle of a smooth curve: the last one mirrored about `here`, or `here` itself
/// where the command before was not a curve of the same kind.
fn mirrored(here: [f64; 2], handle: Option<([f64; 2], bool)>) -> [f64; 2] {
    match handle {
        Some((handle, _)) => [2.0 * here[0] - handle[0], 2.0 * here[1] - handle[1]],
        None => here,
    }
}

/// An arc as the Scene's commands: an arc where it is not turned, or where its radii are equal
/// so that a turn changes nothing, and otherwise the cubics that draw it.
fn arc(from: [f64; 2], arc: geometry::Arc) -> Vec<PathCommand> {
    let [rx, ry] = arc.radii;
    if arc.rotation == 0.0 || rx == ry {
        return vec![PathCommand::Arc { rx, ry, large: arc.large, sweep: arc.sweep, end: at(arc.to) }];
    }
    geometry::arc_edges(from, &arc)
        .into_iter()
        .map(|edge| match edge {
            Edge::Cubic(c) => PathCommand::Cubic { first: at(c.c1), second: at(c.c2), end: at(c.to) },
            Edge::Line(to) => PathCommand::Line(at(to)),
            Edge::Move(to) => PathCommand::Move(at(to)),
            Edge::Close => PathCommand::Close,
        })
        .collect()
}

/// SVG path data being read, a byte at a time.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn done(&self) -> bool {
        self.at == self.data.len()
    }

    fn wrong(&self) -> IconError {
        IconError::PathData { at: self.at }
    }

    fn skip_separators(&mut self) {
        while self.data.get(self.at).is_some_and(|&b| b.is_ascii_whitespace() || b == b',') {
            self.at += 1;
        }
    }

    /// The command letter here, if there is one.
    fn letter(&mut self) -> Option<u8> {
        let letter = *self.data.get(self.at).filter(|b| b"MmLlHhVvCcSsQqTtAaZz".contains(b))?;
        self.at += 1;
        self.skip_separators();
        Some(letter)
    }

    /// A number in SVG's grammar: a sign, digits with or without a point, and an exponent. It
    /// ends where the next can start, so `1.5.5` is 1.5 and .5, and `1-2` is 1 and -2.
    fn number(&mut self) -> Result<f64, IconError> {
        self.skip_separators();
        let begin = self.at;
        let digits = |reader: &mut Reader| {
            let from = reader.at;
            while reader.data.get(reader.at).is_some_and(u8::is_ascii_digit) {
                reader.at += 1;
            }
            reader.at > from
        };
        if matches!(self.data.get(self.at), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let whole = digits(self);
        let fraction = self.data.get(self.at) == Some(&b'.') && {
            self.at += 1;
            digits(self)
        };
        if !whole && !fraction {
            self.at = begin;
            return Err(self.wrong());
        }
        if matches!(self.data.get(self.at), Some(b'e' | b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.data.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !digits(self) {
                self.at = mark;
            }
        }
        let text = std::str::from_utf8(&self.data[begin..self.at]).expect("ASCII");
        let number: f64 = text.parse().map_err(|_| IconError::PathData { at: begin })?;
        self.skip_separators();
        Ok(number)
    }

    /// An arc's flag: one `0` or `1`, which needs nothing after it before the next number.
    fn flag(&mut self) -> Result<bool, IconError> {
        self.skip_separators();
        let flag = match self.data.get(self.at) {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(self.wrong()),
        };
        self.at += 1;
        self.skip_separators();
        Ok(flag)
    }
}
