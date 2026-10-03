use std::sync::{Mutex, PoisonError};

use pyo3::{create_exception, exceptions::PyMemoryError, prelude::*, types::PyBytes};

use crate::backend;
use crate::error::{Error, ErrorKind};
use crate::python::{zeroed, Buffer};

#[pyclass(eq, eq_int, frozen, module = "lzallright._lzallright")]
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

fn lzo_error(e: &Error) -> PyErr {
    LZOError::new_err(EResult::from(e))
}

#[pyclass(frozen, module = "lzallright._lzallright")]
pub struct LZOCompressor {
    dict: Mutex<Box<backend::Dict>>,
}

#[pymethods]
impl LZOCompressor {
    #[new]
    pub fn new() -> Self {
        Self {
            dict: Mutex::new(Box::default()),
        }
    }

    pub fn compress<'py>(&self, py: Python<'py>, data: Buffer) -> PyResult<Bound<'py, PyBytes>> {
        let src = data.as_slice();
        let worst = src
            .len()
            .checked_add(src.len() / 16 + 64 + 3)
            .ok_or_else(|| PyMemoryError::new_err(()))?;
        let (dst, size) = py.detach(|| -> PyResult<_> {
            let mut dst = zeroed(worst)?;
            // Lock without the GIL: a waiting thread never blocks the holder.
            let mut dict = self.dict.lock().unwrap_or_else(PoisonError::into_inner);
            let size = backend::compress(src, &mut dst, &mut dict).map_err(|e| lzo_error(&e))?;
            Ok((dst, size))
        })?;
        Ok(PyBytes::new(py, &dst[..size]))
    }

    #[staticmethod]
    #[pyo3(signature = (data, output_size_hint = None))]
    pub fn decompress<'py>(
        py: Python<'py>,
        data: Buffer,
        output_size_hint: Option<usize>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let src = data.as_slice();
        let mut size = match output_size_hint {
            Some(size) => size,
            None => src.len().saturating_mul(2),
        };
        loop {
            if size > isize::MAX as usize {
                return Err(PyMemoryError::new_err(()));
            }
            // Decompress straight into the result: an exact size hint needs no copy.
            let mut result = Ok(0);
            let out = PyBytes::new_with(py, size, |buf| {
                result = py.detach(|| backend::decompress(src, buf));
                Ok(())
            })?;
            return match result {
                Ok(n) if n == size => Ok(out),
                Ok(n) => Ok(PyBytes::new(py, &out.as_bytes()[..n])),
                Err(e) if *e.kind() == ErrorKind::OutputOverrun => {
                    size = size.saturating_mul(2).max(1);
                    continue;
                }
                Err(e) if *e.kind() == ErrorKind::InputNotConsumed => {
                    Err(InputNotConsumed::new_err((
                        EResult::InputNotConsumed,
                        PyBytes::new(py, &out.as_bytes()[..e.dst_size()]).unbind(),
                    )))
                }
                Err(e) => Err(lzo_error(&e)),
            };
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
            let comp = LZOCompressor::new();
            let compressed = comp.compress(py, LOREM.into()).unwrap();

            let out =
                LZOCompressor::decompress(py, compressed[..].into(), Some(LOREM.len())).unwrap();

            assert_eq!(out.as_bytes(), LOREM);
        });
    }

    #[test]
    fn test_decompress_invalid_data() {
        Python::initialize();

        Python::attach(|py| {
            let err = LZOCompressor::decompress(py, LOREM.into(), None).unwrap_err();
            assert!(err.get_type(py).is(PyType::new::<LZOError>(py)));
        });
    }

    #[test]
    fn test_big_compression_ratio() {
        // https://github.com/vlaci/lzallright/issues/12
        Python::initialize();

        Python::attach(|py| {
            let comp = LZOCompressor::new();
            let data = [0u8; 65536];
            let compressed = comp.compress(py, data[..].into()).unwrap();

            let out = LZOCompressor::decompress(py, compressed[..].into(), None).unwrap();

            assert_eq!(out.as_bytes(), data);
        });
    }
}
