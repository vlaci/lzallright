use cxx::UniquePtr;
use pyo3::{
    create_exception,
    ffi::PyBytes_FromObject,
    prelude::*,
    types::{PyByteArray, PyBytes},
    AsPyPointer,
};

use crate::python::Buffer;

#[derive(Debug, PartialEq, Eq)]
#[pyclass]
pub enum EResult {
    LookbehindOverrun,
    OutputOverrun,
    InputOverrun,
    Error,
    InputNotConsumed,
}

impl From<lzokay_sys::EResult> for EResult {
    fn from(err: lzokay_sys::EResult) -> Self {
        match err {
            lzokay_sys::EResult::LookbehindOverrun => EResult::LookbehindOverrun,
            lzokay_sys::EResult::OutputOverrun => EResult::OutputOverrun,
            lzokay_sys::EResult::InputOverrun => EResult::InputOverrun,
            lzokay_sys::EResult::Error => EResult::Error,
            lzokay_sys::EResult::InputNotConsumed => EResult::InputNotConsumed,
            _ => unreachable!(),
        }
    }
}

create_exception!(module, LZOError, pyo3::exceptions::PyException);
create_exception!(module, InputNotConsumed, LZOError);

#[pyclass(unsendable)]
pub struct LZOCompressor {
    dict: UniquePtr<lzokay_sys::DictBase>,
}

#[pymethods]
impl LZOCompressor {
    #[new]
    pub fn new() -> Self {
        Self {
            dict: lzokay_sys::new_dict(),
        }
    }

    pub fn compress<'a>(&mut self, py: Python<'a>, data: Buffer) -> PyResult<&'a PyBytes> {
        let max_size = data.len() + data.len() / 16 + 64 + 3;
        let mut result = lzokay_sys::EResult::Error;
        let mut compressed_size = 0usize;
        let dst = PyByteArray::new_with(py, max_size, |dst| {
            result = py.allow_threads(|| unsafe {
                lzokay_sys::compress(
                    data.as_ptr(),
                    data.len(),
                    dst.as_mut_ptr(),
                    dst.len(),
                    &mut compressed_size,
                    self.dict.pin_mut(),
                )
            });
            Ok(())
        })?;
        dst.resize(compressed_size)?;
        match result {
            lzokay_sys::EResult::Success => {
                Ok(unsafe { py.from_owned_ptr(PyBytes_FromObject(dst.as_ptr())) })
            }
            e => Err(LZOError::new_err(EResult::from(e))),
        }
    }

    #[staticmethod]
    pub fn decompress<'a>(py: Python<'a>, data: Buffer) -> PyResult<&'a PyBytes> {
        let size = 2 * data.len();
        let mut decompressed_size = 0usize;
        let mut result;
        let dst = PyByteArray::new_with(py, size, |_| Ok(()))?;
        loop {
            let dst_bytes = unsafe { dst.as_bytes_mut() };
            result = py.allow_threads(|| unsafe {
                lzokay_sys::decompress(
                    data.as_ptr(),
                    data.len(),
                    dst_bytes.as_mut_ptr(),
                    dst_bytes.len(),
                    &mut decompressed_size,
                )
            });
            if result == lzokay_sys::EResult::OutputOverrun {
                dst.resize(2 * size)?;
                continue;
            }
            break;
        }
        dst.resize(decompressed_size)?;
        match result {
            lzokay_sys::EResult::Success => {
                Ok(unsafe { py.from_owned_ptr(PyBytes_FromObject(dst.as_ptr())) })
            }
            e => Err(LZOError::new_err(EResult::from(e))),
        }
    }
}

impl Default for LZOCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[pymodule]
fn lzallright(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<LZOCompressor>()?;
    m.add_class::<EResult>()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    pub const LOREM: &[u8] = include_bytes!("../benches/lorem.txt");

    #[test]
    fn test_roundtrip() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let mut comp = LZOCompressor::new();
            let compressed = comp.compress(py, LOREM.into()).unwrap();

            let out = LZOCompressor::decompress(py, compressed.as_bytes().into()).unwrap();

            assert_eq!(out.as_bytes(), LOREM);
        });
    }
}
