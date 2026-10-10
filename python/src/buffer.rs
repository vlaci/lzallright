use pyo3::{
    buffer::PyBuffer,
    exceptions::{PyBufferError, PyMemoryError},
    prelude::*,
    pybacked::PyBackedBytes,
    types::{PyBytes, PyMemoryView},
};

pub enum Buffer {
    Bytes(PyBackedBytes),
    Owned(Vec<u8>),
}

impl Buffer {
    fn new(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        // Exact type: a subclass may export a different buffer via `__buffer__`.
        if let Ok(bytes) = ob.cast_exact::<PyBytes>() {
            return Ok(Self::Bytes(bytes.clone().into()));
        }
        let buf = match PyBuffer::<u8>::get(ob) {
            Ok(buf) => buf,
            // Non-byte item formats (e.g. array('i')): view the raw bytes.
            Err(_) => PyBuffer::<u8>::get(&PyMemoryView::from(ob)?.call_method1("cast", ("B",))?)?,
        };
        if !buf.is_c_contiguous() {
            return Err(PyBufferError::new_err("buffer is not C-contiguous"));
        }
        Ok(Self::Owned(buf.to_vec(ob.py())?))
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Bytes(b) => b,
            Self::Owned(v) => v,
        }
    }
}

impl<'a> From<&'a [u8]> for Buffer {
    fn from(data: &'a [u8]) -> Self {
        Buffer::Owned(data.into())
    }
}

impl<'a, 'py> FromPyObject<'a, 'py> for Buffer {
    type Error = PyErr;

    fn extract(ob: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        Buffer::new(&ob)
    }
}

/// Allocate zero-filled buffer.
pub fn zeroed(len: usize) -> PyResult<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let layout = std::alloc::Layout::array::<u8>(len).map_err(|_| PyMemoryError::new_err(()))?;
    // SAFETY: non-zero size; on success the allocation is `len` initialized
    // (zeroed) bytes from the global allocator, as `Vec` requires.
    unsafe {
        let ptr = std::alloc::alloc_zeroed(layout);
        if ptr.is_null() {
            return Err(PyMemoryError::new_err(()));
        }
        Ok(Vec::from_raw_parts(ptr, len, len))
    }
}
