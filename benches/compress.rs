#![allow(clippy::identity_op)]

use core::time::Duration;
use std::hint::black_box;
use std::io::Write;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use lzallright::LZOCompressor;
use pyo3::prelude::*;

pub const LOREM: &[u8] = include_bytes!("lorem.txt");

const KB: usize = 1024;
const MB: usize = 1024 * KB;

pub fn compress(c: &mut Criterion) {
    pyo3::prepare_freethreaded_python();
    let mut group = c.benchmark_group("LZO compression");
    for sample_size in [1 * KB, 64 * KB, 256 * KB, 1 * MB, 64 * MB, 256 * MB] {
        let mut data: Vec<u8> = Vec::with_capacity(sample_size);
        while data.len() < sample_size {
            data.write_all(LOREM).unwrap();
        }
        if sample_size > 1 * MB {
            group
                .sample_size(10)
                .measurement_time(Duration::from_secs(60));
        }

        group.throughput(Throughput::Bytes(sample_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(sample_size),
            &sample_size,
            |b, &size| {
                Python::with_gil(|py| {
                    let mut comp = LZOCompressor::new();
                    b.iter_batched(
                        || data[0..size].into(),
                        |data| comp.compress(py, black_box(data)),
                        BatchSize::SmallInput,
                    );
                })
            },
        );
    }
    group.finish();
}
pub fn decompress(c: &mut Criterion) {
    pyo3::prepare_freethreaded_python();
    let mut group = c.benchmark_group("LZO decompression");
    for sample_size in [1 * KB, 64 * KB, 256 * KB, 1 * MB, 64 * MB, 256 * MB] {
        let mut data: Vec<u8> = Vec::with_capacity(sample_size);
        while data.len() < sample_size {
            data.write_all(LOREM).unwrap();
        }

        let data = Python::with_gil(|py| {
            let mut comp = LZOCompressor::new();
            comp.compress(py, data[0..sample_size].into())
                .unwrap()
                .as_bytes()
                .to_owned()
        });

        group.throughput(Throughput::Bytes(sample_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(sample_size),
            &sample_size,
            |b, &size| {
                Python::with_gil(|py| {
                    b.iter_batched(
                        || data[..].into(),
                        |data| LZOCompressor::decompress(py, black_box(data), Some(size)),
                        BatchSize::SmallInput,
                    );
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{sample_size} no hint")),
            &sample_size,
            |b, &_size| {
                Python::with_gil(|py| {
                    b.iter_batched(
                        || data[..].into(),
                        |data| LZOCompressor::decompress(py, black_box(data), None),
                        BatchSize::SmallInput,
                    );
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, compress, decompress);
criterion_main!(benches);
