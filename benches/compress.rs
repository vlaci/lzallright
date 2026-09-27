use std::{env, fs, hint::black_box, path::Path};

use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode,
    Throughput,
};
use lzallright::{backend, LZOCompressor};
use pyo3::{prelude::*, types::PyBytes};

const MB: usize = 1024 * 1024;

const CORPUS: [(&str, &[u8]); 4] = [
    ("prose", include_bytes!("corpus/alice-pg11.txt")),
    ("source", include_bytes!("corpus/sqlite-btree.c")),
    ("json", include_bytes!("corpus/wikidata-Q100020.json")),
    ("log", include_bytes!("corpus/nasa-http-jul95.log")),
];

fn pseudo_random_bytes(len: usize) -> Vec<u8> {
    let mut x = 0x9e37_79b9_7f4a_7c15u64;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

fn builtin_inputs() -> Vec<(String, Vec<u8>)> {
    let mut inputs: Vec<_> = CORPUS
        .iter()
        .map(|(name, data)| (name.to_string(), data.to_vec()))
        .collect();
    inputs.push(("random".into(), pseudo_random_bytes(8 * 1024)));

    with_bulk("default", inputs)
}

fn dir_inputs(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut dirs: Vec<_> = fs::read_dir(dir)
        .expect("LZALLRIGHT_BENCH_CORPUS must be a readable directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();

    let mut rv = vec![];

    for dir in dirs {
        let mut files: Vec<_> = fs::read_dir(&dir)
            .expect("LZALLRIGHT_BENCH_CORPUS must be a readable directory")
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        let inputs = files
            .into_iter()
            .map(|path| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                (name, fs::read(&path).unwrap())
            })
            .collect();

        let dir_name = dir.file_name().unwrap().to_string_lossy();
        rv.extend(with_bulk(&dir_name, inputs));
    }
    rv
}

fn with_bulk(corpus: &str, inputs: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    let bulk = inputs
        .iter()
        .flat_map(|(_, data)| data.iter().copied())
        .collect();
    inputs
        .into_iter()
        .chain([("bulk".into(), bulk)])
        .map(|(name, data)| (format!("{corpus}/{name}"), data))
        .collect()
}

fn inputs() -> Vec<(String, Vec<u8>)> {
    match env::var_os("LZALLRIGHT_BENCH_CORPUS") {
        Some(dir) => dir_inputs(Path::new(&dir)),
        None => builtin_inputs(),
    }
}

fn compress_bound(len: usize) -> usize {
    len + len / 16 + 64 + 3
}

fn configure<M: criterion::measurement::Measurement>(group: &mut BenchmarkGroup<M>, len: usize) {
    group.throughput(Throughput::Bytes(len as u64));
    if len > MB {
        group.sample_size(10).sampling_mode(SamplingMode::Flat);
    } else {
        group.sample_size(100).sampling_mode(SamplingMode::Auto);
    }
}

fn backend(c: &mut Criterion) {
    let inputs = inputs();
    let mut dict = backend::Dict::new();
    let compressed: Vec<Vec<u8>> = inputs
        .iter()
        .map(|(name, data)| {
            let mut out = vec![0; compress_bound(data.len())];
            let len = backend::compress(data, &mut out, &mut dict).unwrap();
            out.truncate(len);
            println!(
                "ratio {name}: {} -> {} bytes ({:.3})",
                data.len(),
                len,
                data.len() as f64 / len as f64
            );
            let mut roundtrip = vec![0; data.len()];
            let written = backend::decompress(&out, &mut roundtrip).unwrap();
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
            b.iter(|| backend::compress(black_box(data), &mut out, &mut dict).unwrap())
        });
    }
    group.finish();

    let mut group = c.benchmark_group("backend/decompress");
    for ((name, data), compressed) in inputs.iter().zip(&compressed) {
        configure(&mut group, data.len());
        let mut out = vec![0; data.len()];
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| backend::decompress(black_box(compressed), &mut out).unwrap())
        });
    }
    group.finish();
}

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

criterion_group!(benches, backend, python_api);
criterion_main!(benches);
