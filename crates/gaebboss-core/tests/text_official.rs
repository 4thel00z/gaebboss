//! Text projections on the official sample files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::text::{ComplementKind, complements, plain};

#[test]
fn bvbs_x84_complements_are_bidder_kind_with_marks_60_61() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    let second = &boq.boq_body.as_ref().unwrap().boq_ctgy[1];
    let third = &second.boq_body.as_ref().unwrap().boq_ctgy[2];
    let items = &third
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item;
    let detail = items[1]
        .description
        .as_ref()
        .unwrap()
        .complete_text
        .as_ref()
        .unwrap()
        .detail_txt
        .as_ref()
        .unwrap();
    let found = complements(&doc, detail.node);
    assert_eq!(
        found.iter().map(|c| c.mark).collect::<Vec<_>>(),
        vec![Some(60), Some(61)]
    );
    assert!(found.iter().all(|c| c.kind == ComplementKind::Bidder));
    assert_eq!(found[0].value(), "55");
    assert_eq!(plain(&doc, detail.node), "55\n55");
}

#[test]
fn bvbs_x83_long_texts_render() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x83",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    let mut rendered = 0;
    let mut stack: Vec<&gaebboss_core::model::BoQCtgy> =
        boq.boq_body.as_ref().unwrap().boq_ctgy.iter().collect();
    while let Some(ctgy) = stack.pop() {
        let Some(body) = ctgy.boq_body.as_ref() else {
            continue;
        };
        stack.extend(body.boq_ctgy.iter());
        let Some(list) = body.itemlist.as_ref() else {
            continue;
        };
        for item in &list.item {
            let Some(detail) = item
                .description
                .as_ref()
                .and_then(|d| d.complete_text.as_ref())
                .and_then(|c| c.detail_txt.as_ref())
            else {
                continue;
            };
            let text = plain(&doc, detail.node);
            if text.trim().is_empty() {
                continue;
            }
            rendered += 1;
        }
    }
    assert!(rendered > 10, "{rendered}");
}
