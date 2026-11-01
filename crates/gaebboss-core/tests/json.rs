//! The JSON projection of a document.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::json::to_json;

#[test]
fn json_shape_and_string_decimals() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let value = to_json(&doc);
    assert_eq!(value["version"], "3.3");
    assert_eq!(value["phase"], "X84");
    assert_eq!(value["project"]["name"], "BVBS GAEB Muster");
    assert_eq!(value["boq"]["name"], "BVBS GAEB Bauausf.");
    assert_eq!(value["boq"]["breakdown"].as_array().unwrap().len(), 4);
    assert_eq!(value["boq"]["breakdown"][2]["type"], "Item");
    assert_eq!(value["boq"]["breakdown"][2]["length"], 4);
    assert_eq!(value["boq"]["totals"]["total"], "2000000.00");
    assert_eq!(value["boq"]["totals"]["vat"], "19");
    let items = value["items"].as_array().unwrap();
    assert_eq!(items.len(), 27);
    assert_eq!(items[0]["id"], "ID02BED3D8");
    assert_eq!(items[0]["oz"], "001.001.0010");
    assert_eq!(items[0]["kind"], "Standard");
    assert_eq!(items[0]["unit_price"], "5000.000");
    assert_eq!(items[0]["total"], "5000.00");
    assert!(items[0]["quantity"].is_null());
    assert_eq!(items[0]["counts_towards_total"], true);
    let with_complements = items
        .iter()
        .find(|i| !i["complements"].as_array().unwrap().is_empty())
        .unwrap();
    assert_eq!(with_complements["complements"][0]["mark"], 60);
    assert_eq!(with_complements["complements"][0]["kind"], "Bidder");
    assert_eq!(with_complements["complements"][0]["value"], "55");
    let categories = value["categories"].as_array().unwrap();
    assert_eq!(categories.len(), 12);
    assert_eq!(categories[0]["oz"], "001");
    assert_eq!(categories[0]["depth"], 0);
    assert_eq!(categories[0]["totals"]["total"], "1054734.03");
    assert_eq!(value["markup_items"][0]["total"], "85000.00");
    assert_eq!(
        value["warnings"].as_array().unwrap().len(),
        doc.warnings.len()
    );
}
