use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

mod common;
use common::{compress_bound, configure, inputs};

fn backend(c: &mut Criterion) {
    let inputs = inputs();
    let mut dict = lzallright::Dict::new();
    let compressed: Vec<Vec<u8>> = inputs
        .iter()
        .map(|(name, data)| {
            let mut out = vec![0; compress_bound(data.len())];
            let len = lzallright::compress(data, &mut out, &mut dict).unwrap();
            out.truncate(len);
            println!(
                "ratio {name}: {} -> {} bytes ({:.3})",
                data.len(),
                len,
                data.len() as f64 / len as f64
            );
            let mut roundtrip = vec![0; data.len()];
            let written = lzallright::decompress(&out, &mut roundtrip).unwrap();
            assert!(
                written == data.len() && roundtrip == *data,
                "{name}: round-trip mismatch"
            );
            out
        })
        .collect();

    let mut group = c.benchmark_group("backend/compress");
    for (name, data) in &inputs {
        configure(&mut group, data.len());
        let mut out = vec![0; compress_bound(data.len())];
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| lzallright::compress(black_box(data), &mut out, &mut dict).unwrap())
        });
    }
    group.finish();

    let mut group = c.benchmark_group("backend/decompress");
    for ((name, data), compressed) in inputs.iter().zip(&compressed) {
        configure(&mut group, data.len());
        let mut out = vec![0; data.len()];
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| lzallright::decompress(black_box(compressed), &mut out).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, backend);
criterion_main!(benches);
