#![no_main]

use lzallright::{compress, decompress, Dict};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut comp = vec![0u8; data.len() + data.len() / 16 + 64 + 3];
    let len = compress(data, &mut comp, &mut Dict::new()).unwrap();
    let mut out = vec![0u8; data.len()];
    assert_eq!(decompress(&comp[..len], &mut out).unwrap(), data.len());
    assert_eq!(out, data);
});
