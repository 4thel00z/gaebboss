//! Navigation over the bill of quantities: categories and items in
//! document order with their Ordnungszahl, kind, quantities, prices, texts
//! and totals.

use crate::Document;
use crate::decimal::{Decimal, item_total};
use crate::model::{BoQ, BoQBody, BoQCtgy, Item, MarkupItem, Provis};
use crate::oz::{Breakdown, Oz};
use crate::text::{Complement, complements, markdown, plain};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Standard,
    LumpSum,
    ProvisionalWithTotal,
    ProvisionalWithoutTotal,
    /// An alternative execution; `serial` 1..9 within `group`.
    Alternative {
        group: i64,
        serial: i64,
    },
    /// The base execution of an alternative group.
    Base {
        group: i64,
    },
    Hourly,
    Key,
    /// The bidder fills in the quantity (`QtyTBD`).
    FreeQuantity,
    NotApplicable,
}

/// An item with everything needed to read it: the document, its position
/// and the categories above it.
pub struct ItemRef<'a> {
    pub doc: &'a Document,
    pub item: &'a Item,
    pub oz: Oz,
    pub path: Vec<&'a BoQCtgy>,
}

impl ItemRef<'_> {
    pub fn id(&self) -> &str {
        self.item.id.map(|s| self.doc.str(s)).unwrap_or("")
    }

    pub fn kind(&self) -> ItemKind {
        let item = self.item;
        if item.not_appl == Some(true) {
            return ItemKind::NotApplicable;
        }
        if item.qty_tbd == Some(true) && item.qty.is_none() {
            return ItemKind::FreeQuantity;
        }
        if let (Some(group), Some(serial)) = (item.aln_group_no, item.aln_ser_no) {
            return match serial {
                0 => ItemKind::Base { group },
                _ => ItemKind::Alternative { group, serial },
            };
        }
        match item.provis {
            Some(Provis::WithTotal) => return ItemKind::ProvisionalWithTotal,
            Some(Provis::WithoutTotal) => return ItemKind::ProvisionalWithoutTotal,
            None => {}
        }
        if item.lump_sum_item == Some(true) {
            return ItemKind::LumpSum;
        }
        if item.hour_it == Some(true) {
            return ItemKind::Hourly;
        }
        if item.key_it == Some(true) {
            return ItemKind::Key;
        }
        ItemKind::Standard
    }

    /// Whether the item's total belongs in the sums (Fachdokumentation
    /// 4.5.2, 4.5.3): alternatives, provisional items without total,
    /// not-applicable and not-offered items stay out.
    pub fn counts_towards_total(&self) -> bool {
        if self.item.not_offered == Some(true) {
            return false;
        }
        !matches!(
            self.kind(),
            ItemKind::Alternative { .. }
                | ItemKind::ProvisionalWithoutTotal
                | ItemKind::NotApplicable
        )
    }

    pub fn not_offered(&self) -> bool {
        self.item.not_offered == Some(true)
    }

    pub fn quantity(&self) -> Option<Decimal> {
        self.item.qty
    }

    pub fn unit(&self) -> Option<&str> {
        self.item.qu.map(|s| self.doc.str(s))
    }

    pub fn unit_price(&self) -> Option<Decimal> {
        self.item.up
    }

    /// The total as written in the file (`IT`).
    pub fn total(&self) -> Option<Decimal> {
        self.item.it
    }

    /// Quantity times unit price rounded to cents, when both are present.
    pub fn computed_total(&self) -> Option<Decimal> {
        item_total(self.item.qty?, self.item.up?)
    }

    pub fn vat_percent(&self) -> Option<Decimal> {
        self.item.vat
    }

    pub fn short_text(&self) -> String {
        short_text(self.doc, self.item)
    }

    pub fn long_text(&self) -> String {
        detail_node(self.item)
            .map(|n| plain(self.doc, n))
            .unwrap_or_default()
    }

    pub fn long_text_markdown(&self) -> String {
        detail_node(self.item)
            .map(|n| markdown(self.doc, n))
            .unwrap_or_default()
    }

    pub fn complements(&self) -> Vec<Complement> {
        detail_node(self.item)
            .map(|n| complements(self.doc, n))
            .unwrap_or_default()
    }
}

fn detail_node(item: &Item) -> Option<crate::tape::NodeId> {
    Some(
        item.description
            .as_ref()?
            .complete_text
            .as_ref()?
            .detail_txt
            .as_ref()?
            .node,
    )
}

fn short_text(doc: &Document, item: &Item) -> String {
    let Some(description) = item.description.as_ref() else {
        return String::new();
    };
    let outline = description
        .complete_text
        .as_ref()
        .and_then(|c| c.outline_text.as_ref())
        .or(description.outline_text.as_ref());
    outline.map(|o| plain(doc, o.node)).unwrap_or_default()
}

/// A category (an LV-Bereich) with its label and position.
pub struct CategoryRef<'a> {
    pub oz: Oz,
    pub label: String,
    pub ctgy: &'a BoQCtgy,
    pub depth: usize,
    pub path: Vec<&'a BoQCtgy>,
}

/// A markup item (Zuschlagsposition).
pub struct MarkupRef<'a> {
    pub doc: &'a Document,
    pub item: &'a MarkupItem,
    pub oz: Oz,
    pub path: Vec<&'a BoQCtgy>,
}

impl MarkupRef<'_> {
    pub fn id(&self) -> &str {
        self.item.id.map(|s| self.doc.str(s)).unwrap_or("")
    }

    pub fn total(&self) -> Option<Decimal> {
        self.item.it
    }

    pub fn percent(&self) -> Option<Decimal> {
        self.item.markup
    }

    pub fn base(&self) -> Option<Decimal> {
        self.item.it_markup
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryTotal {
    pub oz: String,
    pub depth: usize,
    pub computed: Option<Decimal>,
    pub declared: Option<Decimal>,
}

/// Sums per Fachdokumentation 4.6.17 next to what the file declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoqTotals {
    pub categories: Vec<CategoryTotal>,
    pub boq: Option<Decimal>,
    pub declared: Option<Decimal>,
}

impl Document {
    pub fn boq(&self) -> Option<&BoQ> {
        self.gaeb.award.as_ref()?.boq.as_ref()
    }

    pub fn breakdown(&self) -> Breakdown {
        self.boq()
            .and_then(|b| b.boq_info.as_ref())
            .map(|info| Breakdown::from_info(self, info))
            .unwrap_or_default()
    }

    /// Every item in document order, with its Ordnungszahl and categories.
    pub fn items(&self) -> impl Iterator<Item = ItemRef<'_>> {
        let mut out = Vec::new();
        let breakdown = self.breakdown();
        if let Some(body) = self.boq().and_then(|b| b.boq_body.as_ref()) {
            walk(
                self,
                &breakdown,
                body,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut |event| {
                    if let Event::Item(item) = event {
                        out.push(item);
                    }
                },
            );
        }
        out.into_iter()
    }

    pub fn categories(&self) -> impl Iterator<Item = CategoryRef<'_>> {
        let mut out = Vec::new();
        let breakdown = self.breakdown();
        if let Some(body) = self.boq().and_then(|b| b.boq_body.as_ref()) {
            walk(
                self,
                &breakdown,
                body,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut |event| {
                    if let Event::Category(c) = event {
                        out.push(c);
                    }
                },
            );
        }
        out.into_iter()
    }

    pub fn markup_items(&self) -> impl Iterator<Item = MarkupRef<'_>> {
        let mut out = Vec::new();
        let breakdown = self.breakdown();
        if let Some(body) = self.boq().and_then(|b| b.boq_body.as_ref()) {
            walk(
                self,
                &breakdown,
                body,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut |event| {
                    if let Event::Markup(m) = event {
                        out.push(m);
                    }
                },
            );
        }
        out.into_iter()
    }

    /// The BoQ total as written in `BoQInfo/Totals/Total`.
    pub fn declared_total(&self) -> Option<Decimal> {
        self.boq()?.boq_info.as_ref()?.totals.as_ref()?.total
    }

    /// Category and BoQ sums from the item totals, rounded per level.
    pub fn computed_totals(&self) -> BoqTotals {
        let mut totals = BoqTotals {
            declared: self.declared_total(),
            ..BoqTotals::default()
        };
        let breakdown = self.breakdown();
        let Some(body) = self.boq().and_then(|b| b.boq_body.as_ref()) else {
            return totals;
        };
        totals.boq = sum_body(
            self,
            &breakdown,
            body,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut totals.categories,
        );
        totals
    }
}

fn sum_body<'a>(
    doc: &'a Document,
    breakdown: &Breakdown,
    body: &'a BoQBody,
    path: &mut Vec<&'a BoQCtgy>,
    parts: &mut Vec<String>,
    out: &mut Vec<CategoryTotal>,
) -> Option<Decimal> {
    let mut sum = Decimal::new(0, 2);
    let mut any = false;
    for ctgy in &body.boq_ctgy {
        parts.push(
            ctgy.rno_part
                .map(|s| doc.str(s).to_string())
                .unwrap_or_default(),
        );
        path.push(ctgy);
        let computed = ctgy
            .boq_body
            .as_ref()
            .and_then(|b| sum_body(doc, breakdown, b, path, parts, out));
        let oz = breakdown.category_oz(&parts.iter().map(String::as_str).collect::<Vec<_>>());
        out.push(CategoryTotal {
            oz: oz.dotted(),
            depth: parts.len() - 1,
            computed,
            declared: ctgy.totals.as_ref().and_then(|t| t.total),
        });
        path.pop();
        parts.pop();
        if let Some(c) = computed {
            sum = sum.checked_add(c)?;
            any = true;
        }
    }
    if let Some(list) = &body.itemlist {
        for item in &list.item {
            let r = ItemRef {
                doc,
                item,
                oz: Oz::default_for(),
                path: path.clone(),
            };
            if !r.counts_towards_total() {
                continue;
            }
            let Some(total) = r.total().or_else(|| r.computed_total()) else {
                continue;
            };
            sum = sum.checked_add(total)?;
            any = true;
        }
        for markup in &list.markup_item {
            let Some(total) = markup.it else { continue };
            sum = sum.checked_add(total)?;
            any = true;
        }
    }
    any.then_some(sum.rescale(2))
}

impl Oz {
    fn default_for() -> Oz {
        Breakdown::default().category_oz(&[])
    }
}

enum Event<'a> {
    Category(CategoryRef<'a>),
    Item(ItemRef<'a>),
    Markup(MarkupRef<'a>),
}

fn walk<'a>(
    doc: &'a Document,
    breakdown: &Breakdown,
    body: &'a BoQBody,
    path: &mut Vec<&'a BoQCtgy>,
    parts: &mut Vec<String>,
    visit: &mut dyn FnMut(Event<'a>),
) {
    for ctgy in &body.boq_ctgy {
        parts.push(
            ctgy.rno_part
                .map(|s| doc.str(s).to_string())
                .unwrap_or_default(),
        );
        let part_refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        let oz = breakdown.category_oz(&part_refs);
        let label = ctgy
            .lbl_tx
            .as_ref()
            .map(|l| plain(doc, l.node))
            .unwrap_or_default();
        visit(Event::Category(CategoryRef {
            oz,
            label,
            ctgy,
            depth: parts.len() - 1,
            path: path.clone(),
        }));
        path.push(ctgy);
        if let Some(inner) = &ctgy.boq_body {
            walk(doc, breakdown, inner, path, parts, visit);
        }
        path.pop();
        parts.pop();
    }
    let Some(list) = &body.itemlist else { return };
    let part_refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    for item in &list.item {
        let rno = item.rno_part.map(|s| doc.str(s)).unwrap_or("");
        let index = item.rno_index.map(|s| doc.str(s));
        let oz = breakdown.oz(&part_refs, rno, index);
        visit(Event::Item(ItemRef {
            doc,
            item,
            oz,
            path: path.clone(),
        }));
    }
    for markup in &list.markup_item {
        let rno = markup.rno_part.map(|s| doc.str(s)).unwrap_or("");
        let index = markup.rno_index.map(|s| doc.str(s));
        let oz = breakdown.oz(&part_refs, rno, index);
        visit(Event::Markup(MarkupRef {
            doc,
            item: markup,
            oz,
            path: path.clone(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::decimal::Decimal;

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).unwrap()
    }

    fn bvbs_x84() -> Document {
        let path = format!(
            "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
            env!("CARGO_MANIFEST_DIR")
        );
        Document::open(std::path::Path::new(&path)).unwrap()
    }

    fn tender(items: &str) -> Document {
        let xml = format!(
            "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Index</Type><Length>1</Length><Num>No</Num></BoQBkdn></BoQInfo><BoQBody><BoQCtgy ID=\"c1\" RNoPart=\"1\"><LblTx><p><span>Rohbau</span></p></LblTx><BoQBody><Itemlist>{items}</Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>"
        );
        Document::parse(xml.as_bytes()).unwrap()
    }

    fn item(id: &str, rno: &str, extra: &str) -> String {
        format!(
            "<Item ID=\"{id}\" RNoPart=\"{rno}\">{extra}<Qty>2.000</Qty><QU>m</QU><Description><CompleteText><DetailTxt><Text><p><span>Langtext {id}</span></p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><p><span>Kurz {id}</span></p></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item>"
        )
    }

    #[test]
    fn oz_from_bvbs_items() {
        let doc = bvbs_x84();
        let items: Vec<ItemRef<'_>> = doc.items().collect();
        assert_eq!(items[0].oz.dotted(), "001.001.0010");
        assert_eq!(items[1].oz.dotted(), "001.001.00101");
        assert_eq!(items[2].oz.dotted(), "001.001.0010A");
        assert_eq!(items[0].id(), "ID02BED3D8");
        assert_eq!(items[0].unit_price(), Some(d("5000.000")));
        assert_eq!(items[0].total(), Some(d("5000.00")));
        assert_eq!(items[0].path.len(), 2);
    }

    #[test]
    fn items_iterate_in_document_order_across_nested_categories() {
        let doc = bvbs_x84();
        let ozs: Vec<String> = doc.items().map(|i| i.oz.dotted()).collect();
        assert_eq!(ozs.len(), 27);
        let mut sorted = ozs.clone();
        sorted.sort();
        assert_eq!(ozs, sorted);
        assert_eq!(ozs.last().map(String::as_str), Some("999.999.9999z"));
    }

    #[test]
    fn categories_carry_label_depth_and_oz() {
        let doc = tender(&item("i1", "10", ""));
        let cats: Vec<CategoryRef<'_>> = doc.categories().collect();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].oz.dotted(), "01");
        assert_eq!(cats[0].label, "Rohbau");
        assert_eq!(cats[0].depth, 0);
        let bvbs = bvbs_x84();
        assert_eq!(bvbs.categories().count(), 12);
        assert_eq!(bvbs.categories().filter(|c| c.depth == 0).count(), 3);
    }

    #[test]
    fn item_kinds_and_counting() {
        let doc = tender(&format!(
            "{}{}{}{}{}{}{}",
            item("std", "10", ""),
            item("lump", "20", "<LumpSumItem>Yes</LumpSumItem>"),
            item("bed", "30", "<Provis>WithoutTotal</Provis>"),
            item("bedgb", "40", "<Provis>WithTotal</Provis>"),
            item(
                "alt",
                "50",
                "<ALNGroupNo>12</ALNGroupNo><ALNSerNo>1</ALNSerNo>"
            ),
            item(
                "base",
                "60",
                "<ALNGroupNo>12</ALNGroupNo><ALNSerNo>0</ALNSerNo>"
            ),
            item("na", "70", "<NotAppl>Yes</NotAppl>"),
        ));
        let kinds: Vec<(ItemKind, bool)> = doc
            .items()
            .map(|i| (i.kind(), i.counts_towards_total()))
            .collect();
        assert_eq!(kinds[0], (ItemKind::Standard, true));
        assert_eq!(kinds[1], (ItemKind::LumpSum, true));
        assert_eq!(kinds[2], (ItemKind::ProvisionalWithoutTotal, false));
        assert_eq!(kinds[3], (ItemKind::ProvisionalWithTotal, true));
        assert_eq!(
            kinds[4],
            (
                ItemKind::Alternative {
                    group: 12,
                    serial: 1
                },
                false
            )
        );
        assert_eq!(kinds[5], (ItemKind::Base { group: 12 }, true));
        assert_eq!(kinds[6], (ItemKind::NotApplicable, false));
    }

    #[test]
    fn free_quantity_item_has_no_quantity_and_no_total() {
        let doc = tender(
            "<Item ID=\"f\" RNoPart=\"10\"><QtyTBD>Yes</QtyTBD><QU>kg</QU><UP>1.500</UP></Item>",
        );
        let items: Vec<ItemRef<'_>> = doc.items().collect();
        assert_eq!(items[0].kind(), ItemKind::FreeQuantity);
        assert_eq!(items[0].quantity(), None);
        assert_eq!(items[0].unit(), Some("kg"));
        assert_eq!(items[0].total(), None);
        assert_eq!(items[0].computed_total(), None);
    }

    #[test]
    fn texts_short_long_and_markdown() {
        let doc = tender(&item("i1", "10", ""));
        let items: Vec<ItemRef<'_>> = doc.items().collect();
        assert_eq!(items[0].short_text(), "Kurz i1");
        assert_eq!(items[0].long_text(), "Langtext i1");
        assert_eq!(items[0].long_text_markdown(), "Langtext i1");
        assert_eq!(items[0].quantity(), Some(d("2.000")));
        assert_eq!(items[0].unit(), Some("m"));
    }

    #[test]
    fn computed_totals_match_bvbs_declared() {
        let doc = bvbs_x84();
        let totals = doc.computed_totals();
        assert_eq!(totals.boq, Some(d("2000000.00")));
        assert_eq!(doc.declared_total(), Some(d("2000000.00")));
        let first = totals.categories.iter().find(|c| c.oz == "001").unwrap();
        assert_eq!(first.computed, Some(d("1054734.03")));
        assert_eq!(first.declared, Some(d("1054734.03")));
        assert!(
            totals.categories.iter().all(|c| c.computed == c.declared),
            "{:?}",
            totals.categories
        );
    }

    #[test]
    fn markup_items_are_listed_with_their_amounts() {
        let doc = bvbs_x84();
        let markups: Vec<MarkupRef<'_>> = doc.markup_items().collect();
        assert_eq!(markups.len(), 1);
        assert_eq!(markups[0].oz.dotted(), "002.001.0030");
        assert_eq!(markups[0].total(), Some(d("85000.00")));
        assert_eq!(markups[0].percent(), Some(d("10.00")));
    }

    #[test]
    fn musterdatei_31_items_have_short_and_long_text() {
        let path = format!(
            "{}/../../tests/fixtures/official/3.1/Musterdatei_3.1_2008-001.X83",
            env!("CARGO_MANIFEST_DIR")
        );
        let doc = Document::open(std::path::Path::new(&path)).unwrap();
        let items: Vec<ItemRef<'_>> = doc.items().collect();
        assert!(items.len() > 50, "{}", items.len());
        let with_short = items.iter().filter(|i| !i.short_text().is_empty()).count();
        let with_long = items.iter().filter(|i| !i.long_text().is_empty()).count();
        assert!(
            with_short * 10 >= items.len() * 9,
            "{with_short}/{}",
            items.len()
        );
        assert!(
            with_long * 10 >= items.len() * 9,
            "{with_long}/{}",
            items.len()
        );
        assert!(items.iter().all(|i| !i.oz.dotted().is_empty()));
    }
}
