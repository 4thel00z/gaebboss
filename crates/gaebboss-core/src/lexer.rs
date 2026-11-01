//! GAEB-specialised XML tokenizer: builds the tape in one pass over the
//! normalized UTF-8 input.

use memchr::{memchr, memmem};

use crate::error::{Error, Result};
use crate::name::Name;
use crate::span::{MAX_INPUT, Span};
use crate::tape::{Attr, NONE, Node, NodeId, Tape};
use crate::warning::{Warning, WarningCode};

struct Frame {
    node: NodeId,
    last_child: NodeId,
    piece_start: usize,
}

struct Lexer<'a> {
    input: &'a [u8],
    tape: Tape,
    stack: Vec<Frame>,
    scratch: Vec<Span>,
    pos: usize,
}

/// Tokenizes normalized UTF-8 input into a tape.
pub fn lex(input: &[u8]) -> Result<Tape> {
    if input.len() > MAX_INPUT {
        return Err(Error::Encoding(format!(
            "input larger than {MAX_INPUT} bytes"
        )));
    }
    if memchr(b'<', input).is_none() {
        return Err(Error::NotXml);
    }
    let mut lexer = Lexer {
        input,
        tape: Tape {
            root: NONE,
            ..Tape::default()
        },
        stack: Vec::with_capacity(32),
        scratch: Vec::with_capacity(16),
        pos: 0,
    };
    lexer.run();
    if lexer.tape.root == NONE {
        return Err(Error::NoGaebRoot);
    }
    Ok(lexer.tape)
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        let input = self.input;
        while self.pos < input.len() {
            let Some(lt) = memchr(b'<', &input[self.pos..]) else {
                self.pos = input.len();
                break;
            };
            let lt = self.pos + lt;
            if lt > self.pos {
                self.text(self.pos, lt);
            }
            match input.get(lt + 1) {
                Some(b'?') => self.pos = self.skip_to(lt + 2, b"?>"),
                Some(b'!') => self.pos = self.declaration(lt),
                Some(b'/') => self.pos = self.end_tag(lt),
                Some(_) => self.pos = self.start_tag(lt),
                None => self.pos = input.len(),
            }
            if self.tape.root != NONE && self.stack.is_empty() {
                break;
            }
        }
        while let Some(frame) = self.stack.pop() {
            let end = input.len();
            self.tape.warnings.push(Warning::new(
                WarningCode::UnclosedElement,
                end,
                format!(
                    "element <{}> was not closed before the end of the file",
                    self.name_of(frame.node)
                ),
            ));
            self.close(frame, end, end);
        }
    }

    fn name_of(&self, node: NodeId) -> String {
        self.tape
            .str(self.input, self.tape.nodes[node as usize].raw_name)
            .to_string()
    }

    fn skip_to(&self, from: usize, marker: &[u8]) -> usize {
        match memmem::find(&self.input[from..], marker) {
            Some(at) => from + at + marker.len(),
            None => self.input.len(),
        }
    }

    fn declaration(&mut self, lt: usize) -> usize {
        let rest = &self.input[lt..];
        if rest.starts_with(b"<!--") {
            return self.skip_to(lt + 4, b"-->");
        }
        if rest.starts_with(b"<![CDATA[") {
            let start = lt + 9;
            let end = match memmem::find(&self.input[start..], b"]]>") {
                Some(at) => start + at,
                None => self.input.len(),
            };
            self.piece(Span::input(start, end), true);
            return (end + 3).min(self.input.len());
        }
        let mut i = lt;
        let mut depth = 0i32;
        while i < self.input.len() {
            match self.input[i] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                b'>' if depth <= 0 => return i + 1,
                _ => {}
            }
            i += 1;
        }
        self.input.len()
    }

    fn text(&mut self, start: usize, end: usize) {
        if self.stack.is_empty() {
            return;
        }
        self.piece(Span::input(start, end), false);
    }

    /// Records a run of character data for the open element, decoding
    /// entities into the side buffer when there are any.
    fn piece(&mut self, span: Span, literal: bool) {
        if self.stack.is_empty() {
            return;
        }
        let bytes = &self.input[span.start()..span.end()];
        if literal || memchr(b'&', bytes).is_none() {
            self.scratch.push(span);
            return;
        }
        let start = self.tape.decoded.len();
        decode_entities(
            bytes,
            span.start(),
            &mut self.tape.decoded,
            &mut self.tape.warnings,
        );
        self.scratch
            .push(Span::decoded(start, self.tape.decoded.len()));
    }

    fn start_tag(&mut self, lt: usize) -> usize {
        let input = self.input;
        let name_start = lt + 1;
        let mut i = name_start;
        while i < input.len() && !matches!(input[i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
            i += 1;
        }
        let (prefix, local) = split_prefix(name_start, i, &input[name_start..i]);
        let attrs_start = self.tape.attrs.len();
        let mut self_closing = false;
        let mut end = input.len();
        while i < input.len() {
            match input[i] {
                b' ' | b'\t' | b'\r' | b'\n' => i += 1,
                b'/' => {
                    self_closing = true;
                    i += 1;
                }
                b'>' => {
                    end = i + 1;
                    break;
                }
                _ => i = self.attribute(i),
            }
        }
        let node_id = self.tape.nodes.len() as NodeId;
        let parent = self.stack.last().map_or(NONE, |f| f.node);
        let raw_name = Span::input(local.0, local.1);
        let node = Node {
            name: Name::lookup(&input[local.0..local.1]),
            raw_name,
            open: Span::input(lt, end),
            inner: Span::input(end, end),
            end: end as u32,
            parent,
            next: NONE,
            first_child: NONE,
            attrs: attrs_start as u32,
            attr_count: (self.tape.attrs.len() - attrs_start).min(u16::MAX as usize) as u16,
            text: Span::EMPTY,
            pieces: 0,
            piece_count: 0,
        };
        self.tape.nodes.push(node);
        if let Some(frame) = self.stack.last_mut() {
            match frame.last_child {
                NONE => self.tape.nodes[frame.node as usize].first_child = node_id,
                last => self.tape.nodes[last as usize].next = node_id,
            }
            frame.last_child = node_id;
        }
        if self.tape.root == NONE {
            self.root(node_id, prefix);
        }
        let frame = Frame {
            node: node_id,
            last_child: NONE,
            piece_start: self.scratch.len(),
        };
        match self_closing {
            true => self.close(frame, end, end),
            false => self.stack.push(frame),
        }
        end
    }

    fn root(&mut self, node_id: NodeId, prefix: Option<(usize, usize)>) {
        let node = &self.tape.nodes[node_id as usize];
        if node.name != Name::GAEB {
            return;
        }
        self.tape.root = node_id;
        let wanted: &[u8] = match prefix {
            Some((s, e)) => &self.input[s..e],
            None => b"",
        };
        self.tape.prefix = prefix.map_or(Span::EMPTY, |(s, e)| Span::input(s, e));
        for attr in self.tape.attrs(node_id) {
            let raw = attr.raw_name.resolve(self.input, &[]);
            let matches = match wanted.is_empty() {
                true => raw == b"xmlns",
                false => raw.strip_prefix(b"xmlns:") == Some(wanted),
            };
            if matches {
                self.tape.default_ns = attr.value;
                return;
            }
        }
    }

    /// Parses one `name="value"` at `i`; returns the position after it.
    fn attribute(&mut self, i: usize) -> usize {
        let input = self.input;
        let name_start = i;
        let mut j = i;
        while j < input.len()
            && !matches!(input[j], b'=' | b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')
        {
            j += 1;
        }
        let name_end = j;
        while j < input.len() && matches!(input[j], b' ' | b'\t' | b'\r' | b'\n') {
            j += 1;
        }
        if input.get(j) != Some(&b'=') {
            return j.max(name_start + 1);
        }
        j += 1;
        while j < input.len() && matches!(input[j], b' ' | b'\t' | b'\r' | b'\n') {
            j += 1;
        }
        let Some(&quote) = input.get(j) else { return j };
        if quote != b'"' && quote != b'\'' {
            return j;
        }
        let value_start = j + 1;
        let value_end = match memchr(quote, &input[value_start..]) {
            Some(at) => value_start + at,
            None => input.len(),
        };
        let (_, local) = split_prefix(name_start, name_end, &input[name_start..name_end]);
        let raw_name = Span::input(name_start, name_end);
        let local_bytes = &input[local.0..local.1];
        let name = match input[name_start..name_end].starts_with(b"xmlns") {
            true => Name::Unknown,
            false => Name::lookup_attr(local_bytes),
        };
        let value = self.attr_value(value_start, value_end);
        self.tape.attrs.push(Attr {
            name,
            raw_name,
            value,
        });
        (value_end + 1).min(input.len())
    }

    fn attr_value(&mut self, start: usize, end: usize) -> Span {
        let bytes = &self.input[start..end];
        if memchr(b'&', bytes).is_none() {
            return Span::input(start, end);
        }
        let at = self.tape.decoded.len();
        decode_entities(
            bytes,
            start,
            &mut self.tape.decoded,
            &mut self.tape.warnings,
        );
        Span::decoded(at, self.tape.decoded.len())
    }

    fn end_tag(&mut self, lt: usize) -> usize {
        let input = self.input;
        let name_start = lt + 2;
        let gt = match memchr(b'>', &input[name_start..]) {
            Some(at) => name_start + at,
            None => input.len(),
        };
        let mut name_end = gt;
        while name_end > name_start && matches!(input[name_end - 1], b' ' | b'\t' | b'\r' | b'\n') {
            name_end -= 1;
        }
        let (_, local) = split_prefix(name_start, name_end, &input[name_start..name_end]);
        let local_bytes = &input[local.0..local.1];
        let after = (gt + 1).min(input.len());
        let matching = self.stack.iter().rposition(|f| {
            self.tape.nodes[f.node as usize]
                .raw_name
                .resolve(input, &[])
                == local_bytes
        });
        let Some(index) = matching else {
            self.tape.warnings.push(Warning::new(
                WarningCode::MismatchedEndTag,
                lt,
                format!(
                    "end tag </{}> has no open element",
                    String::from_utf8_lossy(local_bytes)
                ),
            ));
            return after;
        };
        while self.stack.len() > index + 1 {
            let Some(frame) = self.stack.pop() else { break };
            self.tape.warnings.push(Warning::new(
                WarningCode::MismatchedEndTag,
                lt,
                format!(
                    "element <{}> closed by </{}>",
                    self.name_of(frame.node),
                    String::from_utf8_lossy(local_bytes)
                ),
            ));
            self.close(frame, lt, lt);
        }
        if let Some(frame) = self.stack.pop() {
            self.close(frame, lt, after);
        }
        after
    }

    /// Finishes a node: inner and outer extent, text pieces and text.
    fn close(&mut self, frame: Frame, inner_end: usize, outer_end: usize) {
        let has_children = self.tape.nodes[frame.node as usize].first_child != NONE;
        let pieces_start = self.tape.pieces.len();
        let mut total = 0usize;
        for span in self.scratch.drain(frame.piece_start..) {
            if has_children && is_blank(span.resolve(self.input, &self.tape.decoded)) {
                continue;
            }
            total += span.len();
            self.tape.pieces.push(span);
        }
        let count = self.tape.pieces.len() - pieces_start;
        let text = match count {
            0 => Span::EMPTY,
            1 => self.tape.pieces[pieces_start],
            _ => {
                let at = self.tape.decoded.len();
                let mut joined = Vec::with_capacity(total);
                for span in &self.tape.pieces[pieces_start..] {
                    joined.extend_from_slice(span.resolve(self.input, &self.tape.decoded));
                }
                self.tape.decoded.extend_from_slice(&joined);
                Span::decoded(at, self.tape.decoded.len())
            }
        };
        let node = &mut self.tape.nodes[frame.node as usize];
        node.inner = Span::input(node.open.end(), inner_end.max(node.open.end()));
        node.end = outer_end.max(node.open.end()) as u32;
        node.pieces = pieces_start as u32;
        node.piece_count = count.min(u16::MAX as usize) as u16;
        node.text = text;
    }
}

fn is_blank(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
}

/// `(prefix, local)` byte ranges of a possibly prefixed name.
fn split_prefix(
    start: usize,
    end: usize,
    bytes: &[u8],
) -> (Option<(usize, usize)>, (usize, usize)) {
    match memchr(b':', bytes) {
        Some(colon) => (Some((start, start + colon)), (start + colon + 1, end)),
        None => (None, (start, end)),
    }
}

/// Appends `bytes` to `out` with XML entities and character references
/// replaced; an unknown entity is kept literally and reported.
fn decode_entities(bytes: &[u8], offset: usize, out: &mut Vec<u8>, warnings: &mut Vec<Warning>) {
    let mut rest = bytes;
    let mut at = offset;
    while let Some(amp) = memchr(b'&', rest) {
        out.extend_from_slice(&rest[..amp]);
        let tail = &rest[amp..];
        let Some(semi) = memchr(b';', &tail[..tail.len().min(12)]) else {
            warnings.push(Warning::new(
                WarningCode::BadEntity,
                at + amp,
                "unterminated entity",
            ));
            out.push(b'&');
            rest = &tail[1..];
            at += amp + 1;
            continue;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            b"lt" => Some('<'),
            b"gt" => Some('>'),
            b"amp" => Some('&'),
            b"quot" => Some('"'),
            b"apos" => Some('\''),
            _ => char_reference(entity),
        };
        match decoded {
            Some(c) => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            None => {
                warnings.push(Warning::new(
                    WarningCode::BadEntity,
                    at + amp,
                    format!("unknown entity &{};", String::from_utf8_lossy(entity)),
                ));
                out.extend_from_slice(&tail[..=semi]);
            }
        }
        rest = &tail[semi + 1..];
        at += amp + semi + 1;
    }
    out.extend_from_slice(rest);
}

fn char_reference(entity: &[u8]) -> Option<char> {
    let digits = entity.strip_prefix(b"#")?;
    let code = match digits
        .strip_prefix(b"x")
        .or_else(|| digits.strip_prefix(b"X"))
    {
        Some(hex) => u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?,
        None => std::str::from_utf8(digits).ok()?.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::Name;
    use crate::tape::NONE;
    use crate::warning::WarningCode;

    fn names(tape: &Tape, input: &[u8], n: u32) -> Vec<String> {
        tape.children(n)
            .map(|c| tape.str(input, tape.nodes[c as usize].raw_name).to_string())
            .collect()
    }

    #[test]
    fn minimal_document_has_root_gaeb() {
        let input = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"></GAEB>";
        let tape = lex(input).unwrap();
        let root = &tape.nodes[tape.root as usize];
        assert_eq!(root.name, Name::GAEB);
        assert_eq!(
            tape.str(input, tape.default_ns),
            "http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"
        );
        assert_eq!(root.first_child, NONE);
        assert!(tape.warnings.is_empty());
    }

    #[test]
    fn nested_children_and_siblings_link() {
        let input = b"<GAEB><GAEBInfo><Version>3.3</Version><Date>2026-01-01</Date></GAEBInfo><Award><DP>83</DP></Award></GAEB>";
        let tape = lex(input).unwrap();
        assert_eq!(names(&tape, input, tape.root), vec!["GAEBInfo", "Award"]);
        let info = tape.child(tape.root, Name::GAEBInfo).unwrap();
        assert_eq!(names(&tape, input, info), vec!["Version", "Date"]);
        let version = tape.child(info, Name::Version).unwrap();
        assert_eq!(tape.str(input, tape.nodes[version as usize].text), "3.3");
        assert_eq!(tape.nodes[version as usize].parent, info);
        let dp = tape
            .child(tape.child(tape.root, Name::Award).unwrap(), Name::DP)
            .unwrap();
        assert_eq!(tape.str(input, tape.nodes[dp as usize].text), "83");
    }

    #[test]
    fn attributes_are_indexed() {
        let input = b"<GAEB><Item ID=\"a1\" RNoPart='0010' RNoIndex=\"A\"/></GAEB>";
        let tape = lex(input).unwrap();
        let item = tape.child(tape.root, Name::Item).unwrap();
        assert_eq!(tape.nodes[item as usize].attr_count, 3);
        assert_eq!(
            tape.str(input, tape.attr(item, Name::AttrId).unwrap()),
            "a1"
        );
        assert_eq!(
            tape.str(input, tape.attr(item, Name::AttrRNoPart).unwrap()),
            "0010"
        );
        assert_eq!(
            tape.str(input, tape.attr(item, Name::AttrRNoIndex).unwrap()),
            "A"
        );
        assert!(tape.attr(item, Name::AttrStyle).is_none());
    }

    #[test]
    fn entities_decode_once() {
        let input = b"<GAEB><Name1>Heinrich &amp; Co &#x26; S&#246;hne &lt;KG&gt; &quot;x&quot; &apos;y&apos;</Name1></GAEB>";
        let tape = lex(input).unwrap();
        let n = tape.child(tape.root, Name::Name1).unwrap();
        assert_eq!(
            tape.str(input, tape.nodes[n as usize].text),
            "Heinrich & Co & Söhne <KG> \"x\" 'y'"
        );
    }

    #[test]
    fn text_without_entities_is_borrowed_from_input() {
        let input = b"<GAEB><Name1>plain</Name1></GAEB>";
        let tape = lex(input).unwrap();
        let n = tape.child(tape.root, Name::Name1).unwrap();
        assert!(!tape.nodes[n as usize].text.is_decoded());
        assert!(tape.decoded.is_empty());
    }

    #[test]
    fn cdata_is_text() {
        let input = b"<GAEB><Name1><![CDATA[a < b & c]]></Name1></GAEB>";
        let tape = lex(input).unwrap();
        let n = tape.child(tape.root, Name::Name1).unwrap();
        assert_eq!(tape.str(input, tape.nodes[n as usize].text), "a < b & c");
    }

    #[test]
    fn comment_pi_and_doctype_are_skipped() {
        let input = b"<?xml version=\"1.0\"?><!DOCTYPE GAEB><!-- GXML Toolbox --><GAEB><?pi x?><!-- c --><DP>83</DP></GAEB>";
        let tape = lex(input).unwrap();
        assert_eq!(names(&tape, input, tape.root), vec!["DP"]);
        assert!(tape.warnings.is_empty());
    }

    #[test]
    fn self_closing_has_no_children_and_empty_text() {
        let input = b"<GAEB><CurLbl/><Name>x</Name></GAEB>";
        let tape = lex(input).unwrap();
        let lbl = tape.child(tape.root, Name::CurLbl).unwrap();
        assert_eq!(tape.nodes[lbl as usize].first_child, NONE);
        assert_eq!(tape.str(input, tape.nodes[lbl as usize].text), "");
        assert_eq!(names(&tape, input, tape.root), vec!["CurLbl", "Name"]);
    }

    #[test]
    fn unknown_element_kept_with_raw_name() {
        let input = b"<GAEB><VendorThing a=\"1\">v</VendorThing></GAEB>";
        let tape = lex(input).unwrap();
        let v = tape.children(tape.root).next().unwrap();
        assert_eq!(tape.nodes[v as usize].name, Name::Unknown);
        assert_eq!(
            tape.str(input, tape.nodes[v as usize].raw_name),
            "VendorThing"
        );
        assert_eq!(
            tape.str(input, tape.nodes[v as usize].outer()),
            "<VendorThing a=\"1\">v</VendorThing>"
        );
        assert_eq!(tape.str(input, tape.nodes[v as usize].inner), "v");
    }

    #[test]
    fn unclosed_at_eof_warns() {
        let input = b"<GAEB><Award><DP>83</DP>";
        let tape = lex(input).unwrap();
        assert_eq!(names(&tape, input, tape.root), vec!["Award"]);
        assert!(
            tape.warnings
                .iter()
                .any(|w| w.code == WarningCode::UnclosedElement)
        );
    }

    #[test]
    fn mismatched_end_tag_closes_up_to_match() {
        let input = b"<GAEB><Award><DP>83</Award></GAEB>";
        let tape = lex(input).unwrap();
        let award = tape.child(tape.root, Name::Award).unwrap();
        assert_eq!(names(&tape, input, award), vec!["DP"]);
        assert!(
            tape.warnings
                .iter()
                .any(|w| w.code == WarningCode::MismatchedEndTag)
        );
    }

    #[test]
    fn prefix_is_stripped() {
        let input = b"<g:GAEB xmlns:g=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><g:Award><g:DP>83</g:DP></g:Award></g:GAEB>";
        let tape = lex(input).unwrap();
        assert_eq!(tape.nodes[tape.root as usize].name, Name::GAEB);
        assert_eq!(
            tape.str(input, tape.default_ns),
            "http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"
        );
        assert!(
            tape.child(tape.child(tape.root, Name::Award).unwrap(), Name::DP)
                .is_some()
        );
    }

    #[test]
    fn whitespace_between_elements_is_not_text() {
        let input = b"<GAEB>\n  <Award>\n    <DP>83</DP>\n  </Award>\n</GAEB>";
        let tape = lex(input).unwrap();
        let award = tape.child(tape.root, Name::Award).unwrap();
        assert_eq!(tape.str(input, tape.nodes[award as usize].text), "");
        assert_eq!(tape.str(input, tape.nodes[tape.root as usize].text), "");
    }

    #[test]
    fn mixed_text_in_span_keeps_pieces() {
        let input = b"<GAEB><span>a<br/>b</span></GAEB>";
        let tape = lex(input).unwrap();
        let span = tape.child(tape.root, Name::Span).unwrap();
        assert_eq!(tape.str(input, tape.nodes[span as usize].text), "ab");
        assert_eq!(names(&tape, input, span), vec!["br"]);
        let pieces: Vec<&str> = tape.text_pieces(span).map(|p| tape.str(input, p)).collect();
        assert_eq!(pieces, vec!["a", "b"]);
    }

    #[test]
    fn not_xml_is_an_error() {
        assert!(matches!(lex(b"hello"), Err(crate::error::Error::NotXml)));
        assert!(matches!(
            lex(b"<?xml version=\"1.0\"?>"),
            Err(crate::error::Error::NoGaebRoot)
        ));
    }

    #[test]
    fn every_corpus_file_lexes() {
        let Some(dir) = std::env::var_os("GAEBBOSS_CORPUS") else {
            return;
        };
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(d) = stack.pop() {
            for entry in std::fs::read_dir(d).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if ext.starts_with('X') {
                    files.push(path);
                }
            }
        }
        files.sort();
        let started = std::time::Instant::now();
        let (mut bytes, mut nodes, mut warnings) = (0usize, 0usize, 0usize);
        for path in &files {
            let raw = std::fs::read(path).unwrap();
            let normalized = crate::bytes::normalize(&raw).unwrap();
            let tape = lex(&normalized.bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                tape.nodes[tape.root as usize].name,
                Name::GAEB,
                "{}",
                path.display()
            );
            assert!(tape.nodes.len() > 10, "{}", path.display());
            bytes += raw.len();
            nodes += tape.nodes.len();
            warnings += tape.warnings.len();
        }
        let secs = started.elapsed().as_secs_f64();
        eprintln!(
            "lexed {} files, {} MB, {} nodes, {} warnings in {:.2}s = {:.0} MB/s",
            files.len(),
            bytes >> 20,
            nodes,
            warnings,
            secs,
            bytes as f64 / 1e6 / secs
        );
    }
}
