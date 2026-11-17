//! Info diagrams: Pele's version, with an optional title. Ported from Pele's `src/diagrams/info`.

use super::railroad::close_indents;
use super::railroad::probe::maybe;
use super::tokens::{
    ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, Reader, SINGLE_LINE_COMMENT, SyntaxError, TITLE, TokenType, WHITESPACE,
    YAML, acc_descr_value, acc_title_value, keyword, title_value, tokenize,
};
use crate::catalog::{Diagram, DiagramKind, DrawContext, ParseContext};
use crate::input::{JsText, Units};
use crate::measure::{Emphasis, layout_label};
use crate::scene::{Accessible, Entity, Facet, LabelLook, Paint, Point, Role, Scene, SceneBuilder, Size, TitleLayout};
use crate::{Origin, ParseOptions, RenderError, tagged};

/// The version of Pele this port renders as, which `package.json` gives Pele's info diagram.
const PELE_VERSION: &str = "1.13.0";

const SHOW_INFO: usize = 0;
const INFO: usize = 1;
const ACC_DESCR_KIND: usize = 2;
const ACC_TITLE_KIND: usize = 3;
const TITLE_KIND: usize = 4;
const NEWLINE_KIND: usize = 5;

static TOKENS: [TokenType; 10] = [
    keyword("showInfo"),
    keyword("info"),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Default)]
struct InfoAst {
    title: Option<Vec<u16>>,
    acc_title: Option<Vec<u16>>,
    acc_descr: Option<Vec<u16>>,
}

/// Pele's `parseInfo`.
fn parse_info(src: &[u16]) -> Result<InfoAst, SyntaxError> {
    let mut r = Reader::new(src, tokenize(src, &TOKENS)?, &TOKENS);
    let mut ast = InfoAst::default();
    while r.accept(NEWLINE_KIND) {}
    r.expect(INFO)?;
    while r.accept(NEWLINE_KIND) {}
    if r.accept(SHOW_INFO) {
        while r.accept(NEWLINE_KIND) {}
    }
    while let Some(kind) = r.kind() {
        match kind {
            ACC_DESCR_KIND => ast.acc_descr = Some(acc_descr_value(r.take())),
            ACC_TITLE_KIND => ast.acc_title = Some(acc_title_value(r.take())),
            TITLE_KIND => ast.title = Some(title_value(r.take())),
            _ => return Err(r.fail("a title or the end of input")),
        }
        match r.kind() {
            None => break,
            Some(NEWLINE_KIND) => while r.accept(NEWLINE_KIND) {},
            Some(_) => return Err(r.fail("a line break or the end of input")),
        }
    }
    Ok(ast)
}

pub(crate) struct InfoModel {
    title: Option<JsText>,
    accessible: Accessible,
}

/// Pele's `buildInfo`.
fn build_info(ast: InfoAst, front_title: Option<&JsText>) -> InfoModel {
    let filled = |text: Option<Vec<u16>>| text.filter(|text| !text.is_empty());
    InfoModel {
        title: filled(ast.title).map(JsText::from).or_else(|| front_title.filter(|title| !title.is_empty()).cloned()),
        accessible: Accessible {
            title: filled(ast.acc_title).map(|text| text.trim_start_js().into()),
            description: filled(ast.acc_descr).map(|text| close_indents(&text)),
        },
    }
}

fn parse(cx: &ParseContext<'_>) -> Result<InfoModel, RenderError> {
    let ast = parse_info(cx.text).map_err(|error| cx.syntax(error))?;
    Ok(build_info(ast, cx.title))
}

pub(crate) struct Info;

impl Diagram for Info {
    type Model = InfoModel;

    fn parse(cx: &ParseContext<'_>) -> Result<InfoModel, RenderError> {
        parse(cx)
    }

    /// Pele's `renderInfo`: the title above the version, both centred.
    fn scene(model: &InfoModel, cx: &DrawContext<'_>) -> Result<Scene, RenderError> {
        let size = cx.options.typography.normal.size;
        let pad = cx.options.padding_or_default();
        let title_text = model.title.as_deref().unwrap_or_default();
        let title = layout_label(title_text, false, cx.measure, size, 4000.0, Emphasis::BOLD);
        let title_height = if title.height > 0.0 { title.height + 8.0 } else { 0.0 };
        let version: JsText = format!("Pele v{PELE_VERSION}").as_str().into();
        let text = layout_label(&version, false, cx.measure, size, 4000.0, Emphasis::default());

        let width = (crate::js::max(title.width, text.width) + 2.0 * pad).ceil();
        let height = (title_height + text.height + 2.0 * pad).ceil();
        let mut out = SceneBuilder::new(cx, Size { width, height }, model.accessible.clone());
        let at = Point { x: width / 2.0, y: pad + title_height + text.height / 2.0 };
        out.title(&TitleLayout { at: Point { x: width / 2.0, y: pad + title.height / 2.0 }, label: title });
        let part = out.part(Entity::diagram());
        let look = LabelLook::class("pele-version").fill(Some(Paint::Role(Role::Muted)));
        out.label(part, Facet::Label, &text, at, look);
        out.finish()
    }
}

/// The info front end's model as JSON, in the words of the oracle's `info_front` op, for the
/// differential test. A source of another type answers only that type.
pub fn front_json(source: &str, options: ParseOptions) -> Result<String, RenderError> {
    let front = crate::front(source, &options)?;
    if front.found.kind != DiagramKind::Info {
        return Ok(format!("{{\"type\":\"{}\"}}", front.found.kind.name()));
    }
    let origin = Origin { source };
    let model = parse(&front.context(&origin, options.limits)).map_err(|error| tagged(error, front.found))?;
    let mut out = String::from("{\"type\":\"info\",\"title\":");
    maybe(&mut out, model.title.as_deref());
    out.push_str(",\"accTitle\":");
    maybe(&mut out, model.accessible.title.as_deref());
    out.push_str(",\"accDescr\":");
    maybe(&mut out, model.accessible.description.as_deref());
    out.push('}');
    Ok(out)
}
