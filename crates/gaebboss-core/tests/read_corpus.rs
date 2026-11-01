//! Every real-world file in `GAEBBOSS_CORPUS` reads without an error.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use gaebboss_core::Document;

#[test]
fn every_corpus_file_reads() {
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
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut details: BTreeMap<String, usize> = BTreeMap::new();
    let mut bytes = 0usize;
    for path in &files {
        let doc = Document::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            doc.gaeb.award.is_some() || doc.gaeb.gaeb_info.is_some(),
            "{}",
            path.display()
        );
        bytes += std::fs::metadata(path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        for w in doc
            .warnings
            .iter()
            .filter(|w| w.message.starts_with("<ComplBody>"))
            .take(1)
        {
            let at = w.offset as usize;
            let ctx = &doc.bytes()[at.saturating_sub(400)..(at + 120).min(doc.bytes().len())];
            eprintln!(
                "CONTEXT {}:\n{}",
                path.display(),
                String::from_utf8_lossy(ctx)
            );
        }
        for w in &doc.warnings {
            *histogram.entry(format!("{:?}", w.code)).or_default() += 1;
            if w.code == gaebboss_core::warning::WarningCode::UnknownElement
                || w.code == gaebboss_core::warning::WarningCode::Cardinality
            {
                let key = w
                    .message
                    .split(" in the schema")
                    .next()
                    .unwrap_or(&w.message)
                    .to_string();
                *details.entry(key).or_default() += 1;
            }
        }
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "read {} files, {} MB in {:.2}s = {:.0} MB/s",
        files.len(),
        bytes >> 20,
        secs,
        bytes as f64 / 1e6 / secs
    );
    for (code, n) in &histogram {
        eprintln!("  {code}: {n}");
    }
    let mut top: Vec<(&String, &usize)> = details.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    for (message, n) in top.iter().take(25) {
        eprintln!("    {n:6}  {message}");
    }
}
