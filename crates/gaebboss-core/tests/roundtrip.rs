//! Writing documents back: idempotent canonical output, lossless same-version
//! round trips, and phase profiles applied on the way out.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::name::Name;
use gaebboss_core::version::{Phase, Version};
use gaebboss_core::write::{WriteError, WriteOptions};

fn official() -> Vec<std::path::PathBuf> {
    let root = format!(
        "{}/../../tests/fixtures/official",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut files = Vec::new();
    for dir in ["3.1", "3.3"] {
        for entry in std::fs::read_dir(format!("{root}/{dir}"))
            .unwrap()
            .flatten()
        {
            files.push(entry.path());
        }
    }
    files.sort();
    files
}

fn same_target(doc: &Document) -> WriteOptions {
    WriteOptions {
        version: doc.version(),
        phase: doc.phase().unwrap(),
        indent: None,
        prog_system: None,
        strict: false,
    }
}

fn projection(doc: &Document) -> Vec<String> {
    let mut rows: Vec<String> = doc
        .items()
        .map(|i| {
            format!(
                "{}|{}|{:?}|{:?}|{:?}|{:?}|{}|{}",
                i.id(),
                i.oz,
                i.kind(),
                i.quantity(),
                i.unit_price(),
                i.total(),
                i.short_text(),
                i.long_text()
            )
        })
        .collect();
    rows.extend(
        doc.categories()
            .map(|c| format!("C{}|{}|{}", c.depth, c.oz, c.label)),
    );
    rows.push(format!("T{:?}", doc.computed_totals()));
    rows
}

#[test]
fn write_bvbs_x84_then_parse_equal() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let bytes = doc.write(&same_target(&doc)).unwrap();
    let again = Document::parse(&bytes).unwrap();
    assert_eq!(again.version(), Version::V33);
    assert_eq!(again.phase(), Some(Phase::X84));
    assert_eq!(projection(&again), projection(&doc));
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert!(bytes.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA84/3.3\">")
    );
}

#[test]
fn write_is_idempotent_canonical_for_official_fixtures() {
    for path in official() {
        let doc = Document::open(&path).unwrap();
        if doc.version() == Version::V31 {
            continue;
        }
        let opts = same_target(&doc);
        let first = doc.write(&opts).unwrap();
        let second = Document::parse(&first).unwrap().write(&opts).unwrap();
        assert_eq!(first, second, "{}", path.display());
        assert_eq!(
            projection(&Document::parse(&first).unwrap()),
            projection(&doc),
            "{}",
            path.display()
        );
    }
}

#[test]
fn pretty_output_round_trips_too() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x83",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let opts = WriteOptions {
        indent: Some(2),
        ..same_target(&doc)
    };
    let pretty = doc.write(&opts).unwrap();
    assert!(
        std::str::from_utf8(&pretty)
            .unwrap()
            .contains("\n  <GAEBInfo>")
    );
    assert_eq!(
        projection(&Document::parse(&pretty).unwrap()),
        projection(&doc)
    );
}

#[test]
fn unknown_extra_survives_same_version_roundtrip() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><Name>x</Name><LblBoQ>Test</LblBoQ><OutlCompl>AllTxt</OutlCompl><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Qty>1.000</Qty><Foo a=\"1\">vendor &amp; co</Foo><QU>m</QU></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let out = doc.write(&same_target(&doc)).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    assert!(
        text.contains("<Foo a=\"1\">vendor &amp; co</Foo>"),
        "{text}"
    );
    assert!(text.contains("<Qty>1.000</Qty>"));
    assert!(text.contains("<QU>m</QU>"));
    let other_version = WriteOptions {
        version: Version::V32,
        ..same_target(&doc)
    };
    let (bytes, report) = doc.write_with_report(&other_version).unwrap();
    assert!(!std::str::from_utf8(&bytes).unwrap().contains("<Foo"));
    assert!(
        report
            .dropped
            .iter()
            .any(|d| d.element == "Foo" && d.count == 1),
        "{:?}",
        report.dropped
    );
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("<Version>3.2</Version><VersDate>2013-10</VersDate>")
    );
}

#[test]
fn x84_profile_drops_tender_only_elements_and_reports() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x83",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let opts = WriteOptions {
        phase: Phase::X84,
        strict: true,
        ..same_target(&doc)
    };
    let result = doc.write_with_report(&opts);
    let lenient = doc
        .write_with_report(&WriteOptions {
            strict: false,
            ..opts.clone()
        })
        .unwrap()
        .1;
    assert!(
        lenient
            .missing
            .iter()
            .any(|m| m.type_name == "Award" && m.element == Name::CTR),
        "{:?}",
        lenient.missing
    );
    let Err(WriteError::MissingRequired {
        type_name, element, ..
    }) = result
    else {
        panic!("an X83 without a bidder cannot be a valid X84: {result:?}")
    };
    assert_eq!(type_name, "Award");
    assert_eq!(element, Name::CTR);
}

#[test]
fn x84_profile_from_x83_drops_and_keeps_the_right_elements() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><PrjInfo><NamePrj>P</NamePrj></PrjInfo><Award><DP>83</DP><AwardInfo><Cur>EUR</Cur></AwardInfo><OWN><Address><Name1>Owner</Name1><Street>s</Street><PCode>1</PCode><City>c</City></Address></OWN><CTR><Address><Name1>Bidder</Name1><Street>s</Street><PCode>1</PCode><City>c</City></Address></CTR><BoQ ID=\"b\"><BoQInfo><Name>x</Name><LblBoQ>Test</LblBoQ><OutlCompl>AllTxt</OutlCompl><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn><Totals><Total>0.00</Total></Totals></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Qty>1.000</Qty><QU>m</QU></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let opts = WriteOptions {
        phase: Phase::X84,
        ..same_target(&doc)
    };
    let (bytes, report) = doc.write_with_report(&opts).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("<DP>84</DP>"), "{text}");
    assert!(!text.contains("<OWN>"), "{text}");
    assert!(!text.contains("<QU>"), "{text}");
    assert!(report.dropped.iter().any(|d| d.element == "OWN"));
    assert!(report.dropped.iter().any(|d| d.element == "QU"));
    let again = Document::parse(&bytes).unwrap();
    assert_eq!(again.phase(), Some(Phase::X84));
}

#[test]
fn target_version_31_is_unsupported() {
    let path = format!(
        "{}/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x84",
        env!("CARGO_MANIFEST_DIR")
    );
    let doc = Document::open(std::path::Path::new(&path)).unwrap();
    let opts = WriteOptions {
        version: Version::V31,
        ..same_target(&doc)
    };
    assert!(matches!(
        doc.write(&opts),
        Err(WriteError::UnsupportedTarget { .. })
    ));
}

#[test]
fn text_is_escaped_on_the_way_out() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version><VersDate>2021-05</VersDate><Date>2026-01-01</Date></GAEBInfo><Award><DP>83</DP><OWN><Address><Name1>A &amp; B &lt;GmbH&gt;</Name1><Street>s</Street><PCode>1</PCode><City>c</City></Address></OWN><BoQ ID=\"b\"><BoQInfo><Name>x</Name><LblBoQ>Test</LblBoQ><OutlCompl>AllTxt</OutlCompl><BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody><Itemlist><Item ID=\"i&quot;1\" RNoPart=\"1\"><Qty>1.000</Qty><QU>m</QU></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let out = doc.write(&same_target(&doc)).unwrap();
    let text = std::str::from_utf8(&out).unwrap();
    assert!(
        text.contains("<Name1>A &amp; B &lt;GmbH&gt;</Name1>"),
        "{text}"
    );
    assert!(text.contains("ID=\"i&quot;1\""), "{text}");
}

#[test]
fn every_corpus_file_roundtrips() {
    let Some(dir) = std::env::var_os("GAEBBOSS_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(d).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();
            if ext.starts_with('X') {
                files.push(path);
            }
        }
    }
    files.sort();
    let started = std::time::Instant::now();
    let mut bytes = 0usize;
    let mut dropped_files = 0usize;
    for path in &files {
        let doc = Document::open(path).unwrap();
        if doc.version() < Version::V32 || doc.phase().is_none() {
            continue;
        }
        let opts = same_target(&doc);
        let (first, report) = doc
            .write_with_report(&opts)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        bytes += first.len();
        let reparsed =
            Document::parse(&first).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let second = reparsed.write(&opts).unwrap();
        assert_eq!(first, second, "{}", path.display());
        if !report.dropped.is_empty() {
            dropped_files += 1;
            continue;
        }
        let (before, after) = (projection(&doc), projection(&reparsed));
        if before != after {
            let at = before
                .iter()
                .zip(&after)
                .position(|(a, b)| a != b)
                .unwrap_or(before.len().min(after.len()));
            let (a, b) = (
                before.get(at).cloned().unwrap_or_default(),
                after.get(at).cloned().unwrap_or_default(),
            );
            let diff = a
                .char_indices()
                .zip(b.char_indices())
                .find(|((_, x), (_, y))| x != y)
                .map(|((i, _), _)| i)
                .unwrap_or(a.len().min(b.len()));
            let cut = |s: &str| {
                s.chars()
                    .skip(diff.saturating_sub(150))
                    .take(400)
                    .collect::<String>()
            };
            panic!(
                "{}: row {at} differs\n before: {}\n after:  {}",
                path.display(),
                before.get(at).map(|r| cut(r)).unwrap_or_default(),
                after.get(at).map(|r| cut(r)).unwrap_or_default()
            );
        }
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "wrote {} MB twice in {:.2}s = {:.0} MB/s; {dropped_files} files carried elements their own phase forbids",
        bytes >> 20,
        secs,
        2.0 * bytes as f64 / 1e6 / secs
    );
}
