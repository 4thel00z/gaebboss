//! hexyl-style hexdump: offset gutter, hex columns, ascii column, byte-class
//! coloring and labeled element boundaries, plus `gaebboss hex` itself.

use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::ops::Range;

use gaebboss_core::Document;
use gaebboss_core::boq::Entry;
use gaebboss_core::oz::Oz;

pub struct HexOpts {
    pub width: usize,
    pub color: bool,
}

/// A labeled boundary printed when the dump reaches `offset`.
pub struct Mark {
    pub offset: usize,
    pub label: String,
}

#[derive(Clone, Copy)]
enum ByteClass {
    Null,
    Printable,
    Whitespace,
    Other,
}

fn classify(b: u8) -> ByteClass {
    match b {
        0 => ByteClass::Null,
        b'\t' | b'\n' | b'\r' | 0x0B | 0x0C | b' ' => ByteClass::Whitespace,
        0x21..=0x7E => ByteClass::Printable,
        _ => ByteClass::Other,
    }
}

fn color_code(class: ByteClass) -> &'static str {
    match class {
        ByteClass::Null => "\x1b[90m",
        ByteClass::Printable => "\x1b[36m",
        ByteClass::Whitespace => "\x1b[32m",
        ByteClass::Other => "\x1b[33m",
    }
}

fn ascii_char(b: u8) -> char {
    match classify(b) {
        ByteClass::Printable => b as char,
        ByteClass::Whitespace if b == b' ' => ' ',
        ByteClass::Null | ByteClass::Whitespace | ByteClass::Other => '.',
    }
}

pub fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && io::stdout().is_terminal()
}

/// Dumps `bytes` as if the first one sat at `base_offset`, printing a
/// `── label ──` line before the row that contains each mark. `marks` must be
/// sorted by offset; marks outside the dumped range are dropped.
pub fn hexdump(
    w: &mut impl io::Write,
    bytes: &[u8],
    base_offset: usize,
    opts: &HexOpts,
    marks: &[Mark],
) -> io::Result<()> {
    let width = opts.width.max(1);
    let end_offset = base_offset + bytes.len();
    let mut marks = marks
        .iter()
        .filter(|m| m.offset >= base_offset && m.offset <= end_offset)
        .peekable();
    for (index, row) in bytes.chunks(width).enumerate() {
        let row_offset = base_offset + index * width;
        while let Some(mark) = marks.next_if(|m| m.offset < row_offset + row.len()) {
            writeln!(w, "── {} ──", mark.label)?;
        }
        write_row(w, row, row_offset, width, opts.color)?;
    }
    for mark in marks {
        writeln!(w, "── {} ──", mark.label)?;
    }
    Ok(())
}

fn write_row(
    w: &mut impl io::Write,
    row: &[u8],
    offset: usize,
    width: usize,
    color: bool,
) -> io::Result<()> {
    const RESET: &str = "\x1b[0m";
    let mut hex = String::new();
    let mut ascii = String::new();
    for (i, &b) in row.iter().enumerate() {
        if i > 0 && i % 8 == 0 {
            hex.push(' ');
        }
        if !color {
            let _ = write!(hex, "{b:02x} ");
            ascii.push(ascii_char(b));
            continue;
        }
        let code = color_code(classify(b));
        let _ = write!(hex, "{code}{b:02x}{RESET} ");
        let _ = write!(ascii, "{code}{}{RESET}", ascii_char(b));
    }
    for i in row.len()..width {
        if i > 0 && i % 8 == 0 {
            hex.push(' ');
        }
        hex.push_str("   ");
    }
    writeln!(w, "{offset:08x}  {hex} |{ascii}|")
}

/// What part of the document `gaebboss hex` dumps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Whole,
    Oz(String),
    Id(String),
    Range { start: usize, end: usize },
}

/// Parses `oz:01.0010` / `id:ID02BED3D8` / `range:0x1A40-0x1B02` (offsets
/// decimal or 0x-prefixed hex).
pub fn parse_selector(s: &str) -> Result<Selector, String> {
    if let Some(oz) = s.strip_prefix("oz:") {
        return Ok(Selector::Oz(oz.trim().to_string()));
    }
    if let Some(id) = s.strip_prefix("id:") {
        return Ok(Selector::Id(id.trim().to_string()));
    }
    let Some(rest) = s.strip_prefix("range:") else {
        return Err(format!(
            "unknown selector {s:?}; expected oz:OZ, id:ID or range:START-END"
        ));
    };
    let (start, end) = rest
        .split_once('-')
        .ok_or_else(|| format!("range selector must look like range:0x1A40-0x1B02, got {s:?}"))?;
    let start = parse_offset(start).ok_or_else(|| format!("bad start offset in selector {s:?}"))?;
    let end = parse_offset(end).ok_or_else(|| format!("bad end offset in selector {s:?}"))?;
    if end < start {
        return Err(format!("range end lies before its start in selector {s:?}"));
    }
    Ok(Selector::Range { start, end })
}

fn parse_offset(text: &str) -> Option<usize> {
    let text = text.trim();
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => usize::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

fn label(doc: &Document, entry: &Entry<'_>) -> String {
    let kind = match entry {
        Entry::Category(_) => "BoQCtgy",
        Entry::Item(_) => "Item",
        Entry::Markup(_) => "MarkupItem",
        Entry::Remark(_) => "Remark",
    };
    let oz = entry.oz().map(Oz::dotted).unwrap_or_default();
    [kind, oz.as_str(), entry.id(doc)]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The bytes a selector names, as a range of [`Document::bytes`].
pub fn resolve(doc: &Document, selector: &Selector) -> Result<Range<usize>, String> {
    let len = doc.bytes().len();
    let entry_range = |found: Option<Entry<'_>>, what: String| {
        found
            .and_then(|entry| doc.element_range(entry.node()))
            .ok_or_else(|| format!("no element {what}"))
    };
    let range = match selector {
        Selector::Whole => 0..len,
        Selector::Range { start, end } => *start..*end,
        Selector::Oz(oz) => entry_range(
            doc.entries().find(|e| {
                e.oz()
                    .is_some_and(|entry_oz| entry_oz.dotted() == *oz || entry_oz.compact() == *oz)
            }),
            format!("with Ordnungszahl {oz}"),
        )?,
        Selector::Id(id) => entry_range(
            doc.entries().find(|e| e.id(doc) == id),
            format!("with ID {id}"),
        )?,
    };
    if range.end > len {
        return Err(format!(
            "selector resolved to {}..{}, which lies outside the document ({len} bytes)",
            range.start, range.end
        ));
    }
    Ok(range)
}

/// One mark per category, item, markup item and remark, in offset order.
pub fn element_marks(doc: &Document) -> Vec<Mark> {
    let mut marks: Vec<Mark> = doc
        .entries()
        .filter_map(|entry| {
            let range = doc.element_range(entry.node())?;
            Some(Mark {
                offset: range.start,
                label: label(doc, &entry),
            })
        })
        .collect();
    marks.sort_by_key(|m| m.offset);
    marks
}

pub fn cmd_hex(
    doc: &Document,
    selector: Option<&str>,
    annotate: bool,
    width: usize,
) -> Result<(), String> {
    if width == 0 {
        return Err("--width must be at least 1".to_string());
    }
    let selector = selector.map_or(Ok(Selector::Whole), parse_selector)?;
    let range = resolve(doc, &selector)?;
    let bytes = doc.bytes().get(range.clone()).unwrap_or(&[]);
    let opts = HexOpts {
        width,
        color: use_color(),
    };
    let marks = match annotate {
        true => element_marks(doc),
        false => Vec::new(),
    };
    let stdout = io::stdout();
    let mut w = io::BufWriter::new(stdout.lock());
    hexdump(&mut w, bytes, range.start, &opts, &marks).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(bytes: &[u8], base: usize, width: usize, marks: &[Mark]) -> String {
        let mut out = Vec::new();
        hexdump(
            &mut out,
            bytes,
            base,
            &HexOpts {
                width,
                color: false,
            },
            marks,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn rows_carry_offset_hex_and_ascii() {
        let text = plain(b"<Item ID=\"a\">\n", 0x10, 8, &[]);
        assert_eq!(
            text,
            "00000010  3c 49 74 65 6d 20 49 44  |<Item ID|\n00000018  3d 22 61 22 3e 0a        |=\"a\">.|\n"
        );
    }

    #[test]
    fn marks_print_before_their_row_and_after_the_end() {
        let marks = [
            Mark {
                offset: 4,
                label: "first".to_string(),
            },
            Mark {
                offset: 8,
                label: "end".to_string(),
            },
        ];
        let text = plain(b"abcdefgh", 0, 4, &marks);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("00000000"));
        assert_eq!(lines[1], "── first ──");
        assert!(lines[2].starts_with("00000004"));
        assert_eq!(lines[3], "── end ──");
    }

    #[test]
    fn selectors_parse() {
        assert_eq!(
            parse_selector("oz:01.0010"),
            Ok(Selector::Oz("01.0010".to_string()))
        );
        assert_eq!(parse_selector("id:X1"), Ok(Selector::Id("X1".to_string())));
        assert_eq!(
            parse_selector("range:0x10-32"),
            Ok(Selector::Range { start: 16, end: 32 })
        );
        assert!(parse_selector("range:9-1").is_err());
        assert!(parse_selector("obj:1").is_err());
    }

    #[test]
    fn oz_and_id_resolve_to_the_element() {
        let path = format!(
            "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = Document::open(std::path::Path::new(&path)).unwrap();
        let by_oz = resolve(&doc, &Selector::Oz("001.001.0010".to_string())).unwrap();
        let by_id = resolve(&doc, &Selector::Id("ID02BED3D8".to_string())).unwrap();
        assert_eq!(by_oz, by_id);
        let element = std::str::from_utf8(&doc.bytes()[by_oz]).unwrap();
        assert!(element.starts_with("<Item"), "{element}");
        assert!(element.ends_with("</Item>"), "{element}");
        assert!(resolve(&doc, &Selector::Oz("nope".to_string())).is_err());
        assert!(
            element_marks(&doc)
                .windows(2)
                .all(|w| w[0].offset <= w[1].offset)
        );
    }
}
