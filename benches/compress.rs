use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use lzallright::LZOCompressor;
use pyo3::prelude::*;

pub const LOREM: &[u8] = include_bytes!("lorem.txt");

fn bench_compress(b: &mut Bencher<'_>) {
    Python::with_gil(|py| {
        let mut comp = LZOCompressor::new();
        b.iter(|| comp.compress(py, black_box(LOREM.into())));
    });
}

fn bench_decompress(b: &mut Bencher<'_>) {
    Python::with_gil(|py| {
        let mut comp = LZOCompressor::new();
        let data = comp.compress(py, LOREM.into()).unwrap();

        b.iter(|| LZOCompressor::decompress(py, black_box(data.as_bytes().into())));
    });
}

fn bench_compress_big(b: &mut Bencher<'_>) {
    let mut data = Vec::with_capacity(LOREM.len() * 100);
    for _ in 0..100 {
        data.extend_from_slice(LOREM);
    }

    Python::with_gil(|py| {
        let mut comp = LZOCompressor::new();
        b.iter(|| comp.compress(py, black_box(data[..].into())));
    });
}

fn bench_decompress_big(b: &mut Bencher<'_>) {
    let mut data = Vec::with_capacity(LOREM.len() * 100);
    for _ in 0..100 {
        data.extend_from_slice(LOREM);
    }

    Python::with_gil(|py| {
        let mut comp = LZOCompressor::new();
        let data = comp.compress(py, data[..].into()).unwrap();

        b.iter(|| LZOCompressor::decompress(py, black_box(data.as_bytes().into())));
    });
}

pub fn criterion_benchmark(c: &mut Criterion) {
    pyo3::prepare_freethreaded_python();
    c.bench_function("compress", bench_compress);
    c.bench_function("decompress", bench_decompress);
    c.bench_function("compress_big", bench_compress_big);
    c.bench_function("decompress_big", bench_decompress_big);
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(65));
    targets=criterion_benchmark
);

criterion_main!(benches);
