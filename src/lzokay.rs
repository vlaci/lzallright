use cxx::UniquePtr;

use crate::error::{Error, ErrorKind};

fn map_result(result: lzokay_sys::EResult, out_size: usize) -> Result<usize, Error> {
    match result {
        lzokay_sys::EResult::Success => Ok(out_size),
        lzokay_sys::EResult::LookbehindOverrun => {
            Err(Error::new(ErrorKind::LookbehindOverrun, out_size))
        }
        lzokay_sys::EResult::OutputOverrun => Err(Error::new(ErrorKind::OutputOverrun, out_size)),
        lzokay_sys::EResult::InputOverrun => Err(Error::new(ErrorKind::InputOverrun, out_size)),
        lzokay_sys::EResult::InputNotConsumed => {
            Err(Error::new(ErrorKind::InputNotConsumed, out_size))
        }
        _ => Err(Error::new(ErrorKind::Error, out_size)),
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
