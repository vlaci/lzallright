use cxx::UniquePtr;

pub(crate) use crate::lzo::EResult;

fn map_result(result: lzokay_sys::EResult, out_size: usize) -> Result<usize, EResult> {
    match result {
        lzokay_sys::EResult::Success => Ok(out_size),
        lzokay_sys::EResult::LookbehindOverrun => Err(EResult::LookbehindOverrun),
        lzokay_sys::EResult::OutputOverrun => Err(EResult::OutputOverrun),
        lzokay_sys::EResult::InputOverrun => Err(EResult::InputOverrun),
        lzokay_sys::EResult::InputNotConsumed => Err(EResult::InputNotConsumed(out_size)),
        _ => Err(EResult::Error),
    }
}

pub(crate) struct Dict(UniquePtr<lzokay_sys::DictBase>);

impl Default for Dict {
    fn default() -> Self {
        Self(lzokay_sys::new_dict())
    }
}

impl Dict {
    pub(crate) fn new() -> Box<Self> {
        Box::default()
    }
}

pub(crate) fn compress(src: &[u8], dst: &mut [u8], dict: &mut Dict) -> Result<usize, EResult> {
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

pub(crate) fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, EResult> {
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

    // Regression tests verifying the C++ lzokay implementation handles the
    // same malformed inputs that triggered UB in the Rust port. The C++
    // library has the same zero-byte-length scanning bug.

    #[test]
    fn regression_m1_zero_scan_oob() {
        let _ = decompress(&[0x00, 0x00, 0x00, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m3_zero_scan_oob() {
        let _ = decompress(&[0x12, 0xAA, 0x20, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m4_zero_scan_oob() {
        let _ = decompress(&[0x12, 0xAA, 0x10, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m2_lookbehind_overrun() {
        let _ = decompress(&[0x12, 0xAA, 0xC0, 0xFF], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m4_lookbehind_overrun() {
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
