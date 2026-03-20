#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut out = vec![0u8; 1 << 18];
    let _ = lzallright::lzo::decompress(data, &mut out);
});
