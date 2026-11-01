//! Writing a parsed document back out, compact and pretty.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gaebboss_core::Document;
use gaebboss_core::write::WriteOptions;

#[path = "support.rs"]
mod support;

fn bench(c: &mut Criterion) {
    let synthetic = support::synthetic_x83();
    let Ok(doc) = Document::parse(&synthetic) else {
        return;
    };
    let Some(phase) = doc.phase() else { return };
    let compact = WriteOptions {
        version: doc.version(),
        phase,
        indent: None,
        prog_system: None,
        strict: false,
    };
    let pretty = WriteOptions {
        indent: Some(2),
        ..compact.clone()
    };
    let mut group = c.benchmark_group("write");
    group.throughput(Throughput::Bytes(synthetic.len() as u64));
    group.bench_function("compact", |b| {
        b.iter(|| doc.write(&compact).map(|v| v.len()).unwrap_or(0))
    });
    group.bench_function("pretty", |b| {
        b.iter(|| doc.write(&pretty).map(|v| v.len()).unwrap_or(0))
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
