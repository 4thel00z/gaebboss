//! From a tender (X83) to a priced bid (X84): prices, bidder text
//! complements, free quantities, not-offered items and the totals the
//! Fachdokumentation prescribes, written strictly in the tender's version.

use crate::Document;
use crate::boq::{ItemKind, counts_towards_total};
use crate::decimal::{Decimal, item_total, total_after_discount};
use crate::document::Resolver;
use crate::model::{
    Address, BoQBody, BoQCtgy, BoQText, CTR, GAEB, HtmlLiTC, HtmlPTC, HtmlSpan, HtmlTdTC, Item,
    MLText, MarkupItem, TextComplement, Totals,
};
use crate::span::Span;
use crate::text::ComplementKind;
use crate::version::{Phase, Version};
use crate::write::{WriteError, WriteOptions};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bidder {
    pub name1: String,
    pub name2: Option<String>,
    pub street: String,
    pub pcode: String,
    pub city: String,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub bidder_no: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BidError {
    #[error("not a tender: phase {}", .0.map(|p| p.code()).unwrap_or("unknown"))]
    NotATender(Option<Phase>),
    #[error("no item with ID {0}")]
    UnknownItem(String),
    #[error("the field is not a bidder field")]
    NotBidderField,
    #[error("the tender fixes this item's quantity")]
    NotFreeQuantity,
    #[error("the unit price components do not sum to the unit price")]
    ComponentsDoNotSum,
    #[error("more than six unit price components")]
    TooManyComponents,
    #[error("a price with more than three decimal places")]
    TooManyDecimals,
    #[error("items without a price or a not-offered mark: {}", .0.join(", "))]
    Unpriced(Vec<String>),
    #[error("arithmetic overflow")]
    Overflow,
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Parse(#[from] crate::error::Error),
}

/// A bid under construction: the tender's own tree, re-labelled X84, with
/// the bidder's prices and entries applied.
pub struct Bid {
    doc: Document,
    discount: Option<Decimal>,
    vat: Option<Decimal>,
}

impl Document {
    /// How this document is written as a bid: X84 in its own version
    /// (3.2 for tenders older than that), strictly, signed by gaebboss.
    pub fn x84_options(&self) -> WriteOptions {
        let version = match self.version() {
            Version::V30 | Version::V31 | Version::V32 => Version::V32,
            Version::V33 => Version::V33,
        };
        WriteOptions {
            version,
            phase: Phase::X84,
            indent: None,
            prog_system: Some(format!("gaebboss {}", env!("CARGO_PKG_VERSION"))),
            strict: true,
        }
    }
}

impl Bid {
    /// Starts a bid from a tender (X83; X81 and X82 are accepted too).
    pub fn from_tender(tender: &Document, bidder: Bidder) -> Result<Bid, BidError> {
        if !matches!(tender.phase(), Some(Phase::X81 | Phase::X82 | Phase::X83)) {
            return Err(BidError::NotATender(tender.phase()));
        }
        let mut doc = Document::parse(tender.bytes())?;
        doc.set_phase(Phase::X84);
        let name1 = doc.intern(&bidder.name1);
        let name2 = bidder.name2.as_deref().map(|t| doc.intern(t));
        let street = doc.intern(&bidder.street);
        let p_code = doc.intern(&bidder.pcode);
        let city = doc.intern(&bidder.city);
        let contact = bidder.contact.as_deref().map(|t| doc.intern(t));
        let email = bidder.email.as_deref().map(|t| doc.intern(t));
        let bidder_no = bidder.bidder_no.as_deref().map(|t| doc.intern(t));
        let (gaeb, _) = doc.edit();
        let award = gaeb.award.get_or_insert_with(Default::default);
        let ctr = award.ctr.get_or_insert_with(CTR::default);
        ctr.address = Some(Address {
            name1: Some(name1),
            name2,
            street: Some(street),
            p_code: Some(p_code),
            city: Some(city),
            contact,
            email,
            ..Address::default()
        });
        if bidder_no.is_some() {
            ctr.bidder_no = bidder_no;
        }
        Ok(Bid {
            doc,
            discount: None,
            vat: None,
        })
    }

    /// The bid as it stands.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    fn with_item<T>(
        &mut self,
        id: &str,
        f: impl FnOnce(&mut Item) -> Result<T, BidError>,
    ) -> Result<T, BidError> {
        let (gaeb, resolver) = self.doc.edit();
        let Some(item) = find_item(gaeb, resolver, id) else {
            return Err(BidError::UnknownItem(id.to_string()));
        };
        f(item)
    }

    /// Sets the unit price; the total follows from the quantity when known.
    pub fn price(&mut self, id: &str, up: Decimal) -> Result<(), BidError> {
        let up = price_scale(up)?;
        self.with_item(id, |item| {
            item.up = Some(up);
            item.not_offered = None;
            item.it = item.qty.and_then(|q| item_total(q, up));
            Ok(())
        })
    }

    /// Up to six unit price components (wages, materials, ...) that must
    /// add up to the unit price; sets the unit price when none is set yet.
    pub fn price_components(&mut self, id: &str, components: &[Decimal]) -> Result<(), BidError> {
        if components.len() > 6 {
            return Err(BidError::TooManyComponents);
        }
        let scaled: Vec<Decimal> = components
            .iter()
            .map(|c| price_scale(*c))
            .collect::<Result<_, _>>()?;
        let sum = scaled
            .iter()
            .try_fold(Decimal::new(0, 3), |acc, c| acc.checked_add(*c))
            .ok_or(BidError::Overflow)?;
        self.with_item(id, |item| {
            if item.up.is_some_and(|up| up != sum) {
                return Err(BidError::ComponentsDoNotSum);
            }
            item.up = Some(sum);
            item.it = item.qty.and_then(|q| item_total(q, sum));
            let slots = [
                &mut item.up_comp1,
                &mut item.up_comp2,
                &mut item.up_comp3,
                &mut item.up_comp4,
                &mut item.up_comp5,
                &mut item.up_comp6,
            ];
            for (i, slot) in slots.into_iter().enumerate() {
                *slot = scaled.get(i).copied();
            }
            Ok(())
        })
    }

    /// Marks an item as not offered: no unit price is transmitted.
    pub fn not_offered(&mut self, id: &str) -> Result<(), BidError> {
        self.with_item(id, |item| {
            item.not_offered = Some(true);
            item.up = None;
            item.it = None;
            for slot in [
                &mut item.up_comp1,
                &mut item.up_comp2,
                &mut item.up_comp3,
                &mut item.up_comp4,
                &mut item.up_comp5,
                &mut item.up_comp6,
            ] {
                *slot = None;
            }
            Ok(())
        })
    }

    /// The bidder's quantity for an item the tender left open (`QtyTBD`).
    pub fn free_quantity(&mut self, id: &str, qty: Decimal) -> Result<(), BidError> {
        let qty = match qty.scale() <= 3 {
            true => qty.rescale(3),
            false => return Err(BidError::TooManyDecimals),
        };
        self.with_item(id, |item| {
            if item.qty_tbd != Some(true) {
                return Err(BidError::NotFreeQuantity);
            }
            item.qty = Some(qty);
            item.it = item.up.and_then(|up| item_total(qty, up));
            Ok(())
        })
    }

    /// A markup item's base amount and percentage; its total follows.
    pub fn markup(&mut self, id: &str, base: Decimal, percent: Decimal) -> Result<(), BidError> {
        let (gaeb, resolver) = self.doc.edit();
        let Some(item) = find_markup(gaeb, resolver, id) else {
            return Err(BidError::UnknownItem(id.to_string()));
        };
        item.it_markup = Some(base.rescale(2));
        item.markup = Some(percent);
        item.it = Some(
            base.percent_of(percent)
                .ok_or(BidError::Overflow)?
                .rescale(2),
        );
        Ok(())
    }

    /// Fills a bidder field (`TextComplement` of kind Bidder) by its mark.
    pub fn fill_complement(&mut self, id: &str, mark: i64, text: &str) -> Result<(), BidError> {
        let span = self.doc.intern(text);
        let number = Decimal::parse(text.trim().replace(',', ".").as_str()).ok();
        let trimmed_empty = text.trim().is_empty();
        self.with_item(id, |item| {
            let Some(detail) = item
                .description
                .as_mut()
                .and_then(|d| d.complete_text.as_mut())
                .and_then(|c| c.detail_txt.as_mut())
            else {
                return Err(BidError::NotBidderField);
            };
            let mut fields = Vec::new();
            complements_mut(detail, &mut fields);
            let Some(field) = fields.into_iter().find(|f| f.mark_lbl == Some(mark)) else {
                return Err(BidError::NotBidderField);
            };
            if field.kind != Some(crate::model::AttKind::Bidder) {
                return Err(BidError::NotBidderField);
            }
            fill(field, span, number, trimmed_empty);
            Ok(())
        })
    }

    pub fn discount_percent(&mut self, percent: Decimal) {
        self.discount = Some(percent);
    }

    pub fn vat_percent(&mut self, percent: Decimal) {
        self.vat = Some(percent);
    }

    /// Checks every item is priced or marked not offered, computes the
    /// totals of every category and of the bill (Fachdokumentation
    /// 4.6.17), and hands back the finished X84 document.
    pub fn finish(mut self) -> Result<Document, BidError> {
        let unpriced: Vec<String> = self
            .doc
            .items()
            .filter(|i| !i.not_offered() && i.kind() != ItemKind::NotApplicable)
            .filter(|i| {
                i.unit_price().is_none() || (i.item.qty_tbd == Some(true) && i.quantity().is_none())
            })
            .map(|i| i.oz.dotted())
            .collect();
        if !unpriced.is_empty() {
            return Err(BidError::Unpriced(unpriced));
        }
        let (discount, vat) = (self.discount, self.vat);
        let today = self.doc.intern(&today_iso());
        let (gaeb, _) = self.doc.edit();
        let info = gaeb.gaeb_info.get_or_insert_with(Default::default);
        if info.date.is_none() {
            info.date = Some(today);
        }
        let Some(boq) = gaeb.award.as_mut().and_then(|a| a.boq.as_mut()) else {
            return Ok(self.doc);
        };
        let total = boq
            .boq_body
            .as_mut()
            .map(sum_body)
            .transpose()?
            .unwrap_or(Decimal::new(0, 2));
        let mut totals = Totals {
            total: Some(total),
            ..Totals::default()
        };
        let mut net = total;
        if let Some(pct) = discount {
            let after = total_after_discount(total, pct).ok_or(BidError::Overflow)?;
            totals.discount_pcnt = Some(pct);
            totals.discount_amt = total.checked_sub(after);
            totals.tot_after_disc = Some(after);
            net = after;
        }
        if let Some(pct) = vat {
            let amount = net.percent_of(pct).ok_or(BidError::Overflow)?.rescale(2);
            totals.vat = Some(pct);
            totals.vat_amount = Some(amount);
            totals.total_gross = net.checked_add(amount).map(|g| g.rescale(2));
        }
        let info = boq.boq_info.get_or_insert_with(Default::default);
        info.totals = Some(totals);
        Ok(self.doc)
    }

    /// The bid as X84 bytes, strictly schema-conformant.
    pub fn write(&self) -> Result<Vec<u8>, WriteError> {
        self.doc.write(&self.doc.x84_options())
    }
}

/// Drops what an X84 item must not carry: a description without a long
/// text (the X84 schema allows nothing else in it), also inside
/// sub-descriptions.
fn tidy_for_x84(item: &mut Item) {
    if item
        .description
        .as_ref()
        .is_some_and(|d| d.complete_text.is_none())
    {
        item.description = None;
    }
    for sub in &mut item.sub_descr {
        if sub
            .description
            .as_ref()
            .is_some_and(|d| d.complete_text.is_none())
        {
            sub.description = None;
        }
    }
}

/// Today's date as `YYYY-MM-DD` from the system clock.
fn today_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = match mp < 10 {
        true => mp + 3,
        false => mp - 9,
    };
    let year = match m <= 2 {
        true => y + 1,
        false => y,
    };
    format!("{year:04}-{m:02}-{d:02}")
}

fn price_scale(value: Decimal) -> Result<Decimal, BidError> {
    match value.scale() <= 3 {
        true => Ok(value.rescale(3)),
        false => Err(BidError::TooManyDecimals),
    }
}

fn fill(field: &mut TextComplement, span: Span, number: Option<Decimal>, empty: bool) {
    let body = MLText {
        span: vec![HtmlSpan {
            text: span,
            ..HtmlSpan::default()
        }],
        ..MLText::default()
    };
    field.compl_body = Some(body);
    field.empty = empty.then_some(true);
    if let Some(dec) = field.compl_body_dec.as_mut() {
        dec.value = number;
    }
    if let Some(int) = field.compl_body_int.as_mut() {
        int.value = number
            .and_then(|n| n.rescale(0).mantissa().checked_mul(1))
            .filter(|_| number.is_some_and(|n| n.rescale(0) == n));
    }
}

fn complements_mut<'a>(text: &'a mut BoQText, out: &mut Vec<&'a mut TextComplement>) {
    out.extend(text.text_complement.iter_mut());
    for t in &mut text.text {
        out.extend(t.text_complement.iter_mut());
        for p in &mut t.p {
            p_complements(p, out);
        }
        for d in &mut t.div {
            out.extend(d.text_complement.iter_mut());
        }
        for ul in &mut t.ul {
            for li in &mut ul.li {
                li_complements(li, out);
            }
        }
        for ol in &mut t.ol {
            for li in &mut ol.li {
                li_complements(li, out);
            }
        }
        for table in &mut t.table {
            for tr in &mut table.tr {
                for td in &mut tr.td {
                    td_complements(td, out);
                }
            }
        }
    }
}

fn p_complements<'a>(p: &'a mut HtmlPTC, out: &mut Vec<&'a mut TextComplement>) {
    out.extend(p.text_complement.iter_mut());
}

fn li_complements<'a>(li: &'a mut HtmlLiTC, out: &mut Vec<&'a mut TextComplement>) {
    out.extend(li.text_complement.iter_mut());
    for p in &mut li.p {
        p_complements(p, out);
    }
    for d in &mut li.div {
        out.extend(d.text_complement.iter_mut());
    }
    for ul in &mut li.ul {
        for inner in &mut ul.li {
            li_complements(inner, out);
        }
    }
    for ol in &mut li.ol {
        for inner in &mut ol.li {
            li_complements(inner, out);
        }
    }
}

fn td_complements<'a>(td: &'a mut HtmlTdTC, out: &mut Vec<&'a mut TextComplement>) {
    out.extend(td.text_complement.iter_mut());
    for p in &mut td.p {
        p_complements(p, out);
    }
    for d in &mut td.div {
        out.extend(d.text_complement.iter_mut());
    }
    for ul in &mut td.ul {
        for li in &mut ul.li {
            li_complements(li, out);
        }
    }
    for ol in &mut td.ol {
        for li in &mut ol.li {
            li_complements(li, out);
        }
    }
    for table in &mut td.table {
        for tr in &mut table.tr {
            for inner in &mut tr.td {
                td_complements(inner, out);
            }
        }
    }
}

fn find_item<'a>(gaeb: &'a mut GAEB, resolver: Resolver<'_>, id: &str) -> Option<&'a mut Item> {
    let body = gaeb.award.as_mut()?.boq.as_mut()?.boq_body.as_mut()?;
    find_item_in(body, resolver, id)
}

fn find_item_in<'a>(
    body: &'a mut BoQBody,
    resolver: Resolver<'_>,
    id: &str,
) -> Option<&'a mut Item> {
    if let Some(list) = body.itemlist.as_mut()
        && let Some(index) = list
            .item
            .iter()
            .position(|i| i.id.is_some_and(|s| resolver.str(s) == id))
    {
        return list.item.get_mut(index);
    }
    for ctgy in &mut body.boq_ctgy {
        let Some(inner) = ctgy.boq_body.as_mut() else {
            continue;
        };
        if let Some(found) = find_item_in(inner, resolver, id) {
            return Some(found);
        }
    }
    None
}

fn find_markup<'a>(
    gaeb: &'a mut GAEB,
    resolver: Resolver<'_>,
    id: &str,
) -> Option<&'a mut MarkupItem> {
    let body = gaeb.award.as_mut()?.boq.as_mut()?.boq_body.as_mut()?;
    find_markup_in(body, resolver, id)
}

fn find_markup_in<'a>(
    body: &'a mut BoQBody,
    resolver: Resolver<'_>,
    id: &str,
) -> Option<&'a mut MarkupItem> {
    if let Some(list) = body.itemlist.as_mut()
        && let Some(index) = list
            .markup_item
            .iter()
            .position(|i| i.id.is_some_and(|s| resolver.str(s) == id))
    {
        return list.markup_item.get_mut(index);
    }
    for ctgy in &mut body.boq_ctgy {
        let Some(inner) = ctgy.boq_body.as_mut() else {
            continue;
        };
        if let Some(found) = find_markup_in(inner, resolver, id) {
            return Some(found);
        }
    }
    None
}

/// Sums a body per 4.6.17 and sets `Totals` on every category below it.
/// Items that do not count carry no total at all ("Nur Einh.-Preis"),
/// because an X84 no longer says why they would not.
fn sum_body(body: &mut BoQBody) -> Result<Decimal, BidError> {
    let mut sum = Decimal::new(0, 2);
    for ctgy in &mut body.boq_ctgy {
        let total = sum_category(ctgy)?;
        sum = sum.checked_add(total).ok_or(BidError::Overflow)?;
    }
    if let Some(list) = body.itemlist.as_mut() {
        for item in &mut list.item {
            tidy_for_x84(item);
            if !counts_towards_total(item) {
                item.it = None;
                continue;
            }
            if let (Some(qty), Some(up)) = (item.qty, item.up) {
                item.it = item_total(qty, up);
            }
            if item.qty_tbd != Some(true) {
                item.qty = None;
            }
            let Some(total) = item.it else { continue };
            sum = sum.checked_add(total).ok_or(BidError::Overflow)?;
        }
        for markup in &list.markup_item {
            let Some(total) = markup.it else { continue };
            sum = sum.checked_add(total).ok_or(BidError::Overflow)?;
        }
    }
    Ok(sum.rescale(2))
}

fn sum_category(ctgy: &mut BoQCtgy) -> Result<Decimal, BidError> {
    let total = match ctgy.boq_body.as_mut() {
        Some(body) => sum_body(body)?,
        None => Decimal::new(0, 2),
    };
    let totals = ctgy.totals.get_or_insert_with(Totals::default);
    totals.total = Some(total);
    Ok(total)
}

impl ComplementKind {
    pub fn is_bidder(self) -> bool {
        self == ComplementKind::Bidder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::decimal::Decimal;
    use crate::validate::{Severity, validate, validate_against_tender};

    fn d(text: &str) -> Decimal {
        Decimal::parse(text).unwrap()
    }

    fn bidder() -> Bidder {
        Bidder {
            name1: "Bau GmbH".to_string(),
            name2: None,
            street: "Weg 1".to_string(),
            pcode: "12345".to_string(),
            city: "Berlin".to_string(),
            contact: Some("A. B.".to_string()),
            email: None,
            bidder_no: Some("B-7".to_string()),
        }
    }

    fn tender() -> Document {
        let xml = "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><PrjInfo><NamePrj>P</NamePrj><Cur>EUR</Cur></PrjInfo><Award><DP>83</DP><AwardInfo><Cur>EUR</Cur></AwardInfo><OWN><Address><Name1>Owner</Name1><Street>s</Street><PCode>1</PCode><City>c</City></Address></OWN><BoQ ID=\"b\"><BoQInfo><Name>LV</Name><LblBoQ>T</LblBoQ><OutlCompl>AllTxt</OutlCompl><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody><BoQCtgy ID=\"c1\" RNoPart=\"1\"><LblTx><p><span>Rohbau</span></p></LblTx><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"10\"><Qty>73.789</Qty><QU>m3</QU><Description><CompleteText><DetailTxt><Text><p><span>Beton </span><TextComplement MarkLbl=\"1\" Kind=\"Bidder\" Empty=\"Yes\"><ComplCaption><span>Fabrikat</span></ComplCaption><ComplBody/></TextComplement></p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><p><span>Beton</span></p></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description></Item><Item ID=\"i2\" RNoPart=\"20\"><QtyTBD>Yes</QtyTBD><QU>kg</QU></Item><Item ID=\"i3\" RNoPart=\"30\"><Provis>WithoutTotal</Provis><Qty>5.000</Qty><QU>St</QU></Item><Item ID=\"i4\" RNoPart=\"40\"><Qty>1.000</Qty><QU>psch</QU><LumpSumItem>Yes</LumpSumItem></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></Award></GAEB>";
        Document::parse(xml.as_bytes()).unwrap()
    }

    #[test]
    fn price_sets_it_rounded() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("9.77")).unwrap();
        let item = bid.document().items().find(|i| i.id() == "i1").unwrap();
        assert_eq!(item.unit_price(), Some(d("9.770")));
        assert_eq!(item.total(), Some(d("720.92")));
    }

    #[test]
    fn finish_computes_category_and_boq_totals_and_writes_a_valid_x84() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("9.77")).unwrap();
        bid.free_quantity("i2", d("100.000")).unwrap();
        bid.price("i2", d("0.50")).unwrap();
        bid.price("i3", d("2.00")).unwrap();
        bid.price("i4", d("1000")).unwrap();
        bid.fill_complement("i1", 1, "'Beton C25/30'").unwrap();
        bid.vat_percent(d("19"));
        let doc = bid.finish().unwrap();
        assert_eq!(doc.phase(), Some(crate::version::Phase::X84));
        let totals = doc.computed_totals();
        assert_eq!(doc.declared_total(), Some(d("1770.92")));
        assert_eq!(totals.boq, Some(d("1770.92")));
        assert_eq!(totals.categories[0].declared, Some(d("1770.92")));
        let boq_totals = doc
            .boq()
            .unwrap()
            .boq_info
            .as_ref()
            .unwrap()
            .totals
            .as_ref()
            .unwrap();
        assert_eq!(boq_totals.vat, Some(d("19")));
        assert_eq!(boq_totals.vat_amount, Some(d("336.47")));
        assert_eq!(boq_totals.total_gross, Some(d("2107.39")));
        assert!(
            validate_against_tender(&doc, &tender())
                .iter()
                .all(|f| f.severity != Severity::Error)
        );
        let bytes = doc.write(&doc.x84_options()).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.contains("<DP>84</DP>"));
        assert!(text.contains("<Name1>Bau GmbH</Name1>"));
        assert!(!text.contains("<OWN>"));
        assert!(!text.contains("<QU>"), "{text}");
        assert!(text.contains("<UP>9.770</UP><IT>720.92</IT>"), "{text}");
        let again = Document::parse(&bytes).unwrap();
        assert_eq!(again.declared_total(), Some(d("1770.92")));
        assert!(
            validate(&again)
                .iter()
                .all(|f| f.severity != Severity::Error),
            "{:?}",
            validate(&again)
        );
        let item = again.items().find(|i| i.id() == "i1").unwrap();
        assert_eq!(item.complements()[0].value(), "Beton C25/30");
        assert!(!item.complements()[0].empty);
    }

    #[test]
    fn unpriced_item_is_an_error_listing_oz() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("9.77")).unwrap();
        bid.price("i2", d("1")).unwrap();
        bid.free_quantity("i2", d("1")).unwrap();
        let Err(BidError::Unpriced(ozs)) = bid.finish() else {
            panic!("items i3 and i4 are unpriced")
        };
        assert_eq!(ozs, vec!["01.0030".to_string(), "01.0040".to_string()]);
    }

    #[test]
    fn not_offered_item_has_no_up_and_counts_nothing() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("1")).unwrap();
        bid.not_offered("i2").unwrap();
        bid.not_offered("i3").unwrap();
        bid.not_offered("i4").unwrap();
        let doc = bid.finish().unwrap();
        let item = doc.items().find(|i| i.id() == "i2").unwrap();
        assert!(item.not_offered());
        assert_eq!(item.unit_price(), None);
        assert_eq!(doc.declared_total(), Some(d("73.79")));
    }

    #[test]
    fn bidder_complement_filled_owner_rejected_and_unknown_items_fail() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        assert!(matches!(
            bid.fill_complement("i1", 99, "x"),
            Err(BidError::NotBidderField)
        ));
        assert!(matches!(
            bid.price("nope", d("1")),
            Err(BidError::UnknownItem(_))
        ));
        assert!(matches!(
            bid.free_quantity("i1", d("1")),
            Err(BidError::NotFreeQuantity)
        ));
        bid.price("i1", d("5")).unwrap();
        assert!(matches!(
            bid.price_components("i1", &[d("1"), d("2")]),
            Err(BidError::ComponentsDoNotSum)
        ));
    }

    #[test]
    fn price_components_must_sum_to_the_unit_price_when_declared() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("3.00")).unwrap();
        bid.price_components("i1", &[d("1.00"), d("2.00")]).unwrap();
        let item = bid.document().items().find(|i| i.id() == "i1").unwrap();
        assert_eq!(item.item.up_comp1, Some(d("1.000")));
        assert_eq!(item.item.up_comp2, Some(d("2.000")));
    }

    #[test]
    fn a_bid_needs_a_tender() {
        let x84 = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84"
        ))
        .unwrap();
        let doc = Document::parse(&x84).unwrap();
        assert!(matches!(
            Bid::from_tender(&doc, bidder()),
            Err(BidError::NotATender(Some(crate::version::Phase::X84)))
        ));
    }

    #[test]
    fn discount_applies_to_the_boq_total() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("9.77")).unwrap();
        bid.not_offered("i2").unwrap();
        bid.not_offered("i3").unwrap();
        bid.not_offered("i4").unwrap();
        bid.discount_percent(d("1.5"));
        let doc = bid.finish().unwrap();
        let totals = doc
            .boq()
            .unwrap()
            .boq_info
            .as_ref()
            .unwrap()
            .totals
            .as_ref()
            .unwrap();
        assert_eq!(totals.total, Some(d("720.92")));
        assert_eq!(totals.discount_pcnt, Some(d("1.5")));
        assert_eq!(totals.tot_after_disc, Some(d("710.11")));
    }

    #[test]
    fn fixed_quantities_are_not_written_and_free_items_need_one() {
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("1")).unwrap();
        bid.price("i2", d("1")).unwrap();
        bid.not_offered("i3").unwrap();
        bid.price("i4", d("1")).unwrap();
        let Err(BidError::Unpriced(ozs)) = bid.finish() else {
            panic!("a free quantity item without a quantity is not priced")
        };
        assert_eq!(ozs, vec!["01.0020".to_string()]);
        let mut bid = Bid::from_tender(&tender(), bidder()).unwrap();
        bid.price("i1", d("1")).unwrap();
        bid.free_quantity("i2", d("2")).unwrap();
        bid.price("i2", d("1")).unwrap();
        bid.not_offered("i3").unwrap();
        bid.price("i4", d("1")).unwrap();
        let doc = bid.finish().unwrap();
        let text = String::from_utf8(doc.write(&doc.x84_options()).unwrap()).unwrap();
        assert!(
            !text.contains("<Item ID=\"i1\" RNoPart=\"10\"><Qty>"),
            "{text}"
        );
        assert!(
            text.contains("<Item ID=\"i2\" RNoPart=\"20\"><Qty>2.000</Qty>"),
            "{text}"
        );
    }

    #[test]
    fn finish_supplies_a_date_and_drops_descriptions_without_text() {
        let xml = "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate></GAEBInfo><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><Name>LV</Name><LblBoQ>T</LblBoQ><OutlCompl>OutTxt</OutlCompl><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"10\"><Qty>1.000</Qty><QU>m</QU><Description><OutlineText><OutlTxt><TextOutlTxt><p><span>k</span></p></TextOutlTxt></OutlTxt></OutlineText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
        let tender = Document::parse(xml.as_bytes()).unwrap();
        let mut bid = Bid::from_tender(&tender, bidder()).unwrap();
        bid.price("i1", d("2")).unwrap();
        let doc = bid.finish().unwrap();
        let date = doc.gaeb.gaeb_info.as_ref().unwrap().date.unwrap();
        assert_eq!(doc.str(date).len(), 10);
        let text = String::from_utf8(doc.write(&doc.x84_options()).unwrap()).unwrap();
        assert!(!text.contains("<Description"), "{text}");
    }
}
