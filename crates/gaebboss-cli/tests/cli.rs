//! The gaebboss command line on the official fixtures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture(rel: &str) -> String {
    format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn gaebboss() -> Command {
    Command::cargo_bin("gaebboss").unwrap()
}

#[test]
fn info_prints_version_phase_and_counts() {
    gaebboss()
        .args(["info", &fixture("3.3/bvbs_bauausfuehrung.x84")])
        .assert()
        .success()
        .stdout(predicate::str::contains("version: 3.3"))
        .stdout(predicate::str::contains("phase: X84"))
        .stdout(predicate::str::contains("items: 27"))
        .stdout(predicate::str::contains("categories: 12"))
        .stdout(predicate::str::contains("total: 2000000.00"));
}

#[test]
fn items_tsv_has_one_row_per_item_with_a_header() {
    let output = gaebboss()
        .args(["items", &fixture("3.3/bvbs_bauausfuehrung.x84"), "--tsv"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 28, "{text}");
    assert!(lines[0].starts_with("oz\tid\tkind\tquantity\tunit\tunit_price\ttotal\tshort_text"));
    assert!(
        lines[1].starts_with("001.001.0010\tID02BED3D8\tStandard\t\t\t5000.000\t5000.00"),
        "{}",
        lines[1]
    );
}

#[test]
fn json_decimals_are_strings_and_items_are_listed() {
    let output = gaebboss()
        .args(["json", &fixture("3.3/bvbs_bauausfuehrung.x84")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["version"], "3.3");
    assert_eq!(value["phase"], "X84");
    assert_eq!(value["boq"]["totals"]["total"], "2000000.00");
    assert_eq!(value["items"].as_array().unwrap().len(), 27);
    assert_eq!(value["items"][0]["unit_price"], "5000.000");
    assert!(value["items"][0]["unit_price"].is_string());
    assert_eq!(value["items"][0]["oz"], "001.001.0010");
    assert_eq!(value["categories"][0]["oz"], "001");
}

#[test]
fn text_prints_the_long_text_of_one_item() {
    gaebboss()
        .args([
            "text",
            &fixture("3.3/bvbs_bauausfuehrung.x83"),
            "--oz",
            "001.001.0010",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Baustelleneinrichtung"));
    gaebboss()
        .args([
            "text",
            &fixture("3.3/bvbs_bauausfuehrung.x83"),
            "--oz",
            "nope",
        ])
        .assert()
        .failure();
}

#[test]
fn validate_exit_code_zero_on_clean_and_one_on_errors() {
    gaebboss()
        .args(["validate", &fixture("3.3/bvbs_bauausfuehrung.x84")])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 errors"));
    gaebboss()
        .args([
            "validate",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "--tender",
            &fixture("3.3/bvbs_bauausfuehrung.x83"),
        ])
        .assert()
        .success();
    let dir = tempfile::tempdir().unwrap();
    let broken = dir.path().join("broken.X84");
    let text = std::fs::read_to_string(fixture("3.3/bvbs_bauausfuehrung.x84"))
        .unwrap()
        .replacen("<Total>2000000.00</Total>", "<Total>1.00</Total>", 1);
    std::fs::write(&broken, text).unwrap();
    gaebboss()
        .args(["validate", broken.to_str().unwrap()])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("BoqTotalMismatch"));
}

#[test]
fn roundtrip_writes_a_file_that_parses_to_the_same_items() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.X84");
    gaebboss()
        .args([
            "roundtrip",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "-o",
            out.to_str().unwrap(),
            "--pretty",
        ])
        .assert()
        .success();
    let written = std::fs::read_to_string(&out).unwrap();
    assert!(written.contains("\n  <GAEBInfo>"));
    gaebboss()
        .args(["info", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("items: 27"));
}

#[test]
fn bid_from_official_x83_writes_x84() {
    let dir = tempfile::tempdir().unwrap();
    let prices = dir.path().join("prices.csv");
    let bidder = dir.path().join("bidder.toml");
    let out = dir.path().join("bid.X84");
    let tender = fixture("3.3/bvbs_bauausfuehrung.x83");
    let listing = gaebboss()
        .args(["items", &tender, "--tsv"])
        .output()
        .unwrap();
    let rows: Vec<String> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .skip(1)
        .map(|l| l.split('\t').next().unwrap().to_string())
        .collect();
    let mut csv = String::from("oz,up\n");
    for (i, oz) in rows.iter().enumerate() {
        match i % 7 == 3 {
            true => csv.push_str(&format!("{oz},-\n")),
            false => csv.push_str(&format!("{oz},{}.50\n", i + 1)),
        }
    }
    std::fs::write(&prices, csv).unwrap();
    std::fs::write(&bidder, "name1 = \"Bau GmbH\"\nstreet = \"Weg 1\"\npcode = \"12345\"\ncity = \"Berlin\"\nbidder_no = \"B-7\"\nvat_percent = \"19\"\n").unwrap();
    gaebboss()
        .args([
            "bid",
            &tender,
            "--prices",
            prices.to_str().unwrap(),
            "--bidder",
            bidder.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    gaebboss()
        .args(["validate", out.to_str().unwrap(), "--tender", &tender])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 errors"));
    gaebboss()
        .args(["info", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("phase: X84"));
}
