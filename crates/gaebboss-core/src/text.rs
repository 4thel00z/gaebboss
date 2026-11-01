//! Projections of GAEB formatted text: plain text and Markdown rendered
//! from the tape in document order, and the text complements inside it.

use crate::Document;
use crate::decimal::Decimal;
use crate::name::Name;
use crate::tape::{NONE, NodeId};

/// How an empty bidder field reads in prose.
pub const PLACEHOLDER: &str = "'....'";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplementKind {
    Owner,
    Bidder,
}

/// A `TextComplement`: a field inside a long text that the issuer filled
/// or the bidder has to fill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Complement {
    pub node: NodeId,
    pub mark: Option<i64>,
    pub kind: ComplementKind,
    pub empty: bool,
    pub caption: String,
    pub body: String,
    pub tail: String,
    pub number: Option<Decimal>,
}

const QUOTES: &[char] = &[
    '\'', '"', '\u{201A}', '\u{2018}', '\u{2019}', '\u{201E}', '\u{201C}', '\u{201D}',
];

impl Complement {
    /// The caption, which is what labels the field.
    pub fn label(&self) -> &str {
        &self.caption
    }

    /// The entry without its enclosing quote marks; empty for an empty field.
    pub fn value(&self) -> String {
        if self.empty {
            return String::new();
        }
        let text = self.body.trim();
        let stripped = text
            .strip_prefix(QUOTES)
            .and_then(|t| t.strip_suffix(QUOTES))
            .unwrap_or(text);
        stripped.trim().to_string()
    }

    /// How the field reads inside the prose.
    pub fn inline(&self) -> String {
        let body = match self.empty || self.body.trim().is_empty() {
            true => PLACEHOLDER,
            false => self.body.as_str(),
        };
        join_inline(&join_inline(&self.caption, body), &self.tail)
    }
}

/// Two pieces of one sentence with the space a reader expects between them.
pub fn join_inline(left: &str, right: &str) -> String {
    if left.is_empty() || right.is_empty() {
        return format!("{left}{right}");
    }
    let glued = left.ends_with(char::is_whitespace)
        || right.starts_with(char::is_whitespace)
        || right.starts_with([',', '.', ';', ':', '!', '?', ')']);
    match glued {
        true => format!("{left}{right}"),
        false => format!("{left} {right}"),
    }
}

/// The plain text of any text-carrying node: paragraphs on their own lines,
/// list items bulleted, table cells tab-separated, complements inline.
pub fn plain(doc: &Document, node: NodeId) -> String {
    let mut r = Renderer {
        doc,
        out: String::new(),
        markdown: false,
        list_depth: 0,
    };
    r.blocks(node);
    r.finish()
}

/// Markdown with bold and italic from span styles, lists and GFM tables.
pub fn markdown(doc: &Document, node: NodeId) -> String {
    let mut r = Renderer {
        doc,
        out: String::new(),
        markdown: true,
        list_depth: 0,
    };
    r.blocks(node);
    r.finish()
}

/// Every `TextComplement` below a node, in document order.
pub fn complements(doc: &Document, node: NodeId) -> Vec<Complement> {
    let mut out = Vec::new();
    collect_complements(doc, node, &mut out);
    out
}

fn collect_complements(doc: &Document, node: NodeId, out: &mut Vec<Complement>) {
    let tape = doc.tape();
    for child in tape.children(node) {
        if tape.nodes[child as usize].name == Name::TextComplement {
            out.push(complement(doc, child));
            continue;
        }
        collect_complements(doc, child, out);
    }
}

fn complement(doc: &Document, node: NodeId) -> Complement {
    let tape = doc.tape();
    let attr = |name: Name| tape.attr(node, name).map(|s| doc.str(s));
    let mark = attr(Name::AttrMarkLbl).and_then(|m| m.trim().parse().ok());
    let kind = match attr(Name::AttrKind) {
        Some("Bidder") => ComplementKind::Bidder,
        _ => ComplementKind::Owner,
    };
    let part = |name: Name| {
        tape.child(node, name)
            .map(|c| inline_of(doc, c))
            .unwrap_or_default()
    };
    let body = part(Name::ComplBody);
    let number = tape
        .child(node, Name::ComplBodyDec)
        .or_else(|| tape.child(node, Name::ComplBodyInt))
        .and_then(|c| tape.attr(c, Name::AttrValue))
        .and_then(|v| Decimal::parse(doc.str(v).trim()).ok());
    let empty = attr(Name::AttrEmpty) == Some("Yes") || body.trim().is_empty();
    Complement {
        node,
        mark,
        kind,
        empty,
        caption: part(Name::ComplCaption),
        body,
        tail: part(Name::ComplTail),
        number,
    }
}

fn inline_of(doc: &Document, node: NodeId) -> String {
    let mut r = Renderer {
        doc,
        out: String::new(),
        markdown: false,
        list_depth: 0,
    };
    r.inline_children(node);
    r.out.trim().to_string()
}

struct Renderer<'a> {
    doc: &'a Document,
    out: String,
    markdown: bool,
    list_depth: usize,
}

impl Renderer<'_> {
    fn finish(mut self) -> String {
        let lines: Vec<&str> = self.out.lines().map(str::trim_end).collect();
        let mut joined = String::with_capacity(self.out.len());
        let mut blank_run = 0;
        for line in lines {
            if line.is_empty() {
                blank_run += 1;
                if blank_run > 1 {
                    continue;
                }
            } else {
                blank_run = 0;
            }
            joined.push_str(line);
            joined.push('\n');
        }
        self.out = joined;
        self.out.trim().to_string()
    }

    fn name(&self, node: NodeId) -> Name {
        self.doc.tape().nodes[node as usize].name
    }

    /// Whether the current output line holds more than a list marker.
    fn line_has_content(&self) -> bool {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        let bare = line.trim_start().trim_start_matches("- ");
        let bare = bare
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .strip_prefix(". ")
            .unwrap_or(bare);
        !bare.trim().is_empty()
    }

    fn raw(&self, node: NodeId) -> &str {
        self.doc.str(self.doc.tape().nodes[node as usize].raw_name)
    }

    /// Vendor HTML emphasis the schema does not know: `b`, `i`, `u`, `strong`, `em`.
    fn is_emphasis(&self, node: NodeId) -> bool {
        matches!(self.raw(node), "b" | "i" | "u" | "strong" | "em")
    }

    /// Renders the children of a container at block level.
    fn blocks(&mut self, node: NodeId) {
        let tape = self.doc.tape();
        let mut child = tape.nodes[node as usize].first_child;
        while child != NONE {
            self.block(child);
            child = tape.nodes[child as usize].next;
        }
    }

    fn block(&mut self, node: NodeId) {
        match self.name(node) {
            Name::P | Name::Div => {
                self.inline_children(node);
                self.end_block();
            }
            Name::Ul | Name::Ol => self.list(node),
            Name::Table => self.table(node),
            Name::Br | Name::Page => self.out.push('\n'),
            Name::Span | Name::TextComplement => {
                self.inline(node);
                self.end_block();
            }
            Name::Style
            | Name::Image
            | Name::ImageLower
            | Name::Attachment
            | Name::AttachmentLower => {}
            _ => self.blocks(node),
        }
    }

    fn end_block(&mut self) {
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        if self.markdown && self.list_depth == 0 {
            self.out.push('\n');
        }
    }

    /// Renders text pieces and child elements of a node in document order.
    fn inline_children(&mut self, node: NodeId) {
        let tape = self.doc.tape();
        let mut pieces = tape.text_pieces_with_offsets(node).peekable();
        let mut child = tape.nodes[node as usize].first_child;
        loop {
            let child_at = (child != NONE).then(|| tape.nodes[child as usize].open.start());
            match (pieces.peek().copied(), child_at) {
                (Some((span, at)), Some(child_start)) if at < child_start => {
                    self.out.push_str(self.doc.str(span));
                    pieces.next();
                }
                (Some((span, _)), None) => {
                    self.out.push_str(self.doc.str(span));
                    pieces.next();
                }
                (_, Some(_)) => {
                    self.inline(child);
                    child = tape.nodes[child as usize].next;
                }
                (None, None) => break,
            }
        }
    }

    fn inline(&mut self, node: NodeId) {
        match self.name(node) {
            Name::Br => self.out.push('\n'),
            Name::TextComplement => {
                let text = complement(self.doc, node).inline();
                self.out.push_str(&text);
            }
            Name::Span => self.styled(
                node,
                self.doc
                    .tape()
                    .attr(node, Name::AttrStyle)
                    .map(|s| self.doc.str(s).to_ascii_lowercase())
                    .unwrap_or_default(),
            ),
            Name::Unknown if self.raw(node) == "b" => self.wrapped(node, "**"),
            Name::Unknown if self.is_emphasis(node) => self.wrapped(node, "*"),
            Name::Ul | Name::Ol => {
                self.end_block();
                self.list(node);
            }
            Name::Table => {
                self.end_block();
                self.table(node);
            }
            Name::P | Name::Div => {
                if self.line_has_content() {
                    self.out.push('\n');
                }
                self.inline_children(node);
            }
            Name::Image | Name::ImageLower | Name::Style => {}
            _ => self.inline_children(node),
        }
    }

    fn styled(&mut self, node: NodeId, style: String) {
        if !self.markdown {
            return self.inline_children(node);
        }
        let bold = style.contains("font-weight:bold") || style.contains("font-weight: bold");
        let italic = style.contains("font-style:italic")
            || style.contains("font-style: italic")
            || style.contains("text-decoration:underline")
            || style.contains("text-decoration: underline");
        let marker = match (bold, italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        self.wrapped(node, marker);
    }

    /// Markdown emphasis around the node's text, keeping edge spaces outside.
    fn wrapped(&mut self, node: NodeId, marker: &str) {
        let marker = match self.markdown {
            true => marker,
            false => "",
        };
        let start = self.out.len();
        self.inline_children(node);
        if marker.is_empty() {
            return;
        }
        let inner = self.out.split_off(start);
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            self.out.push_str(&inner);
            return;
        }
        let leading = &inner[..inner.len() - inner.trim_start().len()];
        let trailing = &inner[inner.trim_end().len()..];
        self.out.push_str(leading);
        self.out.push_str(marker);
        self.out.push_str(trimmed);
        self.out.push_str(marker);
        self.out.push_str(trailing);
    }

    fn list(&mut self, node: NodeId) {
        let tape = self.doc.tape();
        let numbered = self.name(node) == Name::Ol;
        self.list_depth += 1;
        let mut index = 0;
        for li in tape.children(node) {
            if tape.nodes[li as usize].name != Name::Li {
                continue;
            }
            index += 1;
            if !self.out.is_empty() && !self.out.ends_with('\n') {
                self.out.push('\n');
            }
            for _ in 1..self.list_depth {
                self.out.push_str("  ");
            }
            match numbered {
                true => self.out.push_str(&format!("{index}. ")),
                false => self.out.push_str("- "),
            }
            self.inline_children(li);
            if !self.out.ends_with('\n') {
                self.out.push('\n');
            }
        }
        self.list_depth -= 1;
        if self.markdown && self.list_depth == 0 {
            self.out.push('\n');
        }
    }

    fn table(&mut self, node: NodeId) {
        let tape = self.doc.tape();
        let mut rows: Vec<Vec<String>> = Vec::new();
        for tr in tape.children(node) {
            if tape.nodes[tr as usize].name != Name::Tr {
                continue;
            }
            let cells = tape
                .children(tr)
                .filter(|c| matches!(tape.nodes[*c as usize].name, Name::Td | Name::Th))
                .map(|c| self.cell(c))
                .collect();
            rows.push(cells);
        }
        if rows.is_empty() {
            return;
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        if !self.markdown {
            for row in rows {
                self.out.push_str(&row.join("\t"));
                self.out.push('\n');
            }
            return;
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        for (i, row) in rows.iter().enumerate() {
            self.out.push('|');
            for c in 0..width {
                self.out.push(' ');
                self.out
                    .push_str(row.get(c).map(String::as_str).unwrap_or(""));
                self.out.push_str(" |");
            }
            self.out.push('\n');
            if i == 0 {
                self.out.push('|');
                for _ in 0..width {
                    self.out.push_str(" --- |");
                }
                self.out.push('\n');
            }
        }
        self.out.push('\n');
    }

    fn cell(&self, node: NodeId) -> String {
        let mut inner = Renderer {
            doc: self.doc,
            out: String::new(),
            markdown: self.markdown,
            list_depth: 0,
        };
        inner.inline_children(node);
        inner.out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::decimal::Decimal;

    fn doc_with(detail_txt_inner: &str) -> Document {
        let xml = format!(
            "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Description><CompleteText><DetailTxt>{detail_txt_inner}</DetailTxt></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>"
        );
        Document::parse(xml.as_bytes()).unwrap()
    }

    fn detail_node(doc: &Document) -> crate::tape::NodeId {
        let item = &doc
            .gaeb
            .award
            .as_ref()
            .unwrap()
            .boq
            .as_ref()
            .unwrap()
            .boq_body
            .as_ref()
            .unwrap()
            .itemlist
            .as_ref()
            .unwrap()
            .item[0];
        item.description
            .as_ref()
            .unwrap()
            .complete_text
            .as_ref()
            .unwrap()
            .detail_txt
            .as_ref()
            .unwrap()
            .node
    }

    fn plain_of(inner: &str) -> String {
        let doc = doc_with(inner);
        plain(&doc, detail_node(&doc))
    }

    #[test]
    fn spans_join_and_br_breaks_the_line() {
        assert_eq!(
            plain_of("<Text><p><span>a</span><span>b</span><br/><span>c</span></p></Text>"),
            "ab\nc"
        );
    }

    #[test]
    fn mixed_text_pieces_interleave_with_br() {
        assert_eq!(plain_of("<Text><p><span>a<br/>b</span></p></Text>"), "a\nb");
    }

    #[test]
    fn paragraph_break_is_newline() {
        assert_eq!(
            plain_of("<Text><p><span>one</span></p><p><span>two</span></p></Text>"),
            "one\ntwo"
        );
    }

    #[test]
    fn ul_items_get_bullets() {
        assert_eq!(
            plain_of("<Text><ul><li><span>x</span></li><li><p><span>y</span></p></li></ul></Text>"),
            "- x\n- y"
        );
    }

    #[test]
    fn table_rows_tab_separated() {
        let inner = "<Text><table><tr><td><span>a</span></td><td><span>b</span></td></tr><tr><td><span>c</span></td><td><span>d</span></td></tr></table></Text>";
        assert_eq!(plain_of(inner), "a\tb\nc\td");
    }

    #[test]
    fn empty_bidder_complement_renders_placeholder() {
        let inner = "<Text><p><span>Fabrikat: </span><TextComplement MarkLbl=\"1\" Kind=\"Bidder\" Empty=\"Yes\"><ComplBody/></TextComplement></p></Text>";
        assert_eq!(plain_of(inner), "Fabrikat: '....'");
    }

    #[test]
    fn owner_complement_renders_body_inline() {
        let inner = "<Text><p><TextComplement MarkLbl=\"2\" Kind=\"Owner\"><ComplCaption><span>Stoff</span></ComplCaption><ComplBody><span>'Beton C25/30'</span></ComplBody><ComplTail><span>, erdfeucht</span></ComplTail></TextComplement></p></Text>";
        let doc = doc_with(inner);
        assert_eq!(
            plain(&doc, detail_node(&doc)),
            "Stoff 'Beton C25/30', erdfeucht"
        );
        let found = complements(&doc, detail_node(&doc));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mark, Some(2));
        assert_eq!(found[0].kind, ComplementKind::Owner);
        assert_eq!(found[0].label(), "Stoff");
        assert_eq!(found[0].value(), "Beton C25/30");
        assert_eq!(found[0].tail, ", erdfeucht");
        assert!(!found[0].empty);
    }

    #[test]
    fn complement_number_is_read() {
        let inner = "<Text><p><TextComplement MarkLbl=\"3\" Kind=\"Bidder\"><ComplBodyDec Value=\"0.5\"/><ComplBody><span>0,5</span></ComplBody></TextComplement></p></Text>";
        let doc = doc_with(inner);
        let found = complements(&doc, detail_node(&doc));
        assert_eq!(found[0].number, Some(Decimal::parse("0.5").unwrap()));
        assert_eq!(found[0].kind, ComplementKind::Bidder);
    }

    #[test]
    fn interleaved_text_and_complement_keep_order() {
        let inner = "<Text><p><span>A</span></p></Text><TextComplement MarkLbl=\"1\" Kind=\"Bidder\" Empty=\"Yes\"><ComplBody/></TextComplement><Text><p><span>B</span></p></Text>";
        assert_eq!(plain_of(inner), "A\n'....'\nB");
    }

    #[test]
    fn vendor_list_directly_under_detailtxt_still_renders() {
        assert_eq!(plain_of("<ul><li><span>x</span></li></ul>"), "- x");
    }

    #[test]
    fn bold_span_becomes_markdown_strong() {
        let doc = doc_with(
            "<Text><p><span style=\"font-weight:bold\">QM-Plan</span><span> rest</span></p><ul><li><span>a</span></li></ul></Text>",
        );
        assert_eq!(markdown(&doc, detail_node(&doc)), "**QM-Plan** rest\n\n- a");
    }

    #[test]
    fn markdown_table_is_gfm() {
        let doc = doc_with(
            "<Text><table><tr><td><span>h1</span></td><td><span>h2</span></td></tr><tr><td><span>c1</span></td><td><span>c2</span></td></tr></table></Text>",
        );
        assert_eq!(
            markdown(&doc, detail_node(&doc)),
            "| h1 | h2 |\n| --- | --- |\n| c1 | c2 |"
        );
    }

    #[test]
    fn short_text_renders_from_outline_text() {
        let xml = "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Description><CompleteText><DetailTxt><Text><p><span>long</span></p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><p><span>Beton</span><span> C25/30</span></p></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
        let doc = Document::parse(xml.as_bytes()).unwrap();
        let item = &doc
            .gaeb
            .award
            .as_ref()
            .unwrap()
            .boq
            .as_ref()
            .unwrap()
            .boq_body
            .as_ref()
            .unwrap()
            .itemlist
            .as_ref()
            .unwrap()
            .item[0];
        let outline = item
            .description
            .as_ref()
            .unwrap()
            .complete_text
            .as_ref()
            .unwrap()
            .outline_text
            .as_ref()
            .unwrap();
        assert_eq!(plain(&doc, outline.node), "Beton C25/30");
    }
}
