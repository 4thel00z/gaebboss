//! Tokenizer throughput on the official BVBS X83 repeated to about 10 MB,
//! and on the largest file of `GAEBBOSS_CORPUS` when it is set.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gaebboss_core::lexer::lex;

fn fixture() -> Vec<u8> {
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
    while out.len() < 10 << 20 {
        out.push_str(body);
    }
    out.push_str("</BoQBody>");
    out.push_str(tail);
    out.into_bytes()
}

fn largest_corpus_file() -> Option<Vec<u8>> {
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

fn bench(c: &mut Criterion) {
    let synthetic = fixture();
    let mut group = c.benchmark_group("lex");
    group.throughput(Throughput::Bytes(synthetic.len() as u64));
    group.bench_function("bvbs-x83-10mb", |b| {
        b.iter(|| lex(&synthetic).map(|t| t.nodes.len()).unwrap_or(0))
    });
    if let Some(real) = largest_corpus_file() {
        group.throughput(Throughput::Bytes(real.len() as u64));
        group.bench_function("corpus-largest", |b| {
            b.iter(|| lex(&real).map(|t| t.nodes.len()).unwrap_or(0))
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
