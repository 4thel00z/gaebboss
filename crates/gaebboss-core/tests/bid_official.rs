//! Pricing the official 3.1 Musterdatei tender with the unit prices of its
//! official bid reproduces that bid's totals.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use gaebboss_core::Document;
use gaebboss_core::bid::{Bid, Bidder};
use gaebboss_core::decimal::Decimal;
use gaebboss_core::validate::{Severity, validate_against_tender, validate_with_tender};

fn fixture(rel: &str) -> Document {
    let path = format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::open(std::path::Path::new(&path)).unwrap()
}

/// The tender priced with the unit prices, free quantities and markups of
/// the official bid.
fn priced_like(tender: &Document, official: &Document) -> Document {
    let prices: HashMap<String, (Option<Decimal>, Option<Decimal>)> = official
        .items()
        .map(|i| (i.oz.compact(), (i.unit_price(), i.quantity())))
        .collect();
    let bidder = Bidder {
        name1: "BVBS".to_string(),
        name2: None,
        street: "k.A.".to_string(),
        pcode: "k.A.".to_string(),
        city: "k.A.".to_string(),
        contact: None,
        email: None,
        bidder_no: None,
    };
    let mut bid = Bid::from_tender(tender, bidder).unwrap();
    let ids: Vec<(String, String)> = tender
        .items()
        .map(|i| (i.id().to_string(), i.oz.compact()))
        .collect();
    for (id, oz) in &ids {
        let Some((up, qty)) = prices.get(oz) else {
            continue;
        };
        if let Some(qty) = qty
            && bid.free_quantity(id, *qty).is_ok()
        {}
        match up {
            Some(up) => bid.price(id, *up).unwrap(),
            None => bid.not_offered(id).unwrap(),
        }
    }
    let markups: HashMap<String, (Option<Decimal>, Option<Decimal>)> = official
        .markup_items()
        .map(|m| (m.oz.compact(), (m.base(), m.percent())))
        .collect();
    let markup_ids: Vec<(String, String)> = tender
        .markup_items()
        .map(|m| (m.id().to_string(), m.oz.compact()))
        .collect();
    for (id, oz) in &markup_ids {
        if let Some((Some(base), Some(pct))) = markups.get(oz) {
            bid.markup(id, *base, *pct).unwrap();
        }
    }
    bid.finish().unwrap()
}

#[test]
fn bvbs_x83_priced_like_its_x84_reproduces_the_official_totals() {
    let tender = fixture("3.3/bvbs_bauausfuehrung.x83");
    let official = fixture("3.3/bvbs_bauausfuehrung.x84");
    let doc = priced_like(&tender, &official);
    let computed = doc.computed_totals();
    let official_totals = official.computed_totals();
    for (ours, theirs) in computed.categories.iter().zip(&official_totals.categories) {
        assert_eq!(ours.oz, theirs.oz);
        assert_eq!(ours.declared, theirs.declared, "category {}", ours.oz);
    }
    assert_eq!(doc.declared_total(), official.declared_total());
    let findings = validate_against_tender(&doc, &tender);
    assert!(
        findings.iter().all(|f| f.severity != Severity::Error),
        "{findings:?}"
    );
    let bytes = doc.write(&doc.x84_options()).unwrap();
    assert!(Document::parse(&bytes).unwrap().phase() == Some(gaebboss_core::version::Phase::X84));
}

#[test]
fn musterdatei_x83_priced_like_its_x84_leaves_the_alternative_category_out() {
    let tender = fixture("3.1/Musterdatei_3.1_2008-001.X83");
    let official = fixture("3.1/Musterdatei_3.1_2008-001.X84");
    let priced = priced_like(&tender, &official);
    let doc = Document::parse(&priced.write(&priced.x84_options()).unwrap()).unwrap();
    let categories = doc.computed_totals().categories;
    let declared = |oz: &str| {
        categories
            .iter()
            .find(|c| c.oz == oz)
            .and_then(|c| c.declared)
            .unwrap()
    };
    let counted = categories
        .iter()
        .filter(|c| c.depth == 2 && c.oz.starts_with("1.10.") && c.oz != "1.10.40")
        .filter_map(|c| c.declared)
        .fold(Decimal::new(0, 2), |sum, total| {
            sum.checked_add(total).unwrap()
        });
    assert_eq!(declared("1.10.40"), Decimal::parse("1212.00").unwrap());
    assert_eq!(declared("1.10"), counted);
    let errors: Vec<String> = validate_with_tender(&doc, &tender)
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}
