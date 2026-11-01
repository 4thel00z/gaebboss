//! `cargo xtask corpus --dir DIR`: files, bytes and throughput of every
//! stage over a directory of GAEB files, plus a warning histogram.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use gaebboss_core::Document;
use gaebboss_core::write::WriteOptions;

pub fn run(dir: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for entry in entries.flatten() {
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
    let inputs: Vec<(PathBuf, Vec<u8>)> = files
        .iter()
        .map(|p| (p.clone(), std::fs::read(p).unwrap_or_default()))
        .collect();
    let bytes: usize = inputs.iter().map(|(_, b)| b.len()).sum();
    println!("{} files, {:.1} MB", inputs.len(), bytes as f64 / 1e6);
    let mb_per_s = |secs: f64| bytes as f64 / 1e6 / secs;

    let started = Instant::now();
    let mut nodes = 0usize;
    for (_, b) in &inputs {
        let normalized = gaebboss_core::bytes::normalize(b).map_err(|e| e.to_string())?;
        nodes += gaebboss_core::lexer::lex(&normalized.bytes)
            .map(|t| t.nodes.len())
            .unwrap_or(0);
    }
    println!(
        "lex     {:>7.0} MB/s  ({nodes} nodes)",
        mb_per_s(started.elapsed().as_secs_f64())
    );

    let started = Instant::now();
    let mut docs = Vec::with_capacity(inputs.len());
    let mut errors = 0usize;
    for (path, b) in &inputs {
        match Document::parse_named(b, path.file_name().and_then(|n| n.to_str())) {
            Ok(doc) => docs.push(doc),
            Err(e) => {
                errors += 1;
                eprintln!("{}: {e}", path.display());
            }
        }
    }
    println!(
        "parse   {:>7.0} MB/s  ({errors} errors)",
        mb_per_s(started.elapsed().as_secs_f64())
    );

    let started = Instant::now();
    let mut items = 0usize;
    let mut text_bytes = 0usize;
    for doc in &docs {
        for item in doc.items() {
            items += 1;
            text_bytes += item.short_text().len() + item.long_text().len();
        }
    }
    println!(
        "items   {:>7.0} MB/s  ({items} items, {:.1} MB of text)",
        mb_per_s(started.elapsed().as_secs_f64()),
        text_bytes as f64 / 1e6
    );

    let started = Instant::now();
    let mut written = 0usize;
    for doc in &docs {
        let Some(phase) = doc.phase() else { continue };
        let opts = WriteOptions {
            version: doc.version(),
            phase,
            indent: None,
            prog_system: None,
            strict: false,
        };
        written += doc.write(&opts).map(|v| v.len()).unwrap_or(0);
    }
    println!(
        "write   {:>7.0} MB/s  ({:.1} MB written)",
        written as f64 / 1e6 / started.elapsed().as_secs_f64(),
        written as f64 / 1e6
    );

    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    for doc in &docs {
        for w in &doc.warnings {
            *histogram.entry(format!("{:?}", w.code)).or_default() += 1;
        }
    }
    println!("warnings:");
    for (code, n) in histogram {
        println!("  {code:<20} {n}");
    }
    Ok(())
}
