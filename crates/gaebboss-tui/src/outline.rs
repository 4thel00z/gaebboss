//! The bill as an owned tree of rows: the bill itself, its categories, items
//! and markup items, each with the facts, texts and findings the panes show.

use std::collections::HashMap;
use std::ops::Range;

use gaebboss_core::Document;
use gaebboss_core::boq::{CategoryRef, Entry, ItemKind, ItemRef, MarkupRef};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::oz::BkdnLevel;
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

#[derive(Clone, Debug, Default)]
pub struct BillFacts {
    pub version: String,
    pub phase: String,
    pub project: String,
    pub project_label: String,
    pub boq_label: String,
    pub currency: String,
    pub breakdown: String,
    pub categories: usize,
    pub items: usize,
    pub markups: usize,
    pub declared: Option<Decimal>,
    pub computed: Option<Decimal>,
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CategoryFacts {
    pub declared: Option<Decimal>,
    pub computed: Option<Decimal>,
    /// Items anywhere below the category.
    pub items: usize,
}

#[derive(Clone, Debug)]
pub struct ItemFacts {
    pub kind: ItemKind,
    pub counts: bool,
    pub not_offered: bool,
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    pub discount: Option<Decimal>,
    pub total: Option<Decimal>,
    /// Quantity times unit price rounded to cents, less the discount.
    pub computed: Option<Decimal>,
    pub vat: Option<Decimal>,
}

#[derive(Clone, Debug, Default)]
pub struct MarkupFacts {
    pub percent: Option<Decimal>,
    pub base: Option<Decimal>,
    pub total: Option<Decimal>,
}

#[derive(Clone, Debug)]
pub enum Facts {
    Bill(BillFacts),
    Category(CategoryFacts),
    Item(ItemFacts),
    Markup(MarkupFacts),
}

#[derive(Clone, Debug)]
pub struct Row {
    pub oz: String,
    pub compact_oz: String,
    pub id: String,
    /// The bill name, category label or item short text.
    pub label: String,
    pub depth: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub node: NodeId,
    pub facts: Facts,
    pub long_text: String,
    pub markdown: String,
    pub findings: Vec<Finding>,
}

impl Row {
    pub fn kind(&self) -> RowKind {
        match self.facts {
            Facts::Bill(_) => RowKind::Bill,
            Facts::Category(_) => RowKind::Category,
            Facts::Item(_) => RowKind::Item,
            Facts::Markup(_) => RowKind::Markup,
        }
    }

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

    /// The last part of the Ordnungszahl: what tells the row apart from its
    /// siblings.
    pub fn own_oz(&self) -> &str {
        self.oz.rsplit('.').next().unwrap_or(&self.oz)
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

fn text(doc: &Document, span: Option<Span>) -> String {
    span.map(|s| doc.str(s).trim().to_string())
        .unwrap_or_default()
}

fn boq_name(doc: &Document) -> String {
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    text(doc, info.and_then(|i| i.name))
}

fn row(
    oz: &gaebboss_core::oz::Oz,
    id: String,
    label: String,
    depth: usize,
    node: NodeId,
    facts: Facts,
) -> Row {
    Row {
        oz: oz.dotted(),
        compact_oz: oz.compact(),
        id,
        label,
        depth,
        parent: None,
        children: Vec::new(),
        node,
        facts,
        long_text: String::new(),
        markdown: String::new(),
        findings: Vec::new(),
    }
}

/// The bill row, labelled with the BoQ name.
fn bill(doc: &Document, facts: BillFacts) -> Row {
    let name = boq_name(doc);
    let label = match name.is_empty() {
        true => "Bill".to_string(),
        false => name,
    };
    Row {
        oz: String::new(),
        compact_oz: String::new(),
        id: text(doc, doc.boq().and_then(|b| b.id)),
        label,
        depth: 0,
        parent: None,
        children: Vec::new(),
        node: doc.tape().root,
        facts: Facts::Bill(facts),
        long_text: String::new(),
        markdown: String::new(),
        findings: Vec::new(),
    }
}

fn headline(doc: &Document) -> BillFacts {
    BillFacts {
        version: doc.version().as_str().to_string(),
        phase: doc
            .phase()
            .map(|p| format!("X{}", p.code()))
            .unwrap_or_default(),
        ..BillFacts::default()
    }
}

/// The shape of an Ordnungszahl in this bill, one `0` per digit and the
/// index noted apart: `000.000.0000 + index`.
fn breakdown_mask(doc: &Document) -> String {
    let breakdown = doc.breakdown();
    let levels = breakdown.levels();
    let mask = levels
        .iter()
        .filter(|l| l.kind != BkdnLevel::Index)
        .map(|l| "0".repeat(l.length))
        .collect::<Vec<_>>()
        .join(".");
    match levels.iter().any(|l| l.kind == BkdnLevel::Index) {
        true => format!("{mask} + index"),
        false => mask,
    }
}

fn bill_row(doc: &Document, findings: &[Finding]) -> Row {
    let project = doc.gaeb.prj_info.as_ref();
    let info = doc.boq().and_then(|b| b.boq_info.as_ref());
    let totals = doc.computed_totals();
    let count = |severity: Severity| findings.iter().filter(|f| f.severity == severity).count();
    let breakdown = breakdown_mask(doc);
    let facts = BillFacts {
        project: text(doc, project.and_then(|p| p.name_prj)),
        project_label: text(doc, project.and_then(|p| p.lbl_prj)),
        boq_label: text(doc, info.and_then(|i| i.lbl_boq)),
        currency: text(doc, project.and_then(|p| p.cur)),
        breakdown,
        categories: doc.categories().count(),
        items: doc.items().count(),
        markups: doc.markup_items().count(),
        declared: totals.declared,
        computed: totals.boq,
        errors: count(Severity::Error),
        warnings: count(Severity::Warning),
        notes: count(Severity::Info),
        ..headline(doc)
    };
    bill(doc, facts)
}

fn category_row(
    c: &CategoryRef<'_>,
    id: String,
    computed: &HashMap<String, Option<Decimal>>,
) -> Row {
    let facts = CategoryFacts {
        declared: c.ctgy.totals.as_ref().and_then(|t| t.total),
        computed: computed.get(&c.oz.dotted()).copied().flatten(),
        items: 0,
    };
    row(
        &c.oz,
        id,
        c.label.clone(),
        c.depth + 1,
        c.ctgy.node,
        Facts::Category(facts),
    )
}

fn item_row(i: &ItemRef<'_>) -> Row {
    let facts = ItemFacts {
        kind: i.kind(),
        counts: i.counts_towards_total(),
        not_offered: i.not_offered(),
        quantity: i.quantity(),
        unit: i.unit().unwrap_or("").trim().to_string(),
        unit_price: i.unit_price(),
        discount: i.discount_percent(),
        total: i.total(),
        computed: i.computed_total(),
        vat: i.vat_percent(),
    };
    let label = i.short_text().trim().to_string();
    let mut item = row(
        &i.oz,
        i.id().to_string(),
        label,
        i.path.len() + 1,
        i.item.node,
        Facts::Item(facts),
    );
    item.long_text = i.long_text();
    item.markdown = i.long_text_markdown();
    item
}

fn markup_row(m: &MarkupRef<'_>) -> Row {
    let facts = MarkupFacts {
        percent: m.percent(),
        base: m.base(),
        total: m.total(),
    };
    row(
        &m.oz,
        m.id().to_string(),
        "Markup".to_string(),
        m.path.len() + 1,
        m.item.node,
        Facts::Markup(facts),
    )
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
        Outline {
            rows: vec![bill(doc, headline(doc))],
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
            row.parent = Some(
                parent_node
                    .and_then(|n| by_node.get(&n).copied())
                    .unwrap_or(ROOT),
            );
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
        count_items(&mut rows);
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

    /// Whether the row is its parent's last child; the bill counts as last.
    pub fn is_last_child(&self, index: usize) -> bool {
        let Some(parent) = self.row(index).and_then(|r| r.parent) else {
            return true;
        };
        self.row(parent)
            .and_then(|p| p.children.last())
            .is_none_or(|last| *last == index)
    }
}

/// Fills each category's item count; parents precede their children, so a
/// reverse pass sees every child before its parent.
fn count_items(rows: &mut [Row]) {
    let mut below = vec![0usize; rows.len()];
    for index in (1..rows.len()).rev() {
        let own = usize::from(rows[index].kind() == RowKind::Item);
        let total = below[index] + own;
        if let Some(parent) = rows[index].parent {
            below[parent] += total;
        }
        if let Facts::Category(facts) = &mut rows[index].facts {
            facts.items = below[index];
        }
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
        let count = |kind: RowKind| outline.rows.iter().filter(|r| r.kind() == kind).count();
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
    fn items_carry_facts_and_ancestors_end_at_the_bill() {
        let doc = bvbs();
        let outline = Outline::build(&doc);
        let index = outline
            .rows
            .iter()
            .position(|r| r.oz == "001.001.0010")
            .unwrap();
        let row = &outline.rows[index];
        assert_eq!(row.kind(), RowKind::Item);
        assert_eq!(row.id, "ID02BED3D8");
        assert_eq!(row.own_oz(), "0010");
        let Facts::Item(facts) = &row.facts else {
            panic!("not an item");
        };
        assert_eq!(facts.total, Some(Decimal::parse("5000.00").unwrap()));
        let ancestors = outline.ancestors(index);
        assert_eq!(ancestors.first(), Some(&index));
        assert_eq!(ancestors.last(), Some(&ROOT));
        assert_eq!(ancestors.len(), 4);
        let range = outline.source_range(&doc, index);
        assert!(doc.bytes()[range].starts_with(b"<Item"));
    }

    #[test]
    fn categories_count_the_items_below_them() {
        let outline = Outline::build(&bvbs());
        let items = |oz: &str| {
            let row = outline.rows.iter().find(|r| r.oz == oz).unwrap();
            match &row.facts {
                Facts::Category(c) => c.items,
                _ => panic!("not a category"),
            }
        };
        let top: usize = outline.rows[ROOT]
            .children
            .iter()
            .map(|c| match &outline.rows[*c].facts {
                Facts::Category(f) => f.items,
                _ => 0,
            })
            .sum();
        assert_eq!(top, 27);
        assert!(items("001") >= items("001.001"));
        assert!(outline.is_last_child(ROOT));
        let last_top = *outline.rows[ROOT].children.last().unwrap();
        assert!(outline.is_last_child(last_top));
        assert!(!outline.is_last_child(outline.rows[ROOT].children[0]));
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
        assert!(
            outline
                .rows
                .iter()
                .any(|r| r.severity() == Some(Severity::Error))
        );
    }
}
