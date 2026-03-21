use cxx::UniquePtr;

#[derive(Debug)]
pub enum Error {
    LookbehindOverrun,
    OutputOverrun,
    InputOverrun,
    InputNotConsumed(usize),
    Error,
}

fn map_result(result: lzokay_sys::EResult, out_size: usize) -> Result<usize, Error> {
    match result {
        lzokay_sys::EResult::Success => Ok(out_size),
        lzokay_sys::EResult::LookbehindOverrun => Err(Error::LookbehindOverrun),
        lzokay_sys::EResult::OutputOverrun => Err(Error::OutputOverrun),
        lzokay_sys::EResult::InputOverrun => Err(Error::InputOverrun),
        lzokay_sys::EResult::InputNotConsumed => Err(Error::InputNotConsumed(out_size)),
        _ => Err(Error::Error),
    }
}

pub struct Dict(UniquePtr<lzokay_sys::DictBase>);

impl Default for Dict {
    fn default() -> Self {
        Self(lzokay_sys::new_dict())
    }
}

impl Dict {
    pub fn new() -> Self {
        Self::default()
    }
}

pub fn compress(src: &[u8], dst: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    let mut out_size: usize = 0;
    // SAFETY: src and dst slice pointers/lengths are valid by construction.
    let result = unsafe {
        lzokay_sys::compress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            dst.len(),
            &mut out_size,
            dict.0.pin_mut(),
        )
    };
    map_result(result, out_size)
}

pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    let mut out_size: usize = 0;
    // SAFETY: src and dst slice pointers/lengths are valid by construction.
    let result = unsafe {
        lzokay_sys::decompress(
            src.as_ptr(),
            src.len(),
            dst.as_mut_ptr(),
            dst.len(),
            &mut out_size,
        )
    };
    map_result(result, out_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Crashing test samples found by running `cargo fuzz decompress`
    // in the C++ lzokay library (out-of-bounds reads during
    // zero-byte-length scanning and lookbehind pointer arithmetic).
    //
    // Run under AddressSanitizer (C++ & Rust):
    //
    //     CXXFLAGS="-fsanitize=address" RUSTFLAGS="-Zsanitizer=address" \
    //       cargo test -Zbuild-std --target x86_64-unknown-linux-gnu \
    //       -- lzokay::tests::fuzz_crash_

    #[test]
    fn fuzz_crash_m1_zero_scan_oob() {
        let _ = decompress(&[0x00, 0x00, 0x00, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn fuzz_crash_m3_zero_scan_oob() {
        let _ = decompress(&[0x12, 0xAA, 0x20, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn fuzz_crash_m4_zero_scan_oob() {
        let _ = decompress(&[0x12, 0xAA, 0x10, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn fuzz_crash_m2_lookbehind_overrun() {
        let _ = decompress(&[0x12, 0xAA, 0xC0, 0xFF], &mut [0u8; 1024]);
    }

    #[test]
    fn fuzz_crash_m4_lookbehind_overrun() {
        let mut dict = Dict::new();
        let compressed = {
            let mut buf = vec![0u8; 67];
            let len = compress(&[], &mut buf, &mut dict).expect("Compress failed");
            buf.truncate(len);
            buf
        };
        let _ = decompress(&compressed, &mut [0u8; 0]);
    }
}
