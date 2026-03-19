#![allow(ambiguous_associated_items)] // EResult::Error
use pyo3::{
    create_exception,
    ffi::PyBytes_FromObject,
    prelude::*,
    types::{PyByteArray, PyBytes},
};

use crate::backend;
use crate::error::{Error, ErrorKind};
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

impl From<&Error> for EResult {
    fn from(err: &Error) -> Self {
        match err.kind() {
            ErrorKind::LookbehindOverrun => EResult::LookbehindOverrun,
            ErrorKind::OutputOverrun => EResult::OutputOverrun,
            ErrorKind::InputOverrun => EResult::InputOverrun,
            ErrorKind::InputNotConsumed => EResult::InputNotConsumed,
            ErrorKind::Error => EResult::Error,
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
    dict: backend::Dict,
}

#[pymethods]
impl LZOCompressor {
    #[new]
    pub fn new() -> Self {
        Self {
            dict: backend::Dict::new(),
        }
    }

    pub fn compress<'a>(
        &mut self,
        py: Python<'a>,
        data: Buffer<'a>,
    ) -> PyResult<Bound<'a, PyBytes>> {
        let src: &[u8] = &data;
        let max_size = src.len() + src.len() / 16 + 64 + 3;
        let mut compressed_size = 0usize;
        let dst = PyByteArray::new_with(py, max_size, |dst| {
            compressed_size = py
                .detach(|| backend::compress(src, dst, &mut self.dict))
                .map_err(|e| LZOError::new_err(EResult::from(&e)))?;
            Ok(())
        })?;
        dst.resize(compressed_size)?;
        // SAFETY: dst is a valid PyByteArray; PyBytes_FromObject returns a new reference.
        Ok(unsafe {
            Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr())).cast_into_unchecked()
        })
    }

    #[staticmethod]
    #[pyo3(signature = (data, output_size_hint = None))]
    pub fn decompress<'a>(
        py: Python<'a>,
        data: Buffer<'a>,
        output_size_hint: Option<usize>,
    ) -> PyResult<Bound<'a, PyBytes>> {
        let src: &[u8] = &data;
        let size = output_size_hint.unwrap_or(2 * src.len());
        let dst = PyByteArray::new_with(py, size, |_| Ok(()))?;
        let result = loop {
            let dst_bytes = unsafe { dst.as_bytes_mut() };
            match py.detach(|| backend::decompress(src, dst_bytes)) {
                Err(e) if *e.kind() == ErrorKind::OutputOverrun => {
                    dst.resize(2 * dst.len())?;
                    continue;
                }
                result => break result,
            }
        };

        let decompressed_size = match &result {
            Ok(size) => *size,
            Err(e) if *e.kind() == ErrorKind::InputNotConsumed => e.dst_size(),
            Err(e) => return Err(LZOError::new_err(EResult::from(e))),
        };
        dst.resize(decompressed_size)?;

        // SAFETY: dst is a valid PyByteArray; PyBytes_FromObject returns a new reference.
        let rv = unsafe {
            Bound::from_owned_ptr(py, PyBytes_FromObject(dst.as_ptr())).cast_into_unchecked()
        };
        match result {
            Ok(_) => Ok(rv),
            Err(e) if *e.kind() == ErrorKind::InputNotConsumed => {
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
        Python::initialize();

        Python::attach(|py| {
            let mut comp = LZOCompressor::new();
            let compressed = comp.compress(py, PyBytes::new(py, LOREM).into()).unwrap();

            let out = LZOCompressor::decompress(py, compressed.into(), Some(LOREM.len())).unwrap();

            assert_eq!(out.as_bytes(), LOREM);
        });
    }

    #[test]
    fn test_decompress_invalid_data() {
        Python::initialize();

        Python::attach(|py| {
            let err =
                LZOCompressor::decompress(py, PyBytes::new(py, LOREM).into(), None).unwrap_err();
            assert!(err.get_type(py).is(PyType::new::<LZOError>(py)));
        });
    }

    #[test]
    fn test_big_compression_ratio() {
        // https://github.com/vlaci/lzallright/issues/12
        Python::initialize();

        Python::attach(|py| {
            let mut comp = LZOCompressor::new();
            let data = [0u8; 65536];
            let compressed = comp.compress(py, PyBytes::new(py, &data).into()).unwrap();

            let out = LZOCompressor::decompress(py, compressed.into(), None).unwrap();

            assert_eq!(out.as_bytes(), data);
        });
    }
}
