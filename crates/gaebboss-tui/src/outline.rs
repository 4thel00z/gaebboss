//! The bill as an owned tree of rows: the bill itself, its categories, items
//! and markup items, each with the fields, texts and findings the panes show.

use std::collections::HashMap;
use std::ops::Range;

use gaebboss_core::Document;
use gaebboss_core::boq::{CategoryRef, Entry, ItemRef, MarkupRef};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::json::kind_name;
use gaebboss_core::span::Span;
use gaebboss_core::tape::{NONE, NodeId};
use gaebboss_core::validate::{Finding, Severity, validate};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    Bill,
    Category,
    Item,
    Markup,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub kind: RowKind,
    pub oz: String,
    pub compact_oz: String,
    pub id: String,
    /// The category label or item short text.
    pub label: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub node: NodeId,
    pub fields: Vec<(String, String)>,
    pub long_text: String,
    pub markdown: String,
    pub findings: Vec<Finding>,
}

impl Row {
    /// The worst severity among this row's findings.
    pub fn severity(&self) -> Option<Severity> {
        let rank = |s: Severity| match s {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        };
        self.findings
            .iter()
            .map(|f| f.severity)
            .min_by_key(|s| rank(*s))
    }

    /// What search matches against, lowercased.
    pub fn haystack(&self) -> String {
        [
            self.oz.as_str(),
            self.compact_oz.as_str(),
            self.id.as_str(),
            self.label.as_str(),
            self.long_text.as_str(),
        ]
        .join("\n")
        .to_lowercase()
    }
}

/// Every row, the bill first, children in document order.
#[derive(Clone, Debug, Default)]
pub struct Outline {
    pub rows: Vec<Row>,
    pub haystacks: Vec<String>,
    pub warnings: Vec<String>,
}

pub const ROOT: usize = 0;

fn decimal(value: Option<Decimal>) -> String {
    value.map(|d| d.to_string()).unwrap_or_default()
}

fn text(doc: &Document, span: Option<Span>) -> String {
    span.map(|s| doc.str(s).trim().to_string())
        .unwrap_or_default()
}

fn yes_no(value: bool) -> String {
    match value {
        true => "yes".to_string(),
        false => "no".to_string(),
    }
}

fn kept(fields: Vec<(&str, String)>) -> Vec<(String, String)> {
    fields
        .into_iter()
        .filter(|(_, value)| !value.is_empty())
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn boq_name(doc: &Document) -> String {
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    text(doc, info.and_then(|i| i.name))
}

fn version_fields(doc: &Document) -> Vec<(&'static str, String)> {
    vec![
        ("Version", doc.version().as_str().to_string()),
        (
            "Phase",
            doc.phase()
                .map(|p| format!("X{}", p.code()))
                .unwrap_or_default(),
        ),
    ]
}

/// The bill row, labelled with the BoQ name, holding `fields`.
fn bill(doc: &Document, fields: Vec<(&str, String)>) -> Row {
    let name = boq_name(doc);
    let label = match name.is_empty() {
        true => "Bill".to_string(),
        false => name,
    };
    Row {
        kind: RowKind::Bill,
        oz: String::new(),
        compact_oz: String::new(),
        id: text(doc, doc.boq().and_then(|b| b.id)),
        label,
        depth: 0,
        parent: None,
        children: Vec::new(),
        node: doc.tape().root,
        fields: kept(fields),
        long_text: String::new(),
        markdown: String::new(),
        findings: Vec::new(),
    }
}

fn bill_row(doc: &Document, findings: &[Finding]) -> Row {
    let project = doc.gaeb.prj_info.as_ref();
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    let totals = doc.computed_totals();
    let count = |severity: Severity| findings.iter().filter(|f| f.severity == severity).count();
    let breakdown = doc
        .breakdown()
        .levels()
        .iter()
        .map(|l| format!("{:?}({})", l.kind, l.length))
        .collect::<Vec<_>>()
        .join(" ");
    let mut fields = version_fields(doc);
    fields.extend([
        ("Project", text(doc, project.and_then(|p| p.name_prj))),
        ("Project label", text(doc, project.and_then(|p| p.lbl_prj))),
        ("Currency", text(doc, project.and_then(|p| p.cur))),
        ("BoQ", boq_name(doc)),
        ("BoQ label", text(doc, info.and_then(|i| i.lbl_boq))),
        ("Breakdown", breakdown),
        ("Categories", doc.categories().count().to_string()),
        ("Items", doc.items().count().to_string()),
        ("Markup items", doc.markup_items().count().to_string()),
        ("Total", decimal(totals.declared)),
        ("Computed", decimal(totals.boq)),
        ("Warnings", doc.warnings.len().to_string()),
        (
            "Findings",
            format!(
                "{} errors, {} warnings, {} notes",
                count(Severity::Error),
                count(Severity::Warning),
                count(Severity::Info)
            ),
        ),
    ]);
    bill(doc, fields)
}

fn category_row(
    c: &CategoryRef<'_>,
    id: String,
    computed: &HashMap<String, Option<Decimal>>,
) -> Row {
    let oz = c.oz.dotted();
    let declared = c.ctgy.totals.as_ref().and_then(|t| t.total);
    let fields = kept(vec![
        ("OZ", oz.clone()),
        ("ID", id.clone()),
        ("Label", c.label.clone()),
        ("Total", decimal(declared)),
        ("Computed", decimal(computed.get(&oz).copied().flatten())),
    ]);
    Row {
        kind: RowKind::Category,
        compact_oz: c.oz.compact(),
        oz,
        id,
        label: c.label.clone(),
        depth: c.depth + 1,
        parent: None,
        children: Vec::new(),
        node: c.ctgy.node,
        fields,
        long_text: String::new(),
        markdown: String::new(),
        findings: Vec::new(),
    }
}

fn item_row(i: &ItemRef<'_>) -> Row {
    let oz = i.oz.dotted();
    let quantity = match (i.quantity(), i.unit()) {
        (Some(q), Some(unit)) => format!("{q} {unit}"),
        (Some(q), None) => q.to_string(),
        (None, Some(unit)) => format!("({unit})"),
        (None, None) => String::new(),
    };
    let short = i.short_text().trim().to_string();
    let fields = kept(vec![
        ("OZ", oz.clone()),
        ("ID", i.id().to_string()),
        ("Kind", kind_name(i.kind()).to_string()),
        ("Counts", yes_no(i.counts_towards_total())),
        (
            "Not offered",
            match i.not_offered() {
                true => "yes".to_string(),
                false => String::new(),
            },
        ),
        ("Quantity", quantity),
        ("Unit price", decimal(i.unit_price())),
        ("Total", decimal(i.total())),
        ("Qty × UP", decimal(i.computed_total())),
        ("VAT", decimal(i.vat_percent())),
    ]);
    Row {
        kind: RowKind::Item,
        compact_oz: i.oz.compact(),
        oz,
        id: i.id().to_string(),
        label: short,
        depth: i.path.len() + 1,
        parent: None,
        children: Vec::new(),
        node: i.item.node,
        fields,
        long_text: i.long_text(),
        markdown: i.long_text_markdown(),
        findings: Vec::new(),
    }
}

fn markup_row(m: &MarkupRef<'_>) -> Row {
    let oz = m.oz.dotted();
    let fields = kept(vec![
        ("OZ", oz.clone()),
        ("ID", m.id().to_string()),
        ("Percent", decimal(m.percent())),
        ("Base", decimal(m.base())),
        ("Total", decimal(m.total())),
    ]);
    Row {
        kind: RowKind::Markup,
        compact_oz: m.oz.compact(),
        oz,
        id: m.id().to_string(),
        label: "Zuschlag".to_string(),
        depth: m.path.len() + 1,
        parent: None,
        children: Vec::new(),
        node: m.item.node,
        fields,
        long_text: String::new(),
        markdown: String::new(),
        findings: Vec::new(),
    }
}

/// The row a finding belongs to: the closest row element at or above its
/// node, else the row with its Ordnungszahl, else the bill.
fn finding_row(
    doc: &Document,
    by_node: &HashMap<NodeId, usize>,
    by_oz: &HashMap<String, usize>,
    finding: &Finding,
) -> usize {
    let tape = doc.tape();
    let mut node = finding.node.unwrap_or(NONE);
    while node != NONE {
        if let Some(row) = by_node.get(&node) {
            return *row;
        }
        node = tape.node(node).map_or(NONE, |n| n.parent);
    }
    finding
        .oz
        .as_ref()
        .and_then(|oz| by_oz.get(oz))
        .copied()
        .unwrap_or(ROOT)
}

impl Outline {
    /// The bill row alone, named and versioned, while [`Outline::build`] runs.
    pub fn pending(doc: &Document) -> Outline {
        let mut fields = version_fields(doc);
        fields.push(("BoQ", boq_name(doc)));
        Outline {
            rows: vec![bill(doc, fields)],
            haystacks: vec![String::new()],
            warnings: Vec::new(),
        }
    }

    pub fn build(doc: &Document) -> Outline {
        let findings = validate(doc);
        let computed: HashMap<String, Option<Decimal>> = doc
            .computed_totals()
            .categories
            .into_iter()
            .map(|c| (c.oz, c.computed))
            .collect();
        let mut rows = vec![bill_row(doc, &findings)];
        let mut by_node: HashMap<NodeId, usize> = HashMap::new();
        for entry in doc.entries() {
            let parent_node = match &entry {
                Entry::Category(c) => c.path.last().map(|p| p.node),
                Entry::Item(i) => i.path.last().map(|p| p.node),
                Entry::Markup(m) => m.path.last().map(|p| p.node),
            };
            let mut row = match &entry {
                Entry::Category(c) => category_row(c, entry.id(doc).to_string(), &computed),
                Entry::Item(i) => item_row(i),
                Entry::Markup(m) => markup_row(m),
            };
            let parent = parent_node
                .and_then(|n| by_node.get(&n).copied())
                .unwrap_or(ROOT);
            row.parent = Some(parent);
            by_node.insert(row.node, rows.len());
            rows.push(row);
        }
        for index in 1..rows.len() {
            let Some(parent) = rows[index].parent else {
                continue;
            };
            rows[parent].children.push(index);
        }
        for index in 0..rows.len() {
            let mut children = std::mem::take(&mut rows[index].children);
            children.sort_by_key(|child| rows[*child].node);
            rows[index].children = children;
        }
        let by_oz: HashMap<String, usize> = rows
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, row)| (row.oz.clone(), index))
            .collect();
        by_node.insert(doc.tape().root, ROOT);
        for finding in findings {
            let row = finding_row(doc, &by_node, &by_oz, &finding);
            rows[row].findings.push(finding);
        }
        let haystacks = rows.iter().map(Row::haystack).collect();
        let warnings = doc.warnings.iter().map(|w| w.to_string()).collect();
        Outline {
            rows,
            haystacks,
            warnings,
        }
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    /// The element's place in the document, the whole file for the bill.
    pub fn source_range(&self, doc: &Document, index: usize) -> Range<usize> {
        let whole = 0..doc.bytes().len();
        let Some(row) = self.row(index) else {
            return whole;
        };
        doc.element_range(row.node).unwrap_or(whole)
    }

    /// The row and every row above it, the bill last.
    pub fn ancestors(&self, index: usize) -> Vec<usize> {
        std::iter::successors(Some(index), |i| self.rows.get(*i).and_then(|r| r.parent)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bvbs() -> Document {
        let path = format!(
            "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
            env!("CARGO_MANIFEST_DIR")
        );
        Document::open(std::path::Path::new(&path)).unwrap()
    }

    #[test]
    fn rows_cover_the_bill_categories_items_and_markups() {
        let outline = Outline::build(&bvbs());
        let count = |kind: RowKind| outline.rows.iter().filter(|r| r.kind == kind).count();
        assert_eq!(count(RowKind::Bill), 1);
        assert_eq!(count(RowKind::Category), 12);
        assert_eq!(count(RowKind::Item), 27);
        assert_eq!(count(RowKind::Markup), 1);
        assert_eq!(outline.rows[ROOT].children.len(), 3);
    }

    #[test]
    fn children_follow_document_order_and_point_back_to_their_parent() {
        let outline = Outline::build(&bvbs());
        for (index, row) in outline.rows.iter().enumerate() {
            assert!(
                row.children
                    .windows(2)
                    .all(|w| outline.rows[w[0]].node < outline.rows[w[1]].node)
            );
            assert!(
                row.children
                    .iter()
                    .all(|c| outline.rows[*c].parent == Some(index))
            );
        }
    }

    #[test]
    fn items_carry_fields_and_ancestors_end_at_the_bill() {
        let doc = bvbs();
        let outline = Outline::build(&doc);
        let index = outline
            .rows
            .iter()
            .position(|r| r.oz == "001.001.0010")
            .unwrap();
        let row = &outline.rows[index];
        assert_eq!(row.kind, RowKind::Item);
        assert_eq!(row.id, "ID02BED3D8");
        assert!(
            row.fields
                .contains(&("Total".to_string(), "5000.00".to_string()))
        );
        let ancestors = outline.ancestors(index);
        assert_eq!(ancestors.first(), Some(&index));
        assert_eq!(ancestors.last(), Some(&ROOT));
        assert_eq!(ancestors.len(), 4);
        let range = outline.source_range(&doc, index);
        assert!(doc.bytes()[range].starts_with(b"<Item"));
    }

    #[test]
    fn a_wrong_total_is_flagged_as_an_error() {
        let path = format!(
            "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(path).unwrap().replacen(
            "<Total>2000000.00</Total>",
            "<Total>1.00</Total>",
            1,
        );
        let doc = Document::parse(text.as_bytes()).unwrap();
        let outline = Outline::build(&doc);
        let flagged: Vec<&Row> = outline
            .rows
            .iter()
            .filter(|r| r.severity() == Some(Severity::Error))
            .collect();
        assert!(!flagged.is_empty());
    }
}
