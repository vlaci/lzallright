mod lzallright;
#[cfg(feature = "lzokay")]
mod lzokay;
mod python;

#[cfg(feature = "lzokay")]
pub(crate) use lzokay as backend;

pub use crate::lzallright::LZOCompressor;
