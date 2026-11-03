//! Validation of the official sample files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::validate::{
    Finding, Severity, validate, validate_against_tender, validate_with_tender,
};

fn fixture(rel: &str) -> Document {
    let path = format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::open(std::path::Path::new(&path)).unwrap()
}

fn error_lines(findings: Vec<Finding>) -> Vec<String> {
    findings
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{:?} {:?}: {}", f.code, f.oz, f.message))
        .collect()
}

fn errors(doc: &Document) -> Vec<String> {
    error_lines(validate(doc))
}

#[test]
fn official_x84s_validated_with_their_x83s_have_no_errors() {
    let found: Vec<(&str, Vec<String>)> = [
        (
            "3.1/Musterdatei_3.1_2008-001.X84",
            "3.1/Musterdatei_3.1_2008-001.X83",
        ),
        ("3.3/bvbs_bauausfuehrung.x84", "3.3/bvbs_bauausfuehrung.x83"),
    ]
    .into_iter()
    .map(|(bid, tender)| {
        let findings = validate_with_tender(&fixture(bid), &fixture(tender));
        (bid, error_lines(findings))
    })
    .filter(|(_, errors)| !errors.is_empty())
    .collect();
    assert!(found.is_empty(), "{found:#?}");
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
fn musterdatei_31_x86_totals_add_up() {
    let found: Vec<(&str, Vec<String>)> = [
        "3.1/Musterdatei_3.1_2008-001.X86",
        "3.1/Musterdatei_3.1_2008-002.X86",
        "3.1/Musterdatei_3.1_2008-003.X86",
        "3.1/Musterdatei_3.1_2008-004.X86",
    ]
    .into_iter()
    .map(|rel| (rel, errors(&fixture(rel))))
    .filter(|(_, errors)| !errors.is_empty())
    .collect();
    assert!(found.is_empty(), "{found:#?}");
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
