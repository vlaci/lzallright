#![allow(ambiguous_associated_items)] // EResult::Error
use pyo3::{
    create_exception,
    ffi::PyBytes_FromObject,
    prelude::*,
    types::{PyByteArray, PyBytes},
};

use crate::lzokay;
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

impl From<&lzokay::Error> for EResult {
    fn from(err: &lzokay::Error) -> Self {
        match err {
            lzokay::Error::LookbehindOverrun => EResult::LookbehindOverrun,
            lzokay::Error::OutputOverrun => EResult::OutputOverrun,
            lzokay::Error::InputOverrun => EResult::InputOverrun,
            lzokay::Error::InputNotConsumed(_) => EResult::InputNotConsumed,
            lzokay::Error::Error => EResult::Error,
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
    dict: lzokay::Dict,
}

#[pymethods]
impl LZOCompressor {
    #[new]
    pub fn new() -> Self {
        Self {
            dict: lzokay::Dict::new(),
        }
    }

    pub fn compress<'a>(&mut self, py: Python<'a>, data: Buffer) -> PyResult<Bound<'a, PyBytes>> {
        let max_size = data.len() + data.len() / 16 + 64 + 3;
        let mut compressed_size = 0usize;
        let dst = PyByteArray::new_with(py, max_size, |dst| {
            compressed_size = py
                .allow_threads(|| lzokay::compress(&data, dst, &mut self.dict))
                .map_err(|e| LZOError::new_err(EResult::from(&e)))?;
            Ok(())
        })?;
        dst.resize(compressed_size)?;
        // SAFETY: dst is a valid PyByteArray; PyBytes_FromObject returns a new reference.
        Ok(unsafe {
            Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr())).downcast_into_unchecked()
        })
    }

    #[staticmethod]
    #[pyo3(signature = (data, output_size_hint = None))]
    pub fn decompress<'a>(
        py: Python<'a>,
        data: Buffer,
        output_size_hint: Option<usize>,
    ) -> PyResult<Bound<'a, PyBytes>> {
        let size = output_size_hint.unwrap_or(2 * data.len());
        let dst = PyByteArray::new_with(py, size, |_| Ok(()))?;
        let result = loop {
            let dst_bytes = unsafe { dst.as_bytes_mut() };
            match py.allow_threads(|| lzokay::decompress(&data, dst_bytes)) {
                Err(lzokay::Error::OutputOverrun) => {
                    dst.resize(2 * dst.len())?;
                    continue;
                }
                result => break result,
            }
        };

        let decompressed_size = match &result {
            Ok(size) => *size,
            Err(lzokay::Error::InputNotConsumed(size)) => *size,
            Err(e) => return Err(LZOError::new_err(EResult::from(e))),
        };
        dst.resize(decompressed_size)?;

        // SAFETY: dst is a valid PyByteArray; PyBytes_FromObject returns a new reference.
        let rv = unsafe {
            Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr())).downcast_into_unchecked()
        };
        match result {
            Ok(_) => Ok(rv),
            Err(lzokay::Error::InputNotConsumed(_)) => {
                Err(InputNotConsumed::new_err::<(_, Py<PyBytes>)>((
                    EResult::InputNotConsumed,
                    rv.into(),
                )))
            }
            Err(_) => unreachable!(),
        }
    }
}

impl Default for LZOCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[pymodule(gil_used = false)]
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
            assert!(err.get_type(py).is(PyType::new::<LZOError>(py)));
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
