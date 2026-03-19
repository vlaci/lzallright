#[derive(Debug, Clone, PartialEq)]
pub enum ErrorKind {
    LookbehindOverrun,
    OutputOverrun,
    InputOverrun,
    Error,
    InputNotConsumed,
}

#[derive(Debug)]
pub struct Error {
    dst_size: usize,
    kind: ErrorKind,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, dst_size: usize) -> Self {
        Self { kind, dst_size }
    }

    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    pub fn dst_size(&self) -> usize {
        self.dst_size
    }
}
