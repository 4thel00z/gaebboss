//! Inputs shared by the benchmarks.
#![allow(dead_code)]

/// The official BVBS X83 with its body repeated to about 10 MB.
pub fn synthetic_x83() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/official/3.3/bvbs_bauausfuehrung.x83"
    );
    let one = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&one);
    let (head, rest) = text.split_once("<BoQBody>").unwrap_or(("", ""));
    let (body, tail) = rest.rsplit_once("</BoQBody>").unwrap_or(("", ""));
    let mut out = String::with_capacity(10 << 20);
    out.push_str(head);
    out.push_str("<BoQBody>");
    let mut copy = 0;
    while out.len() < 10 << 20 {
        copy += 1;
        out.push_str(
            &body
                .replace("RNoPart=\"", &format!("RNoPart=\"{copy}"))
                .replace("ID=\"", &format!("ID=\"c{copy}")),
        );
    }
    out.push_str("</BoQBody>");
    out.push_str(tail);
    out.into_bytes()
}

/// The largest `X*` file under `GAEBBOSS_CORPUS`, when set.
pub fn largest_corpus_file() -> Option<Vec<u8>> {
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
    std::fs::read(best?.1).ok()
}
