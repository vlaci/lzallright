/// List specifying categories of compression and decompression
/// related errors.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    /// Match refers to data before the start of output
    LookbehindOverrun,
    /// Output buffer is too small
    OutputOverrun,
    /// Input ended unexpectedly
    InputOverrun,
    /// Malformed data
    Error,
    /// Trailing data after end of compressed stream
    ///
    /// Note: output buffer contains the complete decompressed data in
    /// this case.
    InputNotConsumed,
}

/// The error type of compression and decompression operations.
#[derive(Debug)]
pub struct Error {
    dst_size: usize,
    kind: ErrorKind,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, dst_size: usize) -> Self {
        Self { kind, dst_size }
    }

    /// Returns the corresponding [`ErrorKind`] for this error.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Returns the amount of data produced when the error occurred.
    ///
    /// In case of [`ErrorKind::InputNotConsumed`], the decompressed
    /// data is complete, having this size.
    pub fn dst_size(&self) -> usize {
        self.dst_size
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self.kind {
            ErrorKind::LookbehindOverrun => "match refers to data before the start of the output",
            ErrorKind::OutputOverrun => "output buffer is too small",
            ErrorKind::InputOverrun => "input ended unexpectedly",
            ErrorKind::Error => "malformed compressed data",
            ErrorKind::InputNotConsumed => "trailing data after end of compressed stream",
        };
        write!(f, "{msg} ({} bytes written)", self.dst_size)
    }
}

impl core::error::Error for Error {}
