use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use lzallright_python::LZOCompressor;
use pyo3::{prelude::*, types::PyBytes};

mod common;
use common::{configure, inputs};

fn python_api(c: &mut Criterion) {
    Python::initialize();
    let inputs = inputs();
    Python::attach(|py| {
        let comp = Bound::new(py, LZOCompressor::new()).unwrap();
        let cases: Vec<_> = inputs
            .iter()
            .map(|(name, data)| {
                let raw = PyBytes::new(py, data);
                let compressed = comp.call_method1("compress", (&raw,)).unwrap();
                (name, data.len(), raw, compressed)
            })
            .collect();

        let mut group = c.benchmark_group("python_api/compress");
        for (name, len, raw, _) in &cases {
            configure(&mut group, *len);
            group.bench_function(BenchmarkId::from_parameter(name), |b| {
                b.iter(|| comp.call_method1("compress", (black_box(raw),)).unwrap())
            });
        }
        group.finish();

        let mut group = c.benchmark_group("python_api/decompress");
        for (name, len, _, compressed) in &cases {
            configure(&mut group, *len);
            group.bench_function(BenchmarkId::new("hint", name), |b| {
                b.iter(|| {
                    comp.call_method1("decompress", (black_box(compressed), *len))
                        .unwrap()
                })
            });
            group.bench_function(BenchmarkId::new("no-hint", name), |b| {
                b.iter(|| {
                    comp.call_method1("decompress", (black_box(compressed),))
                        .unwrap()
                })
            });
        }
        group.finish();
    });
}

criterion_group!(benches, python_api);
criterion_main!(benches);
