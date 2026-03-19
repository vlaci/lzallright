use cxx::UniquePtr;

#[derive(Debug, PartialEq)]
pub(crate) enum EResult {
    LookbehindOverrun,
    OutputOverrun,
    InputOverrun,
    InputNotConsumed(usize),
    Error,
}

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
    pub(crate) fn new() -> Self {
        Self::default()
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
