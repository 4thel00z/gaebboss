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

#[test]
fn bid_prices_accept_semicolons_and_decimal_commas() {
    let dir = tempfile::tempdir().unwrap();
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
    let mut csv = String::from("oz;up\n");
    for oz in &rows {
        csv.push_str(&format!("{oz};12,50\n"));
    }
    let prices = dir.path().join("prices.csv");
    std::fs::write(&prices, csv).unwrap();
    let bidder = dir.path().join("bidder.toml");
    std::fs::write(
        &bidder,
        "name1 = \"Bau GmbH\"\nstreet = \"Weg 1\"\npcode = \"12345\"\ncity = \"Berlin\"\n",
    )
    .unwrap();
    let out = dir.path().join("bid.X84");
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
        .args(["items", out.to_str().unwrap(), "--tsv"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\t12.500\t"));
}

#[test]
fn md_prints_the_bill_with_category_headings() {
    gaebboss()
        .args(["md", &fixture("3.3/bvbs_bauausfuehrung.x83")])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("# BVBS GAEB Bauausf."))
        .stdout(predicate::str::contains(
            "Für die Zertifizierung von GAEB DA XML Schnittstellen",
        ))
        .stdout(predicate::str::contains(
            "Hinweistext für die Prüfkriterien GAEB DA XML 3.3 - Bauausführung",
        ))
        .stdout(predicate::str::contains("## 001 Bauhauptgewerke"))
        .stdout(predicate::str::contains(
            "#### 001.001.0010 Baustelleneinrichtung",
        ));
    gaebboss()
        .args([
            "md",
            &fixture("3.3/bvbs_bauausfuehrung.x83"),
            "--oz",
            "001.001.0010",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("Baustelleneinrichtung"));
}

#[test]
fn text_without_oz_prints_every_item() {
    let output = gaebboss()
        .args(["text", &fixture("3.3/bvbs_bauausfuehrung.x83")])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.starts_with("001.001.0010  "), "{text}");
    assert!(text.contains("\n\n001.001.00101  "), "{text}");
}

#[test]
fn q_runs_jq_and_exits_two_on_a_bad_program() {
    gaebboss()
        .args([
            "q",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            ".items[0].oz",
            "-r",
        ])
        .assert()
        .success()
        .stdout("001.001.0010\n");
    gaebboss()
        .args([
            "q",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            ".boq.totals.total",
        ])
        .assert()
        .success()
        .stdout("\"2000000.00\"\n");
    gaebboss()
        .args(["q", &fixture("3.3/bvbs_bauausfuehrung.x84"), ".items["])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("jq: "));
    gaebboss()
        .args([
            "q",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "error(\"boom\")",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("boom"));
}

#[test]
fn hex_dumps_one_element() {
    gaebboss()
        .args([
            "hex",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "id:ID02BED3D8",
            "--width",
            "8",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "  3c 49 74 65 6d 20 49 44  |<Item ID|",
        ));
    gaebboss()
        .args(["hex", &fixture("3.3/bvbs_bauausfuehrung.x84"), "obj:1"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("unknown selector"));
}

#[test]
fn hex_annotate_labels_items() {
    gaebboss()
        .args([
            "hex",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "oz:001.001",
            "--annotate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("── BoQCtgy 001.001"))
        .stdout(predicate::str::contains(
            "── Item 001.001.0010 ID02BED3D8 ──",
        ));
}

#[test]
fn a_missing_file_exits_one() {
    gaebboss()
        .args(["info", "does-not-exist.x83"])
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with("gaebboss: does-not-exist.x83"));
}

#[test]
fn output_is_now_out() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.X84");
    gaebboss()
        .args([
            "roundtrip",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    gaebboss()
        .args([
            "roundtrip",
            &fixture("3.3/bvbs_bauausfuehrung.x84"),
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .code(2);
}

#[test]
fn tui_without_a_terminal_says_so() {
    gaebboss()
        .args(["tui", &fixture("3.3/bvbs_bauausfuehrung.x84")])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("interactive terminal"));
}

#[test]
fn skill_show_prints_the_bundled_skill() {
    gaebboss()
        .args(["skill", "show"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("---\nname: gaebboss\n"));
}
