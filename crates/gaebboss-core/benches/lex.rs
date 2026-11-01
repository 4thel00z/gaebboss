//! Tokenizer throughput on the official BVBS X83 repeated to about 10 MB,
//! and on the largest file of `GAEBBOSS_CORPUS` when it is set.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gaebboss_core::lexer::lex;

#[path = "support.rs"]
mod support;

fn bench(c: &mut Criterion) {
    let synthetic = support::synthetic_x83();
    let mut group = c.benchmark_group("lex");
    group.throughput(Throughput::Bytes(synthetic.len() as u64));
    group.bench_function("bvbs-x83-10mb", |b| {
        b.iter(|| lex(&synthetic).map(|t| t.nodes.len()).unwrap_or(0))
    });
    if let Some(real) = support::largest_corpus_file() {
        group.throughput(Throughput::Bytes(real.len() as u64));
        group.bench_function("corpus-largest", |b| {
            b.iter(|| lex(&real).map(|t| t.nodes.len()).unwrap_or(0))
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
