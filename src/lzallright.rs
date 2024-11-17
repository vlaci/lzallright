#![allow(ambiguous_associated_items)] // EResult::Error
use cxx::UniquePtr;
use pyo3::{
    create_exception,
    ffi::PyBytes_FromObject,
    prelude::*,
    types::{PyByteArray, PyBytes},
};

use crate::python::Buffer;

#[pyclass(eq, eq_int, module = "lzallright._lzallright")]
#[derive(Debug, PartialEq, Eq)]
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

create_exception!(
    lzallright._lzallright,
    LZOError,
    pyo3::exceptions::PyException
);
create_exception!(lzallright._lzallright, InputNotConsumed, LZOError);

#[pyclass(unsendable, module = "lzallright._lzallright")]
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

    pub fn compress<'a>(&mut self, py: Python<'a>, data: Buffer) -> PyResult<Bound<'a, PyBytes>> {
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
            lzokay_sys::EResult::Success => Ok(unsafe {
                Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr()))
                    .downcast_into_unchecked()
            }),
            e => Err(LZOError::new_err(EResult::from(e))),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (data, output_size_hint = None))]
    pub fn decompress<'a>(
        py: Python<'a>,
        data: Buffer,
        output_size_hint: Option<usize>,
    ) -> PyResult<Bound<'a, PyBytes>> {
        let size = if let Some(size) = output_size_hint {
            size
        } else {
            2 * data.len()
        };
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
                dst.resize(2 * dst.len())?;
                continue;
            }
            break;
        }
        dst.resize(decompressed_size)?;

        let rv = unsafe {
            Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr())).downcast_into_unchecked()
        };
        match result {
            lzokay_sys::EResult::Success => Ok(rv),
            lzokay_sys::EResult::InputNotConsumed => {
                Err(InputNotConsumed::new_err::<(_, Py<PyBytes>)>((
                    EResult::InputNotConsumed,
                    rv.into(),
                )))
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
fn _lzallright(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<LZOCompressor>()?;
    m.add_class::<EResult>()?;
    m.add("LZOError", py.get_type::<LZOError>())?;
    m.add("InputNotConsumed", py.get_type::<InputNotConsumed>())?;
    Ok(())
}

#[cfg(test)]
mod test {
    use pyo3::types::PyType;

    use super::*;

    pub const LOREM: &[u8] = include_bytes!("../benches/lorem.txt");

    #[test]
    fn test_roundtrip() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let mut comp = LZOCompressor::new();
            let compressed = comp.compress(py, LOREM.into()).unwrap();

            let out =
                LZOCompressor::decompress(py, compressed.as_bytes().into(), Some(LOREM.len()))
                    .unwrap();

            assert_eq!(out.as_bytes(), LOREM);
        });
    }

    #[test]
    fn test_decompress_invalid_data() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let err = LZOCompressor::decompress(py, LOREM.into(), None).unwrap_err();
            assert!(err.get_type(py).is(&PyType::new::<LZOError>(py)));
        });
    }

    #[test]
    fn test_big_compression_ratio() {
        // https://github.com/vlaci/lzallright/issues/12
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let mut comp = LZOCompressor::new();
            let data = [0u8; 65536];
            let compressed = comp.compress(py, data[..].into()).unwrap();

            let out = LZOCompressor::decompress(py, compressed.as_bytes().into(), None).unwrap();

            assert_eq!(out.as_bytes(), data);
        });
    }
}
