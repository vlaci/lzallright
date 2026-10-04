use std::{env, fs, path::Path};

use criterion::{BenchmarkGroup, SamplingMode, Throughput};

const MB: usize = 1024 * 1024;

const CORPUS: [(&str, &[u8]); 4] = [
    ("prose", include_bytes!("../corpus/alice-pg11.txt")),
    ("source", include_bytes!("../corpus/sqlite-btree.c")),
    ("json", include_bytes!("../corpus/wikidata-Q100020.json")),
    ("log", include_bytes!("../corpus/nasa-http-jul95.log")),
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

pub fn inputs() -> Vec<(String, Vec<u8>)> {
    match env::var_os("LZALLRIGHT_BENCH_CORPUS") {
        Some(dir) => dir_inputs(Path::new(&dir)),
        None => builtin_inputs(),
    }
}

#[allow(dead_code)] // Unused by the Python bench.
pub fn compress_bound(len: usize) -> usize {
    len + len / 16 + 64 + 3
}

pub fn configure<M: criterion::measurement::Measurement>(
    group: &mut BenchmarkGroup<M>,
    len: usize,
) {
    group.throughput(Throughput::Bytes(len as u64));
    if len > MB {
        group.sample_size(10).sampling_mode(SamplingMode::Flat);
    } else {
        group.sample_size(100).sampling_mode(SamplingMode::Auto);
    }
}
