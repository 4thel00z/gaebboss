//! Support for the generated readers: typed access to a tape node's
//! attributes and children, with every tolerated defect recorded.

use crate::decimal::Decimal;
use crate::name::Name;
use crate::span::Span;
use crate::tape::{Attr, NONE, NodeId, Tape};
use crate::warning::{Warning, WarningCode};

/// An enumerated simple type of the schema.
pub trait EnumValue: Sized + Copy {
    fn parse(text: &str) -> Option<Self>;
    fn as_str(self) -> &'static str;
}

pub struct Reader<'a> {
    pub tape: &'a Tape,
    pub input: &'a [u8],
    pub warnings: Vec<Warning>,
}

impl<'a> Reader<'a> {
    pub fn new(tape: &'a Tape, input: &'a [u8]) -> Reader<'a> {
        Reader {
            tape,
            input,
            warnings: Vec::new(),
        }
    }

    pub fn first_child(&self, node: NodeId) -> NodeId {
        self.tape
            .nodes
            .get(node as usize)
            .map_or(NONE, |n| n.first_child)
    }

    pub fn next(&self, node: NodeId) -> NodeId {
        self.tape.nodes.get(node as usize).map_or(NONE, |n| n.next)
    }

    pub fn name(&self, node: NodeId) -> Name {
        self.tape
            .nodes
            .get(node as usize)
            .map_or(Name::Unknown, |n| n.name)
    }

    pub fn text(&self, node: NodeId) -> Span {
        self.tape
            .nodes
            .get(node as usize)
            .map_or(Span::EMPTY, |n| n.text)
    }

    pub fn attr_count(&self, node: NodeId) -> usize {
        self.tape
            .nodes
            .get(node as usize)
            .map_or(0, |n| usize::from(n.attr_count))
    }

    pub fn attr_at(&self, node: NodeId, index: usize) -> Attr {
        let start = self
            .tape
            .nodes
            .get(node as usize)
            .map_or(0, |n| n.attrs as usize);
        self.tape.attrs.get(start + index).copied().unwrap_or(Attr {
            name: Name::Unknown,
            raw_name: Span::EMPTY,
            value: Span::EMPTY,
        })
    }

    fn str(&self, span: Span) -> &'a str {
        std::str::from_utf8(span.resolve(self.input, &self.tape.decoded)).unwrap_or("")
    }

    fn offset(&self, node: NodeId) -> usize {
        self.tape
            .nodes
            .get(node as usize)
            .map_or(0, |n| n.open.start())
    }

    fn element_name(&self, node: NodeId) -> &'a str {
        self.tape
            .nodes
            .get(node as usize)
            .map_or("", |n| self.str(n.raw_name))
    }

    fn warn(&mut self, code: WarningCode, node: NodeId, message: String) {
        self.warnings
            .push(Warning::new(code, self.offset(node), message));
    }

    fn duplicate(&mut self, parent: NodeId, child: NodeId) {
        let message = format!(
            "<{}> appears more than once in <{}>; the first one is kept",
            self.element_name(child),
            self.element_name(parent)
        );
        self.warn(WarningCode::Cardinality, child, message);
    }

    pub fn unexpected(&mut self, extra: &mut Vec<NodeId>, parent: NodeId, child: NodeId) {
        let message = format!(
            "<{}> is not a child of <{}> in the schema; kept verbatim",
            self.element_name(child),
            self.element_name(parent)
        );
        self.warn(WarningCode::UnknownElement, child, message);
        extra.push(child);
    }

    pub fn unknown_attr(&mut self, node: NodeId, attr: Attr) {
        let raw = self.str(attr.raw_name);
        if raw.starts_with("xmlns") || raw.starts_with("xsi:") {
            return;
        }
        let message = format!(
            "attribute {raw} is not defined on <{}>",
            self.element_name(node)
        );
        self.warn(WarningCode::UnknownAttribute, node, message);
    }

    fn decimal_value(&mut self, node: NodeId, text: &str) -> Option<Decimal> {
        match Decimal::parse(text.trim()) {
            Ok(d) => Some(d),
            Err(e) => {
                let message = format!(
                    "<{}> holds {text:?}, not a decimal: {e}",
                    self.element_name(node)
                );
                self.warn(WarningCode::BadDecimal, node, message);
                None
            }
        }
    }

    fn integer_value(&mut self, node: NodeId, text: &str) -> Option<i64> {
        match text.trim().parse::<i64>() {
            Ok(i) => Some(i),
            Err(_) => {
                let message = format!(
                    "<{}> holds {text:?}, not an integer",
                    self.element_name(node)
                );
                self.warn(WarningCode::BadInteger, node, message);
                None
            }
        }
    }

    fn bool_value(&mut self, node: NodeId, text: &str) -> Option<bool> {
        match text.trim() {
            "Yes" | "yes" | "true" | "1" => Some(true),
            "No" | "no" | "false" | "0" => Some(false),
            other => {
                let message = format!(
                    "<{}> holds {other:?}, not Yes or No",
                    self.element_name(node)
                );
                self.warn(WarningCode::BadEnum, node, message);
                None
            }
        }
    }

    fn enum_value<T: EnumValue>(&mut self, node: NodeId, text: &str) -> Option<T> {
        match T::parse(text.trim()) {
            Some(v) => Some(v),
            None => {
                let message = format!(
                    "<{}> holds {text:?}, which is not one of the allowed values",
                    self.element_name(node)
                );
                self.warn(WarningCode::BadEnum, node, message);
                None
            }
        }
    }

    pub fn set_span(&mut self, slot: &mut Option<Span>, parent: NodeId, child: NodeId) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        *slot = Some(self.text(child));
    }

    pub fn push_span(&mut self, list: &mut Vec<Span>, child: NodeId) {
        list.push(self.text(child));
    }

    pub fn set_decimal(&mut self, slot: &mut Option<Decimal>, parent: NodeId, child: NodeId) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        let text = self.str(self.text(child));
        *slot = self.decimal_value(child, text);
    }

    pub fn push_decimal(&mut self, list: &mut Vec<Decimal>, child: NodeId) {
        let text = self.str(self.text(child));
        if let Some(d) = self.decimal_value(child, text) {
            list.push(d);
        }
    }

    pub fn set_integer(&mut self, slot: &mut Option<i64>, parent: NodeId, child: NodeId) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        let text = self.str(self.text(child));
        *slot = self.integer_value(child, text);
    }

    pub fn push_integer(&mut self, list: &mut Vec<i64>, child: NodeId) {
        let text = self.str(self.text(child));
        if let Some(i) = self.integer_value(child, text) {
            list.push(i);
        }
    }

    pub fn set_bool(&mut self, slot: &mut Option<bool>, parent: NodeId, child: NodeId) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        let text = self.str(self.text(child));
        *slot = self.bool_value(child, text);
    }

    pub fn push_bool(&mut self, list: &mut Vec<bool>, child: NodeId) {
        let text = self.str(self.text(child));
        if let Some(b) = self.bool_value(child, text) {
            list.push(b);
        }
    }

    pub fn set_enum<T: EnumValue>(&mut self, slot: &mut Option<T>, parent: NodeId, child: NodeId) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        let text = self.str(self.text(child));
        *slot = self.enum_value(child, text);
    }

    pub fn push_enum<T: EnumValue>(&mut self, list: &mut Vec<T>, child: NodeId) {
        let text = self.str(self.text(child));
        if let Some(v) = self.enum_value(child, text) {
            list.push(v);
        }
    }

    pub fn set_complex<T>(
        &mut self,
        slot: &mut Option<T>,
        parent: NodeId,
        child: NodeId,
        read: fn(&mut Reader<'a>, NodeId) -> T,
    ) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        *slot = Some(read(self, child));
    }

    pub fn set_boxed<T>(
        &mut self,
        slot: &mut Option<Box<T>>,
        parent: NodeId,
        child: NodeId,
        read: fn(&mut Reader<'a>, NodeId) -> T,
    ) {
        if slot.is_some() {
            return self.duplicate(parent, child);
        }
        *slot = Some(Box::new(read(self, child)));
    }

    pub fn set_attr_span(&mut self, slot: &mut Option<Span>, _node: NodeId, attr: Attr) {
        *slot = Some(attr.value);
    }

    pub fn set_attr_decimal(&mut self, slot: &mut Option<Decimal>, node: NodeId, attr: Attr) {
        let text = self.str(attr.value);
        *slot = self.decimal_value(node, text);
    }

    pub fn set_attr_integer(&mut self, slot: &mut Option<i64>, node: NodeId, attr: Attr) {
        let text = self.str(attr.value);
        *slot = self.integer_value(node, text);
    }

    pub fn set_attr_bool(&mut self, slot: &mut Option<bool>, node: NodeId, attr: Attr) {
        let text = self.str(attr.value);
        *slot = self.bool_value(node, text);
    }

    pub fn set_attr_enum<T: EnumValue>(&mut self, slot: &mut Option<T>, node: NodeId, attr: Attr) {
        let text = self.str(attr.value);
        *slot = self.enum_value(node, text);
    }
}
