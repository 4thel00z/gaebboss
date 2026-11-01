//! Full typed parse (normalize, tokenize, detect, read) throughput on the
//! official X83 repeated to about 10 MB and on the largest corpus file.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gaebboss_core::Document;

#[path = "support.rs"]
mod support;

fn bench(c: &mut Criterion) {
    let synthetic = support::synthetic_x83();
    let mut group = c.benchmark_group("parse");
    group.throughput(Throughput::Bytes(synthetic.len() as u64));
    group.bench_function("bvbs-x83-10mb", |b| {
        b.iter(|| {
            Document::parse(&synthetic)
                .map(|d| d.warnings.len())
                .unwrap_or(0)
        })
    });
    if let Some(real) = support::largest_corpus_file() {
        group.throughput(Throughput::Bytes(real.len() as u64));
        group.bench_function("corpus-largest", |b| {
            b.iter(|| {
                Document::parse(&real)
                    .map(|d| d.warnings.len())
                    .unwrap_or(0)
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
