#![allow(ambiguous_associated_items)] // EResult::Error
use pyo3::{create_exception, prelude::*, types::PyBytes};

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

impl From<crate::backend::EResult> for EResult {
    fn from(err: crate::backend::EResult) -> Self {
        match err {
            crate::backend::EResult::LookbehindOverrun => EResult::LookbehindOverrun,
            crate::backend::EResult::OutputOverrun => EResult::OutputOverrun,
            crate::backend::EResult::InputOverrun => EResult::InputOverrun,
            crate::backend::EResult::Error => EResult::Error,
            crate::backend::EResult::InputNotConsumed(_) => EResult::InputNotConsumed,
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
    dict: Box<crate::backend::Dict>,
}

#[pymethods]
impl LZOCompressor {
    #[new]
    pub fn new() -> Self {
        Self {
            dict: crate::backend::Dict::new(),
        }
    }

    pub fn compress<'a>(&mut self, py: Python<'a>, data: Buffer) -> PyResult<Bound<'a, PyBytes>> {
        let max_size = data.len() + data.len() / 16 + 64 + 3;

        let mut dst = vec![0; max_size];
        let result = py.allow_threads(|| crate::backend::compress(&data, &mut dst, &mut self.dict));
        match result {
            Ok(written) => {
                let dst = PyBytes::new(py, &dst[0..written]);
                Ok(dst)
            }
            Err(e) => Err(LZOError::new_err(EResult::from(e))),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (data, output_size_hint = None))]
    pub fn decompress<'a>(
        py: Python<'a>,
        data: Buffer,
        output_size_hint: Option<usize>,
    ) -> PyResult<Bound<'a, PyBytes>> {
        let size = output_size_hint.unwrap_or(2 * data.len());
        let mut dst = vec![0; size];
        let result = loop {
            match py.allow_threads(|| crate::backend::decompress(&data, &mut dst)) {
                Err(crate::backend::EResult::OutputOverrun) => {
                    dst.resize(dst.len() * 2, 0);
                    continue;
                }
                result => break result,
            };
        };
        match result {
            Ok(written) => Ok(PyBytes::new(py, &dst[0..written])),
            Err(crate::backend::EResult::InputNotConsumed(written)) => {
                Err(InputNotConsumed::new_err::<(_, Py<PyBytes>)>((
                    EResult::InputNotConsumed,
                    PyBytes::new(py, &dst[..written]).into(),
                )))
            }
            Err(e) => Err(LZOError::new_err(EResult::from(e))),
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
