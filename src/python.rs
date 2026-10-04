use pyo3::{exceptions::PyTypeError, ffi, prelude::*, types::PyBytes};

use std::ops::Deref;

pub struct Buffer<'py>(Bound<'py, PyBytes>);

impl<'py> Buffer<'py> {
    fn new(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        // PyBytes_FromObject also accepts iterables of ints; only take buffers.
        if unsafe { ffi::PyObject_CheckBuffer(ob.as_ptr()) } == 0 {
            return Err(PyTypeError::new_err(format!(
                "a bytes-like object is required, not {}",
                ob.get_type()
            )));
        }
        // SAFETY: `ob` is a valid object; PyBytes_FromObject returns a new reference
        // to a `bytes` object, or NULL with an exception set.
        unsafe { Bound::from_owned_ptr_or_err(ob.py(), ffi::PyBytes_FromObject(ob.as_ptr())) }
            .map(|b| Buffer(unsafe { b.cast_into_unchecked() }))
    }
}

impl Deref for Buffer<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl<'py> From<Bound<'py, PyBytes>> for Buffer<'py> {
    fn from(data: Bound<'py, PyBytes>) -> Self {
        Buffer(data)
    }
}

impl<'a, 'py> FromPyObject<'a, 'py> for Buffer<'py> {
    type Error = PyErr;

    fn extract(ob: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        Buffer::new(&ob)
    }
}
