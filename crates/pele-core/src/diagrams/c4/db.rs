//! Pele's `C4Db` (`src/diagrams/c4/db.ts`): what each statement does to the model.
//!
//! A positional argument fills the field of its slot. A named one fills the field it names,
//! whichever slot it is in; a name that is no known field is kept aside in `extra`.

use super::{
    Attr, Boundary, C4Kind, C4Model, Element, ElementField, Found, Named, NodeType, Rel, RelField, RelType, Shape,
    ShapeKind,
};
use crate::input::{JsText, Units, is_space};
use indexmap::IndexMap;

/// Pele's `parseInt(text)`: optional space and sign, then the digits of base 10, or of base 16
/// after `0x`. NaN where there are none.
pub(super) fn parse_int(text: &[u16]) -> f64 {
    let text = text.trim_start_js();
    let (sign, rest) = match text.first() {
        Some(45) => (-1.0, &text[1..]),
        Some(43) => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    let hex = rest.len() >= 2 && rest[0] == 48 && (rest[1] == 120 || rest[1] == 88);
    let (radix, digits) = if hex { (16, &rest[2..]) } else { (10, rest) };
    let length = digits.run(0, |c| char::from_u32(u32::from(c)).is_some_and(|c| c.is_digit(radix)));
    if length == 0 {
        return f64::NAN;
    }
    let digits = String::from_utf16_lossy(&digits[..length]);
    let value = if radix == 10 { digits.parse::<f64>().expect("digits parse") } else { hex_value(&digits) };
    sign * value
}

/// Hex digits as the nearest double, as V8 rounds them: the first 32 digits exactly, with any
/// digit after them that is not zero kept as the lowest bit, so that a tie breaks the right way.
#[expect(clippy::disallowed_methods, reason = "a whole power of two, which every order of multiplying gives exactly")]
fn hex_value(digits: &str) -> f64 {
    let digits = digits.trim_start_matches('0');
    let value =
        |digits: &str| digits.chars().fold(0u128, |value, digit| value << 4 | digit.to_digit(16).unwrap() as u128);
    if digits.len() <= 32 {
        return value(digits) as f64;
    }
    let (head, tail) = digits.split_at(32);
    let sticky = u128::from(tail.chars().any(|digit| digit != '0'));
    let exponent = i32::try_from(4 * tail.len()).unwrap_or(i32::MAX);
    (value(head) | sticky) as f64 * 2f64.powi(exponent)
}

/// Something a named argument can set a field of.
trait Target {
    fn label(&mut self) -> (&mut JsText, &mut Option<Named>);
    fn named(&mut self, attr: &Named);
}

fn extra(extra: &mut Option<IndexMap<JsText, JsText>>, attr: &Named) {
    extra.get_or_insert_with(IndexMap::new).insert(attr.key.clone(), attr.value.clone());
}

impl Target for Element {
    fn label(&mut self) -> (&mut JsText, &mut Option<Named>) {
        (&mut self.label, &mut self.label_attr)
    }

    fn named(&mut self, attr: &Named) {
        if attr.key.is_str("label") {
            self.label = attr.value.clone();
        } else if let Some(field) = ElementField::ALL.iter().find(|field| attr.key.is_str(field.name())) {
            self.fields[*field as usize] = Some(attr.value.clone());
        } else {
            extra(&mut self.extra, attr);
        }
    }
}

impl Target for Rel {
    fn label(&mut self) -> (&mut JsText, &mut Option<Named>) {
        (&mut self.label, &mut self.label_attr)
    }

    fn named(&mut self, attr: &Named) {
        if attr.key.is_str("offsetX") {
            self.offset_x = Some(parse_int(&attr.value));
        } else if attr.key.is_str("offsetY") {
            self.offset_y = Some(parse_int(&attr.value));
        } else if attr.key.is_str("label") {
            self.label = attr.value.clone();
        } else if let Some(field) = RelField::ALL.iter().find(|field| attr.key.is_str(field.name())) {
            self.fields[*field as usize] = Some(attr.value.clone());
        } else {
            extra(&mut self.extra, attr);
        }
    }
}

/// Pele's `put`: a positional argument fills `field`, a named one the field it names.
fn put_element(element: &mut Element, field: ElementField, attr: Option<&Attr>) {
    match attr {
        None => {}
        Some(Attr::Text(text)) => element.fields[field as usize] = Some(text.clone()),
        Some(Attr::Named(named)) => element.named(named),
    }
}

fn put_rel(rel: &mut Rel, field: RelField, attr: Option<&Attr>) {
    match attr {
        None => {}
        Some(Attr::Text(text)) => rel.fields[field as usize] = Some(text.clone()),
        Some(Attr::Named(named)) => rel.named(named),
    }
}

/// Pele's `text`: as `put`, and a missing argument makes the field empty where it is unset.
fn text_element(element: &mut Element, field: ElementField, attr: Option<&Attr>) {
    match attr {
        None => {
            element.fields[field as usize].get_or_insert_with(JsText::default);
        }
        Some(_) => put_element(element, field, attr),
    }
}

fn text_rel(rel: &mut Rel, field: RelField, attr: Option<&Attr>) {
    match attr {
        None => {
            rel.fields[field as usize].get_or_insert_with(JsText::default);
        }
        Some(_) => put_rel(rel, field, attr),
    }
}

/// Pele's `label`: a named argument in the label's slot leaves the label empty and sets what it
/// names.
fn label(target: &mut impl Target, attr: Option<&Attr>) {
    {
        let (_, label_attr) = target.label();
        *label_attr = None;
    }
    match attr {
        Some(Attr::Named(named)) => {
            let (label, label_attr) = target.label();
            *label = JsText::default();
            *label_attr = Some(named.clone());
            target.named(named);
        }
        Some(Attr::Text(text)) => *target.label().0 = text.clone(),
        None => *target.label().0 = JsText::default(),
    }
}

pub(super) struct C4Db {
    title: Option<JsText>,
    acc_descr: Option<JsText>,
    shapes: Vec<Shape>,
    boundaries: Vec<Boundary>,
    rels: Vec<Rel>,
    shape_in_row: f64,
    boundary_in_row: f64,
    shape_index: IndexMap<JsText, usize>,
    // The top-level boundary is not listed, so a boundary a diagram names `global` is its own box.
    boundary_index: IndexMap<JsText, usize>,
    rel_index: IndexMap<JsText, IndexMap<JsText, usize>>,
    current: usize,
    stack: Vec<usize>,
}

impl C4Db {
    pub(super) fn new() -> Self {
        let global = Element {
            alias: "global".into(),
            label: "global".into(),
            fields: {
                let mut fields: [Option<JsText>; 13] = Default::default();
                fields[ElementField::Type as usize] = Some("global".into());
                fields
            },
            ..Element::default()
        };
        C4Db {
            title: None,
            acc_descr: None,
            shapes: Vec::new(),
            boundaries: vec![Boundary { element: global, node_type: None }],
            rels: Vec::new(),
            shape_in_row: 4.0,
            boundary_in_row: 2.0,
            shape_index: IndexMap::new(),
            boundary_index: IndexMap::new(),
            rel_index: IndexMap::new(),
            current: 0,
            stack: Vec::new(),
        }
    }

    pub(super) fn finish(self, c4_type: C4Kind) -> C4Model {
        C4Model {
            c4_type,
            title: self.title,
            acc_descr: self.acc_descr,
            shapes: self.shapes,
            boundaries: self.boundaries,
            rels: self.rels,
            shape_in_row: self.shape_in_row,
            boundary_in_row: self.boundary_in_row,
            shape_index: self.shape_index,
            boundary_index: self.boundary_index,
        }
    }

    pub(super) fn set_title(&mut self, title: JsText) {
        self.title = Some(title);
    }

    /// `descr.replace(/\n\s+/g, '\n')`
    pub(super) fn set_acc_description(&mut self, descr: &[u16]) {
        let mut out = JsText::default();
        let mut at = 0;
        while at < descr.len() {
            out.push(descr[at]);
            if descr[at] == 10 && descr.get(at + 1).is_some_and(|&c| is_space(c)) {
                at = descr.run(at + 1, is_space);
            } else {
                at += 1;
            }
        }
        self.acc_descr = Some(out);
    }

    /// Pele's `element`: the shape or boundary an alias names, made where there is none. A named
    /// argument in the alias's slot always makes a new one.
    fn element<E: AsMut<Element>>(
        list: &mut Vec<E>,
        index: &mut IndexMap<JsText, usize>,
        alias: &Attr,
        make: impl FnOnce() -> E,
    ) -> usize {
        let found = match alias {
            Attr::Text(alias) => index.get(&**alias).copied(),
            Attr::Named(_) => None,
        };
        found.unwrap_or_else(|| {
            let at = list.len();
            list.push(make());
            let element = list[at].as_mut();
            match alias {
                Attr::Text(alias) => {
                    element.alias = alias.clone();
                    index.insert(alias.clone(), at);
                }
                Attr::Named(named) => {
                    element.alias_attr = Some(named.clone());
                    element.named(named);
                }
            }
            at
        })
    }

    fn shape(&mut self, kind: ShapeKind, attrs: &[Attr]) -> usize {
        let at = Self::element(&mut self.shapes, &mut self.shape_index, &attrs[0], || Shape {
            element: Element::default(),
            kind,
        });
        label(&mut self.shapes[at].element, attrs.get(1));
        at
    }

    /// alias, label, ?descr, ?sprite, ?tags, $link
    pub(super) fn add_person_or_system(&mut self, kind: ShapeKind, attrs: &[Attr]) {
        let at = self.shape(kind, attrs);
        text_element(&mut self.shapes[at].element, ElementField::Descr, attrs.get(2));
        self.finish_shape(at, kind, attrs, 3);
    }

    /// alias, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub(super) fn add_container(&mut self, kind: ShapeKind, attrs: &[Attr]) {
        let at = self.shape(kind, attrs);
        let element = &mut self.shapes[at].element;
        text_element(element, ElementField::Techn, attrs.get(2));
        text_element(element, ElementField::Descr, attrs.get(3));
        self.finish_shape(at, kind, attrs, 4);
    }

    fn finish_shape(&mut self, at: usize, kind: ShapeKind, attrs: &[Attr], from: usize) {
        let shape = &mut self.shapes[at];
        put_element(&mut shape.element, ElementField::Sprite, attrs.get(from));
        put_element(&mut shape.element, ElementField::Tags, attrs.get(from + 1));
        put_element(&mut shape.element, ElementField::Link, attrs.get(from + 2));
        shape.kind = kind;
        shape.element.parent = Some(self.current);
    }

    fn boundary(&mut self, attrs: &[Attr], fallback: &str) -> usize {
        let at = Self::element(&mut self.boundaries, &mut self.boundary_index, &attrs[0], || Boundary {
            element: Element::default(),
            node_type: None,
        });
        let element = &mut self.boundaries[at].element;
        label(element, attrs.get(1));
        match attrs.get(2) {
            None => element.fields[ElementField::Type as usize] = Some(fallback.into()),
            given => put_element(element, ElementField::Type, given),
        }
        at
    }

    fn open(&mut self, at: usize) {
        self.boundaries[at].element.parent = Some(self.current);
        self.stack.push(self.current);
        self.current = at;
    }

    /// alias, label, ?type, ?tags, $link
    pub(super) fn add_person_or_system_boundary(&mut self, attrs: &[Attr], fallback: &str) {
        let at = self.boundary(attrs, fallback);
        let element = &mut self.boundaries[at].element;
        put_element(element, ElementField::Tags, attrs.get(3));
        put_element(element, ElementField::Link, attrs.get(4));
        self.open(at);
    }

    /// alias, label, ?type, ?descr, ?sprite, ?tags, $link
    pub(super) fn add_deployment_node(&mut self, node_type: NodeType, attrs: &[Attr]) {
        let at = self.boundary(attrs, "node");
        let boundary = &mut self.boundaries[at];
        text_element(&mut boundary.element, ElementField::Descr, attrs.get(3));
        put_element(&mut boundary.element, ElementField::Sprite, attrs.get(4));
        put_element(&mut boundary.element, ElementField::Tags, attrs.get(5));
        put_element(&mut boundary.element, ElementField::Link, attrs.get(6));
        boundary.node_type = Some(node_type);
        self.open(at);
    }

    pub(super) fn pop_boundary_parse_stack(&mut self) {
        self.current = self.stack.pop().unwrap_or(0);
    }

    /// from, to, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub(super) fn add_rel(&mut self, kind: RelType, attrs: &[Attr]) {
        let (Some(from), Some(to), Some(_)) = (attrs.first(), attrs.get(1), attrs.get(2)) else {
            return;
        };
        let known = match (from, to) {
            (Attr::Text(from), Attr::Text(to)) => Some((from, to)),
            _ => None,
        };
        let found = known.and_then(|(from, to)| self.rel_index.get(&**from).and_then(|targets| targets.get(&**to)));
        let at = match found {
            Some(&at) => at,
            None => {
                let at = self.rels.len();
                let mut rel = Rel {
                    kind,
                    from: JsText::default(),
                    to: JsText::default(),
                    label: JsText::default(),
                    from_attr: None,
                    to_attr: None,
                    label_attr: None,
                    fields: Default::default(),
                    offset_x: None,
                    offset_y: None,
                    extra: None,
                };
                if let Some((from, to)) = known {
                    self.rel_index.entry(from.clone()).or_default().insert(to.clone(), at);
                }
                match from {
                    Attr::Text(from) => rel.from = from.clone(),
                    Attr::Named(named) => rel.from_attr = Some(named.clone()),
                }
                match to {
                    Attr::Text(to) => rel.to = to.clone(),
                    Attr::Named(named) => rel.to_attr = Some(named.clone()),
                }
                self.rels.push(rel);
                at
            }
        };
        let rel = &mut self.rels[at];
        rel.kind = kind;
        label(rel, attrs.get(2));
        text_rel(rel, RelField::Techn, attrs.get(3));
        text_rel(rel, RelField::Descr, attrs.get(4));
        put_rel(rel, RelField::Sprite, attrs.get(5));
        put_rel(rel, RelField::Tags, attrs.get(6));
        put_rel(rel, RelField::Link, attrs.get(7));
    }

    fn find(&self, alias: &[u16]) -> Option<Found> {
        self.shape_index
            .get(alias)
            .map(|&at| Found::Shape(at))
            .or_else(|| self.boundary_index.get(alias).map(|&at| Found::Boundary(at)))
    }

    /// elementName, ?bgColor, ?fontColor, ?borderColor, ?shadowing, ?shape, ?sprite, ?techn,
    /// ?legendText, ?legendSprite
    pub(super) fn update_el_style(&mut self, attrs: &[Attr]) {
        let Some(Attr::Text(alias)) = attrs.first() else {
            return;
        };
        let element = match self.find(alias) {
            Some(Found::Shape(at)) => &mut self.shapes[at].element,
            Some(Found::Boundary(at)) => &mut self.boundaries[at].element,
            None => return,
        };
        use ElementField as F;
        let slots = [
            F::BgColor,
            F::FontColor,
            F::BorderColor,
            F::Shadowing,
            F::Shape,
            F::Sprite,
            F::Techn,
            F::LegendText,
            F::LegendSprite,
        ];
        for (slot, field) in slots.into_iter().enumerate() {
            put_element(element, field, attrs.get(slot + 1));
        }
    }

    /// from, to, ?textColor, ?lineColor, ?offsetX, ?offsetY
    pub(super) fn update_rel_style(&mut self, attrs: &[Attr]) {
        let (Some(Attr::Text(from)), Some(Attr::Text(to))) = (attrs.first(), attrs.get(1)) else {
            return;
        };
        let Some(&at) = self.rel_index.get(&**from).and_then(|targets| targets.get(&**to)) else {
            return;
        };
        let rel = &mut self.rels[at];
        put_rel(rel, RelField::TextColor, attrs.get(2));
        put_rel(rel, RelField::LineColor, attrs.get(3));
        match attrs.get(4) {
            Some(Attr::Text(x)) => rel.offset_x = Some(parse_int(x)),
            Some(Attr::Named(named)) => rel.named(named),
            None => {}
        }
        match attrs.get(5) {
            Some(Attr::Text(y)) => rel.offset_y = Some(parse_int(y)),
            Some(Attr::Named(named)) => rel.named(named),
            None => {}
        }
    }

    /// ?c4ShapeInRow, ?c4BoundaryInRow. As in Mermaid, the slot decides which one a value sets,
    /// not its name.
    pub(super) fn update_layout_config(&mut self, attrs: &[Attr]) {
        let count = |attr: Option<&Attr>| match attr {
            None => f64::NAN,
            Some(Attr::Text(text)) => parse_int(text),
            Some(Attr::Named(named)) => parse_int(&named.value),
        };
        let shapes = count(attrs.first());
        let boundaries = count(attrs.get(1));
        if shapes >= 1.0 {
            self.shape_in_row = shapes;
        }
        if boundaries >= 1.0 {
            self.boundary_in_row = boundaries;
        }
    }
}

impl AsMut<Element> for Shape {
    fn as_mut(&mut self) -> &mut Element {
        &mut self.element
    }
}

impl AsMut<Element> for Boundary {
    fn as_mut(&mut self) -> &mut Element {
        &mut self.element
    }
}
