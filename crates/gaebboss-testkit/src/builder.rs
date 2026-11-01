//! Builds small, schema-shaped GAEB DA XML 3.3 documents for tests, so a
//! test states the fact it needs instead of carrying a page of XML.

use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub struct ItemSpec {
    pub id: String,
    pub rno: String,
    pub index: Option<String>,
    pub qty: Option<String>,
    pub qty_tbd: bool,
    pub unit: Option<String>,
    pub short: String,
    pub long: String,
    pub provis: Option<&'static str>,
    pub lump_sum: bool,
    pub aln: Option<(i64, i64)>,
    pub up: Option<String>,
    pub it: Option<String>,
    pub complements: Vec<(i64, &'static str, Option<String>)>,
}

impl ItemSpec {
    pub fn new(rno: &str, unit: &str, qty: &str, short: &str, long: &str) -> ItemSpec {
        ItemSpec {
            id: format!("i{rno}"),
            rno: rno.to_string(),
            index: None,
            qty: Some(qty.to_string()),
            qty_tbd: false,
            unit: Some(unit.to_string()),
            short: short.to_string(),
            long: long.to_string(),
            provis: None,
            lump_sum: false,
            aln: None,
            up: None,
            it: None,
            complements: Vec::new(),
        }
    }

    pub fn free_quantity(mut self) -> ItemSpec {
        self.qty = None;
        self.qty_tbd = true;
        self
    }

    pub fn provisional(mut self, with_total: bool) -> ItemSpec {
        self.provis = Some(match with_total {
            true => "WithTotal",
            false => "WithoutTotal",
        });
        self
    }

    pub fn lump_sum(mut self) -> ItemSpec {
        self.lump_sum = true;
        self
    }

    pub fn alternative(mut self, group: i64, serial: i64) -> ItemSpec {
        self.aln = Some((group, serial));
        self
    }

    pub fn priced(mut self, up: &str, it: &str) -> ItemSpec {
        self.up = Some(up.to_string());
        self.it = Some(it.to_string());
        self
    }

    /// A text complement in the long text; `None` body is an empty bidder field.
    pub fn complement(mut self, mark: i64, kind: &'static str, body: Option<&str>) -> ItemSpec {
        self.complements
            .push((mark, kind, body.map(str::to_string)));
        self
    }
}

#[derive(Clone, Debug)]
pub struct CategorySpec {
    pub id: String,
    pub rno: String,
    pub label: String,
    pub items: Vec<ItemSpec>,
    pub children: Vec<CategorySpec>,
    pub total: Option<String>,
}

impl CategorySpec {
    pub fn new(rno: &str, label: &str) -> CategorySpec {
        CategorySpec {
            id: format!("c{rno}"),
            rno: rno.to_string(),
            label: label.to_string(),
            items: Vec::new(),
            children: Vec::new(),
            total: None,
        }
    }

    pub fn item(mut self, item: ItemSpec) -> CategorySpec {
        self.items.push(item);
        self
    }

    pub fn child(mut self, child: CategorySpec) -> CategorySpec {
        self.children.push(child);
        self
    }

    pub fn total(mut self, total: &str) -> CategorySpec {
        self.total = Some(total.to_string());
        self
    }
}

/// A GAEB DA XML 3.3 document in the procurement family.
#[derive(Clone, Debug)]
pub struct GaebBuilder {
    phase: String,
    project: String,
    boq_name: String,
    levels: Vec<(&'static str, usize)>,
    categories: Vec<CategorySpec>,
    items: Vec<ItemSpec>,
    bidder: Option<String>,
    total: Option<String>,
}

impl GaebBuilder {
    /// `phase` is the DP code: `83`, `84`, `86`.
    pub fn new(phase: &str) -> GaebBuilder {
        GaebBuilder {
            phase: phase.to_string(),
            project: "Projekt".to_string(),
            boq_name: "LV".to_string(),
            levels: vec![("BoQLevel", 2), ("Item", 4)],
            categories: Vec::new(),
            items: Vec::new(),
            bidder: None,
            total: None,
        }
    }

    pub fn x83() -> GaebBuilder {
        GaebBuilder::new("83")
    }

    pub fn x84() -> GaebBuilder {
        let mut b = GaebBuilder::new("84");
        b.bidder = Some("Bieter GmbH".to_string());
        b
    }

    pub fn project(mut self, name: &str) -> GaebBuilder {
        self.project = name.to_string();
        self
    }

    pub fn boq_name(mut self, name: &str) -> GaebBuilder {
        self.boq_name = name.to_string();
        self
    }

    /// Breakdown levels as `(type, length)`: `("Lot", 1)`, `("BoQLevel", 2)`,
    /// `("Item", 4)`, `("Index", 1)`.
    pub fn levels(mut self, levels: &[(&'static str, usize)]) -> GaebBuilder {
        self.levels = levels.to_vec();
        self
    }

    pub fn category(mut self, category: CategorySpec) -> GaebBuilder {
        self.categories.push(category);
        self
    }

    /// Items directly in the bill, without categories.
    pub fn item(mut self, item: ItemSpec) -> GaebBuilder {
        self.items.push(item);
        self
    }

    pub fn total(mut self, total: &str) -> GaebBuilder {
        self.total = Some(total.to_string());
        self
    }

    pub fn build(&self) -> Vec<u8> {
        let mut out = String::new();
        let _ = write!(
            out,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3\">",
            self.phase
        );
        let _ = write!(
            out,
            "<GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date><ProgSystem>gaebboss-testkit</ProgSystem></GAEBInfo>"
        );
        let _ = write!(
            out,
            "<PrjInfo><NamePrj>{}</NamePrj><Cur>EUR</Cur></PrjInfo>",
            escape(&self.project)
        );
        let _ = write!(
            out,
            "<Award><DP>{}</DP><AwardInfo><Cur>EUR</Cur></AwardInfo>",
            self.phase
        );
        if self.phase != "84" {
            out.push_str("<OWN><Address><Name1>Auftraggeber</Name1><Street>Weg 1</Street><PCode>10115</PCode><City>Berlin</City></Address></OWN>");
        }
        if let Some(bidder) = &self.bidder {
            let _ = write!(
                out,
                "<CTR><Address><Name1>{}</Name1><Street>Weg 2</Street><PCode>10115</PCode><City>Berlin</City></Address></CTR>",
                escape(bidder)
            );
        }
        let _ = write!(
            out,
            "<BoQ ID=\"boq\"><BoQInfo><Name>{}</Name>",
            escape(&self.boq_name)
        );
        if self.phase != "84" {
            out.push_str("<LblBoQ>Leistungsverzeichnis</LblBoQ><OutlCompl>AllTxt</OutlCompl>");
        }
        for (kind, length) in &self.levels {
            let num = match *kind {
                "Index" => "No",
                _ => "Yes",
            };
            let _ = write!(
                out,
                "<BoQBkdn><Type>{kind}</Type><Length>{length}</Length><Num>{num}</Num></BoQBkdn>"
            );
        }
        if let Some(total) = &self.total {
            let _ = write!(out, "<Totals><Total>{total}</Total></Totals>");
        }
        out.push_str("</BoQInfo><BoQBody>");
        for c in &self.categories {
            self.category_xml(&mut out, c);
        }
        if !self.items.is_empty() {
            out.push_str("<Itemlist>");
            for i in &self.items {
                self.item_xml(&mut out, i);
            }
            out.push_str("</Itemlist>");
        }
        out.push_str("</BoQBody></BoQ></Award></GAEB>");
        out.into_bytes()
    }

    fn category_xml(&self, out: &mut String, c: &CategorySpec) {
        let _ = write!(
            out,
            "<BoQCtgy ID=\"{}\" RNoPart=\"{}\">",
            escape(&c.id),
            escape(&c.rno)
        );
        if self.phase != "84" {
            let _ = write!(
                out,
                "<LblTx><p><span>{}</span></p></LblTx>",
                escape(&c.label)
            );
        }
        out.push_str("<BoQBody>");
        for child in &c.children {
            self.category_xml(out, child);
        }
        if !c.items.is_empty() {
            out.push_str("<Itemlist>");
            for i in &c.items {
                self.item_xml(out, i);
            }
            out.push_str("</Itemlist>");
        }
        out.push_str("</BoQBody>");
        if let Some(total) = &c.total {
            let _ = write!(out, "<Totals><Total>{total}</Total></Totals>");
        }
        out.push_str("</BoQCtgy>");
    }

    fn item_xml(&self, out: &mut String, i: &ItemSpec) {
        let _ = write!(
            out,
            "<Item ID=\"{}\" RNoPart=\"{}\"",
            escape(&i.id),
            escape(&i.rno)
        );
        if let Some(index) = &i.index {
            let _ = write!(out, " RNoIndex=\"{}\"", escape(index));
        }
        out.push('>');
        let bid = self.phase == "84";
        if !bid {
            if let Some((group, serial)) = i.aln {
                let _ = write!(
                    out,
                    "<ALNGroupNo>{group}</ALNGroupNo><ALNSerNo>{serial}</ALNSerNo>"
                );
            }
            if let Some(provis) = i.provis {
                let _ = write!(out, "<Provis>{provis}</Provis>");
            }
            if i.lump_sum {
                out.push_str("<LumpSumItem>Yes</LumpSumItem>");
            }
            if i.qty_tbd {
                out.push_str("<QtyTBD>Yes</QtyTBD>");
            }
        }
        if let Some(qty) = &i.qty {
            let _ = write!(out, "<Qty>{qty}</Qty>");
        }
        if !bid {
            if let Some(unit) = &i.unit {
                let _ = write!(out, "<QU>{}</QU>", escape(unit));
            }
        }
        if let Some(up) = &i.up {
            let _ = write!(out, "<UP>{up}</UP>");
        }
        if let Some(it) = &i.it {
            let _ = write!(out, "<IT>{it}</IT>");
        }
        if !bid {
            out.push_str("<Description><CompleteText><DetailTxt><Text><p>");
            let _ = write!(out, "<span>{}</span>", escape(&i.long));
            for (mark, kind, body) in &i.complements {
                match body {
                    Some(text) => {
                        let _ = write!(
                            out,
                            "<TextComplement MarkLbl=\"{mark}\" Kind=\"{kind}\"><ComplBody><span>{}</span></ComplBody></TextComplement>",
                            escape(text)
                        );
                    }
                    None => {
                        let _ = write!(
                            out,
                            "<TextComplement MarkLbl=\"{mark}\" Kind=\"{kind}\" Empty=\"Yes\"><ComplBody/></TextComplement>"
                        );
                    }
                }
            }
            let _ = write!(
                out,
                "</p></Text></DetailTxt><OutlineText><OutlTxt><TextOutlTxt><p><span>{}</span></p></TextOutlTxt></OutlTxt></OutlineText></CompleteText></Description>",
                escape(&i.short)
            );
        }
        out.push_str("</Item>");
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tender_with_nested_categories() {
        let xml = GaebBuilder::x83()
            .category(
                CategorySpec::new("1", "Rohbau").child(
                    CategorySpec::new("1", "Erdarbeiten").item(
                        ItemSpec::new("10", "m3", "20.000", "Aushub", "Boden lösen")
                            .complement(1, "Bidder", None),
                    ),
                ),
            )
            .build();
        let text = String::from_utf8(xml).unwrap();
        assert!(text.contains("<BoQCtgy ID=\"c1\" RNoPart=\"1\"><LblTx><p><span>Rohbau</span></p></LblTx><BoQBody><BoQCtgy"));
        assert!(text.contains("<Item ID=\"i10\" RNoPart=\"10\"><Qty>20.000</Qty><QU>m3</QU>"));
        assert!(text.contains("<TextComplement MarkLbl=\"1\" Kind=\"Bidder\" Empty=\"Yes\">"));
    }

    #[test]
    fn a_bid_carries_prices_and_no_tender_only_elements() {
        let xml = GaebBuilder::x84()
            .total("20.00")
            .category(
                CategorySpec::new("1", "x")
                    .total("20.00")
                    .item(ItemSpec::new("10", "m", "2.000", "s", "l").priced("10.000", "20.00")),
            )
            .build();
        let text = String::from_utf8(xml).unwrap();
        assert!(text.contains("<CTR>"));
        assert!(!text.contains("<OWN>"));
        assert!(!text.contains("<QU>"));
        assert!(text.contains("<UP>10.000</UP><IT>20.00</IT>"));
    }
}
