//! Walking every item with Ordnungszahl, kind and texts.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gaebboss_core::Document;

#[path = "support.rs"]
mod support;

fn bench(c: &mut Criterion) {
    let synthetic = support::synthetic_x83();
    let Ok(doc) = Document::parse(&synthetic) else {
        return;
    };
    let count = doc.items().count() as u64;
    let mut group = c.benchmark_group("items");
    group.throughput(Throughput::Elements(count));
    group.bench_function("oz-and-kind", |b| {
        b.iter(|| {
            doc.items()
                .map(|i| i.oz.dotted().len() + i.kind() as usize)
                .sum::<usize>()
        })
    });
    group.bench_function("with-texts", |b| {
        b.iter(|| {
            doc.items()
                .map(|i| i.short_text().len() + i.long_text().len())
                .sum::<usize>()
        })
    });
    group.bench_function("totals", |b| {
        b.iter(|| doc.computed_totals().categories.len())
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
