//! Each file under tests/fixtures/failure-modes reads without an error and
//! raises exactly the warning or finding its README names.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::validate::{FindingCode, validate};
use gaebboss_core::warning::WarningCode;
use gaebboss_core::write::WriteOptions;

fn read(name: &str) -> Document {
    let path = format!(
        "{}/../../tests/fixtures/failure-modes/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::open(std::path::Path::new(&path)).unwrap()
}

fn has(doc: &Document, code: WarningCode) -> bool {
    doc.warnings.iter().any(|w| w.code == code)
}

fn finds(doc: &Document, code: FindingCode) -> bool {
    validate(doc).iter().any(|f| f.code == code)
}

#[test]
fn unclosed_tag() {
    let doc = read("unclosed-tag.x83");
    assert!(has(&doc, WarningCode::UnclosedElement));
    assert_eq!(doc.items().count(), 1);
    assert_eq!(doc.items().next().unwrap().long_text(), "unclosed");
}

#[test]
fn stray_end_tag() {
    let doc = read("stray-end-tag.x83");
    assert!(has(&doc, WarningCode::MismatchedEndTag));
    let item = doc.items().next().unwrap();
    assert_eq!(item.unit(), Some("m"));
}

#[test]
fn bad_decimal() {
    let doc = read("bad-decimal.x83");
    assert!(has(&doc, WarningCode::BadDecimal));
    assert_eq!(doc.items().next().unwrap().quantity(), None);
}

#[test]
fn bad_enum() {
    let doc = read("bad-enum.x83");
    assert_eq!(
        doc.warnings
            .iter()
            .filter(|w| w.code == WarningCode::BadEnum)
            .count(),
        2
    );
    let item = doc.items().next().unwrap();
    assert_eq!(item.item.provis, None);
    assert_eq!(item.item.lump_sum_item, None);
}

#[test]
fn duplicate_element() {
    let doc = read("duplicate-element.x83");
    assert!(has(&doc, WarningCode::Cardinality));
    assert_eq!(
        doc.items()
            .next()
            .unwrap()
            .quantity()
            .map(|q| q.to_string()),
        Some("1.000".to_string())
    );
}

#[test]
fn duplicate_id() {
    let doc = read("duplicate-id.x83");
    assert!(finds(&doc, FindingCode::DuplicateId));
}

#[test]
fn vendor_elements_survive() {
    let doc = read("vendor-elements.x83");
    assert_eq!(
        doc.warnings
            .iter()
            .filter(|w| w.code == WarningCode::UnknownElement)
            .count(),
        2
    );
    let item = doc.items().next().unwrap();
    assert_eq!(item.item.extra.len(), 2);
    assert_eq!(item.unit(), Some("m"));
    let opts = WriteOptions {
        version: doc.version(),
        phase: doc.phase().unwrap(),
        indent: None,
        prog_system: None,
        strict: false,
    };
    let out = String::from_utf8(doc.write(&opts).unwrap()).unwrap();
    assert!(out.contains("<Codetext>vendor text</Codetext>"));
    assert!(out.contains("<OZ>01.0010</OZ>"));
}

#[test]
fn latin1_bytes() {
    let doc = read("latin1-bytes.x83");
    assert!(has(&doc, WarningCode::EncodingRepaired));
    assert_eq!(doc.items().next().unwrap().long_text(), "Größe");
}

#[test]
fn missing_breakdown() {
    let doc = read("missing-breakdown.x83");
    assert_eq!(doc.items().next().unwrap().oz.dotted(), "01.0010");
    assert!(finds(&doc, FindingCode::MissingRequired));
}

#[test]
fn oz_overflow() {
    let doc = read("oz-overflow.x83");
    assert!(finds(&doc, FindingCode::OzTooLong));
}

#[test]
fn deeply_nested_documents_fail_cleanly() {
    let mut xml = String::from(
        "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQBody>",
    );
    for _ in 0..5000 {
        xml.push_str("<BoQCtgy ID=\"c\" RNoPart=\"1\"><BoQBody>");
    }
    assert!(Document::parse(xml.as_bytes()).is_err());
    let mut text = String::from(
        "<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQBody><Itemlist><Item ID=\"i\" RNoPart=\"1\"><Description><CompleteText><DetailTxt><Text>",
    );
    for _ in 0..100 {
        text.push_str("<ul><li>");
    }
    text.push_str("<span>deep</span>");
    for _ in 0..100 {
        text.push_str("</li></ul>");
    }
    text.push_str("</Text></DetailTxt></CompleteText></Description></Item></Itemlist></BoQBody></BoQ></Award></GAEB>");
    let doc = Document::parse(text.as_bytes()).unwrap();
    assert!(doc.items().next().unwrap().long_text().contains("deep"));
}
