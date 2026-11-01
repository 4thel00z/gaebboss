//! Writing a document back as GAEB DA XML in a target version and phase:
//! elements in schema order, the phase profile applied, unknown elements
//! copied verbatim when the version stays the same.

use std::collections::HashMap;

use crate::Document;
use crate::decimal::Decimal;
use crate::generated::profile::{Restriction, base_restrictions, restrictions};
use crate::generated::write::write_gaeb;
use crate::name::Name;
use crate::read::EnumValue;
use crate::span::Span;
use crate::tape::NodeId;
use crate::version::{Phase, Version};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOptions {
    pub version: Version,
    pub phase: Phase,
    /// Spaces per level; `None` writes everything on one line.
    pub indent: Option<u8>,
    /// Replaces `GAEBInfo/ProgSystem` when set.
    pub prog_system: Option<String>,
    /// Refuse to write when an element the target schema requires is
    /// missing; otherwise it is written anyway and reported.
    pub strict: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("cannot write phase {} in version {}", phase.code(), version.as_str())]
    UnsupportedTarget { version: Version, phase: Phase },
    #[error("<{type_name}> at node {node} lacks the required <{}>", element.as_str())]
    MissingRequired {
        type_name: &'static str,
        element: Name,
        node: NodeId,
    },
}

/// An element the target version or phase does not carry, left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dropped {
    pub type_name: &'static str,
    pub element: String,
    pub count: usize,
}

/// An element the target schema requires that the document lacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missing {
    pub type_name: &'static str,
    pub element: Name,
    pub node: NodeId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteReport {
    pub dropped: Vec<Dropped>,
    pub missing: Vec<Missing>,
}

impl Version {
    /// The `VersDate` of the version's schema release.
    pub fn vers_date(self) -> &'static str {
        match self {
            Version::V30 => "2004-08",
            Version::V31 => "2009-12",
            Version::V32 => "2013-10",
            Version::V33 => "2021-05",
        }
    }
}

impl Document {
    /// The document as GAEB DA XML in the target version and phase.
    pub fn write(&self, opts: &WriteOptions) -> Result<Vec<u8>, WriteError> {
        self.write_with_report(opts).map(|(bytes, _)| bytes)
    }

    /// The bytes plus what the target profile left out or found missing.
    pub fn write_with_report(
        &self,
        opts: &WriteOptions,
    ) -> Result<(Vec<u8>, WriteReport), WriteError> {
        let Some(table) = restrictions(opts.version, opts.phase) else {
            return Err(WriteError::UnsupportedTarget {
                version: opts.version,
                phase: opts.phase,
            });
        };
        let base = base_restrictions(opts.version).unwrap_or(&[]);
        let mut w = Writer::new(self, opts, table, base);
        w.out
            .extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        write_gaeb(&mut w, &self.gaeb, "GAEB")?;
        if opts.indent.is_some() {
            w.out.push(b'\n');
        }
        let report = w.report();
        Ok((w.out, report))
    }
}

const TEXT_LEVEL_TAGS: &[&str] = &[
    "p", "div", "span", "li", "td", "th", "b", "i", "u", "Image", "image",
];

pub struct Writer<'a> {
    doc: &'a Document,
    opts: &'a WriteOptions,
    table: &'static [(&'static str, Restriction)],
    base: &'static [(&'static str, Restriction)],
    cache: HashMap<&'static str, Option<&'static Restriction>>,
    pub out: Vec<u8>,
    depth: usize,
    had_children: Vec<bool>,
    /// Depth at which a text-level element (p, span, td ...) opened; no
    /// indentation is written below it, since whitespace there is text.
    text_level: Option<usize>,
    dropped: HashMap<(&'static str, String), usize>,
    missing: Vec<Missing>,
    /// The open element is a `LblUPCompN` without its 3.3 `Type` attribute yet.
    needs_up_comp_type: bool,
}

impl<'a> Writer<'a> {
    fn new(
        doc: &'a Document,
        opts: &'a WriteOptions,
        table: &'static [(&'static str, Restriction)],
        base: &'static [(&'static str, Restriction)],
    ) -> Writer<'a> {
        Writer {
            doc,
            opts,
            table,
            base,
            cache: HashMap::new(),
            out: Vec::with_capacity(doc.bytes().len()),
            depth: 0,
            had_children: Vec::new(),
            text_level: None,
            dropped: HashMap::new(),
            missing: Vec::new(),
            needs_up_comp_type: false,
        }
    }

    fn report(&self) -> WriteReport {
        let mut dropped: Vec<Dropped> = self
            .dropped
            .iter()
            .map(|((t, e), n)| Dropped {
                type_name: t,
                element: e.clone(),
                count: *n,
            })
            .collect();
        dropped.sort_by(|a, b| (a.type_name, &a.element).cmp(&(b.type_name, &b.element)));
        WriteReport {
            dropped,
            missing: self.missing.clone(),
        }
    }

    pub fn version(&self) -> Version {
        self.opts.version
    }

    pub fn restriction(&mut self, type_name: &'static str) -> Option<&'static Restriction> {
        if let Some(found) = self.cache.get(type_name) {
            return *found;
        }
        let found = self
            .table
            .iter()
            .chain(self.base.iter())
            .find(|(name, _)| *name == type_name)
            .map(|(_, r)| r);
        self.cache.insert(type_name, found);
        found
    }

    pub fn required(&self, restriction: Option<&'static Restriction>) -> &'static [Name] {
        restriction.map_or(&[], |r| r.required)
    }

    pub fn required_any(
        &self,
        restriction: Option<&'static Restriction>,
    ) -> &'static [&'static [Name]] {
        restriction.map_or(&[], |r| r.required_any)
    }

    pub fn allowed(&self, restriction: Option<&'static Restriction>, element: Name) -> bool {
        restriction.is_none_or(|r| r.allowed.contains(&element))
    }

    pub fn dropped(&mut self, type_name: &'static str, element: &str) {
        *self
            .dropped
            .entry((type_name, element.to_string()))
            .or_default() += 1;
    }

    /// Records a required element that is absent; an error when strict.
    pub fn missing(
        &mut self,
        type_name: &'static str,
        element: Name,
        node: NodeId,
    ) -> Result<(), WriteError> {
        if self.opts.strict {
            return Err(WriteError::MissingRequired {
                type_name,
                element,
                node,
            });
        }
        self.missing.push(Missing {
            type_name,
            element,
            node,
        });
        Ok(())
    }

    fn newline(&mut self) {
        let Some(indent) = self.opts.indent else {
            return;
        };
        if self.text_level.is_some() {
            return;
        }
        self.out.push(b'\n');
        for _ in 0..self.depth * usize::from(indent) {
            self.out.push(b' ');
        }
    }

    fn mark_child(&mut self) {
        if let Some(parent) = self.had_children.last_mut() {
            *parent = true;
        }
    }

    pub fn open(&mut self, tag: &str) {
        self.mark_child();
        self.newline();
        if self.text_level.is_none() && TEXT_LEVEL_TAGS.contains(&tag) {
            self.text_level = Some(self.depth);
        }
        self.out.push(b'<');
        self.out.extend_from_slice(tag.as_bytes());
        self.needs_up_comp_type = self.opts.version == Version::V33 && tag.starts_with("LblUPComp");
        if self.depth == 0
            && let Some(ns) = self.opts.phase.namespace(self.opts.version)
        {
            self.out.extend_from_slice(b" xmlns=\"");
            self.out.extend_from_slice(ns.as_bytes());
            self.out.push(b'"');
        }
        self.depth += 1;
        self.had_children.push(false);
    }

    /// Closes the start tag. A unit price component label from a file
    /// older than 3.3 gets the `Type` the 3.3 schema requires, with the
    /// standard's own catch-all value.
    pub fn open_end(&mut self) {
        if self.needs_up_comp_type {
            self.out.extend_from_slice(b" Type=\"Unknown\"");
            self.needs_up_comp_type = false;
        }
        self.out.push(b'>');
    }

    pub fn close(&mut self, tag: &str) {
        self.depth -= 1;
        let had_children = self.had_children.pop().unwrap_or(false);
        if had_children {
            self.newline();
        }
        if self.text_level == Some(self.depth) {
            self.text_level = None;
        }
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b'>');
    }

    fn attr(&mut self, name: &str, value: &str) {
        if name == "Type" {
            self.needs_up_comp_type = false;
        }
        self.out.push(b' ');
        self.out.extend_from_slice(name.as_bytes());
        self.out.extend_from_slice(b"=\"");
        escape_into(value, true, &mut self.out);
        self.out.push(b'"');
    }

    pub fn attr_span(&mut self, name: &str, value: Span) {
        let text = self.doc.str(value).to_string();
        self.attr(name, &text);
    }

    pub fn attr_decimal(&mut self, name: &str, value: Decimal) {
        self.attr(name, &value.to_string());
    }

    pub fn attr_integer(&mut self, name: &str, value: i64) {
        self.attr(name, &value.to_string());
    }

    pub fn attr_bool(&mut self, name: &str, value: bool) {
        self.attr(name, yes_no(value));
    }

    pub fn attr_enum<T: EnumValue>(&mut self, name: &str, value: T) {
        self.attr(name, value.as_str());
    }

    pub fn text(&mut self, span: Span) {
        let text = self.doc.str(span);
        escape_into(text, false, &mut self.out);
    }

    fn leaf(&mut self, tag: &str, text: &str) {
        self.mark_child();
        self.newline();
        self.out.push(b'<');
        self.out.extend_from_slice(tag.as_bytes());
        if text.is_empty() {
            self.out.extend_from_slice(b"/>");
            return;
        }
        self.out.push(b'>');
        escape_into(text, false, &mut self.out);
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(tag.as_bytes());
        self.out.push(b'>');
    }

    /// A text element; the header fields that name version and phase are
    /// rewritten for the target.
    pub fn span_elem(&mut self, tag: &str, value: Span) {
        let substituted: Option<String> = match tag {
            "Version" => Some(self.opts.version.as_str().to_string()),
            "VersDate" => Some(self.opts.version.vers_date().to_string()),
            "DP" => Some(self.opts.phase.code().to_string()),
            "ProgSystem" => self.opts.prog_system.clone(),
            _ => None,
        };
        match substituted {
            Some(text) => self.leaf(tag, &text),
            None => {
                let text = self.doc.str(value).to_string();
                self.leaf(tag, &text);
            }
        }
    }

    pub fn decimal_elem(&mut self, tag: &str, value: Decimal) {
        self.leaf(tag, &value.to_string());
    }

    pub fn integer_elem(&mut self, tag: &str, value: i64) {
        self.leaf(tag, &value.to_string());
    }

    pub fn bool_elem(&mut self, tag: &str, value: bool) {
        self.leaf(tag, yes_no(value));
    }

    pub fn enum_elem(&mut self, tag: &str, value: &str) {
        self.leaf(tag, value);
    }

    /// Unknown children copied verbatim when the version stays the same,
    /// otherwise left out and reported. A prefixed element takes the
    /// namespace declarations it needs along, since the source root that
    /// declared them is not copied.
    pub fn extra(&mut self, nodes: &[NodeId], type_name: &'static str) {
        for node in nodes {
            let Some(n) = self.doc.tape().node(*node) else {
                continue;
            };
            let raw_name = self.doc.str(n.raw_name).to_string();
            if self.opts.version != self.doc.version() {
                self.dropped(type_name, &raw_name);
                continue;
            }
            self.mark_child();
            self.newline();
            let outer = n.outer();
            let bytes = &self.doc.bytes()[outer.start()..outer.end()];
            let declarations = self.namespace_declarations(bytes);
            if declarations.is_empty() {
                self.out.extend_from_slice(bytes);
                continue;
            }
            let name_end = bytes
                .iter()
                .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>'))
                .unwrap_or(bytes.len());
            self.out.extend_from_slice(&bytes[..name_end]);
            for (prefix, uri) in declarations {
                self.out.extend_from_slice(b" xmlns:");
                self.out.extend_from_slice(prefix.as_bytes());
                self.out.extend_from_slice(b"=\"");
                escape_into(&uri, true, &mut self.out);
                self.out.push(b'"');
            }
            self.out.extend_from_slice(&bytes[name_end..]);
        }
    }

    /// The `xmlns:prefix` declarations an element's raw bytes rely on but do
    /// not carry themselves, looked up on the source root.
    fn namespace_declarations(&self, bytes: &[u8]) -> Vec<(String, String)> {
        let mut prefixes: Vec<String> = Vec::new();
        let mut at = 0;
        while let Some(lt) = memchr::memchr(b'<', &bytes[at..]) {
            let start = at + lt + 1;
            let name: Vec<u8> = bytes[start..]
                .iter()
                .take_while(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>'))
                .copied()
                .collect();
            let tag = !name.starts_with(b"/") && !name.starts_with(b"!") && !name.starts_with(b"?");
            if tag && let Some(colon) = name.iter().position(|b| *b == b':') {
                let prefix = String::from_utf8_lossy(&name[..colon]).to_string();
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
            at = start;
        }
        let tape = self.doc.tape();
        let mut out = Vec::new();
        for prefix in prefixes {
            let needle = format!("xmlns:{prefix}=");
            if bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                continue;
            }
            let declared = tape
                .attrs(tape.root)
                .iter()
                .find(|a| self.doc.str(a.raw_name) == format!("xmlns:{prefix}"));
            let Some(attr) = declared else { continue };
            out.push((prefix, self.doc.str(attr.value).to_string()));
        }
        out
    }
}

fn yes_no(value: bool) -> &'static str {
    match value {
        true => "Yes",
        false => "No",
    }
}

fn escape_into(text: &str, attribute: bool, out: &mut Vec<u8>) {
    for b in text.bytes() {
        match b {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'"' if attribute => out.extend_from_slice(b"&quot;"),
            _ => out.push(b),
        }
    }
}
