//! The largest real-world file must parse at speed; a quadratic path or a
//! per-node allocation regression shows up here first.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;

fn largest_corpus_file() -> Option<(std::path::PathBuf, Vec<u8>)> {
    let dir = std::env::var_os("GAEBBOSS_CORPUS")?;
    let mut best: Option<(u64, std::path::PathBuf)> = None;
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(d).ok()?.flatten() {
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
            if !ext.starts_with('X') {
                continue;
            }
            let len = entry.metadata().ok()?.len();
            if best.as_ref().is_none_or(|(l, _)| len > *l) {
                best = Some((len, path));
            }
        }
    }
    let (_, path) = best?;
    let bytes = std::fs::read(&path).ok()?;
    Some((path, bytes))
}

#[test]
fn largest_corpus_file_parses_at_least_100_mb_per_second() {
    let Some((path, bytes)) = largest_corpus_file() else {
        return;
    };
    let floor = match cfg!(debug_assertions) {
        true => 10.0,
        false => 100.0,
    };
    let started = std::time::Instant::now();
    let doc = Document::parse(&bytes).unwrap();
    let items = doc.items().count();
    let secs = started.elapsed().as_secs_f64();
    let rate = bytes.len() as f64 / 1e6 / secs;
    eprintln!(
        "{}: {:.1} MB, {items} items, {:.3}s = {rate:.0} MB/s",
        path.display(),
        bytes.len() as f64 / 1e6,
        secs
    );
    assert!(
        rate >= floor,
        "{rate:.0} MB/s is below the {floor} MB/s floor"
    );
}
