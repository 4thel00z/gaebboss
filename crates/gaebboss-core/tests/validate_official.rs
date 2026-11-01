//! Validation of the official sample files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::validate::{Severity, validate, validate_against_tender};

fn fixture(rel: &str) -> Document {
    let path = format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::open(std::path::Path::new(&path)).unwrap()
}

fn errors(doc: &Document) -> Vec<String> {
    validate(doc)
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{:?} {:?}: {}", f.code, f.oz, f.message))
        .collect()
}

#[test]
fn bvbs_x84_has_no_errors() {
    let doc = fixture("3.3/bvbs_bauausfuehrung.x84");
    assert!(errors(&doc).is_empty(), "{:#?}", errors(&doc));
}

#[test]
fn bvbs_x83_has_no_errors() {
    let doc = fixture("3.3/bvbs_bauausfuehrung.x83");
    assert!(errors(&doc).is_empty(), "{:#?}", errors(&doc));
}

#[test]
fn musterdatei_31_x83_has_no_errors() {
    let doc = fixture("3.1/Musterdatei_3.1_2008-001.X83");
    assert!(errors(&doc).is_empty(), "{:#?}", errors(&doc));
}

#[test]
fn bvbs_x84_matches_its_x83() {
    let tender = fixture("3.3/bvbs_bauausfuehrung.x83");
    let bid = fixture("3.3/bvbs_bauausfuehrung.x84");
    let findings = validate_against_tender(&bid, &tender);
    let errors: Vec<String> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn musterdatei_x84_matches_its_x83() {
    let tender = fixture("3.1/Musterdatei_3.1_2008-001.X83");
    let bid = fixture("3.1/Musterdatei_3.1_2008-001.X84");
    let findings = validate_against_tender(&bid, &tender);
    let errors: Vec<String> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}
