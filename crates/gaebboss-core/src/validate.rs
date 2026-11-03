//! Validation: phase and schema rules, identifiers, Ordnungszahlen,
//! arithmetic, and a bid against its tender.

use std::collections::{HashMap, HashSet};

use crate::Document;
use crate::boq::{BoqTotals, ItemKind, ItemRef, LeftOut, category_counts_towards_total};
use crate::decimal::{Decimal, item_total};
use crate::name::Name;
use crate::tape::NodeId;
use crate::version::{Phase, Version};
use crate::write::WriteOptions;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FindingCode {
    NotAllowedInPhase,
    MissingRequired,
    PhaseRulesNotChecked,
    DecimalDigits,
    PatternRNoPart,
    DuplicateId,
    DanglingIdRef,
    OzNotUnique,
    OzNotAscending,
    OzTooLong,
    ItemTotalMismatch,
    CategoryTotalMismatch,
    BoqTotalMismatch,
    UpWithoutOffer,
    QtyOnNonFreeItemInX84,
    StructureDiffersFromTender,
    IdDiffersFromTender,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub code: FindingCode,
    pub node: Option<NodeId>,
    pub oz: Option<String>,
    pub message: String,
}

fn finding(
    severity: Severity,
    code: FindingCode,
    node: Option<NodeId>,
    oz: Option<String>,
    message: String,
) -> Finding {
    Finding {
        severity,
        code,
        node,
        oz,
        message,
    }
}

/// Every rule the document can be checked against on its own.
pub fn validate(doc: &Document) -> Vec<Finding> {
    let mut out = Vec::new();
    phase_rules(doc, &mut out);
    identifiers(doc, &mut out);
    items(doc, &mut out);
    totals(&doc.computed_totals(), &mut out);
    out
}

/// A bid (X84) checked like `validate` does, its totals summed the way the
/// tender (X83) says (an X84 does not mark alternative or not-applicable
/// items and categories, the tender does), and then against the tender as
/// `validate_against_tender` does.
pub fn validate_with_tender(bid: &Document, tender: &Document) -> Vec<Finding> {
    let mut out = Vec::new();
    phase_rules(bid, &mut out);
    identifiers(bid, &mut out);
    items(bid, &mut out);
    totals(
        &bid.computed_totals_except(&left_out(bid, tender)),
        &mut out,
    );
    out.extend(validate_against_tender(bid, tender));
    out
}

/// The tender's items, looked up the way a bid answers them: by ID, else
/// by Ordnungszahl.
struct TenderItems<'t> {
    items: Vec<ItemRef<'t>>,
    by_id: HashMap<String, usize>,
    by_oz: HashMap<String, usize>,
}

impl<'t> TenderItems<'t> {
    fn new(tender: &'t Document) -> TenderItems<'t> {
        let items: Vec<ItemRef<'t>> = tender.items().collect();
        let by_id = items
            .iter()
            .enumerate()
            .filter(|(_, i)| !i.id().is_empty())
            .map(|(index, i)| (i.id().to_string(), index))
            .collect();
        let by_oz = items
            .iter()
            .enumerate()
            .map(|(index, i)| (i.oz.compact(), index))
            .collect();
        TenderItems {
            items,
            by_id,
            by_oz,
        }
    }

    fn answered_by(&self, item: &ItemRef<'_>) -> Option<&ItemRef<'t>> {
        let index = self
            .by_id
            .get(item.id())
            .or_else(|| self.by_oz.get(&item.oz.compact()))?;
        self.items.get(*index)
    }
}

/// The bid's items and categories the tender keeps out of the sums: items
/// whose tender item does not count and categories the tender makes
/// alternatives.
fn left_out(bid: &Document, tender: &Document) -> LeftOut {
    let tender_items = TenderItems::new(tender);
    let alternatives: HashSet<String> = tender
        .categories()
        .filter(|c| !category_counts_towards_total(c.ctgy))
        .map(|c| c.oz.dotted())
        .collect();
    LeftOut {
        items: bid
            .items()
            .filter(|i| {
                tender_items
                    .answered_by(i)
                    .is_some_and(|t| !t.counts_towards_total())
            })
            .map(|i| i.item.node)
            .collect(),
        categories: bid
            .categories()
            .filter(|c| alternatives.contains(&c.oz.dotted()))
            .map(|c| c.ctgy.node)
            .collect(),
    }
}

fn phase_rules(doc: &Document, out: &mut Vec<Finding>) {
    let (Some(phase), version) = (doc.phase(), doc.version()) else {
        out.push(finding(
            Severity::Info,
            FindingCode::PhaseRulesNotChecked,
            None,
            None,
            "no exchange phase known; phase rules not checked".to_string(),
        ));
        return;
    };
    if version < Version::V32 {
        out.push(finding(
            Severity::Info,
            FindingCode::PhaseRulesNotChecked,
            None,
            None,
            format!(
                "phase rules are checked for 3.2 and 3.3 only, this file is {}",
                version.as_str()
            ),
        ));
        return;
    }
    let opts = WriteOptions {
        version,
        phase,
        indent: None,
        prog_system: None,
        strict: false,
    };
    let Ok((_, report)) = doc.write_with_report(&opts) else {
        return;
    };
    for d in report.dropped {
        out.push(finding(
            Severity::Error,
            FindingCode::NotAllowedInPhase,
            None,
            None,
            format!(
                "<{}> is not allowed in <{}> for phase {} ({} occurrence(s))",
                d.element,
                d.type_name,
                phase.code(),
                d.count
            ),
        ));
    }
    for m in report.missing {
        out.push(finding(
            Severity::Error,
            FindingCode::MissingRequired,
            Some(m.node),
            None,
            format!(
                "<{}> lacks the required <{}> for phase {}",
                m.type_name,
                m.element.as_str(),
                phase.code()
            ),
        ));
    }
}

fn identifiers(doc: &Document, out: &mut Vec<Finding>) {
    let tape = doc.tape();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut references: Vec<(NodeId, &str)> = Vec::new();
    for (index, node) in tape.nodes.iter().enumerate() {
        let id = index as NodeId;
        for attr in tape.attrs(id) {
            match attr.name {
                Name::AttrId => {
                    let value = doc.str(attr.value);
                    if !seen.insert(value) {
                        out.push(finding(
                            Severity::Error,
                            FindingCode::DuplicateId,
                            Some(id),
                            None,
                            format!(
                                "ID \"{value}\" on <{}> is used more than once",
                                doc.str(node.raw_name)
                            ),
                        ));
                    }
                }
                Name::AttrIDRef => references.push((id, doc.str(attr.value))),
                _ => {}
            }
        }
    }
    for (id, value) in references {
        if seen.contains(value) {
            continue;
        }
        out.push(finding(
            Severity::Error,
            FindingCode::DanglingIdRef,
            Some(id),
            None,
            format!("IDRef \"{value}\" points to no ID in the file"),
        ));
    }
}

/// The sort rank of an Ordnungszahl character (Fachdokumentation 4.2.1).
fn rank(c: char) -> u32 {
    match c {
        ' ' => 0,
        '0'..='9' => 1 + (c as u32 - '0' as u32),
        'A'..='Z' => 11 + (c as u32 - 'A' as u32),
        '_' => 37,
        'a'..='z' => 38 + (c as u32 - 'a' as u32),
        _ => 64,
    }
}

fn oz_key(compact: &str) -> Vec<u32> {
    compact.chars().map(rank).collect()
}

fn valid_rno(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 14
        && text.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn items(doc: &Document, out: &mut Vec<Finding>) {
    let x84 = doc.phase() == Some(Phase::X84);
    let mut previous: Option<(Vec<u32>, String)> = None;
    let mut seen: HashSet<String> = HashSet::new();
    for item in doc.items() {
        let oz = item.oz.dotted();
        let node = Some(item.item.node);
        let rno = item.oz.item_part().trim();
        if !valid_rno(rno) {
            out.push(finding(
                Severity::Error,
                FindingCode::PatternRNoPart,
                node,
                Some(oz.clone()),
                format!("RNoPart {rno:?} may only hold digits, letters and underscores (1 to 14)"),
            ));
        }
        let compact = item.oz.compact();
        if compact.chars().count() > 14 {
            out.push(finding(
                Severity::Error,
                FindingCode::OzTooLong,
                node,
                Some(oz.clone()),
                format!("Ordnungszahl {compact:?} is longer than 14 characters"),
            ));
        }
        if !seen.insert(compact.clone()) {
            out.push(finding(
                Severity::Error,
                FindingCode::OzNotUnique,
                node,
                Some(oz.clone()),
                format!("Ordnungszahl {oz} appears more than once"),
            ));
        }
        let key = oz_key(&compact);
        if let Some((prev_key, prev_oz)) = &previous
            && key < *prev_key
        {
            out.push(finding(
                Severity::Error,
                FindingCode::OzNotAscending,
                node,
                Some(oz.clone()),
                format!("Ordnungszahl {oz} follows {prev_oz} but sorts before it"),
            ));
        }
        previous = Some((key, oz.clone()));
        digits(&item, "Qty", item.quantity(), 3, out);
        digits(&item, "UP", item.unit_price(), 3, out);
        digits(&item, "IT", item.total(), 2, out);
        digits(&item, "VAT", item.vat_percent(), 2, out);
        if let (Some(qty), Some(up), Some(it)) = (item.quantity(), item.unit_price(), item.total())
            && let (Some(product), Some(expected)) = (item_total(qty, up), item.computed_total())
            && expected != it
        {
            let discount = item
                .discount_percent()
                .map(|pct| format!(", less {pct} % = {expected}"))
                .unwrap_or_default();
            out.push(finding(
                Severity::Error,
                FindingCode::ItemTotalMismatch,
                node,
                Some(oz.clone()),
                format!("IT is {it} but Qty × UP = {qty} × {up} = {product}{discount}"),
            ));
        }
        if x84 && item.not_offered() && item.unit_price().is_some() {
            out.push(finding(
                Severity::Error,
                FindingCode::UpWithoutOffer,
                node,
                Some(oz.clone()),
                "item is marked NotOffered but carries a unit price".to_string(),
            ));
        }
    }
}

fn digits(
    item: &ItemRef<'_>,
    label: &str,
    value: Option<Decimal>,
    max_scale: u8,
    out: &mut Vec<Finding>,
) {
    let Some(value) = value else { return };
    if value.scale() <= max_scale {
        return;
    }
    out.push(finding(
        Severity::Error,
        FindingCode::DecimalDigits,
        Some(item.item.node),
        Some(item.oz.dotted()),
        format!("{label} {value} has more than {max_scale} decimal places"),
    ));
}

fn totals(totals: &BoqTotals, out: &mut Vec<Finding>) {
    for c in &totals.categories {
        let (Some(declared), Some(computed)) = (c.declared, c.computed) else {
            continue;
        };
        if declared == computed {
            continue;
        }
        out.push(finding(
            Severity::Error,
            FindingCode::CategoryTotalMismatch,
            None,
            Some(c.oz.clone()),
            format!(
                "category {} declares {declared} but its items sum to {computed}",
                c.oz
            ),
        ));
    }
    if let (Some(declared), Some(computed)) = (totals.declared, totals.boq)
        && declared != computed
    {
        out.push(finding(
            Severity::Error,
            FindingCode::BoqTotalMismatch,
            None,
            None,
            format!("the BoQ declares {declared} but its categories sum to {computed}"),
        ));
    }
}

/// A bid (X84) against the tender (X83) it answers: same categories, same
/// items, quantities unchanged unless the tender left them to the bidder.
pub fn validate_against_tender(bid: &Document, tender: &Document) -> Vec<Finding> {
    let mut out = Vec::new();
    let tender_categories: Vec<String> = tender.categories().map(|c| c.oz.dotted()).collect();
    let bid_categories: Vec<String> = bid.categories().map(|c| c.oz.dotted()).collect();
    if tender_categories != bid_categories {
        out.push(finding(
            Severity::Error,
            FindingCode::StructureDiffersFromTender,
            None,
            None,
            format!(
                "the bid has categories {bid_categories:?} but the tender has {tender_categories:?}"
            ),
        ));
    }
    let tender_items = TenderItems::new(tender);
    let mut matched: HashSet<String> = HashSet::new();
    for item in bid.items() {
        let oz = item.oz.dotted();
        let node = Some(item.item.node);
        let Some(tender_item) = tender_items.answered_by(&item) else {
            out.push(finding(
                Severity::Error,
                FindingCode::StructureDiffersFromTender,
                node,
                Some(oz.clone()),
                format!("item {} ({oz}) is not in the tender", item.id()),
            ));
            continue;
        };
        matched.insert(tender_item.oz.compact());
        if tender_item.oz.compact() != item.oz.compact() {
            out.push(finding(
                Severity::Error,
                FindingCode::StructureDiffersFromTender,
                node,
                Some(oz.clone()),
                format!(
                    "item {} is {} in the bid but {} in the tender",
                    item.id(),
                    oz,
                    tender_item.oz
                ),
            ));
        }
        if tender_item.id() != item.id() {
            out.push(finding(
                Severity::Warning,
                FindingCode::IdDiffersFromTender,
                node,
                Some(oz.clone()),
                format!(
                    "item {oz} has ID {} in the bid but {} in the tender",
                    item.id(),
                    tender_item.id()
                ),
            ));
        }
        let free = tender_item.kind() == ItemKind::FreeQuantity || tender_item.quantity().is_none();
        if let Some(qty) = item.quantity()
            && !free
            && tender_item.quantity() != Some(qty)
        {
            out.push(finding(
                Severity::Error,
                FindingCode::QtyOnNonFreeItemInX84,
                node,
                Some(oz.clone()),
                format!(
                    "item {oz} bids quantity {qty} but the tender fixes {}",
                    tender_item
                        .quantity()
                        .map(|q| q.to_string())
                        .unwrap_or_default()
                ),
            ));
        }
    }
    for tender_item in &tender_items.items {
        if matched.contains(&tender_item.oz.compact()) {
            continue;
        }
        out.push(finding(
            Severity::Error,
            FindingCode::StructureDiffersFromTender,
            Some(tender_item.item.node),
            Some(tender_item.oz.dotted()),
            format!(
                "tender item {} ({}) is missing from the bid",
                tender_item.id(),
                tender_item.oz
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    fn x84(items: &str) -> Document {
        let xml = format!(
            "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA84/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><Award><DP>84</DP><CTR><Address><Name1>B</Name1><Street>s</Street><PCode>1</PCode><City>c</City></Address></CTR><BoQ ID=\"b\"><BoQInfo><Name>x</Name><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn><Totals><Total>0.00</Total></Totals></BoQInfo><BoQBody><BoQCtgy ID=\"c1\" RNoPart=\"1\"><BoQBody><Itemlist>{items}</Itemlist></BoQBody><Totals><Total>0.00</Total></Totals></BoQCtgy></BoQBody></BoQ></Award></GAEB>"
        );
        Document::parse(xml.as_bytes()).unwrap()
    }

    fn errors(doc: &Document) -> Vec<Finding> {
        validate(doc)
            .into_iter()
            .filter(|f| f.severity == Severity::Error)
            .collect()
    }

    fn codes(findings: &[Finding]) -> Vec<FindingCode> {
        findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn provis_in_x84_is_not_allowed() {
        let doc = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><Provis>WithTotal</Provis><UP>1.000</UP><IT>0.00</IT></Item>",
        );
        let found = errors(&doc);
        assert!(
            codes(&found).contains(&FindingCode::NotAllowedInPhase),
            "{found:?}"
        );
        assert!(found.iter().any(|f| f.message.contains("Provis")));
    }

    #[test]
    fn missing_required_is_reported() {
        let xml = "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA84/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><Award><DP>84</DP><BoQ ID=\"b\"><BoQInfo><Name>x</Name><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn><Totals><Total>0.00</Total></Totals></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"10\"><UP>1.000</UP></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
        let doc = Document::parse(xml.as_bytes()).unwrap();
        let found = errors(&doc);
        assert!(
            found
                .iter()
                .any(|f| f.code == FindingCode::MissingRequired && f.message.contains("CTR")),
            "{found:?}"
        );
    }

    #[test]
    fn item_total_mismatch_is_reported_with_oz() {
        let doc = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><Qty>2.000</Qty><UP>1.500</UP><IT>4.00</IT></Item>",
        );
        let found = errors(&doc);
        let mismatch = found
            .iter()
            .find(|f| f.code == FindingCode::ItemTotalMismatch)
            .unwrap();
        assert_eq!(mismatch.oz.as_deref(), Some("01.0010"));
        assert!(mismatch.message.contains("3.00"), "{}", mismatch.message);
    }

    #[test]
    fn item_total_mismatch_counts_the_item_discount() {
        let doc = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><Qty>40.000</Qty><UP>20.000</UP><DiscountPcnt>10.00</DiscountPcnt><IT>720.00</IT></Item><Item ID=\"i2\" RNoPart=\"20\"><Qty>40.000</Qty><UP>20.000</UP><DiscountPcnt>10.00</DiscountPcnt><IT>800.00</IT></Item>",
        );
        let found = errors(&doc);
        let mismatches: Vec<&Finding> = found
            .iter()
            .filter(|f| f.code == FindingCode::ItemTotalMismatch)
            .collect();
        assert_eq!(mismatches.len(), 1, "{found:?}");
        assert_eq!(mismatches[0].oz.as_deref(), Some("01.0020"));
        assert!(
            mismatches[0]
                .message
                .ends_with("= 800.00, less 10.00 % = 720.00"),
            "{}",
            mismatches[0].message
        );
    }

    #[test]
    fn category_and_boq_total_mismatch_are_reported() {
        let doc = x84("<Item ID=\"i1\" RNoPart=\"10\"><UP>1.500</UP><IT>3.00</IT></Item>");
        let found = errors(&doc);
        assert!(
            codes(&found).contains(&FindingCode::CategoryTotalMismatch),
            "{found:?}"
        );
        assert!(
            codes(&found).contains(&FindingCode::BoqTotalMismatch),
            "{found:?}"
        );
    }

    #[test]
    fn up_on_not_offered_item_is_an_error() {
        let doc =
            x84("<Item ID=\"i1\" RNoPart=\"10\"><NotOffered>Yes</NotOffered><UP>1.500</UP></Item>");
        assert!(codes(&errors(&doc)).contains(&FindingCode::UpWithoutOffer));
    }

    #[test]
    fn duplicate_id_and_dangling_idref_are_reported() {
        let doc = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><UP>1.000</UP><IT>0.00</IT></Item><Item ID=\"i1\" RNoPart=\"20\"><UP>1.000</UP><IT>0.00</IT></Item><MarkupItem ID=\"m1\" RNoPart=\"30\"><MarkupSubQty><RefItem IDRef=\"nope\"/></MarkupSubQty><ITMarkup>0.00</ITMarkup><IT>0.00</IT></MarkupItem>",
        );
        let found = errors(&doc);
        assert!(
            codes(&found).contains(&FindingCode::DuplicateId),
            "{found:?}"
        );
        assert!(
            codes(&found).contains(&FindingCode::DanglingIdRef),
            "{found:?}"
        );
    }

    #[test]
    fn rno_part_with_umlaut_fails_pattern_and_oz_order_is_checked() {
        let doc = x84(
            "<Item ID=\"i1\" RNoPart=\"20\"><UP>1.000</UP><IT>0.00</IT></Item><Item ID=\"i2\" RNoPart=\"10\"><UP>1.000</UP><IT>0.00</IT></Item><Item ID=\"i3\" RNoPart=\"1ä\"><UP>1.000</UP><IT>0.00</IT></Item><Item ID=\"i4\" RNoPart=\"10\"><UP>1.000</UP><IT>0.00</IT></Item>",
        );
        let found = errors(&doc);
        assert!(
            codes(&found).contains(&FindingCode::PatternRNoPart),
            "{found:?}"
        );
        assert!(
            codes(&found).contains(&FindingCode::OzNotAscending),
            "{found:?}"
        );
        assert!(
            codes(&found).contains(&FindingCode::OzNotUnique),
            "{found:?}"
        );
    }

    #[test]
    fn oz_too_long_is_reported() {
        let doc =
            x84("<Item ID=\"i1\" RNoPart=\"12345678901234\"><UP>1.000</UP><IT>0.00</IT></Item>");
        assert!(codes(&errors(&doc)).contains(&FindingCode::OzTooLong));
    }

    #[test]
    fn price_scales_beyond_the_facets_are_reported() {
        let doc = x84("<Item ID=\"i1\" RNoPart=\"10\"><UP>1.0005</UP><IT>0.001</IT></Item>");
        let found = errors(&doc);
        assert_eq!(
            found
                .iter()
                .filter(|f| f.code == FindingCode::DecimalDigits)
                .count(),
            2,
            "{found:?}"
        );
    }

    #[test]
    fn a_clean_x84_has_no_errors() {
        let doc = x84("<Item ID=\"i1\" RNoPart=\"10\"><UP>1.500</UP><IT>3.00</IT></Item>");
        let xml = String::from_utf8(
            doc.write(&crate::write::WriteOptions {
                version: crate::version::Version::V33,
                phase: crate::version::Phase::X84,
                indent: None,
                prog_system: None,
                strict: false,
            })
            .unwrap(),
        )
        .unwrap();
        let fixed = xml.replace("<Total>0.00</Total>", "<Total>3.00</Total>");
        let doc = Document::parse(fixed.as_bytes()).unwrap();
        assert!(errors(&doc).is_empty(), "{:?}", errors(&doc));
    }

    #[test]
    fn bid_against_tender_checks_structure_and_quantities() {
        let tender_xml = "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><Name>x</Name><LblBoQ>T</LblBoQ><OutlCompl>AllTxt</OutlCompl><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody><BoQCtgy ID=\"c1\" RNoPart=\"1\"><LblTx><p><span>A</span></p></LblTx><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"10\"><Qty>2.000</Qty><QU>m</QU></Item><Item ID=\"i2\" RNoPart=\"20\"><QtyTBD>Yes</QtyTBD><QU>kg</QU></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>";
        let tender = Document::parse(tender_xml.as_bytes()).unwrap();
        let good = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><UP>1.000</UP><IT>2.00</IT></Item><Item ID=\"i2\" RNoPart=\"20\"><Qty>5.000</Qty><UP>1.000</UP><IT>5.00</IT></Item>",
        );
        let findings = validate_against_tender(&good, &tender);
        assert!(
            findings.iter().all(|f| f.severity != Severity::Error),
            "{findings:?}"
        );
        let bad = x84(
            "<Item ID=\"i1\" RNoPart=\"10\"><Qty>3.000</Qty><UP>1.000</UP><IT>3.00</IT></Item><Item ID=\"i9\" RNoPart=\"90\"><UP>1.000</UP><IT>0.00</IT></Item>",
        );
        let findings = validate_against_tender(&bad, &tender);
        assert!(
            codes(&findings).contains(&FindingCode::QtyOnNonFreeItemInX84),
            "{findings:?}"
        );
        assert!(
            codes(&findings).contains(&FindingCode::StructureDiffersFromTender),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.code == FindingCode::StructureDiffersFromTender
                    && f.message.contains("i2")),
            "{findings:?}"
        );
    }
}
