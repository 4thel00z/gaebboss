//! The flat tape the tokenizer produces: one node per element with its
//! spans, links to parent, first child and next sibling, and attributes.

use crate::name::Name;
use crate::span::Span;
use crate::warning::Warning;

pub type NodeId = u32;

/// The absent link.
pub const NONE: NodeId = u32::MAX;

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Name,
    /// The local element name as written.
    pub raw_name: Span,
    /// The start tag, `<` to `>`.
    pub open: Span,
    /// Everything between the start and end tags.
    pub inner: Span,
    /// Where the end tag ends; the element is `open.start()..end`.
    pub end: u32,
    pub parent: NodeId,
    pub next: NodeId,
    pub first_child: NodeId,
    pub attrs: u32,
    pub attr_count: u16,
    /// Direct character data, concatenated; whitespace between child
    /// elements is not text.
    pub text: Span,
    pub pieces: u32,
    pub piece_count: u16,
}

impl Node {
    pub fn outer(&self) -> Span {
        Span::input(self.open.start(), self.end as usize)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Attr {
    pub name: Name,
    pub raw_name: Span,
    pub value: Span,
}

#[derive(Debug, Default)]
pub struct Tape {
    pub nodes: Vec<Node>,
    pub attrs: Vec<Attr>,
    pub pieces: Vec<Span>,
    pub decoded: Vec<u8>,
    pub root: NodeId,
    pub default_ns: Span,
    pub prefix: Span,
    pub warnings: Vec<Warning>,
}

impl Tape {
    /// The text behind a span; empty when the span is not valid UTF-8.
    pub fn str<'a>(&'a self, input: &'a [u8], span: Span) -> &'a str {
        std::str::from_utf8(span.resolve(input, &self.decoded)).unwrap_or("")
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn children(&self, id: NodeId) -> Children<'_> {
        let next = self.node(id).map_or(NONE, |n| n.first_child);
        Children { tape: self, next }
    }

    /// The first child with this name.
    pub fn child(&self, id: NodeId, name: Name) -> Option<NodeId> {
        self.children(id)
            .find(|c| self.nodes[*c as usize].name == name)
    }

    pub fn attrs(&self, id: NodeId) -> &[Attr] {
        let Some(node) = self.node(id) else {
            return &[];
        };
        let start = node.attrs as usize;
        self.attrs
            .get(start..start + node.attr_count as usize)
            .unwrap_or(&[])
    }

    pub fn attr(&self, id: NodeId, name: Name) -> Option<Span> {
        self.attrs(id)
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value)
    }

    /// The direct text pieces of a node in document order, one per run of
    /// character data between child elements.
    pub fn text_pieces(&self, id: NodeId) -> impl Iterator<Item = Span> + '_ {
        let (start, count) = self
            .node(id)
            .map_or((0, 0), |n| (n.pieces as usize, n.piece_count as usize));
        self.pieces
            .get(start..start + count)
            .unwrap_or(&[])
            .iter()
            .copied()
    }
}

pub struct Children<'a> {
    tape: &'a Tape,
    next: NodeId,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if self.next == NONE {
            return None;
        }
        let current = self.next;
        self.next = self
            .tape
            .nodes
            .get(current as usize)
            .map_or(NONE, |n| n.next);
        Some(current)
    }
}
